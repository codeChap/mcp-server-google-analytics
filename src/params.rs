use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

/// Parameters for the `login` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct LoginParams {
    #[schemars(
        description = "The account name from config.toml ([[accounts]] `name` field) to \
                        re-authenticate. Starts a loopback OAuth flow, opens the consent URL \
                        in the default browser, captures the callback, and writes a new \
                        refresh_token to that account's credentials file."
    )]
    pub name: String,
}

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

/// Parameters for the `create_custom_dimension` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateCustomDimensionParams {
    #[schemars(
        description = "GA4 property ID. Accepts: bare number (12345), string (\"12345\"), \
                        or resource name (\"properties/12345\")"
    )]
    pub property_id: String,

    #[schemars(
        description = "The event parameter (or user property) name this dimension is registered \
                        against, e.g. \"website_id\", \"suburb\", \"city\". Max 24 chars for \
                        EVENT scope, 24 for USER scope. Cannot be changed after creation."
    )]
    pub parameter_name: String,

    #[schemars(description = "Human-readable display name shown in the GA UI, e.g. \"Website ID\".")]
    pub display_name: String,

    #[schemars(
        description = "Scope of the dimension: \"EVENT\" (event parameter), \"USER\" (user \
                        property), or \"ITEM\" (ecommerce item parameter). Defaults to \"EVENT\"."
    )]
    pub scope: Option<String>,

    #[schemars(description = "Optional description of the custom dimension (max 150 chars).")]
    pub description: Option<String>,

    #[schemars(
        description = "EVENT-scope only: if true, this dimension is NOT sent to Google signals / \
                        used for ads personalization. Optional."
    )]
    pub disallow_ads_personalization: Option<bool>,
}

/// Parameters for the `archive_custom_dimension` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ArchiveCustomDimensionParams {
    #[schemars(description = "GA4 property ID (bare number, string, or \"properties/12345\").")]
    pub property_id: String,

    #[schemars(
        description = "The trailing ID of the custom dimension to archive — the last segment of \
                        its resource name (e.g. \"3\" from \"properties/123/customDimensions/3\"). \
                        Use list_custom_dimensions to find it."
    )]
    pub custom_dimension_id: String,
}

/// Parameters for the `create_key_event` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateKeyEventParams {
    #[schemars(description = "GA4 property ID (bare number, string, or \"properties/12345\").")]
    pub property_id: String,

    #[schemars(
        description = "The event name to mark as a key event (conversion), e.g. \"enquiry\", \
                        \"book\". Must match the event name as collected."
    )]
    pub event_name: String,

    #[schemars(
        description = "How conversions are counted: \"ONCE_PER_EVENT\" (every occurrence) or \
                        \"ONCE_PER_SESSION\" (at most once per session). Defaults to \
                        \"ONCE_PER_EVENT\"."
    )]
    pub counting_method: Option<String>,

    #[schemars(
        description = "Optional default conversion value, as an object: \
                        {\"numericValue\": 100.0, \"currencyCode\": \"ZAR\"}. Both fields required \
                        together if supplied."
    )]
    pub default_value: Option<Value>,
}

/// Parameters for the `delete_key_event` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteKeyEventParams {
    #[schemars(description = "GA4 property ID (bare number, string, or \"properties/12345\").")]
    pub property_id: String,

    #[schemars(
        description = "The trailing ID of the key event to delete — the last segment of its \
                        resource name (e.g. \"5\" from \"properties/123/keyEvents/5\"). \
                        Use list_key_events to find it."
    )]
    pub key_event_id: String,
}

/// Parameters for the `create_property_annotation` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreatePropertyAnnotationParams {
    #[schemars(description = "GA4 property ID (bare number, string, or \"properties/12345\").")]
    pub property_id: String,

    #[schemars(description = "Annotation title, e.g. \"Deployed bounding-box proximity search\".")]
    pub title: String,

    #[schemars(description = "Optional longer description of what changed.")]
    pub description: Option<String>,

    #[schemars(
        description = "The date the annotation marks, as \"YYYY-MM-DD\". If end_date is also \
                        given, this is the start of a date-range annotation."
    )]
    pub date: String,

    #[schemars(
        description = "Optional end date \"YYYY-MM-DD\" to make this a date-range annotation \
                        instead of a single day."
    )]
    pub end_date: Option<String>,

    #[schemars(
        description = "Annotation color. One of: PURPLE, BROWN, BLUE, GREEN, RED, CYAN, ORANGE. \
                        Defaults to PURPLE. (The API rejects an unset color.)"
    )]
    pub color: Option<String>,
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
