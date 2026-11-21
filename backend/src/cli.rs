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
  demo seed                      Seed the demo template user (sandbox data)
  demo status                    Show the demo template's data counts
  plaid status <email>           List a user's items (Plaid / Muse) with account counts
  plaid disconnect <email> [--yes]
                                 Remove the user's Plaid items at Plaid (stops billing)
                                 and mark them revoked. Keeps all stored data. Without
                                 --yes, only shows what would be disconnected.
";

pub async fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let cmd = match args.first() {
        Some(c) => c.as_str(),
        None => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };

    // `demo` needs the full Plaid/sync stack, so it builds the shared AppState;
    // `invite` only needs a DB pool.
    if cmd == "demo" {
        return demo(&args[1..]).await;
    }
    if cmd == "plaid" {
        return plaid(&args[1..]).await;
    }

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

/// Sandbox institution for demo template data (First Platypus Bank — carries
/// both transactions and investments sandbox fixtures).
const DEMO_SANDBOX_INSTITUTION: &str = "ins_109512";

async fn demo(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("status");
    let state = crate::server::build_state().await?;

    // Resolve or create the persistent template user (idempotent).
    db::upsert_user(
        &state.pool,
        &uuid::Uuid::new_v4().to_string(),
        db::DEMO_TEMPLATE_GOOGLE_ID,
        "demo-template@demo.local",
        "Demo Template",
    )
    .await?;
    let template_id = db::get_user_id_by_google(&state.pool, db::DEMO_TEMPLATE_GOOGLE_ID).await?;

    match sub {
        "seed" => {
            let existing: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM plaid_items WHERE user_id = $1 AND revoked_at IS NULL",
            )
            .bind(&template_id)
            .fetch_one(&state.pool)
            .await?;
            if existing > 0 {
                println!("Template already seeded ({existing} active item(s)); nightly sync refreshes it. Run `demo status` for counts.");
                return Ok(());
            }

            println!("Creating sandbox public token ({DEMO_SANDBOX_INSTITUTION})…");
            let public_token = state
                .plaid
                .sandbox_create_public_token(DEMO_SANDBOX_INSTITUTION, &["transactions"])
                .await?;
            let access_token = state.plaid.exchange_public_token(&public_token).await?;
            let encrypted = crate::encryption::encrypt_token(&state.encryption_key, &access_token)?;
            let item_id = uuid::Uuid::new_v4().to_string();
            db::insert_plaid_item(&state.pool, &item_id, &encrypted, &template_id).await?;

            println!("Syncing sandbox data (this can take a minute)…");
            crate::plaid::sync::sync_item(&state, &access_token, &item_id).await?;

            demo_status(&state.pool, &template_id).await?;
            println!("✓ Demo template seeded.");
        }
        "status" => demo_status(&state.pool, &template_id).await?,
        other => {
            eprintln!("unknown demo subcommand: {other}\n\nUSAGE: demo seed | demo status");
            std::process::exit(2);
        }
    }
    Ok(())
}

async fn demo_status(pool: &sqlx::PgPool, template_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    async fn c(pool: &sqlx::PgPool, tid: &str, sql: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(sql).bind(tid).fetch_one(pool).await
    }
    let items = c(pool, template_id, "SELECT count(*) FROM plaid_items WHERE user_id = $1").await?;
    let accounts = c(pool, template_id,
        "SELECT count(*) FROM accounts a JOIN plaid_items pi ON a.plaid_item_id = pi.id WHERE pi.user_id = $1").await?;
    let txns = c(pool, template_id,
        "SELECT count(*) FROM transactions t JOIN accounts a ON t.account_id = a.id JOIN plaid_items pi ON a.plaid_item_id = pi.id WHERE pi.user_id = $1").await?;
    let holdings = c(pool, template_id,
        "SELECT count(*) FROM holdings h JOIN accounts a ON h.account_id = a.id JOIN plaid_items pi ON a.plaid_item_id = pi.id WHERE pi.user_id = $1").await?;
    let inv = c(pool, template_id,
        "SELECT count(*) FROM investment_transactions it JOIN accounts a ON it.account_id = a.id JOIN plaid_items pi ON a.plaid_item_id = pi.id WHERE pi.user_id = $1").await?;
    let demo_users: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE google_id LIKE 'demo:%'")
        .fetch_one(pool).await?;
    println!("Demo template: {items} items, {accounts} accounts, {txns} transactions, {holdings} holdings, {inv} investment txns");
    println!("Live ephemeral demo users: {demo_users}");
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

async fn user_id_by_email(pool: &sqlx::PgPool, email: &str) -> Result<String, Box<dyn std::error::Error>> {
    let id: Option<String> = sqlx::query_scalar("SELECT id FROM users WHERE lower(email) = $1")
        .bind(norm(email))
        .fetch_optional(pool)
        .await?;
    id.ok_or_else(|| format!("no user with email {email}").into())
}

async fn plaid(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("");
    let Some(email) = args.get(1) else {
        eprintln!("usage: plaid status <email> | plaid disconnect <email> [--yes]");
        std::process::exit(2);
    };
    let confirmed = args.iter().any(|a| a == "--yes");
    let state = crate::server::build_state().await?;
    let uid = user_id_by_email(&state.pool, email).await?;

    let items: Vec<(String, String, bool, i64)> = sqlx::query_as(
        "SELECT pi.id, pi.source, pi.revoked_at IS NOT NULL,
                (SELECT count(*) FROM accounts a WHERE a.plaid_item_id = pi.id)
         FROM plaid_items pi WHERE pi.user_id = $1 ORDER BY pi.created_at",
    )
    .bind(&uid)
    .fetch_all(&state.pool)
    .await?;
    for (id, source, revoked, accounts) in &items {
        println!("{id}  source={source}  revoked={revoked}  accounts={accounts}");
    }

    match sub {
        "status" => {}
        "disconnect" => {
            let targets: Vec<&(String, String, bool, i64)> =
                items.iter().filter(|(_, src, revoked, _)| src == "plaid" && !revoked).collect();
            if targets.is_empty() {
                println!("No active Plaid items to disconnect.");
                return Ok(());
            }
            if !confirmed {
                println!("Would disconnect {} Plaid item(s) above. This cannot be undone without re-linking each bank. Re-run with --yes.", targets.len());
                return Ok(());
            }
            for (id, ..) in targets {
                let blob: Option<Vec<u8>> = sqlx::query_scalar("SELECT access_token_enc FROM plaid_items WHERE id = $1")
                    .bind(id)
                    .fetch_one(&state.pool)
                    .await?;
                let Some(blob) = blob else { continue };
                let token = crate::encryption::decrypt_token(&state.encryption_key, &blob)?;
                match state.plaid.remove_item(&token).await {
                    Ok(()) => {
                        db::revoke_plaid_item(&state.pool, id).await?;
                        println!("✓ {id} removed at Plaid and marked revoked (data kept)");
                    }
                    Err(e) => eprintln!("✗ {id}: Plaid /item/remove failed, left active: {e}"),
                }
            }
        }
        other => {
            eprintln!("unknown plaid subcommand: {other}");
            std::process::exit(2);
        }
    }
    Ok(())
}
