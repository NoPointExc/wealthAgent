#!/bin/sh
# Move the Postgres data volume onto a LUKS-encrypted filesystem.
#
# Run ON THE SERVER, as root, from the repo root:
#
#   sudo ops/encrypt-pgdata.sh --file /var/lib/wealthagent-pgdata.img --size 10G
#   sudo ops/encrypt-pgdata.sh --device /dev/disk/by-id/scsi-0DO_Volume_pgdata
#
# Add --manual-unlock to use a passphrase instead of an on-disk keyfile.
# Trade-off:
#   keyfile (default)  survives unattended reboots; protects a disposed or
#                      detached disk, but NOT a full-server snapshot (the
#                      keyfile is in the snapshot too).
#   --manual-unlock    also protects snapshots; after every reboot you must
#                      SSH in and run: sudo ops/unlock-pgdata.sh
#
# The script: creates the LUKS container, mounts it at /mnt/pgdata-enc,
# takes a pg_dump safety copy INTO the encrypted mount, migrates the existing
# docker volume, and writes docker-compose.override.yml so the pgdata volume
# becomes a bind mount into the encrypted filesystem.
set -eu
cd "$(dirname "$0")/.."

MAPPER=pgdata_crypt
MOUNT=/mnt/pgdata-enc
KEYFILE=/etc/wealthagent/pgdata.key
TARGET="" SIZE="" MODE="" MANUAL=0

while [ $# -gt 0 ]; do
  case "$1" in
    --file)   TARGET="$2"; MODE=file; shift 2 ;;
    --device) TARGET="$2"; MODE=device; shift 2 ;;
    --size)   SIZE="$2"; shift 2 ;;
    --manual-unlock) MANUAL=1; shift ;;
    *) echo "Unknown arg: $1"; exit 1 ;;
  esac
done

[ "$(id -u)" = 0 ] || { echo "ERROR: run as root (sudo)."; exit 1; }
[ -n "$TARGET" ] || { echo "ERROR: pass --file <img-path> --size <e.g. 10G>, or --device <block-dev>."; exit 1; }
[ -e "/dev/mapper/$MAPPER" ] && { echo "ERROR: $MAPPER already open — already set up?"; exit 1; }
grep -q "$MOUNT" /etc/fstab 2>/dev/null && { echo "ERROR: $MOUNT already in fstab."; exit 1; }
command -v cryptsetup >/dev/null || apt-get install -y cryptsetup

if [ "$MODE" = file ]; then
  [ -n "$SIZE" ] || { echo "ERROR: --file requires --size (e.g. 10G)."; exit 1; }
  [ -e "$TARGET" ] && { echo "ERROR: $TARGET already exists."; exit 1; }
  fallocate -l "$SIZE" "$TARGET"
  chmod 0600 "$TARGET"
fi

echo "==> Formatting LUKS container on $TARGET"
if [ "$MANUAL" = 1 ]; then
  cryptsetup luksFormat --type luks2 "$TARGET"
  cryptsetup open "$TARGET" "$MAPPER"
else
  mkdir -p "$(dirname "$KEYFILE")"
  head -c 64 /dev/urandom > "$KEYFILE"
  chmod 0400 "$KEYFILE"
  cryptsetup luksFormat --type luks2 --batch-mode "$TARGET" "$KEYFILE"
  cryptsetup open "$TARGET" "$MAPPER" --key-file "$KEYFILE"
  echo "==> Keyfile written to $KEYFILE — back it up in a password manager."
fi

mkfs.ext4 -q "/dev/mapper/$MAPPER"
mkdir -p "$MOUNT"
mount "/dev/mapper/$MAPPER" "$MOUNT"

# Boot-time config. nofail keeps boot alive if the volume is absent; the
# postgres bind target is a SUBDIRECTORY of the mount, so if the mount is
# missing docker refuses to start postgres instead of silently initializing
# a fresh database on the unencrypted root disk.
if [ "$MANUAL" = 1 ]; then
  echo "$MAPPER $TARGET none luks,noauto" >> /etc/crypttab
  echo "/dev/mapper/$MAPPER $MOUNT ext4 defaults,noauto 0 2" >> /etc/fstab
else
  echo "$MAPPER $TARGET $KEYFILE luks" >> /etc/crypttab
  echo "/dev/mapper/$MAPPER $MOUNT ext4 defaults,nofail 0 2" >> /etc/fstab
fi

VOL="$(basename "$PWD" | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9')_pgdata"
docker volume inspect "$VOL" >/dev/null 2>&1 || { echo "ERROR: docker volume $VOL not found."; exit 1; }

echo "==> Safety dump into the encrypted mount"
docker compose exec -T postgres pg_dump -Fc -U wealthagent wealthagent > "$MOUNT/pre-migration.dump"

echo "==> Stopping stack and migrating $VOL"
docker compose down
SRC="$(docker volume inspect -f '{{ .Mountpoint }}' "$VOL")"
cp -a "$SRC" "$MOUNT/pgdata"
docker volume rm "$VOL"

cat > docker-compose.override.yml <<EOF
# pgdata lives on the LUKS-encrypted filesystem (see ops/encrypt-pgdata.sh).
volumes:
  pgdata:
    driver: local
    driver_opts:
      type: none
      o: bind
      device: $MOUNT/pgdata
EOF

echo "==> Restarting stack"
docker compose up -d

for i in $(seq 1 30); do
  if docker compose exec -T postgres pg_isready -U wealthagent >/dev/null 2>&1; then
    echo "==> Postgres healthy on encrypted volume."
    echo "==> After verifying the app, remove the safety dump: rm $MOUNT/pre-migration.dump"
    exit 0
  fi
  sleep 2
done
echo "ERROR: postgres not healthy after 60s. Old data is still in $MOUNT/pgdata and $MOUNT/pre-migration.dump."
exit 1
