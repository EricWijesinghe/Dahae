# 9. Install by cloud provider (following the provider's screens)

English · [한국어](09-클라우드별-설치.md)

Every server company names its screens differently. This guide lists, provider by provider, **which field to paste into**, and **where to open the firewall and attach a static IP**. What you paste is always the same: the text made on the install page ([guide 0](00-install.md), step 2).

> Providers rename their screens often. If a name differs, look for the **closest name**. If you cannot find it, ask the provider's support desk.

## Before you start (every provider)

1. Choose **Ubuntu 24.04 LTS** as the operating system. Amazon Linux, Oracle Linux and CentOS are not supported.
2. **Pick the format** on the install page that fits the provider:
   - `#cloud-config`: for a *User data* or *cloud-init* field
   - `#!/bin/bash`: for a *Launch script*, *Startup script* or *Init script* field
   - If the field's example starts with `#!/bin/bash`, it is a **startup script** field. Where a field needs a first line of `#cloud-config`, use the `#cloud-config` format.
3. **Firewall:** open **22, 80 and 443** in the provider's firewall (security group, firewall rules). 80 and 443 are needed to get the HTTPS certificate.
4. **Static IP:** attach a static IP to the server if you can. If the IP changes when the server restarts, the domain's A records must change too.
5. After the server is created, note its **public IP** and, on the DNS screen from [guide 0](00-install.md) step 2, point the A records `@` and `www` at it.

> Paste the install text into the server-creation screen once. Some providers (DigitalOcean, for example) do not let you edit the field later. If you made a mistake, delete the server and create a new one.

## Providers at a glance

| Provider | Field to paste into | Install text format | How sure we are |
|---|---|---|---|
| AWS Lightsail | Launch script | `#!/bin/bash` | Medium (check the screen name) |
| AWS EC2 | Advanced details › User data | `#cloud-config` | High (field name medium) |
| Google Compute Engine | Management › Startup script | `#!/bin/bash` | Medium |
| Microsoft Azure | Advanced › Custom data | `#cloud-config` | Medium |
| Oracle Cloud (OCI) | Show advanced options › Paste cloud-init script | `#cloud-config` | Medium |
| DigitalOcean | Advanced options › User data | `#cloud-config` | Medium |
| Vultr | Additional Features › User data | `#cloud-config` | Medium |
| Hetzner | Cloud config (Create Server) | `#cloud-config` | Medium |
| Linode (Akamai) | Add User Data | `#cloud-config` | Low to medium (check the screen name) |
| Naver Cloud (Korea) | Server › Init Script | `#!/bin/bash` | Medium |
| NHN Cloud (Korea) | User script | To be confirmed | Low (the screen name may differ) |
| Kakao Cloud, KT Cloud, Gabia, Cafe24 (Korea) | No confirmed method yet | - | Ask the provider's support desk and use the one-line browser SSH method below |

## Step by step

Pick the region closest to your customers; for a Korean business that is Seoul.

### AWS Lightsail · Launch script
1. In Lightsail, **Create instance** → your region → platform **Linux/Unix**.
2. Blueprint: **OS Only** › **Ubuntu 24.04**.
3. Paste the `#!/bin/bash` format into the **Launch script** field below. (The screen name may differ.)
4. Create a **static IP** and attach it to the instance.
5. On the **Networking** tab, open **80 and 443** (22 is open by default).
6. **Create instance**. Note the public IP.

### AWS EC2 · User data
1. EC2 › **Launch instance** → name → AMI **Ubuntu Server 24.04 LTS** (64-bit x86, or arm64). The default Amazon Linux does not work.
2. Choose the instance type and a key pair. If you choose an arm64 type such as **t4g (Graviton)**, choose the arm64 AMI too.
3. **Edit network settings** › add **SSH 22, HTTP 80 and HTTPS 443** to the security group.
4. Expand **Advanced details** and paste the `#cloud-config` format into the **User data** field at the bottom.
5. **Launch instance**. Note the public IP. (For a static IP, attach an **Elastic IP**.)

> User data may be up to 16 KB; the install text is much smaller. The console encodes what you paste automatically.

For an AWS server with automatic deploys from GitHub, use [guide 10](10-aws-deploy.md) instead.

### Google Compute Engine · Startup script
1. Compute Engine › **VM instances** › **Create instance**.
2. Region of your choice (for Korea, **asia-northeast3 (Seoul)**). Boot disk **Ubuntu 24.04**.
3. Under **Firewall**, tick **Allow HTTP traffic** and **Allow HTTPS traffic**.
4. Paste the `#!/bin/bash` format into **Management** › **Automation** › **Startup script**.
5. **Create**. Note the external IP. You can make it static under **IP addresses**.

> A startup script runs **every time the server boots**. Clear the field once the install has finished; otherwise the installer checks for updates again on every restart.

### Microsoft Azure · Custom data
1. Create a virtual machine → region of your choice (for Korea, **Korea Central** or **Korea South**).
2. Image **Ubuntu Server 24.04 LTS**.
3. Under **Inbound ports**, select **22, 80 and 443**.
4. **Advanced** tab › paste the `#cloud-config` format into **Custom data**.
5. **Review + create**. Note the public IP. (The screen name may differ.)

> There may be no browser SSH. If something goes wrong, look for Azure's **Serial console**. (To be confirmed)

### Oracle Cloud (OCI) · cloud-init script
1. Compute › **Create instance**. Choose your region (for Korea, **South Korea Central (Chuncheon)** or **Seoul**; check on screen).
2. Image **Canonical Ubuntu 24.04**. An **x86** shape is recommended; there are reports that the free Arm shape is not available in the Korean regions. (To be confirmed)
3. Paste the `#cloud-config` format into **Show advanced options** › **Paste cloud-init script**.
4. Add ingress rules for **80 and 443** to the **VCN security list** (or a network security group). 22 is usually open.
5. **Create**. Note the public IP.

### DigitalOcean · User data
1. Droplets › **Create**. Choose a region from the list. (There is no Korean region. To be confirmed)
2. Image **Ubuntu 24.04 (LTS)**.
3. Paste the `#cloud-config` format into **Advanced options** › **User data**.
4. If you create a **firewall** (optional), open **22, 80 and 443**.
5. **Create**. Note the IP. User data cannot be changed after creation.

### Vultr · User data
1. **Deploy Instance** › choose a location (choose Seoul if it is listed and your customers are in Korea; to be confirmed).
2. **Ubuntu 24.04 x64**.
3. Paste the `#cloud-config` format into **Additional Features** › **User data**.
4. In a firewall group (optional), open **22, 80 and 443**.
5. **Deploy**. Open the console with **View Console**.

> User data runs only on the first boot.

### Hetzner · Cloud config
1. Cloud Console › your project › **Create Server**.
2. Choose a location, and the image **Ubuntu 24.04**.
3. Choose type **CX or larger**. (To be confirmed)
4. Paste the text starting with `#cloud-config` into **Cloud config**. The first line must be `#cloud-config`.
5. Open **22, 80 and 443** under **Firewall**, then **Create**.

### Linode (Akamai) · Add User Data
1. **Create Linode** › **Ubuntu 24.04 LTS**.
2. Choose a region. (To be confirmed)
3. Paste the `#cloud-config` format into **Add User Data**. (The screen name may differ.)
4. Open **22, 80 and 443** under **Firewall**, then **Create**.

### Korean providers

#### Naver Cloud · Init Script
1. Console › **Compute** › **Server** › **Create server**.
2. Image **Ubuntu 24.04**.
3. Allow **22, 80 and 443** in the **ACG** (access control group).
4. Choose or create an **Init Script** and paste the `#!/bin/bash` format. (To be confirmed)
5. **Create server**. Note the public IP.

#### NHN Cloud · User script
- The field name and format are not confirmed yet. **The screen name may differ.** If you cannot find it, use the one-line browser SSH method below.

#### Kakao Cloud · KT Cloud · Gabia · Cafe24
- We have not confirmed a user-data field for these providers yet. Ask the provider's support desk: **"When I create a server, is there a field for user data (cloud-init) or a startup script?"**
- If there is none, use the **one-line browser SSH** method.

## When there is no field: one line in browser SSH

After the server is created, open the provider's **SSH in browser** (or console access) and paste, once, the line the install page made in the **One browser-SSH line** format. It already contains the setup code and the domain.

```
curl -fsSL -o /tmp/dahae-install.sh <install file address> && echo '<fingerprint>  /tmp/dahae-install.sh' | sha256sum -c - && sudo env MALKKEUM_DOMAIN=<domain> MALKKEUM_EMAIL=<email> MALKKEUM_SETUP_CODE=<setup code> MALKKEUM_FIREWALL=on bash /tmp/dahae-install.sh
```

> Do this after the server is running; it is one step more than pasting into a field. Paste the line exactly as the page made it. The angle-bracket (`<>`) parts are not for you to fill in: the page has already filled them.

## If something goes wrong

| Symptom | What to check |
|---|---|
| The server is on but `https://<domain>/health` does not open | Whether you waited 10 to 30 minutes, and whether the A record matches the server IP (check at https://dnschecker.org) |
| 80 and 443 are blocked | Whether 80 and 443 are open in the provider firewall (security group, firewall rules) |
| "Amazon Linux" or another operating system came up | Delete the server and create it again with **Ubuntu 24.04** |
| There is no field for the install text | Use the one-line browser SSH method above |
| The install failed | If you entered an **Install-complete alert address** on the install page, check that alert; otherwise run `sudo dahae logs` in the provider's console and send the screen to your developer |

`{"ok":true}` at `https://<domain>/health` means the install finished. Then continue with [guide 0](00-install.md), step 3 (creating the owner account).
