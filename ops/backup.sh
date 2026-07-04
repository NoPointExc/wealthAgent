#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
ts=$(date -u +%Y-%m-%dT%H-%M-%SZ)
docker compose exec -T postgres pg_dump -Fc -U wealthagent wealthagent \
  | rclone rcat "crypt:db/wealthagent-${ts}.dump"
rclone delete --min-age 14d --include 'wealthagent-*.dump' crypt:db/
echo "Backup complete: wealthagent-${ts}.dump"
