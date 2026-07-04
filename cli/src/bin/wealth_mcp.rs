use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::{fmt, EnvFilter};
use wealthagent_cli::{client::WealthClient, config::Config, mcp::WealthMcpServer};

#[tokio::main]
async fn main() -> Result<()> {
    // MCP clients capture stdout — log to stderr only.
    fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let cfg = Config::from_env().map_err(|e| {
        eprintln!("wealth-mcp: configuration error: {e}");
        e
    })?;

    tracing::info!(base_url = %cfg.base_url, "wealth-mcp starting");

    let client = WealthClient::new(&cfg);
    let server = WealthMcpServer::new(client);

    let service = server
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!("MCP transport error: {e:?}"))?;

    service.waiting().await?;
    Ok(())
}
