//! Admin CLI (the `admin` binary). Runs inside the backend container via
//! entrypoint.sh so it reuses the same DATABASE_URL / Postgres secret as the
//! server:
//!
//!   docker compose exec backend entrypoint.sh admin invite add  friend@gmail.com "note"
//!   docker compose exec backend entrypoint.sh admin invite list
//!   docker compose exec backend entrypoint.sh admin invite remove friend@gmail.com

use sqlx::postgres::PgPoolOptions;

use crate::db;

const USAGE: &str = "\
WealthAgent admin CLI

USAGE:
  invite add <email> [note...]   Add an email to the invite allowlist (idempotent)
  invite remove <email>          Remove an email from the allowlist
  invite list                    List all invited emails
";

pub async fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let cmd = match args.first() {
        Some(c) => c.as_str(),
        None => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };

    let db_url = std::env::var("DATABASE_URL").map_err(|_| {
        "DATABASE_URL not set — run this through entrypoint.sh inside the container, \
         e.g. `docker compose exec backend entrypoint.sh admin invite list`"
    })?;
    let pool = PgPoolOptions::new().max_connections(2).connect(&db_url).await?;

    match cmd {
        "invite" => invite(&pool, &args[1..]).await?,
        other => {
            eprintln!("unknown command: {other}\n\n{USAGE}");
            std::process::exit(2);
        }
    }
    Ok(())
}

fn norm(email: &str) -> String {
    email.trim().to_lowercase()
}

async fn invite(pool: &sqlx::PgPool, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    match args.first().map(|s| s.as_str()) {
        Some("add") => {
            let email = match args.get(1) {
                Some(e) => norm(e),
                None => { eprintln!("usage: invite add <email> [note...]"); std::process::exit(2); }
            };
            if !email.contains('@') {
                eprintln!("'{email}' doesn't look like an email"); std::process::exit(2);
            }
            let note = if args.len() > 2 { Some(args[2..].join(" ")) } else { None };
            db::invite_add(pool, &email, note.as_deref()).await?;
            println!("✓ invited  {email}");
        }
        Some("remove") | Some("rm") => {
            let email = match args.get(1) {
                Some(e) => norm(e),
                None => { eprintln!("usage: invite remove <email>"); std::process::exit(2); }
            };
            if db::invite_remove(pool, &email).await? == 0 {
                println!("(no invite found for {email})");
            } else {
                println!("✓ removed  {email}");
            }
        }
        Some("list") | Some("ls") | None => {
            let rows = db::invite_list(pool).await?;
            if rows.is_empty() {
                println!("(no invited emails yet)");
            } else {
                println!("{:<36} {:<12} note", "email", "added");
                for (email, note, created) in rows {
                    println!("{:<36} {:<12} {}", email, created.format("%Y-%m-%d"), note.unwrap_or_default());
                }
            }
        }
        Some(other) => {
            eprintln!("unknown invite subcommand: {other}\n\n{USAGE}");
            std::process::exit(2);
        }
    }
    Ok(())
}
