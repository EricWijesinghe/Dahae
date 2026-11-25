# 5-Store-Apple · iOS 빌드 안내 / iOS build guide

**이 폴더에는 .ipa 가 없어요.** iOS 앱은 Windows 에서 빌드할 수 없어요 (Xcode 와 macOS 가 필요). 그래서 GitHub Actions 의 macOS 러너에서 빌드합니다. 저장소의 `.github/workflows/store-apps.yml` 이 설정 파일이에요.
**There is no .ipa in this folder.** iOS cannot be built on Windows (it needs Xcode on macOS). Builds run on a GitHub Actions macOS runner from `.github/workflows/store-apps.yml` in the repository.

## 1. 최초 1회 설정 / One-time setup (GitHub repository settings)
1. 저장소 Settings > Environments > `store`: 필수 승인자 (DODA 담당자) 를 넣고 배포 브랜치를 `master` 로 제한.
2. 같은 환경의 Variables: `API_BASE` (https://서버 도메인, 필수), `APPLE_TEAM_ID`, `APP_LINK_DOMAIN` (선택).
3. 같은 환경의 Secrets: App Store Connect 키 `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_P8_B64`, 로그인·푸시 키 (선택). 목록은 `docs/DEPLOY.md` §3.
4. Apple Developer 에서 번들 ID `kr.co.dahae.customer` 와 `kr.co.dahae.partner` 를 등록.

## 2. 빌드 실행 / Run the builds
- Actions > `store-apps` > Run workflow: `app` = customer / partner / both, `platform` = ios 또는 both, `upload` = true 면 TestFlight 로 올려요.
- 워크플로는 빌드 전에 `check-native.py` 를 먼저 실행해 설정 오류를 막아요.

## 3. App Store Connect 체크리스트 / Checklist
- [ ] 앱 등록 (이름, 기본 언어, SKU, 번들 ID) / App record created
- [ ] 개인정보 라벨: `../3-Store-Android/data-safety.md` 의 App Store 항목, `app-privacy.ko.md` 답안 (영어: `app-privacy.md`) / App Privacy answers (`app-privacy.md`)
- [ ] 심사 메모: `review-notes.ko.md` 를 보고 영어판 `review-notes.md` 붙여 넣기 (로그인, 계정 삭제, 결제 안내) / Review notes pasted
- [ ] 스크린샷 (6.7", 6.5" 등 요구 크기) / Screenshots at required sizes
- [ ] 연령 등급: `content-rating-draft.md` 답안 기준 / Age rating from the draft
- [ ] 계정 삭제 경로 (앱 내) 가 심사 메모와 같은지 확인 / In-app account deletion matches the notes
- [ ] 수출 규정 (암호화 사용 여부) 답변 / Export compliance answered

## 4. 제출 파일 / Submission files
- `upload` 를 켜면 `.ipa` 가 TestFlight 에 올라가요. App Store 제출은 App Store Connect 에서 빌드를 골라 진행하세요.
- 이 패키지에 .ipa 를 넣지 않았어요. 빌드가 한 번 성공한 뒤 워크플로 아티팩트 (30일 보관) 에서 받으세요.

Files: with `upload` on, the .ipa goes to TestFlight. Pick the build in App Store Connect for review.

## 5. 아직 확인 안 된 것 / Not verified here
- 이 PC 에서는 iOS 빌드와 서명을 실행하지 않았어요. 첫 `store-apps` 실행 (macOS 러너, Xcode 26.6, 자동 서명) 결과로 확인해야 해요.
- Not built or signed on this PC; the first `store-apps` run on the macOS runner (Xcode 26.6, automatic signing) is the real test.
