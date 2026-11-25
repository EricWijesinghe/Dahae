# 0. Install in one go (about 30 minutes)

English · [한국어](00-한번에-설치.md)

You never edit code or files. You rent a server, point your domain at it, paste one command, and the install is done.

**Order:** ① rent a server → ② connect the domain → ③ one command → ④ create the owner account → ⑤ setup wizard

> The installer's on-screen messages are in Korean today. This guide gives the English meaning of each message you will see. English installer messages come with the first country pack outside Korea.

## What you need
| Item | Where | Cost |
|---|---|---|
| One server | Any cloud or hosting provider (AWS Lightsail, DigitalOcean, Hetzner, Vultr, a local provider) | The provider's price list |
| A domain (for example `myshop.com`) | Any domain registrar | The registrar's price list |
| An authenticator app (Google Authenticator or similar) | Your phone's app store | Free |
| Your business registration details | Documents you already have (you enter them in the setup wizard) | - |
| Accounts with a payment gateway and an SMS provider | Their websites. Payment review and sender-number registration take time | Their rates |

> Start the payment and SMS sign-ups **before** you install. While you wait for their review you can install and fill in the other settings. Today these providers come from the Korea pack (PortOne or Toss Payments, and Solapi); see [2. Setup wizard](02-setup-wizard.md).

## ① Rent a server
1. Sign up with a provider and press **Create server** (or "Create instance").
2. Choose:

   | Item | Choice |
   |---|---|
   | Operating system | **Ubuntu 24.04 LTS** (22.04 and Debian 12 also work) |
   | Size (recommended) | **2 vCPU, 4 GB RAM** |
   | Size (minimum) | 1 GB RAM (below 1 GB the installer stops) |
   | Disk | 50 GB is enough |
   | Region | Close to your customers |

3. When the server is ready, note its **public IP address** (for example `203.0.113.10`).
4. In the firewall (security group) settings, open ports **22, 80 and 443**.

> Every provider names its screens differently. Provider-by-provider steps are in [9. Install by cloud provider](09-클라우드별-설치.md) (Korean).

> A server with less than 2 GB of memory (1 GB) still installs: the installer adds 2 GB of swap on disk, but it can be slow. Choose the recommended size for a server that takes real customers.

## ② Connect the domain (A record)
1. On your registrar's site, open **DNS management** (or "DNS settings").
2. Add an **A record**:
   - Host (name): `@` (or leave it empty)
   - Value (IP address): the public IP from ①
3. To make the `www` address work too, add a second A record with host `www` and the same IP. (Visitors to `www` are sent to the main address.)
4. Save. It can take from a few minutes to an hour to take effect.

## ③ Paste one command
> **For developers and installers.** Owners can use the **easiest way** below, which needs no terminal.

1. On the provider's screen, press **"Connect with browser SSH"** (or "Console") to open the terminal.
2. Copy this line, paste it, and press Enter:
   ```
   curl -fsSL INSTALL_URL | sudo bash
   ```
3. Answer two questions:
   - **Service address (domain)**: the domain you connected in ②, for example `myshop.com`
   - **Email**: where certificate notices go (press Enter to skip)
4. Wait. A new server can take about 10 minutes the first time.
5. If you see "the domain does not point at this server yet" (도메인이 아직 이 서버를 가리키지 않아요), don't worry:
   - The screen shows the **value to enter**, for example `A record   myshop.com   →   203.0.113.10`.
   - Fix it in the DNS screen from ②, wait a few minutes, then press **Enter** in the terminal to check again.
   - To do it later, type `s`. The install carries on, and the HTTPS certificate is issued by itself once the domain points at the server.
6. When it finishes you see this screen (the words in brackets are the English meaning):
   ```
   ✔ 설치가 끝났어요!   (채널: stable)                      [Install finished (channel: stable)]

   1) 브라우저에서 이 주소를 여세요:   https://myshop.com/admin/   [Open this address in a browser]
   2) 설치 코드를 입력하고 사장님 계정을 만드세요:               [Enter the setup code and create the owner account]

          설치 코드:  ABCD-2345-EFGH                         [Setup code]

   3) [설정] 메뉴에서 결제·문자·로그인 연동을 차례로 연결하세요.  [Connect payments, SMS and sign-in in Settings]
   ```
7. **Note the setup code (12 characters).** You use it once, in the next step.

> If it says "the app is fine, the HTTPS certificate is not ready yet" (앱은 정상이에요. HTTPS 인증서는 아직이에요.), the domain change has not taken effect. Check ② again, wait a few minutes, then run `sudo dahae status` and look for "HTTPS OK" (HTTPS 정상).

### The easiest way: install while creating the server, with the install page (no terminal)
If you paste the install setup while creating the server, the install starts as soon as the server boots. You never type a command.
1. Download the **install page** (`dahae-install-page.html`) you received with your licence and open it in a browser on your computer.
2. Enter the **domain** (for example `myshop.com`) and a **notification email**. The completion-alert address is optional.
3. Tick the two boxes: you chose Ubuntu 24.04, and you will open ports 22, 80 and 443 in the firewall.
4. Press **[Make the install file]**, note the **setup code** (12 characters) it shows, then press **[Copy]**.
5. On the provider's server-creation screen, paste it into whichever field exists: **"User data"**, **"cloud-init"** or **"Startup script"**. Field names per provider are in [9. Install by cloud provider](09-클라우드별-설치.md) (Korean).
6. Create the server and wait **10 to 30 minutes**. The install runs inside the server by itself.
7. Copy the server's **public IP** from the provider's screen, and set the A records `@` and `www` to that IP in the DNS screen from ②.
8. After 10 to 30 minutes, open `https://myshop.com/health` in a browser. When you see **`{"ok":true}`** the install is done. Go to ④.

> The setup code is valid for **7 days**, so you can take your time with DNS and the payment sign-up.
> To be told whether the install failed, enter a **completion-alert address** in step 2 (a ping URL from a service such as healthchecks.io). You get an alert on success and on failure.
> If `/health` does not open, open the provider's **browser SSH**, run `sudo dahae status`, and send that screen to your developer.
> The install page and the text you copied contain the setup code. Delete both when the install is done.

### Do this: keep a copy of the security key file (your developer helps for now)
The file `/opt/malkkeum/.env` on the server holds every security key. **Without it, backups cannot be restored.**
The console cannot hand it to you yet, so ask your developer to "make a copy of the security keys". They run `sudo dahae keys-backup` on the server, which makes a password-locked copy.
Keep that copy **off the server** (a USB drive or a password manager). Never paste it into a chat message or an email body.

## ④ Create the owner account (setup code)
1. Open `https://your-domain/admin/` in a browser.
2. On the welcome screen, enter the **setup code, your name, email and a password (12 characters or more)**.
3. Sign in with that email and password.
4. On the first sign-in you see **two-factor setup**. Add the key on the screen to your authenticator app, then type the 6-digit number the app shows.

> Lost the setup code, or more than 7 days have passed? Ask your developer or installer; they make a new one with `sudo dahae setup-code --renew`. Once the account exists, the code is no longer used.

## ⑤ Setup wizard
1. In the console's left menu, press **[Settings]**.
2. Fill the cards from the top: business details → payments → SMS and chat messages → app push → sign-in.
3. Each card has instructions and an **[Open site]** button. Paste the values, save, then press **[Test connection]** (on the PortOne card, **[Check API key]**).
4. The full order is in [2. Setup wizard](02-setup-wizard.md).

## Done
From now on these run by themselves:
- **A backup every day at 03:30** (kept for 14 days)
- **An update every Monday at 04:00** (it backs up first, and goes back to the previous version by itself if the new one does not answer)
- **HTTPS certificates**, issued and renewed automatically

Running the install command again is safe. It keeps your settings and data and only updates.

Next: [7. Operations commands](07-operations.md) · [3. App store release](03-app-release.md) · [4. Owner's daily guide](04-owner-guide.md)

## If something goes wrong
| Message on screen | Fix |
|---|---|
| "Administrator rights needed" (관리자 권한이 필요해요) | Check that the command ends in `sudo bash` and paste it again |
| "Another program is using port 80/443" (80/443 포트를 다른 프로그램이 쓰고 있어요) | Another web server is installed. Run `sudo systemctl disable --now nginx apache2`, then run the install command again |
| "Disk left … at least 5 GB is needed" (최소 5 GB 가 필요해요) | Choose a server with a bigger disk |
| "The service did not start" (서비스가 시작되지 않았어요) | Capture the `sudo dahae logs` screen and send it to your developer. Running the install command again is safe |
| The browser says "Not secure" | The domain change has not taken effect yet. Try again in 10 to 30 minutes |
