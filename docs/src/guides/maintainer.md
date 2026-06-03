# Guide - Plugin Repository Maintainer

As a maintainer, you must make the following decisions:
- where to host the plugin repository
- where to host the plugin registry
- which CI/CD tools to use for packaging and publishing plugins
- should the repository and registry be public or private

The SDK and registry are flexible and agnostic to hosting solutions, so you could use any combination of the following:

- Repo Hosts: GitHub, GitLab, Azure DevOps, or any code hosting platform or VCS.
- CI Runners: GitHub Actions, GitLab CI, Azure Pipelines, CircleCI, Jenkins, Buildkite, or any CI that can run CLI commands.
- Registry Hosts: S3, GitHub Releases, GitLab Releases, Azure DevOps Artifacts, or any HTTP server ([supported URL schemes documented here](../reference/registry.md#supported-url-schemes)).
- Both the repo and registry can be private or public. 

If you already have a repo, that's ok, you can use it as-is without breaking existing plugin consumers. 

This guide assumes that you already have a GitHub plugin repo and want to publish a registry to GitHub Releases using GitHub Actions.


initialize
- access tokens
- create registry release
plugin repo shape
- directory per plugin (single file vs multi-file?)
- manifest



## Common steps

### Step 1: Author the manifest

Author `plugins/downsampler/manifest.toml`. If you followed the New path, the scaffold wrote a stub for you to fill in; if you followed the Migrate path, create the file now.

Minimal shape:

```toml
manifest_schema_version = "1.2"

[plugin]
name = "downsampler"
version = "0.1.0"
description = "Downsample incoming writes."
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.2.0,<4.0.0"
```

The `triggers` array must match the functions implemented by the Python file. See [The Manifest Format](../reference/manifest.md) for all fields and validation rules.

Validate locally before wiring CI:

```bash
influxdb3-plugin validate plugins/downsampler
```

### Step 2: Create the registry release

Use one GitHub Release as the registry. The release stores `index.json` and all `{name}-{version}.tar.gz` artifacts.

```bash
REGISTRY_REPO="${PLUGIN_REPO}"
REGISTRY_TAG="plugin-registry"
ARTIFACTS_URL="https://github.com/${REGISTRY_REPO}/releases/download/${REGISTRY_TAG}"

gh release create "${REGISTRY_TAG}" \
  --repo "${REGISTRY_REPO}" \
  --title "Plugin Registry" \
  --notes "Plugin registry index and artifacts"
```

### Step 3: Seed the index

Generate and upload the initial empty registry index:

```bash
SEED_DIR="$(mktemp -d)"
influxdb3-plugin new index "${SEED_DIR}" --artifacts-url "${ARTIFACTS_URL}"
gh release upload "${REGISTRY_TAG}" "${SEED_DIR}/index.json" --repo "${REGISTRY_REPO}"
```

See [The Registry Index Format](../reference/registry-index.md) for the index schema.

### Step 4: Add the GitHub Actions workflow

Create the workflow directory, then copy the [Publish workflow](#publish-workflow) shown below into `.github/workflows/publish.yml`:

```bash
mkdir -p .github/workflows
```

Edit `.github/workflows/publish.yml` and set the `env` values for your repository:

```yaml
env:
  SDK_VERSION: "X.Y.Z"
  PLUGIN_ROOT: "plugins"
  REGISTRY_REPO: "YOUR_ORG/my-private-plugins"
  REGISTRY_TAG: "plugin-registry"
```

### Step 5: Configure authentication

Create a fine-grained GitHub personal access token:

- Resource owner: `YOUR_ORG`.
- Repository access: `${REGISTRY_REPO}`.
- Repository permissions: Contents read and write.

Save it as a repository secret:

```bash
gh secret set GH_RELEASE_TOKEN --repo "${PLUGIN_REPO}"
```

### Step 6: Trigger the first publish

Commit and push:

```bash
git add .
git commit -m "Add initial plugin registry"
git push origin main
```

Watch the workflow:

```bash
gh run list --repo "${PLUGIN_REPO}" --workflow publish.yml
```

After it succeeds, the registry release contains:

- `index.json`
- one `{name}-{version}.tar.gz` artifact for each newly published plugin version

Verify the registry locally:

```bash
gh release download "${REGISTRY_TAG}" \
  --repo "${REGISTRY_REPO}" \
  --pattern index.json \
  --dir /tmp/plugin-registry \
  --clobber

influxdb3-plugin search --index /tmp/plugin-registry/index.json downsampler
```

### Step 7: Verify installation

Download and extract the published artifact:

```bash
gh release download "${REGISTRY_TAG}" \
  --repo "${REGISTRY_REPO}" \
  --pattern "downsampler-0.1.0.tar.gz" \
  --dir /tmp/downsampler-artifact \
  --clobber

mkdir -p /tmp/downsampler-extract
tar -xzf /tmp/downsampler-artifact/downsampler-0.1.0.tar.gz -C /tmp/downsampler-extract
find /tmp/downsampler-extract -maxdepth 2 -type f | sort
```

The archive extracts to a top-level `downsampler-0.1.0/` directory. Copy that directory, or its contents, into the plugin directory configured for your InfluxDB 3 host.

If you use the HTTP API path instead of a manual file move, extract the archive first and send the extracted file entries to `PUT /api/v3/plugins/directory`. Do not send the tarball bytes to `/api/v3/plugins/files`; that endpoint accepts single-file content, not plugin archives.

# How Publish Pipelines Vary

Every plugin publish pipeline does the same four things:

1. Validate the plugin directory.
2. Package the plugin into a `<name>-<version>.tar.gz` artifact.
3. Upload the artifact to the registry backend.
4. Upload the updated `index.json` to the registry backend.

The differences between pipelines live in dimensions the recipes encode in their filenames or describe inline. This page names those dimensions so a reader can choose a recipe with the right mental model.

## Dimensions

### Registry backend (primary)

The registry backend determines authentication, upload primitive, URL shape, and rollback story. This is the dimension that drives recipe choice.

| Backend | Upload primitive | URL shape | Rollback |
|---|---|---|---|
| GitHub Releases | `gh release upload --clobber` | `https://github.com/{org}/{repo}/releases/download/{tag}/...` | Re-upload a previous `index.json` asset |
| S3 | `aws s3api put-object` with `--if-none-match '*'` | `https://{bucket}.s3.{region}.amazonaws.com/...` | Object versioning + `copy-object --version-id` |
| GCS | `gsutil cp` with generation match | `https://storage.googleapis.com/{bucket}/...` | Object versioning |
| Generic HTTPS | Out-of-band (rsync, scp) | Whatever the operator chooses | Backend-specific |

### CI runner (secondary)

The CI runner determines YAML syntax, secret plumbing, and the concurrency primitive that prevents two publish runs from racing on the same registry.

| Runner | Secret plumbing | Concurrency primitive |
|---|---|---|
| GitHub Actions | `secrets.X` or OIDC `id-token: write` | `concurrency: { group: ..., cancel-in-progress: false }` |
| GitLab CI | `CI/CD variables` | `resource_group:` |
| CircleCI | Project env vars or contexts | Workflow-level `serial` |
| Jenkins | Credentials binding plugin | `lock` step from Lockable Resources |

### Repo host (inline variation)

The repo host changes the `git clone` URL and the shape of any personal access token used for index push. Recipes call out the differences inline rather than fragmenting along this dimension.

### Visibility (inline variation)

Public registries do not require authentication for download. Private registries require a token at fetch time. Recipes call out the token shape inline.

### Starting state (recipe section)

A repository either has no existing plugin distribution (`new`) or already distributes via the legacy `gh:` prefix mechanism (`migrate`). The recipe steps for these two states share the manifest authoring, registry setup, workflow installation, authentication, and verification sections. They differ only in repository preparation. Each recipe carries both states as `## New` and `## Migrate` sections so a reader picks the entry point that matches their state and follows shared steps from there.

## What stays the same across every pipeline

- The registry concept itself — see [The Registry](../reference/registry.md).
- Manifest format (`manifest.toml`) — see [The Manifest Format](../reference/manifest.md).
- Index format (`index.json`) — see [The Registry Index Format](../reference/registry-index.md).
- The four-step pipeline shape listed at the top of this page.
- The immutability rule: once `(name, version)` is published, only `yanked` can change.

## How to read a recipe

Recipe filenames use the pattern `<registry>--<ci>.md`. Pick a recipe whose filename matches your registry backend and CI runner. Inside, pick `## New` if you are starting a repository from scratch, or `## Migrate` if you are adding the SDK alongside an existing `gh:` distribution. Repo host and visibility differences appear inline within the steps.

### Publish workflow

```yaml
name: Publish InfluxDB 3 plugins

on:
  push:
    branches:
      - main
  workflow_dispatch:

permissions:
  contents: read

env:
  # Update these values for your repository.
  SDK_VERSION: "X.Y.Z"
  PLUGIN_ROOT: "plugins"
  REGISTRY_REPO: "YOUR_ORG/YOUR_REGISTRY_REPO"
  REGISTRY_TAG: "plugin-registry"

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Install Rust
        run: |
          rustup set profile minimal
          rustup toolchain install stable
          rustup default stable

      - name: Install InfluxDB 3 plugin SDK
        run: cargo install influxdb3-plugin-cli --version "${SDK_VERSION}" --locked

      - name: Fetch current registry index
        env:
          GH_TOKEN: ${{ secrets.GH_RELEASE_TOKEN }}
        run: |
          set -euo pipefail
          mkdir -p .publish/current
          gh release download "${REGISTRY_TAG}" \
            --repo "${REGISTRY_REPO}" \
            --pattern index.json \
            --dir .publish/current \
            --clobber
          test -f .publish/current/index.json

      - name: Validate and package new plugin versions
        shell: bash
        run: |
          set -euo pipefail

          current_index=".publish/current/index.json"
          next_index=".publish/next-index.json"
          artifacts_dir=".publish/artifacts"
          package_out=".publish/package-out"

          cp "${current_index}" "${next_index}"
          mkdir -p "${artifacts_dir}"

          mapfile -t plugin_dirs < <(
            find "${PLUGIN_ROOT}" -mindepth 1 -maxdepth 1 -type d -print | sort
          )

          if [ "${#plugin_dirs[@]}" -eq 0 ]; then
            echo "No plugin directories found under ${PLUGIN_ROOT}" >&2
            exit 1
          fi

          packaged_count=0

          for plugin_dir in "${plugin_dirs[@]}"; do
            if [ ! -f "${plugin_dir}/manifest.toml" ]; then
              echo "Skipping ${plugin_dir}: no manifest.toml"
              continue
            fi

            manifest_id="$(python3 - "${plugin_dir}/manifest.toml" <<'PY'
          import sys
          import tomllib

          with open(sys.argv[1], "rb") as f:
              manifest = tomllib.load(f)

          plugin = manifest["plugin"]
          print(f"{plugin['name']}@{plugin['version']}")
          PY
            )"

            if python3 - "${plugin_dir}/manifest.toml" "${next_index}" <<'PY'
          import json
          import sys
          import tomllib

          with open(sys.argv[1], "rb") as f:
              manifest = tomllib.load(f)
          with open(sys.argv[2], "r", encoding="utf-8") as f:
              index = json.load(f)

          name = manifest["plugin"]["name"]
          version = manifest["plugin"]["version"]
          for entry in index.get("plugins", []):
              if entry.get("name") == name and entry.get("version") == version:
                  sys.exit(0)
          sys.exit(1)
          PY
            then
              echo "Skipping ${manifest_id}: already present in registry index"
              continue
            fi

            echo "Packaging ${manifest_id}"
            rm -rf "${package_out}"
            mkdir -p "${package_out}"

            influxdb3-plugin validate "${plugin_dir}" \
              --index "${next_index}" \
              --output json

            influxdb3-plugin package "${plugin_dir}" \
              --index "${next_index}" \
              --out "${package_out}" \
              --output json

            cp "${package_out}/index.json" "${next_index}"
            find "${package_out}" -maxdepth 1 -name "*.tar.gz" -exec cp {} "${artifacts_dir}/" \;
            packaged_count=$((packaged_count + 1))
          done

          cp "${next_index}" .publish/index.json
          echo "packaged_count=${packaged_count}" >> "${GITHUB_ENV}"

      - name: Upload new artifacts
        if: env.packaged_count != '0'
        env:
          GH_TOKEN: ${{ secrets.GH_RELEASE_TOKEN }}
        run: |
          set -euo pipefail
          while IFS= read -r -d '' artifact; do
            gh release upload "${REGISTRY_TAG}" "${artifact}" \
              --repo "${REGISTRY_REPO}"
          done < <(find .publish/artifacts -maxdepth 1 -name "*.tar.gz" -print0 | sort -z)

      - name: Upload updated index
        if: env.packaged_count != '0'
        env:
          GH_TOKEN: ${{ secrets.GH_RELEASE_TOKEN }}
        run: |
          set -euo pipefail
          gh release upload "${REGISTRY_TAG}" .publish/index.json \
            --repo "${REGISTRY_REPO}" \
            --clobber

      - name: Report no-op
        if: env.packaged_count == '0'
        run: echo "No unpublished plugin versions found; registry index unchanged."

```
---

Back: [Guides](./README.md) | Next: [Plugin Author](./author.md)
