# 3. App store release (App Store and Google Play)

English · [한국어](03-앱-출시.md)

The **developer builds the apps with GitHub Actions and uploads them to the business's store accounts** ([guide 11](11-앱-직접-빌드.md), Korean with an English section). The business creates the store accounts and gives the developer admin access. The business has no technical work to do: no Mac, no code, no build tools.

Always create the accounts **in the business's name** (store policy: the business owns its apps). When the developer cannot own business accounts, the business is the account holder and the developer works through invited access.

## What you need
| Account | Cost | Time | Notes |
|---|---|---|---|
| Apple Developer Program (**organization**) | Store price list | Per the store | Needs a D-U-N-S number (free, takes a few days) |
| Google Play Console (**organization**) | Store price list | Per the store | Personal accounts must run a 12-tester, 14-day test before release, so an organization account is recommended |

## A. Apple (once)
1. The business enrols at developer.apple.com as an **organization** → pays → waits for the approval email. After approval, the account holder opens App Store Connect → Users and Access → Integrations → App Store Connect API and presses **Request Access** once (only the account holder can; without it the developer cannot create API keys).
2. The business invites the developer's Apple ID as an **Admin** member. The developer creates two App IDs, two apps and an APNs key ([guide 11](11-앱-직접-빌드.md), section 6).
3. Create a key under App Store Connect → Integrations → App Store Connect API (role **Admin**; an App Manager key fails the automatic signing export with "Cloud signing permission error").
   - The developer puts the downloaded `.p8` file, the **Issuer ID** and the **Key ID** into the secrets of the GitHub environment `store` (`ASC_KEY_P8_B64`, `ASC_ISSUER_ID`, `ASC_KEY_ID`). They never go into the repository or an email (see "Keeping keys" below).

## B. Google (once)
1. The business signs up at play.google.com/console with an **organization** account → pays the registration fee → verifies its identity.
2. The business invites the developer's Google account as an **admin**. The developer creates a Play service account with the **Release manager** permission and puts its key in `GOOGLE_PLAY_SERVICE_ACCOUNT_JSON` ([guide 11](11-앱-직접-빌드.md), section 5).
3. The developer creates the two apps (customer and partner) in Play Console (guide 11, section 5, item 1).

## Keeping keys
The `.p8` file, the keystore and the service account key are as sensitive as passwords. Never post them in the repository, a group chat or an email body. Put them only in the secrets of the GitHub environment `store`, and keep file copies in a password manager (guide 11 §3, §4).

## B-2. Social sign-in and notifications (optional)
To use Kakao, Naver and Google sign-in and app notifications, the business registers an app **in its own name** on each developer site (the developer helps through invited access), and the developer puts the keys in the `store` environment ([guide 11](11-앱-직접-빌드.md), sections 3 and 7).

## C. Who does what ([guide 11](11-앱-직접-빌드.md), section 1)
- **The business**: opens the store accounts, enrols as an organization, invites admins, pays the account fees, and enters the payment and messaging keys in the server console's setup wizard. Once the owner creates an admin account for the developer under **Staff**, the developer can save the messaging, push and sign-in cards the first time. The payment cards, unlocking, and the connection check after saving work only from the owner account.
- **The developer**: sets up the repository's `store` environment, creates the App IDs and apps, creates the API keys, runs the `store-apps` workflow, builds, uploads and submits for review.
- iPhone: install the build uploaded to **TestFlight** with the TestFlight app on an iPhone and check it (business staff or the developer).
- Android: check the draft on Play's **internal testing** track, install it on a phone, then release.
- Review submission: the developer submits with Admin access. If the business wants, its staff can submit with the same access.

## D. What the owner or installer does during review
- **Review account**: reviewers cannot receive local SMS. In the console, go to **Settings** → **Store review demo account**, set a number and a 6-digit code, turn it on, and write that number and code in the review notes. It turns itself off after 14 days.
- Privacy policy address: `https://<domain>/privacy.html`; account deletion page: `https://<domain>/delete-account.html` (both are created automatically when the server is installed).
- The store listing text and the data safety answers are in the `store/` folder of this guide bundle.

Review time depends on the stores' policies. Once approved, enter the store addresses under **Settings → Business details → App store addresses** in the console to turn on the website buttons.

## E. Store screenshots and listing text
- The developer makes the store screenshots in `docs/store/screenshots/` (customer and partner apps, iPhone, iPad and Android sizes, Korean and English). They are not committed; they are attached to the release as `dahae-store-images-<version>.zip`.
- When the screens change, the developer makes them again: `scripts/store/generate.sh`.
- The listing text is in `docs/store/listing-*.md`; check the length limits with `docs/store/check_listings.py`.
- Privacy answers: Google `docs/store/data-safety.md`, Apple `docs/store/app-privacy.md`.
- Other store questions (ads, target age, age rating, financial features, permissions): Google `docs/store/play-declarations.md`, Apple `docs/store/app-privacy.md` sections 2 and 3; review notes in `docs/store/review-notes.md`.

## App updates
The developer creates a new tag (for example `v1.6.1`) and runs `store-apps` again ([guide 11](11-앱-직접-빌드.md), section 10). Once the stores finish review and release, phones update automatically.
