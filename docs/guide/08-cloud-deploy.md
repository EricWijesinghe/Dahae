# 8. Deploying Dahae on your own cloud account

English · [한국어](08-클라우드-배포.md)

> On AWS, [guide 10](10-aws-deploy.md) covers the AWS template and automatic deploys from GitHub.

This guide puts the whole path in one place: creating a Dahae server on **a cloud account in the owner's name**, connecting **the owner's domain**, and starting operations. The short path (rent a server, paste the install command) is [0. Install in one go](00-install.md); more server detail is in [1. Server setup in detail](01-server-setup.md). This guide ties the two together and adds choosing a provider, setting up the domain, and keeping database backups in outside storage.

> Commands and screen names here follow the repository's installer ([deploy/install.sh](../../deploy/install.sh)), server layout ([deploy/docker-compose.yml](../../deploy/docker-compose.yml), [deploy/Caddyfile](../../deploy/Caddyfile)) and backup code ([backup.rs](../../backend/crates/api/src/backup.rs)). Prices and provider menus change, so they are not listed.
>
> The providers named in section 7 (PortOne, Toss Payments, Solapi, Kakao, Naver, road-name address search) belong to the **Korea pack**, the only country pack available today.

## Contents

1. Before you start
2. Choosing a server
3. Creating the server
4. Domain and DNS
5. Running the installer
6. Database and backups
7. Connecting your services in the console
8. Changing the server or domain later
9. Checklist
10. If something goes wrong

---

## 1. Before you start

| You need | Why | When |
|---|---|---|
| A cloud account (with a payment method on file) | The account that rents the server. Create it in the owner's name | Before creating the server |
| One domain (for example `myshop.example`) | The address customers and staff use. Buy it from a domain registrar or Cloudflare | Before the install |
| An email address for certificate notices | The installer's second question. HTTPS certificate notices go there. Press Enter to skip | During the install |
| A phone with an authenticator app (Google Authenticator or similar) | The 6-digit code for the owner's sign-in ([guide 1](01-server-setup.md), step 5) | Right after the install |
| Business documents | Needed to connect payments, messaging and business details; not needed for the install itself ([guide 2](02-setup-wizard.md)) | When connecting (can be later) |
| A password manager or a USB stick | To keep the server's key file (`.env`) and the key copy (section 6) | Right after the install |

---

## 2. Choosing a server

### Operating system and architecture

- **Operating system:** Ubuntu 24.04 LTS is recommended. Provider-by-provider screens are in [9. Install by cloud provider](09-install-by-cloud.md). The installer also supports Ubuntu 22.04 and Debian 12 ([deploy/install.sh](../../deploy/install.sh) line 2, [DEPLOY.md](../DEPLOY.md) server requirements).
- **CPU architecture:** x86_64 (amd64) and arm64. On any other architecture the installer stops ([install.sh](../../deploy/install.sh) `arch_ok`).
- **Region:** the region closest to your customers and staff, so responses are fast. Customer data is stored on the server in the region you choose. For a Korean business that is Seoul.

### Size (what the installer checks)

| Level | Memory | CPU | Disk | Notes |
|---|---|---|---|---|
| Minimum | 1 GB or more | 1 vCPU | 5 GB free or more | Installs. With less than 2 GB of memory and no swap, the installer makes a 2 GB swap file (when 7 GB or more of disk is free). It may be slow |
| Recommended minimum | 2 GB or more | 2 vCPU | 50 GB recommended | What the installer's message asks for |
| Recommended (this guide) | 4 GB | 2 vCPU | 50 GB | Recommended for a server that takes real customers ([guide 0](00-install.md), step 1) |

- The installer stops below about 900 MB of memory. If it says memory is too low, choose a bigger size.
- A fresh install needs at least 5 GB of free disk. A re-run (update) needs 2 GB.
- Containers have memory limits sized for a 2 GB server: database 768 MB, API 512 MB, web server (Caddy) 384 MB, 1,664 MB in all. The API and web server may hold 65,536 open files ([docker-compose.yml](../../deploy/docker-compose.yml)).

### Server types

| Server type | Supported | Notes |
|---|---|---|
| A normal virtual machine (VM) with Ubuntu 22.04 / 24.04 or Debian 12 | **Yes** | Most cloud providers offer one (AWS Lightsail, AWS EC2, Naver Cloud Server, Vultr, DigitalOcean and others) |
| arm64 virtual machines | **Yes** | The installer accepts arm64. Prices differ per provider |
| Managed databases (RDS, Naver Cloud DB and similar) | **No** | The database runs inside the server as a PostgreSQL container. A separate managed database cannot be connected in this version ([docker-compose.yml](../../deploy/docker-compose.yml) `db`) |
| Ubuntu 20.04 or older, Windows servers, other operating systems | **No** | The installer runs only on Ubuntu or Debian with `apt-get` |
| Container-only services, serverless | **Not applicable** | The installer installs Docker, the firewall (ufw) and scheduled jobs directly on the server |

### 2-1. What runs, at a glance

Everything is on one server. You do not rent a separate database.

```
One cloud server (Ubuntu + Docker)
  web  (Caddy)          only ports 80 and 443 are open outside → HTTPS certificate, domain address
  api                   the API used by the customer app and the console. Not open outside
  db   (PostgreSQL 17)  reachable only inside the server. Data in the dbdata volume
Photos: uploads volume · Backups: /opt/malkkeum/backups/
Outside: domain A record → server IP,  offsite backup → S3-compatible storage (recommended)
```

- **Database:** there is no field for a database address or account. The installer builds the connection so `api` reaches the `db` container on the same server ([docker-compose.yml](../../deploy/docker-compose.yml)). A separate database such as RDS or Naver Cloud DB cannot be connected.
- **Domain:** you enter the root domain (for example `myshop.example`) once, at the installer's question (5-2). Add the A records as in section 4.
- **Order:** create the server (section 3) → static IP and firewall → domain A records (section 4) → install command (section 5) → owner account and two-step sign-in (5-6) → key copy and offsite backup (section 6) → console connections (section 7).

---

## 3. Creating the server

These steps are the same at every provider. Menu names differ, so look for the closest name on screen.

1. Press **Create server (instance)**.
2. Choose **Ubuntu 24.04 LTS** and your region.
3. Choose the recommended size from the table above (2 vCPU, 4 GB, 50 GB).
4. Register an **SSH key**. Key sign-in is safer than password-only sign-in. Follow the provider's instructions to make a key, and keep the private key file only on your own computer.
5. Once the server exists, attach a **static public IP**. If the IP changes, the domain's A records must be fixed again.
6. In the **firewall (security group)**, open only these ports:

   | Port | Use | Open to |
   |---|---|---|
   | 22 | SSH (server access) | Only your own computer's IP if you can |
   | 80 | HTTP (certificate issuance and redirects) | Everywhere (the internet) |
   | 443 | HTTPS (customer app, console, website) | Everywhere (the internet) |

   The installer can also turn on the `ufw` firewall, which then opens only 22, 80 and 443 ([install.sh](../../deploy/install.sh) `firewall_setup`). Ports that Docker opens bypass `ufw`, though, so **keep the cloud firewall rules as well** ([DEPLOY.md](../DEPLOY.md) section 1, item 2).
7. Note the server's **public IP**. For example `203.0.113.10`.
8. Check access from your computer: `ssh <user>@<public IP>`. The provider tells you the Ubuntu image's default user name (usually `ubuntu`).

### Provider notes (confirmed only)

- **AWS Lightsail:** create a **static IP** and attach it to the instance. Allow 22, 80 and 443 in the firewall rules.
- **AWS EC2:** attach an **Elastic IP**, and add 22, 80 and 443 to the **security group**'s inbound rules.
- **Naver Cloud Server:** attach a public IP and add 22, 80 and 443 to the server's **access control (ACG)** rules.
- **Vultr, DigitalOcean:** create a **Reserved IP** and attach it to the server. Set the firewall in the provider's firewall menu.

---

## 4. Domain and DNS

### 4-1. Add two A records

On your domain's management screen (the registrar or Cloudflare), add these two rows.

| Type | Name (host) | Value (target) | Notes |
|---|---|---|---|
| A | The root domain (usually `@`, or leave the field empty) | The server's public IP (for example `203.0.113.10`) | The `myshop.example` address |
| A | `www` | The server's public IP | The `www.myshop.example` address. The server redirects it to `https://myshop.example` |

- How the name field is written differs per registrar: the root domain may be `@`, an empty field, or the domain name itself.
- Without the `www` record the install and HTTPS still work. With it, the `www` address goes to the same site ([Caddyfile](../../deploy/Caddyfile) lines 175-178).
- Use the **root domain** such as `myshop.example`. If you type `https://` or a path at the installer's question, the installer strips it.

### 4-2. Wait and check

- After saving, it usually takes a few minutes to take effect, sometimes a few hours.
- Check from your computer (Windows and Mac both work):

  ```
  nslookup myshop.example
  nslookup www.myshop.example
  ```

  When the "Address" line shows **the server's public IP**, the domain is connected.
- The installer checks the same way. If the domain does not point at the server yet, it shows `A record myshop.example → server IP` and checks again when you press Enter ([install.sh](../../deploy/install.sh) `dns_check`).
- If an **AAAA (IPv6) record** exists, delete it, even if it points at this server (IPv6 visitors are not served yet). The installer checks this record too ([install.sh](../../deploy/install.sh) `dns_check`).

### 4-3. Turn off Cloudflare's proxy (orange cloud)

- When you create an A record in Cloudflare, the **proxy (orange cloud)** is on by default. Then the domain answers with Cloudflare's IP, and the installer's "does it point at the server" check fails.
- The installer has no proxy setting, and only two HTTPS modes (table below). So leave the record as **DNS only (grey cloud)**.
- For Caddy to get a certificate, the internet must reach this server directly on ports 80 and 443.
- Cloudflare R2 for backups is separate from the domain and works as is (section 6).

### 4-4. HTTPS mode (TLS_MODE)

| Setting | Meaning | Use |
|---|---|---|
| Empty (default) | Gets and renews a public certificate automatically (Caddy) | **Real operation** |
| `MALKKEUM_TLS=internal` | Uses a certificate from an internal authority on the server. Browsers show a warning | Installer tests only. Do not use it for a real install |

The installer accepts only these two values ([install.sh](../../deploy/install.sh), [Caddyfile](../../deploy/Caddyfile) lines 16-19).

---

## 5. Running the installer

### 5-1. One command

> **For developers and installers.** Owners should use the **easiest way** (no terminal) in [guide 0](00-install.md).

In the server's terminal (the browser SSH button or a terminal on your computer), paste this line and press Enter:

```
curl -fsSL INSTALL_URL | sudo bash
```

- `main` is the stable channel. The beta channel uses the `beta/install.sh` address ([install.sh](../../deploy/install.sh) lines 3-5).
- If your developer gives you an address for a specific version, use that.

### 5-2. The installer's questions

The installer's messages are in Korean today; [guide 0](00-install.md) gives the English meaning of each one.

| Question | Answer | Notes |
|---|---|---|
| Service address (domain) | `myshop.example` | Asked up to three times if the format is wrong |
| Email for certificate notices (Enter to skip) | `owner@myshop.example` | May be empty |
| Turn on the firewall (ufw) and open only ports 22, 80 and 443? (y/N) | `y` recommended | The default is `N`. It adds a second layer even if you already use the cloud firewall |
| Turn off SSH password sign-in? (y/N) | `y` once key sign-in works | Asked only when the account has an SSH key. Turning it off without a key locks you out. The default is `N` ([install.sh](../../deploy/install.sh) `ssh_keyonly_check`) |

- If the domain is not connected yet, the installer explains and waits for Enter. Type `s` to skip: the install continues, and the HTTPS certificate arrives by itself once the domain is connected.

### 5-3. What the installer does

- Checks the server (CPU architecture, memory, disk, whether another program uses ports 80 and 443)
- Installs the required packages and Docker
- Creates the keys (`/opt/malkkeum/.env`)
- Starts the database, API and web server
- A backup every day at 03:30
- An automatic app update every Monday at 04:00 (backs up first, and rolls back if the new version does not answer; to change or turn it off, see 8-1)
- Automatic security updates and fail2ban (blocks repeated SSH sign-in failures)
- The `dahae` management command

A new server can take 10 to 20 minutes.

**If a check stops you:** if it says ports 80/443 are used by another program, stop that program and run the installer again. The example in the message is `sudo systemctl disable --now nginx apache2`. Running the installer again is safe.

### 5-4. The finish screen

When all goes well, the installer ends with:

- the console address `https://<domain>/admin/`,
- a 12-character **setup code**, shown with hyphens (for example `ABCD-EFGH-JKMN`),
- a reminder to connect payments, messaging and sign-in in **[Setup]**,
- a warning to make the secret key copy and keep it off the server: `sudo dahae keys-backup`.

If the domain is not connected yet, it says the app is fine but the HTTPS certificate is not there yet. Check again a few minutes later with `sudo dahae status`.

The **setup code** is valid for **7 days** after it is first shown. After that, ask the developer or installer who helped you for a new one (they run `sudo dahae setup-code --renew` on the server; [install.sh](../../deploy/install.sh) `cmd_setup_code`).

### 5-5. Installing without questions (MALKKEUM_ANSWERS)

When you install the same way many times, or answering is hard, put the answers in a file.

1. Write these two lines in a file on the server, such as `/root/dahae-answers.txt`:

   ```
   DOMAIN=myshop.example
   EMAIL=owner@myshop.example
   ```

2. Run the install command like this:

   ```
   curl -fsSL INSTALL_URL | sudo env MALKKEUM_ANSWERS=/root/dahae-answers.txt bash
   ```

- The file must be a regular file (not a link). Values cannot contain spaces, backslashes or `=`. Only the `DOMAIN` and `EMAIL` lines are read ([install.sh](../../deploy/install.sh) `load_answers`).
- If the `EMAIL=` line is empty and there is a terminal, the installer asks for the email again.
- Without questions, the firewall stays off by default. To turn it on, add `MALKKEUM_FIREWALL=on` (`sudo env MALKKEUM_FIREWALL=on MALKKEUM_ANSWERS=... bash`). SSH password sign-in is never turned off without a question.
- If the domain is not connected yet, the install finishes without waiting.

### 5-6. Owner account and two-step sign-in

1. Open `https://<domain>/admin/` in a browser.
2. Enter the setup code, your name, email and a password (12 characters or more).
3. After signing in, the **two-step sign-in setup** screen opens. Add the key to your authenticator app and enter the 6-digit number it shows.
   Then **recovery codes** appear. Write them on paper, keep them somewhere safe and finish saving as the screen says. Owner features need this step.
4. Go to **[Setup]** in the left menu and follow section 7.

When you are done, run `sudo dahae status` and check that the app and HTTPS are healthy. Run every command with `sudo`.

---

## 6. Database and backups

### 6-1. Where the data is

| What | Where | Notes |
|---|---|---|
| Database (PostgreSQL 17) | The `db` container on the server, Docker volume `dbdata` | Not open to the internet; no outside port is needed ([docker-compose.yml](../../deploy/docker-compose.yml)) |
| Photos and other uploads | Docker volume `uploads` | Photos are included in the backups |
| Backup files | `/opt/malkkeum/backups/` on the server | Database dumps 14 days; photo archives: once a day's photos reach the offsite backup, archives older than 3 days are removed, otherwise they stay 14 days |
| Keys and settings | `/opt/malkkeum/.env` on the server | Its `DATA_KEY` is the key that opens the backups |

### 6-2. What is in a backup

- **Database backup** (`db-<date>.sql.gz.enc`): every day at 03:30 and right before an update ([install.sh](../../deploy/install.sh) scheduled jobs).
- **Photo backup** (`uploads-<date>.tar.gz.enc`): made at the same time.
- Both are **encrypted with AES-256-GCM**; no plain backup stays on the server disk. If encryption fails, the plain file is kept with a warning, and updates, restores and rollbacks stop until it is fixed ([backup.rs](../../backend/crates/api/src/backup.rs), [DEPLOY.md](../DEPLOY.md) "Backups, RPO and RTO").
- **On the server:** database dumps are removed after 14 days (`BACKUP_KEEP_DAYS=14`). Photo archives: once a day's photos reach the offsite backup, archives older than 3 days are removed (`PHOTO_KEEP_DAYS_OFFSITE=3`), otherwise after 14 days ([install.sh](../../deploy/install.sh) `cmd_backup`).
- **Offsite backups**, once set up in the console, are removed after their retention period (30 days by default).
- At worst you can lose **one day of data**. A backup only on the server disk is lost with the server, so connect an offsite backup.

### 6-3. Back up now, on the server

```
sudo dahae backup
```

A new `.enc` file appears in `backups/`, and it is uploaded to the offsite storage if one is set. If the upload fails, the backup on the server stays.

### 6-4. Connecting an offsite backup (the console's "Offsite backup (S3-compatible)" card)

Any S3-compatible storage works, for example **Cloudflare R2**, **Naver Cloud Object Storage** or **AWS S3**.

1. In the storage service, create one **private bucket** (for example `dahae-backup`).
2. Create access keys.
   - R2: create an API token with **Object Read & Write** permission.
   - Naver Cloud: create an access key under **Authentication key management** on My Page.
   - AWS S3: create keys for an IAM user dedicated to that bucket.
3. In the console's left menu, open **[Setup]** → the **Offsite backup (S3-compatible)** card.
4. Fill in the fields and **save**.

| Field | Stored as | Example | Required | Rule |
|---|---|---|---|---|
| Endpoint | `endpoint` | R2: `https://<account-id>.r2.cloudflarestorage.com` · Naver Cloud: `https://kr.object.ncloudstorage.com` · AWS Seoul: `https://s3.ap-northeast-2.amazonaws.com` | Yes | Starts with `https://` and is a public address |
| Region | `region` | R2: `auto` · Naver Cloud: `kr-standard` · AWS Seoul: `ap-northeast-2` | Yes | Letters, digits and hyphens only |
| Bucket name | `bucket` | `dahae-backup` | Yes | Lower-case letters, digits, `-` and `.`, 3 to 63 characters |
| Access Key | `access_key` | the key ID | Yes | |
| Secret Key | `secret_key` | the secret | Yes | Encrypted when saved. The provider shows it once, so copy it |
| Keep for (days) | `retention_days` | `30` | No | Default 30, 1 to 3650 |

- The stored names match the backup field definitions in [integrations.rs](../../backend/crates/api/src/integrations.rs); the field help is in [setup_guides.dart](../../app/apps/console/lib/screens/setup_guides.dart).
- AWS S3 is confirmed only through the example address in this code's connection test. Pass the connection test before relying on it, and check with your developer if it fails.
- Saving does not run a connection check, and it locks the card. Press **[Unlock (owner code)]**, enter the owner's 6-digit two-step code, then press **[Check connection]**. It uploads a small test file (`malkkeum/connection-test.txt`) and deletes it right away. A green **Connected** means success. Then press **[Lock now]**.
- If a value is wrong, the reason appears next to the field: endpoint format, bucket name, keys that do not match.

### 6-5. Secret key copy (`keys-backup`): do this

The offsite backups are encrypted with the server's `DATA_KEY`. If you lose the server, you lose `DATA_KEY` with it, and the offsite backups cannot be opened. So keep a copy of the key off the server.

- **In the console:** the owner opens **System status › Server update › Recovery › Download the secret key copy** (owner code, one download), then presses **Kept off the server**, which records that the copy is kept.
- **On the server:** your developer runs `sudo dahae keys-backup` and sets a lock password (12 characters or more).

1. Keep the file (`keys-<date>.env.enc`) on a USB stick or in a password manager. Do not paste it into a chat app or an email body.
2. Write the lock password down off the server (in a password manager or similar).
3. If the keys in `.env` change (for example when new keys are made), make and store a new key copy ([DEPLOY.md](../DEPLOY.md), `DATA_KEY` and `.env`).

You can also copy the contents with `sudo cat /opt/malkkeum/.env` right after the install ([guide 0](00-install.md), "keep a copy of the security key file").

### 6-6. Restore practice (regular checks)

A restore **turns the current data back to the time of the backup**. Practising on the live server changes real data, so the check has two parts.

**Monthly check (safe on the live server)**

1. In the console's [Setup], press **[Check connection]** on the offsite backup card. If the card is locked, unlock it first (section 7).
2. Run `sudo dahae backup` once.
3. Check that the file you just made appears in the offsite list:

   ```
   sudo dahae restore-offsite
   ```

   Without a name, it only shows the list.

**Real restore practice (on a separate test server)**

- Install a test server the same way, move the `keys-backup` file to it and restore in the order below (the same as "Moving to a new server" in section 8).
- To restore directly on the live server, use these commands. They ask you to type `yes`, and back up the current state first ([guide 7](07-operations.md), section 4).

  ```
  sudo dahae restore backups/db-20261110-033000.sql.gz.enc
  sudo dahae restore-offsite <name>
  ```

- File names on the server look like `db-<date>.sql.gz.enc`, and `restore` accepts only files inside the `backups/` folder.

### 6-7. WAL archiving (developer setting)

WAL archiving (point-in-time recovery, about every 5 minutes) is a setting your developer turns on separately on the server; the owner cannot turn it on from the console or a command. Without it, the recovery point you can count on is **one day** ([DEPLOY.md](../DEPLOY.md) "Backups, RPO and RTO", [RUNBOOK.md](../RUNBOOK.md#english) recovery point).

---

## 7. Connecting your services in the console

Press **[Setup]** in the console's left menu to open the **Get ready to launch** screen, with a done/total progress count at the top. The cards below are those of the Korea pack, in this order.

| Card | What it connects | Details |
|---|---|---|
| Business details | Service name, brand colour, logo, business information, customer centre | [Guide 2](02-setup-wizard.md), section 1 |
| Payments (PortOne) | Card and easy-pay keys | [Payment setup](payments-setup.md) |
| Partner payouts | Toss Payments payout keys (optional) | [Payment setup](payments-setup.md), [guide 2](02-setup-wizard.md) |
| SMS and AlimTalk (Solapi) | SMS sending keys, KakaoTalk AlimTalk | [Guide 2](02-setup-wizard.md), section 3; [guide 6](06-알림톡-템플릿-등록.md) (Korean) |
| App push (Firebase) | Push settings | [Guide 2](02-setup-wizard.md), section 4 |
| Kakao sign-in | Kakao sign-in keys | [Guide 2](02-setup-wizard.md), section 5 |
| Naver sign-in | Naver sign-in keys | [Guide 2](02-setup-wizard.md), section 5 |
| Google sign-in | Google sign-in keys | [Guide 2](02-setup-wizard.md), section 5 |
| Apple sign-in | Apple sign-in keys | [Guide 2](02-setup-wizard.md), section 5 |
| AI support helper | AI keys | Only when this card appears on the Setup screen. Check with your developer |
| Address search (road-name address) | Address search key | [Guide 2](02-setup-wizard.md) |
| Offsite backup (S3-compatible) | The backup storage from section 6 | Section 6 of this guide |

- Paste the values into a card and **save**. Saved values are encrypted. Saving alone does not run a connection check.
- Saving locks the card. To change it, press **[Unlock (owner code)]**; when done, press **[Lock now]**. Only the owner can unlock.
- On a locked card the connection check button is disabled. After unlocking, press **[Check connection]** (on the payment card, **[Check API key]**). A green **Connected** means success.
- The first save of the Payments (PortOne) card, and a save or clear of the Partner payouts card in Toss mode, asks for the owner's 6-digit two-step code.
- The Payments (PortOne) card shows `<your domain>/v1/webhooks/portone`, and the Partner payouts card shows `<your domain>/v1/webhooks/toss`. Paste them into the PortOne console and the Toss developer console respectively. The payout card's field label says to paste into the PortOne console, but that address is for Toss. How to check is in [Payment setup](payments-setup.md), section 3.
- The app link that brings users back to the app after Naver sign-in is set with your developer through `dahae applinks` ([guide 7](07-operations.md), section 9).
- Outside uptime monitoring (an alert when the server goes down) is set with `sudo dahae heartbeat https://...` ([guide 7](07-operations.md), section 8).

---

## 8. Changing the server or domain later

### 8-1. Updating while keeping the same domain (supported)

The owner updates from the console's **Server update** card. When the developer **runs the install command again** with the same domain, it updates to the latest version and keeps settings and data. It backs up first and rolls back if the new version does not answer ([install.sh](../../deploy/install.sh) header). The developer changes the automatic update day and time with `sudo dahae update-schedule mon@04:00` (`off` turns it off).

### 8-2. Moving to a new server (supported)

This follows the procedure in the developer documentation ([DEPLOY.md](../DEPLOY.md) "Backups, RPO and RTO", about one hour).

1. Create the new server as in sections 3 to 5 and install it. You can wait for the domain (section 4) or connect it later.
2. On the **old server**, run `sudo dahae backup` for a fresh backup, and `sudo dahae keys-backup` for a key copy (6-5).
3. Copy the key file to **your computer** (as in 6-5), then send it to the **new server** and move it into `/opt/malkkeum/backups/`:

   ```
   scp keys-<date>.env.enc <user>@<new server IP>:~/
   sudo mv ~/keys-<date>.env.enc /opt/malkkeum/backups/
   ```

4. On the **new server**, load the key copy (paths are relative to `/opt/malkkeum`):

   ```
   sudo dahae keys-restore backups/keys-<date>.env.enc
   ```

   Enter the lock password, then type `yes` at the confirmation.
5. In the new server's console (`https://<domain>/admin/`), enter the **Offsite backup (S3-compatible)** card again and save. The new database is empty, so this card must be filled before the offsite list appears.
6. On the new server, list and restore:

   ```
   sudo dahae restore-offsite
   sudo dahae restore-offsite db-<date>.sql.gz.enc
   ```

   Type `yes` at the confirmation.

   After the restore, sign in with **the old server's owner account** (email, password, two-step sign-in app). An account made on the new server is replaced by the restore.
7. Point the domain's A records at **the new server's IP**. A few minutes later, check HTTPS with `sudo dahae status`.
8. Turn the old server off only after the new one is confirmed healthy. `sudo dahae uninstall` stops the services and keeps the data; type `yes` at the confirmation.

> `sudo dahae uninstall --purge` also deletes the data and backups. It runs only after you type the domain, and `--yes` does not skip that. Do not run it before the new server is confirmed healthy ([install.sh](../../deploy/install.sh) `cmd_uninstall`).

### 8-3. Changing the domain (console)

The owner changes it in the console under **Setup › Domain**; no terminal is needed ([owner's guide](04-owner-guide.md#changing-the-domain)).

1. Point the new domain's A record (and `www.<new domain>`) at this server's IP (section 4). **Do not delete the old domain's A record.**
2. In the console, enter the new domain and go through the DNS check, the change button, the owner's authenticator code and the server approval code.
3. The site pauses for about 30 seconds and comes back on the new address. If the new address does not answer within 5 minutes, the change is undone and the reason is shown.
4. The old domain keeps answering. Installed apps keep working on the old address; only website pages move to the new one.
5. Follow the on-screen checklist to update outside services (webhooks, payment gateway site addresses, an added Naver callback, search registrations, store addresses, the GitHub `store` variables), and upload a new store build.
6. Retire the old domain only after the new apps are in the stores and the minimum app version has been raised.

> **Servers installed before this feature need the install line run once again.** The server job runner and the `dahae` command are not replaced by `dahae update` on those versions. Running the install line (5-1) again with the same domain installs the new runner and keeps the data; until then, a domain change ends as "refused". After a domain move, run it without `MALKKEUM_DOMAIN` (it uses the domain in `.env`); with it, the server moves back to that domain.

---

## 9. Checklist

- [ ] Cloud account, domain, certificate email and authenticator app are ready.
- [ ] Ubuntu 24.04 server created in your region (recommended 2 vCPU, 4 GB, 50 GB).
- [ ] Static public IP attached.
- [ ] Only 22, 80 and 443 open in the cloud firewall.
- [ ] SSH key sign-in works.
- [ ] Two A records, root and `www`; proxy (orange cloud) off.
- [ ] `nslookup` matches the server's public IP.
- [ ] Install command run and questions answered.
- [ ] Setup code (12 characters) used within 7 days.
- [ ] Owner account made at `https://<domain>/admin/` and two-step sign-in set up.
- [ ] `sudo dahae status` shows the app and HTTPS healthy.
- [ ] Secret key copy made and kept off the server.
- [ ] Offsite backup card filled, and [Check connection] shows a green "Connected".
- [ ] After `sudo dahae backup`, the file appears in the `sudo dahae restore-offsite` list.
- [ ] Payment, messaging and sign-in cards connected ([guide 2](02-setup-wizard.md)).
- [ ] Outside uptime monitoring connected with `sudo dahae heartbeat` ([guide 7](07-operations.md), section 8).
- [ ] A date set for the restore practice in this guide.

## 10. If something goes wrong

| Symptom | First step |
|---|---|
| The install stops partway | Read the message on screen and see "If something goes wrong" in [1. Server setup in detail](01-server-setup.md). Running it again is safe |
| It says the domain does not point at the server | Check section 4 again, including whether the proxy is on |
| The browser says the connection is not secure | [RUNBOOK.md](../RUNBOOK.md#english), section 5 (certificate problems). `sudo dahae status`, `sudo dahae logs web` |
| The site does not open | `sudo dahae status`, `sudo dahae logs` ([RUNBOOK.md](../RUNBOOK.md#english), section 1) |
| Something is wrong after an update | Tell your developer (the developer rolls back; [guide 7](07-operations.md), section 6) |
| Payments do not connect | [Payment setup](payments-setup.md), section 3; [RUNBOOK.md](../RUNBOOK.md#english), sections 3 and 4 |
| Data must be restored | [RUNBOOK.md](../RUNBOOK.md#english), section 9; section 6-6 of this guide |
| The setup code is lost | Tell your developer or installer (after 7 days you get a new code) |

- `sudo dahae help` lists every command. More on operations: [7. Operations commands](07-operations.md), the [operations guide](../OPERATIONS.md#english) and the [incident runbook](../RUNBOOK.md#english).
- `sudo dahae logs` keeps moving: it follows the log, it is not broken. Press Ctrl+C to stop it.
- If you cannot solve it, copy the `sudo dahae logs` screen and send it to your developer as is.
