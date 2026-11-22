# 00. Cover and how to use this book

**Dahae (다해) Manual Book · English edition**

This book shows how to install, set up and run Dahae, the booking, payment and partner payout platform for home-cleaning companies, from the first step to daily work.

| Item | Details |
|---|---|
| Edition | 2nd edition |
| Version covered | Dahae v1.6.0 (released 2026-11-22), including the one-command AWS install (chapter 4), rebranding in the console (name, logo, colours, the Store app build card) and the domain move (chapter 28), and the server settings, uptime heartbeat, business hours, Toss payouts switch and app IDs now in the console (chapters 13, 15, 17 and 26) |
| Who it is for | The owner and admins of the company that uses Dahae. No technical knowledge needed |
| Developer material | Collected in chapter 33 (developer appendix) |
| Copyright | © 2026 Eric Wijesinghe. All rights reserved |

## In this chapter

How to read this book, what its marks mean, and the overall order of work.

## 0.1 Ownership and license

- Dahae is proprietary software of its copyright holder, **Eric Wijesinghe**. The software, source code, designs, logos and documentation all belong to the copyright holder.
- Use rights are granted only by a **separate written license agreement** with the copyright holder.
- The names and logos Dahae and 다해 are trademarks (™) of the copyright holder and are not licensed.
- The source is publicly visible. Viewing or cloning it grants no license.
- Licensing inquiries: EricWijesinghe@hotmail.com

> **Note** The governing license text is the `LICENSE` file in the repository (English). This book explains how to use the software. It does not change the agreement.

## 0.2 Marks used in this book

| Mark | Meaning |
|---|---|
| **[Save]** | A button, menu, tab or field name exactly as it appears on screen. The brackets mean "find this text and click it" |
| `sudo dahae status` | Text (a command) to copy and paste exactly. Do not change a single character |
| `example.co.kr` | An example value. Replace it with your company's real value |
| **You should now see:** | What the screen shows when the steps before it worked. If it looks different, stop and read "If something goes wrong" at the end of the chapter |
| **[Owner]** | Work the owner (owner account) must do personally |
| **[Developer]** | Work the developer (installer) does. The owner only checks the result |
| **to be confirmed** | Not yet checked on a real screen or in the documents. Judge from the screen, or ask the developer |

> **Tip** A box like this holds something handy to know.

> **Warning** A box like this holds a rule you must follow. Ignoring it can cost money or data, or weaken security.

## 0.3 About the pictures

- Console and app pictures are real program screens with sample data. Names, amounts and bookings are examples.
- Secret values such as passwords, setup keys, recovery codes and setup codes are blacked out or blurred.
- Colours and the company name are the default Dahae design. When you set your own name and colours, the screens change to match (chapter 28).
- In the English pictures, the buttons and menus are in English, but some sample names and texts stay in Korean.
- **We do not draw other companies' websites** such as AWS, Cloudflare or payment companies. Those steps are described in text. Other companies rename their screens often, so if a name is slightly different, pick the closest one.

## 0.4 The whole journey at a glance

When you start, read in this order.

| Step | Part | What you do | Who |
|---|---|---|---|
| 1 | Part 1 Getting started (ch. 1–2) | Learn what Dahae is; collect the accounts and documents you need | Owner |
| 2 | Part 2 Server install (ch. 3–7) | Create the server, connect the domain, create the owner account | Owner + developer |
| 3 | Part 3 Admin console (ch. 8–17) | Learn the console; connect payments, messages, sign-in and other services one by one | Owner |
| 4 | Part 4 Running the business (ch. 18–23) | Set services and prices; handle bookings, partners and payouts every day | Owner, admins, consultants |
| 5 | Part 5 Apps and stores (ch. 24–25) | Learn the customer and partner apps; release them to the stores | Owner + developer |
| 6 | Part 6 Updates, backups, monitoring (ch. 26–27) | Watch system status, update, check backups | Owner |
| 7 | Part 7 Brand (ch. 28) | Change the company name, logo, colours and domain | Owner |
| 8 | Part 8 Troubleshooting (ch. 29) | Look things up when something goes wrong | Anyone |
| 9 | Part 9 Appendices (ch. 30–33) | Glossary, checklists, security rules, developer appendix | Anyone / developer |

> **Tip** The first time, follow chapters 1 to 7 in order, to the end. After that, read only the chapter you need.

## 0.5 How to read this book

1. Read **"In this chapter"** at the top of each chapter. It tells you whether you need the chapter now.
2. Prepare everything under **"Before you start"** first, so you do not stop halfway.
3. Follow the numbered steps **one at a time**. Each step holds one action.
4. At **"You should now see"**, check the screen. If it matches, go on. If not, stop.
5. If you are stuck, read **"If something goes wrong"** at the end of the chapter, then chapter 29 (troubleshooting).
6. If that does not solve it, take a screenshot and send it to the developer. Cover any password, key or code first.

## If something goes wrong

| If this happens | Do this |
|---|---|
| A button name in the book differs slightly from the screen | Check whether the console is in Korean. If it is, use the Korean edition or switch the language to English (chapter 8). On another company's site, pick the closest name |
| You see a menu that is not in the book | Your version may be newer than this book. Ask the developer what changed |
| You are stuck at a part marked "to be confirmed" | That part is not confirmed yet. Ask the developer |
