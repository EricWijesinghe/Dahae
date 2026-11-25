# Store review notes (App Review / Play review)

English · [한국어](review-notes.ko.md)

These notes are the source text for the store reviewers. Real phone numbers and codes are **never committed**: fill the `<…>` placeholders at submission time and paste the text into Play Console "App access" and App Store Connect "App Review Information > Notes".

The text describes an install that uses the **Korea pack** (the only country pack available today): Korean phone sign-in, Korean payment gateways and Korean record-keeping law. Sections 1 and 7 are for the owner; sections 2 to 6 are what reviewers read.

---

## 1. Sign-in setup (for the owner)

Dahae signs users in with a phone number and a one-time code sent by SMS. For reviewers who have no local phone number, one review-only number has a fixed code.

- **Customer app**: the reviewer enters phone `<REVIEW_PHONE>` and code `<REVIEW_CODE>` on the sign-in screen. No SMS is sent to this number.
- **Partner app**: the fixed code works **in the customer app only** (the server deliberately refuses it for partner sign-in, `routes/auth.rs`). The partner app is reviewed with a review-only Google account `<REVIEW_GOOGLE_EMAIL>` / `<REVIEW_GOOGLE_PASSWORD>` through [Sign in with Google]. Before submitting, the operations team signs in to the partner app once with that Google account, confirms the number with a company phone, enters areas and services, and **approves** it in the console. The store-apps workflow turns on Google sign-in in the partner build: both platforms need `GOOGLE_SERVER_CLIENT_ID`, iOS also needs `PARTNER_GOOGLE_IOS_CLIENT_ID` for the partner bundle ID, and if `SOCIAL_PROVIDERS` is set it must include `google`. If one is missing, the build log warns (`scripts/ci/native-keys.sh`).
- The review account comes with two sample bookings (one upcoming, one completed), so the booking and history screens can be seen without paying.
- Social sign-in (Kakao, Naver, Google, Apple) is also offered. It needs a real account, so reviewers use the phone sign-in above.
- The review account is on **only during the review period** and switches off by itself after the date you set (at most 30 days).

## 2. Sign-in instructions (paste for reviewers)

Dahae signs users in with a phone number and a one-time code sent by SMS. Reviewers outside Korea cannot receive Korean SMS, so one review-only phone number has a fixed code. No SMS is sent for that number, and no other number can use the fixed code.

- **Customer app**: enter phone `<REVIEW_PHONE>` and code `<REVIEW_CODE>`.
- **Partner app**: tap "Sign in with Google" and use the review Google account `<REVIEW_GOOGLE_EMAIL>` / `<REVIEW_GOOGLE_PASSWORD>`. It is already an approved partner account, so the job, schedule and payout screens are open. (The fixed phone code above works in the customer app only.)
- The review customer account already has two sample bookings (one upcoming, one completed), so the booking and history screens can be reviewed without paying.
- Social login (Kakao, Naver, Google, and Sign in with Apple on iOS) is also offered. It needs a real account, so please use the phone login above.
- The review account is switched off automatically after the period we set (at most 30 days).

## 3. Account deletion

- In the app: More → Account → Delete account (customer and partner apps). Deletion completes inside the app, with no hand-off to a web page or a phone call.
- Web fallback (when the app cannot be used): `https://<domain>/delete-account.html`
- When a customer deletes their account, the saved-card billing keys are deleted at the payment gateway first, so no further charge for regular cleaning can occur.
- Partners with an assigned job are refused by the server until the job is closed.
- Booking and payment records that the law requires us to keep (five years under Korean e-commerce law) remain; the privacy policy and the web page above say so.

> The build includes Sign in with Apple token revocation (REST API `auth/revoke`), the Apple server-to-server notification endpoint (`POST /v1/auth/apple/notifications`, required for the Services ID registration of a Korean developer account) and removal of social links on customer deletion. Add this sentence to the review notes:
> "Deleting an account that used Sign in with Apple also revokes the Apple tokens through the Sign in with Apple REST API, and we process Apple server-to-server notifications (consent-revoked, account-delete)."

## 4. Payments

Everything Dahae sells is **cleaning done in person at the customer's home**. There are no digital goods or feature unlocks in the app.

| Money in the app | What it is | Evidence |
|---|---|---|
| One-off cleaning booking | The fee for a home visit | `apps/customer/lib/screens/booking_flow.dart` |
| **Regular cleaning (subscription)** | A schedule of visits on the same weekday every 1, 2 or 4 weeks. Each visit is charged to the saved card **the day before** that visit. The customer can pause, resume, cancel, skip or move a visit in the app. No app feature is unlocked by it | `subscribe.dart`, `subscriptions.dart`, `backend/migrations/0066_subscriptions.sql` |
| Tip (optional) | A tip the customer may give after the visit. All of it goes to the cleaner | `backend/migrations/0076_tips.sql` |
| Coupons and points | Discounts on the cleaning fee. **Never sold.** They come only from bookings, sign-up and referrals, and cannot be cashed out | `coupons`, `points_ledger` |

- Payment happens on the payment gateway's own screen: PortOne (with gateways such as Toss Payments) or Toss Payments directly, whichever the console has turned on. Card numbers never reach the app or the server.
- **Apple**: Guideline 3.1.3(e) says physical goods and services consumed outside the app must use payment methods **other than** in-app purchase (cards and the like). So the app does not use in-app purchase (IAP). 3.1.1 (the in-app purchase rule) covers unlocking features or content inside the app, and Dahae sells nothing of that kind. The "auto-renewable subscriptions" of 3.1.2 are in-app purchase products, so regular cleaning is not one of them.
- **Google**: the Play payments policy also keeps Play billing out of payments for physical goods and services.

Text to paste into the review notes:

```
PAYMENTS (Guideline 3.1.3(e))
Everything sold in this app is in-person home cleaning performed at the customer's
home, i.e. a physical service consumed outside the app. Under Guideline 3.1.3(e) such
purchases must use a payment method other than in-app purchase, so payments go through
a Korean payment gateway (PortOne or Toss Payments: card and local wallets) on the
gateway's hosted page. The app unlocks no digital content, features or functionality for money, so
Guideline 3.1.1 does not apply.

"Regular cleaning" (shown as a subscription) is a recurring schedule of in-person
visits: weekly, every 2 weeks or every 4 weeks. Each visit is charged individually to
the saved card the day before that visit. It is not an auto-renewable digital
subscription (3.1.2). The customer can pause, resume, cancel, skip or move a visit in
the app: More > Regular cleaning.

Optional tips after a completed visit go in full to the cleaner who performed the
physical service. Coupons and loyalty points only reduce the price of a cleaning visit;
they are never sold and cannot be cashed out.
```

## 5. Permissions

| Permission | App | Why | When it is asked |
|---|---|---|---|
| Photo picker | Customer, partner | Before and after photos, photos in chat and reviews | When the photo button is pressed. The system photo picker (Android photo picker, iOS PHPicker) passes only the photos the user picks; the app never asks for access to the whole library |
| Camera | None | This version never opens the camera itself (photos are picked from the library) | Never |
| Notifications | Customer, partner | Booking and job alerts (push) | After sign-in (Android 13+ asks for POST_NOTIFICATIONS at run time) |
| Location (while in use) | Customer only | [Use my current location] on the address screen | Only when that button is pressed |

- Location: the coordinates are used only by the phone's own address lookup and are never sent to the Dahae server. No background location. Refusing the permission still leaves every feature usable by typing the address. The partner app never asks for location.
- The iOS `Info.plist` contains the camera and photo-library usage texts (`NSCameraUsageDescription`, `NSPhotoLibraryUsageDescription`) for a future take-a-photo button; this version shows no camera permission prompt. Give this answer if reviewers ask.
- Refusing notifications leaves the app fully usable (notices also go by SMS).
- No Android foreground service is used.

## 6. Other answers

- Advertising: none (no ad SDK, no advertising ID).
- Tracking: none (no ATT prompt).
- Usage statistics: the customer app sends eight fixed events, with a random install ID, to the business's own server, and only for users who agreed to receive marketing. No outside analytics SDK.
- Target age: adults (not directed at children). No feature targets children.
- Encryption: the app uses only the operating system's standard encryption such as HTTPS (iOS `ITSAppUsesNonExemptEncryption = false`).
- Financial features: only payment for cleaning. No loans, transfers, investments or other financial products. Partner payouts are handled by the operations team after the partner enters a payout account.
- Privacy policy: `https://<domain>/privacy.html`
- Sign-in: because social sign-in (Kakao, Naver, Google) is offered, Sign in with Apple is offered on iOS too (Guideline 4.8).
- AI replies: replies in customer chat may be generated by AI (the operator can turn this off). AI replies are generated by Anthropic, PBC (USA), which the sign-up consent for cross-border transfer names (Guideline 5.1.2(i)). When a staff member replies, the AI leaves that conversation.
- Chat: customers and partners each talk **only with the operations team**. Users never message each other directly.
- User-generated content: reviews (with photos) are shown to other users. Every review has an in-app **Report** button (four reasons, once per user). At three reports (a server setting) a review is hidden automatically and queued for the operations team, which can keep, hide or delete it and restrict the author's reviews. The review screen shows the content rules, and article 9 of the terms of service says the same.
- Age-rating questionnaire answers: Apple in `app-privacy.md` section 2, Google and Microsoft (IARC) in `play-declarations.md` section 5.

## 7. Before submitting (owner checklist, not for reviewers)

- [ ] Console Settings > Review account: enter the number and the code, set the period (saving creates the two sample bookings)
- [ ] Create the Google account for the partner app review → sign in to the partner app and confirm the number (company phone) → approve it in the console (section 1). Do not register a partner on the review number (that switches the fixed code off)
- [ ] Fill the `<…>` placeholders above (Play "App access", App Store "Review notes")
- [ ] Submit before the review period ends; after approval, switch the review account off or confirm it expired
- [ ] Console > Marketing > Growth > Review reports: handle anything waiting (handle reports at once during review)
- [ ] Sign in with Apple: enter the key (.p8) in the console's Apple card, and register the server notification URL `https://<API_DOMAIN>/v1/auth/apple/notifications` in the Apple developer account
- [ ] App IDs: check that the GitHub `store` variables `CUSTOMER_APP_ID` and `PARTNER_APP_ID` match the store apps and the console's **Settings → App links** (left empty they are `kr.co.dahae.customer` and `kr.co.dahae.partner`). They cannot change after the first upload
- [ ] Name change checklist (if the app ships under a new brand name): trademark search, Kakao channel and AlimTalk, payment-gateway merchant name, notice of the terms change 7 days ahead → `docs/REBRAND-CHECKLIST.md`
