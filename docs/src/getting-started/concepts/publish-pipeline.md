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

- The registry concept itself — see [The Registry](../../reference/registry.md).
- Manifest format (`manifest.toml`) — see [The Manifest Format](../../reference/manifest.md).
- Index format (`index.json`) — see [The Registry Index Format](../../reference/registry-index.md).
- The four-step pipeline shape listed at the top of this page.
- The immutability rule: once `(name, version)` is published, only `yanked` can change.

## How to read a recipe

Recipe filenames use the pattern `<registry>--<ci>.md`. Pick a recipe whose filename matches your registry backend and CI runner. Inside, pick `## New` if you are starting a repository from scratch, or `## Migrate` if you are adding the SDK alongside an existing `gh:` distribution. Repo host and visibility differences appear inline within the steps.

Available recipes:

- [GitHub Releases + GitHub Actions](../recipes/ghreleases--ghactions.md)

Back to [Getting Started](../).
