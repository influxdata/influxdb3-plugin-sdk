#!/usr/bin/env bash
set -euo pipefail

# Overridable for tests / mirrors.
INDEX_BASE="${INDEX_BASE:-https://index.crates.io}"

# Dependency order. A downstream crate's publish-verify build resolves the
# just-published upstream from the registry, so order matters.
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
crate_version() {
    local name="$1" ver
    ver="$(cargo metadata --format-version 1 --no-deps \
        | jq -r --arg n "$name" '.packages[] | select(.name == $n) | .version')"
    if [ -z "$ver" ]; then
        echo "ERROR: crate '$name' not found in workspace" >&2
        return 1
    fi
    printf '%s\n' "$ver"
}

# Return 0 if <version> ($2) appears as a "vers" entry in the index body ($1).
# Yanked versions still count — crates.io versions are immutable and a yanked
# version cannot be republished. The trailing quote in the pattern prevents
# prefix matches (e.g. "0.3" must not match "0.3.0").
version_published() {
    local body="$1" version="$2"
    printf '%s\n' "$body" | grep -qF "\"vers\":\"$version\""
}

# Echo the sparse-index body for a crate. Empty if the crate is not yet in the
# index (HTTP 404 = never published → publishable). Aborts (non-zero) on any
# other HTTP/transport error so a transient failure is never misread as
# "not published" (which would trigger a spurious publish attempt).
fetch_index_versions() {
    local name="$1" url body code
    url="$INDEX_BASE/$(index_path "$name")"
    body="$(mktemp)"
    code="$(curl -sS -o "$body" -w '%{http_code}' "$url" || true)"
    case "$code" in
        200) cat "$body" ;;
        404) : ;;
        *)   rm -f "$body"; echo "ERROR: crates.io index returned HTTP $code for $url" >&2; return 1 ;;
    esac
    rm -f "$body"
}

main() {
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
