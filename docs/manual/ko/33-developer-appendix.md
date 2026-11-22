# 33. 개발자 부록

**이 장에서 할 일:** 개발자(설치 담당)가 서버에서 쓰는 명령, 설정 파일, AWS 배포, 스토어 빌드를 한곳에 모았어요. 사장님은 이 장을 직접 따라 할 필요가 없어요. 개발자에게 이 장을 보여 주세요.

> **준비물**
>
> - 서버 명령 창: AWS 는 Session Manager (5장), 다른 클라우드는 브라우저 SSH (7장).
> - 모든 서버 명령 앞에는 `sudo` 를 붙여요. 예전 이름 `malkkeum` 으로 쳐도 같은 명령이에요.
> - 저장소 문서: `docs/guide/07-운영-명령어.md`, `docs/OPERATIONS.md`, `docs/RUNBOOK.md`, `docs/DEPLOY.md`, `docs/guide/10-AWS-배포.md`, `docs/guide/11-앱-직접-빌드.md`. 이 장은 그 문서들의 요약이고, 다르면 코드(`deploy/install.sh` 의 `USAGE`)가 기준이에요.

---

## 33.1 `dahae` 명령 전체

서버의 설치 폴더는 `/opt/malkkeum` 이에요. 전체 목록은 `sudo dahae help` 로 볼 수 있어요.

| 명령 | 하는 일 |
|---|---|
| `dahae status` | 서비스 상태. 앱, HTTPS, 외부 가동 확인, 외부 백업, 사진 외부 백업, 비밀 키 사본 확인 결과를 보여 줘요 |
| `dahae logs [api\|web\|db]` | 로그 최근 200줄을 보여 주고 계속 따라가요 (`Ctrl + C` 로 나가기). 기본은 `api` |
| `dahae version` | 지금 채널과 버전 |
| `dahae backup` | 지금 바로 백업 (DB + 사진). 끝나면 `✔ 백업 완료: /opt/malkkeum/backups/db-….sql.gz.enc` |
| `dahae restore 파일 [--yes]` | 서버 안 백업으로 되돌리기 (DB + 같은 시각 사진). `backups/` 안의 파일만 받아요 |
| `dahae restore-offsite [이름] [--yes]` | 이름 없이: 외부 백업 목록. 이름과 함께: 내려받아 되돌리기 |
| `dahae update [태그]` | 백업 → 새 버전 → 응답 확인. 응답이 없으면 자동으로 되돌려요 |
| `dahae rollback` | 바로 전 버전의 프로그램으로 되돌리기 (데이터는 그대로) |
| `dahae channel stable\|beta` | 업데이트 채널 바꾸기. 바꾸면 바로 그 채널 버전으로 업데이트해요 |
| `dahae update-schedule mon@04:00\|off` | 자동 업데이트 요일·시각. `off` 는 자동 업데이트 끄기 (요일: mon, tue, wed, thu, fri, sat, sun) |
| `dahae load 파일.tar` | 미리 받아 둔 이미지 불러오기 (인터넷 없이 업데이트, 체험용) |
| `dahae heartbeat <https 주소>\|off` | 외부 가동 확인 핑 주소 설정 (예: healthchecks.io, 주기 5분·유예 15분 권장) |
| `dahae domain <새 도메인> [--yes]` | 서비스 주소 바꾸기: `.env` 백업 → `DOMAIN` 변경 → api·web 다시 시작 → 새 주소로 `/health` 확인 (5분까지). 실패하면 `.env` 를 되돌리고 종료 코드 3. 보통은 콘솔(28장 28.8)에서 해요 |
| `dahae host-approval enroll <번호>\|status` | 서버 승인 코드 만들기 / 상태 (버전 올리기·되돌리기·도메인 바꾸기 승인에 써요. 콘솔에서도 만들어요) |
| `dahae owner-reset` | **서버 승인 코드**를 지워요 (확인 글자 입력 필요). 사장님 2단계 인증 초기화가 아니에요 |
| `dahae applinks --android-sha256 지문[,지문] --apple-team-id 팀ID` | 앱 링크 설정 |
| `dahae demo load [--force]` / `demo clear` | 체험용 데이터 넣기 / 지우기 |
| `dahae setup-code [--renew]` | 사장님 계정 설치 코드. 처음 보인 뒤 7일 동안 써요. `--renew` 는 새 코드 |
| `dahae keys-backup` | `.env` 를 비밀번호(12자 이상)로 잠근 사본 `backups/keys-….env.enc` 를 만들어요. 끝에 "보관했어요" 를 직접 입력해야 보관 확인이 기록돼요 |
| `dahae keys-restore 파일 [--yes]` | 새 서버에 예전 `DATA_KEY`·`JWT_SECRET` 을 넣어요 (`restore-offsite` 전에) |
| `dahae alert update\|backup\|ssh` | 자동 작업 실패·SSH 차단 증가를 외부 가동 확인 주소로 알려요 (예약 작업용) |
| `dahae uninstall [--yes]` | 서비스 끄기. 데이터, `.env`, 백업은 남겨요 |
| `dahae uninstall --purge [--confirm 도메인]` | 데이터와 백업까지 삭제. 도메인을 입력해 확인하고, `--yes` 로는 넘어가지 않아요. **되돌릴 수 없어요** |

**자동 예약 작업** (설치 프로그램이 `/etc/cron.d/malkkeum` 에 써요):
- 매일 03:30 `dahae backup` (실패하면 `dahae alert backup`).
- 기본 매주 월요일 04:00 자동 업데이트 (`update-schedule` 로 바꿈).
- 5분마다 외부 가동 확인 핑 (주소가 있을 때).
- WAL 보관을 켰으면 매주 일요일 02:15 전체 백업 (33.8).
- 자동 업데이트(`dahae auto-update`)는 `update-skip` 표시가 있거나 기다리는 서버 작업이 있으면 건너뛰어요. 안정 채널에서는 새 `stable` 이미지를 처음 본 뒤 24시간이 지나야 받아요.

**사장님 2단계 인증 초기화:** 사장님이 인증 앱과 복구 코드를 모두 잃었을 때는 API 프로그램의 `owner-reset` 명령(`api owner-reset`)이 한 시간짜리 일회용 코드를 출력해요. 컨테이너에서 실행하는 정확한 명령줄과, 그 코드를 넣는 화면(콘솔에는 아직 없음)은 **확인 필요** 예요. 서버 경로는 `POST /v1/auth/staff/owner-reset` 이에요.

---

## 33.2 파일과 설정 값

| 무엇 | 어디 | 메모 |
|---|---|---|
| 설정과 비밀 키 | `/opt/malkkeum/.env` | 권한 600. 손으로 고치지 마세요 |
| 백업 | `/opt/malkkeum/backups/` | `db-날짜-시각.sql.gz.enc`, `uploads-날짜-시각.tar.gz.enc`, `keys-….env.enc`, `backup.log`. 권한 700 |
| 데이터베이스 | Docker 볼륨 `dbdata` (PostgreSQL 17) | 외부에 포트를 열지 않아요 |
| 사진 | Docker 볼륨 `uploads` | 어디에도 연결되지 않은 사진은 30일 뒤 지워요 |

`.env` 의 주요 키 (값은 적지 않아요):

| 키 | 뜻 |
|---|---|
| `DATA_KEY` | 연동 키, 지급 계좌, 서버 안·외부 백업 암호화, 사진 링크 서명. **바꾸면 저장된 키와 예전 백업을 못 열어요.** 재암호화 명령은 없어요 |
| `JWT_SECRET` | 로그인 토큰 서명. 교체는 개발자만 |
| `DB_PASSWORD`, `API_DB_PASSWORD` | 데이터베이스 관리 계정과 앱 계정 비밀번호 |
| `DOMAIN` | 서비스 도메인 |
| `VERSION` | 업데이트 채널 태그 (`stable`/`beta`). `dahae update 태그` 로 올리면 그 태그로 고정돼요 |
| `API_IMAGE`, `WEB_IMAGE`, `PREV_API_IMAGE`, `PREV_WEB_IMAGE` | 지금 이미지와 되돌리기용 이전 이미지. 이전 이미지는 지우지 마세요 |
| `SETUP_CODE`, `SETUP_CODE_AT` | 설치 코드와 처음 보인 시각 |
| `HEARTBEAT_URL` | 외부 가동 확인 주소 |
| `KEY_ESCROW_AT` | 비밀 키 사본 서버 밖 보관 확인 시각 |
| `WAL_ARCHIVE_MODE`, `WAL_S3_*`, `WAL_CIPHER_PASS` | WAL 보관 (선택, 33.8) |

**설치 프로그램 환경 변수** (보통은 필요 없어요, `MALKKEUM_*` 대신 `DAHAE_*` 도 돼요): `MALKKEUM_DOMAIN`, `MALKKEUM_EMAIL`, `MALKKEUM_CHANNEL`, `MALKKEUM_VERSION`, `MALKKEUM_TLS=internal` (테스트 전용, 사장님 설치에는 쓰지 않기), `MALKKEUM_SKIP_DNS_CHECK=1`, `MALKKEUM_HEALTH_WAIT`, `MALKKEUM_HEARTBEAT_URL`, `MALKKEUM_NOTIFY_URL` (설치 결과 알림), `MALKKEUM_SETUP_CODE`, `MALKKEUM_ANSWERS`, `MALKKEUM_IMAGES_TAR`, `MALKKEUM_FIREWALL=on`.

**질문 없이 설치하기:**

```
DOMAIN=myshop.co.kr
EMAIL=owner@example.co.kr
```

위 두 줄을 `/root/dahae-answers.txt` 같은 일반 파일에 적고:

```
curl -fsSL INSTALL_URL | sudo env MALKKEUM_ANSWERS=/root/dahae-answers.txt bash
```

- 값에는 공백, 역슬래시, `=` 를 쓸 수 없어요. 질문 없이 실행하면 방화벽은 기본으로 꺼져요. 켜려면 `MALKKEUM_FIREWALL=on` 을 같이 넣어요. SSH 비밀번호 로그인은 질문 없이 끄지 않아요.

---

## 33.3 백업·복원 명령 순서

**서버 안 백업으로 복원**

```
sudo ls /opt/malkkeum/backups
sudo dahae restore backups/db-20261110-033000.sql.gz.enc
```

- "현재 데이터를 … 로 되돌릴까요?" 에 `yes`. 복원 전에 지금 상태를 먼저 백업해요.
- 한 트랜잭션으로 적용해요. DB 복원이 실패하면 데이터는 그대로예요. 사진 복원이 실패하면 DB 는 이미 복원된 상태예요.
- 끝나면 `✔ 복원 완료: … (사진 파일: 복원됨)`.

**외부 백업으로 복원**

```
sudo dahae restore-offsite
sudo dahae restore-offsite db-20261110-033000.sql.gz.enc
```

**새 서버로 옮기기** (약 1시간)

1. 새 서버에 설치해요.
2. 옛 서버에서 `sudo dahae backup` 과 `sudo dahae keys-backup`.
3. 키 사본을 사장님 컴퓨터로 받은 뒤 새 서버의 `/opt/malkkeum/backups/` 로 옮겨요.
   ```
   scp keys-날짜.env.enc 계정이름@새서버IP:~/
   sudo mv ~/keys-날짜.env.enc /opt/malkkeum/backups/
   ```
4. `sudo dahae keys-restore backups/keys-날짜.env.enc` → 잠금 비밀번호 → `yes`.
5. 새 서버 콘솔에서 **외부 백업 (S3 호환)** 카드를 다시 채우고 저장.
6. `sudo dahae restore-offsite` → `sudo dahae restore-offsite db-날짜.sql.gz.enc` → `yes`.
7. 예전 사장님 계정으로 로그인. A 레코드를 새 IP 로. `sudo dahae status` 로 HTTPS 확인.
8. 새 서버가 정상이면 옛 서버에서 `sudo dahae uninstall`.

**복원 연습:** `bash scripts/ci/backup-drill.sh` (WAL 연습은 `WAL_DRILL=1` 과 `PGBACKREST_*`). 운영 서버가 아닌 시험 서버에서 하세요.

---

## 33.4 되돌리기(rollback) 주의점

- `dahae rollback` 은 프로그램만 되돌려요. 마이그레이션은 되돌리지 않아요. 데이터까지 과거로 돌리려면 `dahae restore`.
- **저장 카드 (마이그레이션 0326):** 첫 시작 때 저장된 빌링 키를 봉인해요. 그 앞 버전으로 되돌리면 저장 카드 결제와 탈퇴 시 키 삭제가 깨져요. 앞으로 가거나 업데이트 전 백업으로 복원하세요.
- **토스 정산 (0472·0473):** 토스 결과를 기다리는 정산이 있으면 1.5 에서 1.4 로 되돌리지 마세요. `dahae rollback` 은 그런 정산이 있거나 확인하지 못하면 멈추고 건수를 알려 줘요. 콘솔 **[정산]** 에서 **[상태 확인]** 으로 정리한 뒤 다시 하세요.
- **토스페이먼츠 결제 (0500~0502):** 토스 이전 버전으로 되돌리기 전에 **결제 (토스페이먼츠)** 카드의 **사용: 토스페이먼츠로 결제 받기** 를 끄고(새 토스 결제와 새 카드 저장만 멈춰요), 진행 중이거나 입금을 기다리는 토스 결제와 대기 중인 토스 환불이 없을 때까지 기다리세요.
- 업데이트가 실패해 자동으로 되돌린 경우 `dahae update` 는 종료 코드 3 으로 끝나고, 콘솔에는 **실패해서 이전 버전으로 되돌렸어요** 로 보여요.

---

## 33.5 콘솔의 서버 작업과 서버 에이전트

콘솔 **[시스템 상태]** 의 **서버 업데이트** 카드는 `POST /admin/host/jobs` 에 `kind=update` 작업을 넣어요. 소유자 인증 앱 코드(`X-Owner-Totp`)가 요청마다 필요하고, 업데이트가 대기 중·진행 중이면 버튼이 꺼져요. 서버의 작업 실행기(`deploy/agent.sh`)가 작업 파일을 읽어 실행해요. 작업을 넣으면 요청한 소유자 계정의 휴대폰으로 "서버 작업이 요청됐어요: 업데이트. …" 문자가 가요.

| 에이전트 작업 종류 | 콘솔에 버튼 | 메모 |
|---|---|---|
| `update` | 있음 (**[지금 업데이트]**) | `dahae update` 실행 |
| `channel`, `schedule`, `backup-now`, `update-skip`, `restart` (`all`) | 있음 (**서버 설정**, 26장) | 소유자 코드. `channel` 은 `dahae channel` 처럼 바로 업데이트해요. `schedule` 값은 `mon@04:00` 또는 `off` |
| `heartbeat` | 있음 (**업타임 하트비트** 카드, 17장) | 소유자 코드. `https://` 주소 또는 `off`. 에이전트가 `dahae heartbeat` 를 실행해요 (api 다시 시작, 테스트 핑) |
| `domain-check` | 있음 (**도메인** 카드의 **[DNS 확인]**, 28장) | 관리자 세션만. 결과 JSON 은 `GET /admin/host/jobs/{id}/output` 으로 10분 동안 읽어요 |
| `domain-set` | 있음 (**도메인** 카드의 **[바꾸기]**, 28장) | 소유자 코드 + 서버 승인 코드 (`POST /admin/host/jobs/{id}/approve`). `dahae domain` 실행, 실패하면 `.env` 를 되돌리고 `rolled_back` |
| `host-approval-enroll` | 있음 (**서버 승인 코드** → **[등록하기]**, 28장 28.8.1) | 소유자 코드. 계정 번호는 서버가 채워요. 인증 앱 주소는 요청한 소유자만 읽어요. 이미 있으면 실패 |
| `update-tag`, `rollback` | 없음 | 서버 승인 코드(TOTP)가 필요한 작업. 승인 칸에서 10분 안에 승인, 5번 틀리면 취소, 1시간에 10번 틀리면 잠김 |
| `status-refresh`, `logs`, `offsite-list` | 없음 | 에이전트는 받지만 지금 콘솔에서는 보내지 않아요 |

- 콘솔의 작업 상태 표시: `queued` **대기 중**, `running` **진행 중**, `ok` **완료**, `failed` **실패**, `rolled_back` **실패해서 이전 버전으로 되돌렸어요**, `awaiting_approval` **승인 코드를 기다려요**, `rejected` **거절됨**, 그 밖 **확인이 필요해요**.
- 이 기능 전에 설치한 서버는 설치 줄을 한 번 다시 실행해야 새 작업 실행기와 `dahae domain` 이 들어가요 (`dahae update` 는 이미지만 바꿔요). 그 전에는 `domain-set` 이 이유 없이 `rejected` 로 끝나요.
- 서버 승인 코드는 콘솔(28장 28.8.1) 또는 `dahae host-approval enroll <사장님 계정 번호>` 로 만들고, 새어 나갔으면 `dahae owner-reset` 후 다시 만들어요.

---

## 33.6 AWS: 손으로 스택 만들기, GitHub 배포, 삭제

**한 번에 만들기 (권장):** CloudShell 에서 (4장)

```
R=https://raw.githubusercontent.com/EricWijesinghe
curl -fsSLO $R/Dahae/master/deploy/aws/launch.sh
bash launch.sh --domain example.co.kr --email owner@example.co.kr
```

옵션: `--version vX.Y.Z` (기본: 최신 릴리스), `--stack dahae-main`, `--instance-type t3.medium`, `--hosted-zone-id Z…` (Route 53 A 레코드), `--vpc vpc-… --subnet subnet-…` (기본: 기본 VPC 의 공개 서브넷), `--region ap-northeast-2`, `--ref master`, `--dry-run`. 정책 만들기는 `--admin`. 이미 있는 스택은 건드리지 않아요. 릴리스의 `install.sh` 를 `SHA256SUMS` 와 대조한 뒤 지문을 템플릿에 고정해요.

**손으로 만들기 (B-1, B-2 요약)**

1. 최신 릴리스 Assets 의 `dahae-aws.yaml` 을 내려받아요 (설치 스크립트 주소와 지문이 이미 고정돼 있어요). (선택) `master` 에서 `scripts/release/build-aws-template.sh` 로 직접 만들 수도 있어요. 주소는 태그(`/v1.6.0/`)가 들어간 주소여야 해요. 출력 지문이 `SHA256SUMS` 의 `install.sh` 줄과 같아야 해요.
2. CloudFormation > 스택 생성 > 템플릿 파일 업로드. 스택 이름은 **반드시 `dahae-` 로 시작**.
3. 매개변수: `Domain`, `Email`, `HostedZoneId` (없으면 비움), `InstanceType` (`t3.medium` 기본; `t4g` 는 ARM 이라 서버가 새로 만들어져요), `DiskSizeGiB` (`50`), `VpcId`, `SubnetId`, `SshCidr` (비움), `GitHubRepo` (`EricWijesinghe/Dahae` 그대로), `GitHubRepoIds` (기본값 그대로), `GitHubEnvironment` (`doda-aws`), `CreateGitHubOidcProvider` (계정에 처음이면 `true`), `AmdAmiId`/`ArmAmiId` (바꾸지 않기).
4. IAM 리소스 생성 승인 체크 → 제출 → `CREATE_COMPLETE` (약 3~5분). 그 뒤 서버 안 설치가 약 15~20분 더 걸려요.

**GitHub 배포 (C 단계)**

- 저장소 **Settings > Environments > New environment** `doda-aws`. **Deployment branches** 는 **Selected branches** 로 `master` 만. 승인자는 선택 (**Required reviewers**).
- **Environment variables** (Secrets 아님): `AWS_REGION` ← `AwsRegion`, `AWS_DEPLOY_ROLE_ARN` ← `DeployRoleArn`, `AWS_INSTANCE_ID` ← `InstanceId`, `DAHAE_DOMAIN` ← `DahaeDomain` (스택 **출력** 탭 값).
- 흐름: 안정 태그 `vX.Y.Z` → `release` → `deploy-aws` (승인자가 있으면 승인 대기) → 서버에서 `dahae update vX.Y.Z` (최대 6시간 대기) → `/health` 와 버전 확인. 베타 태그는 자동 배포하지 않아요.
- 수동: **Actions > deploy-aws > Run workflow** 에 `version`. 되돌리기는 **rollback** 체크. 정산이 진행 중이면 되돌리기가 멈춰요.
- 실패: **Actions > 그 실행 > server output**. 서버가 응답하지 않으면 Session Manager 로 `sudo dahae logs`.
- 서버 자체의 월요일 04:00 자동 업데이트도 켜져 있어요. GitHub 로만 올리려면 `sudo dahae update-schedule off`.

**비용:** 서버 종류, 디스크, 전송량, 스냅샷에 따라 달라요. AWS 요금 계산기에서 서울(ap-northeast-2)로 직접 계산하세요. 이 매뉴얼에는 가격을 적지 않아요.

**안전하게 삭제 (D-4)**

1. `sudo dahae backup` 과 외부 백업 확인.
2. EC2 > 인스턴스 > **작업 > 인스턴스 설정 > 종료 방지 변경** 에서 끄기.
3. CloudFormation > 스택 > `dahae-…` > **삭제**.
4. 남는 것: 디스크 볼륨(직접 삭제), 탄력적 IP(해제), OIDC 공급자(계정 공용이라 남음).
5. GitHub 환경 `doda-aws` 정리. 필요 없으면 IAM 사용자와 정책 `dahae-deployer-policy`, `dahae-boundary` 삭제 (DODA 관리자 계정).

> **주의** 스택을 **업데이트** 할 때 변경 세트에 `ServerInstance` 가 `Replacement: True` 로 나오면 취소하세요. 서버가 새로 만들어지는 변경이에요.

---

## 33.7 릴리스와 채널

| 태그 | 이미지 태그 (GHCR) | 배포 브랜치 |
|---|---|---|
| `v1.2.3` | `1.2.3`, `beta`, `stable`, `latest` | `beta`, `main` |
| `v1.3.0-beta.1` | `1.3.0-beta.1`, `beta` | `beta` 만 |

- 서버는 `.env` 의 `VERSION` 채널 태그를 따라요. 월요일 04:00 (KST) 백업 → 태그 받기 → 상태 확인 → 3분 안에 응답이 없으면 이전 이미지로 되돌려요.
- 마이그레이션은 더하기만 해요(expand/contract). 이전 릴리스가 읽는 것을 지우거나 이름을 바꾸지 않아요.
- 앱 최소 버전은 콘솔 **사업장 정보** 의 `min_app_version` / `latest_app_version` 으로 정해요 (10장).
- 이미지는 릴리스 워크플로가 cosign 키리스 서명을 하고, 설치 프로그램이 서명을 확인한 뒤에 실행해요.

---

## 33.8 WAL 보관 (선택, 엔지니어 설정)

- 시점 복원(약 5분 단위)을 원할 때만. 켜지 않으면 복구 지점은 하루예요.
- `deploy/wal` 을 `/opt/malkkeum/wal` 로 복사하고 `.env` 에 `WAL_S3_BUCKET`, `WAL_S3_ENDPOINT`, `WAL_S3_KEY`, `WAL_S3_SECRET`, `WAL_CIPHER_PASS` 를 넣은 뒤 `bash wal/wal-setup.sh`. 성공하면 `dahae write-cron` 이 매주 일요일 02:15 기본 백업을 추가해요.
- 켜져 있는 동안 모든 `dahae` 명령이 `wal/docker-compose.wal.yml` 을 함께 써요. 기본 `postgres:17` 이미지로 띄우지 마세요 (WAL 이 디스크에 쌓여요).
- `WAL_CIPHER_PASS` 도 `DATA_KEY` 처럼 서버 밖에 보관하세요.
- 시점 복원은 `dahae` 명령에 없어요: `pgbackrest --stanza=malkkeum --type=time "--target=<ISO 시각>" restore` (빈 데이터 볼륨, 같은 이미지).

---

## 33.9 스토어 앱 빌드 (요약)

자세한 순서는 `docs/guide/11-앱-직접-빌드.md` 에 있어요.

- 빌드는 개발자 저장소의 `store-apps` 워크플로(수동 실행)가 해요. 안드로이드 AAB 는 Linux, iOS IPA 는 macOS 러너.
- AAB 는 DODA 의 Google Play Console 내부 테스트 트랙 초안으로, IPA 는 DODA 의 App Store Connect (TestFlight)로 올라가요.
- 모든 비밀 값은 GitHub 환경 **`store`** 의 Secrets 에만 넣어요. 키, `.p8`, keystore 는 저장소에 올리지 않아요.
- 시작 전에 서면 라이선스 계약이 있어야 해요 (`LICENSE` §1.4·§3.1).
- 첫 AAB 는 Play Console 에서 직접 올려야 해요. 그다음부터 API 로 올라가요.
- 빌드 전에 `scripts/store_brand.py` 가 `{API_BASE}/v1/brand` 에서 홈 화면 이름(`short_name`, 파트너 앱은 뒤에 ` 파트너`), 아이콘(`store_1024`, `icon_512`), 색을 받아요. 서버에 연결되지 않거나 로고가 없으면 빌드가 멈춰요. 서버를 켜기 전 빌드만 워크플로 입력 `brand_from_server` 를 꺼요.
- `store` 환경 변수: `API_BASE`, `APPLE_TEAM_ID`, `APP_LINK_DOMAIN` (도메인을 옮긴 뒤에는 `새도메인,예전도메인`, 세 개까지), 선택 `CUSTOMER_APP_ID`, `PARTNER_APP_ID` (기본 `kr.co.dahae.customer`, `kr.co.dahae.partner`, 첫 업로드 전에 정해요. 콘솔 **앱 링크** 카드의 앱 ID 칸과 같은 값).
- 빌드가 끝나면 사장님이 콘솔 **스토어 앱 빌드** 카드에서 **[빌드했어요]** 를 눌러 기록해요 (28장 28.7).

| 증상 | 원인과 해결 |
|---|---|
| 앱이 서버에 연결되지 않음 | `API_BASE` 변수가 없거나 틀림. 고친 뒤 다시 빌드 |
| `/v1/brand has no store_1024 icon` 으로 멈춤 | 콘솔 **사업장 정보** 에 로고(또는 글자 로고)를 저장한 뒤 다시 빌드 |
| iOS 서명 실패 | App ID 기능 중 하나가 꺼져 있음 |
| Play 업로드 권한 오류 | 서비스 계정에 두 앱의 출시 관리자 권한이 있는지 확인 |
| 카카오·구글 로그인 실패 (Android) | Play 앱 서명 SHA-1 이 카카오·구글 콘솔에 없음 |
| 푸시가 안 옴 | `GoogleService-Info.plist` 또는 APNs 키가 빠짐 |

아직 확인하지 않은 것: 인증서·프로필 자동 발급, iOS 카드 앱 복귀(PortOne 웹 결제), 첫 실행 로그의 Xcode 버전.

---

## 33.10 결제 연동 참고

- 키 종류, 콘솔 순서, 테스트 결제: `docs/guide/payments-setup.md`. 연동 사양과 확인할 항목: `docs/guide/payments-reference.md`.

| 공급자 | 웹훅 주소 | 검증 |
|---|---|---|
| PortOne | `/v1/webhooks/portone` | Standard Webhooks 서명 (`webhook-signature`). 실패하면 401, PortOne 이 다시 보내요 |
| 토스페이먼츠 (결제) | `/v1/webhooks/tosspayments` | 보안 키가 있고 `tosspayments-webhook-signature` 헤더가 오면 HMAC-SHA256. 헤더가 없으면 알림으로만 받고 다시 조회 |
| 토스 (지급대행) | `/v1/webhooks/toss` | `tosspayments-webhook-signature` (보안 키 기반 HMAC-SHA256) |

- 웹훅은 결제 상태를 바로 바꾸지 않아요. 서버가 결제사에 다시 조회해요.
- 결제 대사는 `reconcile_interval_secs` (기본 60초)마다 돌아요.
- **PortOne, 토스페이먼츠, 또는 둘 다.** PortOne은 API secret과 store id(env 또는 콘솔)가 있으면 설정된 것이고, 채널 키가 있는 결제 방법만 받아요. 토스페이먼츠는 카드가 저장되고 올바르면 설정된 것이고, `active` 가 켜진 동안 고객에게 보여요. 스위치 없이 저장한 카드는 라이브 키면 켜짐, 테스트 키면 꺼짐으로 저장돼요.
  - 결제 방법마다 결제사는 하나예요: PortOne이 받으면 PortOne, 아니면 토스페이먼츠가 보이면 토스페이먼츠, 둘 다 아니면 숨겨요. 방법 순서는 카탈로그 순서 그대로이고, 결제사를 나열할 때는 PortOne이 먼저예요.
  - 새로 저장하는 카드: PortOne이 카드를 저장할 수 있으면(`card_billing` 또는 예전 `card` 키) PortOne, 아니면 자동결제 키 쌍이 있는 토스페이먼츠, 둘 다 아니면 카드 저장은 꺼져요 (`POST /v1/me/payment-methods` 가 503 `provider_not_configured`). `flow: billing` 은 카드만이고, 간편결제는 두 결제사 모두 1회 결제예요.
  - 행은 옮겨 가지 않아요: `checkouts`, `payment_methods`, `payments` 는 적힌 `gateway` 를 그대로 써서 환불, 웹훅, 대사, 정기결제가 행을 따라가요. `active` 를 끄면 새 토스 체크아웃과 새 카드 저장만 멈춰요.
  - `GET /v1/payments/methods` 와 `GET`/`PUT /v1/admin/payment-methods` 는 방법마다 `gateway` 를 돌려줘요. `GET /v1/admin/setup` 은 `payment_gateways: [{code, mode}]` 를 돌려주고, 예전 `payment_gateway`, `payment_gateway_mode` 는 옛 콘솔을 위해 남아 있어요. `GET /v1/payments/methods` 와 `GET /v1/me/payment-methods/setup` 은 `?gateways=portone,tosspayments` 를 받아요(지금 앱이 보내요). `tosspayments` 가 없으면(v1.5.1 앱) 토스 결제 방법과 토스 카드 저장을 빼요. `DELETE /v1/admin/setup/{portone|tosspayments}` 는 그 카드에 대기·입금 대기 체크아웃, 살아 있는 저장 카드, 대기 중인 환불이 있으면 409 `gateway_in_use` 예요. PortOne `TEST` 채널의 결제나 빌링키는 `payment_stuck` (`portone_test_channel`) 을 남겨요. 마이그레이션은 없어요.
  - API secret이나 store id 없이 env에만 있는 PortOne 채널 키의 방법은 더 이상 보이지 않아요 (원래 결제될 수 없었어요).
  - PASS는 PortOne 상점 아이디와 본인인증 채널을 env나 콘솔에서 읽어요(`identity_enabled`). 결과를 확인하려면 V2 API secret도 있어야 해요.
- 아직 확인하지 않은 항목: PortOne 실제 카드 거절 코드, 현금영수증 발급 응답, 토스 웹훅 서명 실측, 토스 지급 생성 응답 형식 (`payments-setup.md` 5절).

---

## 33.11 개발 환경

- 필요한 것: Rust 1.99.1 (`backend/rust-toolchain.toml`), Flutter 3.47.9 (`.github/actions/flutter-setup`), PostgreSQL 17, Python 3.11 이상.

```bash
# 서버: backend/.env 에 DATABASE_URL, JWT_SECRET, DATA_KEY, PUBLIC_BASE_URL, DEV_MODE=1
cd backend && cargo run -p api          # 마이그레이션 적용 후 :8080

# 앱
cd app && flutter pub get
cd apps/customer && flutter run --dart-define=API_BASE=http://localhost:8080
```

- 구조: `app/` (Flutter: customer, partner, console 과 `packages/`), `backend/` (Rust API), `deploy/` (설치 프로그램, Docker, Caddy), `website/`, `docs/`.
- 테스트와 검사: `CONTRIBUTING.md`, CI 는 `.github/workflows/ci.yml`. 매뉴얼 스크린샷 테스트는 `manual_screenshots` 태그이고 CI 에서 빠져요. 다시 만들기: `scripts/manual/shots.sh` (`SHOT_ONLY=id1,id2` 로 일부만).

---

## 33.12 더 볼 문서

| 문서 | 내용 |
|---|---|
| `docs/OPERATIONS.md` | 설치 뒤 운영 전체 |
| `docs/RUNBOOK.md` | 증상별 장애 대응 (11가지) |
| `docs/DEPLOY.md` | 릴리스, 채널, 스토어, 실서비스 가입 |
| `docs/SECURITY-AND-PRIVACY.md` | 보안 통제와 알려진 한계 |
| `docs/ARCHITECTURE.md` | 구성 요소와 데이터 흐름 |
| `docs/FEATURES.md` | 기능별 설명과 코드 위치 |
| `docs/REBRAND-CHECKLIST.md` | 이름·로고를 바꿀 때 확인할 것 |
| `CHANGELOG.md` | 버전별 변경 |

---

## 자주 생기는 문제

> **자주 생기는 문제**
>
> - **`dahae` 명령을 찾을 수 없어요** → `sudo` 를 붙였는지, 설치가 끝났는지 확인하세요. 예전 이름 `malkkeum` 도 같은 명령이에요.
> - **`restore` 가 파일을 못 찾아요** → `backups/` 로 시작하는 상대 경로를 쓰세요 (예: `backups/db-20261110-033000.sql.gz.enc`).
> - **"로컬 백업을 봉인하지 못했어요" 가 나와요** → 업데이트·복원·되돌리기가 멈춘 상태예요. `dahae logs` 로 원인을 찾고 백업을 다시 성공시키세요.
> - **`rollback` 이 정산 때문에 멈춰요** → 33.4의 토스 정산 항목대로 콘솔 **[정산]** 에서 상태를 정리한 뒤 다시 하세요.
> - **"되돌릴 이전 버전 기록이 없어요"** → 업데이트를 한 번도 하지 않은 서버예요.
