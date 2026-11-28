# 5. Demo install (from an image file, without GitHub)

English · [한국어](#korean)

Install a Dahae server from **one prepared image file (.tar)** instead of downloading the images from GitHub.
Use it for demos where the internet is unreliable, such as a friend's home or a venue. Once installed, the service runs without the internet.

> Developer-only guide: it builds from the source on a development PC, and it is not shipped with the delivered guides.

## What you need
| You need | From |
|---|---|
| The image file `malkkeum-demo.tar` (made in A below) | The development PC |
| Ubuntu 24.04 or Debian 12 (2 GB of memory or more) | A VM on a laptop, a demo mini PC, or a cloud server |
| `install.sh` (the repository's `deploy/install.sh`) | Copy it to the same USB stick |

> The install downloads Docker and basic packages (apt), so **the internet is needed while installing.**
> After the install it runs without the internet, because every image is on that server.

## A. Make the image file on the development PC (beforehand)

Docker Desktop must be running. Run this in Git Bash from the repository's top folder. `IMAGE_REPO` must equal the installer's
default image repository (`IMAGE_REPO_DEFAULT` in `deploy/install.sh`, `ghcr.io/<GitHub account>`).

```
IMAGE_REPO=$(sed -n 's/^IMAGE_REPO_DEFAULT=//p' deploy/install.sh)
docker build -f backend/Dockerfile --build-arg APP_VERSION=demo -t $IMAGE_REPO/malkkeum-api:demo backend
docker build -f deploy/web.Dockerfile --build-arg APP_VERSION=demo -t $IMAGE_REPO/malkkeum-web:demo .
docker pull postgres:17
docker save -o malkkeum-demo.tar $IMAGE_REPO/malkkeum-api:demo $IMAGE_REPO/malkkeum-web:demo postgres:17
```

- The first build can take 40 minutes or more. If memory runs short, add `--build-arg CARGO_BUILD_JOBS=2` to the API build line (fewer parallel compile jobs).
- `postgres:17` is the database image. Without it, the install downloads it from the internet.
- Copy `malkkeum-demo.tar` and `deploy/install.sh` to a USB stick. The `demo` in the file name is the version name; for a later update, build again under another name (for example `demo2`).

## B. Install (on the demo server)

1. Sign in to Ubuntu and plug in the USB stick. Say it is at `/media/usb`.
2. Run the command below. Use a **local address** such as `malkkeum.localhost` as the domain (no internet domain needed).
   ```
   sudo MALKKEUM_IMAGES_TAR=/media/usb/malkkeum-demo.tar \
        MALKKEUM_VERSION=demo \
        MALKKEUM_DOMAIN=malkkeum.localhost \
        MALKKEUM_EMAIL=demo@example.com \
        MALKKEUM_TLS=internal \
        bash /media/usb/install.sh
   ```
   - `MALKKEUM_TLS=internal` makes the certificate on this server, so the browser shows a "not secure" warning once. Press **Advanced → Continue**.
   - Without internet access the install stops at the apt step. Install where the internet works.
3. At the end the installer (its messages are in Korean today) says the install finished and shows the console address `https://malkkeum.localhost/admin/` and a 12-character **setup code** such as `ABCD-2345-EFGH`. Write the code down.
4. In a browser on the same computer, open `https://malkkeum.localhost/admin/`.
   Enter the setup code and create the owner account; the setup wizard starts.
   To show it on another device (for example a tablet for the presentation), install with the server's IP in `MALKKEUM_DOMAIN`. It then opens only on the same Wi-Fi.

## C. Load and clear demo data

```
sudo dahae demo load      # loads sample bookings, customers and partners (not when real data exists)
sudo dahae demo clear     # removes only the samples; real customers, bookings and settlements stay. Type "yes".
```

## D. Update and roll back (without the internet)

Build a new version file and bring it on the USB stick (the same way as A, with the tag `demo2`):

```
sudo dahae load /media/usb/malkkeum-demo2.tar     # loads the file; the screen shows the Loaded image tag
sudo dahae update demo2                            # backup, switch to demo2, check that it answers
```

- If the new version does not answer, it rolls back to the previous version by itself.
- To roll back yourself, use `sudo dahae rollback` (back to the version right before).
- Common commands: `dahae status`, `dahae version`, `dahae logs`, `dahae backup`.

## E. When the demo is over

```
sudo dahae demo clear     # remove the sample data
```

To throw the whole server away, delete the VM. To keep the VM and remove only Dahae, use `sudo dahae uninstall --purge` (you confirm by typing the domain; it deletes the data and backups and leaves Docker installed).

## A real test run (excerpt)

Installed and checked in the order above on a clean Ubuntu 24.04 VM (systemd, no Docker). The installer prints Korean; the English meaning is after each arrow.

```
✔ 이미지를 불러왔어요: /media/usb/malkkeum-demo.tar   ← image file loaded
✔ 설치가 끝났어요!   (채널: stable)                    ← install finished (channel: stable)
GET /admin/ -> 200                          ← the setup wizard screen
{"dev_mode":false,"needs_admin":true}       ← waiting for the owner account
{"ok":true}                                 ← owner account created with the setup code
ok: 3707 rows in 26 tables                  ← dahae demo load
{"ok":true,"version":"demo"}                ← /health
HTTP 200                                    ← booking list (GET /v1/admin/bookings)
✘ 업데이트에 실패해서 이전 버전으로 되돌렸어요.   ← a broken version ("bad") failed and was rolled back by itself
✔ 최신 버전(demo2)으로 업데이트했어요            ← updated to demo2
✔ 이전 버전으로 되돌렸어요                    ← rolled back (dahae rollback)
184                                          ← booking data unchanged after the update and the rollback
✔ 체험용 데이터를 지웠어요                    ← dahae demo clear (0 bookings, 1 owner account left)
```

> If the development PC runs short of memory, the Flutter build of the web image (the first stage of `deploy/web.Dockerfile`) can stall. Building Flutter on the PC first and copying the output into the image gives the same image.

## Common problems
| Symptom | Fix |
|---|---|
| The image file cannot be found (`이미지 파일을 찾을 수 없어요`) | Check the USB path (`ls /media/usb`). |
| The image file lacks the tag (`이미지 파일에 ... 이 없어요`) | The tag inside the file differs from `MALKKEUM_VERSION`. Pass the tag you built with (`demo`) as `MALKKEUM_VERSION=demo`. Without it, the installer looks for the default `local`. |
| Browser warning | Normal with `MALKKEUM_TLS=internal`. Advanced → Continue. |
| No response | Find the cause with `sudo dahae logs`. Also check that Docker runs: `systemctl status docker`. |

## Developer notes (install testing)

- `MALKKEUM_IMAGES_TAR` in `deploy/install.sh` runs `docker load` during the install; the default path (pulling from GHCR) is unchanged.
- An install from an image file records the server in local mode (`IMAGE_SOURCE=local`), so `dahae update` does not pull from the registry. Once installed locally, it stays in local mode.
- Image names must be `<IMAGE_REPO>/malkkeum-api:<tag>` (the installer and the `dahae` command look them up by this name). If you changed the repository address, set `MALKKEUM_IMAGES` too.

---

<a id="korean"></a>

# 5. 체험용 설치 (이미지 파일로, GitHub 없이)

[English](#5-demo-install-from-an-image-file-without-github) · 한국어

GitHub 에서 이미지를 내려받지 않고, 미리 만들어 둔 **이미지 파일(.tar) 하나**로 다해 서버를 설치해요.
친구 집이나 발표 장소처럼 인터넷이 불안한 곳에서 시연할 때 써요. 설치가 끝나면 서비스는 인터넷 없이도 돌아요.

## 준비물
| 준비물 | 어디서 |
|---|---|
| 이미지 파일 `malkkeum-demo.tar` (아래 A 에서 만들어요) | 개발 PC |
| Ubuntu 24.04 또는 Debian 12 (메모리 2GB 이상) | 노트북의 VM, 체험용 미니 PC, 또는 클라우드 서버 |
| `install.sh` (저장소의 `deploy/install.sh`) | 같이 USB 에 복사 |

> 설치 과정에서 Docker 와 기본 패키지(apt)를 받으니, **설치하는 순간에는 인터넷이 필요해요.**
> 설치가 끝난 뒤에는 인터넷 없이 돌아가요. 이미지가 전부 그 서버 안에 있으니까요.

## A. 개발 PC 에서 이미지 파일 만들기 (집에서 미리)

Docker Desktop 이 켜져 있어야 해요. 저장소 최상위 폴더에서 Git Bash 로 실행하세요. `IMAGE_REPO` 는 설치 프로그램의
기본 이미지 저장소(`deploy/install.sh` 의 `IMAGE_REPO_DEFAULT`, `ghcr.io/<GitHub 계정>`)와 같아야 해요.

```
IMAGE_REPO=$(sed -n 's/^IMAGE_REPO_DEFAULT=//p' deploy/install.sh)
docker build -f backend/Dockerfile --build-arg APP_VERSION=demo -t $IMAGE_REPO/malkkeum-api:demo backend
docker build -f deploy/web.Dockerfile --build-arg APP_VERSION=demo -t $IMAGE_REPO/malkkeum-web:demo .
docker pull postgres:17
docker save -o malkkeum-demo.tar $IMAGE_REPO/malkkeum-api:demo $IMAGE_REPO/malkkeum-web:demo postgres:17
```

- 처음 빌드는 40분 이상 걸릴 수 있어요. 메모리가 부족하면 `--build-arg CARGO_BUILD_JOBS=2` 를 API 빌드 줄에 붙이세요 (동시 컴파일 수를 줄여요).
- `postgres:17` 은 데이터베이스 이미지예요. 빼면 설치할 때 인터넷에서 받아요.
- 만든 `malkkeum-demo.tar` 와 `deploy/install.sh` 를 USB 에 복사하세요. 파일 이름의 `demo` 는 버전 이름이에요. 나중에 업데이트할 때 다른 이름(예: `demo2`)으로 한 번 더 만들면 돼요.

## B. 설치 (체험 서버에서)

1. Ubuntu 에 로그인하고 USB 를 꽂아 두세요. 예: `/media/usb` 에 있다고 할게요.
2. 아래 명령을 실행해요. 도메인은 `malkkeum.localhost` 처럼 **로컬 주소**를 써요 (인터넷 도메인이 없어도 돼요).
   ```
   sudo MALKKEUM_IMAGES_TAR=/media/usb/malkkeum-demo.tar \
        MALKKEUM_VERSION=demo \
        MALKKEUM_DOMAIN=malkkeum.localhost \
        MALKKEUM_EMAIL=demo@example.com \
        MALKKEUM_TLS=internal \
        bash /media/usb/install.sh
   ```
   - `MALKKEUM_TLS=internal` 은 인증서를 이 서버 안에서 만들어요. 그래서 브라우저에서 "안전하지 않음" 경고가 한 번 나와요. **고급 → 계속** 을 누르면 돼요.
   - 인터넷에 나갈 수 없는 상태라면 apt 단계에서 멈춰요. 그때는 인터넷 되는 곳에서 설치하세요.
3. 끝나면 이런 화면이 나와요. **설치 코드**(12자리)를 꼭 적어 두세요.
   ```
   ✔ 설치가 끝났어요!   (채널: stable)
   1) 브라우저에서 이 주소를 여세요:   https://malkkeum.localhost/admin/
   2) 설치 코드를 입력하고 사장님 계정을 만드세요:
          설치 코드:  ABCD-2345-EFGH
   ```
4. 같은 컴퓨터의 브라우저에서 `https://malkkeum.localhost/admin/` 을 열어요.
   설치 코드를 입력하고 사장님 계정을 만들면 설정 마법사가 시작돼요.
   다른 기기(예: 발표용 태블릿)에서 보려면 서버의 IP 를 `MALKKEUM_DOMAIN` 에 넣고 설치하세요. 이 경우 같은 Wi-Fi 에서만 열려요.

## C. 체험 데이터 넣기 · 지우기

```
sudo dahae demo load      # 예약·손님·파트너 샘플을 넣어요 (실제 데이터가 있으면 넣지 않아요)
sudo dahae demo clear     # 샘플만 지워요. 실제 고객·예약·정산은 건드리지 않아요. "yes" 를 입력해야 해요.
```

## D. 업데이트 · 되돌리기 (인터넷 없이)

새 버전 파일을 만들어 USB 로 옮긴 뒤 (A 와 같은 방법, 태그만 `demo2` 로):

```
sudo dahae load /media/usb/malkkeum-demo2.tar     # 파일을 불러와요. 화면에 Loaded image 태그가 나와요
sudo dahae update demo2                            # 백업 → demo2 로 바꾸기 → 응답 확인
```

- 새 버전이 응답하지 않으면 자동으로 이전 버전으로 되돌려요.
- 직접 되돌리려면 `sudo dahae rollback` 을 써요 (바로 전 버전으로 돌아가요).
- 자주 쓰는 명령: `dahae status`, `dahae version`, `dahae logs`, `dahae backup`.

## E. 체험이 끝나면

```
sudo dahae demo clear     # 샘플 데이터 지우기
```

서버를 통째로 버리려면 VM 을 지우면 끝이에요. VM 은 두고 다해만 지우려면 `sudo dahae uninstall --purge` 를 써요 (도메인을 입력해 확인해요. 데이터와 백업까지 지우고 Docker 는 남겨요).

## 실제 시험 기록 (발췌)

깨끗한 Ubuntu 24.04 VM(systemd, Docker 미설치)에서 위 순서대로 설치하고 확인한 기록이에요.

```
✔ 이미지를 불러왔어요: /media/usb/malkkeum-demo.tar
✔ 설치가 끝났어요!   (채널: stable)
GET /admin/ -> 200                          ← 설정 마법사 화면
{"dev_mode":false,"needs_admin":true}       ← 사장님 계정을 기다리는 상태
{"ok":true}                                 ← 설치 코드로 사장님 계정 생성
ok: 3707 rows in 26 tables                  ← dahae demo load
{"ok":true,"version":"demo"}                ← /health
HTTP 200                                    ← 예약 목록 (GET /v1/admin/bookings)
✘ 업데이트에 실패해서 이전 버전으로 되돌렸어요.   ← 고장난 버전(bad)은 자동으로 되돌려요
✔ 최신 버전(demo2)으로 업데이트했어요
✔ 이전 버전으로 되돌렸어요                    ← dahae rollback
184                                          ← 업데이트·되돌리기 후에도 예약 데이터 그대로
✔ 체험용 데이터를 지웠어요                    ← dahae demo clear (예약 0건, 사장님 계정 1명 남음)
```

> 개발 PC 의 메모리가 부족하면 웹 이미지의 Flutter 빌드(`deploy/web.Dockerfile` 첫 단계)가 멈출 수 있어요. 그때는 Flutter 를 PC 에서 먼저 빌드하고 결과물을 이미지에 복사해도 같은 이미지가 돼요.

## 자주 막히는 곳
| 증상 | 해결 |
|---|---|
| `이미지 파일을 찾을 수 없어요` | USB 경로가 맞는지 확인 (`ls /media/usb`). |
| `이미지 파일에 ... 이 없어요` | 파일 안의 태그가 `MALKKEUM_VERSION` 과 다른 경우예요. 만들 때 쓴 태그(`demo`)를 `MALKKEUM_VERSION=demo` 로 같이 넣으세요. 안 넣으면 기본값 `local` 을 찾아요. |
| 브라우저 경고 | `MALKKEUM_TLS=internal` 이라서 정상이에요. 고급 → 계속. |
| 응답이 없어요 | `sudo dahae logs` 로 원인을 확인하세요. `docker` 가 실행 중인지 `systemctl status docker` 도 확인. |

---

## 개발자 메모 (설치 테스트)

- `deploy/install.sh` 의 `MALKKEUM_IMAGES_TAR` 가 설치 때 `docker load` 를 하고, 기본 경로(GHCR 에서 내려받기)는 그대로예요.
- 이미지 파일로 설치하면 서버가 로컬 모드(`IMAGE_SOURCE=local`)로 기록돼서 `dahae update` 가 저장소에서 내려받지 않아요. 한 번 로컬로 설치하면 계속 로컬 모드예요.
- 이미지 이름은 `<IMAGE_REPO>/malkkeum-api:<태그>` 형식이어야 해요 (installer 와 `dahae` 명령이 이 이름으로 찾아요). 저장소 주소를 바꿨다면 `MALKKEUM_IMAGES` 도 같이 지정하세요.
