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

if [ "${BASH_SOURCE[0]}" = "${0}" ]; then main "$@"; fi
