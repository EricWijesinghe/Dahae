# 03. AWS: the deployer account

## In this chapter

**[Owner]** In the company's AWS account, create one **deployer account** (IAM user) named `dahae-deployer` that can only install the server. The developer creates the server with this account (chapter 4). The developer never gets the company AWS account's own password.

This takes about 20–30 minutes. The last step (MFA) is done **on a call with the developer**.

## Before you start

- An **AWS account** in the company's name, with a payment method added. AWS console: https://aws.amazon.com/console/
- The email and password that sign in to that AWS account as an administrator
- A computer (much easier than a phone)
- A password manager app
- Time for a call with the developer (about 10 minutes for the last step)

> **Note** The AWS console can be shown in Korean or English. This chapter uses the English screen names. AWS renames screens from time to time, so if a name is slightly different, pick the closest one. We do not draw AWS screens; the pictures are placeholders.

## What this chapter creates

| Name | Kind | What it does |
|---|---|---|
| `dahae-boundary` | Policy (permissions boundary) | Keeps the roles the server uses inside this limit. **Attach it to nobody** |
| `dahae-deployer-policy` | Policy | Sets what the deployer account may do (create a Dahae server) |
| `dahae-deployer` | User (deployer account) | The account the developer signs in with |

> **Warning** Type the policy names **exactly, character for character**. If `dahae-boundary` has any other name, creating the server in chapter 4 fails.

## 3.1 Sign in to AWS and switch the region to Seoul

1. On a computer, open https://aws.amazon.com/console/ in a browser.
2. Sign in with the company AWS account.
3. At the **top right** you see a region name (for example "N. Virginia"). Click it.
4. In the list, click **Asia Pacific (Seoul) ap-northeast-2**.

![AWS region menu with Seoul selected](../img/en/aws-region-seoul.png)

**You should now see:** **Seoul** at the top right.

> **Warning** Everything in this chapter and chapter 4 happens in the **Seoul** region. In another region, the server would be created in another country.

## 3.2 Create the two policies (fast way: one command)

Paste three short lines into **CloudShell** (the black command window inside the AWS console) and both policies are created at once. This is the recommended way. Use the manual way in 3.3 only if you cannot use CloudShell.

1. In the bar at the top, click the **CloudShell** icon (it looks like `>_`). It sits near the right of the search box.
2. A black window opens at the bottom (or in a new tab). The first time, it can take 1–2 minutes to get ready. Wait until you see a line where you can type (a `$` mark).
3. Copy all three lines. You can paste them together.

   ```
   R=https://raw.githubusercontent.com/EricWijesinghe
   curl -fsSLO $R/Dahae/master/deploy/aws/launch.sh
   bash launch.sh --admin
   ```

4. Click once in the black CloudShell window.
5. Paste (Windows: **Ctrl+Shift+V** or right-click > **Paste**; Mac: **Cmd+V**).
6. Press **Enter** on the keyboard.
7. Wait a few seconds.

![CloudShell after the policy command](../img/en/aws-cloudshell-admin.png)

**You should now see:** these three lines.

```
created policy dahae-boundary
created policy dahae-deployer-policy
Next: IAM > Users > the deployer's user > attach dahae-deployer-policy and AWSCloudShellFullAccess (not dahae-boundary).
```

- If you ran it before, you see `policy dahae-boundary already exists` instead of `created`. That is also fine.
- Once the policies exist, skip 3.3 and go to **3.4**.

## 3.3 Create the two policies (manual way)

> Skip this section if you finished 3.2.

Get the policy text (JSON) from the developer, or copy it from the files `deploy/aws/dahae-boundary-policy.json` and `deploy/aws/deployer-iam-policy.json` in the Dahae repository.

### The permissions-boundary policy `dahae-boundary`

1. Type **IAM** in the search box at the top and click **IAM**.
2. In the left menu, click **Policies**.
3. Click **Create policy**.
4. Click **JSON**.
5. Delete everything in the editor and paste the **whole** content of `dahae-boundary-policy.json`.
6. Click **Next**.
7. For the policy name, type exactly `dahae-boundary`.
8. Click **Create policy**.

### The deployer policy `dahae-deployer-policy`

1. Click **Policies > Create policy > JSON** again.
2. Delete everything in the editor and paste the **whole** content of `deployer-iam-policy.json`.
3. Click **Next**.
4. For the policy name, type exactly `dahae-deployer-policy`.
5. Click **Create policy**.

**You should now see:** searching the **Policies** list for `dahae` shows two policies, `dahae-boundary` and `dahae-deployer-policy`.

## 3.4 Create the deployer account (IAM user)

1. Type **IAM** in the search box at the top and click **IAM**.
2. In the left menu, click **Users**.
3. Click **Create user**.
4. In **User name**, type `dahae-deployer`.
5. Tick **Provide user access to the AWS Management Console**.
6. For the password, choose **Custom password**.
7. Type a strong password. A password manager can make one for you. Save it in the password manager right away.
8. "Users must create a new password at next sign-in" can be ticked or not.
9. Click **Next**.

![IAM create user screen](../img/en/aws-iam-create-user.png)

## 3.5 Attach the policies to the deployer account

1. On **Set permissions**, choose **Attach policies directly**.
2. In the search box above the policy list, type `dahae-deployer-policy`.
3. Tick the box to the left of `dahae-deployer-policy`.
4. Clear the search box and type `AWSCloudShellFullAccess`.
5. Tick the box to the left of `AWSCloudShellFullAccess`. (The one-command launch in chapter 4 needs it.)
6. **Do not tick** `dahae-boundary`.
7. Click **Next**.
8. On the review page, click **Create user**.

![Attach policies directly: only two ticked](../img/en/aws-iam-attach-policies.png)

**You should now see:** `dahae-deployer` in the user list. Clicking the user shows only `dahae-deployer-policy` and `AWSCloudShellFullAccess` on the **Permissions** tab.

> **Warning** **Do not create** access keys. Dahae works with temporary permissions, without keys.

## 3.6 Register MFA (two-step sign-in) on a call with the developer

The deployer policy does not let the user register MFA alone, so the owner registers it. The authenticator app goes on the **developer's phone**.

1. **Call the developer.** Stay on the call.
2. In the IAM **Users** list, click `dahae-deployer`.
3. Click the **Security credentials** tab.
4. Click **Assign MFA device**.
5. Type a device name (for example `developer-phone`). If there is no name box, skip this.
6. Choose **Authenticator app**.
7. Click **Next**.
8. Click **Show QR code**. A QR code appears.
9. Tell the developer "I'm showing you the QR code", and let the developer scan it with the authenticator app on the developer's own phone, through screen sharing or a video call.
10. The developer reads out the **6-digit number** in the app. Type it in **MFA code 1**.
11. When the number changes (after about 30 seconds), the developer reads out the **next 6-digit number**. Type it in **MFA code 2**.
12. Click **Add MFA**.

![Assign MFA device (QR covered)](../img/en/aws-iam-mfa.png)

**You should now see:** one device in the **Multi-factor authentication (MFA)** list on the **Security credentials** tab.

> **Note** Before MFA is on, server work with this account is refused. The policy blocks it on purpose. It is not a fault.

## 3.7 Send the account details to the developer

1. Click the account name at the top right of the AWS console.
2. Find the **Account ID** (12 digits).
3. Send these three things to the developer. **Send the password separately, through a different channel.**

| What to send | Example | How to send it |
|---|---|---|
| Sign-in URL | `https://123456789012.signin.aws.amazon.com/console` (the number = Account ID) | A message is fine |
| User name | `dahae-deployer` | A message is fine |
| Password | (the one made in 3.4) | Read it out on a call, or share it from the password manager. **Never in chat** |

MFA is already set up on the developer's phone in 3.6.

**You should now see:** the developer tells you they signed in with the deployer account, MFA code included.

## If something goes wrong

| If this happens | Do this |
|---|---|
| You cannot see the CloudShell icon | Check the region is Seoul. On a narrow screen the icon can hide; widen the browser window, or type **CloudShell** in the search box |
| CloudShell shows `AccessDenied` | Check you signed in as an administrator of the company account. A user with fewer permissions cannot create policies |
| You pasted and nothing happened | Check you pressed **Enter**, and that all three lines were pasted |
| You typed a policy name wrong | Delete the wrong policy from the **Policies** list and create it again with the exact name |
| You attached `dahae-boundary` to the user | On the user's **Permissions** tab, **Remove** `dahae-boundary` |
| The MFA codes are rejected | Check the developer's phone clock is set to automatic, then get two new consecutive numbers |
| The developer can sign in but cannot do anything | Check MFA was registered (3.6). Without MFA the policy blocks everything |
