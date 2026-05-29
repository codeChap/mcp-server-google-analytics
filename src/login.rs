use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{info, warn};
use url::Url;

use crate::config::{self, AccountConfig};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "https://www.googleapis.com/auth/analytics.readonly";
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Deserialize)]
struct TokenResponse {
    refresh_token: Option<String>,
}

/// Run the interactive OAuth flow for a configured account.
///
/// Binds a loopback listener, opens the consent URL in the user's browser,
/// waits for the redirect with `?code=`, exchanges it for a refresh token,
/// and writes the credentials JSON to the account's credentials path.
pub async fn perform_login(name: &str, http: &Client) -> Result<String> {
    let cfg = config::load_config()?
        .context("no ~/.config/mcp-server-google-analytics/config.toml found")?;
    let account = cfg
        .accounts
        .iter()
        .find(|a| a.name == name)
        .with_context(|| format!("account '{name}' not found in config.toml"))?;

    let creds_path = config::resolve_credentials_path(&account.credentials);
    let (client_id, client_secret) = resolve_oauth_client(account, &creds_path)?;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind loopback listener for OAuth callback")?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}");

    let state = random_state();
    let auth_url = build_auth_url(&client_id, &redirect_uri, &state)?;

    info!("OAuth URL (click if browser did not open): {auth_url}");
    if let Err(e) = open::that(&auth_url) {
        warn!("could not open browser automatically: {e}");
    }

    let (code, returned_state) = tokio::time::timeout(CALLBACK_TIMEOUT, wait_for_code(listener))
        .await
        .context("login timed out after 5 minutes")??;

    if returned_state != state {
        bail!("OAuth state mismatch — request may have been tampered with");
    }

    let refresh_token = exchange_code(http, &client_id, &client_secret, &redirect_uri, &code)
        .await
        .context("failed to exchange authorization code for refresh token")?;

    write_credentials(&creds_path, &client_id, &client_secret, &refresh_token)?;

    Ok(format!(
        "Login successful for account '{name}'. Credentials written to {}. \
         Restart the MCP server for the new token to take effect.",
        creds_path.display()
    ))
}

fn resolve_oauth_client(account: &AccountConfig, creds_path: &Path) -> Result<(String, String)> {
    // 1. Prefer values from config.toml.
    if let (Some(id), Some(secret)) = (&account.client_id, &account.client_secret) {
        return Ok((id.clone(), secret.clone()));
    }

    // 2. Fall back to the existing credentials file (e.g. a prior gcloud ADC).
    if creds_path.exists() {
        if let Ok(content) = std::fs::read_to_string(creds_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                if let (Some(id), Some(secret)) = (
                    v.get("client_id").and_then(|x| x.as_str()),
                    v.get("client_secret").and_then(|x| x.as_str()),
                ) {
                    return Ok((id.to_string(), secret.to_string()));
                }
            }
        }
    }

    bail!(
        "no OAuth client_id/client_secret available for account '{}'. \
         Add `client_id` and `client_secret` to the [[accounts]] entry in config.toml, \
         or ensure {} contains an `authorized_user` credential with those fields. \
         Create a Desktop-app OAuth client at https://console.cloud.google.com/apis/credentials.",
        account.name,
        creds_path.display()
    )
}

fn build_auth_url(client_id: &str, redirect_uri: &str, state: &str) -> Result<String> {
    let mut url = Url::parse(AUTH_URL)?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", SCOPE)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("state", state);
    Ok(url.to_string())
}

async fn wait_for_code(listener: TcpListener) -> Result<(String, String)> {
    let (mut stream, _) = listener.accept().await?;

    let mut buf = vec![0u8; 8192];
    let n = stream.read(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]);

    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .context("malformed HTTP request on callback")?
        .to_string();

    let url = Url::parse(&format!("http://127.0.0.1{path}"))?;
    let params: HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    if let Some(err) = params.get("error") {
        let html = format!(
            "<html><body><h1>Login failed</h1><p>{err}</p></body></html>"
        );
        let _ = write_http_response(&mut stream, 400, "Bad Request", &html).await;
        bail!("OAuth consent denied: {err}");
    }

    let code = params
        .get("code")
        .cloned()
        .context("OAuth callback missing `code` parameter")?;
    let returned_state = params.get("state").cloned().unwrap_or_default();

    let html = "<html><body style='font-family:sans-serif;text-align:center;margin-top:4em'>\
                <h1>Login successful</h1>\
                <p>You can close this window and return to your MCP client.</p>\
                </body></html>";
    write_http_response(&mut stream, 200, "OK", html).await?;

    Ok((code, returned_state))
}

async fn write_http_response(
    stream: &mut tokio::net::TcpStream,
    status: u16,
    reason: &str,
    body: &str,
) -> Result<()> {
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

async fn exchange_code(
    http: &Client,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    code: &str,
) -> Result<String> {
    let resp = http
        .post(TOKEN_URL)
        .form(&[
            ("code", code),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("token endpoint returned error: {body}");
    }

    let tokens: TokenResponse = resp.json().await?;
    tokens.refresh_token.context(
        "Google did not return a refresh_token. If you have already granted consent, \
         revoke access at https://myaccount.google.com/permissions and retry.",
    )
}

fn write_credentials(
    path: &Path,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("failed to create directory {}", parent.display())
        })?;
    }

    let payload = serde_json::json!({
        "type": "authorized_user",
        "client_id": client_id,
        "client_secret": client_secret,
        "refresh_token": refresh_token,
    });
    let body = serde_json::to_string_pretty(&payload)?;
    std::fs::write(path, body)
        .with_context(|| format!("failed to write credentials to {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

fn random_state() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}-{:x}", std::process::id())
}
