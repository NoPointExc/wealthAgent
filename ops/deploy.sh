#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

SERVER="${DEPLOY_SERVER:?Set DEPLOY_SERVER, e.g. DEPLOY_SERVER=wealth@your-vm-ip}"
SSH_KEY="${DEPLOY_SSH_KEY:-$HOME/.ssh/id_ed25519}"
APP_URL="${DEPLOY_APP_URL:?Set DEPLOY_APP_URL, e.g. DEPLOY_APP_URL=https://app.example.com}"

if [ -n "$(git status --porcelain 2>/dev/null)" ]; then
  echo "ERROR: working tree has uncommitted/untracked files. Commit or stash first."
  git status --short
  exit 1
fi

if [ ! -f .env.production ]; then
  echo "ERROR: .env.production not found. Copy .env.production.example and fill in."
  exit 1
fi

echo "==> Building and pushing images to GHCR..."
./ops/build-push.sh

echo "==> Syncing config files to server..."
rsync -e "ssh -i ${SSH_KEY}" \
  docker-compose.yml Caddyfile \
  "${SERVER}:~/wealthAgent/"

echo "==> Pulling images and restarting stack on server..."
ssh -i "${SSH_KEY}" "${SERVER}" 'bash -s' <<'ENDSSH'
cd ~/wealthAgent
docker compose pull
docker compose up -d --remove-orphans
ENDSSH

echo "==> Waiting for healthcheck..."
for i in $(seq 1 30); do
  if curl -sf "${APP_URL}/api/health" >/dev/null 2>&1; then
    echo "==> Healthy."
    exit 0
  fi
  sleep 2
done
echo "ERROR: not healthy after 60s. Check: ssh -i ${SSH_KEY} ${SERVER} 'docker compose logs'"
exit 1
