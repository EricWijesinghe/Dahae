# 15. Social sign-in and app links

**In this chapter**
You let customers sign in with Kakao, Naver, Google and Apple accounts.
You also add **app links**, so Naver sign-in returns straight to the app.
All of this is optional. Without it, customers sign in with their phone number and a code.

> **Before you start**
> - A developer account with each company you want: Kakao Developers, Naver Developers, Google Cloud (the same project as Firebase), Apple Developer
> - A way to reach the person who builds the apps. Some values must go into the app builds too.
> - The bundle IDs of the customer and partner apps: `kr.co.dahae.customer`, `kr.co.dahae.partner`

## 15.0 Know this first: server card and app build go together

A sign-in button shows and works only when values are in **two places**.

| Where | Who | What |
|---|---|---|
| The sign-in cards in the console's **[Setup]** | You | Values the server uses to check a sign-in |
| The customer and partner app builds | Your developer | Values the app uses to open the sign-in window |

- If an app build lacks the values, that button does not appear on the app's sign-in screen at all.
- The web pages and the Windows app have no social sign-in buttons.
- The iPhone app shows **Sign in with Apple** by default. If any other social sign-in shows, Apple always shows with it (App Store rule). The Android app has no Apple button.

![Social sign-in buttons in the customer app](../img/en/customer-login-social.png)

After you save a sign-in card and click **[Check connection]**, it shows **Saved · confirm by signing in from the app**. Sign-in keys can only be proven by a real sign-in from the app, so always finish with 15.6.

## 15.1 Kakao login

> **Warning** This card shows **Needs a developer the first time**.

### At Kakao

1. Sign in at developers.kakao.com.
2. Open My Applications → Add application.
3. Enter the app name and company name and save.
4. Go to App settings → App keys.
5. Copy the numeric **App ID** at the top.
6. Enable Kakao Login and turn on the **email** consent item.
7. Send the **Native app key** from the same App keys page to the developer who builds the apps.
8. (Optional) Copy the **Admin key** too. It unlinks Kakao when a customer deletes their account.

### In the console

1. Open **[Setup]** → **Kakao login**.
2. Paste the App ID into **App ID (number)**.
3. (Optional) Paste the Admin key into **Admin key (optional: unlinks Kakao on account deletion) (Optional)**.
4. Click **[Save]**.
5. Click **[Check connection]**.

You should now see: **Saved · confirm by signing in from the app**.

![Kakao login card](../img/en/console-setup-kakao.png)

## 15.2 Naver login

### At Naver

1. Sign in at developers.naver.com.
2. Open Application → Register.
3. Choose **Naver Login** as the API.
4. When registration finishes, copy the **Client ID** and **Client Secret**.
5. Send **only the Client ID** to the developer who builds the apps.

> **Warning** The Client Secret goes only into the console card. Never put it in an app or send it by chat.

### In the console

1. Open **[Setup]** → **Naver login**.
2. Paste into **Client ID**.
3. Paste into **Client Secret (server only)**.
4. Click **[Save]**, then **[Check connection]**.

You should now see: **Saved · confirm by signing in from the app**.

![Naver login card](../img/en/console-setup-naver.png)

> **Tip** To return to the app after Naver sign-in, **app links** (15.5) must be on. The app build also needs the server's domain, or the button won't show.

## 15.3 Google login

### At Google

1. Sign in at console.cloud.google.com and pick **the same project as Firebase** (chapter 16).
2. Go to APIs & Services → Credentials.
3. Create these **OAuth client IDs**:
   - **Web**: one, shared by both apps.
   - **iOS**: one for the customer app (bundle ID `kr.co.dahae.customer`) and **one more for the partner app** (bundle ID `kr.co.dahae.partner`). An iOS client belongs to one bundle ID only, so the partner app cannot use the customer app's.
   - **Android**: one per app (package names `kr.co.dahae.customer` and `kr.co.dahae.partner`, with the signing certificate SHA-1 your developer gives you).
4. Copy every client ID (they end in `.apps.googleusercontent.com`) into a notepad, with a note of which is which.
5. Send these three to the developer who builds the apps:
   - the **Web** client ID (store secret `GOOGLE_SERVER_CLIENT_ID`)
   - the customer app's **iOS** client ID (`GOOGLE_IOS_CLIENT_ID`)
   - the partner app's **iOS** client ID (`PARTNER_GOOGLE_IOS_CLIENT_ID`)

> **Note** The Android client IDs are not typed into any build, but Google needs them so sign-in works for each Android package. App Review signs in to the partner app with Google (chapter 25), so the partner iOS client ID must not be missing. A partner build without Google sign-in shows a warning in the build log. Confirm the list with your developer.

### In the console

1. Open **[Setup]** → **Google login**.
2. Paste **all** the client IDs from step 4 into **Client IDs** on one line, **separated by commas**.
   Example: `111-aaa.apps.googleusercontent.com, 222-bbb.apps.googleusercontent.com, 333-ccc.apps.googleusercontent.com`
3. Click **[Save]**, then **[Check connection]**.

You should now see: **Saved · confirm by signing in from the app**.

![Google login card](../img/en/console-setup-google.png)

## 15.4 Apple login

> **Warning** This card shows **Needs a developer the first time**.

### At Apple

1. Sign in at developer.apple.com.
2. In **Identifiers**, open the customer app's App ID and enable **Sign in with Apple**.
3. Do the same for the partner app's App ID.
4. (Recommended) On the **Keys** page, create a Sign in with Apple key, note its **Key ID** (10 characters) and download the **.p8 file**. You can download it only once.
5. Note your **Team ID** (10 characters) from the account page.

### In the console

1. Open **[Setup]** → **Apple login**.
2. In **Bundle IDs**, enter both bundle IDs separated by a comma: `kr.co.dahae.customer, kr.co.dahae.partner`
3. (Recommended) Enter the Team ID in **Team ID (10 characters) (Optional)**.
4. (Recommended) Enter the Key ID in **Sign in with Apple Key ID (Keys page) (Optional)**.
5. (Recommended) Open the .p8 file in a text editor, **copy all of it** and paste it into **Key file (.p8 contents, needed to revoke Apple sign-in on deletion) (Optional)**. That is everything from the `-----BEGIN PRIVATE KEY-----` line to the `-----END PRIVATE KEY-----` line.
6. Click **[Save]**, then **[Check connection]**.

You should now see: **Saved · confirm by signing in from the app**.

![Apple login card](../img/en/console-setup-apple.png)

> **Warning** Team ID, Key ID and .p8 go **all three together or none**. Only one or two shows **Fill in every required field.** With none, sign-in still works, but account deletion cannot revoke the Apple sign-in. The store review notes (chapter 25) say deletion revokes it, so we recommend entering all three.

> **Tip** (for developers) Apple's server notification URL is `https://<server address>/v1/auth/apple/notifications`. The store review notes say it is required for Korean developers when registering the Services ID. Leave this to your developer.

## 15.5 App links

These values let a customer return to the app after Naver sign-in instead of staying on a web page. **The developer who builds the apps gives you both values.** You only paste them.

1. On **[Setup]**, find the **App links** card. It is near the top, right below the **Uptime heartbeat** card and right above the **Domain** card (chapter 9, 9.2).
2. The card says which values are in use: **Using the values saved in the console** or **Using the server configuration values**.
3. Paste the developer's fingerprint into **Android signing fingerprint (SHA-256)**. **One per line**, up to 8. Each looks like `AA:BB:…`: 32 hex pairs separated by colons.
4. Enter the Team ID in **Apple Team ID (10 characters)** (the same Team ID as in 15.4).
5. The boxes below, **Android package names (one per line)** and **iOS bundle IDs (one per line)**, normally stay **empty**. The note under them says **Leave empty for the built-in app IDs. If you change them, use the same values in the store build.** Fill them, one ID per line, only when you agreed with your developer to publish under other app IDs (chapter 25, 25.2).
6. Click **[Check]**. It tests without saving.
7. Read the result: **Android app links: on** and **iPhone app links: on**, then **Checked. Not saved yet.**
8. Click **[Save]**.

You should now see: **Saved**, and the line at the top changes to **Using the values saved in the console**. Saving applies the values right away.

![App links card](../img/en/console-setup-app-links.png)

> **Tip** You can fill in one side first. The empty side shows off, with the note **With no values both links are off, and Naver sign-in may not return to the app.**

> **Warning** Changing the app ID boxes makes the **Store app build** card show **app IDs** as changed (chapter 28, 28.7). Each box takes up to 4 IDs, with two or more parts split by dots, such as `kr.co.example.app`. Each part starts with a letter and holds only letters, digits and `_` (iOS bundle IDs may also use `-`). A malformed ID shows **Something went wrong. Please try again.** on save.

> **Note** Your developer can set the same values from the server terminal with `sudo dahae applinks --android-sha256 … --apple-team-id …`. But once anyone has clicked **[Save]** on this card, the console values win and the command's values are not used. The card shows **Using the server configuration values** only while nothing has been saved in the console.

## 15.6 Sign in from the app yourself

1. Install the customer app that your developer built with the keys.
2. Check the sign-in screen shows Kakao, Naver, Google (and Apple on iPhone).
3. Tap each button and sign in with a real account.
4. Check you land back on the app's home screen.
5. Do the same in the partner app.

You should now see: each button signs you in and returns you to the app.

## If something goes wrong

| You see | Do this |
|---|---|
| No social buttons in the app | The app build lacks the values. Ask your developer to rebuild with the Kakao native app key, Naver Client ID, the Google Web client ID and both Google iOS client IDs (customer and partner). Web pages never show these buttons. |
| The partner app on iPhone has no Google button, or Google sign-in fails only there | The partner build has no iOS client ID of its own, or was given the customer app's. Ask your developer to save the partner app's iOS client ID as `PARTNER_GOOGLE_IOS_CLIENT_ID` and rebuild (15.3). |
| Naver sign-in doesn't return to the app | On **App links**, click **[Check]** and confirm both lines say **on**. If off, get the fingerprint and Team ID from your developer again. |
| **Fill in every required field.** | Google and Apple need at least one ID. Apple needs Team ID, Key ID and .p8 all together or none. Naver needs both Client ID and Client Secret. |
| **Please check the highlighted value** | Apple Team ID and Key ID are 10 letters or digits. Paste the .p8 including the BEGIN and END lines. |
| Buttons are grey and you can't change anything | Saved cards lock. From the owner account, click **[Unlock (owner code)]** (chapter 9). |
| **[Save]** on App links is grey | Only admin accounts can save it. |
| **Something went wrong. Please try again.** after **[Check]** or **[Save]** on App links | This also appears when a value has the wrong shape. Check one fingerprint per line, 32 colon-separated pairs, no more than 8, and a 10-character Team ID. |
