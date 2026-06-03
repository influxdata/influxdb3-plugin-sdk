#!/usr/bin/env bash
set -euo pipefail

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
    local name="$1"
    cargo metadata --format-version 1 --no-deps \
        | jq -r --arg n "$name" '.packages[] | select(.name == $n) | .version'
}

# Return 0 if <version> ($2) appears as a "vers" entry in the index body ($1).
# Yanked versions still count — crates.io versions are immutable and a yanked
# version cannot be republished. The trailing quote in the pattern prevents
# prefix matches (e.g. "0.3" must not match "0.3.0").
version_published() {
    local body="$1" version="$2"
    printf '%s\n' "$body" | grep -qF "\"vers\":\"$version\""
}

if [ "${BASH_SOURCE[0]}" = "${0}" ]; then main "$@"; fi
