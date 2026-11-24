# Privacy retention and deletion (PIPA)

Source of truth for what the server keeps, purges and deletes. Code: `backend/crates/api/src/privacy.rs`, `backend/crates/api/src/retention.rs`.

## Consent (`user_consents`, migration 0016)

- Append-only. Each change is a row `(user, kind, version, granted, created_at)`. The newest row per kind is the current state.
- Kinds: `terms`, `privacy`, `overseas_transfer` (required at sign-up); `marketing` (optional). `overseas_transfer` and `marketing` are withdrawable later; withdrawing `overseas_transfer` stops push notifications (Google FCM) and the AI chat (Anthropic) for that customer.
- A consent counts only when granted for `CONSENT_VERSION` (privacy.rs). Bumping the version forces re-consent of required kinds (`reconsent_required: true`) and stops marketing until re-asked.
- API: `GET/PUT /v1/me/consents`. A change with a stale version is rejected. `terms` and `privacy` cannot be withdrawn while the customer uses the service; the other kinds can.
- Marketing sends: `realtime::notify` drops any `marketing.*` kind unless `marketing_allowed`. Transactional kinds are unaffected.

## Deletion (`DELETE /v1/me`)

| Data | Action |
|---|---|
| Name, email, phone, CI hash, identity date, attribution, referral code | Cleared; phone set to `deleted:<id>` |
| Social sign-ins (`user_identities`: provider ids, emails) and stored refresh tokens | Deleted; after commit Apple tokens are revoked, Kakao unlinked (Admin key), Naver tokens deleted. Best effort, logged |
| Addresses | Hard delete; one still referenced by a subscription is emptied |
| Saved card (PortOne billing key) | Deleted at PortOne first. If PortOne refuses, nothing changes and the call returns 503 `billing_key_delete_failed`; retry is safe. Card label cleared |
| Subscriptions, planned visits, waitlist places | Cancelled; waitlist request snapshot cleared |
| Unpaid checkouts | Booking snapshot and virtual account cleared (failed/cancelled/refunded now; others by the purge after 30 days) |
| Login codes, PASS sessions, billing tickets | Deleted |
| Push tokens, notifications | Deleted |
| Refresh tokens | Revoked |
| Uploaded photos | File and row deleted |
| Reviews | Text and photos cleared; rating kept for the partner score |
| Chat messages sent by the customer | `sender_id` unlinked; text kept for disputes |
| Referral record | Phone replaced by `hmac:<hex>` (HMAC-SHA256, pepper derived from DATA_KEY) so one reward per phone still holds |
| Finished bookings (completed or cancelled) | Street address, answers, visit notes, household and the partner's team memo cleared; the district code, size, dwelling type, amounts, dates, service and status kept |
| Open bookings (upcoming or in progress) | Kept whole until the booking is completed or cancelled; the retention purge then clears the personal fields as above |
| Bookings, payments, paid checkouts, tips, cash receipts, tax invoice requests, payouts | Kept 5 years (전자상거래법, 부가가치세법), linked to the anonymised row |
| Consent log | Kept as proof of consent (internal 5-year rule, see lawyer list) |

Apple `account-delete` notifications (`POST /v1/auth/apple/notifications`) run the same cascade. A partner blocked by an open job or payout is marked `deletion_requested_at` and deleted by the hourly retention run once clear.

## Export (`GET /v1/me/export`)

JSON: profile, consent history, addresses, payment method labels (no keys), coupons, bookings, last 500 notifications, subscriptions, waitlist places (with their request notes), the customer's reviews (text and photo references), chat messages the customer sent, upload records (size and date, not the files), points ledger, tips, cash receipts (last 4 digits only), tax invoice requests, and referrals (role and state, no phone numbers). Gateway keys, tokens and the encrypted cash receipt identity are never included.

## Staff-handled requests (console)

`backend/crates/api/src/routes/privacy_requests.rs`, admin routes under `/v1/admin/privacy/`:
- `GET customers?phone=`: live customers with that phone (Staff). Audit `privacy.lookup`.
- `GET customers/{id}/export?format=json|csv`: the same export document as `/v1/me/export`, for one customer (Staff). CSV flattens it to `section,detail` rows. Audit `customer.export`.
- `POST customers/{id}/delete` with `{"phone": ...}`: the phone must match the stored one; runs `delete_account` (Admin only). Audit `customer.delete`.
- `GET requests`: the request log (Staff). Each export and deletion writes a `privacy_requests` row (kind, customer id, staff id, `done` or `failed`). It holds no personal details.

## Retention purge (`retention::spawn`, hourly)

| Target | Rule | Setting |
|---|---|---|
| `otp_codes` | older than 1 day | fixed |
| `refresh_tokens` | expired, or revoked over 1 day ago | fixed |
| Ops failure and HTTP error counters | older than 3 days | fixed (`ops::prune`) |
| AI-assisted chat messages (threads with an `ai_usage` row) | older than N days | `AI_CHAT_RETENTION_DAYS`, default 180 |
| Uploads not referenced by bookings, chat, pros, users, cases, completion reports, reviews, pro profiles, content or settings | older than N days; file removed, then row | `UPLOAD_ORPHAN_DAYS`, default 30 |
| Deleted customers' unpaid checkout snapshots | older than 30 days | fixed |
| Bookings of deleted customers that are completed or cancelled and still hold an address, answers, visit notes, household or team memo | cleared (see Deletion) | fixed (`privacy::pseudonymise_closed_bookings`) |
| Pending partner deletions (Apple `account-delete`) | retried until no open job or payout | fixed |
| `apple_notifications` (seen JWS ids, no user data) | never purged (replay guard) | fixed |

Each run logs counts (`retention purge done`). A database error stops the run; the next hour retries it.

## Staff access log

Every staff read of a customer's contact details writes `audit_log` with `action` `customer.pii_view` (booking detail) or `customer.pii_list` (customer search). Payout account reveals already log `payout.reveal`.

## Audit and consent records

- `audit_log` and `user_consents` are append-only: the database refuses UPDATE, DELETE and TRUNCATE (migration `0320_audit_append_only.sql`). A withdrawal or correction is a new row.
- Retention: nothing in the app purges these rows today. Access-log rows (`customer.pii_*`) keep at least 1 year under the personal information safety measures standard. The period for money-action rows is pending accountant review; do not treat any figure as final until it is confirmed.
- Money actions write their audit row in the same transaction as the local money record: refunds (`payment.refund`), payout reveals (`payout.reveal`) and finance exports (`finance.export`). A failed audit write rolls back the local record and releases no payout account or file.
- A refund is sent to the payment gateway before its audit row is written. If the audit write then fails, the gateway may already have refunded the money while our books still show the payment as paid. The next cancel is refused by the gateway balance check, so it cannot refund twice; reconcile the payment with the gateway.
- Demo clear (`DELETE` of demo rows) is the one exception: it switches the guards off inside its own transaction and turns them back on before commit.

## Container logs

- Every service writes Docker `json-file` logs, rotated at 10 MB per file and 14 files per service (`x-logging` in `deploy/docker-compose.yml`); the oldest file is deleted when the 15th would start. There is no time limit: a quiet service keeps older lines longer.
- Postgres logs slow statements without bind values (`log_parameter_max_length=0`) and errors without DETAIL or CONTEXT lines (`log_error_verbosity=terse`), so customer phones, names and addresses from queries do not reach these logs. The statement text itself is kept; it holds placeholders, not values. An error message can still quote one bad input value (for example an invalid id).
- The console log view masks phone numbers. Check any `dahae logs` output before pasting it into a ticket.

## Not yet covered

- Partner (pro) payee number: partner deletion (`partner::delete_pro`) clears the name, phone, payout account, sign-ins, sessions, the console profile (photo, bio, emergency contact) and the verification checklist in one transaction, then removes the partner's uploads and profile photo that no completion report, case or chat message names. The payee number (주민등록번호 or 사업자등록번호) is kept for the 지급명세서 of payouts already made and can no longer be revealed in the console. It has no purge date yet: the period (deletion date plus the tax retention period) is pending accountant review.
- Access (connection) logs: nothing in the database stores IP-level access logs; the 3-month rule depends on the hosting proxy's logs.
- Customers with an open booking can still delete the account. The open booking keeps its personal fields until it is completed or cancelled, then the purge clears them. A booking the partner never closes keeps them until someone closes it.
