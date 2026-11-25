# Dahae install

English · [한국어](README.ko.md)

This repository holds **only the installer and the guides**. There is no code to change or build.

## Steps (once)
| Step | Guide | Who | Time |
|---|---|---|---|
| 1 | [Install in one go](guide/00-install.md) (more detail: [Server setup](guide/01-server-setup.md); provider screens: [Install by cloud provider](guide/09-클라우드별-설치.md), Korean) | Owner or installer | About 30 minutes |
| 2 | [Setup wizard](guide/02-setup-wizard.md) | Installer and owner | 1 to 2 hours (plus the providers' approval time) |
| 3 | [App store release](guide/03-app-release.md) | Installer, then developer | The stores' account approval time |
| 4 | [Owner's daily guide](guide/04-owner-guide.md) | Owner | Five minutes a day |

**Step 1 install file:** download [dahae-install-page.html](dahae-install-page.html) and open it in a browser; it builds the text to paste into the server creation screen. (GitHub shows the file as code, so download it first.)

## Install command
Paste into the server's SSH window (the `v…` version tag in the address never changes):
```
curl -fsSL INSTALL_URL | sudo bash
```
To check the downloaded file first (the checksum file `SHA256SUMS` is under the same tag):
```
curl -fsSLO INSTALL_URL && curl -fsSLO SUMS_URL && sha256sum -c SHA256SUMS && sudo bash install.sh
```
The latest version of this channel is always at `BRANCH_URL` (`main` for stable, `beta` for beta).
To try the beta first, use the install command in the README of this repository's `beta` branch.
(After installing you can also switch with `sudo dahae channel beta`.)

## Release check
The `CHANNEL` file names this branch's channel; the `VERSION` file is the released version.

## Store listing material
- App names and descriptions: [store/listing-en.md](store/listing-en.md) ([한국어](store/listing-ko.md))
- Google Play data safety answers: [store/data-safety.md](store/data-safety.md)

## Updates
The server updates itself early every Monday (and goes back to the previous version by itself if the update fails). To update now, run `sudo dahae update` on the server.
Backup, rollback, removal and the other admin commands are in [Operations commands](guide/07-operations.md). All guides: [guide/README.md](guide/README.md)
App updates are uploaded to the stores by the developer.
