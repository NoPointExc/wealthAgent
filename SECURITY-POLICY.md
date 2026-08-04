# WealthAgent — Information Security Policy

**Owner:** Jiayang Sun (Operator)
**Scope:** The WealthAgent application, its source code, its production
infrastructure, and all customer financial data it processes.
**Effective date:** 2026-08-04
**Last reviewed:** 2026-08-04
**Review cadence:** At least every 12 months, and after any material change to
the architecture, hosting provider, or the categories of data processed.

> **Status legend.** Statements below describe controls that are **in place
> today** unless explicitly tagged **[Planned]**, which marks a control that is
> intended but not yet fully operationalized. Planned items are tracked in the
> risk register (§3) and this document is updated as they land.

---

## 1. Purpose

WealthAgent connects to users' bank and brokerage accounts through
[Plaid](https://plaid.com) and stores balances, transactions, holdings, and
capital-gains data on the operator's behalf. This policy documents how the
organization identifies, mitigates, and monitors the information-security risks
that arise from handling that data. It is the governing document referenced when
attesting to security posture (e.g., partner and vendor security
questionnaires).

## 2. Scope and roles

- **Assets in scope:** the Rust/axum backend, React/Vite frontend, Postgres
  database, Caddy edge, Docker Compose deployment on a single hardened VM, the
  MCP/OAuth interface, source repository, container images (GHCR), and off-site
  backups (Backblaze B2).
- **Data in scope:** user identity (Google email/name), Plaid access tokens,
  account balances, transactions, investment holdings, and derived capital
  gains.
- **Roles.** WealthAgent is operated by a single owner who holds the combined
  responsibilities of **Security Officer**, **System Administrator**, and
  **Incident Responder**. This policy, the risk register, and the incident log
  are the owner's responsibility. Where a duty normally requires separation
  (e.g., independent review), that limitation is acknowledged as an accepted
  risk in §3.

## 3. Risk management

- The organization maintains a lightweight **risk register** recording known
  risks, their likelihood/impact, the mitigating control, and residual-risk
  acceptance. Documented threat-model notes already exist in `DEPLOY.md`
  ("Threat model notes") and in the README's "Privacy encryption" and "Security
  notes for self-hosters" sections; those feed the register.
- Known, accepted residual risks include: (a) the `wealth` deploy user is in the
  `docker` group and is therefore effectively root on the host — mitigated by
  SSH-key hygiene, not privilege reduction (`DEPLOY.md` §"Threat model notes");
  (b) a fully compromised live server can read plaintext regardless of at-rest
  encryption; (c) single-operator model means limited separation of duties.
- Risks are reviewed on the cadence in the header and whenever a new data type,
  vendor, or major dependency is introduced.
- **[Planned]** Formalize the register into a tracked file with periodic
  re-scoring.

## 4. Data classification and handling

| Class | Examples | Handling |
| --- | --- | --- |
| **Restricted** | Plaid access tokens, encryption keys, JWT secret, DB password | Encrypted at rest; stored only as Docker/OS secrets outside the repo; never logged |
| **Confidential** | Balances, transactions, holdings, capital gains, user email | Access gated by authenticated session, scoped per user; encrypted in transit; at-rest encryption available (§6) |
| **Internal** | Source code, deploy runbooks, Caddy config | Private repository; secrets excluded by `.gitignore` + pre-commit scanning |
| **Public** | Marketing site, Google OAuth *client ID* | May be exposed by design |

Handling rules:
- Restricted material is never committed to source control and never written to
  application logs.
- Plaid access tokens are **encrypted at rest with ChaCha20-Poly1305**; deleting
  an item calls Plaid's `/item/remove` so a leaked row cannot be replayed.
- An optional **operator-blind privacy mode** (`PRIVACY_ENCRYPTION=on`) lets
  users opt in to per-user X25519/Argon2id encryption of identifying text
  (descriptions, merchant/account names, tickers), so the operator cannot read
  those fields from the database or disk. Honest limits are documented in the
  README ("Privacy encryption").

## 5. Access control

- **Application access is via Google OAuth self-service registration.** Any user
  can sign up and sign in with a Google account; identity is asserted by Google's
  OAuth. Elevated privileges (owner/admin functions) remain restricted to a
  configured owner list. Registration is protected against abuse by rate limiting
  (§8), and per-user data is scoped so accounts can only read and modify their
  own records.
- **Sessions** use `HttpOnly; Secure; SameSite=Strict` cookies. Session JWTs are
  short-window with a hard 30-day cap.
- **Machine/agent access** to the MCP endpoint uses OAuth 2.1 (PKCE + dynamic
  client registration) with scoped, user-revocable grants, or personal API
  tokens (`wa_pat_...`) of which the database stores only a hash. OAuth access
  tokens are audience-bound and type-guarded.
- **Infrastructure access** is SSH-key only (password auth disabled by the
  bootstrap hardening). The SSH key that reaches the server carries root-level
  privilege (docker group) and is treated accordingly: passphrase-protected,
  never copied to shared machines, rotated on suspected compromise.
- **Least privilege for registry:** the server pulls images with a
  `read:packages`-only PAT, separate from the push credential.

## 6. Encryption

- **In transit:** TLS everywhere at the edge via Caddy (automatic Let's Encrypt
  certificates). HSTS is enforced with `max-age=63072000; includeSubDomains;
  preload`. Plaid and Google traffic is HTTPS-only, pinned in the Content
  Security Policy `connect-src`.
- **At rest:**
  - Plaid access tokens: ChaCha20-Poly1305 (application layer).
  - Optional per-user operator-blind encryption of identifying text
    (`PRIVACY_ENCRYPTION=on`).
  - Optional full Postgres-volume encryption via LUKS (`ops/encrypt-pgdata.sh`),
    with keyfile or manual-unlock modes selectable by threat model
    (`DEPLOY.md` §14).
  - Off-site backups are client-side-encrypted before upload (rclone crypt).
- **Key custody:** encryption keys, DB password, and JWT secret are generated
  with `openssl rand`, stored as `0400` files under a root-owned
  `/etc/wealthagent/secrets/` path the deploy user cannot read, and backed up in
  a password manager. Losing the app encryption key or backup passphrase means
  data loss — this is documented in the runbook.

## 7. Secrets management

- All secrets live in files (Docker/OS secrets), never in the repository. `.env`
  variants and keyfiles are in `.gitignore`.
- A **gitleaks pre-commit hook** (`.pre-commit-config.yaml`) scans for
  accidentally staged secrets; contributors run `pre-commit install`.
- After first deploy, secrets are relocated out of the git working tree to
  root-owned `/etc/wealthagent/secrets/` and the in-repo copies are `shred`-ed
  (`DEPLOY.md` §12).
- **Secret rotation** is performed on suspected exposure and on operator
  offboarding of any shared credential. **[Planned]** scheduled periodic
  rotation of long-lived secrets (JWT secret, DB password).

## 8. Application and network security

- **Edge hardening:** Caddy sets `X-Content-Type-Options: nosniff`,
  `X-Frame-Options: DENY`, `Referrer-Policy: strict-origin-when-cross-origin`, a
  restrictive **Content-Security-Policy**, and strips the `Server` header.
- **Isolation:** the backend and Postgres run in Docker with no host ports
  published for internal services; only Caddy is internet-facing. The VM
  bootstrap (`ops/bootstrap-vm.sh`) configures a UFW firewall and hardened SSH.
- **Rate limiting** is applied to authentication and sensitive endpoints.
- **Input/authorization:** every `/api` route requires an authenticated session
  that passes the invite allowlist; per-user data access is scoped by user id.

## 9. Change management and secure development

- Source is version-controlled in a private git repository; changes land through
  reviewed commits.
- The pre-commit gitleaks hook runs before code enters history.
- Deploys are reproducible and scripted (`ops/deploy.sh`): images are built for
  a pinned platform, pushed to GHCR, and the server pulls immutable images — it
  holds no source, Rust toolchain, or Node.js after cutover (`DEPLOY.md` §13).
- Backend has an automated test suite (`cargo test`); frontend builds are
  verified (`npm run build`) before release.
- **[Planned]** CI pipeline enforcing tests + dependency/secret scanning on
  every push.

## 10. Vulnerability and patch management

- The stack is rebuilt from current base images on redeploy, picking up upstream
  security patches for the OS and language runtimes.
- Dependencies are pinned via `Cargo.lock` / `package-lock.json` for
  reproducible builds.
- **[Planned]** Scheduled dependency-vulnerability scanning (`cargo audit`,
  `npm audit`) and a defined remediation SLA by severity.

## 11. Logging and monitoring

- The application and edge emit operational logs; Docker retains container logs
  on the host. Restricted data (tokens, keys) is excluded from logs by design.
- Backup jobs run under systemd and their success/failure is observable via
  `systemctl` status and the presence of dumps in the B2 crypt remote.
- **[Planned]** Centralized log retention and automated alerting on auth
  failures, backup failures, and resource exhaustion.

## 12. Backup and disaster recovery

- Nightly encrypted database backups to Backblaze B2 via `ops/backup.sh` and
  systemd timers (`ops/systemd/`).
- Backups are client-side-encrypted before leaving the host.
- A **restore drill** (`ops/restore-test.sh`) validates recoverability; the
  runbook sets a recurring reminder to repeat it every 3 months.
- Recovery from total VM loss: reprovision via `DEPLOY.md`, restore the latest
  B2 dump, using the encryption key/passphrase held in the operator's password
  manager.

## 13. Vendor / third-party management

The organization relies on the following sub-processors/vendors; each is
selected for its security posture and used over authenticated, encrypted
channels:

| Vendor | Purpose | Data exposed |
| --- | --- | --- |
| Plaid | Bank/brokerage connectivity | Financial account data (their platform) |
| Google | Sign-in (OAuth) | User email/identity |
| Cloud VM provider (Hetzner/DigitalOcean) | Hosting | Encrypted-at-rest data volume |
| Backblaze B2 | Off-site backups | Client-side-encrypted dumps only |
| Cloudflare | DNS | None (DNS-only, no proxy) |
| GitHub (GHCR) | Source + container registry | Source code (private) |

Vendor security is reviewed at onboarding and on the annual policy review.
**[Planned]** maintain a dated sub-processor list published for customers.

## 14. Data retention and deletion

- User-initiated account deletion removes the user's financial rows and calls
  Plaid `/item/remove` to revoke upstream access.
- Backups follow their own retention window in B2 and age out per the backup
  configuration.
- **[Planned]** documented maximum retention windows per data class and an
  automated purge for orphaned data.

## 15. Personnel security

- Single-operator model. The operator is responsible for maintaining the
  security of any device that holds the production SSH key or secrets and for
  applying key hygiene per §5.
- Any future contributor is granted least-privilege access, must install the
  pre-commit hooks, and has access revoked and relevant credentials rotated on
  offboarding.

## 16. Incident response

- **Detection:** operator monitoring of logs, backup status, and provider/vendor
  security notifications.
- **Response steps:** (1) contain — revoke affected credentials/sessions, rotate
  secrets, and if necessary take the service offline; (2) assess scope and
  affected data; (3) remediate the root cause; (4) recover from known-good
  backups; (5) notify affected users and applicable partners (including Plaid)
  where required by contract or law; (6) record the incident and lessons learned.
- An **incident log** is maintained by the operator.
- **[Planned]** written notification templates and defined notification SLAs.

## 17. Policy governance

- This policy is owned by the Security Officer and reviewed on the cadence in the
  header.
- Material changes to architecture, hosting, vendors, or data categories trigger
  an out-of-cycle review and an update to this document and the risk register.
- The revision history below records changes.

## Revision history

| Date | Change | Author |
| --- | --- | --- |
| 2026-08-04 | Initial version | Jiayang Sun |
