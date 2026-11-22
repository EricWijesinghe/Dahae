# Manual images that are not rendered

`scripts/manual/shots.sh` renders the console, customer app and partner app screens from the real code
with sample data. `scripts/manual/install_page_shot.sh` renders the install page. The images below are
not made, so the manual describes these steps in text, or are drawn with a stand-in. They fall into two groups.

## Third-party websites and terminals (never drawn by us)

We never imitate another company's screen. The owner sees the real one while following the steps. If a
picture is wanted later, take it from a real account and blur keys, codes, IPs and QR codes first.

| id | chapter | what it would show |
|---|---|---|
| aws-region-seoul | 03 | AWS console top bar, region menu open, Asia Pacific (Seoul) ap-northeast-2 |
| aws-cloudshell-admin | 03 | CloudShell after the `--admin` command (policies created, "Next: IAM > Users") |
| aws-iam-create-user | 03 | IAM > Users > Create user: dahae-deployer, console access, custom password |
| aws-iam-attach-policies | 03 | Attach policies directly: dahae-deployer-policy and AWSCloudShellFullAccess ticked, dahae-boundary not ticked |
| aws-iam-mfa | 03 | Assign MFA device: Authenticator app, QR step |
| aws-cloudshell-launch | 04 | CloudShell after the launch command, outputs with PublicIp |
| aws-cfn-stack-complete | 04 | CloudFormation > Stacks: dahae-main CREATE_COMPLETE |
| aws-cfn-outputs | 04 | Stack dahae-main, Outputs tab, PublicIp |
| cloudflare-dns-a-record | 05 | Cloudflare DNS > Add record: A, @, IPv4, DNS only (grey cloud) |
| browser-health-ok | 05 | Browser at https://(domain)/health showing `{"ok":true}` |
| aws-ssm-connect | 05 | EC2 > Instances > Connect > Session Manager, Connect button |
| aws-ssm-status | 05 | Session Manager terminal after `sudo dahae status` |
| console-pwa-install | 06 | Chrome/Edge address bar Install app icon and its dialog (browser UI, not ours) |
| install-finish-terminal | 07 | install.sh finish screen with the admin URL and setup code |
| external-portone-store-id | 11 | admin.portone.io where the Store ID is shown |
| external-portone-webhook | 11 | admin.portone.io V2 webhook settings |
| external-toss-api-keys | 12 | developers.tosspayments.com API keys page |
| external-toss-webhook | 12 | Toss developer center webhook registration (PAYMENT_STATUS_CHANGED, DEPOSIT_CALLBACK) |
| external-solapi-apikey | 14 | console.solapi.com API key creation |
| external-kakao-alimtalk-template | 14 | Kakao business AlimTalk template registration |
| external-r2-token | 17 | Cloudflare R2 API token with Object Read & Write |
| external-play-console-users | 25 | Google Play Console > Users and permissions |
| external-appstore-users | 25 | App Store Connect > Users and Access |

## Drawn with a stand-in

- **customer-login-social**: the real sign-in button row (`SocialSignInRow`) on its own page, with Kakao,
  Naver, Google and Apple. A test build has no native sign-in SDK keys, so the sign-in screen itself
  shows no buttons there.
- **customer-home-branded**: only the main colour and business name change. A logo needs a file uploaded
  to the server, so the built-in mark stays.
- **install-page-form**: the page is bilingual (Korean with English subtitles), so `ko` and `en` use the
  same image. It is built with the installer self-test's placeholder URL (`owner/repo`). The setup code
  and the generated text are blurred.
- **console-mfa-enrol / console-mfa-recovery-codes**: the setup key, the link and the recovery codes are
  sample `X` strings. The test font draws them as solid bars, which works as the blur.
