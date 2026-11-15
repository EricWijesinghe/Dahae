# 결제 연동 설정 가이드 (PortOne · 토스페이먼츠)

> 이 문서는 결제 키를 받은 뒤 채워 넣는 **빈칸 템플릿**이에요. 실제 값은 여기에 적지 마세요.
> 모든 값은 콘솔 [설정] 화면에 붙여넣어 저장해요. 서버 코드·앱·웹 파일·로그에는 넣지 않아요.

## 0. 키 종류와 공개 범위

| 항목 | 예시 형식 | 어디에 넣나 | 공개 여부 |
|---|---|---|---|
| PortOne 상점 아이디 | `store-…` | 콘솔 [설정] › 결제 › 상점 아이디 | 브라우저 SDK에 전달 가능 |
| PortOne V2 API Secret | (PortOne 발급) | 콘솔 › API Secret | **서버 전용** |
| PortOne 웹훅 시크릿 | `whsec_…` | 콘솔 › 웹훅 시크릿 | **서버 전용** |
| PortOne 채널 키 | `channel-key-…` | 콘솔 › 채널 키 (결제수단별) | 브라우저 SDK에 전달 가능 (현금영수증 채널 키는 서버 전용) |
| 토스 지급대행 시크릿 키 | `test_sk_…` / `live_sk_…` | 콘솔 › 지급대행 › 시크릿 키 | **서버 전용** |
| 토스 보안 키 (64자리 16진수) | (토스 개발자센터) | 콘솔 › 지급대행 › 보안 키 | **서버 전용** (JWE 암호화 + 웹훅 서명) |

규칙: `test_` 키와 `live_` 키는 섞지 않아요. 키를 바꾸면 [연결 테스트]를 다시 해야 해요.

## 1. 채워 넣을 값 (받으면 여기에 표시)

| 항목 | 상태 | 값 (마스킹 후 기록) |
|---|---|---|
| PortOne 상점 아이디 | 대기 | `<STORE_ID>` |
| PortOne 테스트 API Secret | 대기 | `<PORTONE_TEST_API_SECRET>` (서버 전용) |
| PortOne 채널 키 · 카드 | 대기 | `<CHANNEL_KEY_CARD>` |
| PortOne 채널 키 · 간편결제 | 대기 | `<CHANNEL_KEY_EASY_PAY>` |
| PortOne 채널 키 · 자동결제(빌링키) | 대기 | `<CHANNEL_KEY_BILLING>` |
| PortOne 채널 키 · 본인인증 | 대기 | `<CHANNEL_KEY_IDENTITY>` |
| PortOne 채널 키 · 현금영수증 | 대기 | `<CHANNEL_KEY_CASH_RECEIPT>` (서버 전용) |
| PortOne 웹훅 시크릿 | 대기 | `<PORTONE_WEBHOOK_SECRET>` (서버 전용) |
| 토스 테스트 시크릿 키 | 대기 | `<TOSS_TEST_SECRET_KEY>` (서버 전용) |
| 토스 보안 키 | 대기 | `<TOSS_SECURITY_KEY>` (서버 전용) |
| 테스트 계정 정보 | 대기 | `<TEST_ACCOUNT_NOTE>` |
| 공개 주소 (PUBLIC_BASE_URL) | 대기 | `<PUBLIC_BASE_URL>` |

## 2. 콘솔 설정 순서

1. **결제 (PortOne)** 카드: 상점 아이디 → API Secret → 채널 키 → 웹훅 시크릿 순서로 붙여넣고 [저장].
   - 상점 아이디는 `store-` 로 시작해야 하고, 채널 키는 `channel-key-` 로 시작해야 해요. 아니면 저장이 거부돼요.
   - [연결 확인]은 API Secret을 확인해요. 상점 아이디나 채널 키가 틀린 것은 첫 결제 시도 때 드러날 수 있어요.
   - 카드에 표시되는 **웹훅 주소** `<PUBLIC_BASE_URL>/v1/webhooks/portone` 을 PortOne 콘솔(V2 웹훅)에 등록해요.
2. **지급대행 (토스)** 카드: 시크릿 키 → 보안 키 → [연결 테스트] → 성공 후 지급 방식을 `toss` 로 바꿔요.
   - `test_sk_` 키는 `sandbox_connected`, `live_sk_` 키는 `live_connected` 로 기록돼요. 테스트 통과가 실서비스 키를 열어주지 않아요.
   - 토스 개발자센터 웹훅 주소: `<PUBLIC_BASE_URL>/v1/webhooks/toss` (`payout.changed`, `seller.changed`). 요청에는 `tosspayments-webhook-signature` 헤더가 있어야 하고, 서명이 틀리면 401 이에요.
3. 저장 뒤 [설정] 화면의 상태가 "연결됨"인지 확인해요.

## 3. 웹훅 점검

| 공급자 | 주소 | 검증 | 실패 시 |
|---|---|---|---|
| PortOne | `/v1/webhooks/portone` | Standard Webhooks 서명 (`webhook-signature`) | 401, PortOne이 재전송 |
| 토스 | `/v1/webhooks/toss` | `tosspayments-webhook-signature` (보안 키 기반 HMAC-SHA256) | 401 |

웹훅은 결제 상태를 바로 바꾸지 않아요. 알림을 받으면 서버가 결제 기관에 다시 조회해서 확인해요.

## 4. 테스트 계정과 테스트 결제

- PortOne 테스트 상점 채널 키로 카드 결제 1회, 부분 환불 1회를 해 보세요. 부분 환불 뒤 `refunded_amount` 가 바뀌지 않는지 확인해요 (환불 금액은 예약 환불 경로에서만 기록돼요).
- 자동결제(빌링키)는 테스트 상점에서 한 번 발급·결제·삭제까지 확인한 뒤 실서비스에 열어요.
- 현금영수증은 설정의 현금영수증 스위치가 꺼진 상태가 기본이에요. 테스트 상점에서 한 번 발급 응답 형식을 확인한 뒤 켜요.
- 테스트 계정 정보는 `<TEST_ACCOUNT_NOTE>` 칸에만 메모하고, 이 문서나 저장소에는 올리지 않아요.

## 5. 아직 확인하지 않은 항목

- PortOne 결제 거절 코드: 명세의 `type` 이름만 알려져 있어요. 실제 카드 거절 코드는 테스트 상점에서 한 번 확인해야 해요.
- 현금영수증 발급 응답 (`cashReceipt.issueNumber`): 테스트 상점 1회 발급으로 확인해요.
- 토스 웹훅 서명 계산: 명세대로 구현했어요. 테스트 웹훅 1건으로 서명이 맞는지 확인해요.
- 토스 지급 생성 응답의 래퍼 형식(`entityBody`): 두 형식 모두 읽도록 했어요. 실제 응답을 한 번 확인해요.

---

# Payment setup guide (PortOne · Toss Payments) — English

This is a **fill-in template**. Enter the real values only in the console (Settings), never in this file, code, apps, web bundles or logs.

- PortOne store id (`store-…`) and channel keys (`channel-key-…`) may reach the browser SDK. Channel key for cash receipts is server-only.
- PortOne API Secret, PortOne webhook secret (`whsec_…`), Toss secret key (`test_sk_` / `live_sk_`) and Toss security key are **server-only**.
- Never mix `test_` and `live_` keys. Changing a key requires a new connection test.

Console order: (1) PortOne card: store id, API secret, channel keys, webhook secret, Save. (2) Toss payout card: secret key, security key, connection test, then mode `toss`.

Webhook URLs (set in each provider's dashboard):
- PortOne: `<PUBLIC_BASE_URL>/v1/webhooks/portone`
- Toss payouts: `<PUBLIC_BASE_URL>/v1/webhooks/toss` (`payout.changed`, `seller.changed`), signed with `tosspayments-webhook-signature`; an invalid signature gets 401.

Webhooks never move money by themselves: the server re-reads the payment from the provider before acting.

Items still to confirm in a test store: PortOne decline codes, cash receipt response shape, Toss webhook signature against one live test webhook, Toss payout create response wrapper.

Fill-in values to provide: store id, PortOne test API secret, channel keys (card, easy pay, billing, identity, cash receipt), PortOne webhook secret, Toss test secret key, Toss security key, test account notes, public base URL.
