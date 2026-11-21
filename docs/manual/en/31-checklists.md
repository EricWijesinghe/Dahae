# 31. Checklists

**In this chapter:** the things to do before launch, daily, weekly, monthly and yearly, one list each. Print them and tick them off one by one. The number at the end of each line is the chapter with the details.

> **Before you start**
>
> - This chapter printed and a pen, or a notes app you can tick in.
> - A console admin account (the owner account is even better).

---

## 31.1 Before launch

### Server (with your developer)

- [ ] The server is in the Seoul region and runs Ubuntu 24.04 (chapters 4, 7).
- [ ] The domain's A record points to the server's public IP. On Cloudflare the cloud is grey (DNS only) (chapter 5).
- [ ] Opening `https://domain/health` in a browser shows text that contains `"ok":true` (chapter 5).
- [ ] The developer's `sudo dahae status` shows `✔ 앱 정상` (app OK) and `✔ HTTPS 정상` (HTTPS OK) (chapter 29).
- [ ] Only the needed ports are open in the cloud firewall: on AWS only 80 and 443 (22 stays closed, chapter 4), on other clouds only 22, 80 and 443 (chapter 7).

### Owner account and security

- [ ] You created the **owner account** yourself with the setup code. The developer did not create it for you (chapter 6).
- [ ] Two-step sign-in is registered in the authenticator app on your phone (chapter 6).
- [ ] The 10 recovery codes are written **on paper** and kept somewhere safe (chapters 6, 32).
- [ ] The **secret key copy** the developer made and its lock password are kept off the server, apart from each other (chapter 27).
- [ ] The console is installed like an app on your PC and phone (chapter 6).

### Setup cards (console **[Setup]**)

- [ ] **Business profile**: service name, main colour, logo, business details, support number (chapter 10).
- [ ] **Payment methods**: only contracted methods are on (chapter 10).
- [ ] **Payments (PortOne)** or **Payments (Toss Payments)**: keys saved, **Connected** on **[Check API key]** (chapters 11, 12).
- [ ] The webhook address is entered on the gateway's screen (chapters 11, 12).
- [ ] **Partner payouts**: manual transfer or Toss payouts chosen (chapter 13).
- [ ] **SMS & KakaoTalk (Solapi)**: **Connected**, sender number approved, AlimTalk templates approved (chapter 14).
- [ ] The social sign-in cards you use (Kakao, Naver, Google, Apple) are filled, and you signed in from the app yourself (chapter 15).
- [ ] **App push (Firebase)** is filled (chapter 16).
- [ ] **Address search (juso.go.kr)** is filled (chapter 16).
- [ ] **Offsite backup (S3-compatible)**: **Connected** on **[Check connection]** (chapter 17).
- [ ] The **Uptime heartbeat** card says connected (ask your developer) (chapter 17).
- [ ] Saved cards lock by themselves. Any card you unlocked is locked again with **[Lock now]** (chapter 9).

### Test payments (test keys, no money moves)

- [ ] One payment with each payment method (chapter 12).
- [ ] One full refund and one partial refund (chapters 12, 21).
- [ ] Closing the payment window brings the app back to a cancelled payment (chapter 12).
- [ ] On Android and iPhone, going to a card or easy-pay app and back to our app works (chapter 12).
- [ ] The webhook log on the gateway's screen shows our server received the notifications (chapter 12).
- [ ] Every test payment is refunded.
- [ ] Switched to live keys and checked **Connected** again (chapters 11, 12).

### Ready to operate

- [ ] Services and prices checked in **[Catalog & prices]** (chapter 18).
- [ ] **[System status]** → **Alert settings**: **Phone for alerts** entered and **[Save]** clicked (chapter 26).
- [ ] No red (**Action needed**) in **[System status]** (chapter 26).
- [ ] Staff accounts created and roles (**Admin** / **Consultant**) set (chapter 20).
- [ ] You read the default values in **[Operating rules]** once (chapter 23).
- [ ] **[Renewal calendar]** has dates for the domain, hosting, Apple developer account, Solapi, offsite backup storage and the secret key copy check (type **Other**) (chapters 23, 27).

### Store release (chapter 25)

- [ ] **[Setup]** → **Store review demo account** is on and its period set (chapter 17).
- [ ] The partner review account is **approved** (chapters 20, 25).
- [ ] After review, the demo account is turned off or its period has ended (chapter 17).
- [ ] If you use Apple sign-in, the Apple card's key and the server notification address are registered (chapter 15).

---

## 31.2 Daily (5 minutes in the morning)

- [ ] **[Dashboard]**: look at today's jobs, quotes waiting, unassigned and unread chats (chapter 19).
- [ ] If anything is unassigned, open it in **[Bookings]** and contact a partner or change the schedule (chapter 19).
- [ ] Answer unread chats in **[Inbox]** (chapter 19).
- [ ] If **[Quote desk]** has new requests, send quotes (chapter 19).
- [ ] If an alert text came to your phone, open **[System status]** (chapter 26).

## 31.3 Weekly (10 minutes)

- [ ] Look at red and yellow items in **[System status]**. Yellow is fine if it is only **Not set up** for services you do not use (chapter 26).
- [ ] **Automatic backup** is green and **Last backup** is within a day (chapter 27).
- [ ] **Offsite backup (S3-compatible)** under **External services** shows a recent success (chapter 27).
- [ ] **Server disk** has 20% or more free (chapter 26).
- [ ] **Recent server jobs** has no **Failed**. After the Monday automatic update, check **Version** (chapter 26).
- [ ] Handle the payouts due in **[Settlements]** (chapter 21).
- [ ] If **App error reports** shows the same error many times, send it to your developer (chapter 26).

## 31.4 Monthly (30 minutes)

- [ ] Offsite backup check: with the owner account, **[Check connection]** on the **Offsite backup (S3-compatible)** card → **Connected**. No need to unlock (chapter 27).
- [ ] Ask your developer to check that last night's file is in the offsite backup list (chapter 27).
- [ ] In **[Renewal calendar]**, check what renews this month and next (chapter 23).
- [ ] Skim last month in **[Activity log]** and ask your developer about anything you do not recognise (chapter 23).
- [ ] In **[Staff]**, accounts of people who left are **[Deactivate]**d (chapter 20).
- [ ] No unhandled requests in **[Account deletion and data requests]** (chapter 23).
- [ ] No test payments or refunds are left over (chapter 21).
- [ ] Check this month's charges from the gateways (PortOne, Toss Payments) and Solapi. Amounts follow each company's price list.

## 31.5 Yearly, or at renewal time

- [ ] Renew the domain (if it expires, the site stops opening).
- [ ] Check the payment method for hosting (server cost).
- [ ] Renew the Apple developer account.
- [ ] Check the offsite backup storage billing and access keys.
- [ ] **Secret key copy check**: check that both the copy file and the lock password are there (chapter 27).
- [ ] Check that the paper with the recovery codes is still there (chapter 32).
- [ ] Do a real restore practice on a test server with your developer (chapter 27).
- [ ] Confirm the VAT filing deadlines with your tax adviser. Keep the **VAT filing deadline approaching** alert on (chapter 26).

## 31.6 Before and after a manual update

- [ ] It is a quiet time with few customers.
- [ ] No red in **[System status]**.
- [ ] The owner's authenticator app is at hand.
- [ ] **[Update now]** → 6 digits → **[Continue]** (chapter 26).
- [ ] **Recent server jobs** shows **Done**.
- [ ] **Version** changed and **Uptime** is short.
- [ ] The booking screen opens in the customer app.

## 31.7 When a staff member leaves

- [ ] In **[Staff]**, **[Deactivate]** that person (chapter 20).
- [ ] If that person is the owner, first hand the role to another admin with **[My account]** → **[Hand over the owner role]** (chapter 8).
- [ ] Change the passwords of company services that person knew (gateways, Solapi, domain company and so on) (chapter 32).
- [ ] If that person handled keys, issue new keys and put them in **[Setup]** again (chapter 29, 29.9).

---

## If something goes wrong

> **If something goes wrong**
>
> - **I cannot tick an item** → Open the chapter at the end of that line and follow the steps again. If it still fails, see chapter 29.
> - **I keep forgetting the weekly check** → Add a weekly reminder on the same weekday in your phone's calendar.
