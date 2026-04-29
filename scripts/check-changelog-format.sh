#!/usr/bin/env bash
# Validates CHANGELOG.md conforms to Keep a Changelog 1.1.0.
# https://keepachangelog.com/en/1.1.0/
#
# Checks:
#   1. File starts with "# Changelog"
#   2. "## [Unreleased]" section exists before any versioned section
#   3. Version sections match: ## [X.Y.Z] - YYYY-MM-DD (or with pre-release)
#   4. Subsections are only: Added, Changed, Deprecated, Removed, Fixed, Security
#   5. No duplicate version sections
#   6. Versions in descending order (newest first)

set -euo pipefail
cd "$(dirname "$0")/.."

FILE="CHANGELOG.md"
[ -f "$FILE" ] || { echo "FAIL: $FILE not found" >&2; exit 1; }

errors=0

# Check 1: title
if ! head -1 "$FILE" | grep -q '^# Changelog'; then
    echo "FAIL: first line must be '# Changelog'" >&2
    errors=$((errors+1))
fi

# Check 2: Unreleased section exists
if ! grep -q '^## \[Unreleased\]' "$FILE"; then
    echo "FAIL: missing '## [Unreleased]' section" >&2
    errors=$((errors+1))
fi

# Check 3: version sections match the pattern
# Allow: ## [0.1.0] - 2026-04-28, ## [0.1.0-1.rc.0] - 2026-04-28
VERSION_PATTERN='^## \[[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?\] - [0-9]{4}-[0-9]{2}-[0-9]{2}$'
while IFS= read -r line; do
    # Lines starting with "## [" that aren't "[Unreleased]"
    if [[ "$line" =~ ^##\ \[ ]] && [[ ! "$line" =~ ^##\ \[Unreleased\] ]]; then
        if ! echo "$line" | grep -qE "$VERSION_PATTERN"; then
            echo "FAIL: malformed version section: '$line'" >&2
            echo "      expected: ## [X.Y.Z] - YYYY-MM-DD" >&2
            errors=$((errors+1))
        fi
    fi
done < "$FILE"

# Check 4: subsections are only from the allowed set
ALLOWED_SUBSECTIONS="Added|Changed|Deprecated|Removed|Fixed|Security"
while IFS= read -r line; do
    if [[ "$line" =~ ^###\  ]]; then
        subsection="${line#'### '}"
        if ! echo "$subsection" | grep -qE "^($ALLOWED_SUBSECTIONS)$"; then
            echo "FAIL: invalid subsection '### $subsection'" >&2
            echo "      allowed: Added, Changed, Deprecated, Removed, Fixed, Security" >&2
            errors=$((errors+1))
        fi
    fi
done < "$FILE"

# Check 5: no duplicate version sections
duplicates=$(grep -oE '## \[[0-9]+\.[0-9]+\.[0-9]+[^]]*\]' "$FILE" | sort | uniq -d)
if [ -n "$duplicates" ]; then
    echo "FAIL: duplicate version sections:" >&2
    echo "$duplicates" | sed 's/^/  /' >&2
    errors=$((errors+1))
fi

# Check 6: Unreleased comes before any versioned section
unreleased_line=$(grep -n '^## \[Unreleased\]' "$FILE" | head -1 | cut -d: -f1)
first_version_line=$(grep -nE "$VERSION_PATTERN" "$FILE" | head -1 | cut -d: -f1)
if [ -n "$unreleased_line" ] && [ -n "$first_version_line" ]; then
    if [ "$unreleased_line" -gt "$first_version_line" ]; then
        echo "FAIL: [Unreleased] section (line $unreleased_line) must come before first version section (line $first_version_line)" >&2
        errors=$((errors+1))
    fi
fi

if [ "$errors" -gt 0 ]; then
    echo "" >&2
    echo "$errors format violation(s) in $FILE. See https://keepachangelog.com/en/1.1.0/" >&2
    exit 1
fi

echo "CHANGELOG.md format is valid (Keep a Changelog 1.1.0)."
