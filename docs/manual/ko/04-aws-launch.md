# 04. 명령 한 줄로 서버 만들기

## 이 장에서 할 일

**[개발자]** 3장에서 만든 담당 계정(`dahae-deployer`)으로 AWS 에 로그인하고, CloudShell 에 명령 한 줄을 붙여 넣어 다해 서버를 만들어요. 명령이 끝나면 서버 안에서 설치가 저절로 이어져요.

보통 개발자가 해요. 담당 계정의 MFA 가 개발자 휴대폰에 있기 때문이에요. 사장님은 이 장을 읽고 무엇이 일어나는지 알아 두면 돼요. 순서는 누가 해도 같아요.

## 준비물

- 3장의 정책 두 개 (`dahae-boundary`, `dahae-deployer-policy`). AWS 관리자가 `bash launch.sh --admin` 을 **한 번** 먼저 실행해 만들어요 (3.2). 없으면 이 장의 명령이 아무것도 만들지 않고 멈춰요.
- 담당 계정 로그인 주소, 사용자 이름 `dahae-deployer`, 비밀번호 (3.7)
- 담당 계정 MFA 가 들어 있는 휴대폰 (3.6)
- 고객이 쓸 **도메인** (예: `example.co.kr` 또는 `app.example.co.kr`). `https://` 없이, **소문자로**. 아직 도메인이 없으면 `auto` 로 시작할 수 있어요 (4.2 팁)
- 인증서 안내와 AWS 알림 메일을 받을 **이메일** (예: `owner@example.co.kr`). 사장님이 읽는 메일함이어야 해요: AWS 가 그곳으로 확인 메일을 보내요 (4.2 의 10번)
- (도메인을 AWS Route 53 에 둔 경우만) **호스팅 영역 ID** (`Z` 로 시작)
- 시간: 명령 약 5–10분 + 서버 안 설치 약 15–20분

## 4.1 담당 계정으로 로그인하기

1. 브라우저에서 담당 계정 로그인 주소(`https://<계정 ID>.signin.aws.amazon.com/console`)를 열어요.
2. **IAM 사용자 이름** 에 `dahae-deployer` 를 입력해요.
3. 비밀번호를 입력하고 **로그인** 을 눌러요.
4. MFA 코드를 묻는 화면이 나오면 휴대폰 인증 앱의 6자리 숫자를 입력해요.
5. 오른쪽 위 지역이 **서울** 인지 확인해요. 아니면 눌러서 **아시아 태평양(서울) ap-northeast-2** 로 바꿔요.

**이렇게 보이면 성공:** AWS 콘솔 첫 화면이 보이고, 오른쪽 위에 `dahae-deployer` 와 **서울** 이 보여요.

## 4.2 명령 붙여 넣기 (세 줄)

1. 화면 위쪽 막대의 **CloudShell** 아이콘(`>_`)을 눌러요.
2. 검은 화면에 `$` 표시가 나올 때까지 기다려요.
3. 아래 세 줄을 메모장에 붙여 넣고, 마지막 줄의 `example.co.kr` 과 `owner@example.co.kr` 을 **실제 도메인과 이메일로** 바꿔요.

   ```
   R=https://raw.githubusercontent.com/EricWijesinghe
   curl -fsSLO $R/Dahae/master/deploy/aws/launch.sh
   bash launch.sh --domain example.co.kr --email owner@example.co.kr
   ```

   > **팁:** 아직 도메인이 없나요? 도메인 자리에 `auto` 라고 써요 (`--domain auto`). 서버가 `3.38.12.200.sslip.io` 같은 무료 임시 주소와 정식 인증서를 받고, DNS 설정은 필요 없어요. 진짜 도메인은 나중에 콘솔에서 옮겨요 (28.8장).

4. 바꾼 세 줄을 모두 복사해요.
5. CloudShell 검은 화면을 한 번 누르고 붙여 넣어요.
6. **Enter** 를 눌러요.
7. 한 줄이 먼저 나와요. 예:

   ```
   Dahae v1.6.0 -> stack dahae-main in ap-northeast-2 (vpc vpc-…, subnet subnet-…, instance t3.medium)
   ```

8. 그다음 AWS 가 서버를 만드는 동안 기다려요 (약 5–10분). **창을 닫지 마세요.**

![CloudShell 에서 서버 만들기 명령을 실행한 화면 (IP 는 가렸어요)](../img/ko/aws-cloudshell-launch.png)

**이렇게 보이면 성공:** 표가 하나 나오고, 그 아래에 `Done. Next:` 로 시작하는 안내 다섯 줄이 보여요. 표에는 `PublicIp`, `InstanceId`, `DeployRoleArn`, `AwsRegion`, `DahaeDomain`, `BackupBucket`, `NextStepUrl` 같은 값이 있어요.

9. 표의 **`PublicIp`** 값(예: `3.38.12.34`)을 복사해서 메모해요. 5장에서 써요. **`BackupBucket`** 값(`dahae-` 로 시작)도 메모해 둬요. 이 서버를 잃고 새로 만들 때만 필요해요 (4.3).
10. 마지막 줄에 `5. Alarm e-mails: AWS sent "AWS Notification - Subscription Confirmation" to …` 가 보여요. 그 메일함을 열고 (스팸함도 봐요), **AWS Notification - Subscription Confirmation** 메일을 열어 **Confirm subscription** 을 한 번 눌러요.

**이렇게 보이면 성공:** 브라우저에 **Subscription confirmed!** 화면이 떠요. 이제 서버 하드웨어가 고장 나고 복구되면 AWS 가 이 주소로 메일을 보내요 (27장 27.3).

> **주의** 이 한 번의 클릭이 없으면 알림 메일이 아예 오지 않아요. 개발자가 명령을 실행했다면, 사장님께 메일을 찾아 눌러 달라고 전하세요.

> **팁** 도메인이 Route 53 에 있다면 명령 끝에 `--hosted-zone-id Z로시작하는ID` 를 붙이세요. 그러면 도메인의 A 레코드도 자동으로 만들어져요 (5.1).

### 명령이 하는 일

명령은 아래 일을 차례로 해요. 사람이 할 일은 없어요.

1. 가장 최근 다해 릴리스 번호를 찾아요.
2. 그 릴리스의 설치 프로그램을 내려받아, 릴리스의 지문 목록(`SHA256SUMS`)과 같은지 확인해요. 다르면 멈춰요.
3. 서버 설계도(CloudFormation 템플릿)에 그 설치 프로그램의 지문을 고정해요. 서버는 다른 설치 프로그램은 실행하지 않아요.
4. 기본 네트워크(기본 VPC)의 공개 서브넷을 찾아요.
5. 정책 `dahae-boundary` 가 있는지 확인해요. 없으면 아무것도 만들지 않고 멈춰요 (3.2).
6. 스택 `dahae-main` 을 만들어요. 스택 안에는 이런 것이 들어 있어요.
   - Ubuntu 24.04 서버 1대 (기본 `t3.medium`, 암호화된 50 GB 디스크)
   - 고정 IP (Elastic IP)
   - 80, 443 만 열린 방화벽 (SSH 22 는 닫혀 있어요. 서버 접속은 AWS 웹 화면으로 해요, 5.5)
   - 매일 디스크 스냅샷 (7개 보관), 하드웨어 고장 때 자동 복구와 위 이메일로 알림 메일, 실수로 지우지 않게 하는 종료 방지
   - 외부 백업용 비공개 S3 저장소. 서버가 자기 AWS 역할로 백업을 올려서 백업 키를 넣을 일이 없어요 (27장 27.3). 스택을 지워도 저장소는 남아요
7. 끝나면 결과 표와 다음 할 일을 보여 줘요.

## 4.3 명령에 붙일 수 있는 선택 항목

필요할 때만 명령 끝에 붙여요. 처음에는 아무것도 붙이지 않는 것을 권장해요.

| 붙이는 글자 | 뜻 | 기본값 |
|---|---|---|
| `--hosted-zone-id Z123…` | Route 53 에 A 레코드를 자동으로 만들어요 | 없음 (직접 DNS 설정) |
| `--instance-type t3.large` | 서버 사양을 바꿔요. 고를 수 있는 값: `t3.medium`, `t3.large`, `t4g.medium`, `t4g.large` | `t3.medium` |
| `--version v1.6.0` | 특정 버전을 설치해요 | 가장 최근 릴리스 |
| `--stack dahae-이름` | 스택 이름을 바꿔요. 반드시 `dahae-` 로 시작해야 해요 | `dahae-main` |
| `--vpc vpc-… --subnet subnet-…` | 네트워크를 직접 골라요 | 기본 VPC 의 공개 서브넷 |
| `--region ap-northeast-2` | 지역 | 서울 |
| `--backup-bucket dahae-…` | **서버를 잃고 새로 만들 때만:** 예전 스택의 `BackupBucket` 값. 새 저장소를 만들지 않고, 새 서버가 그 저장소의 예전 백업을 읽고 이어서 써요 (같은 지역, 27장 27.7) | 없음 (새 저장소) |
| `--dry-run` | 실제로 만들지 않고 무엇을 할지만 보여 줘요 | - |

> **주의** 이 명령은 **이미 있는 스택은 건드리지 않아요.** 같은 이름의 스택이 있으면 멈춰요. 새 버전으로 올리는 일은 이 명령이 아니라 콘솔의 업데이트(26장)나 개발자의 자동 배포로 해요.

> **참고** `t4g` 사양은 ARM 방식이에요. 처음에는 기본값(`t3.medium`)을 쓰세요.

## 4.4 CloudFormation 에서 확인하기

명령 화면을 실수로 닫았거나, 결과를 다시 보고 싶을 때 이렇게 해요.

1. 화면 위 검색창에 **CloudFormation** 을 입력하고 열어요.
2. 왼쪽에서 **스택** 을 눌러요.
3. 목록에서 `dahae-main` 을 찾아요.
4. 상태가 `CREATE_IN_PROGRESS` 면 아직 만드는 중이에요. 기다려요.
5. 상태가 `CREATE_COMPLETE` 가 되면 `dahae-main` 을 눌러요.
6. **출력** 탭을 눌러요.
7. `PublicIp` 값을 복사해요.

![CloudFormation 스택 목록: dahae-main CREATE_COMPLETE](../img/ko/aws-cfn-stack-complete.png)

![스택 출력 탭의 PublicIp (가렸어요)](../img/ko/aws-cfn-outputs.png)

**이렇게 보이면 성공:** `dahae-main` 상태가 `CREATE_COMPLETE` 이고, **출력** 탭에 `PublicIp` 가 있어요.

## 4.5 설치가 끝날 때까지 기다리기

> **주의** `CREATE_COMPLETE` 가 되어도 **설치는 아직 진행 중**이에요. 서버 안에서 설치 프로그램이 약 15–20분 더 일해요. (명령 화면의 안내에는 "약 10분"이라고 나와요.)

1. 기다리는 동안 5장(도메인 연결)을 해요.
2. 5.4 의 방법으로 설치가 끝났는지 확인해요.

설치 프로그램이 서버 안에서 하는 일: Docker, 데이터베이스(PostgreSQL), 웹 서버(Caddy), 방화벽, 자동 백업과 자동 업데이트 예약을 설치하고, 사장님 계정용 **설치 코드**를 만들어요 (6장).

## 4.6 손으로 만드는 방법 (참고)

CloudShell 을 쓸 수 없을 때는 CloudFormation 화면에서 템플릿 파일을 올려 같은 서버를 만들 수 있어요. 템플릿 파일 준비와 매개변수 입력은 개발자 일이라 33장(개발자 부록)에 있어요. 요점만:

- 스택 이름은 반드시 `dahae-` 로 시작해야 해요.
- `GitHubRepo`, `GitHubRepoIds`, `GitHubEnvironment` 같은 항목은 기본값을 그대로 둬요.
- `SshCidr` 는 비워 둬요.
- 맨 아래 **AWS CloudFormation 이 IAM 리소스를 생성할 수 있음을 승인합니다** 를 체크해야 만들어져요.

새 버전 자동 배포(GitHub 연결)도 개발자 일이에요 (33장).

## 자주 생기는 문제

명령이 멈추면 `error:` 로 시작하는 줄이 나와요. 그 줄을 보고 아래 표를 찾으세요.

| 화면에 나온 말 | 뜻과 해결 |
|---|---|
| `error: --domain like example.co.kr is required (or auto …)` | 도메인이 없거나 형식이 틀렸어요. `https://` 를 빼고, **소문자로**, `example.co.kr` 처럼 넣으세요. 도메인이 아직 없으면 `--domain auto` 로 해요 |
| `error: --hosted-zone-id needs a real --domain` | `--domain auto` 에는 `--hosted-zone-id` 를 붙이지 않아요. 둘 중 하나를 빼세요 |
| `error: --email is required` | 이메일이 없거나 형식이 틀렸어요 |
| `error: --stack must start with dahae-` | 스택 이름은 `dahae-` 로 시작해야 해요 |
| `error: --backup-bucket is the old stack's BackupBucket output (dahae-…)` | `--backup-bucket` 뒤의 값이 저장소 이름이 아니에요. CloudFormation → 예전 스택 → **출력(Outputs)** 의 `BackupBucket` 값을 다시 복사해요 |
| **AWS Notification - Subscription Confirmation** 메일이 없어요 | 스팸함을 봐요. `--email` 값을 바르게 넣었는지 확인해요. 개발자가 AWS 에서 다시 보낼 수 있어요 (SNS → 구독 → **확인 요청**) |
| `error: no release version found (use --version vX.Y.Z)` | 최신 버전을 찾지 못했어요. 잠시 뒤 다시 하거나, 개발자에게 버전 번호를 물어 `--version v1.6.0` 처럼 붙이세요 |
| `error: install.sh of … does not match the release's SHA256SUMS` | 설치 프로그램 지문이 맞지 않아요. **더 진행하지 말고** 개발자에게 알려 주세요 |
| `error: no default VPC in ap-northeast-2: pass --vpc and --subnet` | 서울 지역에 기본 네트워크가 없어요. 개발자에게 `--vpc`, `--subnet` 값을 물어보세요 |
| `error: no public subnet in …: pass --subnet` | 인터넷으로 나갈 수 있는 서브넷이 없어요. 개발자에게 물어보세요 |
| `error: policy dahae-boundary is missing: an admin runs this script with --admin once first` | 3장의 정책이 아직 없어요. 아무것도 만들어지지 않았어요. AWS 관리자 계정으로 `bash launch.sh --admin` 을 한 번 실행하고 (3.2), 담당 계정에 정책을 붙인 뒤 (3.5) 이 명령을 다시 실행하세요 |
| `error: stack dahae-main already exists; …` | 이미 서버가 있어요. 새로 만들 필요가 없어요. 업데이트는 26장을 보세요 |
| `AccessDenied` 나 권한 오류 | MFA 코드를 넣고 로그인했는지, 담당 계정에 정책 두 개가 붙어 있는지 확인하세요 (3.5, 3.6) |
| 상태가 `ROLLBACK_COMPLETE` 나 `CREATE_FAILED` 예요 | 서버 만들기가 실패했어요. CloudFormation 의 **이벤트** 탭 화면을 캡처해 개발자에게 보내세요 |
| CloudShell 창이 닫혔어요 | 화면에 `Waiting for stack create/update to complete` 가 나온 뒤라면 서버 만들기는 AWS 에서 계속돼요. 4.4 로 확인하세요. 스택 상태가 `REVIEW_IN_PROGRESS` 에서 멈춰 있으면 만들기가 시작되지 않은 거예요. 다시 실행하면 `already exists` 로 멈추니, 개발자에게 이 스택을 지우고 다시 실행해 달라고 하세요 |
