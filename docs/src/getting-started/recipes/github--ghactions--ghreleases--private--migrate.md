# Migrate From `gh:` To A Private Registry

This recipe migrates an existing GitHub plugin repository that is distributed with the `gh:` prefix mechanism into a private GitHub Release registry published by GitHub Actions.

The migration is additive. Existing `gh:` consumers continue to work because this recipe does not remove or rewrite the current source files.

## Prerequisites

- A GitHub repository that already contains plugin source files consumed with `gh:`.
- Permission to add GitHub Actions workflows and repository secrets.
- The `gh` CLI installed and authenticated.
- The `influxdb3-plugin` CLI, installed with the current path from [Install the CLI](../install.md).

## Step 1: Clone The Existing Repository

```bash
PLUGIN_REPO="YOUR_ORG/existing-plugin-repo"

gh repo clone "${PLUGIN_REPO}"
cd existing-plugin-repo
```

Keep the existing `gh:` source layout in place. Add an SDK packaging area alongside it:

```bash
mkdir -p plugins
```

For each plugin you want to publish through the registry, create one directory under `plugins/`:

```bash
mkdir -p plugins/downsampler
cp path/to/existing/downsampler.py plugins/downsampler/__init__.py
```

If an existing plugin already lives in a directory with `__init__.py`, copy that directory instead:

```bash
cp -R path/to/existing/downsampler plugins/downsampler
```

Do not delete the old files used by `gh:` consumers.

## Step 2: Author The Manifest

Add `plugins/downsampler/manifest.toml`:

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

The `triggers` array must match the functions implemented by the Python file. See [The Manifest Format](../../reference/manifest.md) for the complete schema.

Validate locally before wiring CI:

```bash
influxdb3-plugin validate plugins/downsampler
```

## Step 3: Create The Registry Release

Use one GitHub Release as the registry:

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

See [The Registry Index Format](../../reference/registry-index.md) for the index schema.

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
  REGISTRY_REPO: "YOUR_ORG/existing-plugin-repo"
  REGISTRY_TAG: "plugin-registry"
```

The template walkthrough is [GitHub Actions publish workflow](../../templates/github-actions/).

## Step 6: Configure Authentication

Create a fine-grained GitHub personal access token:

- Resource owner: `YOUR_ORG`.
- Repository access: `existing-plugin-repo`.
- Repository permissions: Contents read and write.

Save it as a repository secret:

```bash
gh secret set GH_RELEASE_TOKEN --repo "${PLUGIN_REPO}"
```

## Step 7: Trigger The First Publish

Commit and push the new SDK packaging area and workflow:

```bash
git add plugins .github/workflows/publish.yml
git commit -m "Publish plugins through SDK registry"
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

influxdb3-plugin search --index /tmp/plugin-registry/index.json
```

## Step 8: Verify Installation

Download and extract one published artifact:

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

## Coexistence With `gh:`

The old `gh:` path and the SDK registry path can exist side by side:

- Existing consumers that reference `gh:` keep using the old source files.
- New consumers can use artifacts and metadata from the registry release.
- The GitHub Actions workflow only reads `PLUGIN_ROOT`; it does not modify old `gh:` files.

Keep both paths until every consumer has moved to the registry.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `validate` reports a missing trigger implementation | The manifest declares a trigger that the copied Python file does not implement. | Update `triggers` or the Python entry point. |
| Workflow says the version is already present | The same `(name, version)` is already in `index.json`. | Bump `plugin.version` in `manifest.toml`. |
| Existing `gh:` users break | Old files were moved or deleted. | Restore the original source layout and keep SDK packaging files alongside it. |
| `gh release download` cannot find `index.json` | The registry release was not seeded. | Run the seed-index step again. |

Back to [Getting Started](../).

Next: [GitHub Actions publish workflow](../../templates/github-actions/).
