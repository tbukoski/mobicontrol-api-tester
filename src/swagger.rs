// Minimal Swagger 2.0 model — only the parts we need to drive the UI.
//
// We avoid modeling the full spec because we're only using it to populate
// dropdowns and parameter lists; full schema resolution is deliberately
// left to the user (they enter body JSON manually).

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Swagger {
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default, rename = "basePath")]
    pub base_path: Option<String>,
    #[serde(default)]
    pub schemes: Vec<String>,
    #[serde(default)]
    pub paths: BTreeMap<String, PathItem>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct PathItem {
    #[serde(default)]
    pub get: Option<Operation>,
    #[serde(default)]
    pub post: Option<Operation>,
    #[serde(default)]
    pub put: Option<Operation>,
    #[serde(default)]
    pub delete: Option<Operation>,
}

impl PathItem {
    pub fn operation(&self, method: HttpMethod) -> Option<&Operation> {
        match method {
            HttpMethod::Get => self.get.as_ref(),
            HttpMethod::Post => self.post.as_ref(),
            HttpMethod::Put => self.put.as_ref(),
            HttpMethod::Delete => self.delete.as_ref(),
        }
    }

    pub fn supports(&self, method: HttpMethod) -> bool {
        self.operation(method).is_some()
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Operation {
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "operationId")]
    pub operation_id: Option<String>,
    #[serde(default)]
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Parameter {
    pub name: String,
    #[serde(rename = "in")]
    pub location: String, // "query" | "path" | "body" | "header" | "formData"
    #[serde(default)]
    pub required: bool,
    #[serde(default, rename = "type")]
    pub param_type: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "enum")]
    pub enum_values: Option<Vec<String>>,
    /// Body parameters reference a definition via $ref; captured as opaque JSON.
    #[serde(default)]
    pub schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
        }
    }

    pub fn all() -> &'static [HttpMethod] {
        &[
            HttpMethod::Get,
            HttpMethod::Post,
            HttpMethod::Put,
            HttpMethod::Delete,
        ]
    }
}

pub fn parse(text: &str) -> Result<Swagger> {
    serde_json::from_str(text).context("Failed to parse swagger JSON")
}

pub fn load_from_file(path: &Path) -> Result<Swagger> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read swagger file: {}", path.display()))?;
    parse(&text)
}
