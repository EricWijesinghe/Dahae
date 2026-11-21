# 05. Connect the domain and check

## In this chapter

Add **A records** so that typing your domain (for example `example.co.kr`) leads to the server from chapter 4. Then check in the browser and on the server screen that the install is finished.

## Before you start

- The server's **`PublicIp`** from chapter 4 (for example `3.38.12.34`)
- The sign-in for wherever the domain is managed (Cloudflare, Gabia, Whois, Cafe24, Route 53 and so on)
- 10 minutes + waiting time for the change to spread (a few minutes to an hour)

## First: which records to add

How many records you add depends on the shape of the domain.

| Your domain looks like | A records to add |
|---|---|
| No prefix (e.g. `example.co.kr`) | Two: name `@` and name `www`, both with the value `PublicIp` |
| With a prefix (e.g. `app.example.co.kr`) | One: name `app`, value `PublicIp` |

- Visitors who come to `www` are moved to `https://example.co.kr` by the server. Without a `www` record, install and HTTPS still work.
- For the bare domain, providers use `@`, an empty box, or the domain name itself in the name box.

## 5.1 If you use Route 53

If you added `--hosted-zone-id` to the chapter 4 command, the A record for `example.co.kr` (the domain you gave in chapter 4) **was created for you**.

1. Type **Route 53** in the AWS search box and open it.
2. Click **Hosted zones**, then your domain.
3. Check the A record for the chapter 4 domain points to `PublicIp`.
4. If the domain has no prefix, click **Create record** and add a `www` A record with the same IP.

**You should now see:** an A record in the list whose value equals `PublicIp`.

If you did not add `--hosted-zone-id`, add the records yourself as in 5.2 or 5.3.

## 5.2 If you use Cloudflare

1. Sign in at https://dash.cloudflare.com.
2. Click your domain.
3. In the left menu, click **DNS** > **Records**.
4. Click **Add record**.
5. In **Type**, choose `A`.
6. In **Name**, type `@` (for a domain with a prefix, the prefix such as `app`).
7. In **IPv4 address**, paste `PublicIp`.
8. Click the **Proxy status** switch so it shows a **grey cloud (DNS only)**. Orange is wrong.
9. Leave **TTL** at `Auto`.
10. Click **Save**.
11. For a domain without a prefix, repeat steps 4–10 with `www` in **Name**.

![Cloudflare add A record: DNS only (grey cloud)](../img/en/cloudflare-dns-a-record.png)

**You should now see:** the A records in the Records list, with **DNS only** (grey cloud) in the Proxy status column.

> **Warning** With the **orange cloud (Proxied)** on, the server cannot get its HTTPS certificate. The Dahae server gets its own certificate and takes customer connections directly. Always keep it on **grey cloud (DNS only)**.

## 5.3 If you use another registrar (Gabia, Whois, Cafe24 and others)

Menu names differ by provider. Look for **"DNS 관리"** (DNS management) or **"DNS 설정"** (DNS settings).

1. Sign in to the site where you bought the domain.
2. Open the **DNS management** screen of your domain.
3. Click the button to add a record.
4. For the type, choose **A**.
5. In host (name), type `@` (or leave it empty; for a domain with a prefix, the prefix such as `app`).
6. In value (IP address), paste `PublicIp`.
7. Leave TTL at `300` or the default.
8. Save.
9. For a domain without a prefix, add one more record with host `www` and the same IP.

> **Warning** If there is an **AAAA record (IPv6)** with the same name pointing somewhere else, delete it. If it stays, getting the certificate can fail.

## 5.4 Wait and check

DNS changes usually spread in a few minutes, sometimes up to an hour. The install inside the server carries on meanwhile (4.5).

### Check in the browser

1. Wait 10–30 minutes.
2. In the browser address bar, type `https://example.co.kr/health` (with your domain).
3. Press Enter.

![The browser showing /health](../img/ko/browser-health-ok.png)

**You should now see:** text starting with `{"ok":true` (a version number follows).

4. Now type `https://example.co.kr/admin/` in the address bar.

**You should now see:** the **Welcome! Create the owner account** screen. The install is finished. Go to chapter 6.

### See whether DNS has spread (optional)

1. Open https://dnschecker.org.
2. Type your domain, choose type **A**, and search.
3. If the results show `PublicIp`, it has spread.

Or open Command Prompt (Windows) or Terminal (Mac) and type `nslookup example.co.kr`. If the address in the result equals `PublicIp`, it is connected.

## 5.5 See the server's status on the web (Session Manager)

If `/health` still does not open after 30 minutes, look inside the server. You can do this in the AWS web console, without an SSH program.

1. Type **EC2** in the AWS search box and open it.
2. On the left, click **Instances**.
3. Tick the box to the left of the server named `dahae-dahae-main`.
4. Click the **Connect** button at the top.
5. Click the **Session Manager** tab.
6. Click **Connect**.

![EC2 Connect: Session Manager tab](../img/en/aws-ssm-connect.png)

7. A black window opens in a new tab. Paste the line below and press Enter.

   ```
   sudo dahae status
   ```

![Session Manager after sudo dahae status](../img/en/aws-ssm-status.png)

**You should now see:** lines marked `✔`, like these. (The server prints its messages in Korean. The English meaning is on the right.)

```
✔ 앱 정상                                       (app is healthy)
✔ HTTPS 정상: https://example.co.kr/admin/     (HTTPS is working)
```

Lines marked `!` are things still to do. Right after install, these lines showing `!` is normal. Later chapters fix them one by one.

| `!` line (Korean on screen) | Meaning | Fixed in |
|---|---|---|
| 외부 가동 확인이 없어요 | No outside uptime check | Chapter 17 (heartbeat) |
| 최근 외부 백업이 없어요 / 최근 사진 파일 외부 백업이 없어요 | No recent offsite backup / no recent photo offsite backup | Chapter 17 (offsite backup) |
| 비밀 키 사본이 없어요 / 서버 밖에 보관했다는 확인이 없어요 | No key copy / no confirmation that it is kept off the server | 6.7 (key copy) |

8. When done, click **Terminate** at the top right to close the window.

> **Tip** If the **Connect** button is grey or the connection fails, check the instance state is **Running**, and try again in 2–3 minutes.

## If something goes wrong

| If this happens | Do this |
|---|---|
| The browser says "Not secure" or "Your connection is not private" | DNS has not spread yet, so there is no certificate. Open it again in 10–30 minutes |
| `/health` does not open | Check the A record value equals `PublicIp`, and on Cloudflare that the cloud is grey. Then check the status as in 5.5 |
| `sudo dahae status` says "HTTPS 인증서가 아직 없어요" (no HTTPS certificate yet) | Once the A record points to this server, the certificate arrives within minutes. Check the A record again |
| `sudo dahae status` says "앱 응답 없음" (app not responding) | The install may still be running. Look again in 10 minutes. If it stays, take a screenshot of `sudo dahae logs` and send it to the developer (Ctrl+C stops it) |
| `dahae: command not found` | The install inside the server is still at the start. Try again in 10 minutes |
| `/admin/` shows a sign-in screen instead of the owner account screen | Someone already created the owner account. Check with the developer |
