#!/usr/bin/env bash
set -euo pipefail

# Requires on PATH: bash, cargo (verify build + metadata), jq (metadata/index
# parsing), curl (sparse-index queries). All are assumed present on the
# self-hosted CI runner — cargo via setup-rust; jq is also relied on by the
# build-release job and the justfile.

# Overridable for tests / mirrors.
INDEX_BASE="${INDEX_BASE:-https://index.crates.io}"

# Workspace metadata, resolved once (lazily) and reused across crates by
# crate_version instead of re-shelling cargo per crate. Empty until first use.
WORKSPACE_METADATA=""

# Dependency order. A downstream crate's publish-verify build resolves the
# just-published upstream from the registry, so order matters. This relies on
# cargo >= 1.66, which blocks after each publish until the new version is
# visible in the index; on older cargo the downstream verify could race the
# index and fail. The toolchain is pinned in rust-toolchain.toml (currently
# 1.94) — keep it >= 1.66. If a verify build ever fails resolving a
# just-published sibling under --locked, drop --locked from the publish step.
CRATES=(influxdb3-plugin-schemas influxdb3-plugin-sdk influxdb3-plugin-cli)

# Sparse-index path for a crate name, per cargo's layout:
#   len 1     -> 1/<name>
#   len 2     -> 2/<name>
#   len 3     -> 3/<name[0]>/<name>
#   len >= 4  -> <name[0:2]>/<name[2:4]>/<name>
# Names are lowercase ASCII; hyphens are kept verbatim.
index_path() {
    local name="$1" len=${#1}
    case "$len" in
        1) printf '1/%s\n' "$name" ;;
        2) printf '2/%s\n' "$name" ;;
        3) printf '3/%s/%s\n' "${name:0:1}" "$name" ;;
        *) printf '%s/%s/%s\n' "${name:0:2}" "${name:2:2}" "$name" ;;
    esac
}

# Echo a crate's version from its Cargo.toml via cargo metadata. Per crate —
# never assumes the three crate versions are equal (per-crate versioning model).
# cargo metadata is resolved once and cached in WORKSPACE_METADATA; later crates
# filter the same JSON rather than re-invoking cargo (3 calls -> 1).
crate_version() {
    local name="$1" ver
    if [ -z "$WORKSPACE_METADATA" ]; then
        WORKSPACE_METADATA="$(cargo metadata --format-version 1 --no-deps)"
    fi
    ver="$(printf '%s' "$WORKSPACE_METADATA" \
        | jq -r --arg n "$name" '.packages[] | select(.name == $n) | .version')"
    if [ -z "$ver" ]; then
        echo "ERROR: crate '$name' not found in workspace" >&2
        return 1
    fi
    printf '%s\n' "$ver"
}

# Return 0 if <version> ($2) appears as a "vers" entry in the index body ($1).
# Yanked versions still count — crates.io versions are immutable and a yanked
# version cannot be republished. The crates.io sparse index is compact (one
# minified JSON object per line, no spaces), so a fixed-string match on
# "vers":"<version>" is exact; the trailing quote blocks prefix matches ("0.3"
# must not match "0.3.0"). Fed via a here-string, not a pipe, so grep -q
# closing the input early cannot SIGPIPE a writer under pipefail. (Plain grep,
# not jq -e: jq -e's exit status over a multi-line stream is version-dependent
# — older jq keys it off the last input's output, not whether any matched.)
version_published() {
    local body="$1" version="$2"
    grep -qF "\"vers\":\"$version\"" <<<"$body"
}

# Echo the sparse-index body for a crate. Empty if the crate is not yet in the
# index (HTTP 404 = never published → publishable). Aborts (non-zero) on any
# other HTTP/transport error so a transient failure is never misread as
# "not published" (which would trigger a spurious publish attempt).
fetch_index_versions() {
    local name="$1" url body code
    url="$INDEX_BASE/$(index_path "$name")"
    body="$(mktemp)"
    # --retry/--retry-all-errors ride out transient index blips (5xx, dropped
    # connections) so a single flaky response doesn't abort the whole publish
    # run. A genuine transport failure still ends non-zero after the retries and
    # is caught by the case below.
    code="$(curl --retry 3 --retry-all-errors -sS -o "$body" -w '%{http_code}' "$url" || true)"
    case "$code" in
        200) cat "$body" ;;
        404) : ;;
        *)   rm -f "$body"; echo "ERROR: crates.io index returned HTTP $code for $url" >&2; return 1 ;;
    esac
    rm -f "$body"
}

main() {
    if [ "$#" -gt 1 ]; then
        echo "ERROR: expected 0 or 1 argument, got $#" >&2
        exit 2
    fi
    local mode="publish"
    case "${1:-}" in
        --dry-run) mode="dry-run" ;;
        --verify)  mode="verify" ;;
        "")        : ;;
        *) echo "ERROR: unknown arg '$1' (expected --dry-run, --verify, or none)" >&2; exit 2 ;;
    esac

    # Only the real publish needs the token; dry-run/verify are read-only.
    if [ "$mode" = "publish" ]; then
        : "${CARGO_REGISTRY_TOKEN:?CARGO_REGISTRY_TOKEN is required to publish}"
    fi

    local missing=0
    for crate in "${CRATES[@]}"; do
        local version body
        version="$(crate_version "$crate")"
        body="$(fetch_index_versions "$crate")"
        if version_published "$body" "$version"; then
            case "$mode" in
                verify) echo "OK: $crate $version is on crates.io" ;;
                *)      echo "skip: $crate $version (already on crates.io)" ;;
            esac
            continue
        fi
        case "$mode" in
            verify)  echo "MISSING: $crate $version is NOT on crates.io" >&2; missing=$((missing+1)) ;;
            dry-run) echo "would publish: $crate $version" ;;
            publish) echo "publish: $crate $version"; cargo publish -p "$crate" --locked ;;
        esac
    done

    if [ "$mode" = "verify" ] && [ "$missing" -gt 0 ]; then
        echo "$missing crate version(s) missing from crates.io." >&2
        exit 1
    fi
}

if [ "${BASH_SOURCE[0]}" = "${0}" ]; then main "$@"; fi
