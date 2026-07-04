//! Admin CLI binary (`admin`). Thin entry point over `cli::run`. Run inside the
//! backend container via entrypoint.sh so secrets/DATABASE_URL are loaded:
//!
//!   docker compose exec backend entrypoint.sh admin invite list

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let args: Vec<String> = std::env::args().skip(1).collect();
    wealth_agent_backend::cli::run(&args).await
}
