# 33. Developer appendix

**In this chapter:** the server commands, settings files, AWS deploys and store builds that the developer (installer) uses, in one place. The owner does not need to follow this chapter. Show it to your developer.

> **Before you start**
>
> - A server command window: Session Manager on AWS (chapter 5), browser SSH on other clouds (chapter 7).
> - Put `sudo` in front of every server command. The old name `malkkeum` runs the same command.
> - Repository docs: `docs/guide/07-운영-명령어.md`, `docs/OPERATIONS.md`, `docs/RUNBOOK.md`, `docs/DEPLOY.md`, `docs/guide/10-AWS-배포.md`, `docs/guide/11-앱-직접-빌드.md`. This chapter summarises them; where they differ, the code (`USAGE` in `deploy/install.sh`) wins.

---

## 33.1 The full `dahae` command

The install folder on the server is `/opt/malkkeum`. `sudo dahae help` prints the full list. The server prints its messages in Korean.

| Command | What it does |
|---|---|
| `dahae status` | Service status: app, HTTPS, outside uptime check, offsite backup, offsite photo backup, secret key copy checks |
| `dahae logs [api\|web\|db]` | Shows the last 200 log lines and keeps following (`Ctrl + C` to leave). Default `api` |
| `dahae version` | Current channel and version |
| `dahae backup` | Back up now (DB + photos). Ends with `✔ 백업 완료: /opt/malkkeum/backups/db-….sql.gz.enc` |
| `dahae restore file [--yes]` | Restore from an on-server backup (DB + photos from the same time). Only files inside `backups/` |
| `dahae restore-offsite [name] [--yes]` | Without a name: list offsite backups. With a name: download and restore |
| `dahae update [tag]` | Backup → new version → health check. Rolls back by itself if it does not answer |
| `dahae rollback` | Go back to the previous program version (data stays) |
| `dahae channel stable\|beta` | Switch the update channel. Updates to that channel's version straight away |
| `dahae update-schedule mon@04:00\|off` | Automatic update day and time. `off` turns automatic updates off (days: mon, tue, wed, thu, fri, sat, sun) |
| `dahae load file.tar` | Load pre-downloaded images (offline update, demos) |
| `dahae heartbeat <https url>\|off` | Set the outside uptime ping URL (for example healthchecks.io; period 5 min, grace 15 min suggested) |
| `dahae domain <new domain> [--yes]` | Move the service address: back up `.env` → set `DOMAIN` → restart api and web → check `/health` on the new address (up to 5 minutes). On failure it restores `.env` and exits with code 3. Normally done in the console (chapter 28, 28.8) |
| `dahae host-approval enroll <number>\|status` | Create the server approval code / show its status (used to approve version changes, rollbacks and domain moves; the console can create it too) |
| `dahae owner-reset` | Deletes the **server approval code** (asks for a confirmation word). It does not reset the owner's two-step sign-in |
| `dahae applinks --android-sha256 fp[,fp] --apple-team-id TEAMID` | Set up app links |
| `dahae demo load [--force]` / `demo clear` | Load / clear demo data |
| `dahae setup-code [--renew]` | The owner account setup code. Valid for 7 days after it is first shown. `--renew` makes a new one |
| `dahae keys-backup` | Makes a passphrase-locked copy (12+ characters) of `.env` as `backups/keys-….env.enc`. The escrow is recorded only when a person types the confirmation word (`보관했어요`) at the end |
| `dahae keys-restore file [--yes]` | Puts the old `DATA_KEY` and `JWT_SECRET` on a new server (before `restore-offsite`) |
| `dahae alert update\|backup\|ssh` | Reports scheduled-job failures or rising SSH blocks to the outside uptime URL (for cron) |
| `dahae uninstall [--yes]` | Stops the service. Data, `.env` and backups stay |
| `dahae uninstall --purge [--confirm domain]` | Deletes data and backups too. Confirm by typing the domain; `--yes` does not skip it. **Cannot be undone** |

**Scheduled jobs** (the installer writes `/etc/cron.d/malkkeum`):
- Daily 03:30 `dahae backup` (on failure, `dahae alert backup`).
- Automatic update, by default every Monday 04:00 (change with `update-schedule`).
- Outside uptime ping every 5 minutes (when a URL is set).
- With WAL archiving on, a full backup every Sunday at 02:15 (33.8).
- The automatic update (`dahae auto-update`) is skipped when an `update-skip` mark or a queued server job is there. On the stable channel it takes a new `stable` image only 24 hours after first seeing it.

**Resetting the owner's two-step sign-in:** when the owner has lost both the authenticator app and the recovery codes, the API program's `owner-reset` command (`api owner-reset`) prints a one-hour, one-time code. The exact command line to run it in the container, and the screen to enter the code (not in the console yet), are **to be confirmed**. The server route is `POST /v1/auth/staff/owner-reset`.

---

## 33.2 Files and settings

| What | Where | Note |
|---|---|---|
| Settings and secret keys | `/opt/malkkeum/.env` | Mode 600. Do not edit by hand |
| Backups | `/opt/malkkeum/backups/` | `db-date-time.sql.gz.enc`, `uploads-date-time.tar.gz.enc`, `keys-….env.enc`, `backup.log`. Mode 700 |
| Database | Docker volume `dbdata` (PostgreSQL 17) | No port is opened to the outside |
| Photos | Docker volume `uploads` | Photos linked to nothing are deleted after 30 days |

Main keys in `.env` (no values shown):

| Key | Meaning |
|---|---|
| `DATA_KEY` | Encrypts connection keys, payout accounts, on-server and offsite backups; signs photo links. **Changing it makes saved keys and older backups unreadable.** There is no re-encrypt command |
| `JWT_SECRET` | Signs sign-in tokens. Only the developer rotates it |
| `DB_PASSWORD`, `API_DB_PASSWORD` | Database admin and app account passwords |
| `DOMAIN` | The service domain |
| `VERSION` | The update channel tag (`stable`/`beta`). After `dahae update <tag>` it stays pinned to that tag |
| `API_IMAGE`, `WEB_IMAGE`, `PREV_API_IMAGE`, `PREV_WEB_IMAGE` | Current images and the previous ones for rollback. Do not delete the previous images |
| `SETUP_CODE`, `SETUP_CODE_AT` | The setup code and when it was first shown |
| `HEARTBEAT_URL` | The outside uptime URL |
| `KEY_ESCROW_AT` | When off-server storage of the key copy was confirmed |
| `WAL_ARCHIVE_MODE`, `WAL_S3_*`, `WAL_CIPHER_PASS` | WAL archiving (optional, 33.8) |

**Installer environment variables** (normally not needed; `DAHAE_*` works instead of `MALKKEUM_*`): `MALKKEUM_DOMAIN`, `MALKKEUM_EMAIL`, `MALKKEUM_CHANNEL`, `MALKKEUM_VERSION`, `MALKKEUM_TLS=internal` (tests only, never for an owner install), `MALKKEUM_SKIP_DNS_CHECK=1`, `MALKKEUM_HEALTH_WAIT`, `MALKKEUM_HEARTBEAT_URL`, `MALKKEUM_NOTIFY_URL` (install result ping), `MALKKEUM_SETUP_CODE`, `MALKKEUM_ANSWERS`, `MALKKEUM_IMAGES_TAR`, `MALKKEUM_FIREWALL=on`.

**Install without questions:**

```
DOMAIN=myshop.co.kr
EMAIL=owner@example.co.kr
```

Put these two lines in a plain file such as `/root/dahae-answers.txt`, then:

```
curl -fsSL INSTALL_URL | sudo env MALKKEUM_ANSWERS=/root/dahae-answers.txt bash
```

- Values cannot contain spaces, backslashes or `=`. Without questions the firewall is off by default; add `MALKKEUM_FIREWALL=on` to turn it on. SSH password login is not turned off without a question.

---

## 33.3 Backup and restore commands

**Restore from an on-server backup**

```
sudo ls /opt/malkkeum/backups
sudo dahae restore backups/db-20261110-033000.sql.gz.enc
```

- Answer `yes` to the confirmation. The current state is backed up first.
- It applies in one transaction. If the DB restore fails, data stays as it was. If the photo restore fails, the DB has already been restored.
- It ends with `✔ 복원 완료: … (사진 파일: 복원됨)` (restore done, photos restored).

**Restore from an offsite backup**

```
sudo dahae restore-offsite
sudo dahae restore-offsite db-20261110-033000.sql.gz.enc
```

**Move to a new server** (about 1 hour)

1. Install on the new server.
2. On the old server: `sudo dahae backup` and `sudo dahae keys-backup`.
3. Download the key copy to the owner's computer, then move it to `/opt/malkkeum/backups/` on the new server.
   ```
   scp keys-date.env.enc user@new-server-ip:~/
   sudo mv ~/keys-date.env.enc /opt/malkkeum/backups/
   ```
4. `sudo dahae keys-restore backups/keys-date.env.enc` → lock password → `yes`.
5. In the new server's console, fill the **Offsite backup (S3-compatible)** card again and save.
6. `sudo dahae restore-offsite` → `sudo dahae restore-offsite db-date.sql.gz.enc` → `yes`.
7. Sign in with the old owner account. Point the A record to the new IP. Check HTTPS with `sudo dahae status`.
8. When the new server works, run `sudo dahae uninstall` on the old one.

**Restore drill:** `bash scripts/ci/backup-drill.sh` (for WAL, `WAL_DRILL=1` and `PGBACKREST_*`). Run it on a test server, not the live one.

---

## 33.4 Rollback cautions

- `dahae rollback` swaps the program only. Migrations are not undone. To go back in time for data as well, use `dahae restore`.
- **Saved cards (migration 0326):** on first start the server seals saved billing keys. Rolling back past it breaks saved-card charges and key cleanup on account deletion. Roll forward, or restore the pre-update backup.
- **Toss payouts (0472, 0473):** do not roll 1.5 back to 1.4 while a payout waits on Toss. `dahae rollback` stops and prints the count if any such payout exists or it cannot check. Settle them with **[Check status]** in the console's **[Settlements]** first.
- **Toss Payments payments (0500–0502):** before rolling back to a version without Toss, turn off **Active: take payments with Toss Payments** on the **Payments (Toss Payments)** card (this stops only new Toss payments and new saved cards), and wait until no Toss payment is in progress or waiting for a deposit and no Toss refund is queued.
- When a failed update rolls back by itself, `dahae update` exits with code 3, and the console shows **Failed, so it went back to the previous version**.

---

## 33.5 Console server jobs and the server agent

The **Server update** card on the console's **[System status]** page queues a `kind=update` job with `POST /admin/host/jobs`. Every request needs the owner's authenticator code (`X-Owner-Totp`), and the button is off while an update is queued or running. The server's job runner (`deploy/agent.sh`) reads the job file and runs it. Queuing a job texts the requesting owner account's phone "서버 작업이 요청됐어요: 업데이트. …" (a server job was requested).

| Agent job kind | Console button | Note |
|---|---|---|
| `update` | Yes (**[Update now]**) | Runs `dahae update` |
| `channel`, `schedule`, `backup-now`, `update-skip`, `restart` (`all`) | Yes (**Server settings**, chapter 26) | Owner code. `channel` updates at once, like `dahae channel`. `schedule` takes `mon@04:00` or `off` |
| `heartbeat` | Yes (**Uptime heartbeat** card, chapter 17) | Owner code. An `https://` URL or `off`. The agent runs `dahae heartbeat` (restarts api, sends a test ping) |
| `domain-check` | Yes (**[Check DNS]** on the **Domain** card, chapter 28) | Admin session only. The result JSON is readable for 10 minutes at `GET /admin/host/jobs/{id}/output` |
| `domain-set` | Yes (**[Change]** on the **Domain** card, chapter 28) | Owner code plus the server approval code (`POST /admin/host/jobs/{id}/approve`). Runs `dahae domain`; on failure it restores `.env` and ends `rolled_back` |
| `host-approval-enroll` | Yes (**Server approval code** → **[Set up]**, chapter 28, 28.8.1) | Owner code. The server fills in the account id; only the requesting owner can read the authenticator link. Fails if one exists |
| `update-tag`, `rollback` | No | Need the server approval code (TOTP). Approve within 10 minutes; 5 wrong codes cancel the job; 10 wrong codes in an hour lock approvals |
| `status-refresh`, `logs`, `offsite-list` | No | The agent accepts them, but the console does not send them today |

- Job states in the console: `queued` **Waiting**, `running` **In progress**, `ok` **Done**, `failed` **Failed**, `rolled_back` **Failed, so it went back to the previous version**, `awaiting_approval` **Waiting for the approval code**, `rejected` **Refused**, anything else **Needs a check**.
- A server installed before this feature needs the install line run once more to get the new job runner and `dahae domain` (`dahae update` swaps images only). Until then `domain-set` ends `rejected` with no reason.
- Create the server approval code in the console (chapter 28, 28.8.1) or with `dahae host-approval enroll <owner account number>`; if it leaked, run `dahae owner-reset` and create it again.

---

## 33.6 AWS: manual stack, GitHub deploys, deleting

**One command (recommended):** in CloudShell (chapter 4)

```
R=https://raw.githubusercontent.com/EricWijesinghe
curl -fsSLO $R/Dahae/master/deploy/aws/launch.sh
bash launch.sh --domain example.co.kr --email owner@example.co.kr
```

Options: `--version vX.Y.Z` (default: latest release), `--stack dahae-main`, `--instance-type t3.medium`, `--hosted-zone-id Z…` (Route 53 A record), `--vpc vpc-… --subnet subnet-…` (default: the default VPC's public subnet), `--region ap-northeast-2`, `--ref master`, `--dry-run`. `--admin` creates the policies. It never touches an existing stack. It checks the release's `install.sh` against `SHA256SUMS`, then pins its hash in the template.

**By hand (B-1, B-2 in short)**

1. Do not use `dahae-aws.yaml` from the v1.5.1 release assets (it predates the permissions boundary and OIDC subject fixes). Build a new one on `master` with `scripts/release/build-aws-template.sh`. The URL must contain the tag (`/v1.5.1/`). The printed hash must match the `install.sh` line in `SHA256SUMS`.
2. CloudFormation > Create stack > Upload a template file. The stack name **must start with `dahae-`**.
3. Parameters: `Domain`, `Email`, `HostedZoneId` (empty if none), `InstanceType` (`t3.medium` default; `t4g` is ARM and replaces the server), `DiskSizeGiB` (`50`), `VpcId`, `SubnetId`, `SshCidr` (empty), `GitHubRepo` (keep `EricWijesinghe/Dahae`), `GitHubRepoIds` (keep the default), `GitHubEnvironment` (`doda-aws`), `CreateGitHubOidcProvider` (`true` if first in the account), `AmdAmiId`/`ArmAmiId` (do not change).
4. Tick the IAM resources acknowledgement → Submit → `CREATE_COMPLETE` (about 3–5 minutes). The install inside the server then takes about 15–20 more minutes.

**GitHub deploys (stage C)**

- Repository **Settings > Environments > New environment** `doda-aws`. **Deployment branches**: **Selected branches**, `master` only. Approvers are optional (**Required reviewers**).
- **Environment variables** (not Secrets): `AWS_REGION` ← `AwsRegion`, `AWS_DEPLOY_ROLE_ARN` ← `DeployRoleArn`, `AWS_INSTANCE_ID` ← `InstanceId`, `DAHAE_DOMAIN` ← `DahaeDomain` (values from the stack's **Outputs** tab).
- Flow: stable tag `vX.Y.Z` → `release` → `deploy-aws` (waits for approval if approvers are set) → `dahae update vX.Y.Z` on the server (waits up to 6 hours) → checks `/health` and the version. Beta tags are never deployed automatically.
- By hand: **Actions > deploy-aws > Run workflow** with `version`. Tick **rollback** to roll back. A rollback stops while a payout is in progress.
- Failures: **Actions > that run > server output**. If the server does not answer, use Session Manager and `sudo dahae logs`.
- The server's own Monday 04:00 automatic update is on too. For GitHub-only updates: `sudo dahae update-schedule off`.

**Costs:** depend on instance type, disk, data transfer and snapshots. Work them out in the AWS pricing calculator for Seoul (ap-northeast-2). This manual gives no prices.

**Deleting safely (D-4)**

1. `sudo dahae backup` and check the offsite backup.
2. EC2 > Instances > **Actions > Instance settings > Change termination protection**: turn it off.
3. CloudFormation > Stacks > `dahae-…` > **Delete**.
4. Left behind: the disk volume (delete it yourself), the Elastic IP (release it), the OIDC provider (shared by the account, so kept).
5. Clean up the GitHub environment `doda-aws`. If no longer needed, delete the IAM user and the policies `dahae-deployer-policy` and `dahae-boundary` (DODA admin account).

> **Warning** When you **update** the stack, cancel if the change set shows `ServerInstance` with `Replacement: True`. That change creates a new server.

---

## 33.7 Releases and channels

| Tag | Image tags (GHCR) | Delivery branch |
|---|---|---|
| `v1.2.3` | `1.2.3`, `beta`, `stable`, `latest` | `beta`, `main` |
| `v1.3.0-beta.1` | `1.3.0-beta.1`, `beta` | `beta` only |

- A server follows the channel tag in `VERSION` in `.env`. Monday 04:00 (KST): backup → pull the tag → health check → roll back to the previous images if it does not answer within 3 minutes.
- Migrations only add (expand/contract). A release never drops or renames what the previous release still reads.
- The app version floor is set in the console's **Business profile** with `min_app_version` / `latest_app_version` (chapter 10).
- The release workflow signs images with cosign (keyless); the installer checks the signature before running them.

---

## 33.8 WAL archiving (optional, engineer setup)

- Only if you need point-in-time recovery (about 5 minutes). Without it the recovery point is one day.
- Copy `deploy/wal` to `/opt/malkkeum/wal`, put `WAL_S3_BUCKET`, `WAL_S3_ENDPOINT`, `WAL_S3_KEY`, `WAL_S3_SECRET` and `WAL_CIPHER_PASS` in `.env`, then run `bash wal/wal-setup.sh`. On success, `dahae write-cron` adds the weekly base backup (Sunday 02:15).
- While it is on, every `dahae` command also loads `wal/docker-compose.wal.yml`. Never start the stock `postgres:17` image (WAL piles up on the disk).
- Keep `WAL_CIPHER_PASS` off the server, like `DATA_KEY`.
- Point-in-time restore is not a `dahae` command: `pgbackrest --stanza=malkkeum --type=time "--target=<ISO time>" restore` (empty data volume, same image).

---

## 33.9 Store app builds (summary)

The full steps are in `docs/guide/11-앱-직접-빌드.md`.

- Builds run in the developer repository's `store-apps` workflow (manual run). Android AAB on a Linux runner, iOS IPA on a macOS runner.
- The AAB goes to DODA's Google Play Console as an internal testing draft; the IPA goes to DODA's App Store Connect (TestFlight).
- Every secret lives only in the Secrets of the GitHub environment **`store`**. Keys, `.p8` files and keystores never go in the repository.
- A written licence agreement must be in place before starting (`LICENSE` §1.4, §3.1).
- The first AAB must be uploaded by hand in Play Console. After that, uploads go through the API.
- Before the build, `scripts/store_brand.py` reads the home-screen name (`short_name`; the partner app adds ` 파트너`), the icons (`store_1024`, `icon_512`) and the colour from `{API_BASE}/v1/brand`. The build stops when the server cannot be read or has no logo. Turn the workflow input `brand_from_server` off only for a build before the server is up.
- `store` environment variables: `API_BASE`, `APPLE_TEAM_ID`, `APP_LINK_DOMAIN` (after a domain move `new.host,old.host`, three at most), optional `CUSTOMER_APP_ID`, `PARTNER_APP_ID` (default `kr.co.dahae.customer`, `kr.co.dahae.partner`; decide before the first upload; same values as the app ID boxes on the console's **App links** card).
- When the build is out, the owner clicks **[I built it]** on the console's **Store app build** card to record it (chapter 28, 28.7).

| Symptom | Cause and fix |
|---|---|
| The app does not reach the server | `API_BASE` missing or wrong. Fix it and rebuild |
| Stops with `/v1/brand has no store_1024 icon` | Save a logo (or a letter logo) on the console's **Business profile**, then build again |
| iOS signing fails | One of the App ID capabilities is off |
| Play upload permission error | Check the service account has release manager rights on both apps |
| Kakao or Google sign-in fails (Android) | The Play app signing SHA-1 is missing from the Kakao or Google console |
| No push | `GoogleService-Info.plist` or the APNs key is missing |

Not yet verified: automatic certificate and profile creation, returning from card apps on iOS (PortOne web payments), the Xcode version in the first run's log.

---

## 33.10 Payment integration reference

- Key types, console order, test payments: `docs/guide/payments-setup.md`. Integration details and open items: `docs/guide/payments-reference.md`.

| Provider | Webhook path | Check |
|---|---|---|
| PortOne | `/v1/webhooks/portone` | Standard Webhooks signature (`webhook-signature`). 401 on failure; PortOne resends |
| Toss Payments (payments) | `/v1/webhooks/tosspayments` | With a security key and a `tosspayments-webhook-signature` header, HMAC-SHA256. Without the header it is taken as a hint and the payment is looked up again |
| Toss (payouts) | `/v1/webhooks/toss` | `tosspayments-webhook-signature` (HMAC-SHA256 with the security key) |

- Webhooks never change payment state directly. The server looks the payment up at the gateway again.
- Reconciliation runs every `reconcile_interval_secs` (default 60 seconds).
- **PortOne, Toss Payments, or both.** PortOne is configured with an API secret and a store id (env or console) and serves a method only when that method has a channel key. Toss Payments is configured when its card is saved and valid, and is offered while `active` is on. A card saved without the switch stores it on with live keys and off with test keys.
  - Each method goes to one gateway: PortOne if it serves the method, else Toss Payments if offered, else the method is hidden. Methods keep catalogue order; where gateways are listed, PortOne comes first.
  - New saved cards: PortOne if it can save cards (`card_billing` or the legacy `card` key), else Toss Payments with its billing key pair, else card saving is off (`POST /v1/me/payment-methods` answers 503 `provider_not_configured`). Only card is ever `flow: billing`; easy-pay methods are one-off on both gateways.
  - Rows never move: `checkouts`, `payment_methods` and `payments` keep the `gateway` they name, so refunds, webhooks, reconciliation and subscription charges follow the row. `active` off stops only new Toss checkouts and new saved cards.
  - `GET /v1/payments/methods` and `GET`/`PUT /v1/admin/payment-methods` return `gateway` per method. `GET /v1/admin/setup` returns `payment_gateways: [{code, mode}]`; the old `payment_gateway` and `payment_gateway_mode` stay for older consoles. `GET /v1/payments/methods` and `GET /v1/me/payment-methods/setup` take `?gateways=portone,tosspayments` (current apps send it); without `tosspayments` (v1.5.1 apps) Toss methods and Toss card saving are left out. `DELETE /v1/admin/setup/{portone|tosspayments}` answers 409 `gateway_in_use` while the card has pending or awaiting-deposit checkouts, live saved cards or queued refunds. A PortOne payment or billing key on a `TEST` channel records `payment_stuck` (`portone_test_channel`). There is no migration.
  - A PortOne channel key set in env without an API secret or store id is no longer listed (it could never be paid).
  - PASS reads the PortOne store id and identity channel from env or the console (`identity_enabled`); checking a result also needs the V2 API secret.
- Not yet verified: real PortOne card decline codes, the cash receipt issue response, a measured Toss webhook signature, the Toss payout creation response wrapper (`payments-setup.md` section 5).

---

## 33.11 Local development

- You need: Rust 1.99.1 (`backend/rust-toolchain.toml`), Flutter 3.47.9 (`.github/actions/flutter-setup`), PostgreSQL 17, Python 3.11 or later.

```bash
# Server: put DATABASE_URL, JWT_SECRET, DATA_KEY, PUBLIC_BASE_URL, DEV_MODE=1 in backend/.env
cd backend && cargo run -p api          # applies migrations, serves on :8080

# Apps
cd app && flutter pub get
cd apps/customer && flutter run --dart-define=API_BASE=http://localhost:8080
```

- Layout: `app/` (Flutter: customer, partner, console and `packages/`), `backend/` (Rust API), `deploy/` (installer, Docker, Caddy), `website/`, `docs/`.
- Tests and checks: `CONTRIBUTING.md`; CI runs `.github/workflows/ci.yml`. The manual screenshot tests carry the `manual_screenshots` tag and are excluded from CI. Regenerate with `scripts/manual/shots.sh` (`SHOT_ONLY=id1,id2` for some only).

---

## 33.12 More documents

| Document | Content |
|---|---|
| `docs/OPERATIONS.md` | Running the server after install |
| `docs/RUNBOOK.md` | Incident response by symptom (11 cases) |
| `docs/DEPLOY.md` | Releases, channels, stores, live service sign-ups |
| `docs/SECURITY-AND-PRIVACY.md` | Security controls and known gaps |
| `docs/ARCHITECTURE.md` | Components and data flow |
| `docs/FEATURES.md` | Features and where they live in the code |
| `docs/REBRAND-CHECKLIST.md` | What to check when changing the name and logo |
| `CHANGELOG.md` | Changes by version |

---

## If something goes wrong

> **If something goes wrong**
>
> - **`dahae` is not found** → Check that you used `sudo` and that the install finished. The old name `malkkeum` is the same command.
> - **`restore` cannot find the file** → Use a relative path starting with `backups/` (for example `backups/db-20261110-033000.sql.gz.enc`).
> - **"로컬 백업을 봉인하지 못했어요" (could not seal the local backup)** → Updates, restores and rollbacks are blocked. Find the cause with `dahae logs` and get a backup to succeed again.
> - **`rollback` stops because of payouts** → Settle them in the console's **[Settlements]** as in the Toss payouts item in 33.4, then try again.
> - **"되돌릴 이전 버전 기록이 없어요" (no previous version recorded)** → This server has never been updated.
