# 32. Security rules

**In this chapter:** the rules you and your staff must keep to protect customer data and money. The rules are short, but missing even one can cause an incident. Make sure your staff read this chapter too.

> **Before you start**
>
> - A password manager app. The company decides which one to use.
> - A screen lock on your phone (PIN or fingerprint).
> - A safe place for the paper with your recovery codes (a safe, a locking drawer).

---

## 32.1 Never put secrets in chat

Secrets are passwords, payment and SMS keys, webhook secrets, recovery codes, setup codes, the secret key copy and its lock password.

1. Do not write secrets in **KakaoTalk, text messages, email bodies or documents**.
2. Keep secrets in a **password manager**.
3. If you must pass one to someone, use something that leaves no written record, such as a **phone call**. Send the user name and the password by different routes (for example the user name by email, the password by phone).
4. Copy payment and SMS keys from the company's site and **paste them straight into the console's [Setup] card.** Do not leave them in Notepad or a chat on the way. Clear Notepad after pasting.
5. If a screenshot you send shows a key or code, cover it before sending.

> **Tip** Keys saved in the console are encrypted on the server. After saving they only show masked, like `••••`. The console is the safest place to keep them.

---

## 32.2 The owner's phone and authenticator app

1. **The owner creates the owner account personally.** The owner code (authenticator app) ends up on the phone of whoever creates the account with the setup code. If the developer creates it, the key that guards the payment cards ends up on the developer's phone (chapter 6).
2. Keep the owner's authenticator entry **only on the owner's phone**. Do not register the same entry on anyone else's phone.
3. Always use a **screen lock** on the phone.
4. Keep the phone's time on **automatic**. A wrong clock makes codes fail.
5. **Write the 10 recovery codes on paper** and keep it locked away. Do not keep a photo of them in the phone's gallery.
6. If you use or lose recovery codes, do **[My account]** → **[Make new recovery codes]** straight away. The old codes stop working (chapter 8).
7. When you change phones, check with your developer how to move the authenticator **before you wipe the old phone** (**to be confirmed**).

> **Warning** If anyone asks for your authenticator code by phone or message, never give it. The Dahae developer has no reason to ask for your 6-digit code either.

---

## 32.3 Using the console

1. Give **every staff member their own account**. Never share one account between several people. That way **[Activity log]** shows who did what (chapters 20, 23).
2. Give only the access people need. Staff who handle only bookings and chat are **Consultant**; only people who must change settings are **Admin**.
3. If your developer needs an admin account, create a separate one in **[Staff]**. When the work is done, **[Deactivate]** it.
4. On a PC that several people use, click **[Sign out]** when you finish.
5. Saved setup cards lock by themselves, and only the owner can unlock them. If you unlocked one, lock it with **[Lock now]** as soon as you finish. Even if you do not, it locks again by itself after 5 minutes (chapter 9).
6. Actions with money or personal data (payouts, showing an account, customer data copies, account deletion) ask for the authenticator code again. This is normal, even if it feels slow. It checks that the person really is who they say.
7. Sensitive values such as a partner's bank account or resident number are **logged every time they are viewed**. Open them only when you must (chapters 20, 21).

---

## 32.4 If you use AWS

1. **Do not create AWS access keys (long-term keys).** Server deploys run with temporary permissions and no keys (chapter 3).
2. Always turn on **MFA** for the developer's AWS account (`dahae-deployer`) (chapter 3).
3. Create the permissions boundary policy `dahae-boundary` but **attach it to nobody** (chapter 3).
4. Do not use the AWS root account (the one you signed up with) day to day. Turn on MFA for the root account too.
5. Keep AWS sign-in details in the password manager and pass them on as in 32.1.

---

## 32.5 When a staff member leaves

Do the checklist in chapter 31, 31.7, on the same day.

1. In **[Staff]**, click **[Deactivate]**.
2. If that person was the owner, do **[Hand over the owner role]** first. The owner account cannot be turned off.
3. Change the passwords of company services that person knew.
4. If that person handled keys, issue new keys and put them in **[Setup]**.

---

## 32.6 Customer personal data

1. When a customer asks for their data, **first check it is really them using their registered phone number** (chapter 23).
2. A data copy file from **[Account deletion and data requests]** goes **only to that customer**. Do not send it to anyone else or keep it on the PC. Delete it from the PC after handing it over.
3. Do not save customers' phone numbers or addresses in your personal phone contacts.
4. Account deletion cannot be undone. Check once more with the customer before running it.
5. Partner (pro) account deletion and data copies cannot be done in the console yet. Send those to your developer.

---

## 32.7 The server

1. **Do not edit** the server's `.env` file by hand. A wrong edit stops every connection.
2. **Do not change** the server's `DATA_KEY`. Saved keys and offsite backups would become impossible to open. If it must change, the developer does it.
3. Only you and your developer hold server access (AWS account, server company account).
4. Do not turn off the weekly automatic update. Security fixes arrive through updates. Only the latest stable and beta versions get security fixes.
5. Keep the secret key copy and its lock password in different places (chapter 27).

---

## 32.8 What the server does for you

The server and installer protect you in these ways without you doing anything.

- Staff sign-in needs both the password and the authenticator code. This two-step sign-in cannot be turned off.
- Too many wrong passwords or codes lock the account for a short time and send the **Suspicious sign-in activity** alert.
- Keys saved in the console and values such as partner bank accounts are stored encrypted.
- **[Activity log]** and consent records cannot be edited or deleted.
- Personal data whose keeping period has ended is deleted automatically.
- The server gets automatic security updates and SSH intrusion blocking (fail2ban). The server firewall (only 22, 80 and 443 open) is turned on automatically by the AWS install; on other clouds the installer asks (the default answer is "no"). If it is off, open only 22, 80 and 443 in the cloud's firewall screen (chapter 7).
- Backups are encrypted.

---

## 32.9 If you suspect an incident

Follow the steps in chapter 29, 29.9 straight away. And **always contact your developer**.

## 32.10 If you find a security problem in the program

If you find a security weakness in the Dahae program itself, **do not post it on a public board or in chat.** Report it privately as described in `SECURITY.md` in the Dahae repository (GitHub **Security → Report a vulnerability**). Do not include customer data in the report.

---

## If something goes wrong

> **If something goes wrong**
>
> - **A staff member sent a key in chat** → Treat the key as leaked: issue a new one on the company's site, put it in **[Setup]**, then revoke the old one (29.9).
> - **I lost the paper with my recovery codes** → Do **[My account]** → **[Make new recovery codes]** straight away.
> - **Someone asks me for my authenticator code** → Do not give it; tell your developer.
> - **I did not sign out on a shared PC** → Click **[My account]** → **[Sign out everywhere]**.
