// Swagger fetching and API call invocation.

use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;

use crate::swagger::HttpMethod;

/// Fetches the raw swagger.json text from a MobiControl server.
pub fn fetch_swagger(client: &reqwest::blocking::Client, fqdn: &str) -> Result<String> {
    let url = format!("https://{fqdn}/MobiControl/api/swagger/v2/swagger.json");
    let resp = client
        .get(&url)
        .send()
        .with_context(|| format!("Failed to GET {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("Swagger fetch failed (HTTP {status})"));
    }
    resp.text().context("Failed to read swagger response body")
}

pub struct ApiRequest {
    pub fqdn: String,
    pub token: String,
    pub method: HttpMethod,
    /// Path template like "/devices/{deviceId}". Path parameters will be
    /// substituted in.
    pub path_template: String,
    pub path_params: HashMap<String, String>,
    pub query_params: HashMap<String, String>,
    /// Raw JSON body, if applicable.
    pub body: Option<String>,
}

#[derive(Debug)]
pub struct ApiResponse {
    pub status: u16,
    pub url: String,
    pub body: String,
}

pub fn invoke(client: &reqwest::blocking::Client, req: ApiRequest) -> Result<ApiResponse> {
    // Substitute path parameters
    let mut path = req.path_template.clone();
    for (k, v) in &req.path_params {
        let placeholder = format!("{{{k}}}");
        path = path.replace(&placeholder, &percent_encode_segment(v));
    }

    let url = format!("https://{}/MobiControl/api{}", req.fqdn, path);

    let mut builder = match req.method {
        HttpMethod::Get => client.get(&url),
        HttpMethod::Post => client.post(&url),
        HttpMethod::Put => client.put(&url),
        HttpMethod::Delete => client.delete(&url),
    };

    builder = builder.header("Authorization", format!("Bearer {}", req.token));

    if !req.query_params.is_empty() {
        let qp: Vec<(&str, &str)> = req
            .query_params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        builder = builder.query(&qp);
    }

    if let Some(body) = &req.body {
        builder = builder
            .header("Content-Type", "application/json")
            .body(body.clone());
    }

    let resp = builder
        .send()
        .with_context(|| format!("Failed to send {} {}", req.method.as_str(), url))?;

    let status = resp.status().as_u16();
    let final_url = resp.url().to_string();
    let body = resp.text().context("Failed to read response body")?;

    Ok(ApiResponse {
        status,
        url: final_url,
        body,
    })
}

/// Minimal RFC3986-style percent-encoding for path segments.
fn percent_encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
