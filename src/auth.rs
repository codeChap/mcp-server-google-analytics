use anyhow::{Context, Result, bail};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tracing::{debug, info};

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const ANALYTICS_SCOPE: &str = "https://www.googleapis.com/auth/analytics.readonly";

/// Google credential types supported by Application Default Credentials.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum Credentials {
    #[serde(rename = "authorized_user")]
    AuthorizedUser {
        client_id: String,
        client_secret: String,
        refresh_token: String,
    },
    #[serde(rename = "service_account")]
    ServiceAccount {
        client_email: String,
        private_key: String,
        token_uri: Option<String>,
    },
}

/// JWT claims for service account token exchange.
#[derive(Serialize)]
struct JwtClaims {
    iss: String,
    scope: String,
    aud: String,
    iat: u64,
    exp: u64,
}

/// Token response from Google's OAuth2 token endpoint.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

/// Cached access token with expiry.
#[derive(Debug)]
struct CachedToken {
    access_token: String,
    expires_at: u64,
}

/// Handles Google Application Default Credentials (ADC) authentication.
pub struct GoogleAuth {
    credentials: Credentials,
    http: Client,
    cached_token: Arc<Mutex<Option<CachedToken>>>,
    /// Quota project ID from ADC or GOOGLE_PROJECT_ID env var.
    quota_project: Option<String>,
}

impl GoogleAuth {
    /// Create a new GoogleAuth by discovering credentials via ADC.
    /// Accepts a shared `reqwest::Client` to avoid duplicate connection pools.
    pub fn new(http: Client) -> Result<Self> {
        let creds_path = discover_credentials_path()?;
        info!("loading credentials from {}", creds_path.display());

        let content = std::fs::read_to_string(&creds_path)
            .with_context(|| format!("failed to read credentials: {}", creds_path.display()))?;

        let credentials: Credentials = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse credentials: {}", creds_path.display()))?;

        // Extract quota_project_id from the raw JSON (present in both credential types).
        let quota_project = std::env::var("GOOGLE_PROJECT_ID").ok().or_else(|| {
            serde_json::from_str::<serde_json::Value>(&content)
                .ok()
                .and_then(|v| v.get("quota_project_id")?.as_str().map(String::from))
        });

        debug!("credential type: {:?}", match &credentials {
            Credentials::AuthorizedUser { .. } => "authorized_user",
            Credentials::ServiceAccount { .. } => "service_account",
        });
        if let Some(ref qp) = quota_project {
            debug!("quota project: {qp}");
        }

        Ok(Self {
            credentials,
            http,
            cached_token: Arc::new(Mutex::new(None)),
            quota_project,
        })
    }

    /// Returns the quota project ID, if available.
    pub fn quota_project(&self) -> Option<&str> {
        self.quota_project.as_deref()
    }

    /// Get a valid access token, refreshing if needed.
    /// Uses a Mutex to prevent concurrent redundant refreshes.
    pub async fn access_token(&self) -> Result<String> {
        let mut guard = self.cached_token.lock().await;

        // Check cache under the lock.
        if let Some(token) = guard.as_ref() {
            let now = now_secs();
            if now + 60 < token.expires_at {
                return Ok(token.access_token.clone());
            }
        }

        // Refresh token while holding the lock.
        debug!("refreshing access token");
        let resp = match &self.credentials {
            Credentials::AuthorizedUser {
                client_id,
                client_secret,
                refresh_token,
            } => self.refresh_authorized_user(client_id, client_secret, refresh_token).await?,
            Credentials::ServiceAccount {
                client_email,
                private_key,
                token_uri,
            } => {
                let uri = token_uri.as_deref().unwrap_or(TOKEN_URL);
                self.refresh_service_account(client_email, private_key, uri).await?
            }
        };

        let now = now_secs();
        let access_token = resp.access_token.clone();
        *guard = Some(CachedToken {
            access_token: resp.access_token,
            expires_at: now + resp.expires_in,
        });

        Ok(access_token)
    }

    async fn refresh_authorized_user(
        &self,
        client_id: &str,
        client_secret: &str,
        refresh_token: &str,
    ) -> Result<TokenResponse> {
        let resp = self
            .http
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", client_id),
                ("client_secret", client_secret),
                ("refresh_token", refresh_token),
            ])
            .send()
            .await
            .context("token refresh request failed")?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("token refresh failed: {body}");
        }

        resp.json().await.context("failed to parse token response")
    }

    async fn refresh_service_account(
        &self,
        client_email: &str,
        private_key: &str,
        token_uri: &str,
    ) -> Result<TokenResponse> {
        let now = now_secs();
        let claims = JwtClaims {
            iss: client_email.to_string(),
            scope: ANALYTICS_SCOPE.to_string(),
            aud: token_uri.to_string(),
            iat: now,
            exp: now + 3600,
        };

        let key = EncodingKey::from_rsa_pem(private_key.as_bytes())
            .context("failed to parse service account private key")?;
        let jwt = encode(&Header::new(Algorithm::RS256), &claims, &key)
            .context("failed to create JWT")?;

        let resp = self
            .http
            .post(token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &jwt),
            ])
            .send()
            .await
            .context("service account token request failed")?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("service account token exchange failed: {body}");
        }

        resp.json().await.context("failed to parse token response")
    }
}

/// Discover the credentials file path via ADC resolution order.
fn discover_credentials_path() -> Result<PathBuf> {
    // 1. GOOGLE_APPLICATION_CREDENTIALS env var.
    if let Ok(path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return Ok(p);
        }
        bail!(
            "GOOGLE_APPLICATION_CREDENTIALS points to non-existent file: {path}\n\
             Set it to a valid credentials JSON file."
        );
    }

    // 2. Default ADC location from gcloud.
    let default_path = default_adc_path();
    if default_path.exists() {
        return Ok(default_path);
    }

    bail!(
        "No Google credentials found.\n\
         Either:\n  \
         1. Set GOOGLE_APPLICATION_CREDENTIALS to a service account key JSON file, or\n  \
         2. Run: gcloud auth application-default login \
         --scopes=https://www.googleapis.com/auth/analytics.readonly"
    );
}

/// Default ADC credentials path: ~/.config/gcloud/application_default_credentials.json
fn default_adc_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
            PathBuf::from(home).join(".config")
        })
        .join("gcloud")
        .join("application_default_credentials.json")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
