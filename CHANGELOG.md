# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Server releases are git tags `vX.Y.Z`
(see [docs/DEPLOY.md](docs/DEPLOY.md#2-releases-and-channels)). Entries up to 1.4.0 were reconstructed
from the release branches; no tags existed for them.

## [Unreleased]

### Added
- One-command AWS EC2 launch from AWS CloudShell: `deploy/aws/launch.sh` checks the release installer against its SHA256SUMS, pins it into the CloudFormation template, finds the default VPC's public subnet and creates stack `dahae-main`; `--admin` creates the two IAM policies for DODA's admin. Guide 10 starts with it.
- Toss Payments as a second payment gateway, next to PortOne. The new console card 결제 (토스페이먼츠) takes the API 개별 연동 keys of the payment-window MID, an optional 자동결제 MID pair for saved cards and an optional 보안 키; it has a test/live badge, the webhook address `/v1/webhooks/tosspayments` to copy, and an API key check per MID. Its switch 사용: 새 결제를 토스페이먼츠로 받기 sends new payments and newly saved cards to Toss Payments; the setup screen says where new payments go, and warns while Toss runs on test keys. Payments, refunds and saved cards already started stay on their gateway, and identity checks (PASS) stay on PortOne, so the PortOne card must stay configured. The first save needs the owner code, like the PortOne card. Setup steps: [payments setup §6](docs/guide/payments-setup.md#6-토스페이먼츠로-결제받기).
- API: public `POST /v1/checkouts/{id}/toss-confirm` (called by the Toss success page), `POST /v1/webhooks/tosspayments`, `gateway` in the checkout intent and in `GET /v1/me/payment-methods/setup`, and `toss_payments_webhook_url` / `payment_gateway` / `payment_gateway_mode` in `GET /v1/admin/setup`. PortOne responses are unchanged apart from the added `gateway`.
- Privacy policy: 토스페이먼츠 is listed as a payment processor.

### Before going live
- Toss Payments: sign the 전자결제 contract (and 자동결제 for saved cards), then follow the console order in [payments setup §6](docs/guide/payments-setup.md#콘솔-순서): save test keys, leave on only the methods Toss approved, switch on and run the test-key checklist on Android, iOS and the web before opening for business, refund the test payments, then replace all four keys with live keys.

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
