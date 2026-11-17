# 출시 계획 (안드로이드 · iOS · 서버) / Launch plan (Android · iOS · server)

대상: 다해 운영사(DODA)와 개발자(Eric). Audience: DODA (operator of the Dahae service) and the developer (Eric).
세부 절차는 [03 앱 출시](guide/03-앱-출시.md), [10 AWS 배포](guide/10-AWS-배포.md), [11 스토어 앱 빌드](guide/11-앱-직접-빌드.md), [store/ios-release.md](store/ios-release.md), [DEPLOY.md](DEPLOY.md) §1·§3·§4 에 있어요.

앱 ID (새 이름): 고객 `kr.co.dahae.customer`, 파트너 `kr.co.dahae.partner`, 콘솔 `kr.co.dahae.console`.
App IDs: customer `kr.co.dahae.customer`, partner `kr.co.dahae.partner`, console `kr.co.dahae.console`.

**원칙.** 개발자(Eric)가 다해 전체를 자기 GitHub 저장소 [EricWijesinghe/Dahae](https://github.com/EricWijesinghe/Dahae) 에서 빌드하고 배포하고 유지보수해요. DODA 는 사업 계정 (Apple 조직, Play 조직, AWS, PortOne, Solapi, 카카오, Firebase 등) 을 소유하고 개발자에게 관리자 권한을 줘요. 개발자가 한국에서 사업자 계정을 소유할 수 없어서 (D-2 비자) 이 구조예요. DODA 는 기술 작업을 하지 않아요. DODA 에 추가 서비스 비용은 없어요. (도메인만 예외: 개발자의 Cloudflare 계정에 등록해요.)

Principle. The developer (Eric) builds, deploys and maintains everything from his GitHub repository. DODA owns every business account and grants the developer admin access, because the developer cannot own business accounts (D-2 visa). DODA does no technical work. No extra paid service for DODA. (One exception: the domain is registered in the developer's Cloudflare account.)

## 1. 질문 답변 요약 / Answers at a glance

| 질문 | 답 |
|---|---|
| 안드로이드 AAB 는 누가 만들어요? | **개발자가 GitHub Actions 로 만들어요.** 워크플로 `store-apps` (수동 실행, 안드로이드는 ubuntu 러너). 만든 AAB 는 DODA 의 Play Console 에 올라가요. The developer builds the AAB with GitHub Actions; it goes to DODA's Play Console. |
| iOS 는 어떻게? | 같은 워크플로의 macOS 러너 (공개 저장소라 무료). 빌드는 DODA 의 App Store Connect (TestFlight) 로 올라가요. Built on a macOS runner in the same workflow; uploaded to DODA's App Store Connect. |
| 서버는 어떻게 배포? | DODA 의 AWS 서울 리전에 CloudFormation 템플릿 `dahae-aws.yaml` (릴리스 첨부 파일) 로 서버 1대를 만들어요. 새 버전은 개발자 저장소의 `deploy-aws.yml` 이 배포해요. DODA 가 원하면 DODA 담당자가 승인해요. The server runs on DODA's AWS Seoul account from the release template; the developer's `deploy-aws.yml` deploys new versions, optionally approved by a DODA reviewer. |

## 2. 안드로이드 (Google Play) / Android

**빌드 시점**
- `store` 환경에 `API_BASE=https://<도메인>` 을 넣고 `store-apps` 를 `android` 로 실행해요 ([guide 11](guide/11-앱-직접-빌드.md) 3·8번). 빌드 자체는 서버가 떠 있지 않아도 돼요.
- 키 (카카오·네이버·구글·Firebase) 가 없으면 해당 로그인 버튼은 숨겨지고 푸시는 꺼져요. 내부 테스트에는 충분해요. 키를 추가하면 다시 빌드해요.
- 심사 제출 전에는 서버가 반드시 떠 있어야 해요. 심사원은 데모 계정으로 로그인해요 (아래 §4-6).

**Play Console 계정 (DODA 조직, 개발자 관리자)**
1. DODA 가 play.google.com/console 에서 **조직** 계정으로 가입해요 (D-U-N-S 번호 필요). 개인 계정은 테스터 12명·14일 비공개 테스트를 거쳐야 정식 출시할 수 있어요 ([03 앱 출시](guide/03-앱-출시.md) 준비물 표).
2. DODA 가 개발자의 Google 계정을 **관리자**로 초대해요. 개발자가 앱 두 개 (`kr.co.dahae.customer`, `kr.co.dahae.partner`) 를 만들고 서비스 계정을 연결해요 ([guide 11](guide/11-앱-직접-빌드.md) 5번).

**업로드와 서명**
- 업로드 키는 개발자가 만들고 두 곳에 오프라인으로 백업해요 ([guide 11](guide/11-앱-직접-빌드.md) 4번). 잃으면 Play 업데이트가 막혀요.
- **Play 앱 서명 (Play App Signing)** 은 켜 두세요 (기본값).
- 첫 AAB 는 Play Console 에 사람이 직접 올려요 (Google Play API 로는 첫 릴리스를 만들 수 없어요, [guide 11](guide/11-앱-직접-빌드.md) 5번 2항). 이후 빌드는 `store-apps` 가 서비스 계정 키 (`GOOGLE_PLAY_SERVICE_ACCOUNT_JSON`, 출시 관리자 권한) 로 내부 테스트 트랙에 **초안(draft)** 으로 올려요. 개발자가 초안을 확인하고 출시(승격)해요.
- 카카오·구글 로그인에는 **Play 앱 서명 키**의 SHA-1 이 필요해요. 첫 업로드 뒤 Play Console → 앱 무결성에서 복사해 카카오·구글 콘솔에 등록해요. 같은 화면의 SHA-256 은 콘솔 설정 → 앱 링크 카드에 넣어요.

**업데이트**
- 개발자가 새 태그 (`v1.5.2`) 를 만들고 `store-apps` 를 다시 실행해요 ([guide 11](guide/11-앱-직접-빌드.md) 10번). 빌드 번호는 자동으로 올라가요.

## 3. iOS (App Store · TestFlight)

**계정 (DODA 조직, 개발자는 관리자)**
- DODA 계정 소유자가 Apple Developer Program 을 **조직**으로 가입해요 (D-U-N-S 번호 필요, 연 99 USD). 계약에 동의하는 일은 계정 소유자만 할 수 있어요.
- DODA 가 개발자의 Apple ID 를 **Admin** 멤버로 초대해요.
- 개발자가 App ID 두 개 (`kr.co.dahae.customer`, `kr.co.dahae.partner`, 푸시·Apple 로그인·Associated Domains 켜기), App Store Connect 앱 두 개, APNs 키, App Store Connect API 키를 만들어요 ([guide 11](guide/11-앱-직접-빌드.md) 6번).
- App Store Connect API 키 (역할 **Admin**. App Manager 키는 자동 서명 내보내기에서 "Cloud signing permission error" 로 실패해요) 는 GitHub 환경 `store` 의 `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_P8_B64` 에 넣어요.
- Apple 팀 ID 는 서버 콘솔 **설정 → 앱 링크** 카드에 넣어요 ([guide 11](guide/11-앱-직접-빌드.md) 6번 6항).

**DODA 가 하는 것 (계정 소유자만 가능한 것)**
1. Apple Developer Program 조직 가입, D-U-N-S 번호, 연 99 USD 결제.
2. 계약 (Agreements) 동의.
3. App Store Connect → 사용자 및 액세스 → 통합 → App Store Connect API → **액세스 요청** (계정 소유자만, 한 번). 이게 없으면 개발자가 API 키를 만들 수 없어요.
4. 세금·정산 정보는 앱이 유료가 아니므로 기본적으로 필요 없어요. 결제는 PortOne 웹 결제이고 인앱 결제(IAP)가 아니에요.
5. 개발자에게 Admin 권한 부여.

**심사**
- 심사 제출은 사람이 해요 (자동 제출하지 않아요). 개발자가 Admin 권한으로 제출해요. DODA 직원도 같은 권한으로 제출할 수 있어요. 심사 메모는 [store/review-notes.md](store/review-notes.md).

## 4. 백엔드 서버 배포 / How we plan to deploy the backend server

DODA 의 서비스가 모두 AWS 에 있으니 **DODA 의 AWS 서울 리전(ap-northeast-2)** 에 올려요. 계정은 DODA 것이고, 개발자는 접근 권한으로 배포해요. 클릭 순서 전체는 [guide 10](guide/10-AWS-배포.md) 에 있어요.

1. **DODA: 배포용 IAM 사용자 만들기.** 먼저 권한 한도 정책 `dahae-boundary` 를 만들어요 (누구에게도 붙이지 않아요, [guide 10](guide/10-AWS-배포.md) A 3번). 그다음 관리자 권한 대신 [`deploy/aws/deployer-iam-policy.json`](../deploy/aws/deployer-iam-policy.json) 정책만 붙이고 MFA 를 켜요. 액세스 키는 만들지 않아요. 로그인 주소와 사용자 이름은 비밀번호와 따로 개발자에게 전달해요.
2. **개발자: CloudFormation 템플릿 한 번 실행** (배포용 IAM 사용자로 로그인, [guide 10](guide/10-AWS-배포.md) B). 개발자가 B-1 에서 만든 `dahae-aws.yaml` 을 CloudFormation 에 올리고, 도메인·이메일·VPC·서브넷을 고르면 만들어져요. `GitHubRepo` 는 기본값 `EricWijesinghe/Dahae` 를 그대로 둬요 (개발자 저장소의 배포 워크플로만 이 서버에 배포할 권한을 받아요):
   - Ubuntu 24.04 서버 1대 (기본 t3.medium, 디스크 50 GB 암호화), 고정 IP (Elastic IP), 80/443 만 열린 보안 그룹 (SSH 22 는 기본으로 닫혀 있고, 접속은 브라우저의 Session Manager 로 해요)
   - 설치 프로그램 자동 실행 (SHA-256 로 확인한 install.sh): PostgreSQL, Caddy (자동 HTTPS), 방화벽, 자동 백업
   - 매일 디스크 스냅샷 (7일 보관), AWS 하드웨어 고장 시 자동 복구, 실수로 지우지 못하게 종료 보호
   - Route 53 을 쓰면 도메인 A 레코드까지 자동. 다른 등록업체면 출력된 IP 로 A 레코드 두 개 (`@`, `www`)
   - 관리형 DB(RDS)는 쓰지 않아요. DB 는 서버 안 컨테이너예요.
3. **설정 마법사 (DODA 관리자).** 사장님 계정을 만들고 콘솔 [설정]에서 결제(PortOne), 문자(Solapi), 푸시(Firebase), 간편 로그인 키를 직접 넣어요 ([guide 02](guide/02-설정-마법사.md)). 사장님이 [직원]에서 개발자에게 관리자 계정을 만들어 주면 문자·푸시·로그인 카드는 개발자가 처음 저장할 수 있어요. PortOne·토스 카드, 잠금 해제, 저장 뒤 연결 확인은 사장님 계정만 돼요. 키는 저장소나 채팅에 올리지 않아요.
4. **CD (GitHub Actions, 개발자 저장소).** 새 버전 태그가 릴리스되면 개발자 저장소의 `deploy-aws.yml` 이 AWS Systems Manager 로 서버에 `dahae update <버전>` 을 보내고, `https://<도메인>/health` 로 확인해요 ([guide 10](guide/10-AWS-배포.md) C).
   - GitHub 환경 `doda-aws` 에 승인자(required reviewers)를 넣으면 승인 후에만 배포돼요. DODA 가 승인하고 싶으면 DODA 담당자를 승인자로 넣어요 (그 사람은 개발자 저장소 접근 권한이 있어야 해요). 넣지 않으면 개발자가 Actions 에서 바로 실행해요.
   - AWS 접속은 GitHub OIDC 역할로 해요. GitHub 에 AWS 키나 비밀번호를 저장하지 않아요. 역할은 이 서버에 명령을 보내는 일만 할 수 있어요.
   - 버전 지정 배포와 되돌리기도 Actions 화면 버튼으로 해요.
5. **백업과 키 사본.** 외부 백업은 S3 버킷을 권장해요 ([guide 02](guide/02-설정-마법사.md) §6). 키 사본을 서버 밖에 꼭 보관해요 ([guide 08](guide/08-클라우드-배포.md) §6-5). 키를 잃으면 외부 백업을 열 수 없어요.
6. **데모 계정.** 심사원은 한국 문자를 받을 수 없어요. 심사 제출 직전에 DODA 관리자가 콘솔 [설정] → [스토어 심사용 데모 계정]을 켜고, 번호와 코드를 심사 메모에 적어요. 14일 뒤 자동으로 꺼져요 ([guide 03](guide/03-앱-출시.md) D).

**확인되지 않은 것.** 템플릿과 CD 는 AWS 검사 도구(cfn-lint, actionlint)를 통과했지만 실제 AWS 계정에서는 아직 띄워 보지 않았어요. 첫 배포 때 DODA 관리자와 개발자가 같이 확인해요.

**템플릿과 새 버전:** v1.5.1 릴리스에 첨부된 `dahae-aws.yaml` 은 권한 한도와 GitHub 로그인 주체 수정 전 파일이라 쓰지 않아요. 개발자가 `master` 에서 v1.5.1 설치 스크립트로 새로 만들어요 ([guide 10](guide/10-AWS-배포.md) B-1). 다음 릴리스부터는 릴리스 페이지(Releases → 최신 버전(Latest) → Assets)의 파일을 그대로 써요. 설치 스크립트 주소와 지문이 이미 채워져 있어요. 새 버전도 같은 저장소에서 릴리스돼요. 릴리스 작업이 `deploy-aws` 를 자동으로 시작해요 (승인자가 있으면 승인 후 배포).

**AWS 가 아닌 곳에 올릴 때**는 [guide 09](guide/09-클라우드별-설치.md) (클라우드별 설치) 와 [guide 08](guide/08-클라우드-배포.md) 을 봐요.

## 5. 일정 / Timeline

| 날 · Day | 누가 · Who | 할 일 · Task |
|---|---|---|
| Day 0 | DODA | 라이선스 계약 서명, 도메인 정하기, AWS 배포용 IAM 사용자 만들기, Play 조직 계정 가입, Apple 조직 가입, App Store Connect API 액세스 요청, 개발자를 Apple 과 Play 관리자로 초대, Firebase·카카오·PortOne·Solapi 계정 가입 |
| Day 0 | 개발자 | 서면 라이선스 계약 확인, 개발자 저장소 `store` 환경과 `doda-aws` 환경 만들기 |
| Day 0–1 | 개발자 · DODA 사장님 | CloudFormation 으로 서버 만들기 (개발자, §4), 사장님 계정과 설정 마법사 (사장님, 개발자가 통화로 안내) |
| Day 1 | 개발자 | App ID, App Store Connect 앱, API 키, APNs 키, GitHub 시크릿 입력, 첫 `store-apps` 실행, 첫 AAB 의 Play 수동 업로드, 첫 TestFlight 빌드 (Apple 처리 10–30분) ([guide 11](guide/11-앱-직접-빌드.md)) |
| Day 1 | 개발자 | Play 내부 테스트 트랙의 초안 확인·출시, Play 앱 서명 SHA-1 등록, SHA-256 을 콘솔 앱 링크에 입력 |
| Day 1–3 | DODA 직원 · 개발자 | 안드로이드·iOS 실기기 테스트 (로그인, 사진, 결제 창, 푸시, 위치) |
| 심사 제출 시 | DODA 관리자 · 개발자 | 데모 계정 켜기, 심사 메모, 개인정보처리방침·계정 삭제 주소 확인, 제출 |
| 심사 기간 | 스토어 | Google 은 보통 며칠, Apple 은 보통 1–3일 (일반적인 기간이며 보장되지 않아요) |

## 6. DODA 가 제공하는 것 / What DODA provides

DODA 는 빌드, 서명, 배포 같은 기술 작업을 하지 않아요. 아래 계정, 계약, 권한만 제공해요. 계정은 DODA 명의로 만들고, 개발자에게 관리자 권한을 줘요. 사장님이 [직원]에서 개발자에게 관리자 계정을 만들어 주면 문자·푸시·로그인 카드는 개발자가 처음 저장할 수 있어요. PortOne·토스 카드, 잠금 해제, 저장 뒤 연결 확인은 사장님 계정만 돼요.

| 항목 | DODA 가 하는 일 | 이유 |
|---|---|---|
| Apple Developer Program (조직) | 조직 가입, D-U-N-S, 계약 동의, App Store Connect API 액세스 요청, 개발자 Apple ID 를 Admin 으로 초대 | 개발자는 D-2 비자라 사업자 계정을 소유할 수 없어요 |
| Google Play Console (조직) | 조직 가입, 개발자 Google 계정을 관리자로 초대 | 같은 이유 |
| AWS 계정 | 계정 개설 (결제 수단 등록), 배포용 IAM 사용자 1명 (정책만, MFA) | 서버 비용과 데이터가 DODA 명의에 있어요 |
| 도메인 | 고객 주소로 쓸 도메인 정하기 | 고객이 들어갈 주소예요. 등록은 개발자의 Cloudflare 계정으로 해요 |
| 결제 (PortOne, PG, 토스 지급대행) 계약 | 계약. PortOne·토스 카드는 **사장님(소유자) 계정만** 처음 저장·잠금 해제할 수 있어요 (인증 앱 코드). 개발자는 통화로 안내해요 | 정산 명의는 DODA 예요 |
| 문자 (Solapi) 발신번호와 계정 | 계정과 발신번호 등록, 키 입력 | 발신번호는 사업자 명의가 필요해요 |
| 카카오 비즈니스 채널 (알림톡), 카카오·네이버·구글 개발자 앱 | 등록, 키 발급 권한 | 사업자 명의 등록이 필요한 곳이 있어요 |
| Firebase | 프로젝트 소유, 개발자에게 접근 권한 | 푸시 키 |
| 사장님 계정 | 콘솔 설정 마법사에서 사장님 계정 만들기, 인증 앱 설정 | 서버 운영 계정 |
| 심사용 데모 계정 | 심사 제출 직전 콘솔에서 켜기 | 번호와 코드는 DODA 가 정해요 |
| 라이선스 계약 | 서면 라이선스 계약 서명 ([양식](legal/COMMERCIAL-LICENSE-AGREEMENT.md)) | 서명 전에는 개발자가 빌드·배포하지 않아요 (§7) |

## 7. 개발자가 제공하는 것 / What the developer provides

- [ ] 서면 라이선스 계약 (저장소 `LICENSE` §1.4·§3.1). 계약 전에는 DODA 를 위해 빌드하거나 배포하지 않아요 ([guide 11](guide/11-앱-직접-빌드.md) 0번).
- [ ] 개발자 GitHub 저장소의 `store` 환경 (시크릿: 키스토어, Play 서비스 계정, App Store Connect API 키, 네이티브 키) 과 `doda-aws` 환경 (AWS 변수 네 개) ([guide 11](guide/11-앱-직접-빌드.md) 3번, [guide 10](guide/10-AWS-배포.md) C).
- [ ] 두 앱의 스토어 빌드, 업로드, 첫 AAB 수동 업로드, TestFlight 확인, 심사 제출 준비.
- [ ] 서버 릴리스와 배포 (`deploy-aws`), 유지보수, 장애 대응.
- [ ] 스토어 자료 (설명 문구, 스크린샷, 데이터 안전, 개인정보 라벨, 심사 메모) 는 저장소 `docs/store/` 에 있어요. 스크린샷과 그래픽은 저장소에 올리지 않고, 개발자가 릴리스에 `dahae-store-images-<버전>.zip` 으로 첨부해요 (`gh release upload`).

앱 빌드 파일과 키를 DODA 에 보내지 않아요. DODA 가 빌드를 받을 일도 없어요.

---

## English

Audience: DODA (operator of the Dahae service) and the developer (Eric). Step-by-step details: [guide 03](guide/03-앱-출시.md), [guide 10](guide/10-AWS-배포.md), [guide 11](guide/11-앱-직접-빌드.md), [store/ios-release.md](store/ios-release.md), [DEPLOY.md](DEPLOY.md) §1, §3, §4.

**Principle.** The developer builds, deploys and maintains everything for DODA from his GitHub repository [EricWijesinghe/Dahae](https://github.com/EricWijesinghe/Dahae). DODA owns every business account (Apple organization, Play organization, AWS, PortOne, Solapi, Kakao, Firebase) and gives the developer admin access, because the developer cannot own business accounts (D-2 visa). DODA does no technical work. DODA pays no extra service.

### 1. Answers at a glance

| Question | Answer |
|---|---|
| Who builds the Android AAB? | **The developer, with the GitHub Actions workflow `store-apps`** (manual; ubuntu runner for Android). The AAB goes to DODA's Play Console. |
| How does iOS work? | The same workflow on a macOS runner (free on the public repository). The IPA goes to DODA's App Store Connect (TestFlight). |
| How is the server deployed? | One server in DODA's AWS Seoul account from the release template `dahae-aws.yaml`; the developer's `deploy-aws.yml` deploys each new version, optionally approved by a DODA reviewer. |

### 2. Android (Google Play)

- **Build timing.** Set `API_BASE=https://<domain>` in the `store` environment and run `store-apps` with `android` ([guide 11](guide/11-앱-직접-빌드.md) steps 3 and 8). The server does not need to be running for the build.
- **Without keys.** Kakao, Naver and Google sign-in buttons stay hidden and push stays off. Enough for internal testing. Add the keys and rebuild later.
- **Server before review.** The server must be running before review, because reviewers log in with the demo account (section 4, item 6).
- **Play Console (DODA organization, developer as administrator).** DODA registers as an **Organization** (D-U-N-S needed). A new personal account must run a closed test with 12 testers for 14 days first. DODA invites the developer's Google account as an administrator; the developer creates the two apps and the service account ([guide 11](guide/11-앱-직접-빌드.md) step 5).
- **Upload and signing.** The developer creates the upload key and keeps two offline copies (guide 11, step 4). Play App Signing stays on (default). The first AAB is uploaded by hand, because the Play API cannot create the first release (guide 11, step 5.2). After that, `store-apps` uploads each AAB to the internal track as a **draft**, with the service account key (Release manager). The developer checks and promotes it.
- **Sign-in SHA-1.** Kakao and Google sign-in need the Play app-signing SHA-1. After the first upload, copy it from Play Console → App integrity and register it in the Kakao and Google consoles. Put the SHA-256 from the same page into the console under Settings → App links.
- **Updates.** Tag a new version and run `store-apps` again (guide 11, step 10). Build numbers are automatic.

### 3. iOS (App Store · TestFlight)

- **Accounts.** DODA enrolls as an organization (D-U-N-S, 99 USD per year) and invites the developer's Apple ID as **Admin**. The developer creates App IDs `kr.co.dahae.customer` and `kr.co.dahae.partner` (Push, Sign in with Apple, Associated Domains), the two App Store Connect apps, the APNs key and the App Store Connect API key, role **Admin** (cloud signing needs it) (guide 11, step 6). The API key goes into the `store` environment secrets. The Apple Team ID goes into the server console under App links.
- **DODA must do** (account holder only where noted): enroll as an organization, accept the agreements (Account Holder only), request App Store Connect API access (Users and Access → Integrations → App Store Connect API, Account Holder only, once; without it the developer cannot create the API key), and grant admin access. Tax and banking are not needed for free apps. Payments use PortOne web checkout, not in-app purchase.
- **Submission.** A person presses "Submit for Review" (not automated). The developer submits with admin access; DODA staff may do the same. Review notes: [store/review-notes.md](store/review-notes.md).

### 4. Backend server: deployment plan

All of DODA's services run on AWS, so the server goes to **DODA's AWS Seoul region (ap-northeast-2)**. The account belongs to DODA; the developer deploys with granted access. Full click path: [guide 10](guide/10-AWS-배포.md).

1. **DODA: create a deployer IAM user.** First create the permissions-boundary policy `dahae-boundary` and attach it to nobody ([guide 10](guide/10-AWS-배포.md) A, step 3). Then attach only [`deploy/aws/deployer-iam-policy.json`](../deploy/aws/deployer-iam-policy.json), turn on MFA, create no access keys. Send the sign-in URL and user name to the developer separately from the password.
2. **Developer: run the CloudFormation template once**, signed in as the deployer IAM user ([guide 10](guide/10-AWS-배포.md) B). Use the `dahae-aws.yaml` the developer builds in B-1. Keep `GitHubRepo` at its default `EricWijesinghe/Dahae`: only the developer's deploy workflow in that repository is trusted by the server's AWS role. It creates:
   - One Ubuntu 24.04 server (default t3.medium, 50 GB encrypted disk), an Elastic IP, a security group open only on 80/443 (SSH 22 closed by default; shell access through Session Manager in the browser)
   - The installer, run automatically and checked by SHA-256: PostgreSQL, Caddy with automatic HTTPS, firewall, automatic backups
   - Daily disk snapshots kept 7 days, auto-recovery on AWS hardware failure, termination protection
   - The domain A record when Route 53 is used; otherwise two A records (`@`, `www`) at the registrar to the output IP
   - No RDS: PostgreSQL runs in a container on the server.
3. **Setup wizard (DODA admin).** The owner creates the owner account and enters PortOne, Solapi, Firebase and sign-in keys in the console ([guide 02](guide/02-설정-마법사.md)). Once the owner creates an admin account for the developer under Staff, the developer can make the first save of the SMS, push and sign-in cards. The PortOne and Toss cards, unlocking, and the connection check after saving work only from the owner account. Keys are never committed or sent in chat.
4. **CD with GitHub Actions (developer repository).** On each release, `deploy-aws.yml` sends `dahae update <version>` through AWS Systems Manager, then checks `https://<domain>/health` ([guide 10](guide/10-AWS-배포.md) C).
   - Add required reviewers to the environment `doda-aws` if DODA wants to approve deploys (the reviewer then needs access to the developer's repository). Without reviewers the developer runs it directly.
   - AWS access uses a GitHub OIDC role: no AWS keys or passwords are stored in GitHub; the role can only send commands to this server.
5. **Backups and key copy.** Offsite backup to an S3 bucket is recommended ([guide 02](guide/02-설정-마법사.md) §6). Keep the key copy outside the server ([guide 08](guide/08-클라우드-배포.md) §6-5); without it, offsite backups cannot be opened.
6. **Demo account.** Reviewers cannot receive Korean SMS. Just before submitting, DODA's admin turns on the review demo account in the console and writes the number and code into the review notes. It turns off after 14 days ([guide 03](guide/03-앱-출시.md) D).

**Not yet verified.** The template and CD pass the AWS linters (cfn-lint, actionlint) but have not been launched on a real AWS account. The first deploy is checked together by DODA's admin and the developer.

**Template and new versions:** the `dahae-aws.yaml` attached to the v1.5.1 release predates the permissions boundary and the GitHub subject fix, so it is not used; the developer builds it on `master` from the v1.5.1 install script ([guide 10](guide/10-AWS-배포.md) B-1). From the next release on, download it from the release page (Releases → Latest → Assets); the install script URL and SHA-256 are already filled in. New versions are released from the same repository, which starts `deploy-aws` automatically (waiting for approval if reviewers are set).

**Other clouds:** [guide 09](guide/09-클라우드별-설치.md) and [guide 08](guide/08-클라우드-배포.md).

### 5. Timeline

| Day | Who | Task |
|---|---|---|
| Day 0 | DODA | Sign the license agreement, pick domain, create the AWS deployer IAM user, register Play and Apple organizations, request App Store Connect API access, invite the developer as admin, sign up for Firebase, Kakao, PortOne, Solapi |
| Day 0 | Developer | Confirm the signed license, create the `store` and `doda-aws` environments |
| Day 0–1 | Developer · DODA owner | CloudFormation server (developer, section 4); owner account and setup wizard (owner, guided by the developer on a call) |
| Day 1 | Developer | App IDs, App Store Connect apps, API key, APNs key, GitHub secrets, first `store-apps` run, first AAB manual Play upload, first TestFlight build (Apple processing 10–30 min) ([guide 11](guide/11-앱-직접-빌드.md)) |
| Day 1 | Developer | Check and promote the AAB draft on the Play internal track; register the Play app-signing SHA-1; enter the SHA-256 under App links |
| Day 1–3 | DODA staff · developer | Test on real Android and iPhone devices (sign-in, photos, payment window, push, location) |
| At submission | DODA admin · developer | Turn on the demo account, review notes, confirm privacy and account-deletion URLs, submit |
| Review | Stores | Google usually takes a few days; Apple usually 1–3 days. Typical, not guaranteed. |

### 6. What DODA provides

DODA does no build, signing or deployment work. It creates the accounts and contracts below in its own name and grants the developer admin access. Once the owner creates an admin account for the developer under Staff, the developer can make the first save of the SMS, push and sign-in cards. The PortOne and Toss cards, unlocking, and the connection check after saving work only from the owner account.

| Item | What DODA does | Why |
|---|---|---|
| Apple Developer Program (organization) | Enroll, D-U-N-S, agreements, request App Store Connect API access, invite the developer's Apple ID as Admin | The developer cannot own business accounts (D-2 visa) |
| Google Play Console (organization) | Register, invite the developer's Google account as administrator | Same reason |
| AWS account | Open it with a payment method; create one deployer IAM user (policy only, MFA) | Server cost and data stay in DODA's name |
| Domain | Choose the customer-facing domain. Registration is through the developer's Cloudflare account | It is the customer address |
| Payments (PortOne, PG, Toss payout) contracts | Sign the contracts. Only the **owner account** can make the first save of, or unlock, the PortOne and Toss cards (authenticator code); the developer guides on a call | Settlement is in DODA's name |
| Solapi SMS account and sender number | Register the account and sender number; enter keys | Sender numbers need a business registration |
| Kakao business channel, Kakao, Naver and Google developer apps | Register; issue keys | Some need business registration |
| Firebase | Own the project; grant the developer access | Push keys |
| Owner account | Create the owner account in the setup wizard | Server operator account |
| Review demo account | Turn it on in the console just before submission | DODA chooses the number and code |
| License agreement | Sign the written license agreement ([form](legal/COMMERCIAL-LICENSE-AGREEMENT.md)) | The developer does not build or deploy before it is signed (section 7) |

### 7. What the developer provides

- [ ] A signed written license agreement (repository `LICENSE` sections 1.4 and 3.1). No build or deploy for DODA before it is signed ([guide 11](guide/11-앱-직접-빌드.md) step 0).
- [ ] The `store` environment in the developer repository (secrets: keystore, Play service account, App Store Connect API key, native keys) and the `doda-aws` environment (four AWS variables) ([guide 11](guide/11-앱-직접-빌드.md) step 3, [guide 10](guide/10-AWS-배포.md) C).
- [ ] Store builds, uploads, the first manual AAB upload, TestFlight checks, review submission.
- [ ] Server releases and deployments (`deploy-aws`), maintenance and incident response.
- [ ] Store material (listing text, screenshots, data safety, app privacy labels, review notes) is in `docs/store/`. Screenshots and graphics are not committed; the developer attaches them to the release as `dahae-store-images-<version>.zip` (`gh release upload`).

App build files and keys are never sent to DODA. DODA receives no builds.
