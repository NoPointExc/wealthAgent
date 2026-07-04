# WealthAgent — Deploy Runbook (Phase 1)

Target: Hetzner CPX21 · Caddy TLS · Postgres in Docker · Backblaze B2 backups

---

## 1 · Provision VM

In [Hetzner Cloud](https://console.hetzner.cloud):

- Image: **Ubuntu 24.04**  Type: **CPX21** (3 vCPU, 4 GB RAM, 80 GB SSD)
- Add your SSH public key during creation · Name: `wealthagent-prod`

SSH in as root, create a non-root user, then reconnect as that user:

```sh
adduser wealth
usermod -aG sudo wealth
mkdir -p /home/wealth/.ssh && cp /root/.ssh/authorized_keys /home/wealth/.ssh/
chown -R wealth:wealth /home/wealth/.ssh
# now reconnect as: ssh wealth@<VM-IP>
```

## 2 · Bootstrap the VM

```sh
git clone <your-repo-url> ~/wealthAgent
cd ~/wealthAgent
./ops/bootstrap-vm.sh
# Log out and back in so the `docker` group takes effect
```

> **Do not use `scp -r` from a Mac.** It copies `.DS_Store`, local `.env` files, and other cruft that should never be on the server. Always use `git clone` on the VM, then `git pull` for updates.

## 3 · Generate secrets

```sh
cd ~/wealthAgent
mkdir -p secrets
openssl rand -base64 32  > secrets/postgres_password
openssl rand 32          > secrets/app_encryption_key   # raw bytes — do not base64
openssl rand -base64 64  > secrets/jwt_secret
printf '%s' '<plaid-client-id>' > secrets/plaid_client_id
printf '%s' '<plaid-secret>'    > secrets/plaid_secret
chmod 0400 secrets/*
```

Store `app_encryption_key` and your B2 rclone crypt passphrase in a password manager — losing either means losing data.

## 4 · Configure rclone (Backblaze B2 backups)

```sh
rclone config
# Create a B2 remote with your Backblaze keys, then a crypt remote wrapping it.
rclone lsf crypt:    # should return empty — confirms connectivity
```

## 5 · Fill in production env

```sh
cp .env.production.example .env.production
vim .env.production   # set HOSTNAME, ALLOWED_ORIGIN, GOOGLE_CLIENT_ID
```

Edit `Caddyfile`: replace `app.example.com` with your real subdomain and set the `tls` email address.

## 6 · DNS A record

In Cloudflare DNS for your domain:

- Type: **A** · Name: `app` · IPv4: the Hetzner VM IP
- Proxy status: **DNS only** (gray cloud) — Caddy needs direct port 80/443 for Let's Encrypt
- TTL: 300

Verify propagation: `dig @1.1.1.1 app.<your-domain>` returns the VM IP.

## 7 · Register OAuth origins

- **Plaid dashboard** → Team Settings → API → add `https://app.<your-domain>` to allowed redirect URIs
- **Google Cloud console** → Credentials → your OAuth Client ID → add `https://app.<your-domain>` to "Authorized JavaScript origins"

## 8 · Authenticate with GHCR (one-time per machine)

Images are stored in the GitHub Container Registry under `ghcr.io/nopointexc/`.

**On your laptop** (to push):
```sh
# Create a PAT at https://github.com/settings/tokens/new
# Required scopes: write:packages, read:packages, delete:packages
echo "<your-PAT>" | docker login ghcr.io -u NoPointExc --password-stdin
```

**On the server** (to pull — needs a separate PAT with read:packages only):
```sh
ssh -i ~/.ssh/id_ed25519 wealth@<VM-IP>
echo "<read-only-PAT>" | docker login ghcr.io -u NoPointExc --password-stdin
```

The server PAT is stored in `~/.docker/config.json`. Treat it as a secret.

## 9 · Deploy

```sh
./ops/deploy.sh
```

This builds both images locally (linux/amd64 via buildx), pushes them to GHCR, rsyncs `docker-compose.yml` and `Caddyfile` to the server, then pulls and restarts the stack. The server needs no source code, Rust toolchain, or Node.js.

## 10 · Browser smoke check

- `https://app.<your-domain>` loads with a green padlock
- HTTP redirects to HTTPS
- Google sign-in works; cookie is `HttpOnly; Secure; SameSite=Strict`
- Link a production Plaid institution; balances + transactions appear

## 11 · Enable nightly backups

```sh
sudo cp ops/systemd/wealthagent-backup.service /etc/systemd/system/
sudo cp ops/systemd/wealthagent-backup.timer   /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now wealthagent-backup.timer
# Force a manual run to verify:
sudo systemctl start wealthagent-backup.service
rclone lsf crypt:db/   # should show one dump
```

Run the restore drill: `./ops/restore-test.sh` → should print "Restore drill OK."  
Calendar reminder: repeat restore drill every 3 months.

## 12 · Relocate secrets to system-owned path

After the first deploy, move secrets out of the git working tree to a root-owned path Docker can read but `wealth` cannot:

```sh
sudo mkdir -p /etc/wealthagent/secrets
sudo cp ~/wealthAgent/secrets/* /etc/wealthagent/secrets/
sudo chown -R root:root /etc/wealthagent
sudo chmod 0700 /etc/wealthagent /etc/wealthagent/secrets
sudo chmod 0400 /etc/wealthagent/secrets/*
# Then pull the updated docker-compose.yml and restart:
cd ~/wealthAgent && git pull && docker compose up -d
# Finally, shred the copies from the repo dir:
shred -u ~/wealthAgent/secrets/postgres_password \
         ~/wealthAgent/secrets/app_encryption_key \
         ~/wealthAgent/secrets/jwt_secret \
         ~/wealthAgent/secrets/plaid_client_id \
         ~/wealthAgent/secrets/plaid_secret
```

## 13 · Remove source code from server (after registry cutover)

Once `ops/deploy.sh` has run successfully at least once with registry images, the server only needs config files:

```sh
ssh -i ~/.ssh/id_ed25519 wealth@<VM-IP>
cd ~/wealthAgent

# Confirm stack is healthy first
docker compose ps

# Remove source code — keep only docker-compose.yml, Caddyfile, .env.production
rm -rf backend frontend migrations ops docs
```

The server footprint after cleanup: `docker-compose.yml`, `Caddyfile`, `.env.production`, `/etc/wealthagent/secrets/`.

## 14 · Encrypt Postgres data at rest (recommended)

Account balances, holdings, and transaction history sit in plaintext inside
the Postgres data directory. `ops/encrypt-pgdata.sh` moves that volume onto a
LUKS-encrypted filesystem: it creates the encrypted container, takes a
`pg_dump` safety copy into it, migrates the existing docker volume, and adds
a `docker-compose.override.yml` that binds `pgdata` to the encrypted mount.

Two backing options:

```sh
# A) Loopback file on the root disk (works on any VM, no extra hardware)
sudo ops/encrypt-pgdata.sh --file /var/lib/wealthagent-pgdata.img --size 10G

# B) Dedicated block-storage volume (attach one in your provider console first)
sudo ops/encrypt-pgdata.sh --device /dev/disk/by-id/<your-volume>
```

Two unlock modes — pick based on your threat model:

- **Keyfile (default).** The LUKS key lives at `/etc/wealthagent/pgdata.key`,
  so the box reboots unattended. Protects a disposed, detached, or
  provider-recycled disk. Does **not** protect a full-server snapshot — the
  keyfile is inside the snapshot too.
- **`--manual-unlock`.** Passphrase only, nothing on disk; also protects
  snapshots. After every reboot you must run `sudo ops/unlock-pgdata.sh` (it
  prompts for the passphrase, mounts, and starts the stack).

Either way, back up the keyfile or passphrase in a password manager — losing
it means restoring from the B2 backups. Off-site backups are unaffected:
`ops/backup.sh` already client-side-encrypts dumps before upload. Note that
an attacker with root on the running box can read the data regardless; this
protects the data *at rest*, not a live compromise.

## Threat model notes

**The `wealth` user is in the `docker` group, which is functionally equivalent to passwordless root.** A user with shell access as `wealth` can run `docker run -v /:/host -it alpine chroot /host` and have unrestricted root on the host.

Treat the SSH key that accesses this server with root-level care:
- Passphrase-protect it: `ssh-keygen -p -f ~/.ssh/id_ed25519`
- Never copy it to a shared or loaner machine
- Rotate it if any laptop that holds it is lost or compromised

This is standard Docker-in-production posture. It is not a bug — it is a deliberate design trade-off. The mitigation is key hygiene, not removing `wealth` from the `docker` group (which would break all compose operations).
