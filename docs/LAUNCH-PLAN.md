# 출시 계획 (안드로이드 · iOS · 서버) / Launch plan (Android · iOS · server)

대상: 다해 운영사(DODA)와 개발팀. Audience: DODA (operator of the Dahae service) and the developer team.
세부 절차는 [03 앱 출시](guide/03-앱-출시.md), [08 클라우드 배포](guide/08-클라우드-배포.md), [01 서버 설치](guide/01-서버-설치.md), [store/ios-release.md](store/ios-release.md), [DEPLOY.md](DEPLOY.md) §1·§3·§4 에 있어요.

앱 ID (새 이름): 고객 `kr.co.dahae.customer`, 파트너 `kr.co.dahae.partner`, 콘솔 `kr.co.dahae.console`.
App IDs: customer `kr.co.dahae.customer`, partner `kr.co.dahae.partner`, console `kr.co.dahae.console`.

## 1. 질문 답변 요약 / Answers at a glance

| 질문 | 답 |
|---|---|
| 안드로이드 AAB 는 누가 만들어요? | **DODA 가 직접 빌드해요.** 도메인이 정해지면 [guide 11](guide/11-앱-직접-빌드.md) 대로 DODA 의 Codemagic 과 Play Console 에서 AAB 두 개를 만들어요. 개발팀에 보내는 파일은 없어요. DODA builds both AABs itself, following guide 11. |
| iOS 는 어떻게? | 맥이 필요 없어요. DODA 가 자기 Apple 계정과 Codemagic 으로 빌드해 TestFlight 로 올려요. 개발자 초대는 없어요. No Mac needed; DODA builds and uploads with its own Apple account and Codemagic. No developer invitation. |
| 서버는 어떻게 배포? | AWS 서울 리전에 CloudFormation 템플릿으로 서버 1대를 만들고, 새 버전은 GitHub Actions 가 승인 후 배포해요 (CD). 키는 DODA 가 콘솔 설정 마법사에 직접 넣어요. AWS Seoul via one CloudFormation template; GitHub Actions deploys each release after approval. |

## 2. 안드로이드 (Google Play) / Android

**AAB 준비 시점**
- DODA 가 도메인을 정한 날 `API_BASE=https://<도메인>` 을 넣고 [guide 11](guide/11-앱-직접-빌드.md) 대로 `customer-android`, `partner-android` 를 실행해요 (`codemagic.yaml`). 빌드 자체는 서버가 떠 있지 않아도 돼요.
- 1차 AAB 는 카카오·네이버·구글·Firebase 키 없이 빌드해요. 해당 로그인 버튼은 숨겨지고 푸시는 꺼져요. 내부 테스트에는 충분해요.
- DODA 가 키를 Codemagic 에 넣으면 다시 빌드해요 (약 1시간). 키 목록은 [DEPLOY.md](DEPLOY.md) §3 표와 guide 11 의 3-3.
- 심사 제출 전에는 서버가 반드시 떠 있어야 해요. 심사원은 데모 계정으로 로그인해요 (아래 §4-6).

**Play Console 계정 (DODA)**
1. play.google.com/console 에서 **조직(Organization)** 계정으로 가입해요. D-U-N-S 번호가 필요해요. 개인 계정은 새로 만들면 테스터 12명·14일 비공개 테스트를 거친 뒤에야 정식 출시할 수 있어요 ([03 앱 출시](guide/03-앱-출시.md) 준비물 표).
2. 개발팀 초대는 없어요. DODA 가 Play 서비스 계정을 만들어 **출시 관리자** 권한을 주고 ([guide 11](guide/11-앱-직접-빌드.md) 4-3), 앱 두 개를 직접 만들어요 (`kr.co.dahae.customer`, `kr.co.dahae.partner`).

**업로드와 서명**
- 업로드 키는 DODA 가 만들고 오프라인 백업으로 보관해요 ([guide 11](guide/11-앱-직접-빌드.md) 3-2). 개발팀의 키를 받지 않아요.
- DODA 는 앱을 만들 때 **Play 앱 서명(Play App Signing)** 을 켜 두세요 (기본값).
- 첫 AAB 는 DODA 가 Play Console 에 직접 올려요 (Google Play API 로는 첫 릴리스를 만들 수 없어요, [guide 11](guide/11-앱-직접-빌드.md) 6번). 이후 빌드는 Codemagic 이 서비스 계정 키(`GCLOUD_SERVICE_ACCOUNT_CREDENTIALS`)로 Play 내부 테스트 트랙에 **초안(draft)** 으로 올려요. DODA 는 Play Console 에서 초안을 확인하고 출시(승격)해요.
- 카카오·구글 로그인에는 **Play 앱 서명 키**의 SHA-1 이 필요해요. 첫 업로드 뒤 DODA 가 Play Console → 앱 무결성(App integrity) 에서 복사해 카카오·구글 콘솔에 등록해요.

**업데이트**
- 새 버전은 DODA 가 포크에서 새 태그를 만들고 Codemagic 을 다시 실행해요 ([guide 11](guide/11-앱-직접-빌드.md) 8번). 빌드 번호는 자동으로 올라가요 (직접 정하지 않아요).

## 3. iOS (App Store · TestFlight)

**왜 DODA 가 맥을 준비할 필요가 없나**
- 빌드는 Codemagic 클라우드 맥에서 해요. DODA 는 맥이 필요 없어요 ([store/ios-release.md](store/ios-release.md) 1·2항).

**계정 (DODA 가 직접 만들어요, 개발자 초대는 없어요)**
- DODA 계정 소유자가 Apple Developer 와 App Store Connect 에서 App ID 두 개 (`kr.co.dahae.customer`, `kr.co.dahae.partner`, 푸시·Apple 로그인·Associated Domains 켜기), 앱 두 개, APNs 키를 만들어요 ([guide 11](guide/11-앱-직접-빌드.md) 4-1, 4-2).
- App Store Connect API 키 (역할 **App Manager**) 는 Codemagic 에 `malkkeum_asc` 이름으로 연결해요 ([guide 11](guide/11-앱-직접-빌드.md) 3-1).
- Apple 팀 ID 는 서버 콘솔 **설정 → 앱 링크** 카드에 넣어요 ([guide 11](guide/11-앱-직접-빌드.md) 4-8).

**DODA 가 직접 해야 할 것 (계정 소유자만 가능)**
1. Apple Developer Program 을 **조직**으로 가입해요. D-U-N-S 번호가 필요하고, 연 99 USD 예요.
2. 계약(Agreements)에 동의해요. 계정 소유자(Account Holder)만 할 수 있어요.
3. 세금·정산 정보는 앱이 유료가 아니므로 기본적으로 필요 없어요. 결제는 PortOne 웹 결제이고 인앱 결제(IAP)가 아니에요.
4. TestFlight 내부 테스트는 DODA 직원이 해요.
5. 심사 제출은 사람이 해요 (자동 제출하지 않아요). DODA 가 직접 눌러요. 심사 메모는 [store/review-notes.md](store/review-notes.md).

## 4. 백엔드 서버 배포 / How we plan to deploy the backend server

DODA 의 서비스가 모두 AWS 에 있으니 **AWS 서울 리전(ap-northeast-2)** 에 올려요. 클릭 순서 전체는 [guide 10](guide/10-AWS-배포.md) 에 있어요.

1. **DODA: 배포용 IAM 사용자 만들기.** 관리자 권한 대신 [`deploy/aws/deployer-iam-policy.json`](../deploy/aws/deployer-iam-policy.json) 정책만 붙이고 MFA 를 켜요. 로그인 주소와 사용자 이름은 비밀번호와 따로, 1:1 로 보내요.
2. **DODA: CloudFormation 템플릿 한 번 실행 ([guide 10](guide/10-AWS-배포.md) B).** `dahae-aws.yaml` 을 CloudFormation 에 올리고 도메인·이메일·VPC·서브넷을 고르면 만들어져요 (템플릿 출처는 아래 "확인 필요" 참고):
   - Ubuntu 24.04 서버 1대 (기본 t3.medium, 디스크 50 GB 암호화), 고정 IP (Elastic IP), 80/443 만 열린 보안 그룹 (SSH 22 는 기본으로 닫혀 있고, 접속은 브라우저의 Session Manager 로 해요)
   - 설치 프로그램 자동 실행 (SHA-256 로 확인한 install.sh): PostgreSQL, Caddy(자동 HTTPS), 방화벽, 자동 백업
   - 매일 디스크 스냅샷 (7일 보관), AWS 하드웨어 고장 시 자동 복구, 실수로 지우지 못하게 종료 보호
   - Route 53 을 쓰면 도메인 A 레코드까지 자동. 다른 등록업체면 출력된 IP 로 A 레코드 두 개 (`@`, `www`)
   - 관리형 DB(RDS)는 쓰지 않아요. DB 는 서버 안 컨테이너예요.
3. **설정 마법사.** 사장님 계정을 만들고 콘솔 [설정]에서 결제(PortOne), 문자(Solapi), 푸시(Firebase), 간편 로그인 키를 직접 넣어요 ([guide 02](guide/02-설정-마법사.md)). **키는 개발팀에 보내지 않아요.**
4. **CD (GitHub Actions).** 새 버전이 릴리스되면 `deploy-aws.yml` 이 AWS Systems Manager 로 서버에 `dahae update <버전>` 을 보내고, 끝나면 `https://<도메인>/health` 로 확인해요. DODA 포크의 워크플로로 돌아요 ([guide 10](guide/10-AWS-배포.md) C).
   - GitHub 환경 `doda-aws` 의 **승인자(required reviewers)** 가 승인해야만 배포돼요. 버전 지정 배포와 되돌리기도 Actions 화면 버튼으로 해요.
   - AWS 접속은 GitHub OIDC 역할로 해요. GitHub 에 AWS 키나 비밀번호를 저장하지 않아요. 역할은 이 서버에 명령을 보내는 일만 할 수 있어요.
   - CD 로 보낸 업데이트는 콘솔의 인증 앱 승인 대신 GitHub 승인자가 승인해요.
5. **백업과 키 사본.** 외부 백업은 S3 버킷을 권장해요 ([guide 02](guide/02-설정-마법사.md) §6). 키 사본을 서버 밖에 꼭 보관해요 ([guide 08](guide/08-클라우드-배포.md) §6-5). 키를 잃으면 외부 백업을 열 수 없어요.
6. **데모 계정.** 심사원은 한국 문자를 받을 수 없어요. 심사 제출 직전에 DODA 가 콘솔 [설정] → [스토어 심사용 데모 계정]을 켜고, 번호와 코드를 심사 메모에 직접 적어요. 개발팀에 보내지 않아요. 14일 뒤 자동으로 꺼져요 ([guide 03](guide/03-앱-출시.md) D).

**확인되지 않은 것.** 템플릿과 CD 는 AWS 검사 도구(cfn-lint, actionlint)를 통과했지만 실제 AWS 계정에서는 아직 띄워 보지 않았어요. 첫 배포는 DODA 담당자가 해요. 개발팀은 DODA 가 요청할 때만 도와요.

**템플릿과 새 버전:** `dahae-aws.yaml` 은 다해 저장소의 릴리스 페이지(Releases → v1.5.1 → Assets)에서 받아요. 설치 스크립트 주소와 지문이 이미 채워져 있어요. 새 버전도 다해 저장소에서 릴리스되고, DODA 포크의 `deploy-aws` 는 **Run workflow** 에 버전을 넣어 설치해요 (포크에서는 릴리스 작업이 돌지 않아서 자동 시작은 없어요).

**AWS 가 아닌 곳에 올릴 때**는 [guide 09](guide/09-클라우드별-설치.md) (클라우드별 설치) 와 [guide 08](guide/08-클라우드-배포.md) 을 봐요.

## 5. 일정 / Timeline

| 날 · Day | 누가 · Who | 할 일 · Task |
|---|---|---|
| Day 0 | DODA | 도메인 정하기, AWS 배포용 IAM 사용자 만들기, Play 조직 계정 가입, Apple 조직 가입 |
| Day 0–1 | DODA | CloudFormation 으로 서버 만들기 (§4), 설정 마법사, 사장님 계정, GitHub CD 연결 |
| Day 1 | DODA | 포크, Codemagic 설정, App ID, App Store Connect 앱, API 키, APNs 키, 첫 AAB 빌드와 Play 첫 업로드, 첫 TestFlight 빌드 (Apple 처리 10–30분) ([guide 11](guide/11-앱-직접-빌드.md)) |
| Day 1 | DODA | Play 내부 테스트 트랙에서 초안 확인·출시, Play 앱 서명 SHA-1 등록 |
| Day 1–3 | DODA 직원 | 안드로이드·iOS 실기기 테스트 (로그인, 사진, 결제 창, 푸시, 위치) |
| 심사 제출 시 | DODA | 데모 계정 켜기, 심사 메모, 개인정보처리방침·계정 삭제 주소 확인, 제출 |
| 심사 기간 | 스토어 | Google 은 보통 며칠, Apple 은 보통 1–3일 (일반적인 기간이며 보장되지 않아요) |

## 6. DODA 가 보내 줄 것 / What DODA sends us

- 없어요. DODA 는 개발팀에 아무것도 보내지 않아요. AWS 배포용 IAM 사용자, GitHub 승인자, 앱 계정, 서비스 계정 키, 앱 키, 도메인 설정은 DODA 가 직접 만들고 직접 넣어요 ([guide 10](guide/10-AWS-배포.md), [guide 11](guide/11-앱-직접-빌드.md)).

## 7. 개발팀이 보내 줄 것 / What we send DODA

- [ ] 서면 라이선스 계약 (저장소 `LICENSE` §1.4·§3.1). 계약 전에는 DODA 가 포크해서 빌드하면 안 돼요 ([guide 11](guide/11-앱-직접-빌드.md) 0번).
- [ ] 스토어 이미지 묶음 `dahae-store-images-1.5.1.zip` 을 저장소 GitHub Release (v1.5.1) 페이지에 첨부 ([guide 11](guide/11-앱-직접-빌드.md) 6번).
- 앱 빌드 파일과 키는 보내지 않아요. 스토어 자료 (설명 문구, 스크린샷, 데이터 안전, 개인정보 라벨, 심사 메모) 는 저장소의 `docs/store/` 에 있어요.

---

## English

Audience: DODA (operator of the Dahae service) and the developer team. Step-by-step details: [guide 03](guide/03-앱-출시.md), [guide 08](guide/08-클라우드-배포.md), [guide 01](guide/01-서버-설치.md), [store/ios-release.md](store/ios-release.md), [DEPLOY.md](DEPLOY.md) §1, §3, §4.

### 1. Answers at a glance

| Question | Answer |
|---|---|
| Who builds the Android AAB? | **DODA builds it itself.** Once the domain is set, DODA follows [guide 11](guide/11-앱-직접-빌드.md) to build two AABs on its own Codemagic and Play Console. Nothing is sent to the developer. |
| How does iOS work? | No Mac needed. DODA builds with its own Apple account and Codemagic, and uploads to TestFlight. No developer invitation. |
| How is the server deployed? | One server in AWS Seoul from a CloudFormation template; GitHub Actions deploys each new release after a DODA reviewer approves (CD). DODA enters its own payment and messaging keys in the console setup wizard. |

### 2. Android (Google Play)

- **AAB timing.** DODA builds both AABs with `API_BASE=https://<domain>` once the domain is set (`codemagic.yaml`, workflows `customer-android` and `partner-android`, steps in [guide 11](guide/11-앱-직접-빌드.md)). The server does not need to be running for the build.
- **First build without keys.** Kakao, Naver, Google sign-in buttons stay hidden and push stays off. This is enough for internal testing. When DODA adds the keys to Codemagic, it rebuilds (about 1 hour). Key list: [DEPLOY.md](DEPLOY.md) §3 and guide 11 step 3-3.
- **Server before review.** The server must be running before DODA submits for Play review, because reviewers log in with the demo account (section 4, item 6).
- **Play Console account (DODA).** Register as an **Organization** (a D-U-N-S number is needed). A new personal account must run a closed test with 12 testers for 14 days before production. No developer invitation: DODA creates the apps itself.
- **Upload and signing.** DODA creates the upload key and keeps an offline backup (guide 11, step 3-2). DODA keeps **Play App Signing** on (the default). The first AAB is uploaded by hand in Play Console, because the Google Play API cannot create the first release (guide 11, step 6). After that, Codemagic uploads each AAB to the internal testing track as a draft, using a Play service account key that DODA creates (role **Release manager**). DODA checks the draft and promotes it.
- **Sign-in SHA-1.** Kakao and Google sign-in need the **Play app-signing key** SHA-1. After the first upload, DODA copies it from Play Console → App integrity and registers it in the Kakao and Google consoles.
- **Updates.** DODA tags a new version on its fork and runs Codemagic again (guide 11, step 8). Build numbers are set by the build script, never by hand.

### 3. iOS (App Store · TestFlight)

- **No Mac for DODA.** Builds run on Codemagic cloud Macs ([store/ios-release.md](store/ios-release.md)).
- **Accounts (DODA creates them; no developer invitation).** The account holder creates App IDs `kr.co.dahae.customer` and `kr.co.dahae.partner` with Push, Sign in with Apple and Associated Domains enabled; creates the two App Store Connect apps and the APNs key (guide 11, steps 4-1 and 4-2). An App Store Connect API key (role App Manager) is connected in Codemagic as `malkkeum_asc` (guide 11, step 3-1). The Apple Team ID goes into the server console's App links card (guide 11, step 4-8).
- **DODA must do (account holder only where noted).**
  1. Enrol in the Apple Developer Program as an **organization**: D-U-N-S number needed, 99 USD per year.
  2. Accept the agreements (Account Holder only).
  3. Tax and banking are not needed for free apps. Payments use PortOne web checkout, not in-app purchase.
  4. TestFlight internal testing is done by DODA staff.
  5. Submit for review: a person presses "Submit for Review" (not automated). DODA presses it. Review notes: [store/review-notes.md](store/review-notes.md).

### 4. Backend server: deployment plan

All of DODA's services run on AWS, so the server goes to **AWS Seoul (ap-northeast-2)**. Full click path: [guide 10](guide/10-AWS-배포.md).

1. **DODA: create a deployer IAM user.** Attach only [`deploy/aws/deployer-iam-policy.json`](../deploy/aws/deployer-iam-policy.json) (no administrator access) and turn on MFA. Send the sign-in URL and user name one-to-one, separately from the password.
2. **DODA: run the CloudFormation template once** ([guide 10](guide/10-AWS-배포.md) B). Upload `dahae-aws.yaml` to CloudFormation, pick domain, email, VPC and subnet (where the template comes from: see "Not yet verified" below). It creates:
   - One Ubuntu 24.04 server (default t3.medium, 50 GB encrypted disk), an Elastic IP, a security group open only on 80/443 (SSH 22 closed by default; shell access through Session Manager in the browser)
   - The installer, run automatically and checked by SHA-256: PostgreSQL, Caddy with automatic HTTPS, firewall, automatic backups
   - Daily disk snapshots kept 7 days, auto-recovery on AWS hardware failure, termination protection
   - The domain A record when Route 53 is used; otherwise two A records (`@`, `www`) at the registrar to the output IP
   - No RDS: PostgreSQL runs in a container on the server.
3. **Setup wizard.** The owner creates the owner account and enters PortOne, Solapi, Firebase and sign-in keys in the console ([guide 02](guide/02-설정-마법사.md)). **Keys are never sent to the developer.**
4. **CD with GitHub Actions.** On each release, `deploy-aws.yml` sends `dahae update <version>` to the server through AWS Systems Manager, then checks `https://<domain>/health`. It runs in DODA's fork ([guide 10](guide/10-AWS-배포.md) C).
   - It deploys only after a **required reviewer** of the GitHub environment `doda-aws` approves. Deploying a chosen version and rolling back are buttons in the Actions tab.
   - AWS access uses a GitHub OIDC role: no AWS keys or passwords are stored in GitHub, and the role can only send commands to this one server.
   - An update sent by CD is approved by the GitHub reviewer instead of the console's authenticator code.
5. **Backups and key copy.** Offsite backup to an S3 bucket is recommended ([guide 02](guide/02-설정-마법사.md) §6). Keep the key copy outside the server ([guide 08](guide/08-클라우드-배포.md) §6-5); without it, offsite backups cannot be opened.
6. **Demo account.** Reviewers cannot receive Korean SMS. DODA turns on the review demo account in the console just before submitting and enters the number and code in the review notes itself. Nothing is sent to the developer. It turns off after 14 days ([guide 03](guide/03-앱-출시.md) D).

**Not yet verified.** The template and CD pass the AWS linters (cfn-lint, actionlint) but have not been launched on a real AWS account. The first deploy is done by DODA staff. The developer helps only when DODA asks.

**Template and new versions:** download `dahae-aws.yaml` from the Dahae repo's release page (Releases → v1.5.1 → Assets); the install script URL and SHA-256 are already filled in. New versions are released in the Dahae repo too, and DODA's fork installs them with **Run workflow** on `deploy-aws` and the version (the release job does not run in the fork, so nothing starts automatically).

**Other clouds:** [guide 09](guide/09-클라우드별-설치.md) and [guide 08](guide/08-클라우드-배포.md).

### 5. Timeline

| Day | Who | Task |
|---|---|---|
| Day 0 | DODA | Pick domain, create the AWS deployer IAM user, register Play organization account, enrol in Apple Developer Program |
| Day 0–1 | DODA | Create the server with CloudFormation (section 4), setup wizard, owner account, connect GitHub CD |
| Day 1 | DODA | Fork, Codemagic setup, App IDs, App Store Connect apps, API key, APNs key, first AAB builds and first manual Play upload, first TestFlight build (Apple processing 10–30 min) ([guide 11](guide/11-앱-직접-빌드.md)) |
| Day 1 | DODA | Check and promote the AAB draft on the Play internal track; register the Play app-signing SHA-1 |
| Day 1–3 | DODA staff | Test on real Android and iPhone devices (sign-in, photos, payment window, push, location) |
| At submission | DODA | Turn on demo account, review notes, confirm privacy and account-deletion URLs, submit |
| Review | Stores | Google usually takes a few days; Apple usually 1–3 days. Typical, not guaranteed. |

### 6. What DODA sends us

- None. DODA sends nothing to the developer. The AWS deployer IAM user, the GitHub approver, app accounts, the Play service account key, app keys and domain settings are created and entered by DODA itself ([guide 10](guide/10-AWS-배포.md), [guide 11](guide/11-앱-직접-빌드.md)).

### 7. What we send DODA

- [ ] A signed written license agreement (repository `LICENSE` sections 1.4 and 3.1). Before it is signed, DODA must not fork and build ([guide 11](guide/11-앱-직접-빌드.md) step 0).
- [ ] The store images zip `dahae-store-images-1.5.1.zip`, attached to the GitHub Release (v1.5.1) of the repository ([guide 11](guide/11-앱-직접-빌드.md) step 6).
- App build files and keys are not sent. Store material (listing text, screenshots, data safety, app privacy labels, review notes) is in `docs/store/` in the repository.
