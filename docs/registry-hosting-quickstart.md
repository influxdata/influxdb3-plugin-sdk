# Registry Hosting Quickstart

How to host a public plugin registry. Two options: S3 or GitHub Releases. Both serve the same static files — `index.json` and `{name}-{version}.tar.gz` artifacts.

For design rationale, see `public-registry-hosting.md`.

## Option A: S3

### 1. Create and configure the bucket

```bash
BUCKET="your-bucket-name"
REGION="us-east-2"

aws s3api create-bucket --bucket $BUCKET --region $REGION \
  --create-bucket-configuration LocationConstraint=$REGION

aws s3api put-bucket-ownership-controls --bucket $BUCKET \
  --ownership-controls 'Rules=[{ObjectOwnership=BucketOwnerEnforced}]'

aws s3api put-bucket-versioning --bucket $BUCKET \
  --versioning-configuration Status=Enabled

aws s3api put-bucket-encryption --bucket $BUCKET \
  --server-side-encryption-configuration \
  '{"Rules":[{"ApplyServerSideEncryptionByDefault":{"SSEAlgorithm":"AES256"}}]}'

aws s3api put-public-access-block --bucket $BUCKET \
  --public-access-block-configuration \
  '{"BlockPublicAcls":true,"IgnorePublicAcls":true,"BlockPublicPolicy":false,"RestrictPublicBuckets":false}'

aws s3api put-bucket-policy --bucket $BUCKET --policy "{
  \"Version\":\"2012-10-17\",
  \"Statement\":[{
    \"Sid\":\"PublicReadGetObject\",\"Effect\":\"Allow\",
    \"Principal\":\"*\",\"Action\":\"s3:GetObject\",
    \"Resource\":\"arn:aws:s3:::${BUCKET}/*\"
  }]
}"
```

### 2. Seed the index

```bash
influxdb3-plugin new index . --artifacts-url "https://${BUCKET}.s3.${REGION}.amazonaws.com/artifacts"
aws s3 cp index.json s3://${BUCKET}/index.json --content-type "application/json"
```

### 3. Set up CI auth (GitHub Actions OIDC)

```bash
ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)

# OIDC provider (skip if already exists)
aws iam create-open-id-connect-provider \
  --url "https://token.actions.githubusercontent.com" \
  --client-id-list "sts.amazonaws.com" \
  --thumbprint-list "6938fd4d98bab03faadb97b34396831e3780aea1"

# Policy
aws iam create-policy --policy-name "${BUCKET}-publish" --policy-document "{
  \"Version\":\"2012-10-17\",
  \"Statement\":[
    {\"Effect\":\"Allow\",\"Action\":[\"s3:GetObject\",\"s3:PutObject\"],\"Resource\":\"arn:aws:s3:::${BUCKET}/*\"},
    {\"Effect\":\"Allow\",\"Action\":\"s3:GetObject\",\"Resource\":\"arn:aws:s3:::${BUCKET}\"}
  ]
}"

# Role (update repo in the sub condition)
aws iam create-role --role-name "${BUCKET}-ci" --assume-role-policy-document "{
  \"Version\":\"2012-10-17\",
  \"Statement\":[{
    \"Effect\":\"Allow\",
    \"Principal\":{\"Federated\":\"arn:aws:iam::${ACCOUNT_ID}:oidc-provider/token.actions.githubusercontent.com\"},
    \"Action\":\"sts:AssumeRoleWithWebIdentity\",
    \"Condition\":{
      \"StringEquals\":{\"token.actions.githubusercontent.com:aud\":\"sts.amazonaws.com\"},
      \"StringLike\":{\"token.actions.githubusercontent.com:sub\":\"repo:YOUR_ORG/YOUR_REPO:*\"}
    }
  }]
}"

aws iam attach-role-policy --role-name "${BUCKET}-ci" \
  --policy-arn "arn:aws:iam::${ACCOUNT_ID}:policy/${BUCKET}-publish"
```

### 4. Workflow config

```yaml
permissions:
  id-token: write
  contents: read

env:
  S3_BUCKET: your-bucket-name
  AWS_REGION: us-east-2
  AWS_ROLE_ARN: arn:aws:iam::ACCOUNT:role/your-bucket-name-ci
```

Workflow steps: fetch index from S3, run publish script, upload artifacts then index.

### 5. Public URLs

```
index:     https://{bucket}.s3.{region}.amazonaws.com/index.json
artifacts: https://{bucket}.s3.{region}.amazonaws.com/artifacts/{name}-{version}.tar.gz
```

### Rollback

S3 versioning is enabled. Restore a previous `index.json`:

```bash
aws s3api list-object-versions --bucket $BUCKET --prefix index.json
aws s3api copy-object --bucket $BUCKET --key index.json \
  --copy-source "${BUCKET}/index.json?versionId=PREVIOUS_VERSION_ID"
```

---

## Option B: GitHub Releases

Artifacts and the index are stored as assets on a single GitHub Release. Works on public repos (unauthenticated download) and private/internal repos (org members download via browser or `gh` CLI).

### 1. Create the release

```bash
gh release create registry --repo YOUR_ORG/YOUR_REPO \
  --title "Plugin Registry" --notes "Plugin registry index and artifacts"
```

### 2. Seed the index

```bash
RELEASE_URL="https://github.com/YOUR_ORG/YOUR_REPO/releases/download/registry"
influxdb3-plugin new index . --artifacts-url "$RELEASE_URL"
gh release upload registry index.json --repo YOUR_ORG/YOUR_REPO
```

### 3. CI auth

Create a fine-grained PAT:
- **Resource owner:** your org
- **Repository access:** the target repo only
- **Permissions:** Contents read+write

Add as a repo secret on the repo where the workflow runs:

```bash
gh secret set GH_RELEASE_TOKEN --repo YOUR_ORG/YOUR_WORKFLOW_REPO
```

### 4. Workflow config

```yaml
env:
  GH_RELEASE_REPO: YOUR_ORG/YOUR_REPO
  GH_RELEASE_TAG: registry
```

Workflow steps after the publish script runs:

```yaml
- name: Publish to GitHub Release
  env:
    GH_TOKEN: ${{ secrets.GH_RELEASE_TOKEN }}
  run: |
    GH_ARTIFACTS_URL="https://github.com/$GH_RELEASE_REPO/releases/download/$GH_RELEASE_TAG"
    mkdir -p /tmp/gh-release

    # Generate index with GitHub artifacts_url
    python3 -c "
    import json
    with open('/tmp/publish-output/index.json') as f:
        idx = json.load(f)
    idx['artifacts_url'] = '${GH_ARTIFACTS_URL}'
    with open('/tmp/gh-release/index.json', 'w') as f:
        json.dump(idx, f, indent=2)
        f.write('\n')
    "

    # Sync artifacts (upload new, skip existing)
    python3 -c "
    import json
    with open('/tmp/publish-output/index.json') as f:
        idx = json.load(f)
    for p in idx['plugins']:
        print(f\"{p['name']}-{p['version']}.tar.gz\")
    " | while read artifact_name; do
      if gh release view "$GH_RELEASE_TAG" --repo "$GH_RELEASE_REPO" \
          --json assets --jq ".assets[].name" | grep -qF "$artifact_name"; then
        continue
      fi
      if [ -f "/tmp/publish-output/artifacts/$artifact_name" ]; then
        gh release upload "$GH_RELEASE_TAG" "/tmp/publish-output/artifacts/$artifact_name" \
          --repo "$GH_RELEASE_REPO" || true
      fi
    done

    # Upload index (overwrite)
    gh release upload "$GH_RELEASE_TAG" /tmp/gh-release/index.json \
      --repo "$GH_RELEASE_REPO" --clobber
```

### 5. Public URLs

```
index:     https://github.com/{org}/{repo}/releases/download/{tag}/index.json
artifacts: https://github.com/{org}/{repo}/releases/download/{tag}/{name}-{version}.tar.gz
```

Private/internal repos: same URLs work for authenticated org members (browser session or `gh` CLI).

### Limitations

- 2 GiB max per asset (not a concern for plugin archives)
- No hard cap on asset count or release count
- Draft releases are not downloadable — use full or prerelease visibility
- `gh release upload` uses the source filename — the file on disk must be named `index.json`, not a temp name

---

## Using Both

S3 as the source of truth, GitHub Releases as a mirror. The workflow runs the publish script once against the S3 index, uploads to S3, then syncs artifacts and a rewritten index to the GitHub Release. See `influxdata/influxdb3_plugins` branch `demo/public-registry-mvp` for a working example.
