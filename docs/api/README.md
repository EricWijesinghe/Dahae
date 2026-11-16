# Dahae API index

> Index of [`backend/crates/api/openapi.json`](../../backend/crates/api/openapi.json). The spec is maintained by hand
> and is the source of truth. Edit it and this page together. The Rust routes must match it:
> `cargo test -p api --test api_contract` fails when they do not. No generator is in this repository.

## 요약 (Korean summary)

이 문서는 `backend/crates/api/openapi.json`을 옮겨 적은 API 색인입니다. 모든 업무 경로는 `/v1` 아래에 있고,
헬스 체크와 앱 링크만 루트에 있습니다. 오류는 `{"error":{"code":"..."}}` 형식이고, 클라이언트는 `code`로 분기합니다.
목록은 커서 방식이며 `X-Next-Cursor` 헤더를 다음 요청의 `cursor`로 되돌려 보냅니다. 돈이 움직이는 POST는
`Idempotency-Key`를 받습니다. 표의 작업 수와 경로 수는 이 문서를 쓸 때 스펙에서 센 값입니다.

Spec: 312 operations on 267 paths, version `1.5.0` (`info.version`). The spec itself is served only to
staff admins at `GET /v1/openapi.json` (`Admin` extractor) ([`backend/crates/api/src/openapi.rs`](../../backend/crates/api/src/openapi.rs)).
Deprecated operations in this spec: 0.

## Conventions

| Topic | Rule | Source |
|---|---|---|
| Base URL | `https://<host>/v1`. Health (`GET /health`) and app links (`/.well-known/...`) live at the root. | [`routes/mod.rs`](../../backend/crates/api/src/routes/mod.rs), spec `info.description` |
| Versioning | The major version is in the path. Breaking changes need `/v2`. Additive changes (new routes, optional request fields, new response fields, new error codes) stay in `/v1`. Clients should ignore unknown response fields and treat unknown error codes as the generic failure for their HTTP status (policy; the Flutter client passes the code through, or uses `http_<status>` when none is given, [`api.dart`](../../app/packages/core/lib/src/api.dart)). Breaking changes should move to `/v2` (policy). | this page |
| Deprecation | A deprecated route answers with `Deprecation: true`, `Sunset: <HTTP date>` (RFC 9745, RFC 8594) and `Link: <successor>; rel="successor-version"`, via `contract::deprecated_route`. The spec sets `"deprecated": true` on the operation in the same change. No operation is deprecated today. | [`contract.rs`](../../backend/crates/api/src/contract.rs) |
| Auth | `Authorization: Bearer <access token>`. Access tokens are short-lived; rotate them with `POST /v1/auth/refresh`. Each role has its own extractor (`Customer`, `Pro`, `Member`, `Staff`, `Admin`). | [`auth.rs`](../../backend/crates/api/src/auth.rs) |
| Errors | Body `{"error":{"code":"<stable_code>"}}`. The code is the contract; `message` is optional in the spec, but the server does not emit it today. `request_id` is added when the request has one. A 409 `price_changed` also copies the current values into the error object. Framework failures (unknown route, bad method, rejected body, timeout, too large) use the same envelope. | [`error.rs`](../../backend/crates/api/src/error.rs), [`contract.rs`](../../backend/crates/api/src/contract.rs) |
| Error codes | Route-specific codes appear in each operation's description. Framework codes: `bad_request`, `not_found`, `method_not_allowed`, `timeout`, `payload_too_large` (413), `unsupported_media_type` (415), `invalid_body` (422), `rate_limited` (429), `internal` (5xx). Shared domain codes include `unauthorized` (401), `forbidden` (403), `stepup_required` (403), `price_changed` (409), `payment_failed` (402), `rate_limited` (429). | [`error.rs`](../../backend/crates/api/src/error.rs), [`contract.rs`](../../backend/crates/api/src/contract.rs) |
| Ownership failures | A caller who does not own a record gets 403 or 404 depending on the route; each route's tests state which. Do not assume one rule for all routes. | [`tests/authz.rs`](../../backend/crates/api/tests/authz.rs) |
| Lists | `limit` 1 to 100, default 50 (`400 invalid_limit` otherwise). Read `X-Next-Cursor` from the response; absent means the last page. A malformed cursor is `400 invalid_cursor`. Cursor-paged operations are marked in the tables. | [`contract.rs`](../../backend/crates/api/src/contract.rs) |
| Filters | Some enumerated filters, e.g. `GET /v1/checkouts?status=`, reject unknown values with 400 `invalid_status` instead of returning an empty list. | [`contract_wire.rs`](../../backend/crates/api/tests/contract_wire.rs) |
| Timestamps and dates | Timestamps are RFC 3339 in UTC. Business dates (visit dates, payout dates, "today") are Korean dates (KST, UTC+9) as `YYYY-MM-DD`. | spec `info.description` |
| Body limits | Auth bodies 16 KiB, client-error and analytics ingest 64 KiB, upload route 12 MiB, every other JSON body 1 MiB, over the limit `413 payload_too_large`. | [`contract.rs`](../../backend/crates/api/src/contract.rs), [`routes/mod.rs`](../../backend/crates/api/src/routes/mod.rs) |
| Rate limits | Per minute, configurable: auth 20, upload 30, general 300 (`RATE_LIMIT_AUTH_PER_MIN`, `RATE_LIMIT_UPLOAD_PER_MIN`, `RATE_LIMIT_GENERAL_PER_MIN`). Over the limit: `429 rate_limited`. | [`config.rs`](../../backend/crates/api/src/config.rs) |
| Unknown fields | Staff bodies in some routes use `deny_unknown_fields`, so a typo fails with `422 invalid_body`. Customer and partner bodies are not all strict; check the route. | e.g. [`routes/admin.rs`](../../backend/crates/api/src/routes/admin.rs) |

## Idempotency-Key

Operations that accept the header are marked "idempotent" in the tables. Send `Idempotency-Key` with one
value per user action (a UUID v4 is fine), and reuse that value on every retry of the same action.

- Format: 1 to 128 visible ASCII characters (no spaces or control characters). Otherwise `400 invalid_idempotency_key`.
- Scope: the caller plus the request path. The caller is `user:<id>` when the bearer token is valid, so a retry after
  the app refreshed its access token still matches. Without a valid token the scope is a hash of the header.
  Without any Authorization header the caller is the literal `anon`, so keys from unauthenticated requests to the
  same path share one namespace. See
  [`idempotency.rs`](../../backend/crates/api/src/idempotency.rs).
- Same key, same body: the stored answer is replayed with its original status and body and the header
  `Idempotent-Replayed: true`. The handler does not run again.
- Same key, different body: `400 idempotency_key_reused`.
- Same key while the first request is still running: `409 idempotency_in_progress`. Retry after a short wait.
- Only final answers are stored: 2xx and 4xx responses other than 401, 403 and 429. A 5xx, 401, 403 or 429 releases
  the key, so a retry runs the request again.
- Keys expire after 24 hours. A claim that never finished is treated as abandoned after 5 minutes.

## Paging example

```
GET /v1/bookings?limit=20
  -> 200 [...]   X-Next-Cursor: <opaque>
GET /v1/bookings?limit=20&cursor=<opaque>
```

## Tables

Columns: method, path, summary (from the spec; the two support helper rows differ from the spec wording), auth, and flags: **idem** (accepts `Idempotency-Key`),
**paged** (cursor list, returns `X-Next-Cursor`), **deprecated**. Auth labels: `none` (public), `bearer: <role>`
(role from the operation's tag), `signature` (provider callback), `ticket` (WebSocket).

The role shown is the tag's role. For `admin` operations the route file decides between the `Admin` extractor
(`StaffAdmin` only) and the `Staff` extractor (admin or consultant); check the route before relying on it.

### Public and discovery (26)

No login. Catalog, prices, home groups, slots, content, reviews, brand and feature flags, setup, app links, uploaded photos.

| Method | Path | Summary | Auth | Flags |
|---|---|---|---|---|
| GET | `/health` | Liveness and release version | none |  |
| GET | `/.well-known/assetlinks.json` | Android app links | none |  |
| GET | `/.well-known/apple-app-site-association` | iOS universal links | none |  |
| GET | `/v1/services` | Active services and their pricing | none |  |
| GET | `/v1/slots` | Open time slots for a date and district | none |  |
| POST | `/v1/quote/estimate` | Price estimate for a service and size | none |  |
| GET | `/v1/content` | Published content blocks | none |  |
| GET | `/v1/public/proof` | Public review and volume proof | none |  |
| GET | `/v1/public/seller-info` | Seller details as an HTML fragment for the website (?field= for one value) | none |  |
| GET | `/v1/payments/methods` | Payment methods enabled for checkout | none |  |
| GET | `/v1/setup` | Whether first-run setup is complete | none |  |
| POST | `/v1/setup/admin` | Create the first admin with the setup code | none |  |
| GET | `/v1/brand` | Brand name, colours, app-update policy and seller details (company: legal_name, ceo, registration_no, mail_order_no, address, hosting_provider; support_phone, support_email) | none |  |
| GET | `/v1/features` | Feature flags for the apps | none |  |
| GET | `/v1/files/{name}` | Serve an uploaded photo (signed, hourly-bucketed URL) | none |  |
| GET | `/v1/policy/cancellation` | Published cancellation policy | bearer |  |
| GET | `/v1/config` | Public app configuration | bearer |  |
| GET | `/ready` | Readiness: database reachable and migrations applied (not proxied publicly) | bearer |  |
| GET | `/metrics` | Prometheus metrics (token or loopback only; not proxied publicly) | bearer |  |
| GET | `/v1/feed` | Home feed for guests | bearer |  |
| GET | `/v1/policy/subscription` | Public subscription policy | bearer |  |
| POST | `/v1/client-errors` | Report an app error | bearer |  |
| POST | `/v1/events` | Send analytics events | bearer |  |
| GET | `/v1/public/offers` | Welcome offer, sign-up coupons and referral amount for guests | none |  |
| GET | `/v1/service-groups` | Home groups in order, with member services, chooser and derived badge (HOT over N); inactive groups and hidden members (studio) left out | none |  |
| GET | `/v1/public/reviews` | Public review list (rating, body, name, booking_id), cursor-paged | none | paged |

### Sign-in and tokens (15)

Sign-in by one-time code, social login and staff password. Rotating tokens.

| Method | Path | Summary | Auth | Flags |
|---|---|---|---|---|
| POST | `/v1/auth/otp` | Send a one-time code by SMS | none |  |
| POST | `/v1/auth/verify` | Exchange a one-time code for tokens | none |  |
| POST | `/v1/auth/refresh` | Rotate the refresh token | none |  |
| POST | `/v1/auth/logout` | Revoke the session | bearer |  |
| POST | `/v1/auth/staff/login` | Staff e-mail and password sign-in | none |  |
| POST | `/v1/auth/social` | Sign in with Kakao, Naver, Google or Apple (Apple may add authorization_code for revocation on deletion) | none |  |
| POST | `/v1/auth/logout-all` | Revoke every session of the signed-in account | bearer |  |
| POST | `/v1/auth/staff/mfa/enrol` | Start staff TOTP enrolment | bearer |  |
| POST | `/v1/auth/staff/mfa/enrol/confirm` | Confirm staff TOTP enrolment with a code | bearer |  |
| POST | `/v1/auth/staff/mfa/verify` | Second factor for a staff sign-in | bearer |  |
| POST | `/v1/auth/staff/mfa/step-up` | Fresh TOTP step-up for customer export and deletion, bank-detail reveals and payouts (valid for five minutes) | bearer |  |
| GET | `/v1/auth/sessions` | Signed-in devices of the current account | bearer |  |
| DELETE | `/v1/auth/sessions/{family}` | Sign one device out | bearer |  |
| POST | `/v1/auth/staff/code/redeem` | Redeem a staff one-time SMS code | bearer |  |
| POST | `/v1/auth/staff/owner-reset` | Lost second factor: owner signs in with password plus a server-issued reset code; TOTP and recovery codes are cleared | bearer |  |

### Customer app (67)

Bookings, quotes, checkout, chat, notifications, consents and account deletion.

| Method | Path | Summary | Auth | Flags |
|---|---|---|---|---|
| GET | `/v1/address/search` | Road-address search (per-user rate limit) | bearer: customer |  |
| POST | `/v1/uploads` | Upload one photo (multipart, field `file`, jpg/png/webp/heic) | bearer: customer |  |
| GET | `/v1/me` | Current customer profile | bearer: customer |  |
| PATCH | `/v1/me` | Update name and preferences | bearer: customer |  |
| DELETE | `/v1/me` | Delete the account (soft delete, personal data purged) | bearer: customer |  |
| POST | `/v1/me/phone` | Bind a verified phone number | bearer: customer |  |
| POST | `/v1/me/identity` | Identity verification | bearer: customer |  |
| GET | `/v1/me/addresses` | Saved addresses | bearer: customer |  |
| POST | `/v1/me/addresses` | Save an address | bearer: customer |  |
| PUT | `/v1/me/addresses/{id}` | Update a saved address | bearer: customer |  |
| DELETE | `/v1/me/addresses/{id}` | Delete a saved address | bearer: customer |  |
| GET | `/v1/me/coupons` | Coupons held by the customer | bearer: customer |  |
| GET | `/v1/me/payment-methods` | Registered payment methods | bearer: customer |  |
| POST | `/v1/me/payment-methods` | Register a billing key | bearer: customer |  |
| GET | `/v1/me/payment-methods/setup` | Provider parameters to start card registration | bearer: customer |  |
| DELETE | `/v1/me/payment-methods/{id}` | Remove a registered payment method | bearer: customer |  |
| POST | `/v1/me/push-token` | Register a push token for this device | bearer: customer |  |
| DELETE | `/v1/me/push-token` | Remove a push token | bearer: customer |  |
| GET | `/v1/me/notifications` | Notifications, newest first (cursor-paginated) | bearer: customer | paged |
| POST | `/v1/me/notifications/read` | Mark all notifications read | bearer: customer |  |
| GET | `/v1/me/consents` | Current consent choices | bearer: customer |  |
| PUT | `/v1/me/consents` | Update consent choices | bearer: customer |  |
| GET | `/v1/me/export` | Download the customer's personal data (JSON) | bearer: customer |  |
| GET | `/v1/me/referral` | Referral code and reward state | bearer: customer |  |
| POST | `/v1/me/referral/claim` | Enter a friend's referral code (once, before first booking) | bearer: customer |  |
| POST | `/v1/reviews/{booking_id}/report` | Report a review (once per user) | bearer: customer |  |
| GET | `/v1/bookings` | The customer's bookings, latest visit first (cursor-paginated) | bearer: customer | paged |
| POST | `/v1/bookings` | Create a booking (idempotent) | bearer: customer | idem |
| GET | `/v1/bookings/{id}` | One booking (404 when not the caller's) | bearer: customer |  |
| POST | `/v1/bookings/{id}/cancel` | Cancel and refund per policy (idempotent) | bearer: customer | idem |
| POST | `/v1/bookings/{id}/deposit` | Pay the deposit (idempotent) | bearer: customer | idem |
| POST | `/v1/bookings/{id}/balance` | Pay the balance (idempotent) | bearer: customer | idem |
| PUT | `/v1/bookings/{id}/notes` | Entry, parking and other notes (bounded fields) | bearer: customer |  |
| POST | `/v1/bookings/{id}/reschedule` | Move a booking to another slot | bearer: customer |  |
| POST | `/v1/bookings/{id}/review` | Review a completed booking (idempotent) | bearer: customer | idem |
| GET | `/v1/chat/threads` | Chat threads visible to the caller, most recent first (cursor-paginated) | bearer: customer | paged |
| POST | `/v1/chat/threads` | Open a thread on a topic (rate-limited per day) | bearer: customer |  |
| GET | `/v1/chat/threads/{id}/messages` | Messages, oldest first; pass `before` (created_at of the oldest shown) for older pages | bearer: customer |  |
| POST | `/v1/chat/threads/{id}/messages` | Send a message | bearer: customer |  |
| POST | `/v1/chat/threads/{id}/read` | Mark a thread read | bearer: customer |  |
| GET | `/v1/checkouts` | The customer's checkouts, newest first (cursor-paginated, optional status filter) | bearer: customer | paged |
| POST | `/v1/checkouts` | Start a payment checkout (idempotent). Optional `points` pays part with points: checked against balance and cap here, debited when the checkout settles; the response `amount` is what the PG charges (`total - points`) | bearer: customer | idem |
| POST | `/v1/checkouts/{id}/confirm` | Confirm payment after the provider redirect (idempotent) | bearer: customer | idem |
| GET | `/v1/bookings/{id}/cancel-preview` | Fee and refund if the booking were cancelled now | bearer: customer |  |
| PUT | `/v1/bookings/{id}/household` | Update the household profile for a booking | bearer: customer |  |
| POST | `/v1/bookings/{id}/cases` | Customer opens a case on a booking | bearer: customer |  |
| PUT | `/v1/checkouts/{id}/cash-receipt` | Request a cash receipt for a paid checkout | bearer: customer |  |
| GET | `/v1/receipt-options` | Receipt types available to the customer | bearer: customer |  |
| GET | `/v1/me/identity/setup` | Identity verification setup for the app | bearer: customer |  |
| POST | `/v1/me/identity/session` | Start an identity verification session | bearer: customer |  |
| PUT | `/v1/checkouts/{id}/tax-invoice` | Request a tax invoice for a paid checkout | bearer: customer |  |
| GET | `/v1/me/points` | Points balance, expiring points and recent ledger entries | bearer: customer |  |
| GET | `/v1/me/points/ledger` | Points ledger, newest first, one page at a time | bearer: customer | paged |
| GET | `/v1/me/feed` | Home feed for the signed-in customer | bearer: customer |  |
| GET | `/v1/subscriptions` | The customer's subscriptions | bearer: customer |  |
| POST | `/v1/subscriptions` | Start a subscription (idempotent) | bearer: customer | idem |
| GET | `/v1/subscriptions/{id}` | One subscription with its visits | bearer: customer |  |
| POST | `/v1/subscriptions/{id}/pause` | Pause a subscription | bearer: customer |  |
| POST | `/v1/subscriptions/{id}/resume` | Resume a subscription | bearer: customer |  |
| POST | `/v1/subscriptions/{id}/cancel` | Cancel a subscription | bearer: customer |  |
| POST | `/v1/subscriptions/{id}/visits/{visit}/skip` | Skip one visit of a subscription | bearer: customer |  |
| POST | `/v1/subscriptions/{id}/visits/{visit}/reschedule` | Move one visit of a subscription | bearer: customer |  |
| POST | `/v1/bookings/{id}/tip` | Tip the partner for a completed booking | bearer: customer |  |
| GET | `/v1/waitlist` | The customer's waitlist entries | bearer: customer |  |
| POST | `/v1/waitlist` | Join the waitlist for a full slot | bearer: customer |  |
| DELETE | `/v1/waitlist/{id}` | Leave the waitlist | bearer: customer |  |
| POST | `/v1/waitlist/{id}/accept` | Accept an offered waitlist slot | bearer: customer |  |

### Partner (pro) app (34)

Partner jobs, quotes, threads, settlements, reviews and team members.

| Method | Path | Summary | Auth | Flags |
|---|---|---|---|---|
| GET | `/v1/partner/quality` | Own quality score, status and tips | bearer: partner |  |
| GET | `/v1/partner/me` | Partner profile | bearer: partner |  |
| PATCH | `/v1/partner/me` | Update partner profile | bearer: partner |  |
| DELETE | `/v1/partner/me` | Delete partner account | bearer: partner |  |
| POST | `/v1/partner/me/phone` | Bind a verified phone number | bearer: partner |  |
| GET | `/v1/partner/prefs` | Work preferences | bearer: partner |  |
| PUT | `/v1/partner/prefs` | Update work preferences | bearer: partner |  |
| GET | `/v1/partner/team` | Team members | bearer: partner |  |
| POST | `/v1/partner/team` | Invite a team member | bearer: partner |  |
| DELETE | `/v1/partner/team/{id}` | Remove a team member | bearer: partner |  |
| GET | `/v1/partner/payout-account` | Payout account (masked) | bearer: partner |  |
| PUT | `/v1/partner/payout-account` | Set the payout account | bearer: partner |  |
| GET | `/v1/partner/settlements` | Settlement statements, newest first (cursor-paginated) | bearer: partner | paged |
| GET | `/v1/partner/reviews` | Reviews of the partner's work, newest first (cursor-paginated) | bearer: partner | paged |
| GET | `/v1/partner/threads` | Chat threads for the partner (cursor-paginated) | bearer: partner | paged |
| GET | `/v1/partner/export` | Partner data export (CSV) | bearer: partner |  |
| GET | `/v1/partner/earnings` | Earnings summary | bearer: partner |  |
| GET | `/v1/partner/jobs` | Open jobs matching the partner's districts (capped at 100) | bearer: partner |  |
| GET | `/v1/partner/jobs/{id}` | One job | bearer: partner |  |
| POST | `/v1/partner/jobs/{id}/accept` | Accept a job | bearer: partner |  |
| POST | `/v1/partner/jobs/{id}/quote` | Submit a quote | bearer: partner |  |
| PUT | `/v1/partner/jobs/{id}/quote` | Edit a submitted quote | bearer: partner |  |
| DELETE | `/v1/partner/jobs/{id}/quote` | Withdraw a quote | bearer: partner |  |
| POST | `/v1/partner/jobs/{id}/assign` | Assign a team member to a job | bearer: partner |  |
| PUT | `/v1/partner/jobs/{id}/memo` | Set the job memo (bounded text) | bearer: partner |  |
| POST | `/v1/partner/jobs/{id}/complete` | Mark a job complete | bearer: partner |  |
| POST | `/v1/partner/jobs/{id}/reschedule` | Accept or decline a reschedule request | bearer: partner |  |
| GET | `/v1/partner/quotes` | The partner's submitted quotes, latest activity first (cursor-paginated) | bearer: partner | paged |
| GET | `/v1/partner/schedule` | Upcoming assigned visits | bearer: partner |  |
| POST | `/v1/partner/jobs/{id}/cases` | Partner opens a case on a job | bearer: partner |  |
| POST | `/v1/partner/jobs/{id}/check-in` | Partner checks in at the visit address | bearer: partner |  |
| POST | `/v1/partner/jobs/{id}/check-out` | Partner checks out of the visit | bearer: partner |  |
| GET | `/v1/partner/jobs/{id}/entry-code` | Entry code for the visit (shown only during the job) | bearer: partner |  |
| GET | `/v1/partner/subscriptions` | Subscriptions that prefer this partner | bearer: partner |  |

### Member (cleaner) app (4)

A team member sees only jobs assigned to them.

| Method | Path | Summary | Auth | Flags |
|---|---|---|---|---|
| GET | `/v1/member/me` | Member (cleaner) profile | bearer: member |  |
| GET | `/v1/member/jobs` | Jobs assigned to the member | bearer: member |  |
| GET | `/v1/member/jobs/{id}` | One assigned job | bearer: member |  |
| POST | `/v1/member/jobs/{id}/complete` | Mark an assigned job complete | bearer: member |  |

### Staff console (159)

Staff routes under /v1/admin. Each route uses the Admin or Staff extractor (see note).

| Method | Path | Summary | Auth | Flags |
|---|---|---|---|---|
| GET | `/v1/openapi.json` | This specification (staff admins only) | bearer: staff |  |
| GET | `/v1/admin/dashboard` | Operations dashboard counters | bearer: staff |  |
| GET | `/v1/admin/bookings` | Bookings for the console, earliest visit first (cursor-paginated) | bearer: staff | paged |
| GET | `/v1/admin/bookings/{id}` | Booking detail | bearer: staff |  |
| POST | `/v1/admin/bookings/{id}/status` | Move a booking to another status | bearer: staff |  |
| POST | `/v1/admin/bookings/{id}/cancel` | Cancel with optional partial refund (idempotent) | bearer: staff | idem |
| GET | `/v1/admin/bookings/{id}/quotes` | Pro quotes for a booking | bearer: staff |  |
| POST | `/v1/admin/bookings/{id}/send-quote` | Send the final price to the customer | bearer: staff |  |
| POST | `/v1/admin/bookings/{id}/resend-quote` | Re-send the last quote card unchanged (409 when no quote was sent yet) | bearer: staff |  |
| GET | `/v1/admin/threads` | Staff inbox (capped at 200) | bearer: staff |  |
| GET | `/v1/admin/threads/{id}/messages` | Thread messages (same paging as customer) | bearer: staff |  |
| POST | `/v1/admin/threads/{id}/messages` | Send a staff message | bearer: staff |  |
| POST | `/v1/admin/threads/{id}/read` | Mark a thread read for staff | bearer: staff |  |
| PUT | `/v1/admin/threads/{id}/ai` | Pause or resume the support helper on a thread | bearer: staff |  |
| GET | `/v1/admin/ai/usage` | support helper usage | bearer: staff |  |
| GET | `/v1/admin/services` | All services including inactive | bearer: staff |  |
| POST | `/v1/admin/services` | Add a service (code, name, fixed price or quote) | bearer: staff |  |
| PUT | `/v1/admin/services/{code}` | Update a service's pricing and content | bearer: staff |  |
| GET | `/v1/admin/service-groups` | All home groups, including inactive groups and services | bearer: staff |  |
| PUT | `/v1/admin/service-groups/{code}` | Update a group's titles (ko/en), order, shown flag, icon, member services and chooser config (audited) | bearer: staff |  |
| GET | `/v1/admin/slot-capacity` | Capacity per start hour | bearer: staff |  |
| PUT | `/v1/admin/slot-capacity` | Set capacity per start hour | bearer: staff |  |
| GET | `/v1/admin/quote-formula` | Quote formula | bearer: staff |  |
| PUT | `/v1/admin/quote-formula` | Replace the quote formula | bearer: staff |  |
| POST | `/v1/admin/coupons` | Issue coupons to customers | bearer: staff |  |
| GET | `/v1/admin/content` | All content blocks | bearer: staff |  |
| POST | `/v1/admin/content` | Create a content block | bearer: staff |  |
| PUT | `/v1/admin/content/{id}` | Update a content block | bearer: staff |  |
| DELETE | `/v1/admin/content/{id}` | Delete a content block | bearer: staff |  |
| GET | `/v1/admin/notify-templates` | Notification text per event: built-in text and the owner's override, per language | bearer: staff |  |
| POST | `/v1/admin/notify-templates/preview` | Fill a notification text with sample values; nothing is saved | bearer: staff |  |
| PUT | `/v1/admin/notify-templates/{kind}/{locale}` | Save the owner's title and message for one event and language | bearer: staff |  |
| DELETE | `/v1/admin/notify-templates/{kind}/{locale}` | Restore the built-in text for one event and language | bearer: staff |  |
| GET | `/v1/admin/customers` | Customers, search by phone or name (capped at 200) | bearer: staff |  |
| GET | `/v1/admin/pros` | Partners (capped at 200) | bearer: staff |  |
| GET | `/v1/admin/team-members/pending` | Team members waiting for platform approval | bearer: staff |  |
| POST | `/v1/admin/team-members/{id}/approve` | Approve a team member's invite (audited) | bearer: staff |  |
| GET | `/v1/admin/pros/{id}/quality` | Quality score, status and tips of a pro | bearer: staff |  |
| POST | `/v1/admin/pros/{id}/status` | Verify (needs a complete checklist), suspend or set a partner fee | bearer: staff |  |
| GET | `/v1/admin/pros/{id}/profile` | Verification checklist, status and employee profile of a pro (emergency contact decrypted, audited) | bearer: staff |  |
| PUT | `/v1/admin/pros/{id}/profile` | Save the employee profile of a pro (emergency contact encrypted) | bearer: staff |  |
| POST | `/v1/admin/pros/{id}/payee-identifier/reveal` | Show a partner's decrypted payee identifier once (admin with recent TOTP step-up, audited, not cached) | bearer: staff |  |
| GET | `/v1/admin/settlements` | Settlement lines (capped at 200) | bearer: staff |  |
| GET | `/v1/admin/settlements/payouts` | Payouts, newest first (filter by state; cursor-paginated) | bearer: staff | paged |
| POST | `/v1/admin/settlements/run-payouts` | Build payouts for a date | bearer: staff |  |
| POST | `/v1/admin/settlements/toss-sellers/{pro_id}` | Register a partner as a Toss seller | bearer: staff |  |
| GET | `/v1/admin/settlements/payouts/{id}/account` | Reveal a payout account (audited) | bearer: staff |  |
| POST | `/v1/admin/settlements/payouts/{id}/{action}` | Payout step: send, confirm, fail, retry or sync (idempotent; send moves money) | bearer: staff | idem |
| POST | `/v1/admin/settlements/{id}/hold` | Hold or release a settlement | bearer: staff |  |
| GET | `/v1/admin/staff` | Staff accounts | bearer: staff |  |
| POST | `/v1/admin/staff` | Create a staff account | bearer: staff |  |
| POST | `/v1/admin/staff/{id}/active` | Activate or deactivate a staff account | bearer: staff |  |
| GET | `/v1/admin/finance/summary` | Revenue, fees and refunds summary | bearer: staff |  |
| GET | `/v1/admin/finance/series` | Daily series | bearer: staff |  |
| GET | `/v1/admin/finance/partners` | Per-partner totals | bearer: staff |  |
| GET | `/v1/admin/finance/services` | Per-service totals | bearer: staff |  |
| GET | `/v1/admin/finance/export/ledger` | Ledger export (CSV) | bearer: staff |  |
| GET | `/v1/admin/finance/export/settlements` | Settlements export (CSV) | bearer: staff |  |
| GET | `/v1/admin/finance/export/monthly` | Monthly export (CSV) | bearer: staff |  |
| GET | `/v1/admin/growth` | Welcome and referral offer settings | bearer: staff |  |
| PUT | `/v1/admin/growth/welcome` | Set the welcome offer | bearer: staff |  |
| PUT | `/v1/admin/growth/welcome-pack` | Set the sign-up welcome coupons (array of {amount, service_code?}; empty = off; at most 3 coupons and 50,000 KRW in total) | bearer: staff |  |
| PUT | `/v1/admin/growth/referral` | Set the referral offer | bearer: staff |  |
| GET | `/v1/admin/reviews` | Reviews, newest first (capped at 200) | bearer: staff |  |
| GET | `/v1/admin/reviews/reports` | Reviews awaiting a report decision | bearer: staff |  |
| PUT | `/v1/admin/reviews/{booking_id}` | Hide or show a review | bearer: staff |  |
| PUT | `/v1/admin/reviews/{booking_id}/decision` | Keep, hide or delete a reported review | bearer: staff |  |
| PUT | `/v1/admin/reviews/{booking_id}/author` | Block or unblock a review author | bearer: staff |  |
| GET | `/v1/admin/payment-methods` | Payment methods and their state | bearer: staff |  |
| PUT | `/v1/admin/payment-methods` | Enable payment methods | bearer: staff |  |
| GET | `/v1/admin/checkouts` | Checkouts, newest first (cursor-paginated, optional status filter) | bearer: staff | paged |
| GET | `/v1/admin/setup` | Setup checklist and state | bearer: staff |  |
| PUT | `/v1/admin/setup/business` | Business details | bearer: staff |  |
| PUT | `/v1/admin/setup/review` | Store the app-review test account | bearer: staff |  |
| DELETE | `/v1/admin/setup/review` | Remove the app-review test account | bearer: staff |  |
| PUT | `/v1/admin/setup/{section}` | Store a setup section | bearer: staff |  |
| DELETE | `/v1/admin/setup/{section}` | Clear a setup section | bearer: staff |  |
| POST | `/v1/admin/setup/{section}/test` | Test a provider connection for a section | bearer: staff |  |
| POST | `/v1/admin/setup/{section}/unlock` | Owner unlock of a saved card for 5 minutes (the card relocks after a save or when the window passes) | bearer: staff |  |
| POST | `/v1/admin/setup/{section}/lock` | Lock a card now (ends an owner unlock early) | bearer: staff |  |
| GET | `/v1/admin/audit` | Staff activity log, newest first (owner only, cursor-paginated; filters are exact unless noted) | bearer: staff | paged |
| GET | `/v1/admin/ops/status` | Service health, counters and version | bearer: staff |  |
| GET | `/v1/admin/ops/alerts` | Alert thresholds and recipients | bearer: staff |  |
| PUT | `/v1/admin/ops/alerts` | Update alert thresholds and recipients | bearer: staff |  |
| PUT | `/v1/admin/cancel-policy` | Owner sets the cancellation fee tiers | bearer: staff |  |
| GET | `/v1/admin/app-links` | Android and iOS app link values, with the files they produce | bearer: staff |  |
| PUT | `/v1/admin/app-links` | Owner saves the app link values (overrides the env values) | bearer: staff |  |
| POST | `/v1/admin/app-links/verify` | Checks app link values and shows the files without saving | bearer: staff |  |
| POST | `/v1/admin/settlements/partners/{pro_id}/tax-type` | Set a partner's tax type | bearer: staff |  |
| GET | `/v1/admin/cases` | List safety and service cases | bearer: staff |  |
| GET | `/v1/admin/cases/{id}` | Case detail | bearer: staff |  |
| PUT | `/v1/admin/cases/{id}` | Update case status and notes | bearer: staff |  |
| POST | `/v1/admin/cases/{id}/reassign` | Reassign a no-show booking to another partner | bearer: staff |  |
| GET | `/v1/admin/finance/cash-receipts` | List cash receipts | bearer: staff |  |
| POST | `/v1/admin/finance/cash-receipts/{id}/reissue` | Reissue a cash receipt | bearer: staff |  |
| GET | `/v1/admin/config` | Read app configuration (owner) | bearer: staff |  |
| PUT | `/v1/admin/config` | Update app configuration (owner) | bearer: staff |  |
| GET | `/v1/admin/finance/export/vat` | Export VAT summary (CSV) | bearer: staff |  |
| GET | `/v1/admin/finance/export/withholding` | Export withholding summary (CSV) | bearer: staff |  |
| GET | `/v1/admin/finance/export/payroll` | Export paid payouts per partner for payroll and withholding filing (CSV, admin only, audited) | bearer: staff |  |
| GET | `/v1/admin/bookings/{id}/entry-code` | Entry code for a booking (staff, audited) | bearer: staff |  |
| POST | `/v1/admin/staff/{id}/password-reset` | Issue a one-time password-reset link for a staff account | bearer: staff |  |
| GET | `/v1/admin/finance/tax-invoices` | List tax-invoice requests | bearer: staff |  |
| GET | `/v1/admin/finance/export/tax-invoices` | Export tax-invoice requests (CSV) | bearer: staff |  |
| POST | `/v1/admin/finance/export/tax-invoices/re-export` | Re-send tax-invoice rows already exported (CSV, confirm=true required) | bearer: staff |  |
| POST | `/v1/admin/finance/tax-invoices/{id}/issued` | Mark a tax invoice as issued | bearer: staff |  |
| PUT | `/v1/admin/growth/points` | Replace the points policy (audited) | bearer: staff |  |
| POST | `/v1/admin/growth/points/adjust` | Grant or deduct points for one customer (admin only, audited, reason required, idempotent) | bearer: staff | idem |
| GET | `/v1/admin/growth/points/{user_id}` | One customer's points snapshot | bearer: staff |  |
| GET | `/v1/admin/peak-pricing` | Peak-pricing rules | bearer: staff |  |
| PUT | `/v1/admin/peak-pricing` | Replace the peak-pricing rules | bearer: staff |  |
| GET | `/v1/admin/quality-thresholds` | Partner quality thresholds | bearer: staff |  |
| PUT | `/v1/admin/quality-thresholds` | Replace the partner quality thresholds | bearer: staff |  |
| GET | `/v1/admin/acquisition` | Customer acquisition table | bearer: staff |  |
| GET | `/v1/admin/campaigns` | Marketing campaigns | bearer: staff |  |
| POST | `/v1/admin/campaigns` | Create a marketing campaign | bearer: staff |  |
| POST | `/v1/admin/campaigns/preview` | Preview a campaign audience and message | bearer: staff |  |
| POST | `/v1/admin/campaigns/test` | Send a campaign test message | bearer: staff |  |
| GET | `/v1/admin/campaigns/{id}` | One campaign with its results | bearer: staff |  |
| POST | `/v1/admin/campaigns/{id}/schedule` | Schedule a campaign | bearer: staff |  |
| POST | `/v1/admin/campaigns/{id}/cancel` | Cancel a scheduled campaign | bearer: staff |  |
| GET | `/v1/admin/analytics/funnel` | Booking funnel from analytics events | bearer: staff |  |
| GET | `/v1/admin/analytics/cohorts` | Retention cohorts from analytics events | bearer: staff |  |
| GET | `/v1/admin/client-errors` | Errors reported by the apps | bearer: staff |  |
| GET | `/v1/admin/privacy/customers` | Live customers with a phone number (customer rights request lookup) | bearer: staff |  |
| GET | `/v1/admin/privacy/customers/{id}/export` | One customer's data as JSON, or as CSV with format=csv (logged as a privacy request) | bearer: staff |  |
| POST | `/v1/admin/privacy/customers/{id}/delete` | Delete a customer's account; admin only, the phone must be typed back (logged as a privacy request) | bearer: staff |  |
| GET | `/v1/admin/privacy/requests` | Log of customer rights requests, newest first (the last 200) | bearer: staff |  |
| GET | `/v1/admin/subscriptions` | Subscriptions (all customers) | bearer: staff |  |
| GET | `/v1/admin/subscriptions/policy` | Subscription policy | bearer: staff |  |
| PUT | `/v1/admin/subscriptions/policy` | Replace the subscription policy | bearer: staff |  |
| GET | `/v1/admin/subscriptions/metrics` | Subscription metrics | bearer: staff |  |
| GET | `/v1/admin/subscriptions/{id}` | One subscription | bearer: staff |  |
| PUT | `/v1/admin/subscriptions/{id}` | Edit a subscription | bearer: staff |  |
| POST | `/v1/admin/subscriptions/{id}/pause` | Pause a subscription for the customer | bearer: staff |  |
| POST | `/v1/admin/subscriptions/{id}/resume` | Resume a subscription for the customer | bearer: staff |  |
| POST | `/v1/admin/demo/load` | Load sample data into the business | bearer: staff |  |
| POST | `/v1/admin/demo/clear` | Remove sample data (owner authenticator code needed) | bearer: staff |  |
| GET | `/v1/admin/settings/{group}` | Read one settings group | bearer: staff |  |
| PUT | `/v1/admin/settings/{group}` | Save one settings group | bearer: staff |  |
| GET | `/v1/admin/keys/status` | Status of the keys bundle backup | bearer: staff |  |
| POST | `/v1/admin/staff/invite` | Invite a staff member by one-time SMS code | bearer: staff |  |
| POST | `/v1/admin/keys/bundle` | Create the sealed keys bundle (owner authenticator code needed) | bearer: staff |  |
| POST | `/v1/admin/keys/verify` | Check that a keys bundle opens with its passphrase | bearer: staff |  |
| GET | `/v1/admin/host/jobs` | Server jobs the owner queued, with live state read from the server status files (admin only) | bearer: staff |  |
| POST | `/v1/admin/host/jobs` | Queue one server job (backup, update, update-skip, channel, restart, schedule); needs owner TOTP and an Idempotency-Key header (admin only, logged) | bearer: staff |  |
| GET | `/v1/admin/host/jobs/{id}` | One queued server job and its live state (admin only) | bearer: staff |  |
| GET | `/v1/admin/renewals` | Renewal calendar: items with days left, and the keys check date (admin only) | bearer: staff |  |
| POST | `/v1/admin/renewals` | Add a renewal item; reminders go out by SMS 30 and 7 days before (admin only, logged) | bearer: staff |  |
| PUT | `/v1/admin/renewals/{id}` | Change a renewal item; a new due date starts its reminders again (admin only, logged) | bearer: staff |  |
| DELETE | `/v1/admin/renewals/{id}` | Remove a renewal item (admin only, logged) | bearer: staff |  |
| POST | `/v1/admin/owner/claim` | Claim the owner role while no owner exists (claimant's own TOTP) | bearer: staff |  |
| POST | `/v1/admin/owner/transfer` | Hand the owner role to another active admin (both TOTPs, one transaction) | bearer: staff |  |
| GET | `/v1/admin/me/account` | Owner-facing account status (masked phone, factor and recovery-code state) | bearer: staff |  |
| POST | `/v1/admin/me/recovery-codes` | Replace every recovery code; owner gates stay shut until the new codes are confirmed | bearer: staff |  |
| POST | `/v1/admin/me/recovery-codes/confirm` | Confirm the recovery codes were saved by typing one back | bearer: staff |  |
| PUT | `/v1/admin/me/alert-phone` | Set the SMS number for owner alerts; the replaced number is told when possible | bearer: staff |  |
| POST | `/v1/admin/me/password` | Change the owner's password; every session ends | bearer: staff |  |
| POST | `/v1/admin/me/sign-out-all` | End every session of the owner, this one included | bearer: staff |  |

### Provider callbacks (3)

Payment and Apple notifications. Authenticated by the provider's signature, not a bearer token.

| Method | Path | Summary | Auth | Flags |
|---|---|---|---|---|
| POST | `/v1/webhooks/portone` | PortOne payment notifications (signature verified) | signature |  |
| POST | `/v1/webhooks/toss` | Toss Payments notifications (signature verified) | signature |  |
| POST | `/v1/auth/apple/notifications` | Sign in with Apple server-to-server notifications (JWS verified against Apple's keys; replays acknowledged once) | signature |  |

### Realtime (2)

WebSocket events authenticated by a single-use ticket.

| Method | Path | Summary | Auth | Flags |
|---|---|---|---|---|
| GET | `/v1/ws` | WebSocket: booking, chat and notification events. Authenticate with a ticket from POST /v1/realtime/ticket | ticket |  |
| POST | `/v1/realtime/ticket` | Single-use WebSocket ticket, valid 30 seconds, bound to the caller | bearer: customer, partner, member or staff |  |

## Support helper endpoints

Two staff routes (`/v1/admin/threads/{id}/ai`, `/v1/admin/ai/usage`) belong to the customer support helper. The
helper answers chat threads when staff are away, or always when the owner chose that setting
([`backend/crates/api/src/ai.rs`](../../backend/crates/api/src/ai.rs), module header). Staff can pause it per thread.

- Contact details are removed from text before it goes to the external model API, whoever typed them
  ([`ai.rs`](../../backend/crates/api/src/ai.rs), comment near the redaction call). The API endpoint is set by
  `ANTHROPIC_BASE_URL` ([`config.rs`](../../backend/crates/api/src/config.rs)).
- By default messages go to a third-party model API (`ANTHROPIC_BASE_URL` default in
  [`config.rs`](../../backend/crates/api/src/config.rs)), which [`ai.rs`](../../backend/crates/api/src/ai.rs) describes as an
  overseas transfer. Consent coverage is not verified on this page.
- The customer sign-up consent list includes an overseas-transfer item, `overseas_transfer`
  ([`signup_consent.dart`](../../app/apps/customer/lib/screens/signup_consent.dart)). Whether that consent covers the
  helper is not verified on this page; check the consent text before relying on it.
- Chat retention for helper data: `AI_CHAT_RETENTION_DAYS`, default 180 ([`config.rs`](../../backend/crates/api/src/config.rs)).

## Contract checks

These are enforced by [`backend/crates/api/tests/api_contract.rs`](../../backend/crates/api/tests/api_contract.rs)
(run with `cargo test -p api --test api_contract`, also part of `cargo test --locked` in CI):

- every route mounted in `backend/crates/api/src/routes` (and the modules listed in that test) has an
  operation in the spec, and every operation has a route;
- every `Idempotency-Key` operation in the test's list documents the header, and every paged list documents
  `limit` and `X-Next-Cursor`;
- every HTTP call in `app/apps/*/lib` and `app/packages/*/lib` names a path and method the spec documents.

[`backend/crates/api/tests/contract_wire.rs`](../../backend/crates/api/tests/contract_wire.rs) checks the wire
behaviour (idempotent replay, key validation, the error envelope on unknown routes, `limit` range, status
filters) and that `GET /v1/openapi.json` is not public.
