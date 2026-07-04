//! HTTP server binary. All logic lives in the `wealth_agent_backend` library;
//! this is just the entry point.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    wealth_agent_backend::server::run().await
}
