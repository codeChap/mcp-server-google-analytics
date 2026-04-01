use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::*,
    tool, tool_handler, tool_router,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::api::{
    ApiError, GoogleAnalyticsClient, build_realtime_report_request, build_report_request,
};
use crate::params::{PropertyIdParams, RunRealtimeReportParams, RunReportParams};

/// A named GA client — one per Google account.
struct NamedClient {
    name: String,
    client: GoogleAnalyticsClient,
}

/// Operation to execute against a specific GA4 property.
enum PropertyOp {
    GetDetails,
    ListAdsLinks,
    ListAnnotations,
    GetMetadata,
    RunReport(Value),
    RunRealtimeReport(Value),
}

/// MCP server for Google Analytics 4 — supports multiple Google accounts.
#[derive(Clone)]
pub struct GoogleAnalyticsServer {
    clients: Arc<Vec<NamedClient>>,
    /// Cache: normalized property ID -> index into `clients`.
    property_map: Arc<Mutex<HashMap<String, usize>>>,
    tool_router: ToolRouter<Self>,
}

/// Normalize a property ID to the bare numeric string for use as a cache key.
fn normalize_property_id(id: &str) -> String {
    id.trim()
        .strip_prefix("properties/")
        .unwrap_or(id.trim())
        .to_string()
}

impl GoogleAnalyticsServer {
    fn ok_json(value: &Value) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string_pretty(value).unwrap_or_else(|e| e.to_string());
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    fn api_error(e: impl std::fmt::Display) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
    }

    /// Aggregate account summaries from all connected Google accounts.
    async fn get_all_account_summaries(&self) -> Result<CallToolResult, McpError> {
        let mut all = Vec::new();
        let multi = self.clients.len() > 1;

        for (idx, nc) in self.clients.iter().enumerate() {
            match nc.client.get_account_summaries().await {
                Ok(Value::Array(accounts)) => {
                    // Populate the property -> account cache.
                    for account in &accounts {
                        if let Some(props) =
                            account.get("propertySummaries").and_then(|v| v.as_array())
                        {
                            for prop in props {
                                if let Some(pid) = prop.get("property").and_then(|v| v.as_str()) {
                                    let key = normalize_property_id(pid);
                                    self.property_map.lock().await.insert(key, idx);
                                }
                            }
                        }
                    }

                    if multi {
                        // Tag each account summary with its config name.
                        for mut account in accounts {
                            if let Some(obj) = account.as_object_mut() {
                                obj.insert("_account".into(), Value::String(nc.name.clone()));
                            }
                            all.push(account);
                        }
                    } else {
                        all.extend(accounts);
                    }
                }
                Ok(other) => all.push(other),
                Err(e) if multi => {
                    // In multi-account mode, report the error inline but keep going.
                    all.push(serde_json::json!({
                        "_account": nc.name,
                        "_error": e.to_string(),
                    }));
                }
                Err(e) => return Self::api_error(e),
            }
        }

        Self::ok_json(&Value::Array(all))
    }

    /// Execute a property-scoped operation, routing to the correct Google account.
    async fn exec_property_op(
        &self,
        property_id: &str,
        op: &PropertyOp,
    ) -> Result<CallToolResult, McpError> {
        let key = normalize_property_id(property_id);

        // Check cache.
        let cached_idx = self.property_map.lock().await.get(&key).copied();

        // Single client — skip the search logic.
        if self.clients.len() == 1 {
            return match self.exec_op_on(0, property_id, op).await {
                Ok(data) => Self::ok_json(&data),
                Err(e) => Self::api_error(e),
            };
        }

        // Try cached client first.
        if let Some(idx) = cached_idx {
            match self.exec_op_on(idx, property_id, op).await {
                Ok(data) => return Self::ok_json(&data),
                Err(ApiError::Api { status: 403, .. }) => {
                    // Permission changed — clear stale entry and search.
                    self.property_map.lock().await.remove(&key);
                }
                Err(e) => return Self::api_error(e),
            }
        }

        // Try all remaining clients.
        let mut last_err = String::new();
        for idx in 0..self.clients.len() {
            if cached_idx == Some(idx) {
                continue; // Already tried.
            }
            match self.exec_op_on(idx, property_id, op).await {
                Ok(data) => {
                    self.property_map.lock().await.insert(key, idx);
                    return Self::ok_json(&data);
                }
                Err(e) => last_err = e.to_string(),
            }
        }

        Self::api_error(format!(
            "no account has access to property {property_id}: {last_err}"
        ))
    }

    /// Execute an operation using a specific client.
    async fn exec_op_on(
        &self,
        idx: usize,
        property_id: &str,
        op: &PropertyOp,
    ) -> Result<Value, ApiError> {
        let client = &self.clients[idx].client;
        match op {
            PropertyOp::GetDetails => client.get_property_details(property_id).await,
            PropertyOp::ListAdsLinks => client.list_google_ads_links(property_id).await,
            PropertyOp::ListAnnotations => client.list_property_annotations(property_id).await,
            PropertyOp::GetMetadata => client.get_metadata(property_id).await,
            PropertyOp::RunReport(body) => client.run_report(property_id, body).await,
            PropertyOp::RunRealtimeReport(body) => {
                client.run_realtime_report(property_id, body).await
            }
        }
    }
}

#[tool_router]
impl GoogleAnalyticsServer {
    pub fn new(clients: Vec<(String, GoogleAnalyticsClient)>) -> Self {
        let named: Vec<NamedClient> = clients
            .into_iter()
            .map(|(name, client)| NamedClient { name, client })
            .collect();
        Self {
            clients: Arc::new(named),
            property_map: Arc::new(Mutex::new(HashMap::new())),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Retrieves all Google Analytics account summaries the authenticated user(s) \
                        have access to, including account names and their GA4 properties. \
                        When multiple accounts are configured, results include an _account field \
                        identifying which account owns each summary. \
                        Use this first to discover available property IDs. No parameters required."
    )]
    async fn get_account_summaries(&self) -> Result<CallToolResult, McpError> {
        self.get_all_account_summaries().await
    }

    #[tool(
        description = "Get detailed information about a specific GA4 property including \
                        display name, industry category, time zone, currency code, \
                        service level, create time, and parent account."
    )]
    async fn get_property_details(
        &self,
        Parameters(p): Parameters<PropertyIdParams>,
    ) -> Result<CallToolResult, McpError> {
        self.exec_property_op(&p.property_id, &PropertyOp::GetDetails)
            .await
    }

    #[tool(
        description = "List all Google Ads links for a GA4 property. \
                        Returns linked Google Ads customer IDs and their configuration."
    )]
    async fn list_google_ads_links(
        &self,
        Parameters(p): Parameters<PropertyIdParams>,
    ) -> Result<CallToolResult, McpError> {
        self.exec_property_op(&p.property_id, &PropertyOp::ListAdsLinks)
            .await
    }

    #[tool(
        description = "List reporting data annotations for a GA4 property (Admin API alpha). \
                        Annotations are notes on specific dates or periods used to mark \
                        releases, campaigns, traffic changes, etc."
    )]
    async fn list_property_annotations(
        &self,
        Parameters(p): Parameters<PropertyIdParams>,
    ) -> Result<CallToolResult, McpError> {
        self.exec_property_op(&p.property_id, &PropertyOp::ListAnnotations)
            .await
    }

    #[tool(
        description = "Get custom dimensions and custom metrics defined for a GA4 property. \
                        Returns only user-defined custom dimensions and metrics, \
                        not the standard built-in ones."
    )]
    async fn get_custom_dimensions_and_metrics(
        &self,
        Parameters(p): Parameters<PropertyIdParams>,
    ) -> Result<CallToolResult, McpError> {
        self.exec_property_op(&p.property_id, &PropertyOp::GetMetadata)
            .await
    }

    #[tool(
        description = "Run a report on a GA4 property. Retrieves event-based analytics data \
                        for the specified dimensions and metrics over the given date range(s).\
                        \n\n## References\
                        \n- Dimensions: https://developers.google.com/analytics/devguides/reporting/data/v1/api-schema#dimensions\
                        \n- Metrics: https://developers.google.com/analytics/devguides/reporting/data/v1/api-schema#metrics\
                        \n\n## Date ranges\
                        \nFormat: YYYY-MM-DD or relative strings: \"today\", \"yesterday\", \"NdaysAgo\".\
                        \nExample: [{\"start_date\": \"30daysAgo\", \"end_date\": \"today\"}]\
                        \n\n## Dimension filter\
                        \nA FilterExpression with one of:\
                        \n- filter: {field_name, string_filter: {match_type: EXACT|BEGINS_WITH|ENDS_WITH|CONTAINS|FULL_REGEXP|PARTIAL_REGEXP, value, case_sensitive}}\
                        \n- filter: {field_name, numeric_filter: {operation: EQUAL|LESS_THAN|GREATER_THAN|..., value: {int64_value or double_value}}}\
                        \n- filter: {field_name, in_list_filter: {values: [...], case_sensitive}}\
                        \n- filter: {field_name, between_filter: {from_value: {...}, to_value: {...}}}\
                        \n- and_group: {expressions: [...]}\
                        \n- or_group: {expressions: [...]}\
                        \n- not_expression: {<filter_expression>}\
                        \n\n## Order by\
                        \n- By dimension: {dimension: {dimension_name, order_type: ALPHANUMERIC|CASE_INSENSITIVE_ALPHANUMERIC|NUMERIC}, desc: bool}\
                        \n- By metric: {metric: {metric_name}, desc: bool}"
    )]
    async fn run_report(
        &self,
        Parameters(p): Parameters<RunReportParams>,
    ) -> Result<CallToolResult, McpError> {
        let body = build_report_request(
            &p.date_ranges,
            &p.dimensions,
            &p.metrics,
            p.dimension_filter.as_ref(),
            p.metric_filter.as_ref(),
            p.order_bys.as_ref(),
            p.limit,
            p.offset,
            p.currency_code.as_deref(),
            p.return_property_quota,
        );

        self.exec_property_op(&p.property_id, &PropertyOp::RunReport(body))
            .await
    }

    #[tool(
        description = "Run a realtime report on a GA4 property. Returns data for the last 30 minutes \
                        by default (configurable via minute_ranges).\
                        \n\n## References\
                        \n- Realtime dimensions: https://developers.google.com/analytics/devguides/reporting/data/v1/realtime-api-schema#dimensions\
                        \n- Realtime metrics: https://developers.google.com/analytics/devguides/reporting/data/v1/realtime-api-schema#metrics\
                        \n\nNo date_ranges parameter — use minute_ranges to control the time window.\
                        \nOnly user-scoped custom dimensions (customUser:*) are supported; custom metrics are not.\
                        \n\nFilters and ordering use the same format as run_report."
    )]
    async fn run_realtime_report(
        &self,
        Parameters(p): Parameters<RunRealtimeReportParams>,
    ) -> Result<CallToolResult, McpError> {
        let body = build_realtime_report_request(
            &p.dimensions,
            &p.metrics,
            p.dimension_filter.as_ref(),
            p.metric_filter.as_ref(),
            p.order_bys.as_ref(),
            p.limit,
            p.offset,
            p.minute_ranges.as_ref(),
            p.return_property_quota,
        );

        self.exec_property_op(&p.property_id, &PropertyOp::RunRealtimeReport(body))
            .await
    }
}

#[tool_handler]
impl ServerHandler for GoogleAnalyticsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "google-analytics-mcp",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Google Analytics 4 MCP server. Provides read-only access to GA4 \
                 accounts, properties, reports, and realtime data via the Analytics \
                 Admin and Data APIs. Supports multiple Google accounts — start with \
                 get_account_summaries to discover available property IDs.",
            )
    }
}
