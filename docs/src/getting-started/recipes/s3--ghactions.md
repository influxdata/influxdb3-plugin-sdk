# S3 + GitHub Actions

> **Status:** Planned. This recipe is scaffolded but not yet written.
>
> The shipped recipe is [GitHub Releases + GitHub Actions](./ghreleases--ghactions.md). For the model behind recipe choice, see [How publish pipelines vary](../concepts/publish-pipeline.md).

## Scenario

- Registry backend: an Amazon S3 bucket that stores `index.json` and `{name}-{version}.tar.gz` artifacts. Public-read or private with consumer-side credentials.
- CI runner: GitHub Actions on the plugin source repository, authenticated to AWS via OIDC role assumption or a static access key.
- Repo host: any.
- Visibility: public or private.

## Outline (pending)

Will follow the same structure as the shipped recipe:

- `## Prerequisites`
- `## New`
- `## Migrate`
- `## Common steps` — manifest, bucket setup, seed index, workflow, AWS auth, first publish, verify
- `## Coexistence with `gh:``
- `## Public visibility`
- `## Troubleshooting`

Backend specifics that this recipe will cover when written:

- Bucket policy (object ownership, public access block, server-side encryption, versioning).
- OIDC trust relationship between GitHub Actions and AWS IAM.
- Conditional upload (`aws s3api put-object --if-none-match '*'`) for safe concurrent publishes.
- Rollback via S3 object versioning and `copy-object --version-id`.

Source material for the S3 hosting steps lives in `docs/internal/registry-hosting-quickstart.md` (S3 section).

Back to [Getting Started](../).
