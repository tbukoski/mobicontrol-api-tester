// OAuth2 Resource Owner grant token retrieval for MobiControl.
//
// POST https://{fqdn}/MobiControl/api/token
//   Authorization: Basic base64(client_id:client_secret)
//   Content-Type: application/x-www-form-urlencoded
//   grant_type=password&username=...&password=...

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub token_type: String,
    #[serde(default)]
    pub expires_in: u64,
}

pub fn get_token(
    client: &reqwest::blocking::Client,
    fqdn: &str,
    client_id: &str,
    client_secret: &str,
    username: &str,
    password: &str,
) -> Result<TokenResponse> {
    let url = format!("https://{fqdn}/MobiControl/api/token");
    let auth_b64 = base64::engine::general_purpose::STANDARD
        .encode(format!("{client_id}:{client_secret}"));

    let resp = client
        .post(&url)
        .header("Authorization", format!("Basic {auth_b64}"))
        .form(&[
            ("grant_type", "password"),
            ("username", username),
            ("password", password),
        ])
        .send()
        .with_context(|| format!("Failed to send token request to {url}"))?;

    let status = resp.status();
    let body = resp
        .text()
        .context("Failed to read token response body")?;

    if !status.is_success() {
        return Err(anyhow!(
            "Token request failed (HTTP {status}): {body}"
        ));
    }

    serde_json::from_str(&body)
        .with_context(|| format!("Failed to parse token response: {body}"))
}
