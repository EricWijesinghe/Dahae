# 10. Deploying Dahae on AWS with automatic deploys

English · [한국어](10-AWS-배포.md)

This guide puts a Dahae server on **the business's AWS account** and connects it so that new versions are deployed from the developer's GitHub repository (`EricWijesinghe/Dahae`). The AWS account and its costs are in the business's name; the developer deploys and maintains the server. If the business wants, it can add a deploy approver (part C). The business's part (part A) is **pressing buttons on screen in order**; it types no commands. The developer uses commands only in B-1 (building the template) and on the server (`dahae setup-code`).

> Files this guide refers to: [deploy/aws/dahae.cfn.yaml](../../deploy/aws/dahae.cfn.yaml) (the AWS template that creates one server), [deploy/aws/deployer-iam-policy.json](../../deploy/aws/deployer-iam-policy.json) (the deployer account's permissions), [deploy/aws/dahae-boundary-policy.json](../../deploy/aws/dahae-boundary-policy.json) (the permissions limit of the server's roles), [.github/workflows/deploy-aws.yml](../../.github/workflows/deploy-aws.yml) (automatic deploys). The filled-in template `dahae-aws.yaml` is made in B-1.
>
> The examples use the Seoul region (`ap-northeast-2`) and `.co.kr` domains because the Korea pack is the only country pack available today. Choose the region closest to your customers; the steps are the same.

## Contents

- What you need, and who does what
- A. Create the deployer account (the business)
- B. Create the server (CloudFormation)
- C. Connect automatic deploys (GitHub)
- D. Backups, monitoring, costs, deletion

---

## What you need, and who does what

| You need | Who | Notes |
|---|---|---|
| An AWS account (with a payment method on file) | The business | Create it in the company's name |
| One IAM user (the deployer account) | The business creates it and gives it to the developer (the developer does part B with it; new versions are deployed through GitHub) | Part A below |
| The permissions-boundary policy `dahae-boundary` | The business creates it once (attached to nobody) | Part A, step 3 |
| A domain (for example `app.example.co.kr`) | Whoever holds the domain (often the developer, at a registrar such as Cloudflare) | The address customers use. Agree it with the business |
| Domain DNS (A record) | Whoever holds the domain | One A record. For a bare domain (for example `example.co.kr`), two: `@` and `www`. On Cloudflare, grey cloud (DNS only) |
| The GitHub repository `EricWijesinghe/Dahae` | The developer | The developer manages it. If the business wants a deploy approver, that person needs access to this repository too |
| An authenticator app (Google Authenticator or similar) | The owner | For the owner's sign-in and approval codes |
| An authenticator app | The developer | MFA for the deployer account (IAM user), set up on a call in part A, step 10 |
| A password manager | The business | Never put account details in chat |

**Security rules:** passwords and MFA setup keys never go in chat, an email body or a document. Keep them in a password manager and, if they must be passed on, use another channel such as a phone call. Do not create AWS access keys (long-term keys): GitHub deploys run on temporary credentials, without keys.

---

## A. Create the deployer account (the business)

Sign in to the AWS console (https://aws.amazon.com/console/) with the company account.

> **Quick way (steps 3 and 7 in one line):** switch the region, press the **CloudShell** icon (`>_`) at the top of the console and paste the line below. It creates both policies (`dahae-boundary` and `dahae-deployer-policy`). Then do only steps 4, 5 and 8 to 11.
> ```
> curl -fsSL https://raw.githubusercontent.com/EricWijesinghe/Dahae/master/deploy/aws/launch.sh | bash -s -- --admin
> ```
> An account that created the policies earlier runs this line **once more**: existing policies get the new version (templates newer than v1.6.0 also create the offsite backup bucket and the alarm e-mail topic, so the permissions grew slightly).

1. At the top right, switch the region to the one you will use (for Korea, **Asia Pacific (Seoul) ap-northeast-2**).
2. Type **IAM** in the search box and open it.
3. **Permissions-boundary policy first (once):** on the left, **Policies › Create policy › JSON**, and paste the whole content of `deploy/aws/dahae-boundary-policy.json` from the Dahae repository. **Next**, policy name **exactly `dahae-boundary`**, **Create policy**. Do **not** attach this policy to any user. The roles the template creates can act only within this limit, and the deployer account can neither create a role without it nor edit it. With a different name, creating the stack in part B fails.
4. On the left, **Users › Create user**. User name: `dahae-deployer` (or another name). Tick **Provide user access to the AWS Management Console**.
5. Choose **Custom password** and make a strong one. Save it in the password manager. "Users must create a new password at next sign-in" may be on or off.
6. Under **Set permissions**, choose **Attach policies directly** and press **Create policy** (a new tab opens).
7. Press the **JSON** tab and paste the whole content of `deploy/aws/deployer-iam-policy.json` from the Dahae repository. **Next**, policy name `dahae-deployer-policy`, **Create policy**.
8. Back in the first tab, refresh the list, tick `dahae-deployer-policy` and the AWS managed policy **`AWSCloudShellFullAccess`** (needed for the one-line command in part B), then **Next › Create user**. (Do not tick `dahae-boundary`.)
9. In the user list, open the new user, go to **Security credentials › Assign MFA device** and choose **Authenticator app**.
10. Do this step **on a call with the developer**: the developer scans the QR code on screen with the developer's own phone and reads back two codes, which you enter. (The deployer policy does not let the user register its own MFA, so the business registers it; the authenticator app is on the developer's phone.)
    - Until MFA is on, server work with this account is refused. The policy blocks it without MFA, on purpose.
    - Do **not** create access keys.
11. Send the developer: the account sign-in address (`https://<12-digit account number>.signin.aws.amazon.com/console`), the user name, and the password (through a separate channel). MFA is already on the developer's phone from step 10.

---

## B. Create the server (CloudFormation)

The developer signs in with the account above and enters the MFA code. Check the region again.

> **All at once (recommended):** press the **CloudShell** icon (`>_`) at the top of the console, change only the domain and email, and paste the line below. It checks the latest release's install script against the release's `SHA256SUMS`, pins it into the template, finds the default VPC's public subnet, creates the stack `dahae-main`, and shows the server IP and what to do next (about 5 to 10 minutes). Then continue from B-3 (domain). B-1 and B-2 below do the same by hand.
> ```
> curl -fsSL https://raw.githubusercontent.com/EricWijesinghe/Dahae/master/deploy/aws/launch.sh | bash -s -- --domain example.co.kr --email owner@example.co.kr
> ```
> - No domain yet? `--domain auto` opens on a temporary `<server IP>.sslip.io` address and skips DNS. Move to the real domain later in the console.
> - With the domain in Route 53, add `--hosted-zone-id <ID>` to create the A record too. Change the size with `--instance-type t3.large`. To only check what it would do, `--dry-run`.
> - It never touches an existing stack. New versions are installed by the GitHub deploys in part C.
> - It also runs in **Git Bash on Windows** instead of CloudShell (install the AWS CLI and sign in to the deployer account with `aws configure` first). The script converts file paths and the template for Windows by itself.

### B-1. Prepare the template file (developer)

The easiest path is `launch.sh` above. To upload the template by hand, download `dahae-aws.yaml` from the GitHub repository's **Releases → Latest → Assets**. The release workflow builds it from `master`'s template with that release's install script address and fingerprint filled in, so use it as is.

(Optional) You can also build the template yourself (Git Bash or a Linux shell, signed in to `gh`):

```bash
git clone https://github.com/EricWijesinghe/Dahae.git && cd Dahae   # master
gh release download v1.6.1 -R EricWijesinghe/Dahae -p install.sh -p SHA256SUMS -p SHA256SUMS.bundle -D rel-v1.6.1
bash scripts/release/verify-blob.sh rel-v1.6.1/SHA256SUMS EricWijesinghe/Dahae   # with cosign installed: checks the release signature
(cd rel-v1.6.1 && sha256sum -c --ignore-missing SHA256SUMS)                       # install.sh: OK
dist=https://raw.githubusercontent.com/EricWijesinghe/Dahae-install           # the public delivery repository (DIST_REPO)
bash scripts/release/build-aws-template.sh rel-v1.6.1/install.sh "$dist/v1.6.1/install.sh" out
```

- The last line prints `aws template written: out/dahae-aws.yaml (install.sh sha256 <fingerprint>)`. That fingerprint must equal the `install.sh` line in `SHA256SUMS`.
- The address must contain a tag (`/v1.6.1/`). The script refuses branch addresses such as `main`.
- Upload the resulting `out/dahae-aws.yaml` in B-2.
- (`deploy/aws/dahae.cfn.yaml` in the repository is the original with blanks; it cannot be used as is.)

### B-2. Create the stack

1. Type **CloudFormation** in the search box and open it.
2. **Stacks › Create stack › With new resources (standard)**.
3. **Prepare template › Upload a template file › Choose file**, pick `dahae-aws.yaml`, **Next**.
4. **Stack name: must start with `dahae-`** (for example `dahae-shop`). The permissions allow only `dahae-*` names, so any other name fails.
5. Fill in the parameters.

| Parameter | Value |
|---|---|
| Domain | The address customers use (for example `app.example.co.kr`), without `https://` |
| Email | The address for certificate notices and **server alarm e-mails** (confirm it once in step 8) |
| HostedZoneId | The hosted zone ID (starts with Z) if the domain is in Route 53. Otherwise **leave empty** |
| InstanceType | `t3.medium` (x86, the default). Keep the default at first. `t4g` is ARM; changing to it creates a new server |
| DiskSizeGiB | `50` (default). Increase it if you store many photos |
| VpcId | The default VPC |
| SubnetId | One subnet of that VPC **that can reach the internet**. If unsure, one of the default subnets |
| SshCidr | **Leave empty** (SSH stays closed; access is through the AWS web console) |
| ExistingBackupBucket | **Leave empty.** Only when rebuilding a lost server: the old stack's `BackupBucket` output (D-1) |
| GitHubRepo | **Keep the default `EricWijesinghe/Dahae`.** Only this repository's `deploy-aws` workflow may deploy to this server. Do not enter another address |
| GitHubRepoIds | **Keep the default `EricWijesinghe@48539716/Dahae@1452913067`.** The fixed numbers GitHub uses for the deploy sign-in (they stay the same if the repository is renamed). The developer can check them with `gh api repos/EricWijesinghe/Dahae --jq '.owner.id,.id'` |
| GitHubEnvironment | `doda-aws` (the default; must be the same name as in part C. Another name works if the repository variable `DEPLOY_AWS_ENVIRONMENT` names the same environment) |
| CreateGitHubOidcProvider | `true` if this account has never connected to GitHub; `false` if it already has |
| AmdAmiId, ArmAmiId | Do not change (the defaults find Ubuntu 24.04) |

6. **Next › Next**. At the bottom, tick **I acknowledge that AWS CloudFormation might create IAM resources** and **Submit**.
7. Wait until the status goes from `CREATE_IN_PROGRESS` to `CREATE_COMPLETE` (about 3 to 5 minutes).
8. **Confirm the alarm e-mail (one click):** the Email address receives **AWS Notification - Subscription Confirmation**. Press **Confirm subscription** in it once. Without it, server alarms (failed hardware checks and automatic recovery) are not sent. If you cannot see the mail, check the spam folder. (The same applies when the stack was made with the CloudShell line.)

> `CREATE_COMPLETE` does not mean **the install has finished**. Inside the server the install takes about 15 to 20 minutes more.

### B-3. Connect the domain

- **With Route 53 (HostedZoneId entered):** the `Domain` A record is created for you. For a bare domain, add the `www` A record yourself (same IP).
- **With Cloudflare (or another registrar):** copy the `PublicIp` value from the stack's **Outputs** tab, and add an **A record** on the DNS screen.
  - Host (name): the front part of the domain (for example `app`). For a bare domain (for example `example.co.kr`), make two records: `@` and `www`.
  - Type: `A`
  - Value: the IP from `PublicIp`
  - TTL: 300 (or the default)
  - On Cloudflare, set the **proxy status to grey cloud (DNS only)**. The server gets its own certificate and takes customer connections directly.
- DNS can take from a few minutes to an hour. The certificate is issued by itself.

### B-4. Check that the install finished

About 20 minutes later, open `https://<domain>/admin` in a browser. When the owner-account setup screen appears, the install has finished. If it does not, check the server with **B-5**.

### B-5. Reach the server through the web console (no SSH)

1. In the AWS console, go to **EC2 › Instances** and tick the server named `dahae-dahae-…`.
2. Press **Connect** at the top › **Session Manager** tab › **Connect**.
3. In the black screen, type (`sudo` is needed):

```
sudo dahae status
```

- If lines such as the offsite backup or the secret key copy show a warning, handle them as part D explains.
- If the Connect button is not available, check that the instance state is `Running`, and try again 2 to 3 minutes later.

### B-6. Get the owner-account setup code

On the server screen (B-5):

```
sudo dahae setup-code
```

A 12-character code appears in the form `XXXX-XXXX-XXXX`. It is valid for **7 days after it is first shown**. After that, get a new one with `sudo dahae setup-code --renew`.

Steps 1 to 4 below are done **by the business's owner**. On a call, the developer reads out the code and the owner continues straight away (the authenticator app is on the owner's phone). If the developer created this account, the owner code that guards the payment cards would end up on the developer's phone.

1. Open `https://<domain>/admin` in a browser.
2. Enter the setup code and create the **owner account** (email, password).
3. Register two-step sign-in in the authenticator app. Write the recovery codes on paper and keep them off the server.
4. On the Setup screen, connect business details, payments and messaging in order ([guide 2](02-setup-wizard.md)).

---

## C. Connect automatic deploys (GitHub)

New versions reach the server through the `deploy-aws` workflow in the developer's repository. If approvers are set on the `doda-aws` environment, a deploy goes in only after approval. The business adds approvers when it wants them.

### C-1. Create the GitHub environment (developer's repository)

The developer does this in the GitHub web interface of `EricWijesinghe/Dahae`.

1. Open https://github.com/EricWijesinghe/Dahae (with the developer account).
2. **Settings › Environments › New environment**, name `doda-aws` (the same as GitHubEnvironment in B-2).
3. Set **Deployment branches** to **Selected branches** and add only `master` (do this). No other branch can use the environment.
4. (Optional) If the business wants to approve deploys, turn on **Required reviewers** and add the business's contact. (Without approvers, a deploy runs as soon as the developer starts it.)
5. Under **Environment variables**, add these four with **Add variable**. The values are in the B-2 stack's **Outputs** tab.

| Variable (Name) | Output | Example |
|---|---|---|
| `AWS_REGION` | `AwsRegion` | `ap-northeast-2` |
| `AWS_DEPLOY_ROLE_ARN` | `DeployRoleArn` | `arn:aws:iam::123456789012:role/dahae-…` |
| `AWS_INSTANCE_ID` | `InstanceId` | `i-0123456789abcdef0` |
| `DAHAE_DOMAIN` | `DahaeDomain` | `app.example.co.kr` |

- Put them under **Variables, not Secrets**. They are not passwords or keys.
- Write the names exactly, including case.
- **The CD workflow (`deploy-aws.yml`) runs in `EricWijesinghe/Dahae`.** The environment and its four variables must be in this repository. The trust (OIDC) is set by `GitHubRepoIds`, `GitHubRepo` and `GitHubEnvironment` in B-2, so no other repository can deploy to this server.

### C-2. What happens automatically

1. The developer creates a stable tag (`vX.Y.Z`) and the release workflow (`release`) finishes.
2. The **deploy-aws** job starts by itself. With approvers on the `doda-aws` environment it waits for approval (without them it deploys at once).
3. The server runs `dahae update v1.6.1` (after an approver presses **Review deployments › doda-aws › Approve and deploy**, when approvers are set). The update may wait up to 6 hours.
4. The server backs up first, pulls the new version and checks that it answers. If the new version does not answer, it **rolls back to the previous version by itself**.
5. Finally the job checks `https://<domain>/health` and the version the server reports. When both pass, the job turns green.

> **Beta versions (tags such as `v1.5.0-beta.1`) are never deployed automatically.** Customer servers get stable versions only.
>
> **Approval differs by path:** an update the owner presses in the console is still approved with the owner's authenticator code. A deploy run from GitHub is approved by **the GitHub approver** when one is set, otherwise by the developer starting it.
>
> **Release and deploy:** new versions are released from `EricWijesinghe/Dahae` (signed server images and install scripts are public). When the release workflow finishes, `deploy-aws` starts by itself. Deploying a specific version and rolling back are in C-3.
>
> **Automatic updates:** by default the server has an **automatic update every Monday at 04:00** (stable channel, 24 hours after a new version is first seen), so the server can update itself without a GitHub deploy. To update only through GitHub, run `sudo dahae update-schedule off` on the server (to restore it, `sudo dahae update-schedule mon@04:00`).

### C-3. Deploy or roll back by hand

- **Deploy a specific version:** the repository's **Actions** tab › **deploy-aws** on the left › **Run workflow** › enter `v1.6.1` in `version` › **Run workflow**. With approvers, it runs after approval.
- **Roll back to the previous version:** on the same screen, tick **rollback** and **Run workflow**. The server runs `dahae rollback` and returns to the version right before the last update.
- **Roll back from the console (no GitHub):** console **System status › Server update** card › **Recovery › Roll back to the previous version**. Enter the owner code, then the server approval code. The approval window shows **the version it will return to** (for example `v1.6.1`). It backs up first and brings the management tools back to that version too.
- **Automatic updates after a rollback:** the rolled-back version is held. While the channel (or the pinned tag) still points at the version from before the rollback, the weekly update does not reinstall it; **when a newer version appears**, automatic updates resume. A manual update (GitHub deploy, console update, `dahae update`) ends the hold. If you roll back to v1.6.x or older, that version's management tools do not know about the hold: turn automatic updates off in the console.
- If there is no previous version to return to (no update since the install), the console says so and changes nothing.
- A rollback stops while a payout is in progress. Settle its state first on the console's **Settlement** screen (the console shows the reason as is).

### C-4. When it fails

- If the job is red, open **Actions › that run › server output**. The server's output is there as is.
- If the server rolled back, the output says so. The service keeps running on the previous version.
- If the server itself does not answer, go in through B-5 and check `sudo dahae logs`.

### C-5. Getting new management tools (job runner and `dahae` command)

From versions newer than v1.6.0, `dahae update <tag>` (GitHub deploys, console updates and automatic updates alike) replaces, along with the app images, **that version's management tools** (the `dahae` command, the server job runner, systemd units, the schedule and the compose settings). It downloads that version's `install.sh` and `agent.sh` from the delivery repository (`EricWijesinghe/Dahae-install`) and uses them only when their fingerprints match the `SHA256SUMS` that the release signed **at exactly that tag** (the signature is checked the same way as the server images); if anything differs, it changes nothing and stops. A channel (stable or beta) is read as the tag it points at now, and refused if that tag is older than a version this server already ran (an old signed file offered again is not used). `.env` and the data stay as they are. `dahae rollback` also brings the management tools and the job runner back to the rolled-back version (from the kept files, without downloading again). The record of tools moves one step only when the app images really change: a weekly automatic update with no new version does not touch the rollback record.

**If the new version is fine but only the management tools could not be replaced**, the console job says that only the tools could not be replaced and asks you to run the install line once again. The service runs fine on the new version. Running the install line once, as below, brings them in line. The same applies to rollbacks.

**Servers made with v1.6.0 or older must run the install line once** (that version's `dahae update` replaces only the app images):
AWS console → **Systems Manager › Run Command › AWS-RunShellScript** → choose the instance, put the install line in the command as is and run it (same domain; the data stays). The web SSH in B-5 works too.
After a domain move, run it without `MALKKEUM_DOMAIN` (it uses the domain in `.env`); with it, the server moves back to that domain.

---

## D. Backups, monitoring, costs, deletion

### D-1. Backups (two layers)

| What | Where | When | How to check |
|---|---|---|---|
| Database and photos (on the server) | Server disk `backups/` (sealed files) | Every day at 03:30 (server time) | `sudo dahae status` |
| Offsite backup | The **S3 bucket** this template created (automatic, no keys to type). If keys for another S3-compatible store are entered in the console's **Setup › Offsite backup**, that store is used instead | Every day, and before updates and rollbacks (photos too) | Console card: **AWS S3 (automatic)** |
| Disk snapshots | AWS **EBS snapshots** (the template makes one daily and keeps 7) | Every day at 03:00 Korea time | EC2 › Snapshots |
| Secret key copy | Console **System status › Server update › Recovery › Download the secret key copy** (or `sudo dahae keys-backup` on the server) | Once, and whenever it changes | Keep it off the server (password manager or USB). After downloading, press **Kept off the server** in the console to record it |

- **Make the secret key copy.** The offsite backup is locked with the secret keys; without a key copy it cannot be opened after the server is lost.
- **What the secret key copy holds:** only the two keys that open the offsite backup (`DATA_KEY` and `JWT_SECRET`). No other values such as the database password.
- **Automatic offsite backup (S3):** the server's role may only **create new files**, read and list in this bucket. **It cannot delete or overwrite**: every upload must carry the "only if no file of this name exists" condition, so even a taken-over server cannot delete or change existing copies (it can add new files). Versioning is on, the bucket encrypts (SSE-S3) and public access is blocked. Bucket rules remove old backups: they expire after 35 days, and expired versions are kept 35 days more. If backups stop, the last copy also disappears after about 70 days, so look at the offsite backup line in `sudo dahae status` now and then. Deleting the stack keeps the bucket.
- **Stacks created before this version have no S3 bucket.** `launch.sh` does not change an existing stack. On such a server, connect another S3-compatible store with keys in the console's **Setup › Offsite backup**, as before (do this). Talk to the developer about adding the bucket by updating the template (see the stack-update warning in D-4).
- **Rebuilding a lost server:** pass the old stack's **BackupBucket** output to the new stack, and the new server reads that bucket and keeps writing to it. With the CloudShell line, add `--backup-bucket <BackupBucket name>` at the end; by hand, put it in **ExistingBackupBucket** in B-2 (same region). After the install, restore on the server with `sudo dahae keys-restore <secret key copy>` and then `sudo dahae restore-offsite`.
- Snapshots alone are not enough. Keep one more copy off the server (the offsite backup).

### D-2. Monitoring

- **Heartbeat:** connect an outside uptime service (for example healthchecks.io) on the server with `sudo dahae heartbeat <address>`. If the server stops, the outside service tells you.
- **Automatic hardware recovery and alarm e-mails:** the template created an AWS alarm. If the server's hardware check fails, AWS brings the same server back on other hardware and sends an alarm e-mail to the Email address (and once more when it is OK again). You must have pressed **Confirm subscription** in B-2 step 8 to get the mail.
- **No disk-space alarm:** the server has no CloudWatch agent, so AWS does not know the disk usage. Watch the disk in `sudo dahae status` and the console's System status.
- **Operations commands:** logs and status commands are in [guide 7](07-operations.md).

### D-3. Costs

- Costs depend on the instance type, disk size, data transfer and the number of snapshots.
- Work out the exact amount yourself in the AWS pricing calculator (https://calculator.aws/) with your region and the settings above. This guide lists no prices.
- A static IP (Elastic IP) costs nothing extra while it is attached to the server; once detached, it is charged.
- **S3 backups:** storage is roughly (database backup + all photos) × about 70 days of copies. All photos are uploaded every day and before each update and rollback, and each copy stays 35 + 35 days. With many photos, add S3 Standard storage to the calculator too.

### D-4. Deleting safely

Before removing the server, **always** do the following in this order. Deleting stops the service.

1. Last backup: run `sudo dahae backup` on the server and check the offsite backup.
2. EC2 › Instances › the server › **Actions › Instance settings › Change termination protection**, and **turn it off** (with it on, deleting the stack fails).
3. CloudFormation › Stacks › choose `dahae-…` › **Delete**.
4. What stays after deletion:
   - **The disk (volume):** stopping the server keeps the disk (data protection). Once you are sure it is no longer needed, delete it yourself under EC2 › Volumes.
   - **The static IP:** check under EC2 › Elastic IPs and release it if not needed (with the business's admin account if the deployer account cannot).
   - **The OIDC provider:** used account-wide, so the stack does not delete it.
   - **The backup S3 bucket (`dahae-…-backupbucket-…`):** kept for data protection. Once you are sure it is no longer needed, empty and delete it in the S3 console (the business's admin account).
5. Delete the variables of the GitHub environment `doda-aws`, or the environment itself.
6. If the deployer IAM user is no longer needed, delete it under **IAM › Users › Delete**. When no Dahae server is left, also delete the policies `dahae-deployer-policy` and `dahae-boundary` under **IAM › Policies** (the business's admin account).

> **Stack updates:** when you **update** the stack in CloudFormation and the change set shows `ServerInstance` with `Replacement: True`, **cancel**. That change creates a new server. To move to a new server, talk to the developer first.
