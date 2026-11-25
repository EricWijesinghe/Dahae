# Google Play App content declarations

English · [한국어](play-declarations.ko.md)

Answers for the items in Play Console → app → **Policy** → **App content**, top to bottom. Enter them for the customer app and the partner app **each**. The app IDs are the GitHub `store` variables `CUSTOMER_APP_ID` and `PARTNER_APP_ID`; left empty they are `kr.co.dahae.customer` and `kr.co.dahae.partner` (an edition with its own store listing uses its own IDs). Where a row does not say otherwise, both apps give the same answer.

- The Data safety table is long, so it has its own file: `data-safety.md`.
- The sign-in details and notes for the review team are in `review-notes.md`.
- These answers match the v1.6.1 release build (points, and removal of social sign-in links on account deletion).
- Play Console's question wording changes from time to time. If the wording differs a little, pick the answer with the same meaning. Reference: https://support.google.com/googleplay/android-developer/answer/9859455 (checked 2026-11).

## At a glance

| # | Item | Answer |
|---|---|---|
| 1 | Privacy policy | `https://<domain>/privacy.html` |
| 2 | Ads | **No**, no ads |
| 3 | App access (sign-in details) | Some functionality is restricted → enter the review phone number and code |
| 4 | Target audience and content | **18 and over** only |
| 5 | Content rating (IARC questionnaire) | Answers in section 5 |
| 6 | News app | **No** |
| 7 | Data safety | `data-safety.md` |
| 8 | Government app | **No** |
| 9 | Financial features | **No financial features** if there are no points. With points, see section 9 |
| 10 | Health app | **No health features** |
| 11 | Advertising ID | **No**, not used |
| 12 | Permission declaration forms | None to submit (reasons in section 12) |
| 13 | Account deletion | In the app + `https://<domain>/delete-account.html` |

## 1. Privacy policy
- Address: `https://<domain>/privacy.html`. It must be a web page anyone can open without signing in (no PDF). It is created automatically when the server is installed.
- Both apps also open it in the app under **More** → **Privacy policy** and **Terms of service**.

## 2. Ads
- **No.** There is no ad SDK and no other company's ads are shown (no ad package in `app/packages/core/pubspec.yaml` or `app/apps/*/pubspec.yaml`).
- The app's own coupon and notice banners, and its own pushes to customers who agreed to marketing, are not "contains ads" in Google's sense.

## 3. App access
- Choose "All or some functionality in my app is restricted" and add instructions.
- Name: `Review login`; username: the review phone number; password: the review verification code.
- The partner app is entered differently: name `Review login (Google)`, username: the review Google account email, password: that Google account's password. The partner app refuses the fixed verification code (`review-notes.md` section 1).
- Paste `review-notes.md` section 2 into the other-instructions box. The real number and code are never written in the repository; enter them only when submitting.
- Turn the review account on in the console under **Settings** → **Store review demo account** (it turns itself off after 14 days by default, 30 at most).

## 4. Target audience and content
| Question | Answer |
|---|---|
| Target age groups | **18 and over** only |
| Could the app unintentionally appeal to children? | No |

Reason: booking and paying for cleaning (customers) and taking jobs and payouts (partners) are services for adults. Choosing any age group under 18 brings in the Families policy and many more requirements, so do not.

## 5. Content rating (IARC questionnaire)
The Microsoft Store uses the same IARC questionnaire. Use these answers as they are.

| Question | Answer | Reason |
|---|---|---|
| Email address | The business contact email | Where the rating authority writes |
| App category | **Utility, productivity, communication or other** (not a game) | |
| Violence, fear | None | |
| Sexual content, nudity | None | |
| Profanity, crude humour | None | |
| Drugs, alcohol, tobacco | None | |
| Gambling, simulated gambling | None | |
| Do users interact or exchange content? | **Yes** | Reviews (text and photos) are shown to other users. Chat is with the operations team only |
| → Can users report or block content? | **Yes** | Every review has a report button; enough reports hide it automatically (`review-notes.md` section 6) |
| Does the app share the user's current location with other users? | **No** | Coordinates are never sent to the server |
| Can users buy digital goods? | **No** | Only on-site cleaning (a physical service) is sold |
| Does the app give unrestricted internet access (browser features)? | No | The in-app web view opens only the payment page |

Expected result: an all-ages rating (IARC 3+, GRAC All in Korea). A "Users interact" notice may be added. Redo the questionnaire when the app's content changes.
Apple's age-rating questionnaire asks different questions; it is in `app-privacy.md` section 2.

## 6. News app
- **No.** This is not a news app.

## 7. Data safety
- Enter the table in `data-safety.md`.

## 8. Government app
- **No.** The app is not made by or for a government or public body. It uses the PASS identity check, but it is not a government service app.

## 9. Financial features
Every app must answer (https://support.google.com/googleplay/android-developer/answer/13849271).

| Situation | Answer |
|---|---|
| Customer app (with points) | Under purchase agreements, select only **"Rewards, points, air miles and other incentives"** |
| Partner app | **"My app doesn't provide any financial features"** |

Reasons and evidence:
- Dahae only collects payment for its own service (cleaning) through a payment gateway. There are no loans, money transfers, wallets, prepaid top-ups, investments, insurance or crypto assets, so "Mobile payments and digital wallets" and "Money transfer" are not selected.
- Points are earned only by bookings, sign-up and referrals, and cannot be bought or cashed out (`points_ledger` has no "purchase" kind). The form still has a "points" option, so a build with points should select it. **Owner to confirm**: selecting it may bring follow-up questions from Google. Answer: "loyalty points used only as a discount on cleaning prices, never sold or exchanged for cash".
- The partner app's payout account entry is for being paid for work, not a financial product.

## 10. Health apps
- Every app must answer. Choose **"My app does not have any health features"**. No health or medical data is handled.

## 11. Advertising ID
- **No.** The advertising ID is not used. The apps' `AndroidManifest.xml` has no `com.google.android.gms.permission.AD_ID` permission.
- Developer check: only the real build shows whether a plugin added the permission. After building, list the final permissions with `bundletool dump manifest --bundle <AAB file>`; if `AD_ID` appears, decide this answer again. (Work package WP-C, which adds this check to `check-native.py --aab`, is waiting to merge. Today `--aab` checks only the 16 KB page alignment.)

## 12. Permissions and declaration forms
These are all the permissions the apps declare themselves (`app/apps/*/android/app/src/main/AndroidManifest.xml`).

| Permission | App | Used for | Play declaration form |
|---|---|---|---|
| `INTERNET` | Customer, partner | Talking to the server | Not needed |
| `POST_NOTIFICATIONS` | Customer, partner | Booking and job alerts. Asked once after sign-in (`push_io.dart`) | Not needed (runtime permission) |
| `ACCESS_COARSE_LOCATION`, `ACCESS_FINE_LOCATION` | **Customer only** | Only when **Use my current location** is tapped on the address screen | **Not needed.** The form is only for background location (`ACCESS_BACKGROUND_LOCATION`), which Dahae does not use |

| Item asked | Answer |
|---|---|
| Photo and video permissions (`READ_MEDIA_IMAGES`/`VIDEO`) | **Not used.** Photos are picked with the system photo picker (`image_picker`), so neither the permission nor the declaration is needed. `check-native.py` checks this permission against a deny list |
| Camera | No permission declared. This version does not open the camera; photos are picked from the gallery (`ImageSource.gallery` only) |
| Foreground service (`FOREGROUND_SERVICE_*`) | **None.** There is no service type to declare |
| Background location | None |
| SMS and call log | None. Users type the verification code themselves |
| All files access, all apps query (`QUERY_ALL_PACKAGES`), exact alarms, full-screen intents | None |

Location notice: before asking for the permission, the app first takes the customer's agreement to the location-based service terms (`user_consents` kind `location`, `website/location-terms.html`). Coordinates are used on the device only, to suggest an address, and never sent to the server.

Developer check: the table above lists the permissions the apps write themselves. Permissions that plugins merge in must be checked once more in the final permission list of the real build (the `bundletool` command in section 11). If an unexpected permission shows up, fix this document and `data-safety.md`.

## 13. Account deletion
| Question | Answer |
|---|---|
| Can users create an account in the app? | Yes |
| How to delete in the app | **More** → **Account** → **Delete account** (customer: `DELETE /v1/me`, partner: `DELETE /v1/partner/me`) |
| Account deletion URL | `https://<domain>/delete-account.html` |
| Data kept after deletion | Booking and payment records for 5 years, dispute records for 3 years (Korean E-Commerce Act), consent records. Stated on the web page and in the privacy policy |

- Google requires that deletion can be **requested** from the web page without reinstalling the app. `delete-account.html` shows how to request it by support phone or email. Those contacts come from the console's **Settings → Business details**, so **be sure to fill in the support email** (when it is empty, the page shows no way to request).
- Deleting a customer account also removes the social sign-in links (`user_identities`) and revokes the Apple sign-in token (`privacy.rs`, `providers/apple.rs`).

## 14. Payment method (for reference)
- Dahae sells only on-site cleaning (a physical service). Play's payments policy keeps Play Billing out of physical goods and services, so a payment gateway (PortOne) is used. Regular cleaning (subscriptions) and tips are payments for the same physical service. Details: `review-notes.md` section 4.

## Before submitting
- [ ] The Play Console app's package name equals the build's app ID. It cannot change after the first upload
- [ ] The domain is set and the addresses in sections 1 and 13 really open
- [ ] The support email shows on `delete-account.html`
- [ ] Confirm the release build includes location, points and error reports, and match sections 4, 9 and 12 and `data-safety.md` to it
- [ ] Check the final permission list from the build file (the `bundletool` command in section 11)
- [ ] Turn the review account on and enter the number and code in section 3
