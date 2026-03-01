mod api;
mod auth;
mod params;
mod server;

use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};
use tracing::info;
use tracing_subscriber::EnvFilter;

use api::GoogleAnalyticsClient;
use auth::GoogleAuth;
use server::GoogleAnalyticsServer;

#[tokio::main]
async fn main() -> Result<()> {
    // Tracing writes to stderr so stdout stays clean for MCP JSON-RPC.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    info!("initializing Google Analytics MCP server");

    // Single HTTP client shared between auth and API calls.
    let http = GoogleAnalyticsClient::build_http_client();
    let auth = GoogleAuth::new(http.clone())?;
    let client = GoogleAnalyticsClient::new(auth, http);
    let server = GoogleAnalyticsServer::new(client);

    info!("starting MCP server via stdio");
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
