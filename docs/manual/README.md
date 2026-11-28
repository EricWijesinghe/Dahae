# Dahae manual book

English · [한국어](ko/00-cover.md)

The manual book walks the owner and admins through installing, setting up and running Dahae, click by click. It is one tree written twice: [`en/`](en/00-cover.md) in plain English and [`ko/`](ko/00-cover.md) in Korean, with the same chapter numbers and file names. Start with the English edition; the Korean one is its translation.

The book covers the Korea pack, the only country pack available today, so the provider chapters (11 to 16) name Korean providers. Developer-only material is in chapter 33.

| Part | Chapters (English) |
|---|---|
| Before you start | [00 Cover and how to use this book](en/00-cover.md) · [01 What Dahae is](en/01-what-is-dahae.md) · [02 Before you start](en/02-before-you-start.md) |
| Install | [03 AWS: the deployer account](en/03-aws-account.md) · [04 One command to create the server](en/04-aws-launch.md) · [05 Connect the domain and check](en/05-domain-dns.md) · [06 Owner account and two-step sign-in](en/06-owner-account.md) · [07 Other clouds, in brief](en/07-other-clouds.md) |
| Setup | [08 Console tour](en/08-console-tour.md) · [09 How the Setup page works](en/09-setup-basics.md) · [10 Business profile and payment methods](en/10-setup-business.md) · [11 Payments (PortOne)](en/11-setup-portone.md) · [12 Payments (Toss Payments)](en/12-setup-toss-payments.md) · [13 Partner payouts](en/13-setup-payout.md) · [14 SMS and AlimTalk (Solapi)](en/14-setup-solapi.md) · [15 Social sign-in and app links](en/15-setup-social-login.md) · [16 Push, AI assistant, address search](en/16-setup-push-ai-juso.md) · [17 Offsite backup, heartbeat, review demo account, business hours](en/17-setup-backup-heartbeat-review.md) |
| Daily work | [18 Catalog and prices](en/18-catalog-prices.md) · [19 Dashboard, bookings, quotes, inbox](en/19-bookings-quotes-inbox.md) · [20 Partners, customers, staff](en/20-partners-staff.md) · [21 Payments, refunds, settlements, finance](en/21-payments-settlement-finance.md) · [22 Coupons, content, growth, campaigns, cases](en/22-marketing-growth.md) · [23 Operating rules, notification text, activity log, data requests](en/23-rules-notify-privacy.md) · [24 Customer and partner apps](en/24-customer-partner-apps.md) · [25 Publishing to the stores](en/25-store-release.md) |
| Running the server | [26 System status, alerts, updates](en/26-system-status-updates.md) · [27 Backups and restore](en/27-backup-restore.md) · [28 Rebranding: name, logo, colours and domain](en/28-brand-today.md) · [29 FAQ and fixes](en/29-troubleshooting.md) |
| Reference | [30 Glossary (KO/EN)](en/30-glossary.md) · [31 Checklists](en/31-checklists.md) · [32 Security rules](en/32-security-rules.md) · [33 Developer appendix](en/33-developer-appendix.md) |

**PDF.** `bash scripts/manual/build.sh [YYYY-MM-DD] [en|ko ...]` prints `targets/manual/Dahae-Manual-en.pdf` and `Dahae-Manual-ko.pdf` (needs Python, Node, Playwright with Chromium and `pdftotext`). Screenshots are in [`img/`](img/), listed in [`screenshots.json`](screenshots.json). Writers follow [`OUTLINE.md`](OUTLINE.md).

The step-by-step [guides](../guide/README.md) are shorter and cover the same ground; the manual adds every screen and field.
