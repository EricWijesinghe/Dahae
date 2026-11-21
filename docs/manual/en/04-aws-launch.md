# 04. One command to create the server

## In this chapter

**[Developer]** Sign in to AWS with the deployer account from chapter 3 (`dahae-deployer`), paste one command into CloudShell, and the Dahae server is created. When the command finishes, the install carries on by itself inside the server.

The developer usually does this, because the deployer account's MFA is on the developer's phone. The owner reads this chapter to know what happens. The steps are the same whoever does them.

## Before you start

- The deployer sign-in URL, user name `dahae-deployer` and password (3.7)
- The phone with the deployer account's MFA (3.6)
- The **domain** customers will use (for example `example.co.kr` or `app.example.co.kr`), without `https://`, **in lower case**
- An **email** for certificate notices (for example `owner@example.co.kr`)
- (Only if the domain is in AWS Route 53) the **hosted zone ID** (starts with `Z`)
- Time: about 5–10 minutes for the command + about 15–20 minutes for the install inside the server

## 4.1 Sign in with the deployer account

1. Open the deployer sign-in URL (`https://<Account ID>.signin.aws.amazon.com/console`) in a browser.
2. In **IAM user name**, type `dahae-deployer`.
3. Type the password and click **Sign in**.
4. When asked for an MFA code, type the 6-digit number from the authenticator app.
5. Check the region at the top right is **Seoul**. If not, click it and choose **Asia Pacific (Seoul) ap-northeast-2**.

**You should now see:** the AWS console home, with `dahae-deployer` and **Seoul** at the top right.

## 4.2 Paste the one command

1. Click the **CloudShell** icon (`>_`) in the top bar.
2. Wait until the black window shows a `$` mark.
3. Paste the line below into Notepad, and replace `example.co.kr` and `owner@example.co.kr` with **your real domain and email**.

   ```
   curl -fsSL https://raw.githubusercontent.com/EricWijesinghe/Dahae/master/deploy/aws/launch.sh | bash -s -- --domain example.co.kr --email owner@example.co.kr
   ```

4. Copy the whole edited line.
5. Click once in the black CloudShell window and paste.
6. Press **Enter**.
7. One line appears first, for example:

   ```
   Dahae v1.5.1 -> stack dahae-main in ap-northeast-2 (vpc vpc-…, subnet subnet-…, instance t3.medium)
   ```

8. Then wait while AWS creates the server (about 5–10 minutes). **Do not close the window.**

![CloudShell after the launch command (IP covered)](../img/en/aws-cloudshell-launch.png)

**You should now see:** a table, and under it four lines of next steps starting with `Done. Next:`. The table holds values such as `PublicIp`, `InstanceId`, `DeployRoleArn`, `AwsRegion`, `DahaeDomain` and `NextStepUrl`.

9. Copy the **`PublicIp`** value from the table (for example `3.38.12.34`) and note it down. You need it in chapter 5.

> **Tip** If the domain is in Route 53, add `--hosted-zone-id <ID starting with Z>` to the end of the command. The domain's A record is then created for you too (5.1).

### What the command does

The command does these things in order. Nobody needs to do anything.

1. Finds the latest Dahae release number.
2. Downloads that release's installer and checks it against the release's list of fingerprints (`SHA256SUMS`). If they differ, it stops.
3. Pins the installer's fingerprint into the server blueprint (the CloudFormation template). The server refuses to run any other installer.
4. Finds a public subnet in the default network (default VPC).
5. Creates the stack `dahae-main`. The stack contains:
   - One Ubuntu 24.04 server (default `t3.medium`, encrypted 50 GB disk)
   - A fixed IP (Elastic IP)
   - A firewall open only on 80 and 443 (SSH port 22 is closed; you reach the server through the AWS web console, 5.5)
   - Daily disk snapshots (7 kept), automatic recovery on hardware failure, termination protection against accidental deletion
6. Prints the result table and the next steps.

## 4.3 Options you can add to the command

Add these to the end of the command only when needed. At first, adding none is recommended.

| Add this | Meaning | Default |
|---|---|---|
| `--hosted-zone-id Z123…` | Creates the A record in Route 53 for you | None (you set DNS yourself) |
| `--instance-type t3.large` | Changes the server size. Allowed values: `t3.medium`, `t3.large`, `t4g.medium`, `t4g.large` | `t3.medium` |
| `--version v1.5.1` | Installs a specific version | Latest release |
| `--stack dahae-name` | Changes the stack name. It must start with `dahae-` | `dahae-main` |
| `--vpc vpc-… --subnet subnet-…` | Picks the network yourself | Public subnet of the default VPC |
| `--region ap-northeast-2` | Region | Seoul |
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
| `error: --domain like example.co.kr is required` | The domain is missing or wrongly formed. Leave out `https://`, use **lower case**, like `example.co.kr` |
| `error: --email is required` | The email is missing or wrongly formed |
| `error: --stack must start with dahae-` | The stack name must start with `dahae-` |
| `error: no release version found (use --version vX.Y.Z)` | The latest version could not be found. Try again later, or ask the developer for the version and add `--version v1.5.1` |
| `error: install.sh of … does not match the release's SHA256SUMS` | The installer's fingerprint does not match. **Stop here** and tell the developer |
| `error: no default VPC in ap-northeast-2: pass --vpc and --subnet` | There is no default network in Seoul. Ask the developer for `--vpc` and `--subnet` values |
| `error: no public subnet in …: pass --subnet` | No subnet can reach the internet. Ask the developer |
| `error: stack dahae-main already exists; …` | The server already exists. You do not need a new one. For updates, see chapter 26 |
| `AccessDenied` or another permission error | Check you signed in with the MFA code, and that both policies are attached to the deployer account (3.5, 3.6) |
| Status `ROLLBACK_COMPLETE` or `CREATE_FAILED` | Creating the server failed. Take a screenshot of the **Events** tab in CloudFormation and send it to the developer |
| The CloudShell window closed | AWS keeps creating the server. Check as in 4.4 |
