# App Store answers: privacy labels, age rating, export compliance

English · [한국어](app-privacy.ko.md)

The answers to enter in App Store Connect, in one place. The customer app (Dahae) and the partner app (Dahae Partner) are entered **separately**.
The Google Play counterpart is `data-safety.md`. When you change one, change the other. Both documents match the v1.6.0 release build.

Evidence: server `backend/migrations/`, `backend/crates/api/src/`, apps `app/packages/core/lib/src/`, iOS `app/apps/*/ios/Runner/PrivacyInfo.xcprivacy`, `Info.plist`.

## 1. Privacy labels (App Privacy)

Where: App Store Connect → the app → [App Privacy].

Apple's definitions (checked 2026-11, https://developer.apple.com/app-store/app-privacy-details/):
- **Collect** = sending data off the device so that we or our partners can see it for longer than it takes to serve the request.
- **Tracking** = linking our app's data with other companies' data for targeted advertising or ad measurement, or passing it to data brokers. Dahae does neither.
- Payment details entered on a payment service's screen, which the developer cannot see, are not collected. Dahae never receives card numbers but stores recurring-billing keys, so it declares "Payment Info".

### Both apps
| Question | Answer |
|---|---|
| Does this app collect data? | Yes |
| Is it used for tracking? | **No** (no IDFA, no ATT prompt, no ad SDK, `NSPrivacyTracking = false`) |
| Privacy policy URL | `https://<domain>/privacy.html` |
| Account deletion | In the app: [More] → [Account] → [Delete account]. Web page: `https://<domain>/delete-account.html` |

### Customer app (Dahae)
| Apple category | Data | Evidence (table/column) | Linked to the user | Tracking | Purpose |
|---|---|---|---|---|---|
| Contact Info · Name | Name | `users.name` | Yes | No | App Functionality |
| Contact Info · Email Address | Email (social sign-in, tax invoice requests) | `users.email`, `user_identities.email`, `tax_invoice_requests.email` | Yes | No | App Functionality |
| Contact Info · Phone Number | Mobile number | `users.phone` | Yes | No | App Functionality, **Developer's Advertising or Marketing** (marketing SMS after marketing consent, `marketing.rs`) |
| Contact Info · Physical Address | Street and detailed address, floor area, home details | `addresses` | Yes | No | App Functionality |
| Financial Info · Payment Info | Recurring-billing key, card display name, cash-receipt number (encrypted) | `payment_methods`, `cash_receipts` | Yes | No | App Functionality |
| Purchases · Purchase History | Bookings, payments, refunds, regular cleaning, tips, coupons, points | `bookings`, `payments`, `checkouts`, `subscriptions`, `booking_tips`, `points_ledger` | Yes | No | App Functionality |
| Identifiers · User ID | Internal ID, social provider ID, identity-check CI hash | `users.id`, `user_identities.provider_user_id`, `users.ci_hash` | Yes | No | App Functionality |
| Identifiers · Device ID | Push token | `push_tokens` | Yes | No | App Functionality, **Developer's Advertising or Marketing** (marketing push after marketing consent) |
| Identifiers · Device ID | Analytics install ID (random) | `analytics_events.anon_id` | **No** | No | **Analytics** |
| Usage Data · Product Interaction | Eight fixed events (app open, sign-up, quote view, booking, payment, review) and screen names | `analytics_events` | **No** | No | **Analytics** |
| User Content · Photos or Videos | Photos in chat, reviews and incident reports | `uploads`, `reviews.photos`, `cases.photos` | Yes | No | App Functionality |
| User Content · Customer Support | Chat with the operations team and AI replies | `chat_messages` | Yes | No | App Functionality |
| User Content · Other User Content | Reviews, visit notes and entry instructions, incident reports | `reviews`, `bookings.visit_notes`, `bookings.access_code_enc`, `cases` | Yes | No | App Functionality |
| Diagnostics · Crash Data | Error reports the app sends itself (no outside SDK): message, stack, app version, deleted after 14 days | `client_errors` | **No** | No | App Functionality |

Device ID is one row in App Store Connect. Set "Linked" to **Yes** (the push token is linked to the account) and choose all three purposes: **App Functionality, Analytics, Developer's Advertising or Marketing**.

Analytics events are sent only when the user agreed to receive marketing information. They are collected with a random value per install, never with the account ID, and deleted after 90 days (`analytics.dart`, `analytics.rs`). Apple does not exempt optional collection, so it must still be declared.

Left as **Not collected**: Health and Fitness, Sensitive Info, Contacts, Search History, Browsing History, Audio, Advertising Data, **Location**.

**Location**: the customer app asks for "while using the app" location only when [Use my current location] is pressed on the address screen (`NSLocationWhenInUseUsageDescription`, Korean and English). The coordinates go only to the iPhone's own address lookup and are never sent to the Dahae server. That is not "collection" under Apple's definition (sent off the device for us or our partners to see), so **Location: not collected**. Only the address the user confirms (the "Physical Address" row) is stored.

**AI replies (Anthropic)**: when the operator turns AI replies on, the customer's chat text and a summary of the booking are sent to the Anthropic API (Anthropic, PBC, USA) to produce an AI reply (`backend/crates/api/src/ai.rs`). In the labels this is part of the "Customer Support" row. Guideline 5.1.2(i) asks apps to disclose and get consent before sending personal data to a third-party service of this kind. Dahae asks for cross-border transfer consent at sign-up (recipients: Anthropic and Google) and states it in sections 5 and 6 of `privacy.html`. When a customer withdraws the cross-border consent under [Manage consent], AI replies and push notifications stop together (v1.5.1).

### Partner app (Dahae Partner)
No analytics events and no location permission.

| Apple category | Data | Evidence | Linked to the user | Tracking | Purpose |
|---|---|---|---|---|---|
| Contact Info · Name | Partner and team-member names, team name | `pros`, `team_members` | Yes | No | App Functionality |
| Contact Info · Email Address | Email (social sign-in) | `user_identities.email` | Yes | No | App Functionality |
| Contact Info · Phone Number | Partner and team-member mobile numbers | `pros.phone`, `team_members.phone` | Yes | No | App Functionality |
| Financial Info · Other Financial Info | Payout account (encrypted, only the last 4 digits shown), tax type | `payout_accounts`, `pros.tax_type` | Yes | No | App Functionality |
| Purchases · Purchase History | Settlement and payout history | `settlements`, `payouts` | Yes | No | App Functionality |
| Identifiers · User ID | Internal ID, social provider ID | `pros.id`, `user_identities` | Yes | No | App Functionality |
| Identifiers · Device ID | Push token | `push_tokens` | Yes | No | App Functionality |
| User Content · Photos or Videos | Before and after photos | `completion_reports`, `uploads` | Yes | No | App Functionality |
| User Content · Customer Support | Chat with the operations team | `chat_messages` | Yes | No | App Functionality |
| User Content · Other User Content | Quotes, completion-report notes | `quotes`, `completion_reports` | Yes | No | App Functionality |
| Diagnostics · Crash Data | Error reports the app sends itself (no outside SDK): message, stack, app version, deleted after 14 days | `client_errors` | **No** | No | App Functionality |

### Matching the in-app privacy manifest (developers)
Apple expects `PrivacyInfo.xcprivacy` to match the labels above. The customer and partner manifests (`app/apps/*/ios/Runner/PrivacyInfo.xcprivacy`) match the tables. Crash data is declared as `NSPrivacyCollectedDataTypeCrashData` (linked false, tracking false, purpose AppFunctionality), and `scripts/ci/check-native.py` checks it stays there.

The manifests of outside SDKs (Firebase, Google Sign-In, Kakao and others) can only be checked with Xcode's "Generate Privacy Report" on a real iOS build. Check it once after a build.

## 2. Age rating

Where: App Store Connect → the app → [App Information] → [Age Rating]. This follows the questionnaire Apple changed in 2025 (ratings 4+, 9+, 13+, 16+, 18+; https://developer.apple.com/help/app-store-connect/reference/age-ratings-values-and-definitions). Both apps give the same answers.

| Group | Question | Answer | Why |
|---|---|---|---|
| In-app controls | Parental Controls | No | There is no such feature |
| In-app controls | Age Assurance | No | The PASS identity check confirms who books; it does not gate content by age |
| Capabilities | Unrestricted Web Access | No | The in-app web view opens only the payment gateway's page. Terms links open in the outside browser |
| Capabilities | User-Generated Content | Customer app **Yes** · partner app **Yes** | Customer reviews (text and photos) are shown to other users. The partner app can also show and report reviews |
| Capabilities | Social media | No | |
| Capabilities | Messaging and Chat | **Yes** | There is support chat with the operations team. Users don't message each other, but chat exists, so the broad answer is given (the rating stays 4+) |
| Capabilities | Advertising | No | No paid ads in the app. The business's own coupon and notice banners are not ads |
| Mature themes | Profanity, horror, alcohol, tobacco, drugs | None | |
| Medical and wellness | Medical or treatment information, health and wellness topics | None | |
| Sexuality and nudity | All items | None | |
| Violence | All items | None | |
| Chance-based activities | Gambling, simulated gambling, contests, loot boxes | None | Referral rewards are not rankings or competitions |

Expected result: **4+**. The rating for the Korean storefront is calculated from the same answers. Reviews may contain inappropriate posts, so the report and hide features must stay (`review-notes.md` section 6).

The IARC questionnaire for Google Play and the Microsoft Store is in `play-declarations.md` section 5.

## 3. Export compliance

| Question | Answer |
|---|---|
| Does the app use encryption? | Only the operating system's standard encryption (HTTPS/TLS, Keychain). No custom algorithms |
| `Info.plist` setting | `ITSAppUsesNonExemptEncryption = false` (both apps, `app/apps/*/ios/Runner/Info.plist`) |
| Is the question asked on every upload? | No. With the setting above, App Store Connect does not ask |
| Documents (US export classification, French declaration) | Not filed; the app is exempt |

Evidence: the apps use only the `http` package and `flutter_secure_storage` (iOS Keychain). Encryption of account numbers and the like (AES-256-GCM) happens on the **server** and is not in the apps. If custom encryption code is ever added to an app, check this answer again.

## 4. Judgement calls
1. **CI hash**: the identity-check CI is stored only as a SHA-256 hash. It identifies a person, so it is listed under "User ID".
2. **"Linked" for Device ID**: the analytics install ID is not linked to the account, but the push token is. App Store Connect asks once per type, so the answer is **Yes**.
3. **Acquisition source (UTM)**: stored only for web sign-ups. The iOS app has no way to receive it, so it is not in the labels (`data-safety.md` section 5, item 3).
4. **Operating-system address lookup**: turning coordinates into an address is done by Apple's own feature. Neither Dahae nor its partners receive the coordinates, so this is not collection. If a feature ever sends coordinates to the server, add "Precise Location".

## 5. What to update together
- When a new table or column holds personal data, update this table, `data-safety.md`, `website/privacy.html` and `PrivacyInfo.xcprivacy` together.
- If an outside **SDK** for ads, analytics or crashes is added, answer again for the data it collects and for "tracking".
