# Getting Started

Choose the recipe that matches your registry backend and CI runner. Within a recipe, the `New` and `Migrate from gh:` sections cover starting state.

For the model behind these choices, see [How publish pipelines vary](./concepts/publish-pipeline.md).

| Registry | CI runner | Repo host | Recipe |
|---|---|---|---|
| GitHub Releases | GitHub Actions | GitHub | [ghreleases--ghactions](./recipes/ghreleases--ghactions.md) |
| S3 | GitHub Actions | any | [s3--ghactions](./recipes/s3--ghactions.md) |

`Current` recipes are end-to-end runnable. `Planned` recipes are scaffolded stubs that name the scenario but do not yet contain step-by-step instructions; they exist so the filename convention and cross-link graph are stable as content fills in.

The shipped recipe targets private GitHub repositories that publish a registry to GitHub Releases from GitHub Actions. The migration path is additive: a repository can keep existing `gh:` consumers working while adding SDK-published artifacts and a registry index.

Public visibility is covered inline within each recipe. Additional CI runners (CircleCI, Jenkins, Buildkite) and registry backends slot in by adding a row to the table and a file to `recipes/` without restructuring the existing layout.

## Install the CLI

Install `influxdb3-plugin` before running a recipe. See [Install the CLI](./install.md) for the canonical install commands and current channel guidance.

## What you will build

The v1 recipe produces:

- A plugin source repository with one or more `manifest.toml` files.
- A GitHub Actions workflow that validates and packages plugin artifacts.
- A GitHub Release that stores the registry `index.json` and versioned plugin archives.
- An additive path that does not remove or break existing `gh:` consumers (when migrating).

Next: [Reference overview](../reference/).
