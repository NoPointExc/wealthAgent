# secrets/

Each file in this directory holds exactly one secret value (no trailing newline
where noted). Docker Compose mounts them as read-only files at `/run/secrets/<name>`.

**Never commit real values.** The `.gitignore` excludes every file here except
this README and `.gitkeep`.

## Files required before first deploy

| File | How to generate |
|------|----------------|
| `postgres_password` | `openssl rand -base64 32 > secrets/postgres_password` |
| `app_encryption_key` | `openssl rand 32 > secrets/app_encryption_key` (raw 32 bytes) |
| `jwt_secret` | `openssl rand -base64 64 > secrets/jwt_secret` |
| `plaid_client_id` | `printf '%s' '<your-client-id>' > secrets/plaid_client_id` |
| `plaid_secret` | `printf '%s' '<your-secret>' > secrets/plaid_secret` |

After creating all files, lock down permissions:

```sh
chmod 0400 secrets/*
```

## Production secret location

On the production server, secrets live at `/etc/wealthagent/secrets/` (owned by `root`, mode `0400`), **not** in this directory. `docker-compose.yml` points to the absolute path. After the initial generate-and-deploy step, shred the copies here and leave only `.gitkeep`.

See `DEPLOY.md §11` for the relocation procedure.

## Notes

- `app_encryption_key` is 32 raw bytes used by ChaCha20-Poly1305. Do not base64-encode it.
- `postgres_password` is used both by the `postgres` service (`POSTGRES_PASSWORD_FILE`) and
  by the backend `entrypoint.sh` to construct `DATABASE_URL` automatically.
- Losing `app_encryption_key` makes all encrypted Plaid access tokens unreadable.
  Back it up in a password manager alongside your B2 rclone crypt passphrase.
