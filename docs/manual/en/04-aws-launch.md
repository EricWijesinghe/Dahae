# 04. One command to create the server

## In this chapter

**[Developer]** Sign in to AWS with the deployer account from chapter 3 (`dahae-deployer`), paste one command into CloudShell, and the Dahae server is created. When the command finishes, the install carries on by itself inside the server.

The developer usually does this, because the deployer account's MFA is on the developer's phone. The owner reads this chapter to know what happens. The steps are the same whoever does them.

## Before you start

- The two policies from chapter 3 (`dahae-boundary`, `dahae-deployer-policy`). An AWS admin creates them by running `bash launch.sh --admin` **once** first (3.2). Without them the command in this chapter stops before creating anything.
- The deployer sign-in URL, user name `dahae-deployer` and password (3.7)
- The phone with the deployer account's MFA (3.6)
- The **domain** customers will use (for example `example.co.kr` or `app.example.co.kr`), without `https://`, **in lower case**. No domain yet? You can start with `auto` (tip in 4.2)
- An **email** for certificate notices and AWS alarm e-mails (for example `owner@example.co.kr`). Use a mailbox the owner reads: AWS sends a confirmation mail there (step 10 in 4.2)
- (Only if the domain is in AWS Route 53) the **hosted zone ID** (starts with `Z`)
- Time: about 5–10 minutes for the command + about 15–20 minutes for the install inside the server

## 4.1 Sign in with the deployer account

1. Open the deployer sign-in URL (`https://<Account ID>.signin.aws.amazon.com/console`) in a browser.
2. In **IAM user name**, type `dahae-deployer`.
3. Type the password and click **Sign in**.
4. When asked for an MFA code, type the 6-digit number from the authenticator app.
5. Check the region at the top right is **Seoul**. If not, click it and choose **Asia Pacific (Seoul) ap-northeast-2**.

**You should now see:** the AWS console home, with `dahae-deployer` and **Seoul** at the top right.

## 4.2 Paste the command (three lines)

1. Click the **CloudShell** icon (`>_`) in the top bar.
2. Wait until the black window shows a `$` mark.
3. Paste the three lines below into Notepad, and in the last line replace `example.co.kr` and `owner@example.co.kr` with **your real domain and email**.

   ```
   R=https://raw.githubusercontent.com/EricWijesinghe
   curl -fsSLO $R/Dahae/master/deploy/aws/launch.sh
   bash launch.sh --domain example.co.kr --email owner@example.co.kr
   ```

   > **Tip:** no domain yet? Write `auto` instead of the domain (`--domain auto`). The server then gets a free temporary address like `3.38.12.200.sslip.io` with a real certificate, and there is no DNS to set up. Move to your real domain later in the console (chapter 28.8).

4. Copy all three edited lines.
5. Click once in the black CloudShell window and paste.
6. Press **Enter**.
7. One line appears first, for example:

   ```
   Dahae v1.6.0 -> stack dahae-main in ap-northeast-2 (vpc vpc-…, subnet subnet-…, instance t3.medium)
   ```

8. Then wait while AWS creates the server (about 5–10 minutes). **Do not close the window.**

![CloudShell after the launch command (IP covered)](../img/en/aws-cloudshell-launch.png)

**You should now see:** a table, and under it five lines of next steps starting with `Done. Next:`. The table holds values such as `PublicIp`, `InstanceId`, `DeployRoleArn`, `AwsRegion`, `DahaeDomain`, `BackupBucket` and `NextStepUrl`.

9. Copy the **`PublicIp`** value from the table (for example `3.38.12.34`) and note it down. You need it in chapter 5. Note the **`BackupBucket`** value too (it starts with `dahae-`): it is needed only if this server is ever lost and replaced (4.3).
10. The last line says `5. Alarm e-mails: AWS sent "AWS Notification - Subscription Confirmation" to …`. Open that mailbox (look in the spam folder too), open the mail **AWS Notification - Subscription Confirmation** and click **Confirm subscription** once.

**You should now see:** a browser page **Subscription confirmed!**. From now on AWS e-mails this address when the server's hardware fails and is recovered (chapter 27, 27.3).

> **Warning** Without that one click, no alarm e-mail ever arrives. If the developer ran the command, tell the owner to look for the mail and click it.

> **Tip** If the domain is in Route 53, add `--hosted-zone-id <ID starting with Z>` to the end of the command. The domain's A record is then created for you too (5.1).

### What the command does

The command does these things in order. Nobody needs to do anything.

1. Finds the latest Dahae release number.
2. Downloads that release's installer and checks it against the release's list of fingerprints (`SHA256SUMS`). If they differ, it stops.
3. Pins the installer's fingerprint into the server blueprint (the CloudFormation template). The server refuses to run any other installer.
4. Finds a public subnet in the default network (default VPC).
5. Checks the policy `dahae-boundary` exists. If not, it stops before creating anything (3.2).
6. Creates the stack `dahae-main`. The stack contains:
   - One Ubuntu 24.04 server (default `t3.medium`, encrypted 50 GB disk)
   - A fixed IP (Elastic IP)
   - A firewall open only on 80 and 443 (SSH port 22 is closed; you reach the server through the AWS web console, 5.5)
   - Daily disk snapshots (7 kept), automatic recovery on hardware failure with an alarm e-mail to the email above, termination protection against accidental deletion
   - A private S3 bucket for offsite backups. The server writes its backups there with its own AWS role, so nobody types backup keys (chapter 27, 27.3). The bucket stays even if the stack is deleted
7. Prints the result table and the next steps.

## 4.3 Options you can add to the command

Add these to the end of the command only when needed. At first, adding none is recommended.

| Add this | Meaning | Default |
|---|---|---|
| `--hosted-zone-id Z123…` | Creates the A record in Route 53 for you | None (you set DNS yourself) |
| `--instance-type t3.large` | Changes the server size. Allowed values: `t3.medium`, `t3.large`, `t4g.medium`, `t4g.large` | `t3.medium` |
| `--version v1.6.0` | Installs a specific version | Latest release |
| `--stack dahae-name` | Changes the stack name. It must start with `dahae-` | `dahae-main` |
| `--vpc vpc-… --subnet subnet-…` | Picks the network yourself | Public subnet of the default VPC |
| `--region ap-northeast-2` | Region | Seoul |
| `--backup-bucket dahae-…` | **Only when a lost server is replaced:** the old stack's `BackupBucket` value. The new server reads the old backups and keeps using that bucket (same region) instead of making a new one (chapter 27, 27.7) | None (a new bucket) |
| `--dry-run` | Shows what it would do, without creating anything | - |

> **Warning** This command **never touches an existing stack.** If a stack with the same name exists, it stops. New versions come through the console's update (chapter 26) or the developer's automatic deploy, not through this command.

> **Note** `t4g` sizes are ARM machines. Use the default (`t3.medium`) at first.

## 4.4 Check in CloudFormation

Use this if you closed the command window by mistake, or want to see the result again.

1. Type **CloudFormation** in the search box at the top and open it.
2. On the left, click **Stacks**.
3. Find `dahae-main` in the list.
4. If the status is `CREATE_IN_PROGRESS`, it is still being created. Wait.
5. When the status is `CREATE_COMPLETE`, click `dahae-main`.
6. Click the **Outputs** tab.
7. Copy the `PublicIp` value.

![CloudFormation stacks: dahae-main CREATE_COMPLETE](../img/en/aws-cfn-stack-complete.png)

![PublicIp on the stack's Outputs tab (covered)](../img/en/aws-cfn-outputs.png)

**You should now see:** `dahae-main` with status `CREATE_COMPLETE`, and `PublicIp` on the **Outputs** tab.

## 4.5 Wait for the install to finish

> **Warning** `CREATE_COMPLETE` does **not** mean the install is finished. The installer keeps working inside the server for about 15–20 more minutes. (The command window says "about 10 minutes".)

1. While you wait, do chapter 5 (connecting the domain).
2. Check the install is finished as in 5.4.

What the installer does inside the server: installs Docker, the database (PostgreSQL), the web server (Caddy), the firewall, and the automatic backup and update schedule, and creates the **setup code** for the owner account (chapter 6).

## 4.6 The manual way (for reference)

If CloudShell cannot be used, the same server can be created by uploading a template file in CloudFormation. Preparing the template and filling in its parameters is developer work, described in chapter 33 (developer appendix). The key points:

- The stack name must start with `dahae-`.
- Keep the defaults for `GitHubRepo`, `GitHubRepoIds` and `GitHubEnvironment`.
- Leave `SshCidr` empty.
- Tick **I acknowledge that AWS CloudFormation might create IAM resources** at the bottom, or nothing is created.

Automatic deploys of new versions (the GitHub connection) are developer work too (chapter 33).

## If something goes wrong

When the command stops, it prints a line starting with `error:`. Find that line in the table below.

| What the screen says | Meaning and fix |
|---|---|
| `error: --domain like example.co.kr is required (or auto …)` | The domain is missing or wrongly formed. Leave out `https://`, use **lower case**, like `example.co.kr`. No domain yet: use `--domain auto` |
| `error: --hosted-zone-id needs a real --domain` | `--domain auto` does not take `--hosted-zone-id`. Leave one of them out |
| `error: --email is required` | The email is missing or wrongly formed |
| `error: --stack must start with dahae-` | The stack name must start with `dahae-` |
| `error: --backup-bucket is the old stack's BackupBucket output (dahae-…)` | The value after `--backup-bucket` is not a bucket name. Copy the old stack's `BackupBucket` value from CloudFormation → the old stack → **Outputs** |
| No **AWS Notification - Subscription Confirmation** mail | Look in the spam folder. Check the `--email` value was typed right. The developer can send it again from AWS (SNS → Subscriptions → **Request confirmation**) |
| `error: no release version found (use --version vX.Y.Z)` | The latest version could not be found. Try again later, or ask the developer for the version and add `--version v1.6.0` |
| `error: install.sh of … does not match the release's SHA256SUMS` | The installer's fingerprint does not match. **Stop here** and tell the developer |
| `error: no default VPC in ap-northeast-2: pass --vpc and --subnet` | There is no default network in Seoul. Ask the developer for `--vpc` and `--subnet` values |
| `error: no public subnet in …: pass --subnet` | No subnet can reach the internet. Ask the developer |
| `error: policy dahae-boundary is missing: an admin runs this script with --admin once first` | The chapter 3 policies do not exist yet. Nothing was created. Run `bash launch.sh --admin` once with the AWS admin account (3.2), attach the policy to the deployer account (3.5), then run this command again |
| `error: stack dahae-main already exists; …` | The server already exists. You do not need a new one. For updates, see chapter 26 |
| `AccessDenied` or another permission error | Check you signed in with the MFA code, and that both policies are attached to the deployer account (3.5, 3.6) |
| Status `ROLLBACK_COMPLETE` or `CREATE_FAILED` | Creating the server failed. Take a screenshot of the **Events** tab in CloudFormation and send it to the developer |
| The CloudShell window closed | If the screen had already shown `Waiting for stack create/update to complete`, AWS keeps creating the server. Check as in 4.4. If the stack stays at `REVIEW_IN_PROGRESS`, creation never started. Running the command again stops with `already exists`, so ask the developer to delete that stack and run it again |
