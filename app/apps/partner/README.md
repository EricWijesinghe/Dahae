# 다해 파트너 (Dahae Partner)

다해 파트너는 청소 전문가(파트너)가 배정된 일을 확인하고, 방문 현장에서 체크인·체크아웃하고, 사진을 올리고,
정산 내역을 보는 Windows 앱이에요. Flutter로 만들었고 서버 API(`backend/`)와 통신해요.
A Flutter app for cleaning professionals: assigned jobs, check-in and check-out, completion photos, and payout history.
Windows is the main target; the same code also runs on Android, iOS and in a browser.

## 구성 · Layout

| 경로 / Path | 내용 / What it is |
|---|---|
| `lib/` | 화면과 상태 / screens and state |
| `test/` | 위젯·단위 테스트 / widget and unit tests |
| `integration_test/` | 전체 흐름 테스트 (실제 기기 또는 데스크톱 필요) / end-to-end flow |
| `windows/`, `android/`, `ios/` | 플랫폼 설정 / platform projects |
| `pubspec.yaml` `msix_config` | Windows 스토어 패키지 설정 (표시 이름, 게시자) / Windows store package metadata |

공통 코드(디자인, API 클라이언트, 다국어)는 `app/packages/ui`, `app/packages/core` 에 있어요.
Shared design, API client and translations live in `app/packages/ui` and `app/packages/core`.

## 실행 · Run

필요한 것: Flutter SDK(`app/` 워크스페이스), Windows 개발 도구(Visual Studio C++ 워크로드).
Requirements: Flutter SDK (workspace in `app/`) and Visual Studio with the C++ desktop workload for Windows.

```bash
cd app/apps/partner
flutter pub get
flutter run -d windows
```

로컬 서버에 연결하려면 API 서버를 먼저 켜세요 (개발용 DB와 API 실행은 `docs/DEPLOY.md` 참고).
Start the local API first (see `docs/DEPLOY.md`). Without `API_BASE`, debug builds use `http://127.0.0.1:8080`
(`http://10.0.2.2:8080` on the Android emulator).

다른 서버에 붙일 때는 주소를 빌드 시 지정해요:
To point at another server, pass the address at build time:

```bash
flutter run -d windows --dart-define=API_BASE=https://도메인
```

`API_BASE` 가 없는 릴리스 빌드는 실행되지 않아요. 로그인 정보가 평문으로 나가는 것을 막기 위한 설정이에요.
A release build without `API_BASE` refuses to start, so logins are never sent in cleartext.

## 테스트 · Test

```bash
cd app/apps/partner
flutter test
```

- 골든(화면 픽셀 비교) 테스트는 CI 러너에서만 실행돼요. 로컬에서는 건너뛰어요.
  Golden (pixel) tests run only on the CI runner, so `flutter test` skips them locally.
  CI runs them with `flutter test --run-skipped --tags golden`.
- 전체 흐름 테스트: `flutter test integration_test` (기기나 데스크톱 대상 필요).
  End-to-end flow: `flutter test integration_test`, needs a target device.
- 이 PC에서 무거운 빌드·테스트는 `bash F:/EC/scripts/build-slot.sh <명령>` 을 통해 실행하세요.
  On the shared build PC, run heavy commands through `scripts/build-slot.sh`.

## 빌드 · Build

개발용 실행 파일:
Local build:

```bash
flutter build windows --dart-define=API_BASE=https://도메인
```

스토어 제출용 MSIX (저장소 루트에서):
Store package (from the repository root):

```powershell
pwsh scripts/store/build-msix.ps1 -Apps partner -Store -ApiBase https://도메인
```

- 결과 파일: `dist/msix/` 아래 파트너 `.msix` 파일 (서명되지 않은 스토어용 패키지).
  Output: an unsigned partner `.msix` under `dist/msix/`.
- 게시자(Publisher)와 제품 ID는 Git에 올리지 않아요. `scripts/store/msix.local.psd1` (gitignored) 또는 환경 변수에 넣어요.
  Publisher and identity values are never committed. Put them in `scripts/store/msix.local.psd1` (gitignored) or environment variables.
  `pubspec.yaml` 의 값은 자리표시자예요.
  The values in `pubspec.yaml` are placeholders only.
- 테스트용 설치(자체 서명): `-Store` 없이 실행하면 `scripts/store/.local` 의 테스트 인증서로 서명해요.
  Local test install: run without `-Store` to sign with the test certificate in `scripts/store/.local`.

## 스토어 제출 · Store submission

절차 전체는 `docs/store/microsoft-store.md` 에 있어요. 요약:
The full steps are in `docs/store/microsoft-store.md`. In short:

1. Partner Center에서 `다해 파트너` 앱을 예약하고 제품 ID 값을 `msix.local.psd1` 에 넣어요.
   Reserve the `다해 파트너` app in Partner Center and copy its identity values into `msix.local.psd1`.
2. `-Store -ApiBase https://도메인` 으로 패키지를 만들고 Partner Center에 올려요.
   Build the store package and upload it.
3. 스토어 문구와 스크린샷은 `docs/store/` 에서 관리해요. 예전 이름이 남아 있으면 안 돼요.
   Store copy and screenshots are kept in `docs/store/`.

앱 ID(`kr.co.dahae.partner`)와 Windows 게시자 값은 사장님 확인 후 정해요. 자세한 내용은 `docs/REBRAND-CHECKLIST.md` 3번.
The app ID (`kr.co.dahae.partner`) and the Windows publisher are owner decisions; see section 3 of `docs/REBRAND-CHECKLIST.md`.
