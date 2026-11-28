# 1. Server setup in detail

English · [한국어](01-서버-설치.md)

If this is your first time, follow the single page [0. Install in one go](00-install.md). This guide walks through the same steps in more detail and covers sample data and troubleshooting.

You never edit code or files. You rent a server, point your domain at it, paste one command, and you are done.

## What you need
| Item | Where | Cost |
|---|---|---|
| A domain (for example `myshop.co.kr`) | A domain registrar | The registrar's price list |
| A server (Ubuntu 24.04, in a region near your customers, **Seoul** for the Korea pack) | A server hosting company or cloud provider | The provider's price list |

> Choose "Ubuntu 24.04 LTS" and a region near your customers. Size: **2 vCPU / 4 GB memory recommended, 1 GB memory minimum**. A 50 GB disk is enough (the install needs at least 5 GB free).
> A 1 GB server installs too. The installer creates 2 GB of swap automatically, but it can be slow.
> Ubuntu 22.04 and Debian 12 also work.

## Steps

### ① Create the server
1. Create an instance (server) on the provider's site. Choose **Ubuntu 24.04** as the operating system.
2. When it is ready, note the **public IP address** it shows (for example `3.38.12.34`).
3. In the firewall (security group) settings, open ports **22, 80 and 443**.

### ② Connect the domain
1. Go to the **DNS management** page of the site where you bought the domain.
2. Add an **A record**: host `@` (or empty), value the IP address from ①.
3. To also answer on `www`, add the same A record for host `www`. (The `www` address redirects to the main address automatically.)
4. It can take from a few minutes to an hour to take effect.

### ③ Run the install command
> **This step is for developers and installers.** Owners should use the **easiest way** in [guide 0](00-install.md) (no black screen).

1. On the provider's page, press **"Connect via SSH in the browser"** (or "Console") to open the black screen.
2. Paste this one line as it is and press Enter:
   ```
   curl -fsSL INSTALL_URL | sudo bash
   ```
3. Answer two questions. A wrong answer is asked again, up to three times.
   - Service address → `myshop.co.kr` (the domain you connected in ②)
   - Email → where certificate notices go (press Enter to skip)
4. What the installer does by itself:
   - Checks the server (memory, disk, ports 80/443)
   - Installs the software it needs (Docker)
   - Creates the security keys
   - Checks that the domain points at this server
   - Starts the service, schedules a daily backup and a weekly update, and installs the `dahae` admin command
5. If the domain is not connected yet, it shows the A record value to enter and waits. After fixing DNS, press **Enter** to check again, or type `s` to skip.
6. After a few minutes you see a screen like this (the installer speaks Korean today):
   ```
   ✔ 설치가 끝났어요!   (채널: stable)

   1) 브라우저에서 이 주소를 여세요:   https://myshop.co.kr/admin/
   2) 설치 코드를 입력하고 사장님 계정을 만드세요:

          설치 코드:  ABCD-2345-EFGH

   3) [설정] 메뉴에서 결제·문자·로그인 연동을 차례로 연결하세요.
   ```
   It says: the install is done; open the admin address; enter the setup code and create the owner account; then connect payments, messaging and sign-in under **Settings**.
7. **Write down the setup code.** You use it once, to create the owner account.

> **If you installed with user data (cloud-init) or a startup script**, you will not see this screen; the result is only in the server's logs. Open `https://<domain>/health` in a browser: `{"ok":true}` means it is done. Use the setup code shown on the install-file page. Provider-by-provider steps are in [9. Install by cloud provider](09-클라우드별-설치.md) (Korean).

> If you see "the app is fine, the HTTPS certificate is not ready yet", the service is running and only the domain connection is left. Once the A record takes effect, the certificate is issued automatically. Check with `sudo dahae status`.

### ④ Keep the important file safe
The server's `/opt/malkkeum/.env` file holds every security key. Without it, backups cannot be restored.
- The developer makes you a copy of the secret keys ([guide 0](00-install.md), "Keep the security key file"). Store it **in a password manager or on a USB stick**. Never send it by messenger or email.
- You only keep this file; you never edit it. To change anything, use the `dahae` command or the console's **Settings** page.

### ⑤ Create the owner account
1. Open `https://<domain>/admin/`.
2. On the welcome screen, enter the setup code, your name, email and a password (12 characters or more).
3. After signing in you see **two-step verification setup**. Add the key to an authenticator app on your phone (Google Authenticator or similar) and enter the 6-digit code.
4. Go to **Settings** in the left menu and follow [2. Setup wizard](02-setup-wizard.md).

### ⑥ Get an alert when the server goes down (recommended)
A server cannot report that it is down by itself. Get a ping address from an uptime service such as healthchecks.io and enter one line:
```
sudo dahae heartbeat https://hc-ping.com/your-copied-address
```
The full steps are in [7. Operations commands](07-operations.md), section 8.

## What the owner prepares outside the server
The install command cannot do these for you. Contracts and reviews take time, so start early.

| Task | Where | Guide |
|---|---|---|
| Buy the domain, add the A record | Domain registrar | ② above |
| Open firewall ports 22, 80, 443 | Server provider | ① above |
| Business registration (Korea: business registration and mail-order sales report) | Tax office, local government | [2. Setup wizard](02-setup-wizard.md), section 1 |
| Payments (Korea pack: PortOne sign-up and a PG contract) | admin.portone.io | [2. Setup wizard](02-setup-wizard.md), section 2 |
| SMS and AlimTalk (Korea pack: Solapi sign-up, sender number, Kakao channel) | solapi.com | [2. Setup wizard](02-setup-wizard.md), section 3, [AlimTalk templates](06-알림톡-템플릿-등록.md) (Korean) |
| App push: a Firebase project | console.firebase.google.com | [2. Setup wizard](02-setup-wizard.md), section 4 |
| Kakao, Naver, Google and Apple sign-in | Each developer site | [2. Setup wizard](02-setup-wizard.md), section 5 |
| Store accounts (Apple $99 a year, Google $25 once) | Apple Developer, Google Play Console | [3. App store release](03-app-release.md) |
| (Optional) Offsite backup storage | Cloudflare R2, Naver Cloud and others | [2. Setup wizard](02-setup-wizard.md), section 6 |
| (Recommended) Uptime service | healthchecks.io and others | ⑥ above |

## Common admin commands (on the black screen)
When to use each command, with examples, is in [7. Operations commands](07-operations.md).

| Command | What it does |
|---|---|
| `sudo dahae status` | Checks that the service and HTTPS are fine |
| `sudo dahae version` | Shows the current channel and version |
| Server update | Press **Update now** in the console's **Server update** card (it backs up first and rolls back automatically if the update fails) |
| Go back to the previous version | Tell the developer or installer (when something looks wrong after an update) |
| `sudo dahae channel beta` | Switches to the beta channel (you can switch back to `stable`) |
| `sudo dahae backup` | Backs up right now |
| `sudo dahae restore backups/db-<date>-<time>.sql.gz` | Restores a backup (data and photo files) |
| `sudo dahae logs` | Shows the error log (Ctrl+C to leave) |
| Get the setup code again | Tell the developer or installer |
| `sudo dahae heartbeat <address>` | Alerts you when the server goes down |
| `sudo dahae demo load` | Adds sample customers, partners and bookings |
| `sudo dahae demo clear` | Removes only the sample data |
| `sudo dahae uninstall` | Removes the service (data stays; with `--purge`, data and backups are deleted too) |

The old name `malkkeum` runs the same commands.

## Adding and removing sample data
Use this when an empty screen makes the product hard to try. It adds 40 fake customers, 12 partners (with team members), bookings for the last 90 days and the next 14, payments, chats and settlements.

- **Add:** `sudo dahae demo load`. It does nothing if real bookings or payments already exist.
- **Remove:** `sudo dahae demo clear`. It removes only the sample data and never touches real customers, bookings or settlements. It asks once more before deleting.
- Sample customers have fake numbers like `010-0000-xxxx`, so no SMS or alert goes out. No payment is really charged.
- Sample agent and admin accounts get random passwords nobody can sign in with. `demo clear` removes those accounts too.
- Always run `sudo dahae demo clear` before the real service starts.

Automatic: **a backup every day at 3:30 a.m.** (kept 14 days), **an update every Monday at 4 a.m.** (rolled back automatically on failure), **automatic HTTPS certificate renewal**.
Running the install command again is safe: settings and data stay, and it backs up before updating.
A beta-channel server installs from the address above with `main` changed to `beta`. The install guide given to owners uses `stable`.
In the delivery repository, the install address in the guides is pinned to a version tag (`…/v1.2.3/install.sh`). Tags never change, and you can check the file against that tag's `SHA256SUMS` (`sha256sum -c --ignore-missing SHA256SUMS`).

## When something goes wrong
| Symptom | Fix |
|---|---|
| During install: "the domain does not point at this server yet" | Set the A record in ② to the value shown and press Enter. If you skipped, run `sudo dahae status` a few minutes later |
| "Another program is using port 80/443" | Another web server is on the machine. Run `sudo systemctl disable --now nginx apache2`, then the install command again |
| The browser says "Not secure" | DNS has not taken effect yet. Open it again in 10 to 30 minutes |
| Lost the setup code | Tell the developer or installer |
| The site does not open | `sudo dahae status`; if that fails, capture the `sudo dahae logs` screen and send it to the developer |
| Problems after an update | Tell the developer. They roll the program back to the previous version; customer and booking data are not deleted. To take the data back to an earlier point too, tell the developer |
