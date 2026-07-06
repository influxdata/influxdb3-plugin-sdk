# The Registry

A registry is a collection of plugin artifacts and a plugin index. The index lists every published plugin version and points at the location where the artifacts are served; the artifacts contain everything needed to execute a plugin.

This page defines what a registry is and how its two halves relate. The on-disk format of `index.json` is specified in [The Registry Index Format](../reference/registry-index.md).

A registry has two parts:

| Part | Form | Role |
|---|---|---|
| Index | An `index.json` file. | Catalog of published plugin versions. |
| Artifacts | One `{name}-{version}.tar.gz` artifact per published plugin version. | Plugin files required to execute on db. |

## Globally Unique Identity

A plugin's identity is globally defined by the tuple `(index_url, name, version)`:

- `index_url` is the location of the registry's `index.json`. It is supplied by the consumer's registry configuration; the index does not declare its own URL.
- `name` and `version` are defined in a plugin's `manifest.toml` and recorded in the matching index entry.

Two registries with different `index_url` values are distinct, even when they list the same `(name, version)` pair. Within one registry, `(name, version)` is unique, and names that share a canonical form cannot coexist regardless of version.

## Index and Artifacts Can Be Hosted Separately

The index file and the artifacts do not need to live at the same location, on the same host, or even use the same URL scheme. The index's `artifacts_url` field is an independent base URL that consumers combine with each entry's `name` and `version` to compute the archive URL:

```text
{artifacts_url}/{name}-{version}.tar.gz
```

A non-exhaustive list of valid topologies includes:

- Index and artifacts hosted together (for example, both under one S3 bucket prefix or one GitHub Release).
- Index hosted on a CDN or static site; artifacts hosted on a separate object store.
- Index served from one origin and mirrored to another, with `artifacts_url` rewritten per mirror.

## Supported URL Schemes

Schemes for `index_url` are governed by the consumer, not by the index format, so any scheme can be used.

Schemes for `artifacts_url` are documented in in the [registry index format](../reference/registry-index.md#artifacts_url) and are limited to `https`, `http`, and `file`. 

The `http` and `file` schemes are intended for local development and testing. 

## Publication and Immutability

A registry grows by appending entries to `index.json` and uploading the matching archive:

1. The plugin author creates or updates a plugin directory that follows the [plugin format](./plugin-format.md).
2. Use `influxdb3-plugin package` to package an archive and append a new entry to `index.json`.
3. Upload the new archive to `{artifacts_url}/{name}-{version}.tar.gz` and replace `index.json` at its hosted location with the newly generated version.

Once `(name, version)` is published, the artifact and the index entry are immutable. To update a plugin, bump `plugin.version` in the manifest and publish a new entry.

## Yanking

Yanking is the only permitted mutation to an existing entry. Use `influxdb3-plugin yank` on a plugin version to mark it unavailable. Yanking is reversible by clearing the flag.

Entries are never deleted from the registry. Consumers may continue to use a yanked version, but new installs should skip yanked versions.

## Artifact Integrity

Every index entry carries a `hash` field of the archive bytes. Consumers should verify the hash before extracting an archive and reject mismatches. 

The hash is used to verify the integrity between the index and the artifact. Do not install a plugin when an archive's bytes disagree with the index entry's `hash`.

---

Back: [Explanation](./README.md) | Next: [The Plugin Directory Format](./plugin-format.md)
