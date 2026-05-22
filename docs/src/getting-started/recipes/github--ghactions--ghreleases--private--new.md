# Create A New Private Plugin Registry

This recipe bootstraps a new private GitHub plugin source repository that publishes versioned InfluxDB 3 plugin artifacts to a private GitHub Release by using GitHub Actions.

Use this path when you are starting a new plugin repository and want SDK-published artifacts from the beginning.

## Prerequisites

- A GitHub account that can create private repositories.
- The `gh` CLI installed and authenticated.
- The `influxdb3-plugin` CLI, installed with the current path from [Install the CLI](../install.md).

## Step 1: Create The Repository

Create a private plugin source repository:

```bash
PLUGIN_REPO="YOUR_ORG/my-private-plugins"

gh repo create "${PLUGIN_REPO}" --private --clone
cd my-private-plugins
```

Create the default plugin directory layout:

```bash
mkdir -p plugins
influxdb3-plugin new process_writes plugins/downsampler --name downsampler
```

The scaffold writes:

```text
plugins/downsampler/
  manifest.toml
  __init__.py
  README.md
```

## Step 2: Author The Manifest

Open `plugins/downsampler/manifest.toml` and set the plugin metadata.

Minimal shape:

```toml
manifest_schema_version = "1.1"

[plugin]
name = "downsampler"
version = "0.1.0"
description = "Downsample incoming writes."
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.2.0,<4.0.0"
```

See [Manifest format](../../reference/manifest.md) for all fields and validation rules.

## Step 3: Create The Registry Release

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

## Step 4: Seed The Index

Generate and upload the initial empty registry index:

```bash
SEED_DIR="$(mktemp -d)"
influxdb3-plugin new index "${SEED_DIR}" --artifacts-url "${ARTIFACTS_URL}"
gh release upload "${REGISTRY_TAG}" "${SEED_DIR}/index.json" --repo "${REGISTRY_REPO}"
```

See [Index format](../../reference/registry-index.md) for the index schema.

## Step 5: Add The GitHub Actions Workflow

Create the workflow directory and copy the template:

```bash
mkdir -p .github/workflows
curl -fsSLo .github/workflows/publish.yml \
  https://raw.githubusercontent.com/influxdata/influxdb3-plugin-sdk/main/docs/src/templates/github-actions/publish.yml
```

Edit `.github/workflows/publish.yml`:

```yaml
env:
  SDK_VERSION: "X.Y.Z"
  PLUGIN_ROOT: "plugins"
  REGISTRY_REPO: "YOUR_ORG/my-private-plugins"
  REGISTRY_TAG: "plugin-registry"
```

The template walkthrough is [GitHub Actions publish workflow](../../templates/github-actions/).

## Step 6: Configure Authentication

Create a fine-grained GitHub personal access token:

- Resource owner: `YOUR_ORG`.
- Repository access: `my-private-plugins`.
- Repository permissions: Contents read and write.

Save it as a repository secret:

```bash
gh secret set GH_RELEASE_TOKEN --repo "${PLUGIN_REPO}"
```

## Step 7: Trigger The First Publish

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
- `downsampler-0.1.0.tar.gz`

Verify the registry locally:

```bash
gh release download "${REGISTRY_TAG}" \
  --repo "${REGISTRY_REPO}" \
  --pattern index.json \
  --dir /tmp/downsampler-registry \
  --clobber

influxdb3-plugin search --index /tmp/downsampler-registry/index.json downsampler
```

## Step 8: Verify Installation

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

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `gh release upload` returns 404 | The token cannot access the registry repo. | Check `GH_RELEASE_TOKEN` and repository permissions. |
| Workflow says the version is already present | The same `(name, version)` is already in `index.json`. | Bump `plugin.version` in `manifest.toml`. |
| `influxdb3-plugin package` fails validation | Manifest metadata and Python entry points do not match. | Run `influxdb3-plugin validate plugins/downsampler` locally. |
| No plugins are packaged | `PLUGIN_ROOT` points at the wrong directory. | Set `PLUGIN_ROOT` to the directory containing plugin subdirectories. |

Back to [Getting Started](../).

Next: [Migrate from `gh:`](./github--ghactions--ghreleases--private--migrate.md).
