# 이름·로고·도메인 바꾸기 — 콘솔 안내 / Rebrand from the console

이름, 로고, 색, 도메인, 결제·문자 연결과 운영 설정은 모두 **콘솔에서** 바꿔요. 터미널도 코드 수정도 필요 없어요.
저장하면 서버가 보내는 문자·결제 이름, 웹사이트, 약관 페이지, 웹 앱, 설치된 고객·파트너 앱이 한 번에 바뀌어요.
그렇게 바뀔 수 없는 것(스토어 앱 빌드, 바깥 회사의 관리 화면)은 콘솔이 무엇을 해야 하는지 목록으로 알려 줘요.
아래는 그 순서와, 사장님이 바깥 사이트에서 직접 해야 하는 일이에요.

## 1. 콘솔에서 바꾸는 것 (저장하면 바로 반영)

| 무엇 | 콘솔 위치 | 언제 보이나요 |
|---|---|---|
| 서비스 이름, 영문 이름, 짧은 이름(12자), 한 줄 소개, 대표 색 | 설정 > 사업자 정보 | 문자·알림톡·결제 이름은 바로, 웹사이트·약관은 다음 페이지 열 때, 앱은 다음 요청 때 |
| 로고 | 설정 > 사업자 정보 | 저장하면 콘솔이 아이콘·공유 이미지 7가지를 만들어 올려요. 로고가 없으면 이름 첫 글자 마크를 만들어요 |
| 사업자 정보(상호, 대표, 사업자번호, 통신판매, 주소, 개인정보 책임자·이메일, 호스팅) | 설정 > 사업자 정보 | 웹사이트 바닥글·약관 (전자상거래법) |
| 결제(PortOne·토스페이먼츠), 문자(Solapi), 푸시, 소셜 로그인, AI, 외부 백업, 주소 검색 | 설정의 각 카드 | 저장 즉시. 약관의 "처리 위탁" 칸도 켠 카드에 맞춰 바뀌어요 |
| 지급대행(토스) 켜기 | 설정 > 지급대행 (사장님 인증) | 저장 즉시 |
| 영업시간 (AI 상담 꺼짐 시간) | 설정 > 영업시간 | 저장 즉시 |
| 업데이트 채널·자동 업데이트 시각·지금 백업·재시작·외부 가동 확인 주소 | 시스템 > 서버 | 서버 작업으로 실행 (사장님 인증) |
| 앱 링크 (Android 지문, Apple 팀 ID, 스토어 앱 ID) | 설정 > 앱 링크 (사장님 코드) | `/.well-known/*` 파일이 바로 바뀌어요 |
| 도메인 | 설정 > 도메인 | 2번 참고 |

여러 서버를 쓰는 경우에도 다른 서버는 1분 안에 따라와요.

## 2. 도메인 바꾸기

1. 새 도메인의 A 레코드(그리고 `www.새도메인`)를 서버 IP 로 넣어요. **예전 도메인의 레코드는 지우지 마세요.**
2. 설정 > 도메인 → 새 도메인 → [DNS 확인] → [바꾸기] → 사장님 인증 코드 → 서버 승인 코드.
3. 30초쯤 멈췄다가 새 주소로 돌아와요. 새 주소가 응답하지 않으면 서버가 스스로 되돌리고 이유를 보여 줘요.
4. 예전 주소는 계속 응답해요. 설치된 앱은 예전 주소로 계속 작동하고, 새 버전 앱은 다음 실행 때 새 주소로 옮겨 가요.
   웹사이트 페이지만 새 주소로 넘어가요(검색 엔진용).
5. 화면의 체크리스트를 복사 버튼으로 하나씩 처리해요: PortOne·토스 웹훅, PG 계약의 사이트 주소(PG 가 다시 심사할 수 있어요),
   네이버 콜백 **추가**(예전 것은 남겨 둬요), 네이버 서비스 URL, 카카오 사이트 도메인, Google 동의 화면 링크, 검색 등록(Search Console·
   Search Advisor), 스토어의 개인정보·계정 삭제 주소, GitHub `store` 변수(`API_BASE`, `APP_LINK_DOMAIN=새,예전`), 알림톡 웹 링크 버튼.
6. 새 스토어 빌드가 나오면 사업자 정보의 최소 앱 버전을 올리고, 그 다음에만 예전 주소를 **은퇴**시켜요.

처음 한 번: 이 기능 전에 설치한 서버는 설치 줄을 한 번 다시 실행해야 해요 ([guide 08](guide/08-클라우드-배포.md) 8-3).

## 3. 새 스토어 빌드가 있어야 바뀌는 것

콘솔의 **스토어 앱 빌드** 카드가 바뀐 항목(이름, 아이콘, 색, 앱 ID, 도메인)을 보여 줘요. 빌드는 [guide 11](guide/11-앱-직접-빌드.md).
빌드가 이름·아이콘을 서버에서 받아 오므로 저장소를 고칠 필요는 없어요. 앱 ID 는 GitHub `store` 변수에서 받아요.
콘솔 앱 링크의 앱 ID 와 같게 넣으세요 (다르면 네이버 로그인 앱 링크가 확인되지 않아요).

- 스토어 앱 ID (`CUSTOMER_APP_ID`, `PARTNER_APP_ID`): 새 ID 는 새 스토어 앱이에요. 첫 업로드 전에 정하세요.
  Firebase 앱도 같은 ID 로 등록해야 해요.
- 홈 화면 이름, 아이콘, 시작 화면, 스토어 설명·스크린샷·개인정보 주소
- 서명 키와 Apple 팀, Firebase 파일, 카카오·네이버·Google 의 앱 키
- 앱 링크 도메인 (네이버 로그인)
- **Windows 패키지 게시자 (스토어 업로드 전)**: 빌드 기본값은 임시 `CN=Dahae` / `Dahae` 예요. Partner Center 의 실제 게시자 값은
  `scripts/store/msix.local.psd1` 에 넣으세요 (Git 에 올리지 않아요).

## 4. 바깥 사이트에서 직접 바꾸는 것

- [ ] **상표 확인**: KIPRIS(https://www.kipris.or.kr)에서 새 이름을 37류(청소업), 35류(중개·광고), 9류(앱)로 검색하세요.
- [ ] **결제대행 가맹점 이름**: PortOne·토스·카카오페이·네이버페이 관리 화면의 가맹점 이름과 카드 명세서 표시 이름.
- [ ] **카카오톡 채널 이름**과 **알림톡 템플릿**: 채널 이름은 카카오 심사가 필요해요. 템플릿은 `#{brand}` 변수를 쓰면 본문이
      자동으로 바뀌지만, 이름이나 주소를 직접 적은 템플릿은 새로 승인받아야 해요 ([guide 06](guide/06-알림톡-템플릿-등록.md)).
- [ ] **문자 발신번호** 등록 내용, **소셜 로그인 동의 화면**의 앱 이름, **스토어 등록 이름**.
- [ ] **약관 변경 공지 (시행 7일 전)**: 콘솔에서 약관 문구를 게시하면 앱과 웹사이트에 같은 글이 나와요. 시행일 7일 전에 공지하세요.

## 5. 바뀌지 않는 것

- 소프트웨어 표시 "Powered by Dahae · 소프트웨어 © 2026 Eric Wijesinghe" 는 라이선스 조건이라 모든 페이지에 남아요.
- 콘솔 웹 앱의 설치 ID(`malkkeum-console`)는 그대로예요. 바꾸면 이미 설치한 콘솔이 둘로 갈라져요.
- 서비스 지역(서울), 예약 시작 시각, 휴일, 통화(원), 언어(한국어·영어), 시간대(KST)는 코드 변경이 필요해요.

---

## English

Everything an operator configures (name, English and short name, tagline, colour, logo, company details, gateways,
SMS, push, social, AI, backup, payouts switch, business hours, server jobs, app links, domain) is changed in the
console and reaches the server texts, the website and legal pages, the web apps and installed apps at once.
A domain move keeps the old domain serving everything except website pages (302), so installed apps keep working;
the console then lists the outside consoles to update with copy buttons, and the old domain is retired only after
new store builds are live and `min_app_version` is raised. Store builds read the name and icons from the server and the app ids
from the GitHub `store` variables (keep them equal to the app ids in the console's app links card); the console's store build card says when one is needed. Servers installed before this feature need the
install line run once (new job runner and CLI). The licensor credit stays on every page.
