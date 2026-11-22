#!/usr/bin/env bash
# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# One-command Dahae server on AWS EC2, run in AWS CloudShell (the terminal in the AWS console; nothing to install).
#
# DODA's AWS admin, once:  curl -fsSL https://raw.githubusercontent.com/EricWijesinghe/Dahae/master/deploy/aws/launch.sh | bash -s -- --admin
#   creates the policies dahae-boundary and dahae-deployer-policy (attach the second one plus AWSCloudShellFullAccess
#   to the deployer's IAM user). Run again, policies that already exist get this version as their default.
# Also runs from Git Bash on Windows (the AWS CLI for Windows): file paths are handed over in Windows form.
# Deployer, to create the server (once; later versions arrive through the deploy-aws workflow):
#   curl -fsSL https://raw.githubusercontent.com/EricWijesinghe/Dahae/master/deploy/aws/launch.sh | bash -s -- \
#     --domain example.co.kr --email owner@example.co.kr
#   No domain yet: --domain auto gives https://<server ip>.sslip.io; move to the real domain later in the console.
# Options: --version vX.Y.Z (default: latest release)  --stack dahae-main  --instance-type t3.medium
#          --hosted-zone-id Z123 (Route 53 A record)  --vpc vpc-… --subnet subnet-… (default: default VPC, public subnet)
#          --region ap-northeast-2  --ref master (where the template and policies come from)  --dry-run
# The release's install.sh is downloaded and checked against the release's SHA256SUMS before its hash is pinned
# into the template; the server refuses to run any other install.sh.
set -euo pipefail

REPO=EricWijesinghe/Dahae
DIST=EricWijesinghe/Dahae-install
AWS=${DAHAE_AWS:-aws}
REGION=ap-northeast-2 REF=master STACK=dahae-main TYPE=t3.medium
DOMAIN="" EMAIL="" VERSION="" ZONE="" VPC="" SUBNET="" ADMIN=0 DRY=0

die() { echo "error: $*" >&2; exit 1; }
need() { [ -n "${2:-}" ] || die "$1 needs a value"; }
while [ $# -gt 0 ]; do
  case "$1" in
    --admin) ADMIN=1 ;;
    --dry-run) DRY=1 ;;
    --domain) need "$1" "${2:-}"; DOMAIN=$2; shift ;;
    --email) need "$1" "${2:-}"; EMAIL=$2; shift ;;
    --version) need "$1" "${2:-}"; VERSION=$2; shift ;;
    --stack) need "$1" "${2:-}"; STACK=$2; shift ;;
    --instance-type) need "$1" "${2:-}"; TYPE=$2; shift ;;
    --hosted-zone-id) need "$1" "${2:-}"; ZONE=$2; shift ;;
    --vpc) need "$1" "${2:-}"; VPC=$2; shift ;;
    --subnet) need "$1" "${2:-}"; SUBNET=$2; shift ;;
    --region) need "$1" "${2:-}"; REGION=$2; shift ;;
    --ref) need "$1" "${2:-}"; REF=$2; shift ;;
    *) die "unknown option $1" ;;
  esac
  shift
done
RAW="https://raw.githubusercontent.com/$REPO/$REF"
aws_() { "$AWS" --region "$REGION" --output text "$@"; }
# The template and the policies have Korean text; the AWS CLI on Windows would read them in the system code page.
export AWS_CLI_FILE_ENCODING=UTF-8
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
# Git Bash (MSYS): aws.exe is a Windows program and cannot open /tmp/...; give it the same folder as C:/...
wwork=$work
if command -v cygpath >/dev/null 2>&1; then wwork=$(cygpath -m "$work"); fi

update_policy() { # update_policy <arn> <file> : the file becomes the default version (at most 5 kept: the oldest other one goes)
  local n old
  n=$(aws_ iam list-policy-versions --policy-arn "$1" --query 'length(Versions)')
  if [ "${n:-0}" -ge 5 ]; then
    old=$(aws_ iam list-policy-versions --policy-arn "$1" --query 'Versions[?IsDefaultVersion==`false`].VersionId' | tr -s '[:space:]' '\n' | sort -V | head -n1)
    aws_ iam delete-policy-version --policy-arn "$1" --version-id "$old" >/dev/null
  fi
  aws_ iam create-policy-version --policy-arn "$1" --policy-document "file://$wwork/$2" --set-as-default >/dev/null
}

if [ "$ADMIN" = 1 ]; then
  acct=$(aws_ sts get-caller-identity --query Account)
  for p in dahae-boundary:dahae-boundary-policy.json dahae-deployer-policy:deployer-iam-policy.json; do
    name=${p%%:*} file=${p#*:} arn="arn:aws:iam::$acct:policy/${p%%:*}"
    curl -fsSL -o "$work/$file" "$RAW/deploy/aws/$file"
    if aws_ iam get-policy --policy-arn "$arn" >/dev/null 2>&1; then
      update_policy "$arn" "$file"
      echo "updated policy $name (new default version)"
    else
      aws_ iam create-policy --policy-name "$name" --policy-document "file://$wwork/$file" >/dev/null
      echo "created policy $name"
    fi
  done
  echo "Next: IAM > Users > the deployer's user > attach dahae-deployer-policy and AWSCloudShellFullAccess (not dahae-boundary)."
  exit 0
fi

[[ "$DOMAIN" == auto || "$DOMAIN" =~ ^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}$ ]] ||
  die "--domain like example.co.kr is required (or auto for a temporary <ip>.sslip.io address)"
[[ "$DOMAIN" != auto || -z "$ZONE" ]] || die "--hosted-zone-id needs a real --domain"
[[ "$EMAIL" =~ ^[A-Za-z0-9._+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$ ]] || die "--email is required"
[[ "$STACK" =~ ^dahae-[A-Za-z0-9-]+$ ]] || die "--stack must start with dahae- (the deployer may only manage dahae-* stacks)"
if [ -z "$VERSION" ]; then
  VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p')
fi
[[ "$VERSION" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "no release version found (use --version vX.Y.Z)"

# The installer the server will run: the release's own copy, checked against the release's SHA256SUMS.
url="https://raw.githubusercontent.com/$DIST/$VERSION/install.sh"
curl -fsSL -o "$work/install.sh" "$url"
curl -fsSL -o "$work/SHA256SUMS" "https://github.com/$REPO/releases/download/$VERSION/SHA256SUMS"
want=$(awk '$2 == "install.sh" || $2 == "*install.sh" {print $1}' "$work/SHA256SUMS")
got=$(sha256sum "$work/install.sh" | cut -d' ' -f1)
[[ -n $want && $want == "$got" ]] || die "install.sh of $VERSION does not match the release's SHA256SUMS"
curl -fsSL -o "$work/src.yaml" "$RAW/deploy/aws/dahae.cfn.yaml"
sed -e "s|__INSTALL_SH_SHA256__|$got|g" -e "s|__INSTALL_URL__|$url|g" "$work/src.yaml" > "$work/dahae-aws.yaml"
! grep -q '__INSTALL_' "$work/dahae-aws.yaml" || die "template placeholder left"

if [ -z "$SUBNET" ]; then
  [ -n "$VPC" ] || VPC=$(aws_ ec2 describe-vpcs --filters Name=isDefault,Values=true --query 'Vpcs[0].VpcId')
  [[ -n $VPC && $VPC != None ]] || die "no default VPC in $REGION: pass --vpc and --subnet"
  SUBNET=$(aws_ ec2 describe-subnets --filters "Name=vpc-id,Values=$VPC" Name=map-public-ip-on-launch,Values=true \
    --query 'sort_by(Subnets,&AvailabilityZone)[0].SubnetId')
  [[ -n $SUBNET && $SUBNET != None ]] || die "no public subnet in $VPC: pass --subnet"
fi
[ -n "$VPC" ] || VPC=$(aws_ ec2 describe-subnets --subnet-ids "$SUBNET" --query 'Subnets[0].VpcId')
acct=$(aws_ sts get-caller-identity --query Account)
aws_ iam get-policy --policy-arn "arn:aws:iam::$acct:policy/dahae-boundary" >/dev/null 2>&1 ||
  die "policy dahae-boundary is missing: an admin runs this script with --admin once first"
oidc=true
aws_ iam get-open-id-connect-provider \
  --open-id-connect-provider-arn "arn:aws:iam::$acct:oidc-provider/token.actions.githubusercontent.com" >/dev/null 2>&1 && oidc=false

params=(Domain="$DOMAIN" Email="$EMAIL" InstanceType="$TYPE" VpcId="$VPC" SubnetId="$SUBNET" CreateGitHubOidcProvider="$oidc")
[ -z "$ZONE" ] || params+=(HostedZoneId="$ZONE")
echo "Dahae $VERSION -> stack $STACK in $REGION (vpc $VPC, subnet $SUBNET, instance $TYPE)"
deploy=(cloudformation deploy --stack-name "$STACK" --template-file "$wwork/dahae-aws.yaml"
  --capabilities CAPABILITY_IAM CAPABILITY_NAMED_IAM --no-fail-on-empty-changeset --parameter-overrides "${params[@]}")
# Create only: a template change on a running stack would stop and start the server without updating it.
# Updates go through `dahae update` / the deploy-aws workflow.
if aws_ cloudformation describe-stacks --stack-name "$STACK" >/dev/null 2>&1; then
  die "stack $STACK already exists; update the server with the deploy-aws workflow (guide 10 C), not this command"
fi
if [ "$DRY" = 1 ]; then echo "dry run: $AWS --region $REGION ${deploy[*]}"; exit 0; fi
"$AWS" --region "$REGION" "${deploy[@]}"
"$AWS" --region "$REGION" cloudformation describe-stacks --stack-name "$STACK" \
  --query 'Stacks[0].Outputs[].[OutputKey,OutputValue]' --output table
if [ "$DOMAIN" = auto ]; then
  DOMAIN=$("$AWS" --region "$REGION" cloudformation describe-stacks --stack-name "$STACK" \
    --query "Stacks[0].Outputs[?OutputKey=='DahaeDomain'].OutputValue" --output text)
  dns="nothing to do: $DOMAIN already points at the server (move to a real domain later in the console)"
else
  dns="A records @ and www -> the PublicIp above (Cloudflare: DNS only; skip if you passed --hosted-zone-id)"
fi
cat <<EOF
Done. Next:
 1. DNS: $dns
 2. Wait about 10 minutes, then open https://$DOMAIN/admin
 3. Owner setup code: EC2 > Instances > the dahae server > Connect > Session Manager, then: sudo dahae setup-code
 4. Automatic deploys: set the GitHub environment doda-aws variables from the outputs above (guide 10 C).
EOF
