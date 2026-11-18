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
| 토스페이먼츠 클라이언트 키 (결제창 MID) | `test_ck_…` / `live_ck_…` | 콘솔 › 결제 (토스페이먼츠) › 클라이언트 키 | 브라우저 SDK에 전달 가능 |
| 토스페이먼츠 시크릿 키 (결제창 MID) | `test_sk_…` / `live_sk_…` | 콘솔 › 결제 (토스페이먼츠) › 시크릿 키 | **서버 전용** |
| 토스페이먼츠 자동결제 키 쌍 (빌링 MID, 선택) | `test_ck_…` + `test_sk_…` | 콘솔 › 결제 (토스페이먼츠) › 자동결제 클라이언트 키 / 시크릿 키 | 클라이언트 키만 브라우저, 시크릿 키는 **서버 전용** |
| 토스페이먼츠 보안 키 (선택) | 64자리 16진수 | 콘솔 › 결제 (토스페이먼츠) › 보안 키 | **서버 전용** (웹훅 서명) |
| 토스 지급대행 시크릿 키 | `test_sk_…` / `live_sk_…` | 콘솔 › 지급대행 › 시크릿 키 | **서버 전용** |
| 토스 보안 키 (64자리 16진수) | (토스 개발자센터) | 콘솔 › 지급대행 › 보안 키 | **서버 전용** (JWE 암호화 + 웹훅 서명) |

규칙: `test_` 키와 `live_` 키는 섞지 않아요. 키를 바꾸면 [연결 확인]를 다시 해야 해요.
토스페이먼츠 결제 카드에는 **API 개별 연동 키**(`ck`/`sk`)를 넣어요. 위젯 연동 키(`gck`/`gsk`)는 받지 않아요.

## 1. 채워 넣을 값 (받으면 여기에 표시)

| 항목 | 상태 | 값 (마스킹 후 기록) |
|---|---|---|
| PortOne 상점 아이디 | 대기 | `<STORE_ID>` |
| PortOne 테스트 API Secret | 대기 | `<PORTONE_TEST_API_SECRET>` (서버 전용) |
| PortOne 채널 키 (결제수단마다 1개: card, card_billing, kakaopay, naverpay, tosspay, transfer, virtual_account, identity, cash_receipt 중 쓰는 것) | 대기 | `<CHANNEL_KEYS>` (cash_receipt 는 서버 전용) |
| PortOne 웹훅 시크릿 | 대기 | `<PORTONE_WEBHOOK_SECRET>` (서버 전용) |
| 토스페이먼츠 결제창 MID 테스트 키 쌍 | 대기 | `<TOSSPAYMENTS_TEST_CK>` / `<TOSSPAYMENTS_TEST_SK>` (sk 는 서버 전용) |
| 토스페이먼츠 자동결제 MID 테스트 키 쌍 (선택) | 대기 | `<TOSSPAYMENTS_BILLING_TEST_CK>` / `<TOSSPAYMENTS_BILLING_TEST_SK>` (sk 는 서버 전용) |
| 토스 테스트 시크릿 키 | 대기 | `<TOSS_TEST_SECRET_KEY>` (서버 전용) |
| 토스 보안 키 | 대기 | `<TOSS_SECURITY_KEY>` (서버 전용) |
| 테스트 계정 정보 | 대기 | `<TEST_ACCOUNT_NOTE>` |
| 공개 주소 (PUBLIC_BASE_URL) | 대기 | `<PUBLIC_BASE_URL>` |

## 2. 콘솔 설정 순서 (붙여넣고 잠그기)

콘솔 **설정 › 결제 연결** 에서 카드별로 값을 붙여넣어요. 칸 오른쪽의 눈 아이콘으로 입력 중인 값을 가리거나 보여줄 수 있어요.

1. **PortOne** 카드: 상점 아이디, V2 API Secret, 웹훅 시크릿, 그리고 결제수단별 채널 키 칸을 채우고 **저장**.
   - 채널 키 칸: 일반결제(카드), 정기결제(빌링키), 카카오페이, 네이버페이, 토스페이, 가상계좌, 본인인증, 현금영수증, 계좌이체(transfer). 콘솔에는 페이코·삼성페이·애플페이·휴대폰 칸도 있어요. 쓰지 않는 칸은 비워 두세요. 채널 키가 없는 결제수단은 고객 앱에 보이지 않아요 (개발 모드 제외).
   - 각 칸 아래 한 줄 설명이 값을 어디서 복사하는지 알려줘요. 형식이 맞지 않으면 저장 전에 칸 아래에 문구가 떠요.
   - 카드를 처음 저장하는 것은 소유자 인증 코드가 필요해요. 결제 금액이 가는 곳을 정하기 때문이에요.
   - [API 키 확인]은 API Secret만 확인해요. 상점 아이디나 채널 키가 틀린 것은 첫 결제 시도 때 드러날 수 있어요.
   - 기존 설치 호환: 카드 채널 키만 넣었던 설치는 정기결제가 그 `card` 키를 그대로 써요. 정기결제 칸을 채우면 그 칸이 우선해요. 현금영수증은 `cash_receipt` 칸이 없으면 결제수단(계좌이체·가상계좌)의 채널 키를 써요.
   - PortOne 콘솔(V2 웹훅)에 카드 옆 복사 버튼의 **웹훅 주소** `<PUBLIC_BASE_URL>/v1/webhooks/portone` 을 등록해요.
2. **결제 (토스페이먼츠)** 카드 (선택, PortOne 대신 새 결제를 받고 싶을 때): 자세한 순서는 [6절](#6-토스페이먼츠로-결제받기)을 보세요.
   - **사용** 스위치를 끈 채로 키를 넣고 **저장** (처음 저장은 소유자 인증 코드) → [API 키 확인] → 토스 개발자센터에 웹훅 등록 → 잠금 해제 → **사용** 켜고 저장.
   - 화면의 "결제 연결" 제목 아래에 **새 결제를 받는 곳**이 PortOne 인지 토스페이먼츠인지 보여요.
3. **지급대행 (토스)** 카드: 시크릿 키(`test_sk_` 또는 `live_sk_`)와 보안 키(64자리)를 붙여넣고 **저장** → [연결 확인] → 성공 후 지급 방식을 `toss` 로 바꿔요.
   - 카드의 **테스트** / **실거래** 표시는 시크릿 키 종류에서 와요. 테스트 통과가 실서비스 키를 열어주지 않아요.
   - 저장하면 카드가 잠겨요. [연결 확인]은 사장님(소유자) 계정이 잠긴 카드에서 바로 할 수 있어요 (잠금 해제 필요 없음).
   - 토스 개발자센터 웹훅 주소: `<PUBLIC_BASE_URL>/v1/webhooks/toss` (`payout.changed`, `seller.changed`). 요청에는 `tosspayments-webhook-signature` 헤더가 있어야 하고, 서명이 틀리면 401 이에요.
4. **잠금**: 저장하면 카드가 **잠김** 상태가 돼요. 비밀 값(API Secret, 시크릿 키, 웹훅 시크릿, 보안 키, 채널 키)은 `••••` 로만 보이고 일부도 보이지 않아요. 상점 아이디와 지급 방식처럼 식별용 값만 그대로 보여요.
   - 값을 바꾸려면 **잠금 해제**를 눌러요. 소유자 계정만 가능하고, 소유자 인증 앱의 6자리 코드가 필요해요. 새 값을 붙여넣고 **저장**하면 다시 잠겨요.
   - 잠금 해제는 5분 뒤 자동으로 잠겨요. 카드에 남은 시간이 "N분 후 잠겨요"로 보여요. 바로 잠그려면 **잠그기**를 눌러요.
   - 잠금, 잠금 해제, 저장, 실패한 해제 시도는 감사 기록(감사 로그)에 남아요. 기록에는 값이 들어가지 않아요.
5. 저장 뒤 [설정] 화면의 상태가 "연결됨"인지 확인해요.

## 3. 웹훅 점검

| 공급자 | 주소 | 검증 | 실패 시 |
|---|---|---|---|
| PortOne | `/v1/webhooks/portone` | Standard Webhooks 서명 (`webhook-signature`) | 401, PortOne이 재전송 |
| 토스페이먼츠 (결제) | `/v1/webhooks/tosspayments` | 보안 키가 있고 `tosspayments-webhook-signature` 헤더가 오면 HMAC-SHA256 확인. 헤더가 없으면 알림으로만 받고 다시 조회 | 서명이 틀리면 401. 5xx 면 토스가 최대 7번 재전송 |
| 토스 (지급대행) | `/v1/webhooks/toss` | `tosspayments-webhook-signature` (보안 키 기반 HMAC-SHA256) | 401 |

웹훅은 결제 상태를 바로 바꾸지 않아요. 알림을 받으면 서버가 결제 기관에 다시 조회해서 확인해요.

## 4. 테스트 계정과 테스트 결제

- PortOne 테스트 상점 채널 키로 카드 결제 1회, 부분 환불 1회를 해 보세요. 부분 환불 뒤 `refunded_amount` 가 바뀌지 않는지 확인해요 (환불 금액은 예약 환불 경로에서만 기록돼요).
- 자동결제(빌링키)는 테스트 상점에서 한 번 발급·결제·삭제까지 확인한 뒤 실서비스에 열어요.
- 현금영수증은 설정의 현금영수증 스위치가 꺼진 상태가 기본이에요. 테스트 상점에서 한 번 발급 응답 형식을 확인한 뒤 켜요.
- 토스페이먼츠는 [6절의 테스트 순서](#테스트-키로-먼저-확인할-것)를 따라요.
- 테스트 계정 정보는 `<TEST_ACCOUNT_NOTE>` 칸에만 메모하고, 이 문서나 저장소에는 올리지 않아요.

## 5. 아직 확인하지 않은 항목

- PortOne 결제 거절 코드: 명세의 `type` 이름만 알려져 있어요. 실제 카드 거절 코드는 테스트 상점에서 한 번 확인해야 해요.
- 현금영수증 발급 응답 (`cashReceipt.issueNumber`): 테스트 상점 1회 발급으로 확인해요.
- 토스 웹훅 서명 계산: 명세대로 구현했어요. 테스트 웹훅 1건으로 서명이 맞는지 확인해요.
- 토스 지급 생성 응답의 래퍼 형식(`entityBody`): 두 형식 모두 읽도록 했어요. 실제 응답을 한 번 확인해요.
- 토스페이먼츠 결제: 실제 테스트 키로만 확인할 수 있는 항목은 [6절](#테스트-키로-먼저-확인할-것) 끝에 있어요.

## 6. 토스페이먼츠로 결제받기

PortOne 은 그대로 둔 채, 새 결제만 토스페이먼츠(직접 연동)로 받을 수 있어요. 언제든 다시 PortOne 으로 돌릴 수 있어요.

### 먼저 필요한 계약

- 토스페이먼츠 **전자결제** 계약 (결제창용 상점아이디, MID).
- 카카오페이·네이버페이 등 간편결제는 결제사마다 따로 심사·승인을 받아요.
- **자동결제(빌링)** 계약: 저장한 카드로 결제하거나 정기 방문을 쓸 때만. 정기결제 전용 심사가 따로 있고, MID 도 결제창과 달라요.
- 가상계좌와 현금영수증은 토스페이먼츠 상점관리자에서 설정해요.

자동결제 계약이 없으면 토스페이먼츠 모드에서 카드를 저장할 수 없어요. 카드는 매번 결제창으로 결제하고, 새 정기 방문은 만들 수 없어요. 이미 PortOne 으로 저장한 카드는 PortOne 으로 계속 결제돼요.

### 키 받기 (개발자센터)

1. developers.tosspayments.com 에 로그인 → **API 키** 메뉴 → 결제창 MID 선택 → **API 개별 연동 키**의 클라이언트 키(`test_ck_`)와 시크릿 키(`test_sk_`)를 복사해요. 위젯 연동 키(`gck`/`gsk`)가 아니에요.
2. 자동결제 계약이 있으면 자동결제 MID 를 골라 그 MID 의 `ck`/`sk` 를 복사해요. 두 칸은 함께 넣거나 함께 비워요.
3. 보안 키(64자리)는 선택이에요. 넣으면 웹훅 서명을 확인해요.
4. 네 개의 키는 모두 테스트 키이거나 모두 라이브 키여야 해요. 섞으면 저장할 때 거절돼요 (`key_mode_mismatch`).

### 콘솔 순서

1. **결제 (토스페이먼츠)** 카드에 키를 넣어요. **사용** 스위치는 끈 채로 둬요.
2. **저장**: 처음 저장은 소유자 인증 코드가 필요해요. 저장하면 카드가 잠겨요.
3. **[API 키 확인]**: 결제창 MID 키, 그다음 자동결제 MID 키를 확인해요. 자동결제 키만 틀리면 "자동결제(빌링) 키가 올바르지 않아요"가 떠요. 카드에 **테스트** / **실거래** 표시가 시크릿 키 종류대로 떠요.
4. **웹훅 등록**: 카드 옆 복사 버튼의 주소 `<PUBLIC_BASE_URL>/v1/webhooks/tosspayments` 를 개발자센터 **웹훅** 메뉴에서 **결제창 MID** 에 등록하고, `PAYMENT_STATUS_CHANGED` 와 `DEPOSIT_CALLBACK` 이벤트를 골라요. 자동결제 MID 에는 등록하지 않아도 돼요 (자동결제 승인은 웹훅이 오지 않아요). 지급대행 주소 `/v1/webhooks/toss` 와 헷갈리지 마세요.
5. **켜기**: 잠금 해제(소유자 코드) → **사용** 켜기 → 저장. "결제 연결" 제목 아래가 "새 결제를 받는 곳: 토스페이먼츠"로 바뀌어요.
6. **되돌리기**: 잠금 해제 → **사용** 끄기 → 저장. 전환은 매번 감사 기록에 남아요.

### 전환 규칙 (무엇이 어디로 가나)

| 대상 | 처리하는 곳 |
|---|---|
| 새 결제 | 지금 켜져 있는 곳 (토스페이먼츠 카드가 저장돼 있고 **사용**이 켜져 있을 때만 토스페이먼츠) |
| 이미 시작한 결제 | 시작한 곳. 결제 도중 전환해도 PortOne 에서 시작한 결제는 PortOne 에서 끝나요 |
| 환불, 결제 조회, 대사 | 결제한 곳 (결제마다 기록돼 있어요) |
| 새로 저장하는 카드 | 지금 켜져 있는 곳. 토스페이먼츠는 카드만, 자동결제 키가 있을 때만 |
| 이미 저장한 카드, 정기 방문 결제 | 카드를 저장한 곳 |
| 본인인증 (PASS) | 항상 PortOne |
| 현금영수증 | PortOne 결제는 지금처럼. 토스페이먼츠 결제는 결제창(계좌이체·가상계좌)에서 바로 발급되고, 결제를 취소하면 토스페이먼츠가 함께 취소해요 |
| 지급대행 (파트너 지급) | 바뀌지 않아요 (지급대행 카드) |

그래서 **PortOne 카드는 지우지 마세요.** PortOne 결제의 환불, PortOne 으로 저장한 카드, 본인인증이 모두 PortOne 카드를 써요. 토스페이먼츠 카드도 토스페이먼츠 결제나 카드가 남아 있는 동안은 지우지 마세요. 지우면 그 환불이 실패하고 알림이 와요.

토스페이먼츠에서는 카카오페이·네이버페이·토스페이·페이코를 저장 수단으로 쓸 수 없어요 (토스 자동결제는 카드만). 고객 앱에서는 이 수단들이 1회 결제로 보여요. 이미 PortOne 으로 저장한 간편결제는 그대로 써요.

### 테스트 키로 먼저 확인할 것

테스트 키에서는 돈이 움직이지 않아요. 가상계좌 번호는 `X` 로 시작해요. 카카오페이는 계약된 MID 키로만, 페이코는 라이브 키로만, 애플페이는 Safari 에서만 테스트돼요.

1. 결제수단마다 1회 결제: 카드, 간편결제(계약된 것), 계좌이체(현금영수증 단계 포함), 가상계좌(입금 시뮬레이션 → 결제 완료로 바뀌는지).
2. 결제 1건 전액 환불, 1건 부분 환불.
3. 결제창을 닫거나 실패했을 때 앱이 결제 취소로 돌아오는지.
4. 자동결제 키가 있으면: 카드 저장(테스트 인증번호 000000) → 그 카드로 예약 결제 → 환불 → 카드 삭제.
5. 개발자센터 웹훅 기록에서 우리 서버가 200 으로 답했는지.
6. Android, iOS 기기에서 카드사 앱, 토스, 카카오페이로 넘어갔다가 앱으로 돌아오는지.
7. 모두 통과하면 라이브 키로 바꿔 저장하고 (네 키 모두), [API 키 확인] 후 사용을 켜요.

실제 테스트 키로만 확인할 수 있는 항목: 웹훅에 서명 헤더가 오는지, 가상계좌 은행 코드와 예금주 표시, 자동결제 승인 응답 시간(서버는 45초까지 기다려요), 거래 내역 조회(`/v1/transactions`)의 날짜 형식과 페이지, 결제 승인 10분 제한. 개발자센터에서 API 버전을 정해 기록해 두세요.

---

# Payment setup guide (PortOne · Toss Payments) — English

This is a **fill-in template**. Enter the real values only in the console (Settings), never in this file, code, apps, web bundles or logs.

- PortOne store id (`store-…`) and channel keys (`channel-key-…`) may reach the browser SDK. Channel key for cash receipts is server-only.
- PortOne API Secret, PortOne webhook secret (`whsec_…`), Toss secret keys (`test_sk_` / `live_sk_`) and Toss security keys are **server-only**. The Toss Payments client keys (`test_ck_` / `live_ck_`) may reach the browser.
- Never mix `test_` and `live_` keys. Changing a key requires a new connection test.

Console order (Settings › Payments): (1) PortOne card: store id, V2 API secret, webhook secret and the channel key slots (card, card billing, KakaoPay, NaverPay, TossPay, virtual account, identity, cash receipt, bank transfer; also PAYCO, Samsung Pay, Apple Pay and phone), then Save (the first save needs the owner's code), then the owner presses Check API key. Installs that only had the card channel key keep billing and cash receipts on it. (2) Toss Payments card (optional, to take new payments with Toss Payments instead of PortOne): see "Taking payments with Toss Payments" below. (3) Toss payout card: secret key and security key, Save, Check connection, then mode `toss`.

Each card locks after it is saved. Secrets and channel keys show only a mask and cannot be read back; identifiers such as the store id and the Toss mode show in full. To replace a value, press Unlock (owner only, 6-digit authenticator code), paste the new value and Save. An unlock ends after 5 minutes (the card shows the minutes left), or sooner with Lock now. The owner can run a connection check on a locked card; no unlock is needed. Lock, unlock, save and failed unlock attempts are written to the audit log without the values.

Webhook URLs (set in each provider's dashboard):
- PortOne: `<PUBLIC_BASE_URL>/v1/webhooks/portone`
- Toss Payments (payments): `<PUBLIC_BASE_URL>/v1/webhooks/tosspayments`, registered on the payment-window MID with `PAYMENT_STATUS_CHANGED` and `DEPOSIT_CALLBACK`. The signature is checked when the security key is set and the header is present.
- Toss payouts: `<PUBLIC_BASE_URL>/v1/webhooks/toss` (`payout.changed`, `seller.changed`), signed with `tosspayments-webhook-signature`; an invalid signature gets 401.

Webhooks never move money by themselves: the server re-reads the payment from the provider before acting.

Items still to confirm in a test store: PortOne decline codes, cash receipt response shape, Toss webhook signature against one live test webhook, Toss payout create response wrapper.

Fill-in values to provide: store id, PortOne test API secret, one channel key per method used (card, card_billing, kakaopay, naverpay, tosspay, transfer, virtual_account, identity, cash_receipt), PortOne webhook secret, Toss test secret key, Toss security key, test account notes, public base URL.

## Taking payments with Toss Payments

PortOne stays as it is; only new payments move to Toss Payments (direct integration), and you can switch back at any time.

Contracts first: the Toss Payments online-payment contract (payment-window MID); each easy pay approved separately (KakaoPay and NaverPay review the merchant); an auto-billing (빌링) contract with its own MID only if you want saved cards or recurring visits; virtual-account and cash-receipt settings in the Toss merchant admin. Without auto-billing, Toss mode has no saved cards (cards pay in the window each time) and no new subscriptions; cards already saved with PortOne keep charging through PortOne.

Keys (developer center, API keys menu): the **API individual integration** client key (`test_ck_`) and secret key (`test_sk_`) of the payment-window MID, not the widget keys (`gck`/`gsk`); optionally the billing MID's pair (both or neither) and the 64-character security key. All keys must be test keys, or all live keys (`key_mode_mismatch` otherwise).

Console order: (1) enter the keys with **Active** off; (2) Save (owner code on the first save); (3) Check API key: the window keys, then the billing keys (`billing_invalid_key` when only the billing pair fails); (4) register `<PUBLIC_BASE_URL>/v1/webhooks/tosspayments` on the payment-window MID with `PAYMENT_STATUS_CHANGED` and `DEPOSIT_CALLBACK` (not on the billing MID: billing approvals send no webhook); (5) unlock (owner code), switch **Active** on, Save: the payments heading now says "New payments go to: Toss Payments"; (6) to switch back, unlock, switch it off, Save. Every switch is in the audit log.

Switching rules: new payments and newly saved cards go to the live gateway (Toss only when its card is saved and Active is on); a payment already started, its refunds, lookups and reconciliation stay on the gateway it started on; saved cards and their subscription charges stay on the gateway they were saved with; identity checks (PASS) always use PortOne; cash receipts for Toss payments come from the Toss window and are cancelled by Toss with the payment; payouts are unchanged. So **keep the PortOne card**, and keep the Toss card while Toss payments or cards exist: clearing a card makes its refunds fail and alert. Under Toss, KakaoPay, NaverPay, Toss Pay and PAYCO cannot be saved (Toss billing is card only) and become one-off payments.

Test with test keys first (no money moves; virtual accounts start with `X`; KakaoPay needs a contracted MID key, PAYCO live keys, Apple Pay Safari): one payment per method, a full and a partial refund, a closed or failed window, a saved card (test code 000000) charge, refund and delete when billing keys are set, the webhook log showing 200, and the hand-off to card apps, Toss and KakaoPay and back on Android and iOS. Then save the live keys (all four), check, and switch on. Only real test keys can show: whether webhooks carry the signature header, virtual-account bank codes and holder, billing charge latency (the server waits up to 45 s), the `/v1/transactions` date format and paging, and the 10-minute confirm limit. Set and record the API version in the developer center.
