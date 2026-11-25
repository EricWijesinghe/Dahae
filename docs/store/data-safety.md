# Google Play Data safety answers

English · [한국어](data-safety.ko.md)

Enter these answers in Play Console → app → **Policy** → **App content** → **Data safety**. The customer app and the partner app are entered **separately**.
The same facts in Apple's format are in `app-privacy.md`; the other Play declarations (ads, target age, financial features and so on) are in `play-declarations.md`. When you change one, change the others.

The evidence is the code itself: apps `app/packages/core/lib/src/`, server `backend/migrations/` and `backend/crates/api/src/`. The answers must also match the privacy policy, `website/privacy.html`.

Google's definitions (checked 2026-11, https://support.google.com/googleplay/android-developer/answer/10787469):
- **Collected** = the app sends the data off the device. Data used only on the device is not collected.
- **Shared** = handed to a third party. Data sent to a **service provider** that processes it on our behalf (payment gateway, SMS sender, push, AI processing), and transfers the user starts themselves, need not be declared as shared.
- Data collected under an anonymous ID must be declared too.

## Reference build

These answers match the **v1.6.1 release build**. It includes points, in-app error reports (no third-party SDK), and removal of social sign-in links plus Apple token revocation on account deletion.

## 1. General questions (both apps)

| Question | Answer |
|---|---|
| Does the app collect or share any of the required user data types? | Yes |
| Is all collected user data encrypted in transit? | Yes (HTTPS only) |
| Can users create an account? How? | Yes. Phone number verification (SMS) and social sign-in (Kakao, Naver, Google, and Apple on iOS) |
| Account deletion URL | `https://<domain>/delete-account.html` |
| Can users request that their data be deleted? | Yes. In the app: **More** → **Account** → **Delete account**, and the web address above |
| Can users request deletion of some data without deleting the account? (optional question) | No (addresses and saved cards can be removed in the app, but there is no separate request address) |

Records the law requires us to keep stay after deletion: bookings, payments and refunds for 5 years, consumer complaints and disputes for 3 years (Korean E-Commerce Act), and consent records. Chat messages are kept with the sender link removed. This is stated in section 3 of `privacy.html` and in `delete-account.html`; Google allows legally required retention as long as it is disclosed.

## 2. Customer app (Dahae): answers by data type

"Required/optional": optional if the user can use the app without providing it. The "processed ephemerally" question is **No** for every row (stored on the server).

| Google category | Data type | Collected | Shared | Required? | Purpose | Evidence |
|---|---|---|---|---|---|---|
| Personal info | Name | Yes | Yes (partly masked, to the assigned cleaning partner) | Optional | App functionality, account management | `users.name` |
| Personal info | Email address | Yes (social sign-in, tax invoice requests) | No | Optional | Account management, app functionality | `users.email`, `user_identities.email`, `tax_invoice_requests.email` |
| Personal info | User IDs | Yes | No | Required | App functionality, account management, fraud prevention, security and compliance | Internal ID `users.id`, social provider ID `user_identities`, hash of the identity-check CI `users.ci_hash` (the CI itself is not stored) |
| Personal info | Address | Yes | Yes (the assigned partner) | Required | App functionality | `addresses` (street and unit address, floor area, home details: pets, parking, elevator) |
| Personal info | Phone number | Yes | No | Required | App functionality, account management, developer communications, advertising or marketing (only advertising texts for customers who opted in to marketing) | `users.phone`, advertising send log `marketing_sends` |
| Financial info | User payment info | Yes | No | Optional | App functionality | Card numbers are not stored. The **billing key** issued by the payment gateway and the card display name `payment_methods`, cash-receipt number (encrypted) `cash_receipts.identity_enc` |
| Financial info | Purchase history | Yes | No | Required | App functionality, fraud prevention, security and compliance | `bookings`, `payments`, `checkouts`, regular cleaning `subscriptions`, tips `booking_tips`, coupons, points `points_ledger` |
| Messages | Other in-app messages | Yes | No | Optional | App functionality (customer support) | `chat_messages`. Support chats are with the operations team only. When AI support is on, the text goes to the service provider **Anthropic** (section 4) |
| Photos and videos | Photos | Yes (only what the user uploads) | Yes (the assigned partner; review photos are public to other users) | Optional | App functionality | `uploads`, `reviews.photos`, incident reports `cases.photos` |
| App activity | App interactions | Yes (**only when the marketing consent is on**) | No | Optional | Analytics | `analytics_events`: eight fixed event names, screen name, platform. No user ID, no free text. Deleted after 90 days (`analytics.rs`) |
| App activity | Other user-generated content | Yes | Yes (reviews are public to other users; visit notes go to the assigned partner) | Optional | App functionality | Reviews `reviews`, review reports `review_reports`, visit notes and entry instructions (the door code is encrypted and deleted 24 hours after the job ends), incident reports `cases` |
| App info and performance | Crash logs | Yes | No | Required | Analytics | Error reports the app sends itself (no third-party SDK, `error_reporter.dart` → `POST /v1/client-errors`): error message, stack and app version. No user ID; anything shaped like an email, phone or card number is masked by both the app and the server. Deleted after 14 days (`client_errors.rs`) |
| Device or other IDs | Device or other IDs | Yes | No | Optional | App functionality (notifications), analytics, advertising or marketing (advertising pushes only, with marketing consent) | Push token `push_tokens` (Firebase Cloud Messaging), analytics install ID (a random 32-character value made on the device, `analytics.dart`) |

### Not collected (all "No")
- **Location (approximate and precise)**: not collected. The customer app uses the location permission only when the user taps **Use my current location** on the address screen, and passes the coordinates only to the phone operating system's geocoder. **The coordinates are never sent to the Dahae server or stored.** Only the address the user confirms is stored (`app/apps/customer/lib/location.dart`).
- Card number, expiry date, CVC: entered on the payment gateway's screen (PortOne or Toss Payments); neither the app nor the server receives them.
- Health and fitness, race, religion, sexual orientation, contacts, calendar, audio, files, web browsing history, installed apps, in-app search history, SMS or email content.
- Advertising ID: not used. There is no ad SDK.

## 3. Partner app (Dahae Partner): answers by data type

The partner app sends no analytics events (no analytics wiring in `app/apps/partner/lib/main.dart`) and has no location permission.

| Google category | Data type | Collected | Shared | Required? | Purpose | Evidence |
|---|---|---|---|---|---|---|
| Personal info | Name | Yes | Yes (team name and display name, to the customer who booked) | Required | App functionality, account management | `pros.name`, `pros.team_name`, `team_members.name` |
| Personal info | Email address | Yes (social sign-in) | No | Optional | Account management | `user_identities.email` |
| Personal info | User IDs | Yes | No | Required | App functionality, account management | `pros.id`, `user_identities` |
| Personal info | Phone number | Yes | No | Required | App functionality, account management | `pros.phone`, `team_members.phone` |
| Financial info | Other financial info | Yes | No | Required (to receive payouts) | App functionality (payouts), fraud prevention, security and compliance | Payout account (stored encrypted, only the last 4 digits shown) `payout_accounts`, settlement and payout history `settlements`, `payouts`, tax type `pros.tax_type` |
| Messages | Other in-app messages | Yes | No | Optional | App functionality | Chat with the operations team `chat_messages` |
| Photos and videos | Photos | Yes | Yes (the customer of that booking) | Required (completion report) | App functionality | Before and after photos `completion_reports` |
| App activity | Other user-generated content | Yes | Yes (quotes and completion reports go to that customer) | Optional | App functionality | Quotes, completion report notes, training progress `training_progress` |
| App info and performance | Crash logs | Yes | No | Required | Analytics | Same as the customer app |
| Device or other IDs | Device or other IDs | Yes | No | Optional | App functionality (notifications) | Push token `push_tokens` |

Customer details a partner **sees** in the app (part of the name, the address, visit notes) are not the partner's own data, so they are not declared on the partner app form. They are declared in the "Shared" column of the customer app form.

## 4. Service providers (not "shared" on Google's form, but listed in the privacy policy)

| Provider | What | When |
|---|---|---|
| PortOne and its payment gateways (Toss Payments and others), or Toss Payments directly | Payment processing, billing keys, PASS identity check, cash receipts | On payment and identity check |
| Solapi | SMS and AlimTalk sending (phone number, message text) | Verification codes, booking alerts, advertising texts the customer agreed to |
| Google (Firebase Cloud Messaging) | Push sending (push token, notification title and body) | When a notification is sent. Transfer to the US |
| Anthropic | Generating AI support answers (the customer's support message, a summary of that booking, the business's prices and opening hours) | Only when the operator turns AI support on. Transfer to the US. Nothing is sent when it is off (`backend/crates/api/src/ai.rs`) |
| Storage provider (S3-compatible) | Photo files, encrypted database backups | At the location the operator configured |

Customers agree to the overseas transfer (Google, Anthropic) at sign-up and can review or withdraw it in the app under **More** → **Account** → **Consents** (`user_consents`, kind `overseas_transfer`). Withdrawing stops AI support and push notifications (Google FCM) together, and the app asks again when the privacy policy text changes (v1.5.1).

## 5. Judgement calls (the owner and the developer confirm before submitting)

1. **Is sending to a partner "sharing"?** Google says a transfer the user starts need not be declared as sharing, and a booking sending the address to a partner may count. Partners are separate businesses, not staff, so this document conservatively says **Shared: Yes**. Disclosing more is never a problem.
2. **Location and the OS geocoder**: turning coordinates into an address may use the phone operating system's geocoding service over the network (Google on Android, Apple on iPhone). Neither Dahae nor its service providers receive the coordinates, so the answer is "not collected". If a feature ever sends coordinates to the server (for example a partner arrival check), change location to **Yes**.
3. **Acquisition source (UTM)**: on a web sign-up, the `utm_*` and `ref` values in the link are stored once on the new account (`users.acquisition`, `attribution.dart`). The mobile apps have no path to receive them (`Uri.base` has a value only on the web), so they are not on the store forms. If an app deep link starts carrying them, add "App activity > Other actions, purpose: analytics, advertising or marketing".
4. **Analytics and the marketing consent**: analytics events are tied to the single "receive marketing" consent (`apps/customer/lib/state.dart`). Turning it off also drops queued events, which is why the answer is "optional".

## 6. Change these together
- When a new table or column holds personal data, update this table, `app-privacy.md`, `website/privacy.html` and iOS `PrivacyInfo.xcprivacy` together.
- If a third-party **SDK** for ads, analytics or crashes is added, the data it collects must be declared here too (Google counts what an SDK collects as the app's collection).
