# 11. Store app builds (GitHub Actions): the developer builds, the business's store accounts publish

English · [한국어](11-앱-직접-빌드.md)

> A developer-run guide. The business owns the Apple and Play organization accounts and gives the developer admin access. The business does no build, signing or deployment work; it only creates the accounts, grants access and turns on operating settings in the console.

## 0. Overview

- The developer runs the **`store-apps`** workflow (manual, `workflow_dispatch`) in the repository [EricWijesinghe/Dahae](https://github.com/EricWijesinghe/Dahae).
  - Android AAB: a Linux runner (ubuntu)
  - iOS IPA: a macOS runner. GitHub-hosted runners are free for this public repository.
- The AAB goes to **the business's Google Play Console** (a draft on the internal testing track); the IPA goes to **the business's App Store Connect** (TestFlight).
- The business owns the Apple and Play organization accounts and gives the developer **Admin** access, which the developer uses to make keys and upload the apps.
- There is no extra cost to the business.
- The repository is public. Keys, passwords, `.p8` files and keystores are never committed: every secret lives only in the Secrets of the GitHub environment **`store`**.
- A signed written license is required before the work starts (`LICENSE` §1.4 and §3.1).

## 1. Roles

| Task | Who |
|---|---|
| Apple Developer Program as an **organization**, D-U-N-S number, agreements, App Store Connect API **access request** (once) | The business (account holder) |
| Play Console **organization** account, D-U-N-S number | The business |
| Invite the developer's Apple ID as an **Admin** member of the Apple organization | The business invites, the developer accepts |
| Invite the developer's Google account to Play Console as an **administrator** | The business invites, the developer accepts |
| App IDs, the two apps, the App Store Connect API key, the Play service account, the keystore, the GitHub secrets | The developer (with admin access) |
| Payment and SMS keys in the server console | Once the owner creates an admin account for the developer under **[Staff]**, the developer can make the first save of the SMS, push and sign-in cards. The payment cards (PortOne, Toss Payments), unlocking, and the connection check after saving work only from the owner account. |
| First Play AAB upload by hand | The developer |
| Review submission | The developer (the business's staff may submit with the same access) |

## 2. Accounts (the business creates them and gives the developer access)

- **Apple:** the business enrolls as an organization (D-U-N-S required, 99 USD per year; [guide 3](03-app-release.md)) and invites the developer's Apple ID as **Admin**.
- **Google Play:** the business registers an organization account ([guide 3](03-app-release.md)) and invites the developer as an administrator.
- **Firebase, social sign-in and the payment and messaging providers:** owned by the business; the developer gets access. Key table: [DEPLOY.md §3](../DEPLOY.md#3-store-apps-per-client).

## 3. GitHub setup (developer repository)

1. Settings → Environments → **New environment** named **`store`**. Required reviewers are optional.
2. Environment **variables**: `API_BASE` = `https://<domain>` (the first address baked into the app), `APPLE_TEAM_ID` (10 characters), `APP_LINK_DOMAIN` (bare host; after a domain move `new,old`, three at most). Optional: `CUSTOMER_APP_ID`, `PARTNER_APP_ID` (store app ids, default `kr.co.dahae.*`; set them before the first upload, because a new id is a new app). The home-screen name, icons and colour come from the server (`API_BASE/v1/brand`) at build time; the build stops if the server cannot be read (the input `brand_from_server` off uses the repository's name and icons).
3. Environment **secrets** (`store`), names exactly as written:
   - Android: `ANDROID_KEYSTORE_B64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` (`upload`), `ANDROID_KEY_PASSWORD`, `GOOGLE_PLAY_SERVICE_ACCOUNT_JSON` (for automatic uploads)
   - iOS: `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_P8_B64`
   - Native keys (optional; a missing value hides that feature, and the build still passes): `GOOGLE_SERVICES_JSON_B64`, `GOOGLE_SERVICE_INFO_PLIST_B64`, `PARTNER_GOOGLE_SERVICES_JSON_B64`, `PARTNER_GOOGLE_SERVICE_INFO_PLIST_B64`, `KAKAO_NATIVE_KEY`, `NAVER_CLIENT_ID`, `NAVER_CLIENT_NAME`, `GOOGLE_SERVER_CLIENT_ID`, `GOOGLE_IOS_CLIENT_ID`, `PARTNER_GOOGLE_IOS_CLIENT_ID`, `SOCIAL_PROVIDERS`
   - **Never** put `NAVER_CLIENT_SECRET` here. The server uses it.
   - The authoritative names are in `.github/workflows/store-apps.yml` and `scripts/ci/native-keys.sh`.
   - base64 is not encryption: paste it without line breaks.

## 4. Android upload key (once)

```
keytool -genkeypair -v -keystore dahae-upload.jks -alias upload -keyalg RSA -keysize 2048 -validity 10000
```

Base64-encode the file into `ANDROID_KEYSTORE_B64`, and add the passwords and the alias. Keep the original file in two separate places. Losing it blocks Play updates; recovery goes through Play support (to be confirmed). Keep Play App Signing on (the default).

## 5. Google Play Console (the business's organization, the developer as administrator)

1. Create the customer and partner apps under the app ids (`CUSTOMER_APP_ID` / `PARTNER_APP_ID`; default `kr.co.dahae.customer` / `kr.co.dahae.partner`). The package name cannot change after the first upload.
2. **The first AAB is uploaded by hand.** Run `store-apps` with platform `android` and upload off, download the AAB artifact, and upload it in Internal testing → create release → review → roll out. The Play API cannot create the first release.
3. **Service account** (for automatic uploads): Google Cloud console → **IAM & Admin → Service accounts** → create one (for example `<developer>-play-upload`) → enable the **Google Play Android Developer API** for the project → create a JSON key. Then Play Console → **Users and permissions → Invite new users** → the service account's email → release permissions on both apps. Paste the full JSON into the `store` environment secret `GOOGLE_PLAY_SERVICE_ACCOUNT_JSON`. (The Play Console "API access" page and linking a Cloud project are no longer needed.)
   - Naming rule: every account or key made for the developer carries the developer's name, so the business can see whose it is.
4. Later builds appear as **drafts** on the internal track. Check them and promote.
5. After the first upload, copy the **Play app-signing SHA-1** (Setup → App integrity) and register it in the Kakao and Google consoles. Put the SHA-256 from the same page into the console's **Settings → App links** (Android fingerprint); without it, Android Naver sign-in does not return to the app.

## 6. Apple App Store Connect (the business's organization, the developer as admin)

1. App IDs for the app ids (`CUSTOMER_APP_ID` / `PARTNER_APP_ID`; default `kr.co.dahae.customer` / `kr.co.dahae.partner`): enable **Push Notifications, Sign in with Apple and Associated Domains** on both. Signing fails if one is missing.
2. Create the two App Store Connect apps (bundle IDs as above, primary language of your customers). The record must exist before the first TestFlight upload.
3. **API key:** Users and Access → Integrations → App Store Connect API → Team Keys, role **Admin** (cloud signing needs it). The page opens only after the business's Account Holder has requested API access (the table in section 1). The `.p8` downloads **once**. Key ID → `ASC_KEY_ID`, Issuer ID → `ASC_ISSUER_ID`, base64 of the `.p8` → `ASC_KEY_P8_B64`.
4. Certificates and profiles are not made by hand; the workflow creates them with this key (to be confirmed on the first run).
5. **APNs key:** create it under Keys, and upload it to Firebase → Cloud Messaging → Apple app configuration for each iOS app.
6. Put the Apple **Team ID** in the server console under **Settings → App links**.

## 7. Firebase and sign-in keys (optional)

Firebase belongs to the business; the developer uses the granted access to download the config files and base64-encode them. Server-side keys go into the **setup wizard** of the server console ([guide 2](02-setup-wizard.md)), not into GitHub.

## 8. Run the build

1. **Actions → store-apps → Run workflow**. `app`: `customer`, `partner` or `both`. `platform`: `android`, `ios` or `both`. `upload` off builds and keeps the artifacts only; turn it on to upload. The version name is the newest `vX.Y.Z` tag (no tag: `pubspec.yaml`), so create the release tag first.
2. Android: the AAB is an artifact, and with `upload` on it becomes a draft on the internal track. iOS: with `upload` on, the IPA goes to App Store Connect; Apple processing takes 10 to 30 minutes.
3. Build numbers are set by [scripts/ci/build-number.py](../../scripts/ci/build-number.py). Never set them by hand.

## 9. Checks and review

1. **iOS:** TestFlight internal testers check on real devices (sign-in, photos, payment window, push, location).
2. **Android:** check the internal-track draft on a phone, then promote.
3. **Submission is manual.** Paste [review-notes.md](../store/review-notes.md), fill App Privacy from [app-privacy.md](../store/app-privacy.md), and submit both apps. For Play, use [data-safety.md](../store/data-safety.md) and [play-declarations.md](../store/play-declarations.md).
4. The server must be running before review. The business turns on the demo account in the console (**Settings → Review demo account**) and writes the number and code into the review notes. It turns off after 14 days ([guide 3](03-app-release.md)).

## 10. Updates

Tag a new version, run `store-apps` again, check the Play draft or the TestFlight build, and submit. Phones update by themselves after review and release. A name, logo, colour, app id or domain change in the console reaches the store apps only through a new build; the console's store build card lists what changed.

## 11. Common problems

| Symptom | Fix |
|---|---|
| The app cannot reach the server | `API_BASE` is missing or wrong. Fix it and rebuild |
| iOS signing fails | One of the three App ID capabilities is off |
| Play upload permission error | The service account needs release permissions on both apps |
| The first Play upload fails through the API | Expected. Upload the first AAB in Play Console |
| Kakao or Google sign-in fails on Android | The Play app-signing SHA-1 is not registered |
| No push | `GoogleService-Info.plist` or the APNs key is missing |

## 12. Not yet verified

Xcode 26.6 is on the macOS 26 runner image (per actions/runner-images); confirm it in the first run log. Automatic profile creation, the iOS card-app return (PortOne web checkout) and the app size can be confirmed only by the first real build.
