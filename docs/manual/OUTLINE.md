# 다해 매뉴얼북 · Dahae Manual Book — Outline

Two books from one tree: `docs/manual/ko/*.md` (한국어, 해요체) and `docs/manual/en/*.md` (plain English).
Same chapter numbers and file names in both. Readers: the licensee's owner/admin (non-technical) first;
developer-only material lives in chapter 33.

## Writer rules (every group)

1. **Exact labels.** Every button, menu, tab, card and field name is copied from
   `app/packages/core/lib/l10n/app_ko.arb` (KO book) or `app_en.arb` (EN book), or from the `Tr('…','…')`
   pairs in `app/apps/console/lib/screens/setup_guides.dart`. Write them in bold brackets: **[저장]** / **[Save]**.
   Each section below lists the ARB keys it relies on; resolve them, do not paraphrase.
2. **Click by click.** One action per numbered step: where to look, what to click, what you should see next.
   After each block, a "이렇게 보이면 성공 / You should now see" line.
3. **Never invent.** No feature, price, fee, period, menu name or legal fact the code or docs do not show.
   If unsure, write **확인 필요** / **to be confirmed** and move on. Costs: only "업체 요금표 / see the provider's
   price list" unless a doc gives a figure.
4. **Screenshots** are placeholders: `![<caption>](../shots/<id>-ko.png)` (EN: `-en.png`). Use only ids from
   `screenshots.json`; add a new id there if a section truly needs one (keep ids unique, prefix by app).
5. **Secrets.** Never show a real key. Use `test_ck_…`, `whsec_…`, `example.co.kr`, `owner@example.co.kr`.
6. Each chapter file starts with `# NN. Title`, then a 2–3 line "이 장에서 할 일 / In this chapter", then
   "준비물 / You need" (if any), then sections, then "자주 막히는 곳 / Common problems".
7. No AI mention anywhere. Product name in text: 다해 / Dahae (see chapter 28 for how names show today).

## Known discrepancies to settle while writing (check code, write the code's answer)

| Topic | Sources disagree | Check |
|---|---|---|
| Setup code lifetime | guide 00 + ARB `cSetupCodeExpired` + backend say **7 days**; guide 10 B-6 says "2 hours after first shown", `--renew` | `backend/crates/api/src/routes/setup.rs` (`SETUP_CODE_TTL_SECS` = 7 days), `deploy/install.sh` (`setup-code`, `--renew`) |
| Setup card order | guide 00 ⑤ lists business → payments → SMS → push → login | Real order: `setup.dart` (Business, Payment methods, Review demo account, Uptime heartbeat, App links) then server order `Section::ALL` in `backend/crates/api/src/integrations.rs` (PortOne, Toss Payments, Solapi, Push, Kakao, Naver, Google, Apple, AI, Offsite backup, Partner payouts, Address search) |
| Keys backup from the console | guide 00 says not yet in the console, ask developer (`dahae keys-backup`) | grep console for keys-backup; if absent keep "ask your developer" |
| Photo offsite backup | `cIntBackupPhotos` appears in System status | is there a separate card? `routes/setup.rs`, `ops.dart` |
| Console host jobs | `deploy/agent.sh` accepts backup-now, rollback, channel, heartbeat…; console shows only **[지금 업데이트]** | `host_update.dart`, backend host job routes; document only what the console shows, rest goes to ch. 33 |

---

## Part 1. 시작하기 · Getting started

### 00. 표지와 읽는 법 · Cover and how to use this book — `00-cover`
- Title, edition (release from `CHANGELOG.md`), owner/licensor notice (`README.md` 라이선스 section, `docs/legal/`),
  how to read: icons for "사장님 / Owner", "개발자 / Developer", "확인 필요"; label convention **[버튼]**.
- Roadmap diagram: install (Part 2) → console setup (Part 3) → run the business (Part 4) → apps (Part 5).
- Sources: `README.md`, `CHANGELOG.md`, `docs/guide/README.md`.

### 01. 다해는 무엇인가요 · What Dahae is — `01-what-is-dahae`
- 1.1 한 줄 소개, who it is for (`README.md` 누구를 위한 프로그램인가요 / Who it is for).
- 1.2 Components: server (API + DB + web), admin console (`/admin/`), customer app, partner app, website;
  how they connect (`docs/ARCHITECTURE.md`, `README.md` 구조, `docs/guide/08` 2-1).
- 1.3 Roles: 소유자 (owner, `oaOwnerBadge`), 관리자 (`cRoleAdmin`), 상담원 (`cRoleConsultant`), 파트너, 고객,
  개발자 (licensor). What each can see: `adminOnly` rows in `app/apps/console/lib/nav_sections.dart`.
- 1.4 Main features tour (`README.md` 주요 기능, `docs/FEATURES.md`).
- Screenshots: `console-dashboard`, `customer-home`, `partner-jobs`.

### 02. 시작 전에 준비할 것 · Before you start — `02-before-you-start`
- 2.1 Accounts checklist (who owns each): AWS (or other cloud), domain/Cloudflare, PortOne and/or Toss Payments,
  Solapi + Kakao channel, Firebase/Google, Kakao/Naver/Google/Apple developer, Google Play, Apple Developer,
  Cloudflare R2 or NCP Object Storage, uptime service (healthchecks.io), authenticator app, password manager.
  Sources: `docs/guide/00` 준비물, `docs/guide/10` 준비물과 역할 나누기, `docs/LAUNCH-PLAN.md` 6–7.
- 2.2 Documents: 사업자등록증, 통신판매업 신고증, 발신번호 서류, logo file (`docs/guide/02` 권장 순서).
- 2.3 Costs: only as docs state (provider price lists; R2/NCP "월 몇 천 원 수준" from guide 02 §6).
- 2.4 Start the slow approvals first (PG review, sender number, AlimTalk templates, store accounts).
- 2.5 Timeline (`docs/LAUNCH-PLAN.md` 5).
- Screenshots: none.

## Part 2. 서버 설치 · Installing the server

### 03. AWS 담당 계정 만들기 · AWS: the deployer account — `03-aws-account`
- 3.1 Sign in to the AWS console, switch region to 서울 ap-northeast-2.
- 3.2 Fast path: open CloudShell (`>_`), paste
  `curl -fsSL https://raw.githubusercontent.com/EricWijesinghe/Dahae/master/deploy/aws/launch.sh | bash -s -- --admin`;
  expected output lines ("created policy …", "Next: IAM > Users …") from `deploy/aws/launch.sh` L55–62.
- 3.3 Manual path: `dahae-boundary` policy (exact name, attach to nobody), `dahae-deployer-policy`
  (`deploy/aws/dahae-boundary-policy.json`, `deploy/aws/deployer-iam-policy.json`).
- 3.4 Create IAM user `dahae-deployer`: console access, custom password, attach `dahae-deployer-policy`
  + `AWSCloudShellFullAccess` (not `dahae-boundary`).
- 3.5 MFA on a phone call with the developer (QR on the developer's phone, two codes read back); no access keys.
- 3.6 What to send the developer and by which channel (sign-in URL, user name, password separately).
- Sources: `docs/guide/10` A (KO + English), `deploy/aws/launch.sh` header.
- Screenshots: `aws-region-seoul`, `aws-cloudshell-admin`, `aws-iam-create-user`, `aws-iam-attach-policies`, `aws-iam-mfa`.

### 04. 명령 한 줄로 서버 만들기 · One command to create the server — `04-aws-launch`
- 4.1 Developer signs in as the deployer (MFA), region check.
- 4.2 CloudShell launch command with `--domain`, `--email`; what it does (SHA256SUMS check, default VPC public
  subnet, stack `dahae-main`, ~5–10 min); printed line "Dahae vX → stack …"; outputs (`PublicIp`).
- 4.3 Options: `--hosted-zone-id`, `--instance-type`, `--version`, `--stack`, `--vpc/--subnet`, `--region`, `--dry-run`;
  never touches an existing stack.
- 4.4 After `CREATE_COMPLETE` the install still runs inside the server (~15–20 min).
- 4.5 Manual alternative (B-1/B-2 template upload, parameter table) — short, points to ch. 33.
- Sources: `deploy/aws/launch.sh`, `deploy/aws/dahae.cfn.yaml`, `docs/guide/10` B, B-1, B-2, `docs/LAUNCH-PLAN.md` 4.
- Screenshots: `aws-cloudshell-launch`, `aws-cfn-stack-complete`, `aws-cfn-outputs`.

### 05. 도메인 연결하고 확인하기 · Connect the domain and check — `05-domain-dns`
- 5.1 Route 53 case (automatic A record; add `www` yourself for an apex domain).
- 5.2 Cloudflare: DNS > Add record, type A, name (`@`/`www`/`app`), IPv4 = `PublicIp`, TTL, **DNS only (grey cloud)**.
- 5.3 Other registrars (가비아, 후이즈, 카페24): "DNS 관리" A record.
- 5.4 Waiting and checking: `https://<domain>/health` → `{"ok":true}`; `https://<domain>/admin` shows the
  first-run screen; "안전하지 않음" means DNS not yet propagated.
- 5.5 Server web shell without SSH: EC2 > Instances > Connect > Session Manager; `sudo dahae status`.
- Sources: `docs/guide/10` B-3–B-5, `docs/guide/08` 4-1–4-4, `docs/guide/00` ②.
- Screenshots: `cloudflare-dns-a-record`, `browser-health-ok`, `aws-ssm-connect`, `aws-ssm-status`.

### 06. 사장님 계정과 2단계 인증 · Owner account and two-step sign-in — `06-owner-account`
- 6.1 Get the setup code: `sudo dahae setup-code` (and `--renew`), format `XXXX-XXXX-XXXX`; lifetime (see discrepancy table).
- 6.2 First-run screen: `cFirstRunTitle`, fields `cFirstRunCode`, `cName`, `cEmail`, `cPassword` (`cPasswordRule`),
  button `cFirstRunCreate`; errors `cWrongSetupCode`, `cSetupCodeExpired`, `cWeakPassword`.
- 6.3 Why the OWNER (not the developer) creates it: the owner code lives on the owner's phone (`docs/guide/10` B-6).
- 6.4 Sign in (`login.dart`: `cTitle`, `cEmail`, `cPassword`, `cLogin`, `cServerChange`).
- 6.5 2FA enrolment (`staff_mfa.dart`: `cMfaEnrolTitle`, `cMfaEnrolBody`, QR/`cMfaSecret`/`cMfaLink`, `cMfaCode`,
  `cMfaConfirm`), recovery codes (`cMfaCodesTitle`, `cMfaCopy`, `cMfaDownload`, `cMfaContinue`); later sign-ins
  (`cMfaUseRecovery`), errors `cMfaInvalid`, `cMfaLocked`, `cMfaExpired`.
- 6.6 Keys backup copy (`sudo dahae keys-backup`, keep off-server) — `docs/guide/00` ③ box, `docs/guide/08` 6-5.
- 6.7 Install the console like an app (PC Edge/Chrome, Android Chrome, iPhone Safari) — `docs/guide/04`, `docs/store/console-pwa.md`.
- Screenshots: `console-first-run`, `console-login`, `console-mfa-enrol`, `console-mfa-recovery-codes`, `console-mfa-signin`, `console-pwa-install`.

### 07. 다른 클라우드에 설치하기 (간단히) · Other clouds, in brief — `07-other-clouds`
- 7.1 Server spec (Ubuntu 24.04 LTS, 2 vCPU/4 GB recommended, 1 GB minimum, 50 GB, Seoul), ports 22/80/443.
- 7.2 Install page (`dahae-install-page.html`, `deploy/install-page/`): domain, email, two checks,
  **[설치 파일 만들기]**, **[복사하기]**, paste into User data / cloud-init / startup script; 7-day install code.
- 7.3 Provider field table (Lightsail, EC2, GCE, Azure, OCI, DigitalOcean, Vultr, Hetzner, Linode, 네이버 클라우드,
  NHN, 카카오/KT/가비아/카페24) — names only as guide 09 gives them.
- 7.4 Fallback: browser SSH one line `curl -fsSL INSTALL_URL | sudo bash`, the two questions, the finish screen.
- 7.5 What runs automatically afterwards (03:30 backup 14 days, Monday 04:00 update with auto-rollback, HTTPS renew).
- Sources: `docs/guide/00`, `01`, `08`, `09`.
- Screenshots: `install-page-form`, `install-finish-terminal`.

## Part 3. 관리자 콘솔 · The admin console

### 08. 콘솔 둘러보기 · Console tour — `08-console-tour`
- 8.1 Layout: side menu (desktop) / drawer (phone), `Ctrl+K` palette, `Ctrl+1..9` (`app/apps/console/lib/shell.dart`).
- 8.2 Every menu row in order, with one line each and who sees it (`nav_sections.dart`):
  `cDashboard` 대시보드, `cBookings` 예약, `cQuoteDesk` 견적 데스크, `cInbox` 상담함, `cCatalog` 카탈로그 · 가격,
  `cMarketing` 쿠폰 · 콘텐츠, `cPartners` 파트너, `cSettlements` 정산, `fFinance` 재무 (admin), `cSubsTitle` 정기 청소 구독,
  `cCustomers` 고객, `cStaff` 직원 (admin), `oaTitle` 내 계정 (admin), `cSetup` 설정 (admin), `rulesTab` 운영 규칙 (admin),
  `ntNavLabel` 알림 문구 (admin), `cOps` 시스템 상태, `cAudit` 활동 기록 (admin), `prqTitle` 탈퇴·개인정보 요청,
  `rnwTitle` 갱신 일정 (admin).
- 8.3 Language switch, sign out, step-up code prompt for sensitive actions (`stepup.dart`, `cOwnerCodeTitle`).
- 8.4 My account (`owner_account.dart`): owner badge, `oaClaim`, `oaTransfer`, `oaPassword`, `oaRecoveryCodes`,
  `oaAlertPhone`, `oaSignOutAll`, `oaSessions`.
- Screenshots: `console-menu-desktop`, `console-menu-phone`, `console-palette`, `console-my-account`, `console-stepup-prompt`.

### 09. 설정 화면 사용법 · How the Setup page works — `09-setup-basics`
- 9.1 Header `cSetupTitle`, progress `cSetupProgress`, `cSetupAllDone`, test-mode banner `cSetupDevMode`.
- 9.2 Anatomy of a card: **[따라 하기]** `cSetupGuide`, **[사이트 열기]** `cSetupOpenSite`, `cSetupOptional`,
  dev-help marker (`devHelp` in `setup_guides.dart`), fields, `cSetupShow/cSetupHide`, `cSetupKeepValue`, `cSetupFromEnv`,
  **[저장]** `cSetupSave`, `cSetupSaved`, **[지우기]** `cSetupClear`.
- 9.3 Lock and owner code: saved cards lock (`cSetupLocked`, `cSetupLockedBadge`); **[잠금 해제 (소유자 코드)]**
  `cSetupUnlock`, 5-minute window (`cSetupUnlockLeft`, `UNLOCK_TTL_SECS` in `backend/crates/api/src/routes/setup.rs`),
  one save then relock, **[잠그기]** `cSetupLockNow`, `cSetupOwnerOnly`; first save of money cards needs the owner
  code (`setup.rs` L702); errors `cOwnerCodeWrong`, `cOwnerCodeLocked`.
- 9.4 Connection check: **[연결 확인]** `cSetupTest` / **[API 키 확인]** `cSetupTestApiKey`; results
  (`setupMessage` in `setup_guides.dart`: `cSetupConnected`, `cSetupCheckInApp`, `cSetupInvalidKey`, `cSetupUnreachable`,
  `cSetupNotConnected`, `err_invalid_*`); test vs live badge `cSetupModeTest`/`cSetupModeLive`.
- 9.5 Recommended order and what can wait (`docs/guide/02` 권장 순서).
- Screenshots: `console-setup-overview`, `console-setup-card-guide-open`, `console-setup-card-locked`, `console-setup-owner-code`, `console-setup-test-ok`, `console-setup-test-invalid`.

### 10. 사업장 정보와 결제 수단 · Business profile and payment methods — `10-setup-business`
- 10.1 Business card (`setup_business.dart`): `cSetupBusinessName`, `cSetupColor` (+ `cSetupColorContrast`), `cSetupLogo`,
  `cSetupCompany`, `cSetupLegalName`, `cSetupCeo`, `cSetupRegNo`, `cSetupMailOrderNo`, `cSetupAddress`,
  `cSetupSupportPhone`, `cSetupSupportEmail`, `cSetupSupportHours`, `cSetupPrivacyOfficer`, `cSetupHostingProvider`;
  where each appears (site footer, terms, apps).
- 10.2 Store links `cSetupStoreLinks` (`cSetupCustomerApp`, `cSetupPartnerApp`) — fill after release.
- 10.3 App versions `uAppVersionTitle` (`uLatestVersion`, `uMinVersion`, `uAppVersionHint`).
- 10.4 Payment methods card (`setup_pay_methods.dart`): `cSetupPayMethods`, `cSetupPayMethodsHint`, `cSetupNeedsKey`,
  `cPayMethodNoKey`; cannot switch all off (`docs/guide/02` §2 notes).
- Sources: `docs/guide/02` §1, `setup_formats.dart` (input formats).
- Screenshots: `console-setup-business`, `console-setup-business-color-warning`, `console-setup-store-links`, `console-setup-app-versions`, `console-setup-pay-methods`.

### 11. 결제 (PortOne) · Payments (PortOne) — `11-setup-portone`
- 11.1 Prepare: business registration, PortOne signup, PG contract/review (`docs/guide/payments-setup.md` 0–1).
- 11.2 Step by step from `guides['portone']` in `setup_guides.dart` (steps + field labels): Store ID, V2 API Secret,
  channel keys per slot (`slotLabels`: card, card_billing, kakaopay, naverpay, tosspay, virtual_account, identity,
  cash_receipt, other slots `cSetupOtherMethods`, hints `cSlotHint*`), webhook URL `cSetupWebhook` → PortOne V2 → `whsec_…`,
  cash receipts toggle.
- 11.3 First save with owner code, **[API 키 확인]**, green result; "결제 연결" header `cPaymentsTitle`,
  `cSetupGatewayLive`, `cSetupGatewayTestKeys`.
- 11.4 Webhook check and test payment (`payments-setup.md` 3–4), unconfirmed items (§5 → 확인 필요).
- Fields: `PORTONE_FIELDS` in `backend/crates/api/src/integrations.rs`.
- Screenshots: `console-setup-portone`, `console-setup-portone-channels`, `console-setup-portone-webhook`, `external-portone-store-id`, `external-portone-webhook`.

### 12. 결제 (토스페이먼츠)와 결제사 바꾸기 · Toss Payments and switching gateways — `12-setup-toss-payments`
- 12.1 Contracts needed (payment window; auto-billing for saved cards/subscriptions) — `payments-setup.md` 6.
- 12.2 Keys from developers.tosspayments.com: API individual integration keys `test_ck_`/`test_sk_` (not widget `gck/gsk`),
  optional billing MID keys and 64-char security key; fields `TOSS_PAYMENTS_FIELDS`.
- 12.3 Webhook `cSetupTossWebhook` (`…/v1/webhooks/tosspayments`), events `PAYMENT_STATUS_CHANGED`, `DEPOSIT_CALLBACK`.
- 12.4 Save test keys (owner code) → **[API 키 확인]** (checks secret key only).
- 12.5 Switch: unlock → **사용** toggle (`active`) → save; what moves and what stays (started payments, refunds,
  saved cards stay on their first gateway; PASS identity stays on PortOne) — `payments-setup.md` 전환 규칙.
- 12.6 Test each method, refund all tests; then unlock → live keys → check; the "(테스트 키…)" note disappears.
- 12.7 Roll back: unlock → turn **사용** off → save.
- Sources: `docs/guide/02` §2-1, `payments-setup.md` 6, `guides['tosspayments']`, CHANGELOG Toss entries.
- Screenshots: `console-setup-toss`, `console-setup-toss-active`, `console-payments-header-test-keys`, `external-toss-api-keys`, `external-toss-webhook`.

### 13. 파트너 지급 · Partner payouts — `13-setup-payout`
- 13.1 Default: manual transfer (no card needed); flow in Settlements (link to ch. 21).
- 13.2 Toss payouts: contract first; card `cIntPayout`, fields `PAYOUT_FIELDS` (mode `toss`, secret key, security key);
  owner code `cOwnerCodeBody`; test secret key first; developer must check the payout API before live keys
  (`backend/crates/api/src/providers/payout.rs` header) — mark 확인 필요 as guide 02 §7 does.
- Sources: `guides['payout']`, `docs/guide/02` §7, `payments-reference.md`.
- Screenshots: `console-setup-payout`.

### 14. 문자 · 알림톡 (Solapi) · SMS and AlimTalk — `14-setup-solapi`
- 14.1 Solapi signup, sender number registration, API Key/Secret, sender digits only; fields `SOLAPI_FIELDS`.
- 14.2 Check = balance only, no SMS sent.
- 14.3 AlimTalk: Kakao business channel, pfId, the 7 templates (common body `#{body}`), template ids per event
  (`messageEventKeys` in `setup_guides.dart`), review wait — `docs/guide/06` in full.
- 14.4 Marketing SMS needs an 080 opt-out number (`cmNeedOptout`) — link to ch. 22.
- Screenshots: `console-setup-solapi`, `console-setup-solapi-templates`, `external-solapi-apikey`, `external-kakao-alimtalk-template`.

### 15. 간편 로그인과 앱 링크 · Social sign-in and app links — `15-setup-social-login`
- 15.1 Kakao (`KAKAO_FIELDS`: app ID, admin key optional; native app key goes to the app builder).
- 15.2 Naver (client ID + secret; secret only on the server card).
- 15.3 Google (client IDs, comma separated).
- 15.4 Apple (`APPLE_FIELDS`: client IDs/bundle IDs `kr.co.dahae.customer, kr.co.dahae.partner`, team ID, key ID, .p8).
- 15.5 Result "저장됨 · 앱에서 로그인해 확인하세요" (`cSetupCheckInApp`); test from the apps.
- 15.6 App links card (`setup_app_links.dart`): `cAppLinkTitle`, `cAppLinkHint`, `cAppLinkTeamLabel`,
  `cAppLinkAndroidLabel` (+ hint), `cAppLinkCheck`, on/off states, sources `cAppLinkSourceDb/Env`; `dahae applinks` (guide 07 §9).
- Sources: `guides['kakao'|'naver'|'google'|'apple']`, `docs/guide/02` §5, `docs/guide/11` §7.
- Screenshots: `console-setup-kakao`, `console-setup-naver`, `console-setup-google`, `console-setup-apple`, `console-setup-app-links`, `customer-login-social`.

### 16. 앱 푸시, AI 상담원, 주소 검색 · Push, AI assistant, address search — `16-setup-push-ai-juso`
- 16.1 Firebase push: project, service account, **새 비공개 키 생성**, paste the whole JSON (`PUSH_FIELDS`, `err_invalid_service_account`).
- 16.2 AI assistant (`guides['ai']`, `AI_FIELDS`, `localFields`: `aiCapThreadLabel`, `aiCapMemberLabel`, `aiCapDailyLabel`, `aiCapHint`);
  what it does in the Inbox (ch. 19), switch off via System status flag `oFlagAi`. Prices: none stated → provider price list.
- 16.3 Address search (도로명주소, `guides['juso']`, `confm_key`), what happens without it (확인 필요).
- Screenshots: `console-setup-push`, `console-setup-ai`, `console-setup-juso`.

### 17. 외부 백업, 업타임 하트비트, 심사용 데모 계정 · Offsite backup, heartbeat, review demo account — `17-setup-backup-heartbeat-review`
- 17.1 Offsite backup card `cIntBackup` (`BACKUP_FIELDS`): Cloudflare R2 steps and NCP Object Storage steps
  (endpoint, region `auto`/`kr-standard`, bucket, keys, retention default 30); check uploads/deletes a small file;
  encryption tied to `DATA_KEY` (keep keys backup).
- 17.2 Uptime heartbeat card `oHeartbeat` (`HeartbeatCard` in `host_controls.dart`): `cRbHeartbeatOff`/`oHeartbeatOn`,
  URL field `cRbHeartbeatUrl` (owner code, `heartbeat` job); healthchecks.io.
- 17.4 Business hours card (`BusinessHoursCard` in `setup_operator.dart`, `cRbHours*`), used by the AI offline mode.
- 17.3 Store review demo account card (`setup_review.dart`): `cReviewTitle`, `cReviewHint`, `cReviewEnable`,
  `cReviewPhone`, `cReviewCode`, `cReviewActiveUntil`, auto switch-off; link to ch. 25.
- Screenshots: `console-setup-backup`, `console-setup-heartbeat`, `console-setup-review-account`, `external-r2-token`.

## Part 4. 영업 운영 · Running the business

### 18. 서비스와 가격 · Catalog and prices — `18-catalog-prices`
- Tabs in `catalog.dart`: `cPricing` (services, `cgNewService` dialog, fixed vs quote price, `cActive`, archive, deposit,
  list price, minutes, surcharge), `cgTabGroups` (`catalog_groups.dart`: home menu groups, chooser types, move question),
  `cSlotCapacity`, `cFormula` (`f_*` estimate keys), `cgTabPeak` (`catalog_rules.dart`), `cgTabQuality` (thresholds),
  `cTabCancelPolicy` (`cancel_policy.dart`), `cgTabDemo` (`catalog_demo.dart`, sample data), count-priced items
  (`catalog_counts.dart`, `cntTitle`).
- Sources: `docs/sample-data.md`, `docs/guide/01` 체험용 데이터.
- Screenshots: `console-catalog-pricing`, `console-catalog-new-service`, `console-catalog-groups`, `console-catalog-capacity`, `console-catalog-formula`, `console-catalog-peak`, `console-catalog-quality`, `console-catalog-cancel-policy`, `console-catalog-counts`.

### 19. 대시보드, 예약, 견적, 상담 · Dashboard, bookings, quotes, inbox — `19-bookings-quotes-inbox`
- 19.1 Dashboard numbers (`dashboard.dart`) and the "매일 아침 5분" routine (`docs/guide/04`).
- 19.2 Bookings (`bookings.dart`): search/filter, detail `detailTitle`, `cAssign`/`cTeamName`, `cConfirm`, `cChangeStatus`,
  `cMarkDone`, `cCancelRefund` (`cRefundAmount`), statuses.
- 19.3 Quote desk: `cEstimateCalc`, `cFinalPrice`, `cQuoteNote`, `cSendQuote`, override approval (`cQuoteOverride`,
  `cQuoteNeedsAdmin`), partner quotes `cProQuotes`, resend/revise (`docs/guide/04` 견적).
- 19.4 Inbox (`inbox.dart`, `inbox_booking_card.dart`): customer/partner tabs, reply `cReplyHint`, `cOpenBooking`,
  AI badge, `aiPause`/`aiResume`, `aiOutageBanner`, `aiUsageMonth`.
- 19.5 Subscriptions (`subscriptions.dart`): cadence, `cgSubEdit`, pause/resume.
- Screenshots: `console-dashboard`, `console-bookings-list`, `console-booking-detail`, `console-booking-refund`, `console-quote-desk`, `console-inbox`, `console-inbox-ai`, `console-subscriptions`.

### 20. 파트너, 고객, 직원 · Partners, customers, staff — `20-partners-staff`
- 20.1 Partner approval (`people.dart` PartnersScreen, `member_approvals.dart`): statuses `cPro_*`, `cVerify`, `cSuspend`.
- 20.2 Partner profile and checklist (`people_profile.dart`: `cPfTitle`, checklist items, payee number with step-up,
  emergency contact, employment type).
- 20.3 Quality score and auto pause (`cQuality*`, ch. 18 thresholds, rules ch. 23).
- 20.4 Customers (`CustomersScreen`): search, points ledger (`points_ledger.dart`, `pointsAdjust*`, cap `rulePointsAdjustMax`).
- 20.5 Staff (`StaffScreen`, `staff_invite.dart`, `staff_code_dialogs.dart`, `staff_reset.dart`): invite by SMS or copy,
  roles admin/consultant, `cStaffInvitePending`, resend, reset password, activate/deactivate; staff 2FA (ch. 6).
- 20.6 Owner hand-over (`oaTransfer`, `oaClaim`) — link to ch. 8.
- Sources: `docs/guide/04` 파트너 관리, 직원.
- Screenshots: `console-partners-list`, `console-partner-profile-checklist`, `console-customers`, `console-points-ledger`, `console-staff-list`, `console-staff-invite`, `partner-signup`.

### 21. 결제, 환불, 정산, 재무 · Payments, refunds, settlements, finance — `21-payments-settlement-finance`
- 21.1 How a payment flows (deposit, balance, virtual account) — customer side `checkout.dart`, `pay_flow.dart`.
- 21.2 Refunds from a booking (ch. 19) and from cases (ch. 22); where a refund goes (first gateway, ch. 12).
- 21.3 Settlements (`SettlementsScreen`): tabs `poTabSettlements`/`poTabPayouts`, statuses `pS_*`, `cHold`/`cRelease`,
  `cRunPayouts`, manual transfer (`poShowAccount` with step-up, `poRevealNote`, `poConfirmPaid`, `poMarkFailed`,
  `payout_not_sent_dialog.dart` `poNotSent`), Toss (`poSend`, `poSync`, `poRetry`, `poCheckToss`, seller registration
  `payout_seller_dialog.dart` `poSellerAction`), withholding (`tTaxType`, `tWithholdingLabel`).
- 21.4 Finance (`finance.dart`): periods, KPIs, by service/partner, CSV exports `fExportLedger`, `fExportMonthly`,
  `fExportSettlements` (web console only), VAT estimate note.
- 21.5 Tax (`finance_tax.dart`): cash receipts, tax invoices queue and export, withholding CSV, quarterly VAT CSV,
  `tTaxLayoutNote` — `docs/finance-tax.md`, `docs/guide/04` 세금계산서.
- 21.6 System status payment checks and reconciliation (link ch. 26).
- Screenshots: `console-settlements`, `console-payout-account-reveal`, `console-payouts-toss`, `console-finance-overview`, `console-finance-exports`, `console-finance-tax`, `customer-checkout`.

### 22. 쿠폰, 콘텐츠, 성장, 캠페인, 사건 · Coupons, content, growth, campaigns, cases — `22-marketing-growth`
- Tabs in `marketing.dart`: `cIssueCoupon` (coupon name, amount, service, target phones or all), `cContent`
  (kinds `cKind_*`: notice, news, tip, FAQ, banner, legal with slug privacy/terms; `cNewContent`), `growthTab`
  (`growth.dart`: `welcomeOfferTitle`, `welcomePackTitle`, `referralSettingsTitle`, `reviewsModerationTitle`,
  `ReviewReportQueue` `modTitle`, `CaseQueue` `consoleCasesTitle` with `caseRefund`/`caseHold`/`caseReassign`,
  acquisition table), `cmTab` (`campaigns.dart`, `campaign_tab.dart`: audience segments, channel push/SMS,
  080 opt-out, consent count, test send, schedule), funnel `anFunnel` (`analytics_view.dart`).
- Sources: `docs/guide/04` 마케팅.
- Screenshots: `console-coupons`, `console-content-editor`, `console-growth-offers`, `console-review-reports`, `console-cases`, `console-campaign-new`, `console-funnel`.

### 23. 운영 규칙, 알림 문구, 기록, 개인정보, 갱신 · Rules, notification text, activity log, privacy, renewals — `23-rules-notify-privacy`
- 23.1 Operating rules (`rules.dart`): groups `rulesGroup*`, each `rule*` key, apply modes `rulesApplyInstant`/
  `rulesApplyRebuild`/`rulesApplyStoreManual`, source badges, range, reset, maintenance mode (`ruleMaintenanceOn`).
- 23.2 Notification text (`notify_templates.dart`): built-in vs custom, KO/EN, variables, preview, restore, errors `ntErr*`.
- 23.3 Activity log (`audit.dart`): filters who/what/period, detail.
- 23.4 Account deletion and data requests (`privacy_requests.dart`): find by phone, export CSV (step-up), delete with
  phone re-entry (admin only), log — `docs/RUNBOOK.md` 11, `docs/privacy-retention.md`.
- 23.5 Renewal calendar (`renewals.dart`): kinds `rnwKind*`, SMS 30 and 7 days before.
- Sources: `docs/guide/04` 운영 규칙, 활동 기록, 탈퇴·개인정보 요청.
- Screenshots: `console-rules`, `console-notify-templates`, `console-audit`, `console-privacy-requests`, `console-renewals`.

## Part 5. 고객 앱 · 파트너 앱 · 스토어 · Apps and stores

### 24. 고객 앱과 파트너 앱 둘러보기 · Customer and partner apps — `24-customer-partner-apps`
- 24.1 Customer: home and service groups (`home.dart`, `service_groups.dart`, `move_chooser.dart`), booking flow
  (`booking_flow.dart`, `booking_address.dart`, `pick.dart`), quote flow (`quote_flow.dart`, `quote_review.dart`),
  checkout and pay methods (`checkout.dart`, `pay_method_picker.dart`, `virtual_account.dart`), bookings and detail,
  cancel/reschedule (`cancel_booking_sheet.dart`, `reschedule_gate.dart`), chat, reviews, wallet/points, subscriptions,
  account/privacy/delete (`more.dart`, `privacy.dart`), guest mode (`guest.dart`).
- 24.2 Partner: sign-up and approval (`auth.dart`, `member.dart`), jobs and schedule (`jobs.dart`, `schedule.dart`,
  `job_detail.dart`, `navigate.dart`, `site_actions.dart`), completion photos (`complete.dart`), quotes (`quotes.dart`),
  earnings (`earnings.dart`), quality card, chat.
- 24.3 A test run the owner should do end to end (book → assign → complete → settle) — staging/test mode only.
- Labels from `app_ko.arb`/`app_en.arb` (customer/partner keys).
- Screenshots: `customer-home`, `customer-service`, `customer-booking-flow`, `customer-quote-flow`, `customer-checkout`, `customer-booking-detail`, `customer-chat`, `customer-more`, `partner-jobs`, `partner-job-detail`, `partner-complete`, `partner-earnings`, `partner-signup`.

### 25. 스토어에 앱 올리기 · Publishing to the stores — `25-store-release`
- 25.1 Who does what (`docs/guide/11` §1, `docs/guide/03` C): owner company owns Play/Apple accounts, developer builds.
- 25.2 Owner tasks: Google Play developer account (organization), Apple Developer (D-U-N-S, organization), invite the
  developer with the roles guide 11 §2/§5/§6 names; app IDs `kr.co.dahae.customer`, `kr.co.dahae.partner`.
- 25.3 What the developer does (`store-apps` workflow, upload key, Firebase) — summary, details in ch. 33.
- 25.4 Listing text and screenshots (`docs/store/listing-ko.md`, `listing-en.md`, `docs/store/screenshots/captions.json`),
  data safety and app privacy answers (`data-safety.md`, `app-privacy.md`, `play-declarations.md`).
- 25.5 Review: demo account card (ch. 17), review notes (`docs/store/review-notes.md` incl. §7 owner checklist),
  account deletion answer, payments answer.
- 25.6 After approval: put store links into the business card (ch. 10); updates (`docs/guide/03` 앱 업데이트); iOS
  (`docs/store/ios-release.md`), Microsoft Store (`microsoft-store.md`) only if docs say it is used (else 확인 필요).
- Sources: `docs/LAUNCH-PLAN.md` 2–3, `docs/guide/03`, `docs/guide/11`.
- Screenshots: `external-play-console-users`, `external-appstore-users`, `console-setup-review-account`.

## Part 6. 업데이트 · 백업 · 모니터링 · Updates, backups, monitoring

### 26. 시스템 상태, 알림, 업데이트 · System status, alerts, updates — `26-system-status-updates`
- 26.1 System status (`ops.dart`): `oOverall` levels green/yellow/red, `oChecks` (DB, disk, backup, errors, payments,
  webhook), `oIntegrations` per service, `oJobs` background jobs (`oJob_*`), uptime/version, actions `oAct*`.
- 26.2 Alerts (`oAlerts`): rule list `oRule*`, phone `oAlertPhone`, quiet hours `oQuietHours`, save; 6-hour de-dup and
  reconciliation note (`docs/guide/04` 시스템 상태). Thresholds live in Operating rules (ch. 23).
- 26.3 Feature switches and maintenance (`platform.dart`: `oFlags` booking/chat/payments/AI, `oMaintenance`,
  `oMaintenanceMessage`), app error reports `oClientErrors`.
- 26.4 Server update (`host_update.dart`): `hjTitle`, `hjBody`, **[지금 업데이트]** `hjUpdateNow`, job states `hjState*`,
  auto rollback; weekly auto update schedule (guide 00 끝!).
- 26.5 AWS: new versions come through the developer's approved GitHub deploy (`docs/guide/10` C, summary).
- Screenshots: `console-ops-overview`, `console-ops-integrations`, `console-ops-alerts`, `console-ops-flags`, `console-ops-update`, `console-ops-update-running`.

### 27. 백업과 복원 · Backups and restore — `27-backup-restore`
- 27.1 What is backed up and when (03:30, 14 days on server; offsite copy if ch. 17 set) — `docs/guide/08` 6-1–6-3.
- 27.2 Two layers on AWS (`docs/guide/10` D-1).
- 27.3 The keys backup and why restore fails without it (`DATA_KEY`, `/opt/malkkeum/.env`).
- 27.4 Restore: done by the installer/developer with `dahae restore` / `restore-offsite` (guide 07 §4, guide 02 §6);
  what the owner tells them; restore drill (guide 08 6-6) — monthly checklist item.
- 27.5 Moving to a new server (guide 08 8-2); changing domain now points to ch. 28.8 (console domain card).
- Screenshots: `console-ops-backup-check`, `aws-ssm-status`.

## Part 7. 브랜드 · Brand

### 28. 브랜드 바꾸기: 이름, 로고, 색, 도메인 · Rebranding: name, logo, colours and domain — `28-brand-today`
(2nd edition; file name kept so links stay valid. Source: `app/apps/console/lib/screens/setup_business.dart`,
`brand_assets.dart`, `brand_preview.dart`, `setup_store_build.dart`, `setup_domain.dart`, `setup_domain_checklist.dart`,
`host_controls.dart` (approval enrolment), labels `cRb*` in `app/tool/strings_rebrand_console.py`.)
- 28.1 What changes when (`cRbAppliedList`).
- 28.2 Names: service name, `cRbNameEn`, `cRbShortName` (12), `cRbTagline`, `cRbTaglineEn` (60).
- 28.3 Logo once; 7 slots made by the console; letter logo `cRbLetterMark`, `cRbNoLogoWarn`.
- 28.4 Colour (partner app: accent only). 28.5 Company details + `cRbPrivacyEmail`.
- 28.6 Save: preview `cRbPreviewTitle`, upload progress, `cRbAppliedTitle`; website and legal pages fill in live.
- 28.7 Store app build card (`cRbStore*`, owner ack).
- 28.8 Domain move: approval code enrolment (`cRbEnroll*`), DNS, check (`cRbDomainDns*`), change with owner code +
  approval code, rollback and refusals, the `cRbCheck_*` checklist with values from `domains.rs::checklist_items`,
  retire (`cRbRetire*`), `cRbServerMoved` banner.
- 28.9 Outside consoles and legal checks (`docs/REBRAND-CHECKLIST.md`). 28.10 What does not change.
- Screenshots: `console-setup-business-names`, `console-setup-business-letter-logo`, `console-brand-preview`,
  `console-brand-applied`, `console-setup-store-build`, `console-ops-approval-enrol`, `console-setup-domain-dns-ok`,
  `console-setup-domain-dns-bad`, `console-domain-approval-code`, `console-setup-domain-done`,
  `console-setup-domain-checklist`, `console-domain-retire-dialog`, `console-server-moved-banner`,
  `console-setup-business-color-warning`.

## Part 8. 문제 해결 · Troubleshooting

### 29. 자주 묻는 질문과 해결 · FAQ and fixes — `29-troubleshooting`
- Symptom table, owner-first wording, each with "누가 / who" and "어디서 / where":
  site not opening, "안전하지 않음", setup code lost/expired, locked out of 2FA (recovery codes), card locked,
  key rejected, webhook signature failures, payment needs review, payouts failing, SMS failing, push failing,
  social login failing, disk low, backup stale, after-update problems (rollback), suspected key leak.
- Sources: `docs/RUNBOOK.md` (all 11), `docs/guide/README.md` 급할 때, `docs/guide/00` 문제가 생기면, `docs/guide/08` 10,
  `docs/guide/09` 문제가 생기면, `docs/guide/10` C-4, `docs/guide/11` §11, `README.md` FAQ/Troubleshooting,
  `oAct*` keys in `ops.dart`, `setupMessage` codes.
- Screenshots: `console-ops-red`, `console-setup-test-invalid`.

## Part 9. 부록 · Appendices

### 30. 용어 풀이 (한영) · Glossary (KO/EN) — `30-glossary`
- Two-column KO/EN list: 콘솔, 소유자, 관리자, 상담원, 파트너, 설치 코드, 소유자 코드, 2단계 인증, 복구 코드, 웹훅, 채널 키,
  PG, 지급대행, 정산, 원천징수, 현금영수증, 세금계산서, 알림톡, 발신번호, A 레코드, DNS, HTTPS, CloudShell, IAM, MFA,
  CloudFormation 스택, 외부 백업, 하트비트, 점검 모드, 테스트 키/라이브 키, … — each term as the ARB/docs spell it.

### 31. 체크리스트 · Checklists — `31-checklists`
- Before launch (from `docs/guide/08` 9, `docs/store/review-notes.md` 7, `payments-setup.md` test items, ch. 9–17 cards).
- Daily (guide 04 매일 아침 5분), weekly (System status), monthly (restore drill, renewals, keys backup check
  `rnwKindKeysTest`, test refunds cleared), yearly renewals.

### 32. 보안 수칙 · Security rules — `32-security-rules`
- Never paste secrets in chat/email; password manager; no AWS access keys; owner code on the owner's phone; recovery
  codes on paper; who may unlock cards; step-up prompts; staff off-boarding; data requests.
- Sources: `docs/SECURITY-AND-PRIVACY.md`, `SECURITY.md`, `docs/guide/10` 보안 원칙, `docs/RUNBOOK.md` 10.

### 33. 개발자 부록 · Developer appendix — `33-developer-appendix`
- 33.1 `dahae` command reference (status, logs, backup, restore, restore-offsite, update, rollback, channel, heartbeat,
  applinks, setup-code, keys-backup, uninstall, others) — `docs/guide/07`, `deploy/install.sh`, `deploy/agent.sh`.
- 33.2 Environment: `/opt/malkkeum/.env` keys that matter (`DATA_KEY`, …) without values; `MALKKEUM_ANSWERS`
  unattended install; `TLS_MODE` (guide 08 4-4, 5-5); WAL archive (6-7).
- 33.3 AWS manual template build (guide 10 B-1/B-2 full), GitHub environment `doda-aws`, approved deploys,
  manual deploy/rollback, failures (C-1–C-4), costs and safe delete (D-3, D-4).
- 33.4 Releases and channels (`docs/DEPLOY.md`, `CHANGELOG.md`), demo install (guide 05).
- 33.5 Store builds (`docs/guide/11` full: `store` environment, upload key, Play/TestFlight upload, not-yet-verified items).
- 33.6 Payment integration reference (`payments-reference.md`), local dev (`README.md` 개발자용).
- Screenshots: none.
