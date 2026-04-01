mod api;
mod auth;
mod config;
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

    let http = GoogleAnalyticsClient::build_http_client();

    let clients = match config::load_config()? {
        Some(cfg) => {
            // Multi-account: one client per config entry.
            info!("loaded {} account(s) from config.toml", cfg.accounts.len());
            let mut clients = Vec::with_capacity(cfg.accounts.len());
            for account in &cfg.accounts {
                let path = config::resolve_credentials_path(&account.credentials);
                let auth = GoogleAuth::from_credentials(&path, http.clone())?;
                let client = GoogleAnalyticsClient::new(auth, http.clone());
                clients.push((account.name.clone(), client));
            }
            clients
        }
        None => {
            // Single account: discover credentials via ADC chain.
            let path = auth::discover_credentials_path()?;
            let auth = GoogleAuth::from_credentials(&path, http.clone())?;
            let client = GoogleAnalyticsClient::new(auth, http);
            vec![("default".to_string(), client)]
        }
    };

    let server = GoogleAnalyticsServer::new(clients);

    info!("starting MCP server via stdio");
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
