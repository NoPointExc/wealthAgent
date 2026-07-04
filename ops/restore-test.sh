#!/bin/sh
set -eu
latest=$(rclone lsf crypt:db/ | sort | tail -1)
echo "Testing restore of: $latest"
docker run --rm -d --name pg-restore-test -e POSTGRES_PASSWORD=test postgres:16-alpine
sleep 5
rclone cat "crypt:db/${latest}" \
  | docker exec -i pg-restore-test pg_restore -U postgres -d postgres --clean --if-exists --no-owner
docker exec pg-restore-test psql -U postgres -c '\dt' postgres
docker stop pg-restore-test
echo "Restore drill OK."
