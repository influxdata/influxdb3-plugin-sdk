# Index Query Design

## Summary

Add shared read/query primitives for registry indexes to
`influxdb3-plugin-schemas`.

The workspace already has primitives for reading an index in the narrow sense:
`Index::parse_json` parses and validates `index.json`, and the public `Index`
and `IndexEntry` types expose the registry contents. What is missing is a
stable, reusable query layer for common read operations:

- search plugin entries by name or description
- filter by trigger type
- filter yanked entries
- filter by database-version compatibility
- inspect metadata for a selected plugin or exact plugin-version
- consistently select and display the latest visible version

These operations should be available to the SDK CLI, the Rust UI backend, and
the future database `plugin.search` / `plugin.info` HTTP API without requiring
any consumer to shell out to the CLI.

## Goals

- Provide shared index search and info behavior for CLI, UI backend, and
  database consumers.
- Keep the implementation in an existing crate. No new crate or project.
- Keep the API pure and deterministic over parsed `Index` values.
- Make `influxdb3-plugin-schemas` the single source of truth for index read
  semantics, matching its existing ownership of schema shape and validation.
- Let the database layer reuse the same filtering and projection logic while
  layering database-specific registry behavior around it.
- Make result shapes convenient for both human output and JSON/API projection.

## Non-Goals

- No HTTP fetching.
- No filesystem reads.
- No auth handling.
- No cache freshness, ETag, or conditional GET behavior.
- No configured-registry catalog.
- No RBAC.
- No installed-plugin or lockfile state.
- No dependency resolution or install-plan generation.
- No CLI output formatting.
- No UI-specific model.
- No attempt to fully implement database `plugin.search` inside the SDK CLI.

## Current State

### Existing Read Primitives

`influxdb3-plugin-schemas` currently provides:

- `Index::parse_json(input: &str) -> Result<Index, SchemaErrors>`
- `Index::to_canonical_json()`
- public schema types: `Index`, `IndexEntry`, `PluginName`, `TriggerType`,
  `Dependencies`, `Description`, `ArtifactHash`, `ArtifactsUrl`
- field-level validation for index entries
- duplicate `(name, version)` and canonical-name collision checks during parse

Because `Index.plugins` is public, callers can manually iterate entries today.
That is enough for ad hoc use, but it does not give the CLI, UI backend, and
database one shared definition of search, info, compatibility filtering, or
latest-version selection.

### Existing Mutation Primitives

`influxdb3-plugin-sdk` currently provides author-side mutation helpers:

- `mutate_index::add_entry`
- `mutate_index::yank`
- `mutate_index::unyank`
- `package::package_plugin`, which derives an updated index from a plugin
  directory

Those functions are useful for publishing workflows, but they are not a read
API and they are not the right home for database/UI index inspection behavior.

### CLI

The CLI can read an index for `validate`, `package`, and `yank`, but it has no
command that browses an index. `new list` lists scaffold templates, not registry
plugins.

## Design Decision

Add index query APIs to `influxdb3-plugin-schemas`.

Rationale:

- Search and info are pure operations over schema data.
- Cargo is the model for the user-facing shape: `search` is a package-level
  summary operation, while `info` selects one version when no explicit version
  is supplied and inspects exactly one version when a version is supplied.
- The database and UI backend can safely depend on `schemas` without taking on
  author-side packaging dependencies such as archive construction or Python
  source analysis.
- The SDK CLI already depends on `schemas`, so it can build local
  `index search` and `index info` commands on top of the same primitives.
- This matches the existing boundary direction in
  `docs/schema-boundary-refactor.md`: schemas owns schema document shape,
  schema validation, schema-derived conversions, and registry-index invariants.

The SDK crate should not own these primitives unless its public contract changes
from "author-side packaging library" to "shared runtime library." That is not
needed for this feature.

## Proposed API

Expose a new `index_query` module from `influxdb3-plugin-schemas`.

```rust
pub mod index_query;

pub use index_query::{
    IndexInfo, IndexInfoQuery, IndexInfoResult, IndexSearchHit,
    IndexSearchQuery, IndexSearchResult, IndexVersionVisibility,
    IndexVisibilityReason,
};
```

Add convenience methods on `Index`:

```rust
impl Index {
    pub fn search(&self, query: &IndexSearchQuery) -> IndexSearchResult;

    pub fn info(&self, query: &IndexInfoQuery) -> IndexInfoResult;
}
```

### Search Query

```rust
#[derive(Debug, Clone, Default)]
pub struct IndexSearchQuery {
    pub query: Option<String>,
    pub trigger_type: Option<TriggerType>,
    pub database_version: Option<semver::Version>,
    pub include_yanked: bool,
    pub include_incompatible: bool,
}
```

Semantics:

- `query = None` or a whitespace-only string means "match all visible entries."
- `query = Some(...)` matches plugin name or description.
- Search text does not include Python dependency strings, trigger names,
  database-version ranges, URLs, or hashes in v1. Trigger type and database
  compatibility are explicit filters.
- Matching is case-insensitive.
- Plugin-name matching checks both the display name and canonical name so
  `my-plugin` and `my_plugin` behave as users expect.
- Description matching uses case-insensitive substring matching.
- `trigger_type` keeps only entries that support the requested trigger type.
- `include_yanked = false` hides yanked entries by default.
- If `database_version` is provided and `include_incompatible = false`, entries
  whose `dependencies.database_version` does not match are hidden.
- If `database_version` is not provided, compatibility is not evaluated and no
  entry is hidden for compatibility.
- `database_version` is optional because raw index browsing may not be tied to
  a running database. The database implementation should pass its own current
  database version when serving DB-backed search/info endpoints.
- No result limit or pagination is part of the v1 schemas primitive. CLI, UI,
  and database callers may layer presentation or HTTP-level limits outside the
  shared query logic.

### Search Result

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct IndexSearchResult {
    pub hits: Vec<IndexSearchHit>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct IndexSearchHit {
    pub name: PluginName,
    pub version: semver::Version,
    pub description: Description,
    pub triggers: Vec<TriggerType>,
    pub visibility: IndexVersionVisibility,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum IndexVersionVisibility {
    Visible,
    Hidden { reasons: Vec<IndexVisibilityReason> },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum IndexVisibilityReason {
    Yanked,
    IncompatibleDatabaseVersion {
        required: semver::VersionReq,
        actual: semver::Version,
    },
}
```

Search returns one row per plugin, grouped by canonical plugin name. This
matches Cargo's package-level search pattern rather than returning one row per
version.

Each search hit summarizes the latest visible matching version:

- `version`
- `description`
- `triggers`
- `visibility`

There is no per-version list in v1 search results. If callers need exact
version detail, they should call `info(name@version)`. If callers need version
history, that is deferred to a future explicit all-versions mode.

By default search returns only visible hits. If `include_yanked` or
`include_incompatible` is set, matching hidden versions can be selected and
returned, with `visibility = Hidden { reasons }` so CLI/UI/database callers can
label them consistently.

### Info Query

```rust
#[derive(Debug, Clone)]
pub struct IndexInfoQuery {
    pub name: PluginName,
    pub version: Option<semver::Version>,
    pub database_version: Option<semver::Version>,
    pub include_yanked: bool,
    pub include_incompatible: bool,
}
```

Semantics:

- `name` matches by canonical plugin name.
- `version = None` follows Cargo's pattern and selects one version: the latest
  visible version for the plugin. `include_yanked` and `include_incompatible`
  opt hidden versions into that selection.
- `version = Some(v)` is an exact inspection request. If that plugin-version
  exists, `info` returns it even when it is yanked or incompatible, with
  `visibility = Hidden { reasons }`. Exact-version inspection does not require
  `include_yanked` or `include_incompatible`.
- Yank and compatibility visibility is computed the same way as
  `IndexSearchQuery`: yanked entries are hidden by default, and database
  compatibility is evaluated only when `database_version` is supplied.
- No trigger-type filter exists on `info`; info is identity-based and returns
  the plugin's trigger support in the result.
- No all-versions mode exists in v1. Version-history listing is deferred until
  there is a concrete CLI/UI/API need.

### Info Result

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum IndexInfoResult {
    Found(IndexInfo),
    NotFound {
        name: PluginName,
        version: Option<semver::Version>,
    },
    FilteredOut {
        name: PluginName,
        version: Option<semver::Version>,
        reasons: Vec<IndexVisibilityReason>,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct IndexInfo {
    pub name: PluginName,
    pub version: semver::Version,
    pub description: Description,
    pub triggers: Vec<TriggerType>,
    pub homepage: Option<url::Url>,
    pub repository: Option<url::Url>,
    pub documentation: Option<url::Url>,
    pub dependencies: Dependencies,
    pub hash: ArtifactHash,
    pub visibility: IndexVersionVisibility,
}
```

`IndexInfoResult` distinguishes absence from present-but-hidden state:

- `Found` means a selected or exact plugin-version exists and is returned. For
  exact version requests, `Found` may carry hidden visibility reasons.
- `NotFound` means no matching plugin or exact plugin-version exists in this
  index.
- `FilteredOut` means the plugin exists, but `info(name)` could not select a
  visible version under the current filters. For example, every version is
  yanked, or every version is incompatible with the supplied database version.

This follows Cargo's distinction between a version that does not exist and a
version that exists but is not normally selectable.

## Ordering

All results must be deterministic.

- Search hits sort by canonical plugin name ascending.
- Version selection uses SemVer precedence descending, newest first.
- When SemVer precedence compares equal, fall back to full version string
  ordering for deterministic output.

No relevance ranking is introduced in v1. This keeps behavior simple,
predictable, and easy to preserve across CLI, UI, and HTTP API consumers.

## Database API Integration

The future database `plugin.search` and `plugin.info` APIs should use these
schemas primitives after the database has already handled database-owned
concerns:

1. Resolve configured registries.
2. Fetch or revalidate indexes.
3. Parse each index with `Index::parse_json`.
4. Call `Index::search` or `Index::info` per reachable registry.
5. Add database-specific response fields:
   - registry identity (`index_url`, optional display alias)
   - per-registry reachability status for best-effort search
   - RBAC visibility effects, if any
   - installed state from lockfiles, if included in the endpoint response

This keeps the core filtering behavior shared while preserving the database as
the owner of registry configuration, freshness, auth, RBAC, partial failure,
and installed-state semantics.

The schemas primitives intentionally operate on one `Index` at a time.
Multi-registry aggregation stays in the database, UI backend, or CLI layer
because registry identity is the configured `index_url`, not a field inside
`index.json`.

## CLI Integration

The SDK CLI can add local index-inspection commands later without defining new
search behavior:

```text
influxdb3-plugin index search --index ./index.json downsample
influxdb3-plugin index search --index ./index.json --trigger-type process_writes
influxdb3-plugin index search --index ./index.json --database-version 3.2.0
influxdb3-plugin index info --index ./index.json downsampler
influxdb3-plugin index info --index ./index.json downsampler --version 1.2.0
```

These commands should be documented as local index inspection. They should not
claim to be equivalent to database `plugin.search`, because they do not know
configured registries, auth, cache freshness, RBAC, or installed state.

## UI Backend Integration

The Rust UI backend should depend directly on `influxdb3-plugin-schemas` and
call `Index::search` / `Index::info` for raw index browsing.

If the UI is connected to a running database, it should prefer the database
HTTP API so it receives database-owned behavior such as configured registries,
auth, compatibility against the actual server version, and per-registry status.

The UI backend should never consume CLI output as an API boundary.

## Compatibility And Stability

The query structs and result structs become part of the schemas crate public
API. To keep the API evolvable:

- result structs and enums should derive `serde::Serialize` so CLI JSON, UI
  backend responses, and database adapters can project them without duplicate
  conversion code
- query structs do not need `serde::Serialize` or `serde::Deserialize` in v1;
  CLI flags, UI requests, and database HTTP requests can map into the Rust
  query structs explicitly
- structs should be marked `#[non_exhaustive]` if external construction should
  remain flexible
- adding optional fields is a minor change
- renaming or removing fields is a breaking change
- enum result types, if introduced later, should be `#[non_exhaustive]`

The first implementation should avoid borrowing result structs from `Index`.
Owned result projections are simpler for CLI JSON, UI serialization, and
database response construction. The index is bounded to roughly hundreds of
entries, so cloning metadata is acceptable.

Search/info result JSON should be treated as a convenient shared projection,
not as the final database HTTP API contract. The database may wrap or map these
results to add `index_url`, registry status, installed state, or API-specific
field names.

## Deferred

- Multi-index helpers in schemas.
- Search limits or pagination in schemas.
- Dependency-text search.
- Trigger-type filtering on `info`.
- All-versions or version-history mode for `info`.
- Per-version detail lists in `search`.

## Behavioral Test Coverage

The implementation should cover the following behavior. These are intentionally
behavior-level assertions rather than code-level test cases; implementation can
group them into fixtures and table-driven tests.

### Search Query Matching

1. **Empty query matches all visible plugins.** Assert `query = None` returns
   one hit per plugin with at least one visible version.
2. **Whitespace query matches all visible plugins.** Assert
   `query = Some("   ")` behaves the same as `None`.
3. **Name substring match.** Given `downsampler`, query `sample` returns
   `downsampler`.
4. **Description substring match.** Given a description containing `WAL commit`,
   query `wal` returns that plugin.
5. **Case-insensitive matching.** Query `DOWNSAMPLE` matches `downsampler`.
6. **Canonical-name matching.** Query `my_plugin` matches plugin name
   `my-plugin`, and query `my-plugin` matches plugin name `my_plugin` where a
   fixture can validly contain that spelling.
7. **No dependency-text search.** Given a plugin depends on `requests` but its
   name and description do not mention it, query `requests` returns no hit.
8. **No URL/hash/trigger text search.** Given a documentation URL, hash, or
   trigger contains the query string but name and description do not, assert no
   hit.

### Search Filtering

9. **Trigger-type filter includes supported plugin.** With
   `trigger_type = process_writes`, search returns plugins whose selected
   matching version includes `process_writes`.
10. **Trigger-type filter excludes unsupported plugin.** A plugin with only
    `process_request` is absent from `process_writes` search.
11. **Yanked hidden by default.** A plugin with only yanked versions does not
    appear when `include_yanked = false`.
12. **Yanked included when requested.** The same plugin appears when
    `include_yanked = true`, with
    `visibility = Hidden { reasons: [Yanked] }`.
13. **Incompatible hidden when DB version supplied.** A plugin version requiring
    `>=4.0.0` is hidden for database version `3.2.0` when
    `include_incompatible = false`.
14. **Incompatible included when requested.** The same version appears with
    `include_incompatible = true`, with
    `Hidden { reasons: [IncompatibleDatabaseVersion { ... }] }`.
15. **No compatibility filtering when DB version omitted.** Database-version
    ranges are not evaluated; result visibility is not hidden for
    compatibility.
16. **Yanked and incompatible reasons accumulate.** A version that is both
    yanked and incompatible returns `Hidden` with both reasons when included.

### Search Version Selection

17. **One hit per plugin.** Multiple visible versions of one plugin produce
    exactly one search hit.
18. **Latest visible version selected.** Given versions `1.0.0`, `1.2.0`, and
    `2.0.0`, the search hit uses `2.0.0`.
19. **Latest hidden skipped by default.** Given `2.0.0` yanked and `1.2.0`
    visible, default search selects `1.2.0`.
20. **Latest incompatible skipped by default.** Given `2.0.0` incompatible and
    `1.2.0` compatible for database version `3.2.0`, default search selects
    `1.2.0`.
21. **Hidden version can become selected when included.** Given `2.0.0` yanked
    and `1.2.0` visible, `include_yanked = true` selects `2.0.0` and marks it
    hidden.
22. **Summary fields come from selected version.** Assert `description` and
    `triggers` on a hit equal the selected version's fields, not unioned older
    versions.
23. **No per-version details in search.** Assert the search result shape has no
    versions list.

### Search Ordering

24. **Hits sorted by canonical name.** Given unsorted index entries, hits are
    ordered alphabetically by canonical plugin name.
25. **SemVer precedence used for selection.** `1.0.0-alpha` sorts before
    `1.0.0`; the selected version is `1.0.0`.
26. **Build-metadata tie deterministic.** Given `1.0.0+build.1` and
    `1.0.0+build.2`, the selected result is deterministic according to the
    specified fallback ordering.

### Info Lookup

27. **`info(name)` selects latest visible version.** Given multiple visible
    versions, returns `Found` for the newest visible version only.
28. **`info(name)` does not return all versions.** Assert the result contains
    exactly one `IndexInfo`.
29. **`info(name)` skips yanked by default.** Given latest version yanked and
    an older version visible, selected version is the older visible version.
30. **`info(name)` skips incompatible by default when DB version supplied.**
    Given latest incompatible and older compatible, selected version is the
    older compatible version.
31. **`info(name)` includes yanked in selection when requested.** With
    `include_yanked = true`, yanked latest can be selected and returned with
    hidden visibility.
32. **`info(name)` includes incompatible in selection when requested.** With
    `include_incompatible = true`, incompatible latest can be selected and
    returned with hidden visibility.
33. **`info(name)` with no DB version applies no compatibility filtering.** The
    latest version is selected regardless of database-version range.
34. **`info(name)` plugin missing.** Unknown name returns
    `NotFound { name, version: None }`.
35. **`info(name)` plugin exists but all versions yanked.** Returns
    `FilteredOut { version: None, reasons: [Yanked] }`.
36. **`info(name)` plugin exists but all versions incompatible.** Returns
    `FilteredOut { version: None, reasons: [IncompatibleDatabaseVersion] }`.
37. **`info(name)` plugin exists but all versions hidden for mixed reasons.**
    Returns `FilteredOut` with combined reasons represented.

### Exact-Version Info

38. **`info(name@version)` found visible.** Exact existing visible version
    returns `Found` for that version.
39. **`info(name@version)` found yanked.** Exact yanked version returns `Found`
    with `Hidden { reasons: [Yanked] }`, even when `include_yanked = false`.
40. **`info(name@version)` found incompatible.** Exact incompatible version
    returns `Found` with
    `Hidden { reasons: [IncompatibleDatabaseVersion { ... }] }`, even when
    `include_incompatible = false`.
41. **`info(name@version)` found yanked and incompatible.** Returns `Found`
    with both hidden reasons.
42. **`info(name@version)` missing version.** Existing plugin with missing
    requested version returns `NotFound { version: Some(...) }`.
43. **`info(name@version)` missing plugin.** Unknown plugin returns
    `NotFound { name, version: Some(...) }`.
44. **Exact-version info ignores trigger-type filtering.** There is no
    trigger-type field on `IndexInfoQuery`; returned metadata includes triggers
    but is not filtered by them.

### Info Result Content

45. **Info includes full metadata.** Assert `name`, `version`, `description`,
    `triggers`, links, dependencies, hash, and visibility match the index entry.
46. **Visible info has `visibility = Visible`.** Non-yanked compatible exact or
    selected version returns visible state.
47. **Hidden reason includes required and actual DB versions.** Incompatible
    reason contains the entry's `VersionReq` and the supplied database version.

### Single-Index Boundary

48. **No cross-index aggregation behavior.** The API accepts one `Index`; no
    multi-index ambiguity or aggregation behavior is represented in schemas.

### Serialization

49. **Search result serializes.** Assert `IndexSearchResult` serializes to
    stable JSON containing expected fields.
50. **Info `Found` serializes.** Assert `IndexInfoResult::Found` serializes with
    full expected metadata.
51. **Info `NotFound` serializes.** Assert `NotFound` includes name and optional
    version.
52. **Info `FilteredOut` serializes.** Assert `FilteredOut` includes name,
    optional version, and reasons.
53. **Visibility reasons serialize.** Assert `Yanked` and
    `IncompatibleDatabaseVersion` serialize distinctly and include required
    data.

### Edge Cases

54. **Empty index search.** Returns `hits = []`.
55. **Empty index info.** Returns `NotFound`.
56. **Plugin with only hidden versions and search default.** Does not appear.
57. **Plugin with mixed visible/hidden versions and search default.** Appears
    using latest visible version.
58. **Plugin with mixed visible/hidden versions and include flags.** Appears
    using latest version allowed by include flags.
59. **Description/name match on hidden latest but visible older.** Default
    search returns the plugin if an older visible version matches; selected
    version is the latest visible matching version.
60. **Trigger filter applied before grouping.** If the latest version lacks the
    requested trigger but an older version supports it, search with that trigger
    returns the plugin summarized by the older matching version.
