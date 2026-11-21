# 11. Payments (PortOne)

**In this chapter:** connect PortOne so customers can pay by card and easy-pay apps.
You take four things from the PortOne site (store ID, V2 API secret, channel keys, webhook secret), put them into the **Payments (PortOne)** card in the console, and check with a test payment.

> **Before you start**
> - Business registration certificate (needed for PortOne sign-up and the payment provider's review)
> - A PortOne account (admin.portone.io), in the company's name
> - A contract and review with the payment providers (PGs). You apply through PortOne; the provider sets the review time.
> - The owner account and authenticator app (the first save needs the owner code)
> - The card basics from chapter 9

> **Warning** This card carries the **Needs a developer the first time** badge. For a first contract and channel setup, do it together with your developer.

> **Warning** PortOne can rename or move its menus. The PortOne screen names below are the Korean names used in the card's **How to set up** steps, with an English gloss. If the screens differ, follow PortOne's own help.

---

## 11.1 What goes where

| Console box | Value from PortOne | Looks like | Secret? |
|---|---|---|---|
| **Store ID** | Store ID (상점 아이디) | starts with `store-` | No |
| **V2 API Secret** | V2 API Secret | long text, no spaces | **Yes (server only)** |
| **Webhook secret (Optional)** | Webhook secret | starts with `whsec_` | **Yes (server only)** |
| **Channel key per payment method (Optional)** | One channel key per method | starts with `channel-key-` | No (the cash-receipt channel key is server only) |
| **Issue cash receipts (Optional)** | (a switch) | on / off | — |

> **Warning** Never show "server only" values to anyone. After saving, the console itself shows them only as `••••`.

---

## 11.2 Sign up to PortOne and apply to payment providers

1. In the console **[Setup]** page, open the **Payments (PortOne)** card. It sits right under the **Payments** heading.
2. Click **[Open site]** next to **How to set up**. admin.portone.io opens in a new window.
3. Sign up with your business registration ready, and sign in.
4. Following PortOne's instructions, apply for the providers you want (card, KakaoPay, NaverPay, TossPay and so on). While the review runs, you can fill in other cards.

![The PortOne card opened](../img/en/console-setup-portone.png)

---

## 11.3 Copy the store ID and the V2 API secret

**Store ID**

1. On the PortOne site go to **[결제 연동] → [연동 정보] → [식별코드 · API Keys]** (Integration → Info → IDs & API Keys).
2. Find the store ID (상점 아이디) and copy it. It starts with `store-`.
3. Paste it into the card's **Store ID** box.

![The PortOne page showing the store ID](../img/ko/external-portone-store-id.png)

**V2 API Secret**

1. On the same PortOne page click **[V2 API Secret 발급]** (create a V2 API Secret).
2. Copy the value. You may not be able to see it again, so paste it right away.
3. Paste it into the card's **V2 API Secret** box.
4. Use the eye button to check there are no spaces before or after.

**You should now see:** no red text under either box.

---

## 11.4 Add a channel key per payment method

In PortOne you create one "channel" per payment provider. Each channel has a channel key, and the console has one box per method.

1. On the PortOne site go to **[결제 연동] → [채널 관리]** (Integration → Channels).
2. Add a channel for each provider you use (card, KakaoPay, NaverPay, TossPay and so on).
3. In the console card, look at the boxes under **Channel key per payment method (Optional)**. Each box has a name and a help line.

![The channel key boxes in the PortOne card](../img/en/console-setup-portone-channels.png)

| Box | Channel key to enter | Help line under the box |
|---|---|---|
| **Card payment** | Card payment channel | Copy the card payment channel key from the channel list in the PortOne console |
| **Recurring billing key** | Saved-card / recurring channel | Channel key for recurring billing. Left blank, the card channel key is used |
| **KakaoPay** | KakaoPay channel | Copy this payment method's channel key from the PortOne console |
| **NaverPay** | NaverPay channel | (same) |
| **TossPay** | TossPay channel | (same) |
| **Virtual account** | Virtual account channel | (same) |
| **Identity verification** | Identity verification channel | Identity verification channel key, used for identity sign-in |
| **Cash receipt** | Cash receipt channel | Cash receipt channel key. Used on the server only, never sent to the customer app |

Under the heading **Other payment methods** there are more boxes: **PAYCO (other)**, **Samsung Pay (other)**, **Apple Pay (other)**, **Bank transfer (other)**, **Phone bill (other)**.

4. Copy one channel key from the PortOne channel list. It starts with `channel-key-`.
5. Paste it into the box for the same method in the console.
6. Repeat steps 4–5 for each method you use.
7. Leave the boxes for unused methods empty.

**You should now see:** no red text under the boxes you filled.

> **Tip** A method without a channel key does not appear in the customer app. You can add it later when you add the channel (unlock, 9.5).

> **Tip** With **Cash receipt** empty, cash receipts use the bank transfer or virtual account channel key. With **Recurring billing key** empty, the **Card payment** channel key is used.

---

## 11.5 Connect the webhook

A webhook is how PortOne tells our server that a payment finished.

![The webhook URL row and the webhook secret box in the PortOne card](../img/en/console-setup-portone-webhook.png)

1. In the console card, find the row **Webhook URL (paste into the PortOne console)**. The address looks like `https://(your domain)/v1/webhooks/portone`.
2. Click the **[Copy]** button at the right of the row. **Copied** appears.
3. On the PortOne site go to **[결제 연동] → [웹훅]** (Integration → Webhooks).
4. Paste the address and choose version **V2**.
5. After saving, PortOne shows a secret starting with `whsec_`. Copy it.
6. Paste it into the card's **Webhook secret (Optional)** box.

![PortOne's V2 webhook settings](../img/ko/external-portone-webhook.png)

> **Warning** The box says **(Optional)**, but fill it in. The webhook secret proves that a notice really came from PortOne. Left empty, a real business server refuses every PortOne notice (only a test-mode server accepts them).

> **Tip** A webhook never changes a payment by itself. When a notice arrives, the server asks PortOne again before acting.

> **Tip** If you later change the domain, this webhook address changes with it. The **After the move** list on the console's **Domain** card shows the new address with a copy button (chapter 28, 28.8.6).

---

## 11.6 The Issue cash receipts switch

The **Issue cash receipts (Optional)** switch starts off. The help line says: **Off means no cash receipts are issued. Test in the PortOne sandbox before turning it on.**

1. Leave it off for the first save.
2. If you will issue cash receipts, turn it on while test channel keys are in place (11.9) and check a test payment gets a receipt. While it is off no receipt is issued, so test with it on.
3. The card is locked by then, so: **[Unlock (owner code)]** → switch on → **[Save]**.
4. If the test shows a problem, turn it off again the same way.

---

## 11.7 Save (owner code)

1. Check that **Store ID** and **V2 API Secret** are filled in. These two are required.
2. Click **[Save]**.
3. The **Owner authenticator code** window opens. Type the 6 digits from your authenticator app into **6-digit code**.
4. Click **[Confirm]**.

**You should now see:** **Saved**, and the labels **Saved** and **Locked** on the card header.
Under the **Payments** heading you see **New payments go to: PortOne**.

> **Warning** Only the owner can make the first save. It decides where payment money goes.

---

## 11.8 Check API key

1. Click **[Check API key]** at the bottom of the card. (The owner can press it on a locked card.)
2. Wait a moment.

**You should now see:** a green **Connected** label on the card header.

![Connected after Check API key](../img/en/console-setup-test-ok.png)

> **Warning** **[Check API key]** checks only the V2 API secret. A wrong store ID or channel key only shows up at the first payment. So always do the test payment in 11.9.

---

## 11.9 Make a test payment

Test with PortOne test channel keys first, then switch to the real channel keys.

1. In the **Payment methods** card (10.9), check the method you want to test is on.
2. In the customer app, make a booking and pay by card.
3. In the console under **[Bookings]**, check the booking shows as paid.
4. Make a partial refund on it (chapter 21).
5. If you will use saved cards (recurring), save a card → pay with it → delete the card, once.
6. If you will issue cash receipts, first turn the switch on (11.6). Then pay once by bank transfer or virtual account and check a receipt is issued.
7. Refund every test payment.

**You should now see:** the payments and refunds look the same in the console and on the PortOne site.

> **To be confirmed** These have not yet been checked in a real test store. Check them with your developer during testing.
> - How real card decline codes arrive
> - The shape of the cash receipt response

---

## 11.10 Switch to real payments

1. On the PortOne site, prepare the real payment channels. Follow PortOne's help on how test and real channels are separated (to be confirmed).
2. In the console, open the card and click **[Unlock (owner code)]**.
3. In **Owner authenticator code**, type the 6 digits and click **[Confirm]**.
4. Paste real channel keys only into the boxes you are changing. (Empty boxes keep their saved values.)
5. Click **[Save]** within 5 minutes.
6. Click **[Check API key]**.
7. Make one small real payment and one refund.

> **Warning** The PortOne card has no **Test** / **Live** label. Keep your own note of whether the saved channel keys are test or real.

---

## 11.11 Never remove the PortOne card

Even when new payments go to Toss Payments later (chapter 12), keep the PortOne card as it is.

- Refunds of payments taken through PortOne use the PortOne card.
- Cards saved through PortOne, and their recurring charges, use PortOne.
- Identity verification (PASS) always uses PortOne.

> **Warning** Clicking **[Disconnect]** on the PortOne card makes all of the above fail.

---

## If something goes wrong

| What you see | Why | What to do |
|---|---|---|
| **The store ID must start with store-.** | You copied another value | Copy the value starting with `store-` from PortOne **[식별코드 · API Keys]** |
| **The V2 API secret does not look right. Paste the whole value with no spaces.** | Only part was copied, or a space slipped in | Copy the whole value again and paste it |
| **A channel key starts with channel-key-.** | That is not a channel key | Copy the channel key again from **[채널 관리]** |
| **The webhook secret must start with whsec_.** | You pasted the webhook address or another value | Copy the `whsec_` value from PortOne's webhook page |
| **Fill in every required field.** | Store ID or V2 API Secret is empty | Fill in both |
| **Only the owner can unlock** | An admin who is not the owner tried the first save | Sign in as the owner and save |
| **The key was rejected. Check the value you copied** | PortOne refused the V2 API secret | Create a new secret in PortOne, then unlock, paste, save |
| **Could not reach the service. Try again shortly** | PortOne did not answer | Check again in a few minutes |
| A payment method is missing in the customer app | No channel key for it, or it is off under **Payment methods** | Add the channel key (11.4) and switch it on (10.9) |
| A payment went through but the console does not show it as paid | The webhook address or secret may be wrong | Check 11.5 again and tell your developer |
