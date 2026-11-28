# Dahae guides

English · [한국어](README.ko.md)

New to Dahae? Follow **guide 0** to install, then **guide 2** to fill in the setup wizard. Every guide has an English version; the Korean translations are linked next to them. Guides under "Korea pack" apply only to installs that use the Korea pack.

| No. | Guide | What it covers | Who | When |
|---|---|---|---|---|
| 0 | [Install in one go](00-install.md) ([한국어](00-한번에-설치.md)) | Renting a server, connecting a domain, the one-line install command, creating the owner account | Owner or installer | Once, about 30 minutes |
| 1 | [Server setup in detail](01-server-setup.md) ([한국어](01-서버-설치.md)) | Server size, firewall, step-by-step fallback when guide 0 gets stuck | Installer | When guide 0 is blocked |
| 2 | [Setup wizard](02-setup-wizard.md) ([한국어](02-설정-마법사.md)) | Business details, payments, messaging, social sign-in, offsite backup, payouts | Installer and owner | Right after install |
| 3 | [App store release](03-app-release.md) ([한국어](03-앱-출시.md)) | Publishing the customer and partner apps to the App Store and Google Play | Installer, then developer | When the apps go to the stores |
| 4 | [Owner's daily guide](04-owner-guide.md) ([한국어](04-사장님-가이드.md)) | Daily console use: quotes, partners, staff, privacy requests | Owner | Five minutes a day |
| 5 | [Demo install](05-demo-install.md) ([한국어](05-demo-install.md#korean)) | Building a demo server from an image file, without GitHub | Developer | Offline demos |
| 7 | [Operations commands](07-operations.md) ([한국어](07-운영-명령어.md)) | The `dahae` command: status, logs, backup, restore, update, rollback, heartbeat | Installer | Backups, updates, troubleshooting |
| 8 | [Deploying on your own cloud account](08-cloud-deploy.md) ([한국어](08-클라우드-배포.md)) | Creating the cloud server, domain and DNS, the install command, offsite backup | Owner or installer | Once, when you set up your own server (about an hour) |
| 9 | [Install by cloud provider](09-install-by-cloud.md) ([한국어](09-클라우드별-설치.md)) | Which field to paste into for each provider, firewall, static IP, one-line browser SSH fallback | Owner or installer | When creating the server |
| 10 | [Deploying on AWS with automatic deploys](10-aws-deploy.md) ([한국어](10-AWS-배포.md)) | The deployer IAM account, launching the server with CloudFormation, approved GitHub deploys and rollback | Owner, installer, developer | Once, for a company that runs on AWS (about two hours) |
| 11 | [Store app builds (developer, GitHub Actions)](11-store-app-builds.md) ([한국어](11-앱-직접-빌드.md)) | Setting up the `store` environment, store accounts and keys, the `store-apps` runs, Play and TestFlight uploads, review submission | Developer, with the business's store accounts | First store release and each update |

## Korea pack

These guides are only for installs that use the Korea pack (the only country pack available today).

| Guide | What it covers | Who | When |
|---|---|---|---|
| [KakaoTalk AlimTalk templates](06-알림톡-템플릿-등록.md) (Korean, English summary at the top) | Registering the seven AlimTalk templates and entering them in the wizard | Installer | When AlimTalk is used |
| [Payment setup (PortOne · Toss Payments)](payments-setup.md) (Korean, English section included) | Key order, webhook check, test payments | Installer | When filling the payment card |
| [Payment integration reference](payments-reference.md) (Korean, English summary at the end) | PortOne V2 and Toss payout API notes, items still to confirm in a sandbox | Developer | When reviewing payment code |

## When something is wrong

| Situation | Where to look |
|---|---|
| The site does not open | [7. Operations commands](07-operations.md), 1 `status` and 2 `logs` |
| Something broke after an update | [7. Operations commands](07-operations.md), 6 `rollback` |
| Restore data from a backup | [7. Operations commands](07-operations.md), 4 `restore` |
| Lost the setup code | Run `sudo dahae setup-code` on the server |
| Not sure the payment keys are right | [Payment setup](payments-setup.md), section 3, webhook check |

## Engineering docs

- [Feature catalogue](../FEATURES.md#english), [Architecture](../ARCHITECTURE.md), [Security and privacy](../SECURITY-AND-PRIVACY.md), [Security policy](../../SECURITY.md)
- [Operations guide](../OPERATIONS.md#english), [Incident runbook](../RUNBOOK.md#english), [Quality and checks](../QUALITY.md), [API index](../api/README.md)
