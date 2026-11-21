# 02. Before you start

## In this chapter

Before installing, collect the **accounts, documents and apps** you need, and apply first for the reviews that take long. After this chapter, you will rarely have to stop halfway through the install.

## Before you start

- Business registration certificate (사업자등록증, photo or PDF)
- One company email address (using the same address for every sign-up makes things easier)
- One phone (the owner's phone, for the authenticator app)
- A payment method (a card for the cloud bill)

## 2.1 Install two apps on your phone

1. On the owner's phone, open the **App Store** (iPhone) or **Play Store** (Android).
2. Install an **authenticator app**, for example Google Authenticator or 1Password. It shows the 6-digit code you need to sign in to the console.
3. Choose a **password manager app**. Keep account passwords, recovery codes and the key-copy password in it.

**You should now see:** the authenticator app icon on your phone's home screen.

> **Warning** Never put passwords, setup keys or recovery codes in **KakaoTalk, text messages, email bodies or documents.** Keep them in the password manager, and if you must pass one on, use another channel such as a phone call.

## 2.2 Accounts checklist

Create every account **in the company's (business) name**. Give the developer admin access only. Printing this table and ticking each row helps.

| Done | Account | Used for | Who creates it | When needed | Chapter |
|---|---|---|---|---|---|
| ☐ | **AWS account** (payment method added) | Renting the server | Owner | Before install | 3 |
| ☐ | **Domain** (e.g. `example.co.kr`) | The address customers and staff use | Owner and developer decide together (registration and DNS can be done in a Cloudflare account) | Before install | 5 |
| ☐ | **Authenticator app** | Console two-step sign-in | Owner's phone | Right after install | 6 |
| ☐ | **PortOne** (payments) + PG contract | Customer payments (PortOne and/or Toss Payments: at least one), identity checks (PASS needs PortOne) | Owner | Before opening (review takes long) | 11 |
| ☐ | **Toss Payments** (optional) | Payments (alone or next to PortOne), automatic partner payouts | Owner | Before opening, if you take payments with it | 12, 13 |
| ☐ | **Solapi** (SMS) + sender number | SMS, AlimTalk | Owner | Before opening (sender number registration takes long) | 14 |
| ☐ | **KakaoTalk channel** (AlimTalk, optional) | KakaoTalk AlimTalk | Owner | When using AlimTalk | 14 |
| ☐ | **Firebase** (Google account) | App push notifications | Owner (gives the developer access) | Before app release | 16 |
| ☐ | **Kakao, Naver, Google, Apple developer accounts** | Social sign-in | Owner | Before app release | 15 |
| ☐ | **Google Play Console** (organization) | Android app release | Owner (invites the developer as administrator) | Before app release | 25 |
| ☐ | **Apple Developer Program** (organization) | iPhone app release | Owner (invites the developer as Admin) | Before app release | 25 |
| ☐ | **Offsite backup storage** (Cloudflare R2, Naver Cloud Object Storage and others) | Backups outside the server | Owner | Recommended right after install | 17 |
| ☐ | **Uptime service** (healthchecks.io and others) | Tells you when the server stops | Owner | Recommended right after install | 17 |
| ☐ | **Password manager app** | Keeping secrets | Owner | From the start | 2.1 |

> **Tip** Google Play and Apple organization sign-up need a **D-U-N-S number**. Getting one can take a while, so apply early (the time depends on the issuer).

## 2.3 Documents checklist

| Done | Document | Used for |
|---|---|---|
| ☐ | Business registration certificate (사업자등록증) | PG review, sender number registration, Kakao channel, store organization sign-up, business details in the console |
| ☐ | Mail-order business report (통신판매업 신고증) | Business details in the console (shown at the bottom of the website, as the law requires) |
| ☐ | Sender number registration documents | Solapi sender number. Follow Solapi's list of required documents |
| ☐ | Logo image file | Business details in the console, app header, website |
| ☐ | Customer service phone, email and opening hours | Business details in the console |

## 2.4 Costs

This book lists no prices, because providers change them often. Check them where the table says.

| Item | Where to check the cost |
|---|---|
| AWS server | The AWS pricing calculator (https://calculator.aws/), with the **Seoul (ap-northeast-2)** region. It depends on server type, disk size, data transfer and snapshots |
| Other cloud servers | The provider's price list |
| Domain | The registrar's price list |
| Payments (PortOne, PG, Toss Payments) | Each company's fee table |
| SMS and AlimTalk (Solapi) | Solapi's price list. If AlimTalk fails, an SMS goes out instead, and SMS costs more than AlimTalk |
| Offsite backup storage | Each service's price list. Cloudflare R2 or Naver Cloud Object Storage cost a few thousand won a month for a few backups |
| Apple Developer Program | 99 USD per year (organization) |
| Google Play Console | The provider's price list |

> **Note** An AWS fixed IP (Elastic IP) costs nothing extra while it is attached to the server, and is charged when it is detached.

## 2.5 Apply for slow reviews first

These wait on another company's review and can take days. **Apply before you install.** You can install and set up other things while you wait.

1. Decide how you take payments: PortOne, Toss Payments, or both. You need at least one. Identity checks (PASS) need PortOne.
   - For PortOne: sign up for **PortOne and apply for the PG review**. (Time depends on the company.)
2. For Toss Payments: apply for the **Toss Payments electronic payment contract**. For saved cards or recurring visits, also apply for the **automatic payment (자동결제) contract**.
3. Sign up for **Solapi and register a sender number**.
4. Open a **KakaoTalk channel** and submit the AlimTalk templates for review (chapter 14). Kakao's review usually takes a few business days.
5. Apply for the **D-U-N-S number, the Google Play organization and the Apple organization**.

**You should now see:** a "request received" or "under review" notice from each company.

## 2.6 Sample timeline

This is the sample timeline from the launch plan. Real times depend on each company's review.

| Day | Who | Task |
|---|---|---|
| Day 0 | Owner | Sign the license agreement, choose the domain, create the AWS deployer account (chapter 3), register the Play and Apple organizations, invite the developer as admin, sign up for Firebase, Kakao, PortOne and/or Toss Payments, and Solapi |
| Day 0 | Developer | Confirm the license agreement, prepare deployment |
| Day 0–1 | Developer · owner | Create the server (chapter 4, developer); owner account and setup (chapter 6 and Part 3, done by the owner with the developer guiding on a call) |
| Day 1 | Developer | Build the apps and make the first uploads |
| Day 1–3 | Staff · developer | Test on real Android and iPhone devices (sign-in, photos, payment window, push, location) |
| At submission | Admin · developer | Turn on the review demo account (chapter 17), review notes, submit |
| Review | Stores | Google usually takes a few days; Apple usually 1–3 days (not guaranteed) |

> **Warning** The developer does not build or deploy anything before the written license agreement is signed.

## If something goes wrong

| If this happens | Do this |
|---|---|
| You are not sure whose name to use for an account | Use the company (business) name for all of them. Give the developer admin access only |
| The payment review is not finished yet | That is fine. Install the server and do the other setup first. Connect the payment card when the review is done |
| You do not know how to send a password to the developer | Do not send it in chat. Read it out on a call, or use the password manager's sharing feature |
| You are not sure which authenticator app to use | A widely used app such as Google Authenticator is fine. Find out in advance how to move it to a new phone |
