# 30. Glossary (KO/EN)

**In this chapter:** look up the hard words used in this manual and in the console, in plain words. The Korean names are listed too, so you can use them with the Korean screens or when you talk with Korean partners.

> **Before you start**
>
> - Nothing. Open this chapter whenever you meet a word you do not know.

Words in bold are names that appear on the console screen exactly as written.

---

## 30.1 People and accounts

| Term | Korean | Meaning | Chapter |
|---|---|---|---|
| **Owner** | **소유자** | The one top administrator of the company. Can unlock payment cards, update the server and hand over the owner role. Usually the business owner | 6, 8 |
| **Admin** | **관리자** | A staff account that sees every menu and setting | 8, 20 |
| **Consultant** | **상담원** | A staff account for bookings and chat only. Does not see Setup, Staff, Finance and so on | 8, 20 |
| Partner (pro) | 파트너 (기사님) | The person who goes to do the cleaning. Uses the partner app | 20, 24 |
| Team member | 팀원 | Someone a partner invited. The owner must approve them before they get jobs | 20 |
| Customer | 고객 | Someone who books through the customer app or website | 24 |
| Developer (installer) | 개발자 (설치 담당) | The person who installs the server and types server commands for you | 3, 33 |

## 30.2 Sign-in and security

| Term | Korean | Meaning | Chapter |
|---|---|---|---|
| **Setup code** | **설치 코드** | A 12-character code (`XXXX-XXXX-XXXX`) used once to create the owner account. Valid for 7 days | 6 |
| Authenticator app | 인증 앱 | A phone app that shows a 6-digit number that changes every 30 seconds (for example Google Authenticator) | 6 |
| **Two-step sign-in** | **2단계 인증** | Signing in with your password plus the 6 digits from the authenticator app. It cannot be turned off | 6 |
| **Recovery codes** | **복구 코드** | Ten codes to use instead of the authenticator app if you lose your phone. Each works once | 6, 29 |
| Owner code (**Owner authenticator code**) | 소유자 코드 (**사장님 인증 코드**) | The 6 digits in the owner's authenticator app. Asked for when unlocking cards or updating | 9, 26 |
| Step-up | 최근 재인증 | For important actions such as payouts or personal data, the authenticator code is checked again within 5 minutes | 8, 21 |
| Password manager | 비밀번호 관리 앱 | An app that keeps passwords and keys encrypted | 32 |
| **Sign out everywhere** | **모든 기기에서 로그아웃** | Signs out every PC and phone signed in with your account at once | 8, 29 |

## 30.3 The Setup page

| Term | Korean | Meaning | Chapter |
|---|---|---|---|
| Setup card | 설정 카드 | A box on the **[Setup]** page that connects one company service | 9 |
| **Locked** / **Unlock (owner code)** | **잠김** / **잠금 해제 (소유자 코드)** | Saved cards lock. Only the owner can unlock them, for 5 minutes | 9 |
| **Check connection** | **연결 확인** | Tests whether the saved key really connects to the company's service. On the payment cards it is **Check API key** | 9 |
| **Test** / **Live** | **테스트** / **실거래** | A test key is for trials and moves no money; a live key moves real money | 11, 12 |
| Key | 키 | A long secret string that lets a company's service recognise our server | 9 |

## 30.4 Payments and money

| Term | Korean | Meaning | Chapter |
|---|---|---|---|
| Payment gateway | PG (결제대행사) | A company that takes card and easy-pay payments for you. Dahae uses PortOne and Toss Payments | 11, 12 |
| Channel key | 채널 키 | A PortOne key for each payment method (card company and so on). Starts with `channel-key-` | 11 |
| Webhook | 웹훅 | An automatic message from the gateway to our server saying "this was paid" | 11, 12, 29 |
| Webhook secret / Security key | 웹훅 시크릿 / 보안 키 | A secret value that proves the message really came from the gateway | 11, 12, 13 |
| Payment reconciliation | 결제 대사 | Comparing the gateway's records with ours | 21, 26 |
| **Settlements** | **정산** | Working out the money owed to partners | 21 |
| Payout | 지급 | Sending the settled money to the partner's bank account | 13, 21 |
| **Manual transfer** / **Toss payouts** | **직접 이체** / **토스 지급대행** | You send it from the bank yourself / Toss sends it for you | 13 |
| Withholding | 원천징수 | Tax held back from what a partner is paid | 21 |
| Cash receipt | 현금영수증 | A receipt issued for cash payments such as bank transfers | 21 |
| Tax invoice | 세금계산서 | A tax document for business customers, issued on Hometax | 21 |

## 30.5 Texts and notifications

| Term | Korean | Meaning | Chapter |
|---|---|---|---|
| AlimTalk (KakaoTalk) | 알림톡 | A company notification sent through KakaoTalk. Templates must be approved | 14 |
| Sender number | 발신번호 | Our company number that people see when we text them. Must be registered and approved first | 14 |
| Template | 템플릿 | A pre-approved AlimTalk message pattern | 14 |
| Push notification | 푸시 알림 | A notification that arrives in the app (Firebase) | 16 |
| Social sign-in | 간편 로그인 | Signing in with a Kakao, Naver, Google or Apple account | 15 |
| **App links** | **앱 링크** | A setting that brings people back into the app, not the web, after signing in | 15 |

## 30.6 Server and internet addresses

| Term | Korean | Meaning | Chapter |
|---|---|---|---|
| Server | 서버 | A computer on the internet that holds the Dahae program and data | 3–7 |
| Domain | 도메인 | The site address (for example `example.co.kr`) | 5 |
| DNS | DNS | The internet's phone book that turns a domain into a server IP | 5 |
| A record | A 레코드 | One line in DNS that says "this domain is this IP" | 5 |
| Public IP | 공인 IP | The number address that finds the server on the internet (for example `203.0.113.10`) | 4, 5 |
| HTTPS / Certificate | HTTPS / 인증서 | The padlock in the address bar. The server gets and renews it by itself | 5 |
| CloudShell | CloudShell | A black command window inside the AWS web console | 3, 4 |
| IAM | IAM | The part of AWS that manages people and permissions | 3 |
| MFA | MFA | Two-step sign-in for AWS | 3 |
| CloudFormation stack | CloudFormation 스택 | A bundle that creates the server and what it needs on AWS in one go. Shows `CREATE_COMPLETE` when done | 4 |
| Session Manager | Session Manager | Opens a server command window from the AWS web console without SSH | 5, 29 |
| `dahae` command | `dahae` 명령 | The management command the developer uses on the server (for example `sudo dahae status`) | 33 |

## 30.7 Backups, updates, status

| Term | Korean | Meaning | Chapter |
|---|---|---|---|
| Backup | 백업 | A copy of the data. Made every day at 3:30 in the morning | 27 |
| Offsite backup | 외부 백업 | A backup copy kept in storage outside the server | 17, 27 |
| S3-compatible storage / Bucket | S3 호환 저장소 / 버킷 | An internet file store (Cloudflare R2 and others) for offsite backups, and the folder inside it | 17 |
| Secret key copy | 비밀 키 사본 | A password-locked file of the server's secret keys. Without it, offsite backups cannot be opened | 27 |
| `DATA_KEY` | `DATA_KEY` | The server's secret key that locks connection keys and backups. Never change or delete it | 27, 33 |
| Restore | 복원 | Putting data back to the moment of a backup. Data after that moment is lost | 27 |
| Rollback | 되돌리기 | Putting only the program back to the previous version. Data stays | 26, 33 |
| **Update** | **업데이트** | Changing the server program to a new version. Happens by itself every Monday at 4:00 in the morning | 26 |
| Stable / beta channel | 안정 / 베타 채널 | The line a server takes updates from. Servers that serve customers use stable | 33 |
| **System status** | **시스템 상태** | The console page that shows whether the server and connected services are healthy | 26 |
| **Uptime heartbeat** | **업타임 하트비트** | The server sends an "I am alive" signal to an outside service, which warns you when it stops | 17 |
| **Maintenance mode** | **점검 모드** | A switch that shows a maintenance banner at the top of the apps and stops new bookings. The apps and the console keep working | 26 |
| **Feature switches** | **기능 스위치** | Switches that turn bookings, chat, AI assistant and payments off and on separately | 26 |
| **Background jobs** | **백그라운드 작업** | Work the server does out of sight at set intervals | 26 |

## 30.8 Operations pages

| Term | Korean | Meaning | Chapter |
|---|---|---|---|
| **Operating rules** | **운영 규칙** | The page where the owner sets limits and alert thresholds | 23 |
| **Activity log** | **활동 기록** | A record of who did what and when. It cannot be deleted | 23 |
| **Account deletion and data requests** | **탈퇴·개인정보 요청** | The page for customer account deletion and data copy requests | 23 |
| **Renewal calendar** | **갱신 일정** | The page where you note things to renew regularly, such as the domain, server cost and accounts | 23 |
| **Store review demo account** | **스토어 심사용 데모 계정** | A temporary account opened so app store reviewers can sign in | 17, 25 |
| Unassigned | 미배정 | A booking that has no pro yet | 19 |
| No-show | 노쇼 (미도착) | The pro did not arrive at the agreed time | 19, 26 |

---

## If something goes wrong

> **If something goes wrong**
>
> - **The word I want is not here** → Read the chapter where the word appears, or take a screenshot and ask your developer.
> - **The names differ on the Korean screen** → Changing the console language changes the menu names. The "Korean" column shows the names on the Korean screen.
