# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Server releases are git tags `vX.Y.Z`
(see [docs/DEPLOY.md](docs/DEPLOY.md#2-releases-and-channels)). Entries up to 1.4.0 were reconstructed
from the release branches; no tags existed for them.

## [Unreleased]

## [1.6.6] - 2026-11-28

1.6.6 prepares servers for other countries and for other release streams. A Korean server keeps working exactly as
on 1.6.5: the same amounts, tax, cancel rules, messages, exports and times (golden tests freeze them). Every database
change is additive, so a rollback to the version the server ran before keeps running on the new schema (proven with
1.6.5; see the upgrade notes for older versions).

### Added
- Update editions: a signed edition table (`deploy/editions`, baked into the delivered installer and listed in `SHA256SUMS`) names the release streams a server may follow (image repository, signer, delivery repository) and the domains allowed to switch. `dahae edition <name>` checks the new stream first (its channel version must be newer than anything this server ran, and its `SHA256SUMS` must be signed by that stream's signer at that tag), keeps the old values in `host/edition.prev` (written whole before `.env` changes), then runs the normal update. If the new version does not answer, the old values come back (exit 3), also when the old images came back under the new repository. Edition rows are checked before they reach `.env` (registry hosts with a port are accepted). A switch cut off by a reboot is settled by the next state-changing `dahae` command (update, rollback, channel, edition, domain, restore, restore-offsite, auto-update): it is kept if the new edition's images already run, otherwise the old values come back. A server that follows another edition keeps that edition's image repository, signer and delivery address when an older installer runs again (for example in a rollback).
- Console › Server: an "Update edition" card, shown only when the installer offered this server another edition. The switch needs the owner code and then the server approval code, like a rollback.
- `dahae version` and the agent's `host.json` show the edition (`edition: dahae` on every existing server).
- Console setup › Business profile: a fill colour for buttons and selected items in light mode (empty keeps the brand colour). The customer app and the console use it, with white or dark text on it, whichever reads better; the partner app keeps its own colours. The preview shows the label and its contrast.
- Country packs: the Korean rules (VAT and withholding rates, rounding, cancel defaults, consent kinds, banks, areas) now live in a built-in KR pack checked at boot; a bad pack stops the server from starting instead of running with wrong rules. Tests prove every old constant equals its pack value.
- Install facts: on first boot of 1.6.6 a server that already has staff records `install.country=KR`, currency KRW, time zone Asia/Seoul and its languages, and `install.locked_at` once it has taken a live payment. A fresh install leaves them unset. `install.locked_at` is also written at the first live payment on any server that does not have it yet.
- Install time zone: business dates, booking slots, day windows, quiet hours and the cancel window follow `install.tz`. Existing servers get `install.tz = Asia/Seoul` on first boot; a fresh install without it also uses Asia/Seoul. Asia/Seoul is the only zone offered in this release. For zones with daylight saving, a time in the overlap resolves to the later instant, the same as Postgres, so the server and the database agree on every slot.
- Money and tax types for other currencies (0, 2 and 3 decimal places) and rates, with the Korean VAT split and withholding proven equal to the old code on every total from -5,000,000 to 5,000,000 and the integer extremes.
- Phone numbers are also stored in international form (E.164, for example `+821012345678`) next to each phone. Old rows are filled in the background, in small batches; sign-in and lookups still read the old columns.
- `/ready` reports two more fields. `adapters_ok` is false when a live payment, saved card, queued refund or open payout names a payment or payout provider this version does not know; readiness then fails, and an update to such a version rolls itself back. `pack_ok` is a warning only (false after a rollback to a version with an older country pack) and never fails readiness.
- Every checkout and payment records its currency (KRW on existing rows).
- CI fails on a new hard-coded Korean time zone (`Asia/Seoul`, `+09:00`, `KST`) outside the allowed places.

### Changed
- Documentation is English first: README, guides, install page and store notes, with the Korean texts kept as linked translations.
- The job runner asks for CLI revision 3.
- Finance (summary, series, services, monthly and VAT exports): VAT is taxed at the rate in force on each fee day and labelled from the rates used. Korean numbers and labels are unchanged; finance refuses a day with no VAT rate in force. Payout withholding reads the pack rule.
- Rounding works in wider integers and stops at the limit instead of wrapping on huge amounts; Korean rounding is unchanged.
- A pre-release tag (`v1.6.6-beta.1`) without its own changelog section takes its release notes from the release it leads to (`[1.6.6]`).
- The manual checksum check in the delivery README and the server guide uses `sha256sum -c --ignore-missing SHA256SUMS`, because `SHA256SUMS` also lists `agent.sh` and `editions`.

### Fixed
- First install waits until the database really exists before preparing accounts, and update health waits use clock time, so a slow server no longer times out early.
- The home page carries the saved Google Search Console and Naver Search Advisor verification tags, and the Naver verification file is served as plain text.
- Sample data no longer uses the old brand in a team name.

### Security
- Payment webhooks: each PortOne, Toss Payments and Toss payout delivery is recorded before it is handled, so a repeated delivery is answered and changes nothing. An event counts as done only after its work finishes; a delivery cut off mid-work, or one that overlaps a running attempt, is retried instead of acknowledged. Unsigned Toss Payments hints skip the record. Records older than 30 days are removed.
- Payment confirmation and saved-card charges check the currency against the provider and the install and refuse a mismatch.

### Upgrade notes
- Nothing changes by itself: a server without `EDITION` in `.env` keeps its `.env` byte for byte (custom signer and image overrides and local-image installs included) and offers no edition. Only the domains listed in the table get `EDITIONS_OFFERED`.
- Migrations 0503-0511 are additive (new tables, nullable or defaulted columns, partial unique indexes on the new columns only, triggers that touch only the new columns, a new SQL function). Older versions ignore all of them, so "Roll back to the previous version" keeps working. 0508-0510 build their indexes without locking writes.
- A rollback goes to the version the server ran before this update, not always 1.6.5: a server with automatic updates off or skipped may go back to 1.6.2, 1.6.3 or 1.6.4. Only 1.6.5 was started on the new schema (below). 1.6.2-1.6.4 ship the same migrations (last 0502), the same migration runner (missing ones ignored) and the same readiness check as 1.6.5, so they are expected to behave the same, but this was not run. Servers older than 1.6.2 are not covered: update them to 1.6.5 before 1.6.6.
- The phone backfill runs in the background at every start and fills only empty copies; the server is ready before it finishes. Numbers that already belong to another row stay empty and are logged.
- Checked before release: a 1.6.5 server started on a database that 1.6.6 had migrated and used, applied nothing, answered `/ready`, and passed the 1.6.5 smoke test (sign-up, coupons, bookings, saved cards, checkout and payment, quotes, chat, settlement, cancellation); its new rows got `KRW` and empty phone copies. 1.6.6 then started again on the same database, filled those copies and passed its own smoke test. Test saved cards were cleared between runs, because the smoke tests use fixed test card keys.

| Migration | What it adds | Why 1.6.5 keeps working |
|---|---|---|
| 0503 | table `webhook_events` (provider deliveries already handled) | 1.6.5 never reads or writes it |
| 0504, 0505 | `currency` on `checkouts` and `payments`, `NOT NULL DEFAULT 'KRW'`, format check added `NOT VALID` then validated | rows 1.6.5 inserts get `KRW`, which passes the check |
| 0506 | nullable `webhook_events.done_at` | table unused by 1.6.5 |
| 0507 | nullable `phone_e164` copies on users, pros, team members, OTP codes, referrals, notification outbox and staff; triggers that clear a copy when the phone changes without it; partial unique indexes on staff and referrals copies | 1.6.5 leaves the copies NULL, and the unique indexes skip NULL; the triggers write only the copy columns |
| 0508-0510 | partial unique indexes on the users, pros and team members copies, built `CONCURRENTLY` | the same: NULL copies are not indexed |
| 0511 | SQL function `dahae_tz()` (Asia/Seoul when the connection names no zone) | 1.6.5 never calls it |

1.6.5 runs its migrations with missing ones ignored, so the newer rows in the migration table do not stop it.

### 한국어 요약
- 한국 서버는 1.6.5 와 똑같이 작동해요: 금액, 세금, 취소 규칙, 문자, CSV 내보내기, 시간이 그대로예요. 이번 버전은 다른 나라 서비스와 여러 업데이트 판(에디션)을 준비하는 바탕이에요.
- 사장님 화면에서 바뀌는 것: 콘솔 설정 › 사업장 정보 › 채움 색상에서 버튼과 선택 항목의 색을 고를 수 있어요 (비우면 대표 색상). 고객 앱과 콘솔에 쓰이고, 파트너 앱은 지금 색 그대로예요. 서버 화면의 "업데이트 판 (Edition)" 카드는 판 표에 도메인이 적힌 서버에서만 보이고, 다른 서버에는 보이지 않아요.
- 결제 알림(웹훅)은 받은 기록을 먼저 남기고 처리해서, 같은 알림이 다시 와도 두 번 처리하지 않아요. 결제 통화가 맞지 않으면 승인하지 않아요.
- 전화번호를 국제 형식(+82…)으로도 함께 저장해요. 비어 있는 번호는 서버가 켜질 때마다 뒤에서 조금씩 채워요. 로그인은 지금처럼 기존 번호로 해요.
- 데이터베이스 변경(0503-0511)은 모두 추가만 해요. "이전 버전으로 되돌리기"는 이 서버가 바로 전에 쓰던 버전으로 돌아가고, 1.6.5 로 되돌려도 문제없이 작동해요. 1.6.6 이 바꾼 DB 위에서 1.6.5 서버를 켜고 1.6.5 의 전체 점검을 돌려 확인했어요.
- 서버 상태 점검(`/ready`)에 두 칸이 늘었어요. `adapters_ok` 는 이 버전이 모르는 결제사를 쓰는 결제가 남아 있으면 `false` 가 되고, 그러면 업데이트가 스스로 되돌아가요. `pack_ok` 는 알림용이라 서비스를 멈추지 않아요.
- 저절로 바뀌는 설정은 없어요. 판(에디션)을 바꾸지 않은 서버의 `.env` 는 한 글자도 바뀌지 않아요.

## [1.6.5] - 2026-11-24

### Added
- Updates check free disk space first and stop, with nothing changed, when the disk is too full.
- CI refuses a migration that drops or renames something unless it carries a contract marker.

### Security
- Payments: a PortOne payment can back only its own checkout. A payment made with another checkout's id is refused, and a payment already recorded or held by another checkout is never refunded automatically; a staff case is opened instead.
- Staff: only the partner assigned to a booking can read its door code, on the visit day; the staff route is removed. Re-issuing an admin invite or switching an admin back on needs the owner code. Only admins can invite staff. Setting the tax type and registering the Toss seller ask for a fresh second code.
- Privacy: deleting a partner clears their photo, bio, emergency contact and verification checklist and removes their uploads; their payee number can no longer be shown. Closed bookings drop the team memo with the other personal fields.
- Server: the job runner moves an approval file into its own folder before reading it and reads job files with a time limit, so a stuck file can no longer hold the server's update and backup lock. Private API answers are sent with `Cache-Control: no-store`. The database no longer logs statement text.

### Changed
- Updates: a build that was rolled back automatically is held and not retried every week, and a server pinned to a version rejoins its channel once the channel reaches it. Release images are built without a cache.
- Website: Korean lines break between words, and whole-home cleaning has a description.

### Before going live
- Owners who kept an AAAA (IPv6) record, as older manual pages said, should delete it now on every name that points at the server (the domain, `www.`, and an old domain that still answers), even if it points at this server. IPv6 visitors are not served yet: they all arrive from Docker's gateway address and share one sign-in limit. No check warns about a record that was already there (manual chapter 5, 5.3).
- The database container's settings changed, so the first `dahae update` after this release restarts Postgres once (a short pause while the update runs). Later updates do not.

## [1.6.4] - 2026-11-24

The 1.6.3 tag was never published: a test script failed the release check. 1.6.4 is 1.6.3 with that test fixed; everything below ships in 1.6.4.

### Added
- Website: an unknown address shows the website's own "page not found" page (sent as 404) instead of the home page.
- Website: `/favicon.ico`, `/apple-touch-icon.png` and `/apple-touch-icon-precomposed.png` point to the brand icon, so browsers and phones that ask for them directly get it.

### Changed
- Customer app: the design's line icons (bottom bar, menu, booking choices, air-conditioner types, chat, coupons, news) are painted in the theme's ink colour, so they show on the light screen and in dark mode instead of white on white; on a selected choice the icon takes the same colour as its label. The partner app keeps its own colour tab icons.
- Customer app: a service without a photo shows its own art on its colour instead of a grey "photo goes here" box, and the 기본 서비스 band appears only when it has something to list. On a narrow phone the 전체 / PICK toggle shrinks its text to fit, and the logo drops its name when even that is not enough. The guest line in More is shown at full size under its label, and long English group names get a third line.
- Apps: Korean text wraps at the spaces between words instead of splitting a word across lines.
- Console: in English the title leads with the brand's English name; a brand with no English name goes after the title ("Operations console · 다해").
- Website: the service grid shows every service with its name, art and "from" price, including whole-home, window and disinfection cleaning and per-item (counts) prices; a service added later gets a generic name and picture instead of its code, and prices line up at the bottom of each card.
- Legal pages and the website footer show "준비 중" for a business detail the owner has not entered yet (for example "통신판매업 신고 준비 중") instead of a bracketed placeholder such as `[상호]`.
- Server: the API and web containers may hold 65,536 open files (Docker's default 1,024 would stall at about 1,000 connected apps), and the web container's memory cap is 384 MB (was 256 MB). The caps add up to 1,664 MB, which still fits a 2 GB server.
- Server backups: after a backup whose photo archive reached the offsite storage, local photo archives older than 3 days are removed; otherwise they are kept 14 days. Database dumps are still kept 14 days. This stops photo archives filling a 50 GB disk. When a restore finds no local photo archive it points to `dahae restore-offsite`.

### Fixed
- Website: error pages carry the same security headers as the rest of the site, and the website's own test files are not served.
- A server with demo data can still create its owner in setup: the inactive sample staff that `demo load` adds no longer count as an owner. A real owner, even a deactivated one, still closes setup.
- The apps show AI 상담 only while the owner's AI switch is on and an AI key is saved, so a customer never opens a chat that cannot answer. The console still shows the owner's own switch.
- A guest (not signed in) no longer asks the server for consent records, which answered 401; analytics stay off for guests.
- Server jobs from the console were sometimes refused as "from the future" when the job runner picked them up in the same second they were queued; the "kept off the server" confirmation of the key copy was the one most often hit. Job times are now compared to the millisecond, with 2 seconds of leeway.
- The server's compose file is written to a new file and then renamed, so a full disk can no longer leave a half-written one.

### Before going live
- Servers get the new container limits, photo retention and job-runner fix with the next `dahae update`, which also refreshes the host tools. Servers that load images themselves (no delivery address) run the install line once more instead.

## [1.6.3] - 2026-11-24

Tagged but not published (see 1.6.4).

## [1.6.2] - 2026-11-23

### Added
- First console sign-in now asks the owner to type one recovery code back, confirming the codes were saved before they are needed.

### Fixed
- The web apps can read a picked image (the app CSP allows `blob:`), so the console logo/photo upload and the customer chat and growth photos work; a logo the console cannot read shows a message instead of silently keeping the old one.
- Finance and privacy CSV exports ask for the authenticator code instead of failing; a step-up code sent after the access token expired renews the sign-in and goes through.
- A second console tab on one login no longer signs both tabs out.
- An address can be typed when address search fails or a road name has no district; several console messages, colour swatches, empty states and confirmations are clearer; a server job the host agent never picks up says so after 3 minutes.

## [1.6.1] - 2026-11-23

### Fixed
- Customer and partner apps: a saved server address (found after a domain move) that stops answering for this install at cold starts at least 24 hours apart, while the build's own address answers with the same `install_id`, is dropped, and the app starts on the build's address; offline, or when the build's address answers with a parking page or another install, the saved address stays. Both addresses are probed at once (one 4-second wait at most), only a JSON `/v1/config` with the install's id counts as an answer, and a move back to the build's own address clears the saved value.
- Windows apps: the window title and the exe's ProductName, FileDescription and CompanyName come from the brand at build time (`scripts/store_brand.py`, also run by `build-msix.ps1 -ApiBase`); 다해 is only the default. The title is written as `\u` escapes, so it no longer depends on the build machine's code page.
- Money: disconnecting a payment card is also refused while that gateway has checkouts that expired within the last day (a late payment still settles on them, `gateway_in_use`) or payments from the last year that can still be refunded (`gateway_has_refundable_payments`, new console text); before, clearing the card made those refunds and late payments fail.
- A PortOne payment or saved card on a TEST channel raises its own owner alert (rule `payment_test_channel`, "PortOne 테스트 채널로 결제가 들어왔어요", console label in Korean and English) and its own status check `test_channel`, instead of the stuck-payment alert and the `payments` check.
- Store builds: the partner iOS app takes its own Google iOS client id (new secret `PARTNER_GOOGLE_IOS_CLIENT_ID`) instead of the customer app's, which belongs to another bundle id and made its Google button fail; a partner build without Google sign-in warns in the build log, since App Review signs in to the partner app with Google.
- Money: a PortOne payment or saved card on a TEST channel whose key this server does not hold (channel keys are public, so any client can pick one) reads as failed and books nothing, and the saved card is refused (`portone_test_channel`); before, it was booked as paid with no money moved. The test-channel alert now says both causes, that testing with test keys is fine, and to delete the store's test channels in the PortOne console.
- Docs: OPERATIONS lists all 18 owner alert rules; the payments guide explains the test-channel alert.
- Money: a payment card in use cannot be re-saved onto another account (`gateway_account_changed`): another PortOne store, or a Toss secret key that cannot read that MID's newest payment. A new key of the same account still saves. Disconnecting PortOne while the server's env also holds PortOne keys is guarded unless the env names the same store, an open subscription charge blocks a disconnect, and demo payments no longer do.
- Docs and CI: the rebrand checklist no longer says the Windows window title and exe info need hand edits; `check-native.py` fails when the partner app could take the customer's iOS Google client id; the store review notes say both OSes need `GOOGLE_SERVER_CLIENT_ID`.
### Added
- `dahae update <tag>` (and channel updates) also replaces the host tools (CLI, job agent, systemd units, cron, compose) with that release's install.sh from the delivery repo, used only when it matches the release's cosign-signed SHA256SUMS; any mismatch stops the update before the backup. `dahae rollback` puts the previous version's tools back too and keeps the payout double-pay guard (now exit code 4 with its reason in the console).
- AWS template: a private S3 bucket for offsite backups (block public access, versioning, SSE-S3, 35-day lifecycle, kept on stack delete); the server role may only put, get and list in it, and backups and photo backups go there with no keys to type. The console backup card shows "AWS S3 (automatic)".
- The API signs offsite backups with the EC2 instance role (IMDSv2 token, session token signed, credentials refreshed before expiry) when no backup keys are saved and `BACKUP_S3_BUCKET` is set; the template's metadata hop limit is 2 so the container reaches it.
- AWS template: the system-check alarm e-mails the stack's Email through SNS (one "Confirm subscription" click), next to the auto-recover action. No disk alarm: the server has no CloudWatch agent.
- Console recovery card (System status > Server update): one-click rollback to the previous version (owner code, then the server approval code) and a secret key copy download (owner code; one download, recorded as kept off the server once the owner confirms it).
- `deploy/aws/launch.sh` runs from Git Bash on Windows: the AWS CLI gets Windows file paths and reads the Korean template as UTF-8. `--admin` now updates policies that already exist.
- Host tools: the rollback copy of the installer moves only when the images really change, so a weekly auto-update with no new release no longer makes the next rollback re-run the current tools, and an install-line re-run keeps the record one version back.
- Host tools: the installer and agent.sh are verified against the SHA256SUMS signed at exactly that tag; a channel is read as the tag it points at (its VERSION) and refused when older than the newest version the server ran. An installer without tools mode is never run as a refresh.
- Host tools: agent.sh is fetched and verified with the installer and kept beside it, so a rollback does not download it again (a moved `beta` branch no longer breaks it); in tools mode the agent is checked before anything is written.
- A tools refresh or tools rollback that fails after the app answers now ends with exit code 5 and a reason in the console instead of a bare failure or a false "ok"; a rollback with no previous version ends with exit code 6 and says so; other rollback failures point at `dahae logs`.
- After a rollback the weekly auto-update holds the rolled-back version until the channel moves to a newer one (`host/rollback-hold`); a manual update ends the hold.
- The console's key copy download no longer records the copy as kept: the owner confirms "Kept off the server" after the file was saved (`POST /v1/admin/host/jobs/{id}/kept`), and the one-time download mark and its audit entry commit together.
- `dahae keys-backup` seals only DATA_KEY and JWT_SECRET (what keys-restore reads) instead of the whole `.env` with the database superuser password.
- The idempotency record stores a DATA_KEY HMAC of the request body instead of a plain SHA-256, so the key copy passphrase cannot be guessed from the database or a dump.
- AWS backup bucket: the server role may only create objects (PutObject requires `If-None-Match: *`, which the API sends on role uploads), so a taken-over server cannot overwrite or delete existing copies.
- AWS template: `ExistingBackupBucket` (`launch.sh --backup-bucket`) lets a replacement stack read and keep using the retained bucket of a lost server.
- A role refusal (401/403) on the automatic AWS bucket reports `no_instance_role` instead of "wrong key".
- Console recovery card: the rollback approval shows the target version; closing the approval dialog, a job still running after the wait, and a job old server tools do not know each get their own message.
- `launch.sh` ends with the alarm e-mail confirmation step.

## [1.6.0] - 2026-11-22

### Security
- `url` 2.5.9 (RUSTSEC-2026-0592: stack overflow in `Url::origin()` on nested `blob:` URLs). Image builds always take the latest OS security updates (OpenSSL 3.5.9-r1 in the web image, glibc deb13u5 in the API image) instead of a cached upgrade layer.

### Added
- Domain move from the console. New server jobs `domain-check` (admin) and `domain-set` (owner code, then the server approval code), `heartbeat` and `host-approval-enroll` (owner; the server fills in the caller's id); `POST /v1/admin/host/jobs/{id}/approve` hands the approval code to the server, and `GET /v1/admin/host/jobs/{id}/output` returns a job's result (DNS check, enrolment address; capped at 64 KB). The new `dahae domain <new> [--yes]` backs up `.env`, sets `DOMAIN`, restarts api and web and checks `https://<new>/health` for up to 5 minutes; when the new address does not answer it restores `.env` and ends as rolled back (exit 3). Jobs the server cannot hold for approval now say why in their step text.
- Former domains keep serving: the API records the old domain as an alias when it starts on a new one, adds it to CORS, and answers Caddy's on-demand TLS question (`/internal/tls-ask`, never proxied). On a former domain every API, payment, app-link, sign-in, app and SEO path is served as before; only website pages get a 301 to the new domain. `GET /v1/admin/domains` lists the domains with the after-move checklist (values to copy), `PUT /v1/admin/domains/checklist/{id}` ticks an item and `DELETE /v1/admin/domains/{host}` (owner code) retires a former domain.
- App links take the store app ids: `android_packages` and `ios_bundle_ids` in `/v1/admin/app-links`. Empty lists serve the same files as before.
- Store builds read the home-screen name, launcher icons and colour from `{API_BASE}/v1/brand` (`scripts/store_brand.py`) and the app ids from the new variables `CUSTOMER_APP_ID` / `PARTNER_APP_ID`; `APP_LINK_DOMAIN` may list up to three hosts. The build stops when the server cannot be read (workflow input `brand_from_server` off skips it).
- Caddy: the web app manifests (`/site.webmanifest`, `/app/manifest.json`, `/admin/manifest.json`) come from the API, and the two app shells' `index.html` is templated (server-side includes), nothing else in the Flutter output.
- One-command AWS EC2 launch from AWS CloudShell: `deploy/aws/launch.sh` checks the release installer against its SHA256SUMS, pins it into the CloudFormation template, finds the default VPC's public subnet and creates stack `dahae-main`; `--admin` creates the two IAM policies for DODA's admin. Guide 10 starts with it.
- No domain yet: `launch.sh --domain auto` (template `Domain=auto`) opens the server on `<elastic ip>.sslip.io` with a real certificate; the owner moves to the real domain later in the console.
- Toss Payments as a payment gateway. PortOne and Toss Payments are both optional: an install may set up either one, or both, from the console, and customers (app, web, saved cards, subscriptions) are offered the methods of every gateway set up; with neither, payment stays unavailable. Each method goes to one gateway: PortOne when it has an API secret, a store id and that method's channel key, otherwise Toss Payments while its card is saved and its switch 사용: 토스페이먼츠로 결제 받기 is on (a card saved without touching it is stored on with live keys and off with test keys). New saved cards go to PortOne when it can save cards (`card_billing` or the `card` key), otherwise to Toss Payments when it has its 자동결제 pair. Off stops only new Toss payments and card saves. Only card is a saved-card tile (while a gateway saves cards); KakaoPay, NaverPay, Toss Pay and PAYCO are one-off payments on either gateway, and the web and Windows apps pay card in the window. Payments, refunds, saved cards and subscriptions always stay on the gateway their row names, and identity checks (PASS) stay on PortOne. The new console card 결제 (토스페이먼츠) takes the API 개별 연동 keys of the payment-window MID, an optional 자동결제 MID pair and an optional 보안 키; it has a test/live badge, the webhook address `/v1/webhooks/tosspayments` to copy, and an API key check per MID. The first save needs the owner code, like the PortOne card. The payments heading lists every gateway set up (결제를 받는 곳: …), PortOne first, with a test-keys mark per gateway; the two payment cards count as one setup step; Setup > Payment methods shows each method's gateway (card: where it is saved). A saved PortOne channel key can be cleared on its own (이 칸 지우기). A payment card with open checkouts, live saved cards or queued refunds cannot be disconnected (`gateway_in_use`). A PortOne payment or saved card on a TEST channel raises the stuck-payment alert (`portone_test_channel`). Setup steps: [payments setup §6](docs/guide/payments-setup.md#6-토스페이먼츠로-결제받기-단독-또는-portone과-함께).
- API: public `POST /v1/checkouts/{id}/toss-confirm` (called by the Toss success page), `POST /v1/webhooks/tosspayments`, `gateway` in the checkout intent, in `GET /v1/me/payment-methods/setup`, on each entry of `GET /v1/payments/methods` and `GET`/`PUT /v1/admin/payment-methods`, and `toss_payments_webhook_url` / `payment_gateways` in `GET /v1/admin/setup` (`payment_gateway` / `payment_gateway_mode` stay for older consoles). PortOne responses are unchanged apart from the added `gateway`. No migration. A method whose PortOne channel key comes from env without an API secret or store id is no longer listed; it could never be paid. `GET /v1/payments/methods` and `GET /v1/me/payment-methods/setup` take `?gateways=portone,tosspayments` (sent by current apps); without it Toss methods and Toss card saving are left out. Easy-pay methods now report `flow: checkout`. Saving a card when no gateway saves cards answers 503 `provider_not_configured` instead of trying PortOne.
- Privacy policy: 토스페이먼츠 is listed as a payment processor.

### Before going live
- Servers installed before this release: run the install line once more (same domain; browser SSH or AWS SSM Run Command) so the server gets the new job runner and `dahae domain`. Until then a domain move from the console ends as rejected. Then create the server approval code from the console.
- After a domain move, raise `min_app_version` only once store builds with the new `API_BASE` are live, and retire the old domain only after that. Naver's handling of several callback URLs and a PG's re-review of a changed site URL are not verified.
- Toss Payments: sign the 전자결제 contract (and 자동결제 for saved cards), then follow the console order in [payments setup §6](docs/guide/payments-setup.md#콘솔-순서): on a server that already takes payments switch 사용 off before the first save (test keys are stored off when it is left untouched, live keys on; with it on, every switched-on method without a PortOne channel key is offered through Toss at once), save test keys, leave on only the methods Toss approved, switch on and run the test-key checklist on Android, iOS and the web, refund the test payments, then replace all four keys with live keys and save with 사용 on.
- Older store apps (v1.5.1) open PortOne pages only, so the server offers them PortOne methods and PortOne card saving only. For store users to get the Toss methods, publish the new apps and raise the customer app's Minimum version.
- Servers that ran a pre-release build with the Toss Payments card: a card saved without touching 사용 is now on if it holds live keys (test keys: off). With 사용 on, methods that still have a PortOne channel key, and new saved cards while PortOne has a `card`/`card_billing` key, go back to PortOne. Right after updating check Setup › Payment methods and the Toss card: turn 사용 off, or clear those PortOne keys (이 칸 지우기).

### Changed
- Store app builds run on GitHub Actions (`store-apps`, manual: Android AAB on Linux, iOS IPA on macOS) instead of Codemagic; `codemagic.yaml` is removed. Setup: [guide 11](docs/guide/11-앱-직접-빌드.md).

## [1.5.1] - 2026-11-17

First published 1.5 release. The `v1.5.0` tag on the same code was never released (the release run stopped on a shell lint warning, fixed here); it contains everything below.

### Before going live
- Store app IDs are now `kr.co.dahae.customer`, `kr.co.dahae.partner`, and `kr.co.dahae.console` (were `com.malkkeum.*`), and the Naver iOS legacy callback scheme is `dahaenaver` (was `malkkeumnaver`). Set before the first store upload: the store app ID cannot change afterwards. DODA's Firebase apps must be registered with the new IDs.
- In the Toss sandbox, check that an EXPRESS payout comes back with `payoutDate` set to the day it was requested, and that Toss refuses a reused `refPayoutId` (the retry after a failed transfer relies on it).
- In the Toss sandbox, check the error Toss returns for a reused `refPayoutId` and for an `Idempotency-Key` replayed with a different body (neither code is in the docs; both are kept as "maybe sent", never as a decline, after a send without an answer), and whether `GET /v2/payouts/{id}` answers inside `entityBody` (both shapes are read). Also send one payout to a sandbox seller: the create body is now a list, as in the docs.
- After the first customer build that sends consents is published, set the customer `min_app_version` in the console business profile to that build.
- Ship the 1.5.1 partner app with this server and then set the partner `min_app_version` to 1.5.1: older partner builds do not send the reschedule request they showed, so the server answers their reschedule confirm with `reschedule_changed` until the pro updates.
- PortOne and Toss live keys are entered by the owner in the console setup screen (they are never shipped in the release).

### Added
- AWS deployment kit: `deploy/aws/dahae.cfn.yaml` launches one Seoul server from the AWS console (Elastic IP, encrypted disk, IMDSv2, daily snapshots kept 7, auto-recover alarm, GitHub deploy role), `deploy/aws/deployer-iam-policy.json` is the least-privilege policy for the client's deployer user, and guide 10 walks a non-specialist through it. Each release attaches `dahae-aws.yaml` pinned to its `install.sh`.
- Automatic AWS deploys: `.github/workflows/deploy-aws.yml` runs `dahae update <tag>` (or `dahae rollback`) on the client's server through AWS SSM after the stable release, approved in the `doda-aws` GitHub environment, then checks health and version. It uses OIDC (no AWS keys) and skips cleanly until the environment's variables are set.
- Easy cloud install (no terminal): the install page makes the setup code in the browser and shows it on screen before the server is created; it offers a `#cloud-config` form, a `#!/bin/bash` form for Lightsail, Google Compute Engine and Naver Cloud, and a one-line browser SSH form for providers with no paste field. The page also asks for Ubuntu 24.04 and the firewall ports before it makes the file. Installs can report success or failure to an optional ping URL (`MALKKEUM_NOTIFY_URL`). The page is published as `dahae-install-page.html` in the delivery repo.
- Console "지금 업데이트" (update now) button on the ops screen for the owner, with the owner code for each press; the recent server jobs and their state are listed under it.
- An expired setup code (right code, past its seven days) now says so in the console and asks the owner to ask the developer or installer for a new one (HTTP 410 `setup_code_expired`). A wrong code still says it is not right.
- Setup codes last seven days (was two hours), so the owner can finish DNS and signups first.
- Guide 9, 클라우드별 설치 (Install by cloud provider): paste field, firewall, static IP and click path per provider. Provider screen names not yet checked on a live console are marked 확인 필요 (to be confirmed).
- Follow-up, not done yet: `dahae domain` to change the domain, the console keys backup download (a developer still runs `keys-backup`), and a console roll back button (the API has no rollback job or approval step yet).
- Owner copyright notice: every app shows "다해 소프트웨어 © 2026 Eric Wijesinghe. All rights reserved." on its open-source licenses page (partner and console get the entry, console also shows the line under the menu), the Dahae license is in that page, the website footers credit the software, and the Windows and macOS copyright fields name the owner. Owner setting: none; the text follows NOTICE and is checked in `scripts/ci/check-native.py`.
- Cleaner verification: a pro can be approved as active only when the console checklist is complete (ID, tax type, experience, emergency contact), recorded with the audit row; the console person screen has an employee profile (photo, bio, employment type, encrypted emergency contact, hire date). The emergency contact is encrypted at rest and every view is logged. Not yet included: the partner app does not show the verification status or checklist.
- Owner account (내 계정) for the console: one owner per server, claimed or handed over with both authenticator codes; owner-only actions need a fresh code each time; recovery codes re-issue, SMS alert number, password change, sign out everywhere; server-issued one-time reset (`api owner-reset`) after a lost second factor.
- Home categories are server data: `service_groups` (moving, living, appliance, disinfection) with `services.group_code`. `GET /v1/service-groups` returns them in order with a derived HOT/N badge; the owner edits title, order, icon, members and chooser config through `PUT /v1/admin/service-groups/{code}` (audited). Quote answers carry the chosen `home_type` (원룸·투룸, 아파트, 빌라·주택), validated on input.
- Customer app counts services: 가전·가구 cleaning (aircon, sofa, mattress) and window cleaning pick an amount per option with −/+ steppers, and disinfection (방역·소독) picks each treatment once; the total and duration update live, two or more items get the multi-item discount, and checkout, booking detail and the booking confirmation list the counts (for example "벽걸이 1 · 스탠드 2").
- Customer app: move-in / move-out starts with the new-build or existing-build question and a home-size question (원룸·투룸, 아파트, 빌라·주택, each marked 예상 견적); the chosen home carries into the quote, whole-home asks the home first, the 견적 방식 page is folded into the review (six steps), the floor question appears only without an elevator, and required-question errors show only after "다음".
- Customer app: theme choice is a sheet with light, dark and system options and a check on the current one; phone numbers show as 010-1234-5678 through one shared formatter; notice rows and the notice detail show the posting date (server sends `created_at` on `/v1/content`).
- Customer app: the PICK tab shows popular services and the newest reviews, reviews have a full list (후기 전체 보기) with star icons and report in an overflow menu (`GET /v1/public/reviews`); the web splash goes once the home has data and never blocks taps, and the web build preloads the Korean font so date pickers show no boxes.
- Service scope (서비스 범위) covers whole-home, window and pest-control/disinfection services; new 3D icons for the three services; SEO keyword pages for whole-home, window and disinfection.
- Local database and photo backups are sealed with the offsite format (MKB1, AES-256-GCM from DATA_KEY) and no plaintext dump is left on the server disk once sealing succeeds (on a seal failure the plaintext is kept, the backup reports an error, and update, rollback and restore stop); restore opens them and removes the opened copies afterwards. Backup file writes are atomic (`.part` then rename). `dahae keys-backup` asks the owner to type a confirmation in a terminal that the sealed key copy is stored off the server (recorded as `KEY_ESCROW_AT`; `--yes` never records it). The console checkbox for this is not built yet. Safe default: the console has no live-payment switch yet, so the offsite gate before live payments is not enforced.
- Owner can queue server jobs (backup, update, update-skip, channel, restart, schedule) from the API: each accepted job is one file in the server job inbox with owner TOTP on the request and a double click queues one job; live state (queued, running, ok, failed) is read back from the server's status files as capped text (`/admin/host/jobs`).
- Renewal calendar (갱신 일정) in the console: domains, old-domain aliases, hosting, store and SMS accounts, storage, and the yearly keys check. The owner gets an SMS 30 and 7 days before each date; the keys check follows the last successful upload test.
- Server job runner (`deploy/agent.sh`, root) is installed and started by the installer on new installs and updates (systemd path, service and timer units). It runs schedule, update, update-skip, channel, restart and status jobs; a stable update waits 24 hours after a new digest is first seen; live progress is served read-only at `/ops-progress/<token>` for 10 minutes after a job ends; `dahae setup-code` expires after 2 hours (`--renew` gives a new code); `MALKKEUM_ANSWERS` installs without questions.
- Owner activity log (활동 기록): newest first, filter by person, kind and day; owner only (`GET /v1/admin/audit`).
- Keys bundle: the owner downloads DATA_KEY and JWT_SECRET sealed under a server-generated passphrase, and proves
  it opens with an upload test (owner TOTP on each request); `api keys-open` prints the two keys for restores.
- Maintenance mode is set in the owner's rules (점검 모드, message included) and read by the apps from `/config`. While it is on, the apps show a banner, customers cannot submit a new booking, and the API refuses new bookings only: direct and quote bookings, waitlist accepts, subscriptions and new booking checkouts. Cancellations and payments on existing bookings stay open. If the API is down, the website shows a static maintenance page instead of a bare 502.
- Owner settings registry: business limits, marketing cap, points cap, quality window and ops alert thresholds are read from the database (env is the seed), validated, and audited on every change.
- Owner console: app link values (Android signing fingerprints, Apple Team ID) can be saved and checked under the setup screen's "앱 링크" card. Saved values override the `ANDROID_CERT_SHA256` and `APPLE_TEAM_ID` server settings, which remain the fallback.
- Console "알림 문구" screen: edit the push and inbox text of each customer and cleaning-team notification in Korean and English, with a preview and restore to the built-in text. Only the placeholders listed for each notification (booking number, business name) can be used.
- Console: 탈퇴·개인정보 요청 screen. Staff find a customer by phone, download the customer's data as CSV, and see every request's outcome. Deletion is admin-only and needs the phone typed again; each request is recorded (migration 0157).
- Console setup: a PortOne cash receipt switch (off by default, saved value beats the env default). Changing Toss payouts needs the owner's TOTP in the same request. Services that need a developer's first-time help are badged, and guide cards reserve screenshot slots.
- Console catalogue: add a new service (code, name, fixed price or quote), archive one by switching it off, edit peak-hour surcharges and quality thresholds, edit a subscription's cadence, day and start time, and load or remove sample data (removal needs the owner's authenticator code).
- Customer app: guests see how a booking works on the Bookings tab and the FAQ and terms on the Chat tab (chat still asks for sign-in). The welcome offer and the referral amount show before sign-in, and a quote in progress is kept on the device across restarts.
- Public `GET /v1/public/offers`: welcome offer, sign-up coupons and referral amount for guests, cacheable for 5 minutes, no user data.
- Customer booking cancel sheet (quote and fixed-price bookings): shows the server's fee tier, fee, refund and payment method, a link to the cancellation and refund policy, an optional reason, and a confirm button that states the refund amount. Booking detail shows the full address with district; the calendar header shows the full year and an empty day says 이 날은 예약이 없어요.
- Console home tiles (상품 묶음): the four home groups (titles in Korean and English, order, picture, shown or hidden, member services, and the move-chooser questions) are edited in the catalogue under a new tab, stored in `service_groups` (migrations 0224 and 0225) and served at `GET /v1/service-groups`.
- Console catalogue: counted-pricing services (수량 선택) are edited as options with a display name, minutes and price, add, remove and reorder, a per-option maximum, a two-or-more discount, and a live price preview. Each option keeps a fixed id, so renaming it leaves its add-on art and add-on link in place.
- Console sidebar: each entry shows its own icon (the icons had shifted by one row).
- Server logs: Caddy writes a redacted JSON access log (14 files of 10 MB), container logs keep 14 files of 10 MB, and an optional Uptime Kuma monitor starts only with `COMPOSE_PROFILES=monitoring`. The edge rate limit on `/v1/auth/*` is not in place (the stock Caddy image has no rate-limit module); see docs/DEPLOY.md.

### Changed
- Peak surcharge can apply to fixed-price and subscription visits, matching quotes. Owner setting `apply_to_fixed` in the peak-pricing tab, default off, so no live price changes until the owner turns it on. When on, the surcharge is part of the list price and flows through three places: the coupon and subscription discount apply to the surcharged price (percent discounts also discount the surcharge, and coupon minimum spend sees it); partner payout gross uses the surcharged price; subscription MRR and recognized revenue keep the base plan price and do not include visit surcharges. Quotes always show the surcharge. Stored settings saved before the switch existed load with it off.
- Public write routes have their own per-IP budgets and body cap: error reports and analytics batches (120 requests a minute, 64 KB body), address search (120 a minute per IP, on top of the per-user cap) and the Toss webhook (120 a minute). Owner setting: none; the limits are fixed in code. A pro no longer sees the detail of a job another pro has already claimed. A cross-owner authorization test covers the bookings, chat, cases and partner job routes.
- Daily PortOne payment pull (`PAYMENT_DIFF_INTERVAL_SECS`, default 3600): runs after 03:00 KST, diffs the previous KST day's PortOne payments against checkouts and payments (window padded by one hour at each end), logs each mismatch, and raises the owner's `payment_mismatch` alert (on by default, toggleable in the console alert settings). Checkouts paid at PortOne but unfinished after 24 h now raise `payment_stuck` and get a fulfil retry; the retry stops after 30 days.
- Payment methods are offered to customers only when an admin has switched them on and their PortOne channel key is configured (a wallet without a key no longer appears in `GET /v1/payments/methods` or starts a checkout, and a checkout is never created with a missing key). The admin payment-method list cannot be saved empty (`payment_methods_empty`). A blank channel key counts as missing. The admin list reports `configured` per method, and the console marks a switched-on method that has no channel key.
- Tax-invoice requests are cancelled when their payment is refunded in full, in the app or in the PortOne console. An open request not yet exported is cancelled. A request already exported or issued stays in the owner queue and writes an audit row (`tax_invoice.refund_needs_cancel`); the owner must cancel it on 홈택스 (수정세금계산서). The console does not flag these yet. A partial refund leaves the request open. Owner setting: none (automatic, no default to change).
- Partner payee identifier for the 지급명세서 (주민등록번호 or 사업자등록번호): collected on the employee profile (`PUT /v1/admin/pros/{id}/profile`, field `payee_identifier`), stored encrypted with its own AAD (migration 0375), shown only masked, and revealed by `POST /v1/admin/pros/{id}/payee-identifier/reveal` with a fresh step-up, audited in the same transaction. Entered on the console profile tab (masked afterwards; the owner gets the consent wording in docs/guide). Collection at onboarding is deferred until the verification checklist (WP-18) lands. Owner setting: none; the base-of-withholding decision stays open until the accountant answer is recorded in docs/finance-tax.md (the current base is unchanged: payout amount after the platform fee).
- Per-person amounts CSV for an outside payroll service (`GET /v1/admin/finance/export/payroll`): one row per person with paid count, gross, income tax, local tax and net, plus a total row. Employees are included with their amounts; no payroll is calculated in the system. Owner setting: none.
- Partner quality: the score, status, five categories and up to three tips are readable by the owner (`GET /v1/admin/pros/{id}/quality`, score and status on each row of the console partner list) and by the partner (read-only card under More, `GET /v1/partner/quality`). No-show reassignment skips quality-paused partners and, when the training gate is on, partners missing a required module, and refuses with `quality_paused` or `training_required` when nobody else qualifies, and the console shows that reason in plain Korean. No owner setting added; the gate follows the existing quality pause and training gate settings.
- Business KPI alerts (OPS-2, OPS-8, OPS-14): three owner-toggleable rules (on by default) fire for confirmed bookings with no pro or team older than 24 hours, a same-day cancel rate over 30% with at least 5 bookings, and the quarterly VAT filing reminder 7 days ahead. Thresholds are in 설정 > 운영 (`unassigned_alert_hours`, `same_day_cancel_alert_pct`, `tax_deadline_remind_days`). The dashboard and status page show a 예약 흐름 card: median hours from booking to assignment, bookings with no pro for over 24 hours (visits from today on), today's same-day cancellations, and no-show rate per service over 30 days. A failed read shows the card as unavailable and does not block the page. The unassigned rule ignores past visits, and a failed business rule is logged and skipped so the other owner alerts still run. Open items: withholding (원천세) reminder not built; VAT dates are the corporate quarterly months (individual general taxpayers mainly file 1·7월), to confirm with the tax adviser before making the months a setting; per-pro cancellation counts (OPS-14) not built yet.
- Database point-in-time recovery (engineer path, optional): pgBackRest WAL archiving to S3-compatible storage with its own cipher (`deploy/wal`, `WAL_ARCHIVE_MODE` off by default), weekly full base backup script, `pg_stat_statements` loaded on every database, restore leg in `scripts/ci/backup-drill.sh` (`WAL_DRILL=1`). Recovery point objective is about 5 minutes once archiving is on, 24 hours before. Owner setting: none yet; the restore step belongs to the setup wizard.
- Customer signup collects the required consents (terms, privacy, overseas transfer) before the code is sent, and the server refuses to create a new account without them (`required_consent`). Each grant is stored with the policy version and time. Marketing is a separate optional tick. Safe default: required consents on, no settings to change. Social sign-up (Kakao, Naver, Google, Apple) asks the same required consents, and the social buttons stay off until they are ticked. Release gate: before this customer build ships, set the customer `min_app_version` in the business profile (console) to the first build that sends consents, so older builds are asked to update instead of failing signup with `required_consent`.
- Audit and consent records are append-only (migration 0320): the database refuses edits and deletes. Refunds, payout reveals and finance exports write their audit row in the same transaction as the action.
- Account deletion: a finished booking (completed or cancelled) loses its street address, answers, visit notes and household profile, keeping the district, size, dwelling type, amounts, dates and status for the 5-year record; open bookings are cleared by the hourly purge once they are completed or cancelled.
- Team members invited by a partner wait for the platform owner's approval before they see customer addresses or get jobs (new `pending_approval` state, `POST /v1/admin/team-members/{id}/approve`, approvals audited). Existing active members are unchanged. Owner setting: none; the approval is a console action.
- Server installer hardening: `ufw` turns on only after a confirmation (22, 80, 443 open), fail2ban bans SSH brute-force attempts for 1 hour, unattended security upgrades are on, and SSH password login can be turned off when a key is present. Owner decisions: firewall on or off (default off without a terminal answer), SSH password login off (default kept on until confirmed). A rising count of banned SSH addresses sends a failure ping to the heartbeat URL (`dahae alert ssh`).
- Partner app Windows store metadata uses the 다해 name: pubspec, `build-msix.ps1` defaults, and the executable's CompanyName and LegalCopyright all use the placeholder `Dahae` (publisher `CN=Dahae`) until the real Partner Center values are put in `scripts/store/msix.local.psd1`. Test certificates are now kept per publisher. The README now covers run, test, build and store steps. The app ID `com.malkkeum.partner` is unchanged pending the owner's decision.
- Owner console v1 distribution (proposed, owner sign-off pending): the console would ship as an installable web app at `/admin/` (Edge, Chrome, Android, iPhone). Install steps and offline test list in `docs/store/console-pwa.md`; device verification not yet run. No store listing or desktop shell yet. APPS-2 stays open until an Android install is tested.
- Consultation chat: customer and partner messages now push staff devices, at most once per thread per minute.
- Saved-card billing keys are stored encrypted at rest; existing cards are sealed at the next server start, and one card per key is enforced on a keyed digest.
- Payments: a cancellation made in the PortOne console now updates the booking's refunded amount from PortOne, and opens a staff payment-dispute case on the booking (once per provider transaction). Queued refunds ask PortOne to cancel only while the balance is still the amount.
- Partner job acceptance no longer waits on required training modules by default. Owner setting `training_gate_enabled` (운영 규칙, 파트너 품질, default off) turns the gate on; with it on, a published required module without completed progress blocks accept. Safe default: off, because partners have no way to publish or complete training yet.
- AI assistant: phone numbers and e-mail addresses from customers and staff are masked before the message goes to the model; a failed model call is recorded as an `error` outcome that spends no reply cap and opens no complaint case, but at most 6 per chat per hour and the shop's daily cap in total, and the console inbox shows an outage notice while calls keep failing; a reply that states a won amount (Arabic or Korean numerals, `₩`, `KRW`, `won`), refund or discount not found in the shop data or booking is replaced by the staff handoff line.
- Catalogue: counts pricing for aircon, sofa, mattress, window and disinfection (several units per booking, `counts` and `unit_counts` in the API, multi-item discount); whole_home quote; prices are placeholders the owner edits in the console; old mattress bookings and subscriptions move to counts (migrations 0230-0233)
- Console quote desk shows customer answers in Korean labels (home type, cleaning type, building, layout, floor, add-ons) instead of raw codes, and the estimate breakdown adds the peak surcharge and VAT lines so it adds up to the total; inbox card previews read 견적 카드 · amount and 예약 요청 카드. Staff inbox rows show the customer's name (or 이름 없음) and phone (010-1234-5678).
- Partner app: review reply button hidden until replies ship; sign-out asks for confirmation; version row reads the installed build (PackageInfo); job detail and job cards show aircon add-on counts for older bookings (e.g. 벽걸이 1 · 스탠드 2).
- Console payment cards lock after save: the owner unlocks with an authenticator code (5 minutes), the first save of the PortOne card needs that code, and saved secrets and channel keys show only a mask. Billing and cash receipts have their own channel slots (card billing, cash receipt). Existing installs keep working: billing falls back to the saved `card` channel key and cash receipts to the payment method's channel key until those slots are filled. Account deletion stops, with a retryable error, while a saved card cannot be revoked at PortOne because the API secret is missing.
- Finance has one revenue and fee definition: the VAT export and the console dashboard revenue tile now read the same aggregate as the finance summary, so held settlements are left out of VAT and refunds use the summary's cancel-date basis. Partner, service and payout-bearing finance series (summary, series, partners, services) are admin-only; consultants get 403. Dashboard revenue is the last 30 KST days including today.
- Completing a job requires the arrival check-in first; the owner can complete without one by giving a reason (audited as `check_in.override`), and team members check in and out on their own jobs (`/v1/member/jobs/{id}/check-in`, `check-out`). Check-out is allowed only on an assigned job on its visit day; the customer's app gets a live update on both. The no-show scan skips jobs with a member check-in. The partner app shows the check-in and check-out card on the team member's job, and asks the owner for a short reason (up to 200 characters) when completion is refused for a missing arrival.
- CI: PortOne webhook tests (valid, forged, tampered, stale, replayed) and Toss webhook test added; Dart files a pull request changes must pass `dart format`; install e2e also runs on backend changes.
- Staff invites and password resets now use a one-time 6-digit code sent by SMS (or copied by the admin), valid 24 hours, single use, locked after five wrong tries; consultants can invite consultants only.
- Customer app: guests see the quote price before the address step without signing up; sign-in is asked only when they book, and the service detail shows the price-first line.
- Customer app: guests get the sign-in sheet with a reason for each protected action (chat, account, booking), member-only menu rows show a sign-in hint, the notifications screen shows only notices, and a nameless new account opens the name step after signing in from the sheet.
- Release hardening: release tags must point into master, the image and delivery jobs run in protected
  environments, every action is pinned to a commit, and the installer's SHA256SUMS is cosign-signed. An
  ed25519-signed installer manifest and a cloud-init install page are published with each release.
  Not done yet: a dispatch-driven release that replaces `scripts/release/build-all.ps1` and its
  `PASSWORD.txt`, and an Android-signing environment. Both still run locally.
- Customer and partner apps take the business colour and logo from the business profile, keep the last good copy for offline start, and the console warns when white button text would be hard to read on the chosen colour.
- Server installer: stricter domain and email checks, and settings values can no longer split `.env` lines.
  A root job runner (`deploy/agent.sh`) checks queued server jobs before running a fixed command.
- API JWT library upgraded from jsonwebtoken 10.3 to 11.1; token formats, verification rules and the Apple client secret are unchanged.
- Customer bookings, notifications, wallet points history and payment history, and partner quotes
  page through their lists on scroll (with a bottom loader and retry on error) instead of showing
  only the first 50 rows; the points ledger has a paged `GET /me/points/ledger` endpoint.
- SMS and AlimTalk to customers, partners and reminders now go through the notification outbox: a provider outage is retried with backoff, and a text that fails 8 times is dead-lettered, counted on the console's outbox card and raised as an owner alert. OTP codes stay inline with one quick retry.
- Website seller details (상호, 대표자, 사업자등록번호, 통신판매업 신고번호, 주소, 연락처, 호스팅 제공자, 개인정보 보호책임자) are written into the HTML by the server (Caddy includes `/v1/public/seller-info`), so they are visible without JavaScript; the JavaScript fill and the `build_legal.py --brand` bake are removed.
- 정산 내역 CSV (`/admin/finance/export/settlements`) ends with a 합계 row; KST month edges and totals covered by tests.
- Hardening: new encrypted values carry a version prefix and are bound to their column (old values still read); login codes use `OTP_PEPPER` when set; no-show cases are unique per booking; reviews reaching the report threshold are queued for staff, not hidden; reading partner quotes no longer changes them (a timer closes stale ones); setup code kept out of the structured log.
- Partner schedule: accepting a job, or assigning it to a team member, is refused with `schedule_overlap` when it overlaps another live job of the same pro or member on the same visit date (window = start hour plus the estimated minutes, one hour when unknown). Claims per pro are serialized so two overlapping accepts cannot both win. The same check now covers no-show reassignment, preferred-partner auto-accept on subscription visits, and customer reschedules of an accepted job (refused with `schedule_overlap`, the move is rolled back); a job already taken by another pro reports `already_taken`. The partner app shows the conflict in Korean and English.

### Fixed
- Subscription visits: a visit moved to another arrival window tells the customer (`subscription.visit_moved`); a day with no free window tells the customer and staff (`subscription.visit_unplaced`, `subscription.capacity_full`). Notices are sent once, when the visit is planned.
- Tax-invoice CSV export (`/admin/finance/export/tax-invoices`) marks each request exported in the same transaction and skips requests already exported, so a second export is empty. A new owner-only `POST /admin/finance/export/tax-invoices/re-export?confirm=true` sends the same rows again without changing exported_at and writes an audit row.
- Cash receipts: a failed local write after a receipt is cancelled at PortOne is no longer discarded; the receipt is never cancelled at PortOne twice, and it is counted as needing a check in the ops status JSON. A provider refusal of a cancel is retried by the reconciler, up to five times with growing gaps. Cash receipts stay off until the owner enables them after the PortOne sandbox check. Console display of the new count is not built yet.
- Day-before visit reminders are claimed in SQL (oldest visit first), so a busy day no longer starves later bookings; advertising pushes that reach the notify path are held in the outbox 21:00-08:00 KST and sent at 08:00 (SMS campaigns are already gated by quiet hours at send time) (quiet hours are fixed by 정보통신망법, no owner setting).
- Checkout: the checkout and virtual-account deposit screens show the amounts the server computed (the price to show and the amount to deposit after points and coupons), and the deposit screen names the bank (for example 국민은행) instead of PortOne's bank code. The dev payment fake issues its virtual account at `KOOKMIN`, PortOne's real code, so demos show 국민은행.
- Rewards: referral, cashback and welcome points are worked out on the price after discounts.
- Subscriptions: charges settle, retry and hold capacity safely; a stored charge on a skipped or cancelled visit is settled and refunded; a paused subscription releases its planned seats; a failure shows the real reason (a slot that fills after the card was charged refunds it and says so, `subscription.slot_full_refunded`, owner-editable) and a visit with no retry left is marked missed; creating a subscription sends an idempotency key, so a retry cannot make a second one.
- Scheduling: quote deposits and pro-confirmed reschedules run under the pro's schedule lock with the overlap check, a deposit is refused for a pro whose offers are paused, and the partner banner shows the slot a reschedule asks for. A pro can no longer confirm a reschedule once the booking is cancelled or completed (`booking_closed`); the job stays where it was and the customer is not told it moved.
- Uploads: the purge keeps photos that completion reports, reviews, logos and content still name; an upload row is committed before its file is written, and a failed write removes both.
- Privacy: overseas transfer consent can be withdrawn in 동의 관리 (push and the AI chat stop with it, and staff chat pushes then carry no customer text). After a policy version change, a customer whose overseas consent was given for the older text is asked again (`reconsent_required`) instead of push and the AI chat quietly switching off. The data export covers every section that holds the customer's data, and a saved card's billing key is revoked at PortOne before the card is removed.
- Server operations: restore clears the schema first; re-running the installer with a stopped database updates it; the rollback image is kept; the update job has its own longer timeout and the job lock waits as long as an update may run; photo archives over 5 GiB upload in parts. A failed database or photo offsite upload shows on the status page and raises an owner alert (`backup_offsite_failing`, `backup_uploads_failing`, on by default). The previous release can still queue refunds and reset staff passwords after this release's migrations (0470, 0471). Migrations 0472 (`toss_ref`) and 0473 (`owner_not_sent_at`, `toss_memo`) are not understood by 1.4: rolling back while a Toss payout is failed with the owner's not-sent, or requested or sent after a send with no answer, could let a cancel refund a job Toss may still pay, or re-send it under a new key. `dahae rollback` now refuses while any such payout exists (or when it cannot check) and says how many; settle them with the status check in the console first.
- Payouts: a Toss 401 or 403 is no longer taken as a payout decline; a payout at Toss blocks booking cancels, staff case refunds and card changes only while money may be in transit (a case refund on a job in a batch nobody has sent takes the job out of that batch first); only an ambiguous Toss send blocks a manual fail. An ambiguous send (no answer from Toss) is settled by the status check when Toss lists it, or by the owner's not-sent confirmation (authenticator code) after checking the Toss dashboard; a send missing from Toss's list stays ambiguous. The owner is alerted when a Toss payout has had no answer for 30 minutes (`payout_unconfirmed`, on by default). After Toss itself reports a transfer failed (no money moved), the retry is sent with a new `refPayoutId` (migration 0472), since Toss refuses a reference it has seen; an ambiguous send keeps its reference.
- Payouts: a refused re-send of a Toss payout that had no answer before (or after the owner's not-sent) no longer marks it failed or changes its key and `refPayoutId`; it stays "maybe sent" for the status check or the owner, so a retry can never pay twice. After the owner's not-sent the batch is kept as it was until Toss settles it: its bookings wait to be cancelled, and the memo of the first send is reused (migration 0473). A refused Toss lookup (payout list or one payout) counts as "Toss did not answer", so the status check and the not-sent confirmation stay open. A Toss payout lookup answered inside `entityBody` is read, and so is a wrapped seller answer. The payout create body is sent as a list, as the Toss docs show. Owner setting: none.
- Payments: a tax-invoice request waits for the checkout to settle and is refused once paid; saving the PortOne card keeps the env webhook secret and channel keys; the console explains `checkout_not_open`.
- Apps and console: partner job actions run one at a time, so a second tap cannot send accept twice; a campaign send creates and schedules in one replayable call, so a retry cannot send twice; the connection test runs on a locked card with the saved values without a second owner code; the setup guides name the real check buttons and say where the Toss webhook goes.

### Security
- Every staff role must enrol TOTP before sign-in; customer phone lookup, search and export are admin-only; customer export, deletion, bank-detail reveals and payouts need a TOTP step-up within five minutes (`POST /v1/auth/staff/mfa/step-up`; the console asks for the code and retries). Staff without a second factor get no session refresh. `STAFF_MFA_REQUIRED=0` is refused outside `DEV_MODE`.
- Live-event sockets open with a 30-second, single-use ticket from `POST /v1/realtime/ticket` instead of the access token in the URL, so the long-lived token no longer reaches proxy and access logs. The old `?token=` form still works for one release. Silent sockets are pinged every 25 s and closed after 60 s without an answer.
- Sign-in and staff: the store-review login can be switched only by the owner; finance exports need a live admin and a fresh step-up (the console asks for the authenticator code); the copy-reset path needs an enrolled second factor and an admin-given phone reset needs a second factor; deactivating a staff member ends the session; the deprecated socket token checks the session epoch. Sign-in code sends are watched hourly (`otp_volume` owner alert at 3,000 an hour) with a hard ceiling at ten times that, and `sms_required` has copy in both languages.
- Sign-in codes: the hourly ceiling now bounds all SMS spend; past it every number gets the same refusal, account or not, so the answer no longer reveals which numbers have an account, and the owner alert says codes are paused for everyone until the hour rolls over. The per-network cap is 1,000 codes an hour for an IPv4 /24 (carrier NAT) and stays 300 for an IPv6 /64. The store-review fixed code signs in only the review customer account: it never opens a partner or member session or binds a phone, and once a pro, team member or staff member holds the number it gets a real SMS code.
- Sessions: sign out everywhere (`POST /v1/auth/logout-all`) for staff ends the access tokens and socket tickets at once, like the owner's sign-out; customer, partner and member sockets close within a minute once no signed-in device is left (sign-out everywhere, account deletion, partner deletion, team-member removal, which now revokes the member's refresh tokens), and their unspent socket tickets are refused. The deprecated `?token=` socket uses the epoch of the token it checked.
- Personal data in logs: phone numbers written with spaces or mixed dashes and spaces are redacted in reviews and error reports; the edge access log drops query strings and keeps 3 months, the documented retention rule.

## [1.4.0] - 2026-11-13

### Added
- 다해 (Dahae) brand across the apps, console, website and store material: design tokens, 3D icon set,
  new home, bookings, chat, booking flows and More tab; remaining design gaps closed (white flow canvas,
  banner art card, address sheet with search, teal brand word).
- One-command installer with the `dahae` CLI: preflight, signed images (cosign keyless), restore,
  uninstall, update channels, rollback-safe migrator, arm64 images, `www` redirect.
- Install end-to-end test (tar mode, wizard, restore, smoke, uninstall), upgrade path from an older
  installer, and a WSL harness.
- Owner console as a sideloadable Android APK with a first-launch server address screen.
- iOS release hardening (web credentials, per-app signing, IPA checks).
- Crash and error reports from the customer and partner apps to the API's own `client_errors`
  (no third-party SDK), declared in the store privacy answers and iOS privacy manifests.
- Durable notification outbox and a replica-safe background job runner with per-job health; the console
  status page shows each job's last success and flags stalled ones.
- Durable refund queue: a compensating refund that fails is retried with backoff and alerts the owner;
  customers see "refund in progress" until it lands.
- Cursor pagination (`X-Next-Cursor`) on long lists; the console, partner and chat lists load more on
  scroll.
- Repository guides: CONTRIBUTING, SECURITY, CODEOWNERS, issue and pull request templates, `.editorconfig`.
- Website: canonical and Open Graph / Twitter metadata, JSON-LD (`LocalBusiness`, `Service`, `WebSite`),
  self-hosted font subset, legal-page revision history, business details baked in on request
  (`scripts/build_legal.py --brand`).
- `scripts/local.env.example`: machine-specific tool locations, read by `scripts/env.sh` and the
  PowerShell release scripts.

### Changed
- Korean and English only. Twelve more languages are archived in `archive/i18n-extra-languages` with
  restore steps; accounts that used one read English.
- CI runs on GitHub-hosted runners only; the Rust and Flutter toolchains, actions and base images are
  pinned; cargo-deny, image scanning and SBOMs gate every release, and a tag publishes nothing unless CI
  and the install end-to-end test pass on it.
- Hardened compose stack (read-only filesystems, dropped capabilities, resource limits), working offsite
  backups, a copy of the encryption keys and alerts on failed unattended jobs; strict website CSP with no
  third-party hosts.
- Smaller apps: the Korean font subset (Dahae Sans) cuts fonts from 7.6 MB to 2.9 MB per app.
- A failed refresh keeps the data on screen with one notice instead of a blank error page; the partner
  app opened without signal starts from the last saved profile and catches up when the connection returns.
- The website hero loads a smaller three.js build only after first paint, and never with reduced motion.
- Documentation states RPO/RTO and how to keep `DATA_KEY` safe.

### Fixed
- Payments: each PortOne payment is bound to one checkout; idempotency keys are scoped to the verified
  user and reused per action, so a retry after a lost reply never charges twice; a confirm that never
  answers waits and resumes for the same account only.
- Cancellations refund once, under the booking lock, against the provider's cancellable balance; saved-card
  charges (tips, waitlist, deposits) go through PortOne billing; money actions check the live staff role.
- Abuse and privacy: durable sign-in lockout and rate limits, review report quorum, EXIF stripped from
  uploads.
- Sign-out clears the previous account's data, pending payment and live connection.
- Payment races: a deposit settled for a booking another checkout already confirmed is refunded.
- Installer security: non-superuser API database role, restore guards, `.env` permissions; Caddyfile
  parses on real servers.

### Removed
- Hardcoded developer-machine paths and account names from scripts and install guides.

## [1.3.0] - 2026-11-12

### Added
- Store readiness: privacy manifests, Data Safety answers, predictive back, native symbols, store
  screenshot pipeline and a single rising build number.
- Complete account deletion with Sign in with Apple revocation and server notifications, Kakao and
  Naver unlink.
- 전자상거래법 seller details in the customer app and website footer; hosting provider field.
- Points wallet, payment history and receipts; guest mode and personalised feed.
- Current-location address lookup behind location-terms consent.

### Fixed
- Strict CSP for the console and web app (no Google hosts).
- Large-table migrations rebuilt with `CONCURRENTLY` and `NOT VALID` + `VALIDATE`.

## [1.2.0] - 2026-11-12

### Added
- Operations screens, staff MFA and lockout, booking policy, tax exports, partner and console
  improvements; public API contract (`openapi.json`).

## [1.0.0] - 2026-11-11

### Added
- Setup wizard with encrypted integration keys and connection tests; first-run owner account.
- Single-domain deployment (Caddy: website, console, API), installer, backups and weekly updates.
- Website with live prices and legal pages; store listings and generated screenshots.
- AI-assisted support replies, reviews, referrals, demo data, finance dashboard, offsite backups,
  customer web app (PWA), native social sign-in and push.

## [0.1.0] - 2026-11-10

### Added
- Domain core (catalog, pricing, quotes, coupons, booking states, payouts) and the API
  (auth, bookings, payments, chat, quote desk, partner jobs, console, audit log).
- Customer, partner and console apps on a shared Flutter workspace; CI pipeline.
