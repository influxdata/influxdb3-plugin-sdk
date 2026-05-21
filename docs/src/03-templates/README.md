# Templates

The templates section contains runnable files that plugin repository maintainers can copy into their own repositories.

The v1 template is a GitHub Actions workflow for private GitHub Release registries. It installs the SDK CLI, fetches the current index, validates and packages plugin directories, uploads artifacts, and replaces the registry index.

| Template | Purpose | Status |
|---|---|---|
| [GitHub Actions publish workflow](./github-actions/) | Publish plugin artifacts and `index.json` to a private GitHub Release. | Current |

For the repository scenarios that use this template, see [Getting Started](../01-getting-started/).
