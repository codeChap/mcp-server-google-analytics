use anyhow::Result;
use reqwest::Client;
use serde_json::{Map, Value, json};
use std::time::Duration;
use thiserror::Error;
use tracing::debug;
use url::Url;

use crate::auth::GoogleAuth;

const ADMIN_V1BETA: &str = "https://analyticsadmin.googleapis.com/v1beta";
const ADMIN_V1ALPHA: &str = "https://analyticsadmin.googleapis.com/v1alpha";
const DATA_V1BETA: &str = "https://analyticsdata.googleapis.com/v1beta";
const USER_AGENT: &str = concat!("google-analytics-mcp/", env!("CARGO_PKG_VERSION"));

/// Default timeout for admin/metadata requests (30s).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Longer timeout for report requests which can be slow on large properties.
const REPORT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("token refresh failed: {0}")]
    TokenRefresh(#[source] anyhow::Error),

    #[error("HTTP request failed: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("Google API error ({status}): {body}")]
    Api { status: u16, body: String },
}

/// Client for Google Analytics 4 REST APIs.
pub struct GoogleAnalyticsClient {
    auth: GoogleAuth,
    http: Client,
}

impl GoogleAnalyticsClient {
    pub fn new(auth: GoogleAuth, http: Client) -> Self {
        Self { auth, http }
    }

    /// Build a shared reqwest::Client with the custom user agent.
    pub fn build_http_client() -> Client {
        Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .expect("failed to build HTTP client")
    }

    /// Add common auth headers (Bearer token + quota project) to a request builder.
    fn apply_auth(&self, builder: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
        let b = builder.bearer_auth(token);
        match self.auth.quota_project() {
            Some(qp) => b.header("x-goog-user-project", qp),
            None => b,
        }
    }

    /// Make an authenticated GET request with the given timeout.
    async fn get(&self, url: &str, timeout: Duration) -> Result<Value, ApiError> {
        let token = self.auth.access_token().await.map_err(ApiError::TokenRefresh)?;
        debug!("GET {url}");

        let builder = self.http.get(url).timeout(timeout);
        let resp = self.apply_auth(builder, &token).send().await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(resp.json().await?)
    }

    /// Make an authenticated POST request with a JSON body and the given timeout.
    async fn post(&self, url: &str, body: &Value, timeout: Duration) -> Result<Value, ApiError> {
        let token = self.auth.access_token().await.map_err(ApiError::TokenRefresh)?;
        debug!("POST {url}");

        let builder = self.http.post(url).json(body).timeout(timeout);
        let resp = self.apply_auth(builder, &token).send().await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(resp.json().await?)
    }

    /// Auto-paginate a GET endpoint that returns items under `items_key` with `nextPageToken`.
    async fn get_all_pages(&self, base_url: &str, items_key: &str) -> Result<Value, ApiError> {
        let mut all_items: Vec<Value> = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = Url::parse(base_url)
                .expect("invalid base URL — this is a bug");
            url.query_pairs_mut().append_pair("pageSize", "200");
            if let Some(token) = &page_token {
                url.query_pairs_mut().append_pair("pageToken", token);
            }

            let resp = self.get(url.as_str(), DEFAULT_TIMEOUT).await?;

            if let Some(items) = resp.get(items_key).and_then(|v| v.as_array()) {
                all_items.extend(items.iter().cloned());
            }

            match resp.get("nextPageToken").and_then(|v| v.as_str()) {
                Some(token) if !token.is_empty() => page_token = Some(token.to_string()),
                _ => break,
            }
        }

        Ok(json!(all_items))
    }

    // ── Admin API v1beta ──────────────────────────────────────────────

    /// List all account summaries with auto-pagination.
    pub async fn get_account_summaries(&self) -> Result<Value, ApiError> {
        let url = format!("{ADMIN_V1BETA}/accountSummaries");
        self.get_all_pages(&url, "accountSummaries").await
    }

    /// Get details for a single property.
    pub async fn get_property_details(&self, property_id: &str) -> Result<Value, ApiError> {
        let rn = property_resource_name(property_id);
        let url = format!("{ADMIN_V1BETA}/{rn}");
        self.get(&url, DEFAULT_TIMEOUT).await
    }

    /// List Google Ads links for a property with auto-pagination.
    pub async fn list_google_ads_links(&self, property_id: &str) -> Result<Value, ApiError> {
        let rn = property_resource_name(property_id);
        let url = format!("{ADMIN_V1BETA}/{rn}/googleAdsLinks");
        self.get_all_pages(&url, "googleAdsLinks").await
    }

    // ── Admin API v1alpha ─────────────────────────────────────────────

    /// List reporting data annotations for a property with auto-pagination.
    pub async fn list_property_annotations(&self, property_id: &str) -> Result<Value, ApiError> {
        let rn = property_resource_name(property_id);
        let url = format!("{ADMIN_V1ALPHA}/{rn}/reportingDataAnnotations");
        self.get_all_pages(&url, "reportingDataAnnotations").await
    }

    // ── Data API v1beta ───────────────────────────────────────────────

    /// Get metadata (custom dimensions and metrics) for a property.
    pub async fn get_metadata(&self, property_id: &str) -> Result<Value, ApiError> {
        let rn = property_resource_name(property_id);
        let url = format!("{DATA_V1BETA}/{rn}/metadata");
        let resp = self.get(&url, DEFAULT_TIMEOUT).await?;

        // Filter to custom definitions only.
        let custom_dimensions = filter_custom(&resp, "dimensions");
        let custom_metrics = filter_custom(&resp, "metrics");

        Ok(json!({
            "custom_dimensions": custom_dimensions,
            "custom_metrics": custom_metrics,
        }))
    }

    /// Run a standard report.
    pub async fn run_report(
        &self,
        property_id: &str,
        request_body: Value,
    ) -> Result<Value, ApiError> {
        let rn = property_resource_name(property_id);
        let url = format!("{DATA_V1BETA}/{rn}:runReport");
        self.post(&url, &request_body, REPORT_TIMEOUT).await
    }

    /// Run a realtime report.
    pub async fn run_realtime_report(
        &self,
        property_id: &str,
        request_body: Value,
    ) -> Result<Value, ApiError> {
        let rn = property_resource_name(property_id);
        let url = format!("{DATA_V1BETA}/{rn}:runRealtimeReport");
        self.post(&url, &request_body, REPORT_TIMEOUT).await
    }
}

/// Normalize a property ID to the resource name format `properties/12345`.
/// Accepts: `12345`, `"12345"`, or `"properties/12345"`.
fn property_resource_name(property_id: &str) -> String {
    let id = property_id.trim();
    if id.starts_with("properties/") {
        id.to_string()
    } else {
        format!("properties/{id}")
    }
}

/// Filter metadata entries to only those with `customDefinition: true`.
fn filter_custom(metadata: &Value, key: &str) -> Vec<Value> {
    metadata
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|item| {
                    item.get("customDefinition")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Recursively convert all object keys from snake_case to camelCase.
/// Keys already in camelCase pass through unchanged (no underscores to convert).
pub fn snake_to_camel_case(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let new_map: Map<String, Value> = map
                .iter()
                .map(|(k, v)| (to_camel_case(k), snake_to_camel_case(v)))
                .collect();
            Value::Object(new_map)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(snake_to_camel_case).collect()),
        other => other.clone(),
    }
}

fn to_camel_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = false;
    for (i, c) in s.chars().enumerate() {
        if c == '_' {
            // Preserve leading underscores as-is.
            if i == 0 || result.is_empty() {
                result.push(c);
            } else {
                capitalize_next = true;
            }
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

/// Build a report request body, inserting the shared optional fields.
/// Used by both `run_report` and `run_realtime_report`.
fn insert_common_report_fields(
    obj: &mut Map<String, Value>,
    dimension_filter: Option<&Value>,
    metric_filter: Option<&Value>,
    order_bys: Option<&Value>,
    limit: Option<i64>,
    offset: Option<i64>,
    return_property_quota: Option<bool>,
) {
    if let Some(f) = dimension_filter {
        obj.insert("dimensionFilter".into(), snake_to_camel_case(f));
    }
    if let Some(f) = metric_filter {
        obj.insert("metricFilter".into(), snake_to_camel_case(f));
    }
    if let Some(o) = order_bys {
        obj.insert("orderBys".into(), snake_to_camel_case(o));
    }
    if let Some(l) = limit {
        obj.insert("limit".into(), json!(l.to_string()));
    }
    if let Some(o) = offset {
        obj.insert("offset".into(), json!(o.to_string()));
    }
    if let Some(q) = return_property_quota {
        obj.insert("returnPropertyQuota".into(), json!(q));
    }
}

/// Build the REST API request body for `runReport`.
pub fn build_report_request(
    date_ranges: &[Value],
    dimensions: &[String],
    metrics: &[String],
    dimension_filter: Option<&Value>,
    metric_filter: Option<&Value>,
    order_bys: Option<&Value>,
    limit: Option<i64>,
    offset: Option<i64>,
    currency_code: Option<&str>,
    return_property_quota: Option<bool>,
) -> Value {
    let mut body = json!({
        "dateRanges": date_ranges.iter().map(|dr| snake_to_camel_case(dr)).collect::<Vec<_>>(),
        "dimensions": dimensions.iter().map(|d| json!({"name": d})).collect::<Vec<_>>(),
        "metrics": metrics.iter().map(|m| json!({"name": m})).collect::<Vec<_>>(),
    });

    let obj = body.as_object_mut().unwrap();
    insert_common_report_fields(obj, dimension_filter, metric_filter, order_bys, limit, offset, return_property_quota);

    if let Some(c) = currency_code {
        obj.insert("currencyCode".into(), json!(c));
    }

    body
}

/// Build the REST API request body for `runRealtimeReport`.
pub fn build_realtime_report_request(
    dimensions: &[String],
    metrics: &[String],
    dimension_filter: Option<&Value>,
    metric_filter: Option<&Value>,
    order_bys: Option<&Value>,
    limit: Option<i64>,
    offset: Option<i64>,
    minute_ranges: Option<&Value>,
    return_property_quota: Option<bool>,
) -> Value {
    let mut body = json!({
        "dimensions": dimensions.iter().map(|d| json!({"name": d})).collect::<Vec<_>>(),
        "metrics": metrics.iter().map(|m| json!({"name": m})).collect::<Vec<_>>(),
    });

    let obj = body.as_object_mut().unwrap();
    insert_common_report_fields(obj, dimension_filter, metric_filter, order_bys, limit, offset, return_property_quota);

    if let Some(mr) = minute_ranges {
        obj.insert("minuteRanges".into(), snake_to_camel_case(mr));
    }

    body
}
