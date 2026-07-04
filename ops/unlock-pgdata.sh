#!/bin/sh
# Unlock and mount the encrypted pgdata volume after a reboot, then start the
# stack. Only needed if ops/encrypt-pgdata.sh was run with --manual-unlock.
set -eu
cd "$(dirname "$0")/.."

MAPPER=pgdata_crypt
MOUNT=/mnt/pgdata-enc

[ "$(id -u)" = 0 ] || { echo "ERROR: run as root (sudo)."; exit 1; }

if [ ! -e "/dev/mapper/$MAPPER" ]; then
  TARGET="$(awk -v m="$MAPPER" '$1 == m { print $2 }' /etc/crypttab)"
  [ -n "$TARGET" ] || { echo "ERROR: $MAPPER not found in /etc/crypttab."; exit 1; }
  cryptsetup open "$TARGET" "$MAPPER"
fi

mountpoint -q "$MOUNT" || mount "/dev/mapper/$MAPPER" "$MOUNT"
docker compose up -d
echo "==> Unlocked, mounted, stack started."
