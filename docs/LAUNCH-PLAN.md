# 출시 계획 (안드로이드 · iOS · 서버) / Launch plan (Android · iOS · server)

대상: 다해 운영사(DODA)와 개발팀. Audience: DODA (operator of the Dahae service) and the developer team.
세부 절차는 [03 앱 출시](guide/03-앱-출시.md), [08 클라우드 배포](guide/08-클라우드-배포.md), [01 서버 설치](guide/01-서버-설치.md), [store/ios-release.md](store/ios-release.md), [DEPLOY.md](DEPLOY.md) §1·§3·§4 에 있어요.

앱 ID (새 이름): 고객 `kr.co.dahae.customer`, 파트너 `kr.co.dahae.partner`, 콘솔 `kr.co.dahae.console`.
App IDs: customer `kr.co.dahae.customer`, partner `kr.co.dahae.partner`, console `kr.co.dahae.console`.

## 1. 질문 답변 요약 / Answers at a glance

| 질문 | 답 |
|---|---|
| 안드로이드 AAB 는 언제? | DODA 가 도메인 이름을 알려 주는 **그날** 보내요 (앱 두 개, AAB 파일 두 개). When DODA gives the domain: the same day. |
| iOS 는 어떻게? | 맥이 필요 없어요. 개발자가 Codemagic 클라우드 맥에서 빌드해 TestFlight 로 올려요. DODA 는 Apple 계정에 **관리자(Admin)** 로 개발자를 초대해 주세요. No Mac needed; DODA invites the developer as Admin. |
| 서버는 어떻게 배포? | 서울 리전 Ubuntu 서버 1대 + 도메인 + 설치 명령 한 줄. 키는 DODA 가 콘솔 설정 마법사에 직접 넣어요. One Seoul VM, one domain, one install command. |

## 2. 안드로이드 (Google Play) / Android

**AAB 준비 시점**
- 개발팀은 DODA 의 도메인을 받은 날 `API_BASE=https://<도메인>` 을 넣어 AAB 를 빌드해요 (`codemagic.yaml` 의 `customer-android`, `partner-android`). 빌드 자체는 서버가 떠 있지 않아도 돼요.
- 1차 AAB 는 카카오·네이버·구글·Firebase 키 없이 빌드해요. 해당 로그인 버튼은 숨겨지고 푸시는 꺼져요. 내부 테스트에는 충분해요.
- DODA 가 키를 보내면 다시 빌드해요 (약 1시간). 키 목록은 [DEPLOY.md](DEPLOY.md) §3 표.
- 심사 제출 전에는 서버가 반드시 떠 있어야 해요. 심사원은 데모 계정으로 로그인해요 (아래 §4-7).

**Play Console 계정 (DODA)**
1. play.google.com/console 에서 **조직(Organization)** 계정으로 가입해요. D-U-N-S 번호가 필요해요. 개인 계정은 새로 만들면 테스터 12명·14일 비공개 테스트를 거친 뒤에야 정식 출시할 수 있어요 ([03 앱 출시](guide/03-앱-출시.md) 준비물 표).
2. 개발자 이메일을 [사용자 및 권한]에 초대해요. 권한: **앱 출시**, **앱 정보 관리**.
3. 앱 두 개를 만들어요 (`kr.co.dahae.customer`, `kr.co.dahae.partner`).

**업로드와 서명**
- 업로드 키는 개발자가 만들고 오프라인 백업으로 보관해요 ([DEPLOY.md](DEPLOY.md) §3 항목 2).
- DODA 는 앱을 만들 때 **Play 앱 서명(Play App Signing)** 을 켜 두세요 (기본값).
- 빌드가 끝나면 Codemagic 이 AAB 를 Play 내부 테스트 트랙에 **초안(draft)** 으로 올려요. 올리려면 DODA 가 만든 Play 서비스 계정 키 (권한 **출시 관리자**, 키는 1:1 로 전달) 가 필요해요. DODA 는 Play Console 에서 초안을 확인하고 출시(승격)해요.
- 카카오·구글 로그인에는 **Play 앱 서명 키**의 SHA-1 이 필요해요. 첫 업로드 뒤 DODA 가 Play Console → 앱 무결성(App integrity) 에서 복사해 카카오·구글 콘솔에 등록해요. 업로드 키의 지문은 개발팀이 알려 줘요.

**업데이트**
- 새 버전마다 개발팀이 빌드 번호가 올라간 새 AAB 를 보내요. DODA 가 같은 방법으로 올려요. 빌드 번호는 자동으로 올라가요 (직접 정하지 않아요).

## 3. iOS (App Store · TestFlight)

**왜 DODA 가 맥을 준비할 필요가 없나**
- 빌드는 Codemagic 클라우드 맥에서 해요. 저장소는 비공개예요. DODA 는 맥이 필요 없어요 ([store/ios-release.md](store/ios-release.md) 1·2항).

**접근 방법 (비밀번호를 공유하지 않아요)**
- DODA 가 App Store Connect → [사용자 및 액세스]에서 개발자의 Apple ID 를 **관리자(Admin)** 로 초대해요. 비밀번호나 2단계 인증 코드는 주고받지 않아요.
- 관리자 역할이 있어야 개발자 사이트에서 App ID, APNs 키를 만들 수 있어요.
- 개발자는 초대를 수락한 뒤 다음을 해요: App ID 두 개 (`kr.co.dahae.customer`, `kr.co.dahae.partner`, 푸시·Apple 로그인·Associated Domains 켜기), App Store Connect 앱 두 개, App Store Connect API 키 (역할 **App Manager**, Codemagic 에 `malkkeum_asc` 이름으로 연결), APNs 키 (푸시용).
- 개발자는 Apple 팀 ID 를 받아 서버 `.env` 의 `APPLE_TEAM_ID` 에 넣어요 (`dahae applinks --apple-team-id`, [DEPLOY.md](DEPLOY.md) §3).

**DODA 가 직접 해야 할 것 (계정 소유자만 가능)**
1. Apple Developer Program 을 **조직**으로 가입해요. D-U-N-S 번호가 필요하고, 연 99 USD 예요.
2. 계약(Agreements)에 동의해요. 계정 소유자(Account Holder)만 할 수 있어요.
3. 세금·정산 정보는 앱이 유료가 아니므로 기본적으로 필요 없어요. 결제는 PortOne 웹 결제이고 인앱 결제(IAP)가 아니에요.
4. TestFlight 내부 테스트는 DODA 직원이 해요. 개발자는 설치 링크만 보내요.
5. 심사 제출은 사람이 해요 (자동 제출하지 않아요). DODA 가 직접 누르거나, 개발자에게 권한을 주고 개발자가 눌러도 돼요. 심사 메모는 [store/review-notes.md](store/review-notes.md).

## 4. 백엔드 서버 배포 / How we plan to deploy the backend server

DODA 가 고르는 방식이에요. 아래는 권장안이에요.

1. **서버 1대 (VM)**: Ubuntu 24.04 LTS, 한국 **서울** 리전. 권장 2 vCPU / 메모리 4 GB / 디스크 50 GB (최소 메모리 1 GB, 디스크 여유 5 GB). 고정 IP 를 연결해요. 제공사 예: 네이버 클라우드 서버, AWS Lightsail (서울) 등 ([guide 08](guide/08-클라우드-배포.md) §2~3). 가격은 제공사 요금표를 따르고, 관리형 DB 는 쓰지 않아요 (DB 는 서버 안 컨테이너).
2. **방화벽**: 22 (SSH, 가능하면 개발팀 IP 만), 80, 443 만 열어요. 클라우드 방화벽 규칙도 함께 설정해요 ([DEPLOY.md](DEPLOY.md) §1 "서버 밖" 항목 2).
3. **도메인**: DODA 가 소유한 도메인 (국내 등록업체에서 구입 가능). 루트 도메인과 `www` 의 A 레코드 두 개를 서버 IP 로 연결해요 ([guide 08](guide/08-클라우드-배포.md) §4). 클라우드플레어를 쓴다면 프록시(주황색 구름)는 꺼요.
4. **설치 명령 한 줄**: 서버 SSH 에서 설치 명령을 실행해요 ([guide 08](guide/08-클라우드-배포.md) §5, [guide 01](guide/01-서버-설치.md)). 설치 프로그램이 서명된 이미지를 내려받고, PostgreSQL, Caddy(자동 HTTPS), 방화벽, 자동 백업을 설정해요.
5. **설정 마법사**: 사장님 계정을 만들고 콘솔 [설정]에서 결제(PortOne), 문자(Solapi), 푸시(Firebase), 간편 로그인 키를 직접 넣어요 ([guide 02](guide/02-설정-마법사.md)). **키는 개발팀에 보내지 않아요.**
6. **백업과 키 사본**: 외부 백업은 S3 호환 저장소를 권장해요 ([guide 02](guide/02-설정-마법사.md) §6). `dahae keys-backup` 으로 만든 키 사본을 서버 밖(USB·비밀번호 관리자)에 보관해요 ([guide 08](guide/08-클라우드-배포.md) §6-5). 키를 잃으면 외부 백업을 열 수 없어요.
7. **데모 계정**: 심사원은 한국 문자를 받을 수 없어요. 심사를 제출하기 직전에 콘솔 [설정] → [스토어 심사용 데모 계정]을 켜고, 번호와 코드를 개발팀에 알려 줘요. 14일 뒤 자동으로 꺼져요 ([guide 03](guide/03-앱-출시.md) D).

**누가 설치하나**
- DODA 관리자가 직접 하거나, DODA 가 허락하면 개발팀이 SSH 로 설치해요.
- 권장: 첫 설치는 개발팀과 DODA 담당자가 통화하면서 함께 해요.

**업데이트**
- 같은 도메인에서 설치 명령을 다시 실행하면 데이터를 그대로 두고 최신 버전으로 올라가요. 실패하면 이전 버전으로 되돌려요 ([guide 08](guide/08-클라우드-배포.md) §8-1).
- 자동 업데이트 요일·시각은 `sudo dahae update-schedule` 로 정해요. 운영 명령은 [guide 07](guide/07-운영-명령어.md).
- 서버를 옮길 때는 옛 서버에서 백업과 키 사본을 만들어 새 서버로 옮겨요 ([guide 08](guide/08-클라우드-배포.md) §8-2, 약 1시간).

## 5. 일정 / Timeline

| 날 · Day | 누가 · Who | 할 일 · Task |
|---|---|---|
| Day 0 | DODA | 도메인 정하기, 서울 서버 만들기 (고정 IP, 22/80/443), Play 조직 계정 가입, Apple 조직 가입, Apple 관리자 초대 |
| Day 0 | 개발팀 | 도메인을 받으면 AAB 2개 빌드 (`API_BASE`) |
| Day 0–1 | 개발팀 + DODA | 서버 설치 (§4), 설정 마법사, 사장님 계정 |
| Day 1 | 개발팀 | App ID, App Store Connect 앱, API 키, APNs 키, Codemagic 연결, 첫 TestFlight 빌드 (Apple 처리 10–30분) |
| Day 1 | DODA | Codemagic 이 올린 AAB 초안을 Play 내부 테스트 트랙에서 확인·출시, Play 앱 서명 SHA-1 등록 |
| Day 1–3 | DODA 직원 | 안드로이드·iOS 실기기 테스트 (로그인, 사진, 결제 창, 푸시, 위치) |
| 심사 제출 시 | DODA + 개발팀 | 데모 계정 켜기, 심사 메모, 개인정보처리방침·계정 삭제 주소 확인, 제출 |
| 심사 기간 | 스토어 | Google 은 보통 며칠, Apple 은 보통 1–3일 (일반적인 기간이며 보장되지 않아요) |

## 6. DODA 가 보내 줄 것 / What DODA sends us

- [ ] 도메인 이름 (예: `myshop.co.kr`)
- [ ] 서버 SSH 접속 권한, 또는 설치 날 통화 가능 시간
- [ ] Apple 관리자 초대 (개발자 Apple ID 에 Admin)
- [ ] Play Console 개발자 초대 (앱 출시, 앱 정보 관리)
- [ ] Play 서비스 계정 키 (JSON, 권한 **출시 관리자**): Codemagic 의 Play 자동 업로드에 필요해요. 1:1 로 전달해요
- [ ] 키 (선택, 있을 때만): 카카오 네이티브 키, 네이버 클라이언트 ID·시크릿, 구글 OAuth 클라이언트 ID (웹·iOS), Firebase 설정 파일 (고객·파트너, Android·iOS)
- 키 파일과 비밀번호는 카카오톡 단체방이 아니라 1:1 로 보내요 ([guide 03](guide/03-앱-출시.md) 전달 방법).

## 7. 개발팀이 보내 줄 것 / What we send DODA

- [ ] AAB 파일 두 개: `kr.co.dahae.customer`, `kr.co.dahae.partner` (안드로이드)
- [ ] TestFlight 설치 안내 (iOS)
- [ ] 스토어 설명 문구 `docs/store/listing-ko.md`, `listing-en.md`
- [ ] 스크린샷 `docs/store/screenshots/`
- [ ] 데이터 안전 답변 `docs/store/data-safety.md` (Google), `docs/store/app-privacy.md` (Apple)
- [ ] 심사 메모 `docs/store/review-notes.md`
- [ ] 서버 설치 명령과 설치 안내 ([guide 00](guide/00-한번에-설치.md), [guide 01](guide/01-서버-설치.md))

---

## English

Audience: DODA (operator of the Dahae service) and the developer team. Step-by-step details: [guide 03](guide/03-앱-출시.md), [guide 08](guide/08-클라우드-배포.md), [guide 01](guide/01-서버-설치.md), [store/ios-release.md](store/ios-release.md), [DEPLOY.md](DEPLOY.md) §1, §3, §4.

### 1. Answers at a glance

| Question | Answer |
|---|---|
| When is the Android AAB ready? | The same day DODA gives us the domain name. Two files: customer and partner. |
| How does iOS work? | No Mac needed. The developer builds on Codemagic cloud Macs and uploads to TestFlight. DODA invites the developer as Admin in App Store Connect; no password or 2FA code is shared. |
| How is the server deployed? | One Ubuntu 24.04 VM in Seoul, one domain, one install command. DODA enters its own payment and messaging keys in the console setup wizard. |

### 2. Android (Google Play)

- **AAB timing.** The developer builds both AABs with `API_BASE=https://<domain>` on the day the domain is known (`codemagic.yaml`, workflows `customer-android` and `partner-android`). The server does not need to be running for the build.
- **First build without keys.** Kakao, Naver, Google sign-in buttons stay hidden and push stays off. This is enough for internal testing. When DODA sends the keys, we rebuild (about 1 hour). Key list: [DEPLOY.md](DEPLOY.md) §3.
- **Server before review.** The server must be running before DODA submits for Play review, because reviewers log in with the demo account (section 4, item 7).
- **Play Console account (DODA).** Register as an **Organization** (a D-U-N-S number is needed). A new personal account must run a closed test with 12 testers for 14 days before production. Invite the developer under Users and permissions: App release and App information management.
- **Upload and signing.** The developer creates the upload key and keeps an offline backup ([DEPLOY.md](DEPLOY.md) §3 item 2). DODA keeps **Play App Signing** on (the default). Codemagic uploads each AAB to the internal testing track as a draft, using a Play service account key from DODA (role **Release manager**). DODA checks the draft and promotes it.
- **Sign-in SHA-1.** Kakao and Google sign-in need the **Play app-signing key** SHA-1. After the first upload, DODA copies it from Play Console → App integrity and registers it in the Kakao and Google consoles. The developer provides the upload key fingerprint.
- **Updates.** Each new version is a new AAB with a higher build number. DODA uploads it the same way. Build numbers are set by the build script, never by hand.

### 3. iOS (App Store · TestFlight)

- **No Mac for DODA.** Builds run on Codemagic cloud Macs from a private repository ([store/ios-release.md](store/ios-release.md)).
- **Access.** DODA invites the developer's Apple ID in App Store Connect → Users and Access with the **Admin** role. The developer needs Admin to create App IDs and APNs keys. No password or 2FA code is shared.
- **Developer does.** Creates App IDs `kr.co.dahae.customer` and `kr.co.dahae.partner` with Push, Sign in with Apple and Associated Domains enabled; creates the two App Store Connect apps; creates an App Store Connect API key (role App Manager, named `malkkeum_asc` in Codemagic); creates the APNs key; sets the Apple Team ID in the server `.env` (`APPLE_TEAM_ID`, [DEPLOY.md](DEPLOY.md) §3).
- **DODA must do (account holder only where noted).**
  1. Enrol in the Apple Developer Program as an **organization**: D-U-N-S number needed, 99 USD per year.
  2. Accept the agreements (Account Holder only).
  3. Tax and banking are not needed for free apps. Payments use PortOne web checkout, not in-app purchase.
  4. TestFlight internal testing is done by DODA staff.
  5. Submit for review: a person presses "Submit for Review" (not automated). DODA presses it, or the developer does if DODA grants access. Review notes: [store/review-notes.md](store/review-notes.md).

### 4. Backend server: deployment plan

DODA chooses what it is comfortable with. This is the recommended plan.

1. **One VM.** Ubuntu 24.04 LTS in the **Seoul** region. Recommended 2 vCPU / 4 GB RAM / 50 GB disk (minimum 1 GB RAM, 5 GB free disk). Attach a static IP. Examples: Naver Cloud Server, AWS Lightsail (Seoul) ([guide 08](guide/08-클라우드-배포.md) §2–3). Prices follow the provider's price list. No managed database: PostgreSQL runs in a container on the same server.
2. **Firewall.** Open only 22 (SSH, ideally from the developer's IP), 80 and 443, in the cloud firewall as well ([DEPLOY.md](DEPLOY.md) §1, "Outside the server", item 2).
3. **Domain.** A domain DODA owns. Add two A records, one for the bare domain and one for `www`, pointing at the server IP ([guide 08](guide/08-클라우드-배포.md) §4). If Cloudflare is used, turn off the orange-cloud proxy.
4. **One install command.** Run the install line over SSH ([guide 08](guide/08-클라우드-배포.md) §5, [guide 01](guide/01-서버-설치.md)). The installer pulls signed images and sets up PostgreSQL, Caddy with automatic HTTPS, the firewall and automatic backups.
5. **Setup wizard.** The owner creates the owner account and enters PortOne, Solapi, Firebase and sign-in keys in the console ([guide 02](guide/02-설정-마법사.md)). **Keys are never sent to the developer.**
6. **Backups and key copy.** Offsite backup to an S3-compatible bucket is recommended ([guide 02](guide/02-설정-마법사.md) §6). Save the key copy from `dahae keys-backup` outside the server ([guide 08](guide/08-클라우드-배포.md) §6-5). Without the key copy, offsite backups cannot be opened.
7. **Demo account.** Reviewers cannot receive Korean SMS. Turn on the review demo account in the console just before submitting, and send the number and code to the developer. It turns off automatically after 14 days ([guide 03](guide/03-앱-출시.md) D).

**Who runs the install.** DODA's administrator, or the developer over SSH with DODA's permission. Recommended: the first install is done together on a call with the developer and a DODA contact.

**Updates.** Re-running the install line on the same domain keeps data and moves to the latest version; a failed update rolls back ([guide 08](guide/08-클라우드-배포.md) §8-1). Schedule with `sudo dahae update-schedule`; commands in [guide 07](guide/07-운영-명령어.md). Moving to a new server: back up on the old one, copy the key file, restore on the new one, about 1 hour ([guide 08](guide/08-클라우드-배포.md) §8-2).

### 5. Timeline

| Day | Who | Task |
|---|---|---|
| Day 0 | DODA | Pick domain, create Seoul server (static IP, ports 22/80/443), register Play organization account, enrol in Apple Developer Program, invite developer as Apple Admin |
| Day 0 | Developer | Build both AABs once the domain is known |
| Day 0–1 | Developer + DODA | Server install (section 4), setup wizard, owner account |
| Day 1 | Developer | App IDs, App Store Connect apps, API key, APNs key, Codemagic link, first TestFlight build (Apple processing 10–30 min) |
| Day 1 | DODA | Check and promote the Codemagic AAB draft on the Play internal track; register the Play app-signing SHA-1 |
| Day 1–3 | DODA staff | Test on real Android and iPhone devices (sign-in, photos, payment window, push, location) |
| At submission | DODA + developer | Turn on demo account, review notes, confirm privacy and account-deletion URLs, submit |
| Review | Stores | Google usually takes a few days; Apple usually 1–3 days. Typical, not guaranteed. |

### 6. What DODA sends us

- [ ] Domain name (e.g. `myshop.co.kr`)
- [ ] SSH access to the server, or a time slot for a call on install day
- [ ] Apple Admin invite for the developer's Apple ID
- [ ] Play Console developer invite (App release, App information management)
- [ ] Play service account key (JSON, role **Release manager**): needed for Codemagic's automatic Play upload; sent one-to-one
- [ ] Keys, if available: Kakao native key, Naver client ID and secret, Google OAuth client IDs (web and iOS), Firebase config files (customer and partner, Android and iOS)
- Send keys and passwords one-to-one, never in a group chat ([guide 03](guide/03-앱-출시.md)).

### 7. What we send DODA

- [ ] AAB files: `kr.co.dahae.customer`, `kr.co.dahae.partner` (Android)
- [ ] TestFlight invitation instructions (iOS)
- [ ] Store listing text: `docs/store/listing-ko.md`, `listing-en.md`
- [ ] Screenshots: `docs/store/screenshots/`
- [ ] Data safety answers: `docs/store/data-safety.md` (Google), `docs/store/app-privacy.md` (Apple)
- [ ] Review notes: `docs/store/review-notes.md`
- [ ] Install command and guides ([guide 00](guide/00-한번에-설치.md), [guide 01](guide/01-서버-설치.md))
