use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

/// Parameters for tools that only need a property ID.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PropertyIdParams {
    #[schemars(
        description = "GA4 property ID. Accepts: bare number (12345), string (\"12345\"), \
                        or resource name (\"properties/12345\")"
    )]
    pub property_id: String,
}

/// Parameters for the `run_report` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunReportParams {
    #[schemars(
        description = "GA4 property ID. Accepts: bare number (12345), string (\"12345\"), \
                        or resource name (\"properties/12345\")"
    )]
    pub property_id: String,

    #[schemars(
        description = "Date ranges for the report. Each object has: \
                        start_date (required, YYYY-MM-DD or relative: \"today\", \"yesterday\", \"NdaysAgo\"), \
                        end_date (required, same format), \
                        name (optional). \
                        Example: [{\"start_date\": \"30daysAgo\", \"end_date\": \"today\"}]"
    )]
    pub date_ranges: Vec<Value>,

    #[schemars(
        description = "List of dimension names. See: \
                        https://developers.google.com/analytics/devguides/reporting/data/v1/api-schema#dimensions \
                        Example: [\"country\", \"city\"]"
    )]
    pub dimensions: Vec<String>,

    #[schemars(
        description = "List of metric names. See: \
                        https://developers.google.com/analytics/devguides/reporting/data/v1/api-schema#metrics \
                        Example: [\"activeUsers\", \"sessions\"]"
    )]
    pub metrics: Vec<String>,

    #[schemars(
        description = "Optional dimension filter expression. Supports: \
                        simple filter (string_filter, numeric_filter, in_list_filter, between_filter), \
                        not_expression, and_group, or_group. \
                        Example: {\"filter\": {\"field_name\": \"country\", \
                        \"string_filter\": {\"match_type\": \"EXACT\", \"value\": \"US\"}}}"
    )]
    pub dimension_filter: Option<Value>,

    #[schemars(
        description = "Optional metric filter expression. Same structure as dimension_filter. \
                        Example: {\"filter\": {\"field_name\": \"activeUsers\", \
                        \"numeric_filter\": {\"operation\": \"GREATER_THAN\", \
                        \"value\": {\"int64_value\": 100}}}}"
    )]
    pub metric_filter: Option<Value>,

    #[schemars(
        description = "Optional ordering. List of order_by objects. Each can have: \
                        dimension (with dimension_name and optional order_type: ALPHANUMERIC, \
                        CASE_INSENSITIVE_ALPHANUMERIC, NUMERIC), \
                        metric (with metric_name), or pivot; plus desc (bool). \
                        Example: [{\"metric\": {\"metric_name\": \"activeUsers\"}, \"desc\": true}]"
    )]
    pub order_bys: Option<Value>,

    #[schemars(description = "Maximum rows to return (max 250000). Used for pagination.")]
    pub limit: Option<i64>,

    #[schemars(description = "Row offset for pagination.")]
    pub offset: Option<i64>,

    #[schemars(description = "ISO 4217 currency code (e.g. \"USD\", \"EUR\").")]
    pub currency_code: Option<String>,

    #[schemars(description = "Whether to include property quota information in the response.")]
    pub return_property_quota: Option<bool>,
}

/// Parameters for the `run_realtime_report` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunRealtimeReportParams {
    #[schemars(
        description = "GA4 property ID. Accepts: bare number (12345), string (\"12345\"), \
                        or resource name (\"properties/12345\")"
    )]
    pub property_id: String,

    #[schemars(
        description = "List of realtime dimension names. See: \
                        https://developers.google.com/analytics/devguides/reporting/data/v1/realtime-api-schema#dimensions \
                        Example: [\"country\", \"unifiedScreenName\"]"
    )]
    pub dimensions: Vec<String>,

    #[schemars(
        description = "List of realtime metric names. See: \
                        https://developers.google.com/analytics/devguides/reporting/data/v1/realtime-api-schema#metrics \
                        Example: [\"activeUsers\"]"
    )]
    pub metrics: Vec<String>,

    #[schemars(
        description = "Optional dimension filter expression. Same structure as run_report's dimension_filter."
    )]
    pub dimension_filter: Option<Value>,

    #[schemars(
        description = "Optional metric filter expression. Same structure as run_report's metric_filter."
    )]
    pub metric_filter: Option<Value>,

    #[schemars(description = "Optional ordering. Same structure as run_report's order_bys.")]
    pub order_bys: Option<Value>,

    #[schemars(description = "Maximum rows to return.")]
    pub limit: Option<i64>,

    #[schemars(description = "Row offset for pagination.")]
    pub offset: Option<i64>,

    #[schemars(
        description = "Optional minute ranges for the realtime report. By default covers the last 30 minutes. \
                        Each object has: start_minutes_ago (int, max 29), end_minutes_ago (int, min 0), name (optional). \
                        Example: [{\"start_minutes_ago\": 10, \"end_minutes_ago\": 0}]"
    )]
    pub minute_ranges: Option<serde_json::Value>,

    #[schemars(description = "Whether to include property quota information in the response.")]
    pub return_property_quota: Option<bool>,
}
