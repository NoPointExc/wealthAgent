#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

VERSION=${1:-$(git rev-parse --short HEAD)}
IMAGE_BASE=ghcr.io/nopointexc/wealthagent

if [ ! -f .env.production ]; then
  echo "ERROR: .env.production not found. Cannot read GOOGLE_CLIENT_ID for frontend build."
  exit 1
fi
. ./.env.production

echo "==> Building backend image (linux/amd64)..."
docker buildx build --platform linux/amd64 \
  -t "${IMAGE_BASE}-backend:${VERSION}" \
  -t "${IMAGE_BASE}-backend:latest" \
  --push \
  ./backend

echo "==> Building frontend+caddy image (linux/amd64)..."
docker buildx build --platform linux/amd64 \
  -f frontend/Dockerfile.runtime \
  --build-arg VITE_GOOGLE_CLIENT_ID="${GOOGLE_CLIENT_ID}" \
  -t "${IMAGE_BASE}-frontend:${VERSION}" \
  -t "${IMAGE_BASE}-frontend:latest" \
  --push \
  .

echo "==> Images pushed:"
echo "    ${IMAGE_BASE}-backend:${VERSION}"
echo "    ${IMAGE_BASE}-frontend:${VERSION}"
