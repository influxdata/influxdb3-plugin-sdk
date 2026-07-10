# Design: Inter-Plugin Dependencies (`dependencies.plugins`)

Status: implemented (manifest schema 1.3, index schema 2.1).

## Summary

Add an optional `dependencies.plugins` field to the plugin manifest and the registry index. It lets a plugin author declare other plugins their plugin depends on, as an array of fully-resolved references: `(index_url, name, version)`, where `version` is a SemVer version requirement (`semver::VersionReq`).

The key `dependencies.plugins` is already reserved for exactly this purpose in the manifest reference, the registry-index reference, and the internal spec ("decided deferred from v1"). This design fills that reserved slot additively.

## Motivation

Plugins can build on other plugins (shared helpers, layered processing). Today there is no structured way to declare that relationship; consumers cannot resolve or even see it. Declaring dependencies in the manifest — and copying them into the index at packaging time — gives registry consumers the data they need to resolve and install a plugin's dependency closure.

## Format

### Manifest (`manifest.toml`)

```toml
[dependencies]
database_version = ">=3.2.0,<4.0.0"
python = ["requests>=2.31,<3"]

[[dependencies.plugins]]
index_url = "https://plugins.example.com/index.json"
name = "geo-lookup"
version = ">=1.0.0,<2.0.0"
```

### Index (`index.json`)

```json
"dependencies": {
  "database_version": ">=3.2.0,<4.0.0",
  "python": [],
  "plugins": [
    {
      "index_url": "https://plugins.example.com/index.json",
      "name": "geo-lookup",
      "version": ">=1.0.0, <2.0.0"
    }
  ]
}
```

The index value is copied from the parsed manifest at packaging time, exactly like every other `dependencies` field. `name` is preserved verbatim; `version` is re-emitted in the normalized `semver::VersionReq` display form (`>=1.0.0,<2.0.0` becomes `>=1.0.0, <2.0.0`), matching `database_version`; `index_url` appears in its normalized `url::Url` form, like every other URL field. An empty `plugins` array is omitted from the index entirely (see Serialization).

## Field Semantics

Each entry is a fully-resolved reference aligned with the global plugin-identity tuple `(index_url, name, version)` defined in the registry explanation, except that `version` is a range rather than an exact version: "any version of `name` at the registry `index_url` that satisfies `version`". `index_url` is the shared `IndexUrl` newtype (a validated, parsed `url::Url`), used by both `PluginDependency` and `PluginId::Registry` — dependency references and plugin identities compare under one normalization and validation rule.

| Field | Type | Required | Rules |
|---|---|---:|---|
| `index_url` | string | Yes | URL of the dependency's registry index. Must parse as an absolute URL with scheme `https`, `http`, or `file` (same scheme set and validation as `artifacts_url`). Parsed and stored as `url::Url`, so it carries the same normalization as every other URL field; serialized in normalized form. |
| `name` | string | Yes | Plugin name in the dependency's registry. Same validation rules as `plugin.name` (charset, length, Windows reserved names). |
| `version` | string | Yes | SemVer version requirement, same syntax and parser (`semver::VersionReq`) as `dependencies.database_version`. Note Cargo semantics: a bare `"1.2"` means `^1.2`. |

`dependencies.plugins` is optional. Missing or `[]` means the plugin has no plugin dependencies.

## Validation

Validation is purely syntactic and local — identical philosophy to `dependencies.python` (validate parseability, no network I/O). `name` is stored as its original string; `version` is stored as the parsed `semver::VersionReq` (like `database_version`); `index_url` is stored as the parsed `url::Url`.

Per entry, with field-path-aware error reporting (`dependencies.plugins[i].index_url` in the manifest; `plugins[j].dependencies.plugins[i].index_url` in the index):

- `index_url` must parse as a URL; scheme must be `https`, `http`, or `file`. New error variant for the scheme failure (mirroring the artifacts-url scheme error); the existing malformed-URL error covers parse failures.
- `name` must pass the existing plugin-name validator (reuses existing error variants).
- `version` must parse as a SemVer range. New error variant mirroring the `database_version` one.

Across entries within one manifest or index entry:

- Entries must be unique by `(index_url, canonical(name))`, where canonical form is the existing lowercase-and-underscore folding. `geo-lookup` and `geo_lookup` at the same `index_url` collide. `index_url` comparison uses parsed `url::Url` equality (normalized: host case folded, default port dropped) — the same equality `PluginId` uses. New duplicate-dependency error variant, reported at the entry path.
- The same canonical name at two different `index_url`s is allowed — different registries are distinct plugins by the identity model.

Both parsers (`Manifest::parse_toml`, `Index::parse_json`) apply the same rules via a shared validator, preserving the collect-all-errors-in-one-pass behavior. A dependency entry missing a required key fails phase 1 as a root-level TOML/JSON parse error, consistent with other missing-required-field handling.

All new error variants are additive (a minor version change per the error-type stability contract).

## Serialization

`plugins` deliberately does not follow the `python` pattern (always emitted). It is `#[serde(default, skip_serializing_if = "Vec::is_empty")]`: omitted from canonical index JSON when missing or empty. The asymmetry with `python` is accepted (D4): omitting the empty field keeps every pre-existing index entry byte-identical when a legacy index is rewritten by newer tooling, so published entries remain untouched in fact, not just in spirit.

Within an entry, `name` is preserved verbatim in canonical index JSON; `version` serializes in the normalized `semver::VersionReq` display form (comparators joined with `", "`), matching `database_version`; `index_url` serializes in its normalized `url::Url` form, like every other URL field.

## Schema Versioning and Compatibility

- `manifest_schema_version`: `1.2` → `1.3` (additive minor). Existing `1.x` manifests remain valid; `plugins` defaults to empty.
- `index_schema_version`: `2.0` → `2.1` (additive minor). Existing `2.x` indexes remain valid.

Known trade-off of the minor bump: pre-`2.1` consumers ignore unknown fields, so they can install a plugin without seeing its declared dependencies. This is the documented semantics of minor schema evolution ("unknown fields are ignored within a supported major"), and the key was reserved in advance precisely so tooling authors know it may appear. A major bump was considered and rejected as disproportionate for an additive optional field.

### Migration of existing indexes

Implicit upgrade on write; no migration command and no operator action (D7).

- `Index::to_canonical_json` always stamps `index_schema_version` to `IndexSchemaVersion::CURRENT` as part of its existing normalization pass (alongside entry sorting and NFC description folding). Whatever minor was parsed, the written file declares the writer's schema version.
- Because an empty `plugins` array is omitted (D4), rewriting a `2.0` index through `2.1` tooling changes exactly one line — the version string. Entries gain the `plugins` key only when a newly published entry actually declares plugin dependencies.
- Parsing is unchanged: any `2.x` index remains valid input regardless of minor.

## Decisions

| # | Question | Decision | Rationale |
|---|---|---|---|
| D1 | Store `index_url` as parsed `url::Url` or verbatim string? | Parsed: the `IndexUrl` newtype (validated `url::Url`, scheme check shared with `ArtifactsUrl`), used by both `PluginDependency` and `PluginId::Registry`. | One type for registry-index URLs everywhere: dependency references and plugin identities compare under the same normalization and validation rules, with no verbatim-vs-normalized drift. Canonical JSON emits the normalized form deterministically. The scheme set can widen later without breaking (accepting more is additive). |
| D2 | Allowed `index_url` schemes? | `https`, `http`, `file` — mirror `artifacts_url`. | Keeps air-gapped/`file` registries working; blocks unsupported schemes. Applies to plugin identity too (`PluginId::Registry` carries `IndexUrl`), narrowing the previously scheme-agnostic identity model; accepted because widening the set later is non-breaking. Consumer-side registry config still governs how indexes are fetched. |
| D3 | Minor or major schema bump? | Minor (`1.3` manifest, `2.1` index). | Additive optional field; key was reserved for additive introduction. See compatibility note above. |
| D4 | Serialize empty as `[]` or omit? | Omit when empty: `#[serde(default, skip_serializing_if = "Vec::is_empty")]`. Deliberately different from `python`. | The field is optional and absence means "no plugin dependencies". Omission keeps legacy index entries byte-identical when older indexes are rewritten by newer tooling — the entry-immutability promise holds at the byte level. |
| D5 | Verify the dependency exists in the referenced index at package time? | No. | SDK performs no network I/O; validation stays syntactic. Resolution is the consumer/installer's job. |
| D6 | Self-dependency / cycle checks? | None, and no doc note. | Self-dependency is undetectable in general (a manifest does not know its own future `index_url`); same-name deps at other registries are legitimate. Cycles cannot be detected locally. |
| D7 | How do existing `2.0` indexes migrate to `2.1`? | Implicitly, on write: `to_canonical_json` always stamps `IndexSchemaVersion::CURRENT`. No migrate command, no operator action. | The minor is informational within a major (parsers check the major only; consumers ignore unknown fields), so no flag day is needed. Stamping on write keeps the declared version truthful once entries carry `plugins`; combined with D4, a rewrite of a dependency-free `2.0` index changes only the version line. |

## Out of Scope

- Dependency resolution, lockfiles, transitive closure, and version unification (consumer concerns).
- Existence or reachability checks against the referenced registry (D5).
- Cycle and self-dependency detection (D6).
- Optional/feature-gated dependencies.

## Affected Surfaces

High level only (implementation to follow separately):

- `influxdb3-plugin-schemas`: new `IndexUrl` (newtype over `url::Url`, scheme validation shared with `ArtifactsUrl`) and `PluginDependency` types, `Dependencies.plugins` field, shared per-entry validation in both parsers, new error variants, schema-version constants, `to_canonical_json` version stamping (D7). `PluginId::Registry.index_url` and `PluginId::registry()` switch from raw `url::Url` to `IndexUrl` — breaking, and it narrows the identity model's accepted schemes (D2). Breaking-under-`0.x` minor bump, cascading minor bumps to `sdk` and `cli` per the versioning model.
- `influxdb3-plugin-sdk`: no behavioral change — packaging copies `dependencies` wholesale from manifest to index entry, so the field flows through `IndexEntry::from_manifest` automatically.
- `influxdb3-plugin-cli`: `info` output gains `dependencies.plugins` in the JSON payload (additive to the stable schema) and a `plugins:` line in human output.
- Docs: `docs/src/reference/manifest.md` and `registry-index.md` (replace the "reserved" sentences with the field specification, bump example schema versions), `docs/internal/spec.md` (deferred/reserved-key mentions), `CHANGELOG.md`.
- Tests/fixtures: valid and invalid fixtures for the new field, snapshot updates, `Dependencies` initializers in test helpers, legacy-rewrite coverage (parse a `2.0` fixture → canonical write → version stamped `2.1`, entries otherwise byte-identical).
