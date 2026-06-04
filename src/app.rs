// Main application state and UI for MobiControl API Tester.
//
// All network and CPU work (token fetch, swagger parse, API call) runs on a
// background std::thread. Results are delivered back to the UI thread over an
// mpsc channel and polled each frame.

use anyhow::{anyhow, Result};
use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use crate::api::{self, ApiRequest, ApiResponse};
use crate::auth;
use crate::credentials::{self, Credentials};
use crate::paths;
use crate::swagger::{self, HttpMethod, Swagger};

pub struct App {
    // Credentials form
    creds: Credentials,

    // Loaded swagger
    swagger: Option<Swagger>,
    swagger_status: String,

    // Endpoint selection
    selected_method: HttpMethod,
    selected_path: Option<String>,
    path_filter: String,

    // Parameter inputs (keyed by param name) - covers path/query/header/formData
    param_values: HashMap<String, String>,
    // JSON body editor (used when an op has a body parameter)
    body_text: String,

    // Output
    output_path: String,

    // Status / last response
    status_message: String,
    last_response: Option<ApiResponse>,

    // Background task
    task_rx: Option<Receiver<TaskResult>>,
    task_label: String,

    // Reusable HTTP client
    client: reqwest::blocking::Client,
}

enum TaskResult {
    SwaggerLoaded {
        source: String,
        swagger: Result<Swagger>,
    },
    ApiCompleted {
        result: Result<ApiResponse>,
        wrote_to: Option<PathBuf>,
    },
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            creds: Credentials::default(),
            swagger: None,
            swagger_status: "No swagger loaded.".to_string(),
            selected_method: HttpMethod::Get,
            selected_path: None,
            path_filter: String::new(),
            param_values: HashMap::new(),
            body_text: String::new(),
            output_path: paths::default_output_path()
                .to_string_lossy()
                .into_owned(),
            status_message: String::new(),
            last_response: None,
            task_rx: None,
            task_label: String::new(),
            client,
        }
    }

    fn is_busy(&self) -> bool {
        self.task_rx.is_some()
    }

    /// Spawns a background worker. Result is delivered via mpsc and the UI
    /// is repainted on completion.
    fn spawn<F>(&mut self, ctx: &egui::Context, label: &str, work: F)
    where
        F: FnOnce() -> TaskResult + Send + 'static,
    {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            let result = work();
            let _ = tx.send(result);
            ctx_clone.request_repaint();
        });
        self.task_rx = Some(rx);
        self.task_label = label.to_string();
    }

    fn poll_tasks(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.task_rx else { return };
        match rx.try_recv() {
            Ok(result) => {
                self.handle_task_result(result);
                self.task_rx = None;
                self.task_label.clear();
            }
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            Err(TryRecvError::Disconnected) => {
                self.task_rx = None;
                self.task_label.clear();
                self.status_message = "Background task terminated unexpectedly.".to_string();
            }
        }
    }

    fn handle_task_result(&mut self, result: TaskResult) {
        match result {
            TaskResult::SwaggerLoaded { source, swagger } => match swagger {
                Ok(sw) => {
                    let count = sw.paths.len();
                    self.swagger = Some(sw);
                    self.swagger_status = format!("Loaded {count} paths from {source}.");
                    self.status_message.clear();
                    // Reset selection - old selection may not exist in new swagger
                    self.selected_path = None;
                    self.param_values.clear();
                    self.body_text.clear();
                }
                Err(e) => {
                    self.swagger_status = format!("Failed to load swagger: {e}");
                }
            },
            TaskResult::ApiCompleted { result, wrote_to } => match result {
                Ok(resp) => {
                    let written = wrote_to
                        .as_ref()
                        .map(|p| format!(" - response written to {}", p.display()))
                        .unwrap_or_default();
                    self.status_message =
                        format!("HTTP {} ({}){}", resp.status, resp.url, written);
                    self.last_response = Some(resp);
                }
                Err(e) => {
                    self.status_message = format!("Error: {e}");
                }
            },
        }
    }

    // ---------- Action handlers ----------

    fn action_save_credentials(&mut self) {
        let default = paths::default_credentials_path();
        let dlg = rfd::FileDialog::new()
            .add_filter("Encrypted credentials", &["enc"])
            .set_directory(default.parent().unwrap_or(std::path::Path::new(".")))
            .set_file_name(
                default
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            );
        if let Some(path) = dlg.save_file() {
            match credentials::save(&self.creds, &path) {
                Ok(()) => {
                    self.status_message =
                        format!("Credentials saved to {}", path.display());
                }
                Err(e) => {
                    self.status_message = format!("Save failed: {e}");
                }
            }
        }
    }

    fn action_load_credentials(&mut self) {
        let default = paths::default_credentials_path();
        let dlg = rfd::FileDialog::new()
            .add_filter("Encrypted credentials", &["enc"])
            .set_directory(default.parent().unwrap_or(std::path::Path::new(".")));
        if let Some(path) = dlg.pick_file() {
            match credentials::load(&path) {
                Ok(creds) => {
                    self.creds = creds;
                    self.status_message =
                        format!("Credentials loaded from {}", path.display());
                }
                Err(e) => {
                    self.status_message = format!("Load failed: {e}");
                }
            }
        }
    }

    fn action_browse_output(&mut self) {
        let cur = PathBuf::from(&self.output_path);
        let dlg = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .add_filter("All files", &["*"])
            .set_directory(
                cur.parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(paths::default_dir),
            )
            .set_file_name(
                cur.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "mobicontrol_api_output.json".to_string()),
            );
        if let Some(path) = dlg.save_file() {
            self.output_path = path.to_string_lossy().into_owned();
        }
    }

    fn action_fetch_swagger(&mut self, ctx: &egui::Context) {
        let fqdn = self.creds.fqdn.trim().to_string();
        let client = self.client.clone();
        let sample_path = paths::sample_swagger_path();

        self.spawn(ctx, "Fetching swagger...", move || {
            // 1) Try server fetch if FQDN provided.
            if !fqdn.is_empty() {
                match api::fetch_swagger(&client, &fqdn) {
                    Ok(text) => {
                        let source = format!("server ({fqdn})");
                        return TaskResult::SwaggerLoaded {
                            source,
                            swagger: swagger::parse(&text),
                        };
                    }
                    Err(server_err) => {
                        // Fall through to sample fallback
                        match swagger::load_from_file(&sample_path) {
                            Ok(sw) => {
                                return TaskResult::SwaggerLoaded {
                                    source: format!(
                                        "sample fallback (server fetch failed: {server_err})"
                                    ),
                                    swagger: Ok(sw),
                                };
                            }
                            Err(sample_err) => {
                                return TaskResult::SwaggerLoaded {
                                    source: "none".to_string(),
                                    swagger: Err(anyhow!(
                                        "Server fetch failed: {server_err}. \
                                         Sample fallback also failed: {sample_err}"
                                    )),
                                };
                            }
                        }
                    }
                }
            }

            // 2) No FQDN - go straight to sample.
            match swagger::load_from_file(&sample_path) {
                Ok(sw) => TaskResult::SwaggerLoaded {
                    source: format!("sample ({})", sample_path.display()),
                    swagger: Ok(sw),
                },
                Err(e) => TaskResult::SwaggerLoaded {
                    source: "none".to_string(),
                    swagger: Err(anyhow!(
                        "No FQDN provided and sample swagger could not be read: {e}"
                    )),
                },
            }
        });
    }

    fn action_run(&mut self, ctx: &egui::Context) {
        let Some(path_template) = self.selected_path.clone() else {
            self.status_message = "Select an API path first.".to_string();
            return;
        };
        let Some(swagger) = &self.swagger else {
            self.status_message = "Load a swagger document first.".to_string();
            return;
        };
        let Some(path_item) = swagger.paths.get(&path_template) else {
            self.status_message = "Selected path not found in swagger.".to_string();
            return;
        };
        let Some(op) = path_item.operation(self.selected_method) else {
            self.status_message = format!(
                "Method {} not supported on {}.",
                self.selected_method.as_str(),
                path_template
            );
            return;
        };

        // Split parameters by location.
        let mut path_params = HashMap::new();
        let mut query_params = HashMap::new();
        let mut body: Option<String> = None;

        for p in &op.parameters {
            match p.location.as_str() {
                "path" => {
                    let v = self.param_values.get(&p.name).cloned().unwrap_or_default();
                    if v.is_empty() && p.required {
                        self.status_message =
                            format!("Required path parameter '{}' is empty.", p.name);
                        return;
                    }
                    path_params.insert(p.name.clone(), v);
                }
                "query" => {
                    if let Some(v) = self.param_values.get(&p.name) {
                        if !v.is_empty() {
                            query_params.insert(p.name.clone(), v.clone());
                        } else if p.required {
                            self.status_message =
                                format!("Required query parameter '{}' is empty.", p.name);
                            return;
                        }
                    } else if p.required {
                        self.status_message =
                            format!("Required query parameter '{}' is empty.", p.name);
                        return;
                    }
                }
                "body" => {
                    let trimmed = self.body_text.trim();
                    if !trimmed.is_empty() {
                        body = Some(trimmed.to_string());
                    } else if p.required {
                        self.status_message =
                            format!("Body parameter '{}' is required.", p.name);
                        return;
                    }
                }
                "header" | "formData" => {
                    // Not commonly used by MobiControl operations - skip with note
                    if p.required {
                        self.status_message = format!(
                            "Required {} parameter '{}' is not supported by this UI.",
                            p.location, p.name
                        );
                        return;
                    }
                }
                _ => {}
            }
        }

        // Collect everything the worker needs.
        let client = self.client.clone();
        let creds = self.creds.clone();
        let method = self.selected_method;
        let output_path = self.output_path.trim().to_string();

        self.spawn(ctx, "Running API call...", move || {
            // 1) Get a token
            let token = match auth::get_token(
                &client,
                &creds.fqdn,
                &creds.client_id,
                &creds.client_secret,
                &creds.username,
                &creds.password,
            ) {
                Ok(t) => t.access_token,
                Err(e) => {
                    return TaskResult::ApiCompleted {
                        result: Err(e),
                        wrote_to: None,
                    };
                }
            };

            // 2) Invoke the API
            let req = ApiRequest {
                fqdn: creds.fqdn.clone(),
                token,
                method,
                path_template,
                path_params,
                query_params,
                body,
            };
            let resp = match api::invoke(&client, req) {
                Ok(r) => r,
                Err(e) => {
                    return TaskResult::ApiCompleted {
                        result: Err(e),
                        wrote_to: None,
                    };
                }
            };

            // 3) Write to the output file if configured
            let wrote_to = if !output_path.is_empty() {
                let p = PathBuf::from(&output_path);
                if let Err(e) = std::fs::write(&p, &resp.body) {
                    return TaskResult::ApiCompleted {
                        result: Err(anyhow!(
                            "API call succeeded (HTTP {}) but failed to write output file '{}': {e}",
                            resp.status,
                            p.display()
                        )),
                        wrote_to: None,
                    };
                }
                Some(p)
            } else {
                None
            };

            TaskResult::ApiCompleted {
                result: Ok(resp),
                wrote_to,
            }
        });
    }

    // ---------- UI rendering ----------

    fn render_credentials(&mut self, ui: &mut egui::Ui) {
        ui.heading("Credentials");
        egui::Grid::new("cred_grid")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label("Client ID:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.creds.client_id)
                        .desired_width(f32::INFINITY),
                );
                ui.end_row();

                ui.label("Client Secret:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.creds.client_secret)
                        .password(true)
                        .desired_width(f32::INFINITY),
                );
                ui.end_row();

                ui.label("Username:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.creds.username)
                        .desired_width(f32::INFINITY),
                );
                ui.end_row();

                ui.label("Password:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.creds.password)
                        .password(true)
                        .desired_width(f32::INFINITY),
                );
                ui.end_row();

                ui.label("FQDN:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.creds.fqdn)
                        .hint_text("e.g. a000666.mobicontrol.cloud")
                        .desired_width(f32::INFINITY),
                );
                ui.end_row();
            });

        ui.horizontal(|ui| {
            let busy = self.is_busy();
            if ui
                .add_enabled(!busy, egui::Button::new("Save Credentials..."))
                .clicked()
            {
                self.action_save_credentials();
            }
            if ui
                .add_enabled(!busy, egui::Button::new("Load Credentials..."))
                .clicked()
            {
                self.action_load_credentials();
            }
            if ui
                .add_enabled(!busy, egui::Button::new("Fetch Swagger"))
                .on_hover_text(
                    "Fetches swagger.json from https://<FQDN>/MobiControl/api/swagger/v2/swagger.json.\n\
                     Falls back to sample_swagger.json next to the executable if unreachable.",
                )
                .clicked()
            {
                self.action_fetch_swagger(ui.ctx());
            }
        });
    }

    fn render_swagger_status(&self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Swagger:").strong());
            ui.label(&self.swagger_status);
        });
    }

    fn render_endpoint_selection(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_source("method_combo")
                .selected_text(self.selected_method.as_str())
                .show_ui(ui, |ui| {
                    for m in HttpMethod::all() {
                        let response = ui.selectable_value(
                            &mut self.selected_method,
                            *m,
                            m.as_str(),
                        );
                        if response.clicked() {
                            // Clear selection that may not support new method
                            self.param_values.clear();
                            self.body_text.clear();
                            if let Some(p) = &self.selected_path {
                                if let Some(sw) = &self.swagger {
                                    if let Some(item) = sw.paths.get(p) {
                                        if !item.supports(self.selected_method) {
                                            self.selected_path = None;
                                        }
                                    }
                                }
                            }
                        }
                    }
                });
            ui.label("Filter:");
            ui.add(
                egui::TextEdit::singleline(&mut self.path_filter)
                    .hint_text("substring of path...")
                    .desired_width(300.0),
            );
            if !self.path_filter.is_empty() && ui.button("✕").on_hover_text("Clear filter").clicked() {
                self.path_filter.clear();
            }
        });

        // Path list (scrollable, filtered)
        ui.label("Path:");
        let frame = egui::Frame::group(ui.style());
        frame.show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(sw) = &self.swagger {
                        let filter = self.path_filter.to_lowercase();
                        let method = self.selected_method;
                        // Collect into an owned Vec<String> so we don't keep an immutable
                        // borrow on self.swagger while we mutate other fields of self below.
                        let entries: Vec<String> = sw
                            .paths
                            .iter()
                            .filter(|(_, item)| item.supports(method))
                            .filter(|(p, _)| {
                                filter.is_empty() || p.to_lowercase().contains(&filter)
                            })
                            .map(|(p, _)| p.clone())
                            .collect();

                        if entries.is_empty() {
                            ui.weak("(no paths match)");
                        } else {
                            for p in &entries {
                                let selected = self.selected_path.as_deref() == Some(p.as_str());
                                if ui.selectable_label(selected, p).clicked()
                                    && self.selected_path.as_deref() != Some(p.as_str())
                                {
                                    self.selected_path = Some(p.clone());
                                    self.param_values.clear();
                                    self.body_text.clear();
                                }
                            }
                        }
                    } else {
                        ui.weak("(load a swagger document)");
                    }
                });
        });
    }

    fn render_parameters(&mut self, ui: &mut egui::Ui) {
        let Some(swagger) = &self.swagger else {
            return;
        };
        let Some(selected) = self.selected_path.clone() else {
            ui.weak("(select a path)");
            return;
        };
        let Some(item) = swagger.paths.get(&selected) else {
            return;
        };
        let Some(op) = item.operation(self.selected_method) else {
            return;
        };

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{} {}",
                    self.selected_method.as_str(),
                    selected
                ))
                .strong()
                .monospace(),
            );
        });
        if let Some(summary) = &op.summary {
            ui.label(summary);
        }
        if let Some(desc) = &op.description {
            if !desc.is_empty() && Some(desc) != op.summary.as_ref() {
                ui.weak(desc);
            }
        }
        ui.add_space(4.0);

        // Clone parameters so we can borrow self.param_values mutably while iterating.
        let parameters = op.parameters.clone();

        let non_body: Vec<_> = parameters
            .iter()
            .filter(|p| p.location != "body")
            .collect();
        let body_param = parameters.iter().find(|p| p.location == "body");

        if non_body.is_empty() && body_param.is_none() {
            ui.weak("(no parameters)");
        } else {
            egui::Grid::new("param_grid")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    for p in &non_body {
                        let req_marker = if p.required { " *" } else { "" };
                        let type_label = p.param_type.as_deref().unwrap_or("?");
                        let label_text =
                            format!("{} ({}, {}{})", p.name, p.location, type_label, req_marker);
                        let label = ui.label(label_text);
                        if let Some(d) = &p.description {
                            if !d.is_empty() {
                                label.on_hover_text(d);
                            }
                        }

                        let entry = self.param_values.entry(p.name.clone()).or_default();
                        if let Some(values) = &p.enum_values {
                            egui::ComboBox::from_id_source(("enum", &p.name))
                                .selected_text(if entry.is_empty() {
                                    "(select)".to_string()
                                } else {
                                    entry.clone()
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(entry, String::new(), "(none)");
                                    for v in values {
                                        ui.selectable_value(entry, v.clone(), v);
                                    }
                                });
                        } else if p.param_type.as_deref() == Some("boolean") {
                            egui::ComboBox::from_id_source(("bool", &p.name))
                                .selected_text(if entry.is_empty() {
                                    "(unset)".to_string()
                                } else {
                                    entry.clone()
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(entry, String::new(), "(unset)");
                                    ui.selectable_value(entry, "true".to_string(), "true");
                                    ui.selectable_value(entry, "false".to_string(), "false");
                                });
                        } else {
                            ui.add(
                                egui::TextEdit::singleline(entry).desired_width(f32::INFINITY),
                            );
                        }
                        ui.end_row();
                    }
                });
        }

        if let Some(p) = body_param {
            ui.add_space(6.0);
            let req_marker = if p.required { " *" } else { "" };
            ui.label(
                egui::RichText::new(format!("Body (JSON){req_marker}"))
                    .strong(),
            );
            if let Some(d) = &p.description {
                if !d.is_empty() {
                    ui.weak(d);
                }
            }
            if let Some(schema) = &p.schema {
                if let Some(reference) = schema.get("$ref").and_then(|v| v.as_str()) {
                    ui.weak(format!("Schema: {reference}"));
                }
            }
            ui.add(
                egui::TextEdit::multiline(&mut self.body_text)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(6)
                    .hint_text("Paste request body JSON here..."),
            );
        }
    }

    fn render_output_and_run(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Output file:");
            ui.add(
                egui::TextEdit::singleline(&mut self.output_path)
                    .desired_width(ui.available_width() - 110.0),
            );
            if ui.button("Browse...").clicked() {
                self.action_browse_output();
            }
        });

        let can_run = !self.is_busy()
            && self.selected_path.is_some()
            && !self.creds.client_id.is_empty()
            && !self.creds.client_secret.is_empty()
            && !self.creds.username.is_empty()
            && !self.creds.password.is_empty()
            && !self.creds.fqdn.is_empty();

        ui.horizontal(|ui| {
            if ui
                .add_enabled(can_run, egui::Button::new(egui::RichText::new("Run").strong()))
                .clicked()
            {
                self.action_run(ui.ctx());
            }
            if self.is_busy() {
                ui.spinner();
                ui.label(&self.task_label);
            }
        });
    }

    fn render_status(&self, ui: &mut egui::Ui) {
        if !self.status_message.is_empty() {
            ui.separator();
            ui.label(egui::RichText::new(&self.status_message).strong());
        }
        if let Some(resp) = &self.last_response {
            ui.collapsing("Response preview", |ui| {
                egui::ScrollArea::vertical()
                    .max_height(250.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Show body verbatim; pretty-print if it parses as JSON.
                        let pretty = match serde_json::from_str::<serde_json::Value>(&resp.body) {
                            Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|_| resp.body.clone()),
                            Err(_) => resp.body.clone(),
                        };
                        // TextEdit::multiline needs a &mut to something implementing TextBuffer.
                        // &mut &str works (read-only buffer); &mut on a temporary does not.
                        let mut view: &str = pretty.as_str();
                        ui.add(
                            egui::TextEdit::multiline(&mut view)
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .desired_rows(12),
                        );
                    });
            });
        }
    }

}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_tasks(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.render_credentials(ui);
                    ui.separator();
                    self.render_swagger_status(ui);
                    ui.separator();
                    self.render_endpoint_selection(ui);
                    ui.separator();
                    self.render_parameters(ui);
                    ui.separator();
                    self.render_output_and_run(ui);
                    self.render_status(ui);
                });
        });
    }
}
