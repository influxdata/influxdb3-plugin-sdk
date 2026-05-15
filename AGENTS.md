# Agent Instructions for influxdb3-plugin-sdk

This document is the source of truth for AI agents working on this repo. All agents (Claude, Copilot, Codex, etc.) must follow these instructions.

## Before making any change

Read and understand:

- `CONTRIBUTING.md` - contribution guidance, versioning model, cascade rules, and stability tiers
- `RELEASE.md` - release procedure; do not cut releases without explicit user authorization
- `.github/PULL_REQUEST_TEMPLATE.md` - canonical PR checklist for required checks and applicable review items

## Change checklist

When modifying this repo, verify every applicable item in `.github/PULL_REQUEST_TEMPLATE.md` before pushing.

Keep documentation in sync with code and process changes. In particular, consider whether changes require updates to:

- `README.md`
- crate READMEs
- `CONTRIBUTING.md`
- `RELEASE.md`
- `.github/RELEASE_CHECKLIST.md`
- `.github/PULL_REQUEST_TEMPLATE.md`
- `AGENTS.md`

## Non-negotiable release and publishing rules

- Do not set `publish = true` on any crate. This is gated on security and legal review. The crates are `publish = false` by design.
- Do not modify the tag format (`vX.Y.Z`) without updating the justfile, `.circleci/config.yml` tag filter, `RELEASE.md`, and `.github/RELEASE_CHECKLIST.md` in lockstep.
- Do not cut a release, run `just tag-version`, or push a `v*` tag without explicit user authorization. Releases trigger the full build and publish pipeline.
