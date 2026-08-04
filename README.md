# WealthAgent

Self-hosted net-worth tracker with an AI-agent interface. Connect your bank
and brokerage accounts via [Plaid](https://plaid.com), see your portfolio,
transactions, and capital gains in a web dashboard — and let AI agents
(Claude Desktop, Claude Code, or anything that speaks
[MCP](https://modelcontextprotocol.io)) query and annotate the same data over
an OAuth-protected MCP endpoint.

**Live demo / hosted option:** [www.texasnetworth.com](https://www.texasnetworth.com)
(sign up with a Google account). Everything the hosted service runs is in this
repo — you can spin up the identical stack on your own server.

## Features

- **Portfolio dashboard** — accounts, holdings, balance history, allocation breakdowns
- **Transactions** — search, tags, notes, saved searches, bulk edit
- **Capital gains** — realized-lot tracking with user-supplied cost basis for unknown lots
- **Plaid sync** — daily automatic refresh; access tokens encrypted at rest (ChaCha20-Poly1305)
- **MCP server** — `/mcp` endpoint with OAuth 2.1 (PKCE, dynamic client registration) or personal API tokens, so AI agents can read/annotate your finances with scoped access
- **Multi-user** — Google sign-in with open self-service registration; owner/admin functions stay restricted to an owner list you control

## Stack

| Layer | Tech |
| --- | --- |
| Backend | Rust (axum, sqlx), Postgres 16 |
| Frontend | React + TypeScript + Vite + Tailwind |
| Edge | Caddy (TLS, security headers, reverse proxy) |
| Deploy | Docker Compose on a single VM |

## Quick start (local development)

Prerequisites:

- Rust (stable), Node 20+, Docker
- A [Plaid](https://dashboard.plaid.com) account — free **sandbox** keys work
- A [Google OAuth 2.0 Client ID](https://console.cloud.google.com/apis/credentials) (type: Web application, authorized JS origin `http://localhost:5173`)

### 1. Start Postgres

```sh
docker run -d --name wealth-pg -p 5432:5432 \
  -e POSTGRES_USER=wealthagent -e POSTGRES_PASSWORD=dev -e POSTGRES_DB=wealthagent \
  postgres:16-alpine
```

### 2. Configure and run the backend

```sh
cd backend
openssl rand 32 > keyfile.local          # encryption key for Plaid tokens (gitignored)

cat > .env <<EOF
DATABASE_URL=postgres://wealthagent:dev@localhost:5432/wealthagent
JWT_SECRET=$(openssl rand -base64 64 | tr -d '\n')
APP_ENCRYPTION_KEY_PATH=./keyfile.local
GOOGLE_CLIENT_ID=<your-google-client-id>.apps.googleusercontent.com
OWNER_EMAILS=you@example.com             # your Google email — seeds the invite allowlist
PLAID_CLIENT_ID=<your-plaid-client-id>
PLAID_SECRET=<your-plaid-sandbox-secret>
PLAID_ENV=sandbox
PLAID_PRODUCTS=auth,transactions,investments
PLAID_COUNTRY_CODES=US
EOF

cargo run                                 # migrations run automatically; listens on :8080
```

### 3. Run the frontend

```sh
cd frontend
npm install
echo 'VITE_GOOGLE_CLIENT_ID=<your-google-client-id>.apps.googleusercontent.com' > .env
npm run dev                               # http://localhost:5173, proxies /api to :8080
```

Sign in with the Google account you put in `OWNER_EMAILS`, click **Add
institution**, and use Plaid sandbox credentials (`user_good` / `pass_good`)
to link a fake bank.

### Managing access

The hosted service allows open self-service registration with a Google account.
Self-hosters who prefer to keep sign-in restricted can run in allowlist mode and
add people with the admin CLI:

```sh
# local dev
cargo run --bin admin -- invite add friend@example.com "college roommate"

# production (inside the container)
docker compose exec backend entrypoint.sh admin invite add friend@example.com
```

## Production deployment

The full runbook is in [DEPLOY.md](DEPLOY.md): provision an Ubuntu VM, run
`ops/bootstrap-vm.sh` (Docker, UFW, hardened SSH), generate secrets per
[secrets/README.md](secrets/README.md), point your DNS at the box, edit
`Caddyfile` with your domains, and `docker compose up -d`. Caddy obtains TLS
certificates automatically. Encrypted off-site backups to Backblaze B2 are
set up via `ops/backup.sh` and the systemd units in `ops/systemd/`.

Deploys after the first one are a single command from your laptop:

```sh
DEPLOY_SERVER=wealth@<your-vm-ip> DEPLOY_APP_URL=https://app.example.com ./ops/deploy.sh
```

## Connecting AI agents

Once running, the backend serves MCP at `https://<your-app-domain>/mcp` with
two auth options:

- **OAuth 2.1** — Claude Desktop and other MCP clients can connect with one
  click; the app shows a consent screen with scoped grants you can revoke.
- **Personal API tokens** — create one in the app under Settings → API
  Tokens, then use `Authorization: Bearer wa_pat_...`.

See `backend/skills/wealthAgent/skill.md` for client configuration examples.

## Privacy encryption (operator-blind mode)

Set `PRIVACY_ENCRYPTION=on` and users can opt in (via `POST
/api/privacy/setup` with a passphrase) to per-user encryption of the
identifying text in their data — transaction descriptions, merchant names,
notes, account names, and securities identity (tickers, security names,
holdings). How it works:

- Each opted-in user gets an X25519 keypair. The public key stays in the DB,
  so the nightly Plaid sync **seals new data while the user is offline** —
  encryption needs no secret.
- The private key is stored only *wrapped*: under an Argon2id key derived
  from the user's passphrase, and under keys derived from each API token's
  raw value (of which the DB stores only a hash). Unlocking
  (`POST /api/privacy/unlock`) holds the key in server memory with a 12h TTL.
- Result: `SELECT * FROM transactions` on the server shows ciphertext for
  those fields. The operator cannot read *what* users bought or *where* by
  querying the database or reading disk.

Honest limits: amounts, dates, tags, and quantities stay plaintext (SQL
aggregation depends on them), as do sentinel symbols (CASH/DEBT/N-A) and
Plaid `security_id`s (the holdings upsert key — resolvable by an operator
with their own Plaid credentials); capital gains require an unlocked
session, since FIFO lot matching needs decrypted tickers. A malicious
operator could
still capture plaintext at Plaid-sync time or dump process memory during an
active session — this protects against an honest-but-curious operator and
stolen database copies, not a fully compromised server. There is **no
passphrase recovery**: a lost passphrase means wiping and re-syncing from the
bank (only tags/notes/manual cost basis are truly user-generated). API tokens
created before opting in can't decrypt — recreate them while unlocked.

## Security notes for self-hosters

- All secrets live in files (Docker secrets / `/etc/wealthagent/secrets/`),
  never in the repo. Never commit any `.env` variant.
- Financial rows (balances, transactions) are plaintext inside Postgres by
  default; `ops/encrypt-pgdata.sh` moves the data volume onto a
  LUKS-encrypted filesystem (see DEPLOY.md §14), and
  `PRIVACY_ENCRYPTION=on` adds per-user operator-blind encryption of the
  identifying text columns (see above).
- Plaid access tokens are encrypted at rest; deleting an item calls Plaid's
  `/item/remove` so leaked rows can't be replayed.
- Session JWTs are short-window with a hard 30-day cap; OAuth access tokens
  are audience-bound and type-guarded.
- A pre-commit gitleaks hook is configured in `.pre-commit-config.yaml` —
  run `pre-commit install` after cloning if you plan to contribute.

## License

[MIT](LICENSE). This is not financial advice, and the software comes with no
warranty; you are handling your own bank credentials and data.
