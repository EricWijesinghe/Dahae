# 2. Setup wizard (console → [Settings])

English · [한국어](02-설정-마법사.md)

Every integration key is pasted into the console. Saved keys are **stored encrypted** and take effect at once, with no server restart.
Each card has step-by-step instructions and an **[Open site]** button. After saving, the owner presses **Test connection** and sees a green "Connected".

> The payment, messaging and sign-in providers below are those of the **Korea pack**, the only country pack available today. Other countries get their own providers when their packs ship; the card layout stays the same.

## Order and what you need
| Order | Card | What you need | Time |
|---|---|---|---|
| 1 | Business details | Business registration, mail-order sales registration, logo image | 10 minutes |
| 2 | Payments (PortOne), at least one of 2 and 2-1 | Business registration, a PortOne account and the gateway's review (also needed for PASS identity checks) | The provider's review time |
| 2-1 | Payments (Toss Payments), at least one of 2 and 2-1 | A Toss Payments e-payment contract (plus a recurring-billing contract for saved cards) | The provider's review time |
| 3 | SMS and AlimTalk (Solapi) | Sender-number registration documents | The provider's time |
| 4 | App push (Firebase) | A Google account | 10 minutes |
| 5 | Kakao / Naver / Google / Apple sign-in | A developer account for each | 10 to 20 minutes each |

> While you wait for the payment review, you can fill in the other cards.

## 1. Business details
- **Service name, brand colour and logo**: shown in the app header, on the website and in notifications. Picking a colour updates the preview at the top at once.
  - The customer and partner apps load the saved colour and logo. Offline, they keep the last colour they received.
  - If "white text is hard to read on this colour" appears under the colour, pick a darker one, so button labels stay readable.
- **Business information**: the details the law requires in the website footer and terms. They appear on the website as soon as you enter them.
- **App store addresses**: once the apps are live, enter their store addresses to turn on the website's download buttons.
- **Domain**: the address chosen at install. To change it later: Settings > Domain ([Owner's daily guide](04-owner-guide.md#changing-the-domain)).
- **Server approval code**: a second authenticator code that approves version upgrades, rollbacks and domain changes. Create it once in Settings.

## 2. Payments (PortOne)
Follow the steps in the card. In short:
1. Sign up at admin.portone.io → copy the Store ID
2. Issue a V2 API Secret → copy it
3. Under channel management, add a channel for each payment method (card, Kakao Pay, Naver Pay, Toss Pay, …) → paste each channel key into the field with the same name
4. Webhook: copy the **webhook address** shown in the card into PortOne (V2), then paste the `whsec_…` secret it gives you
5. Save → green "Connected"

> Payment methods you don't use can be switched off in [Settings] → [Payment methods]. The change applies at once.
> If a method is on but you see "the payment channel is not connected yet", customers don't see that method. Ask your installer to connect the gateway channel. Switching every method off is not saved.
> Cash receipts are turned on with the **Issue cash receipts** switch on the PortOne card. It starts off. Try it in the gateway's test environment before turning it on.

## 2-1. Payments (Toss Payments, optional)
Use this to take payments through Toss Payments directly, alone or next to PortOne. With both, methods that have a PortOne channel key go to PortOne and the other enabled methods go to Toss Payments. Follow the steps in the card. In short:
1. A Toss Payments e-payment contract (plus a recurring-billing contract for saved cards and regular visits)
2. developers.tosspayments.com → API keys → the payment-window MID → paste the client key (`test_ck_`) and secret key (`test_sk_`) of the **individual API integration keys** (not the widget keys `gck`/`gsk`)
3. (Optional) the two recurring-billing MID keys and the security key (64 characters)
4. In the developer centre's webhook menu, register the card's **webhook address** (`…/v1/webhooks/tosspayments`) on the payment-window MID and select `PAYMENT_STATUS_CHANGED` and `DEPOSIT_CALLBACK`
5. On a server that already takes payments, switch **Use** off first. Save the four test keys (the first save asks for the owner's authenticator code) → **[Check API key]** → green. This check looks at the secret key only
6. **Use: take payments through Toss Payments** is on by default. When on, every enabled method without a PortOne channel key is offered to customers through Toss Payments. So first, in Settings > **Payment methods**, enable only the methods Toss Payments approved → **Unlock** → save with **Use** on → make a test payment and refund for each method, and refund every test payment
7. **Unlock** → replace the four keys with live keys (`live_`) and save → **[Check API key]**. Under the "Payment connection" heading, check that "(test key: no real money moves)" has gone from next to Toss Payments in "Where payments go"

> Changing these settings never moves a payment, refund or saved card that has already started: it stays with the gateway it began on (PortOne or Toss Payments). Don't delete that gateway's card while such payments or cards remain. PASS identity checks need PortOne.
> Switching **Use** off stops only new Toss payments: Unlock → switch **Use** off and save. Toss payments in progress, refunds and saved cards keep working. The full rules and test order are in [Payment setup, section 6](payments-setup.md#6-토스페이먼츠로-결제받기-단독-또는-portone과-함께).

## 3. SMS and AlimTalk (Solapi)
1. Sign up at solapi.com → register a sender number (your customer-service number)
2. Create an API key → paste the API Key and API Secret
3. Enter the sender number, digits only
4. (Optional) KakaoTalk AlimTalk: connect your Kakao channel → enter the pfId and the approved template ID for each notification type ([guide 6](06-알림톡-템플릿-등록.md), Korean)

**The connection test does not send a message; it only checks the balance.**

## 4. App push (Firebase)
1. Create a project at console.firebase.google.com (use the same project for the app builds)
2. Project settings → Service accounts → **Generate new private key** → download the .json file
3. Open the file in a text editor, copy all of it, and paste it

## 5. Social sign-in
| Card | Value to paste | Value for the person who builds the apps |
|---|---|---|
| Kakao | App ID (a number) | Native app key |
| Naver | Client ID, Client Secret | Client ID (the Client Secret goes only in the server card, never in the app) |
| Google | Client IDs (comma separated) | iOS/Android client IDs |
| Apple | Bundle IDs: `kr.co.dahae.customer, kr.co.dahae.partner` (or your own app IDs) | – |

After saving a sign-in card you see "Saved · sign in from the app to check". Signing in from the app confirms it.

## 6. Offsite backup (S3-compatible storage, optional)
Backups inside the server are lost with the server. So every night an encrypted copy can also go to outside storage. Without it, the backups inside the server (14 days) still run.

**Which one?** For a small business, **Cloudflare R2** is simple, and any S3-compatible service works in the same fields (AWS S3, Backblaze B2, Naver Cloud Object Storage and others). A few backups cost very little; check each service's price list.

**Cloudflare R2 steps**
1. Sign up at dash.cloudflare.com → R2 → **Create bucket** (for example `dahae-backup`, public access off)
2. R2 → **Manage API tokens** → create a token → permission **Object Read & Write** → create
3. Copy the **Access Key ID** and **Secret Access Key** shown (the secret is shown only once)
4. Settings → **Offsite backup** card:
   - Endpoint: `https://<account-id>.r2.cloudflarestorage.com` (the account ID is on the right of the R2 screen)
   - Region: `auto`
   - Bucket name, Access Key, Secret Key
   - Retention: 30 days by default
5. Save → **Test connection** (owner account, no unlock needed; it uploads a small file and deletes it at once)

**Backup files are encrypted.** The key comes from the server's `DATA_KEY`, so if you lose the server's `/opt/malkkeum/.env` file (which holds `DATA_KEY`), the offsite backups cannot be opened either. Don't edit this file; just keep a copy somewhere safe ([0. Install in one go](00-install.md), "keep a copy of the security key file").

**Restore:** run `sudo dahae restore-offsite` on the server to list the offsite backups. Then `sudo dahae restore-offsite <name>` downloads and restores the one you pick (type `yes` at the question).

## 7. Partner payouts (optional; the default is manual transfer)
Without setup, payouts are **manual transfers**. On the settlement screen, [Request payout] shows the partner's account details; transfer the money in your banking app, then press **[Mark as transferred]**. Each view of the account details is logged. No contract is needed.

**To pay out automatically through Toss Payments payouts** (a Toss Payments payout contract comes first):
1. Try it first with a **test secret key** from the Toss Payments developer centre
2. Settings → **Partner payouts** card → method: `toss`, enter the secret key → save
3. Settlement screen → [Request payout] → [Request via Toss] for each partner. After sending, [Check status] shows whether it finished
4. Failed payouts show the reason; [Retry] sends them again. The server stops the same payout from going out twice

> Before switching to a live secret key, a developer must compare the Toss payouts API documentation and endpoints with the code (the comment at the top of `backend/crates/api/src/providers/payout.rs`).

## When you are done
The progress bar at the top fills up and "Everything is ready!" appears. Next: [3. App store release](03-app-release.md).
