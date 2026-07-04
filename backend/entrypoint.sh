#!/bin/sh
set -eu

load_secret() {
  var_name="$1"
  file_var="${var_name}_FILE"
  file_path=$(eval "echo \${${file_var}:-}")
  if [ -n "$file_path" ] && [ -f "$file_path" ]; then
    val=$(cat "$file_path")
    export "$var_name=$val"
  fi
}

load_secret JWT_SECRET
load_secret PLAID_CLIENT_ID
load_secret PLAID_SECRET

# Construct DATABASE_URL from postgres_password secret if DATABASE_URL is not set directly.
# Percent-encode chars that are valid in base64 but not in a URL password field.
if [ -z "${DATABASE_URL:-}" ] && [ -f /run/secrets/postgres_password ]; then
  PG_PASS=$(cat /run/secrets/postgres_password)
  PG_PASS_ENC=$(printf '%s' "$PG_PASS" | sed 's/%/%25/g; s/+/%2B/g; s|/|%2F|g; s/=/%3D/g')
  export DATABASE_URL="postgresql://wealthagent:${PG_PASS_ENC}@postgres:5432/wealthagent"
fi

# With no args → start the HTTP server (default ENTRYPOINT use).
# With args  → run that command (e.g. `entrypoint.sh admin invite add x@y.com`),
# reusing the secrets / DATABASE_URL loaded above. `admin` is on PATH.
if [ "$#" -gt 0 ]; then
  exec "$@"
fi
exec /usr/local/bin/wealth-agent-backend
