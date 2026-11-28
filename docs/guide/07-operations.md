# 7. Operations commands (`dahae`)

English · [한국어](07-운영-명령어.md)

These are the commands for looking after the server. The install puts them in place.

> **For developers and installers.** Owners update the server with the **Update now** button in the console's **Server update** card. A developer runs the other commands on the server.
> The command's messages are in Korean today; the English meaning of the important ones is given below.

## How to use them
1. On the provider's screen, press **"Connect with browser SSH"** to open the terminal.
2. Type the command and press Enter.
3. Always start with `sudo` (administrator rights are needed).

`sudo dahae help` lists every command. The old name `malkkeum` works the same way.

| What you want | Command |
|---|---|
| Check the server is healthy | `sudo dahae status` |
| Read the logs | `sudo dahae logs` |
| Back up now | `sudo dahae backup` |
| Restore a backup | `sudo dahae restore <file>` |
| Update now | `sudo dahae update` |
| Go back to the version before the update | `sudo dahae rollback` |
| Switch between stable and beta | `sudo dahae channel stable` |
| Get an alert when the server goes down | `sudo dahae heartbeat <url>` |
| Change the service address (domain) | Console Settings > Domain (terminal: `sudo dahae domain <new-domain>`) |
| Connect app links (when the developer asks) | `sudo dahae applinks …` |
| Remove the service | `sudo dahae uninstall` |

---

## 1. status: is the server healthy?
**When:** the site does not open, right after the install, or any time.
```
sudo dahae status
```
- `✔ 앱 정상` (app OK): the service is running.
- `✔ HTTPS 정상: https://domain/admin/` (HTTPS OK): the certificate is fine too.
- `! HTTPS 인증서가 아직 없어요` (no HTTPS certificate yet): check that the domain's A record points at this server. If it does, the certificate is issued within a few minutes.
- `! 외부 가동 확인이 없어요` (no outside uptime check): set up section 8 (heartbeat).

## 2. logs: read the logs
**When:** `status` says the app does not answer, or the developer asks for the logs.
```
sudo dahae logs
```
- Shows the last 200 lines and keeps following new ones. **Ctrl + C** leaves.
- Capture the screen and send it to the developer.
- Web server or database logs: `sudo dahae logs web`, `sudo dahae logs db`.

## 3. backup: back up now
**When:** before a big change (such as a bulk price edit). Normally it runs by itself every day at 03:30.
```
sudo dahae backup
```
- When done it prints `✔ 백업 완료: /opt/malkkeum/backups/db-<date>-<time>.sql.gz` (backup done).
- It saves the customer and booking data (`db-….sql.gz`) together with the photos (`uploads-….tar.gz`).
- Backups inside the server are kept for 14 days.
- With [Settings] → [Offsite backup] connected, a copy also goes off the server.
- List the backups: `sudo ls /opt/malkkeum/backups`

## 4. restore: go back to a backup
**When:** data was deleted by mistake or is damaged. **The current data goes back to the time of the backup**, so use it only when you must.
```
sudo ls /opt/malkkeum/backups
sudo dahae restore backups/db-20261110-033000.sql.gz
```
1. The first line lists the backups; pick the `db-….sql.gz` file of the date you want.
2. The second line asks whether to restore the current data to that file. Type `yes`.
3. It **backs up the current state first**. If you change your mind, restore that file.
4. If a photo backup from the same time exists, the photos are restored too.
5. `✔ 복원 완료` (restore done) means it finished.

- If an error happens part-way, the data is left unchanged.
- To run without the question, add `--yes`: `sudo dahae restore backups/db-20261110-033000.sql.gz --yes`
- If the whole server is lost, restore from the offsite backup: `sudo dahae restore-offsite` (list) → `sudo dahae restore-offsite <name>`. See [2. Setup wizard](02-setup-wizard.md), section 6.

## 5. update: update now
**When:** the developer says a new version is out and you don't want to wait for Monday. Normally it runs by itself every Monday at 04:00.
```
sudo dahae update
```
(Owners use the **Update now** button in the console.)
- Order: **backup → fetch the new version → check that it answers**.
- `✔ 최신 버전(…)으로 업데이트했어요` (updated to the latest version) means it worked.
- If the new version does not answer within 3 minutes, it **goes back to the previous version by itself**. Send the `sudo dahae logs` screen to the developer.
- Current version: `sudo dahae version`

## 6. rollback: go back to the version before the update
**When:** the update worked but a screen or feature misbehaves.
```
sudo dahae rollback
```
- Goes back to the previous **program** version. It backs up first.
- **Data is not deleted.** Bookings made after the update stay.
- Only the program goes back; the data layout the update changed stays. To take the data back in time too, use section 4 (restore).
- If no update has ever run, it says there is no previous version to go back to.
- Tell the developer after a rollback. The next Monday update installs the new version again.

## 7. channel: stable or beta
**When:** only when the developer asks you to try a beta. A server that takes real customers stays on `stable`.
```
sudo dahae channel beta
sudo dahae channel stable
```
- `stable` = the stable version (the default), `beta` = the early version.
- Switching updates to that channel's version at once (the same order as section 5).

## 8. heartbeat: get an alert when the server goes down
**When:** once, after the install. A server cannot report that it is down, so an outside service watches it.
1. Sign up with an uptime service such as healthchecks.io and create a **check**: period 5 minutes, grace 15 minutes.
2. Copy the **ping address** it shows (`https://hc-ping.com/…`).
3. In the terminal:
   ```
   sudo dahae heartbeat https://hc-ping.com/your-ping-address
   ```
4. `✔ 외부 가동 확인을 켰어요. 테스트 핑을 보냈어요.` (outside uptime check on, test ping sent) means it is done.

- The server sends an "I'm fine" signal every 5 minutes. When the signal stops, the service tells you by email or another channel.
- The address must start with `https://`.
- To turn it off: `sudo dahae heartbeat off`

## 9. applinks: connect app links
**When:** once, when the apps go to the stores and **the developer gives you the values**. It makes Naver sign-in return straight to the app.
```
sudo dahae applinks --android-sha256 AA:BB:…(value from the developer) --apple-team-id ABCDE12345
```
- The developer gives you both values. Paste them exactly.
- Several Android fingerprints are joined with commas. You can enter just one of the two values first.
- `✔ 앱 링크를 설정했어요` (app links set) means it is done.

## 9-1. domain: change the service address
**Normally this is done in the console under Settings > Domain.** Only in the terminal:
```
sudo dahae domain myshop.com
```
- First make the new domain's A record point at this server; otherwise it stops without changing anything.
- Type the new domain once more to confirm. The service pauses for about 30 seconds and comes back on the new address.
- If the new address does not answer within 5 minutes, the old settings come back and it says the domain could not be changed.
- The old address keeps answering, so installed apps keep working on it. Retire it in the console.

## 10. uninstall: remove the service
**When:** you stop the service or move servers. First run `sudo dahae backup` and copy the backup files and the `.env` file somewhere else.

**Switch off only (data stays)**
```
sudo dahae uninstall
```
- Type `yes` to stop the service and remove the scheduled backups and updates and the `dahae` command.
- Data and backups stay on the server. Running the install command again carries on from there.

**Delete everything (cannot be undone)**
```
sudo dahae uninstall --purge
```
- To confirm, you must **type the domain**. `--yes` does not skip this (in scripts: `--confirm <domain>`).
- It deletes customer and booking data, photos, the backups inside the server and the security keys. **It cannot be undone.**
- Cancel the server rental and the domain separately to stop their charges.

---

## Other commands
| Command | What it does |
|---|---|
| `sudo dahae version` | Current channel and version |
| `sudo dahae setup-code` | Show the setup code for creating the owner account again (`--renew` makes a new one) |
| `sudo dahae restore-offsite` | List offsite backups / download and restore one |
| `sudo dahae demo load` / `demo clear` | Load / clear demo data ([1. Server setup](01-server-setup.md)) |
| `sudo dahae load <file>.tar` | Load an image file to update without internet ([5. Demo install](05-demo-install.md)) |
