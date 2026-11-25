# 4. Owner's guide: the console, day to day

English · [한국어](04-사장님-가이드.md)

Console address: `https://<domain>/admin/`. Nothing to install: it works like an app on both PC and phone.

## Install it like an app (once)
| Device | How |
|---|---|
| PC (Windows) | Open the console address in **Edge** or **Chrome** → the **Install app** icon (⊕) at the right of the address bar → **Install**. A "Dahae admin" icon appears in the Start menu and taskbar |
| Android phone | Open the console address in **Chrome** → ⋮ at the top right → **Install app** (or Add to home screen). It opens straight from the home-screen icon |
| iPhone | Open it in **Safari** → the Share button → **Add to Home Screen** |

New features arrive the next time you open it; there is nothing to update.
Press `Ctrl + K` to jump to a menu or a booking number.

## Five minutes every morning
1. **Dashboard**: check today's jobs, quotes waiting, unassigned bookings and unread chats.
2. If there are **unassigned** bookings, open them under **Bookings** and contact a partner or move the schedule.
3. Answer **unread chats** in the **Inbox** (the customer app is notified right away).

## System status (once a week, and when something is wrong)
**System status** shows at a glance whether the server is healthy. Each item is **green (fine), yellow (check) or red (act)**.

- **Server status**: the colour at the top follows the worst item.
- **Database**: response time and size. If red, restart the server or ask the installer to check.
- **Server disk**: red below 10% free. Clear old backups or grow the disk.
- **Automatic backup**: red when there has been no backup for over 36 hours. Ask the installer to check `sudo dahae backup`.
- **Server errors (24 hours)**: the count of 500-range errors. Many turn it yellow or red. The installer reads `sudo dahae logs` and passes it to the developer.
- **Payments to check**: a payment went through but its automatic processing failed. Check that payment in the payment provider's admin screen and refund it there if needed.
- **Payment notifications (webhooks)**: the signature check on a payment provider's notification failed. Check the webhook secret under Settings > Payments again.
- **External services**: for each of payments, messaging, push and social sign-in, the last success and failure times and the error code. Under a red item, follow its instructions: paste the key again in the settings card and press **Check connection**.

**Automatic alerts**: when a problem repeats, alerts collect in the inbox and an SMS goes to the owner's phone, at most once every 6 hours for the same problem. In **Alert settings** at the bottom of **System status** you choose which alerts to get, the quiet hours (by default 10 p.m. to 7 a.m.) and the phone number for SMS. Problems during quiet hours are reported once when they end.

**Payment history mismatch** (the "PortOne payment history mismatch" alert, on by default): every day after 3 a.m. the previous day's PortOne payment history is matched against the app's payment records. A different amount, or a payment on one side only, is reported. The check runs once a day.

## Quotes (move-in and move-out cleaning)
1. New requests collect in the **Quote desk**, together with the quotes partners sent.
2. Pick the partner quote you like, enter the final price → **Send quote**.
3. When the customer pays the deposit (10%), the booking is assigned to that partner automatically.

## Partners
- **Partners**: check newly registered partners and press **Approve**; **Suspend** if there is a problem.
- Before approving, fill in the **verification checklist**: ID checked, tax type checked, experience checked, emergency contact checked. All four must be ticked to approve. **Save checklist** keeps a partial checklist.
- **Profile**: photo, introduction, employment type (freelancer, employee, contractor) and start date. The emergency contact is stored encrypted, and every view is logged.
- The **resident registration number (or business registration number)** in **Profile** is needed for the payment statements filed with the tax office (Korea pack). Explain the purpose to the partner and get their agreement before entering it. After saving, only the last 4 digits show. Saving with the field empty keeps the stored number. The full number shows only after **Show original**, which asks for the authenticator code again, and every view is logged.
- The list at the top of **Partners** (rows marked **Waiting for approval**): team members a partner invites must be **approved** by the owner before they can see customer addresses and take jobs. Until then, no jobs show in the team member's app.
- **Payouts**: the list of payouts due. **Request payout** creates a payout. With manual transfer, check the details with **Show account**, make the transfer, then **Mark as transferred**. With Toss payout service, **Request payout via Toss** → **Check status**. Failed payouts: **Retry**. Disputed ones: **Hold** (a requested payout cannot be held).

## Tax invoices (Korea pack)
- Under the tax section of **Finance**, **Tax invoice upload CSV** downloads only the requests not exported yet. A request downloaded once does not appear in the next export (downloading the same period again gives an empty file).
- Press **Export again** only when the file was lost or damaged. It downloads the already exported requests as the same file. It runs only after you press **Export again** once more in the confirmation.
- After issuing in Hometax, record the approval number.
- When a payment is fully refunded, tax invoice requests not exported yet are cancelled automatically. Requests already exported or issued in Hometax stay in the list; cancel those yourself in Hometax with an amended tax invoice. On a partial refund, the request stays.

## Marketing
- **Coupons and content**: edit the home banners, notices and FAQs in Korean and English. Issue coupons to customers.
- **Catalogue · Prices**: service prices, bookings per time slot, and the move quote formula. Saving changes the prices in the apps and on the website right away.
  - For services chosen by count (air conditioners, sofas, mattresses, windows, pest control and disinfection), edit the option names, prices, durations, the maximum per option and the multi-unit discount (0 to 50%) here.
- **Catalogue · Bundles**: the title, order, image, products and visibility of the four big menus on the app's first screen (move-in and move-out cleaning, home cleaning, appliance and furniture cleaning, pest control and disinfection). The move cleaning bundle also sets the products for new and older buildings and the home-size choices. A product can be in one bundle only.
- **Catalogue · Peak pricing**: the weekend, holiday and evening surcharge rates and their cap. With "Apply the surcharge to fixed prices and regular cleaning" off, fixed-price bookings and regular cleaning are booked at the base price (quotes always show the surcharge). It is off by default, so booking prices do not change until you turn it on. Once turned on and saved, it applies from new bookings and the next charged regular cleaning visit. The surcharge counts toward coupon and regular-discount calculations and toward partner payouts. It is not counted in the regular cleaning monthly revenue (MRR).

## Operating rules (limits the owner sets)
- Under **Operating rules** you can change the business limits, advertising send counts, points limits, the partner quality window, and server and alert thresholds.
- Under each item, its **allowed range** and either **applies now** or **applies after a server restart** are shown. Press **Save** after changing a value.
- Advertising send counts, points limits and server alert thresholds can only go **below the default** (or, for alert thresholds, only **alert sooner**). The review report threshold can only go **above the default**. These limits are for safety.
- **Business alerts** (on by default): a confirmed booking with no partner for over 24 hours; same-day cancellations above 30% of today's visits (only with 5 or more bookings); 7 days before a VAT filing deadline (Korea pack: for companies 25 January, April, July and October; for individual general taxpayers usually 25 January and July, so confirm the dates with your tax adviser). The thresholds are under server and alert thresholds in **Operating rules**. The booking flow card (dashboard and status page) shows time to assignment, unassigned bookings, same-day cancellations and the no-show rate by service.
- **Reset to default** returns the install-time value. Every change is kept in the server log.
- **Block job acceptance before partner training** is off by default. When on, partners who have not finished the required training cannot accept jobs. Turn it on once the training screens are ready.
- For items marked "cannot be changed here" (for example the payment reconciliation interval), ask the developer.

## Staff
- Create support staff accounts under **Staff**. The **Agent** role covers bookings and chat only; **Admin** covers every setting.

## Activity log (owner only)
**Activity log** shows who did what and when, newest first. Refunds, customer contact lookups, staff changes and payout holds are recorded here.

1. **Period**: choose today, the last 7 days, the last 30 days or all time. The default is the last 7 days.
2. **Who** and **What**: pick from the drop-downs to see only that person or that kind of action.
3. Click a row for details. Records cannot be deleted. If something looks wrong, tell the installer or the developer.

## Account deletion and privacy requests
- Under **Deletion and privacy requests**, enter the customer's **phone number** and press **Find**.
- **Download a copy (CSV)**: downloads the copy of personal data the customer asked for, as a spreadsheet. Give the file to that customer only.
- **Delete account** (admins only): you must type the customer's phone number again to proceed. The name, contacts and the like are erased and **this cannot be undone**. Booking and payment records stay, as the law requires.
- The **request log** below keeps every request as completed or failed.

## My account (owner)
- The owner's account is the **owner**. There is exactly one owner, who can change account security and server settings.
- The admin created during first setup is the owner. If there is no owner, an admin can take the role under **My account** → **Become the owner**.
- On first sign-in, after setting up the authenticator app (OTP), **10 recovery codes** appear. Write them on paper, keep it somewhere safe, then press **I've written them down** to unlock the owner features.
- Every owner feature asks for the 6-digit authenticator code. Each code works once.
- Under **My account** you can: change the phone that gets alerts (a notice SMS goes to the old number), change the password (signs out every device), sign out of every device, make new recovery codes (the old ones stop working), and hand over ownership (the receiving admin must also enter their own authenticator code).
- If you lose the authenticator app, owner features are blocked. The server administrator can create a one-hour reset code; with it and your password you set up two-step verification again. (The console has no screen to enter the reset code yet, so the server administrator has to help.)

## Changing the domain
**Settings** → **Domain** shows the current address and any old addresses that still answer.
0. Once: **Settings** → **Server approval code** → **Register** adds it to the authenticator app. Without it, the server refuses the change after you enter the authenticator code.
1. Point the new domain's A record at this server's IP. Leave the old domain's record as it is.
2. Enter the new domain and press **Check DNS**. When it says "points at this server", continue.
3. **Change** → authenticator code → server approval code. The server first checks the new address; if it does not answer, nothing changes and the reason is shown. If it answers, the console pauses for about 30 seconds and comes back on the new address. The approval prompt shows the job as the server itself wrote it. If it differs from the domain you entered, do not approve.
4. Copy each value from the checklist into its site (PortOne and Toss webhooks, PG site address, **add** the Naver callback, search registration, store addresses) and tick it.
5. Installed apps keep working on the old address. **Retire** the old address only after a new store build is out and the minimum app version is raised (needs the authenticator code).

## When this happens
| Situation | What to do |
|---|---|
| A customer paid but says the booking is missing | Wait 1 to 2 minutes; it recovers automatically (matched against the payment provider). If it is still missing, search **Bookings** by phone number |
| Many customers say payment fails | **Settings** → the payment card's **Check API key** |
| Customers say the verification SMS does not arrive | **Settings** → the messaging card's **Check connection**, and check the Solapi balance |
| A settings card is red | Paste the key again as the card says and save |
| The server does not open | Ask the installer to check `sudo dahae status` |
