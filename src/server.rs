use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::*,
    tool, tool_handler, tool_router,
};
use serde_json::Value;
use std::sync::Arc;

use crate::api::{
    GoogleAnalyticsClient, build_realtime_report_request, build_report_request,
};
use crate::params::{PropertyIdParams, RunRealtimeReportParams, RunReportParams};

/// MCP server for Google Analytics 4.
#[derive(Clone)]
pub struct GoogleAnalyticsServer {
    client: Arc<GoogleAnalyticsClient>,
    tool_router: ToolRouter<Self>,
}

impl GoogleAnalyticsServer {
    fn ok_json(value: &Value) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string_pretty(value).unwrap_or_else(|e| e.to_string());
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    fn api_error(e: impl std::fmt::Display) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
    }
}

#[tool_router]
impl GoogleAnalyticsServer {
    pub fn new(client: GoogleAnalyticsClient) -> Self {
        Self {
            client: Arc::new(client),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Retrieves all Google Analytics account summaries the authenticated user \
                        has access to, including account names and their GA4 properties. \
                        Use this first to discover available property IDs. No parameters required."
    )]
    async fn get_account_summaries(&self) -> Result<CallToolResult, McpError> {
        match self.client.get_account_summaries().await {
            Ok(data) => Self::ok_json(&data),
            Err(e) => Self::api_error(e),
        }
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
        match self.client.get_property_details(&p.property_id).await {
            Ok(data) => Self::ok_json(&data),
            Err(e) => Self::api_error(e),
        }
    }

    #[tool(
        description = "List all Google Ads links for a GA4 property. \
                        Returns linked Google Ads customer IDs and their configuration."
    )]
    async fn list_google_ads_links(
        &self,
        Parameters(p): Parameters<PropertyIdParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.client.list_google_ads_links(&p.property_id).await {
            Ok(data) => Self::ok_json(&data),
            Err(e) => Self::api_error(e),
        }
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
        match self.client.list_property_annotations(&p.property_id).await {
            Ok(data) => Self::ok_json(&data),
            Err(e) => Self::api_error(e),
        }
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
        match self.client.get_metadata(&p.property_id).await {
            Ok(data) => Self::ok_json(&data),
            Err(e) => Self::api_error(e),
        }
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

        match self.client.run_report(&p.property_id, body).await {
            Ok(data) => Self::ok_json(&data),
            Err(e) => Self::api_error(e),
        }
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

        match self.client.run_realtime_report(&p.property_id, body).await {
            Ok(data) => Self::ok_json(&data),
            Err(e) => Self::api_error(e),
        }
    }
}

#[tool_handler]
impl ServerHandler for GoogleAnalyticsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "google-analytics-mcp".into(),
                title: None,
                version: env!("CARGO_PKG_VERSION").into(),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Google Analytics 4 MCP server. Provides read-only access to GA4 \
                 accounts, properties, reports, and realtime data via the Analytics \
                 Admin and Data APIs. Start with get_account_summaries to discover \
                 available property IDs."
                    .into(),
            ),
        }
    }
}
