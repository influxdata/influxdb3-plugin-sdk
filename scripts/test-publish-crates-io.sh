#!/usr/bin/env bash
# Unit tests for scripts/publish-crates-io.sh.
# Run from repo root: ./scripts/test-publish-crates-io.sh
set -euo pipefail
cd "$(dirname "$0")/.."

# Source the script WITHOUT running main (guarded by BASH_SOURCE check).
# shellcheck source=/dev/null
source ./scripts/publish-crates-io.sh

TESTS_RUN=0; TESTS_PASSED=0; TESTS_FAILED=0; FAILURES=""
pass() { TESTS_RUN=$((TESTS_RUN+1)); TESTS_PASSED=$((TESTS_PASSED+1)); echo "  ✓ $1"; }
fail() { TESTS_RUN=$((TESTS_RUN+1)); TESTS_FAILED=$((TESTS_FAILED+1)); FAILURES="${FAILURES}\n  ✗ $1"; echo "  ✗ $1"; }
eq() { if [ "$2" = "$3" ]; then pass "$1"; else fail "$1 (expected '$2', got '$3')"; fi; }

echo "== index_path =="
eq "len>=4 (schemas)" "in/fl/influxdb3-plugin-schemas" "$(index_path influxdb3-plugin-schemas)"
eq "len>=4 (sdk)"     "in/fl/influxdb3-plugin-sdk"     "$(index_path influxdb3-plugin-sdk)"
eq "len>=4 (cli)"     "in/fl/influxdb3-plugin-cli"     "$(index_path influxdb3-plugin-cli)"
eq "len 1"            "1/a"        "$(index_path a)"
eq "len 2"            "2/ab"       "$(index_path ab)"
eq "len 3"            "3/a/abc"    "$(index_path abc)"
eq "len 4"            "ab/cd/abcd" "$(index_path abcd)"

echo "== version_published =="
FIX='{"name":"x","vers":"0.3.0","yanked":false}
{"name":"x","vers":"0.4.0","yanked":true}'
if version_published "$FIX" "0.3.0"; then pass "present"; else fail "present"; fi
if version_published "$FIX" "0.4.0"; then pass "yanked still counts"; else fail "yanked still counts"; fi
if version_published "$FIX" "0.5.0"; then fail "absent"; else pass "absent"; fi
if version_published "$FIX" "0.3";   then fail "no prefix false-match"; else pass "no prefix false-match"; fi

echo "== crate_version (real workspace) =="
for c in influxdb3-plugin-schemas influxdb3-plugin-sdk influxdb3-plugin-cli; do
    v="$(crate_version "$c")"
    if [[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+ ]]; then pass "crate_version $c -> $v"; else fail "crate_version $c -> '$v'"; fi
done

echo ""
echo "== Results =="
echo "$TESTS_RUN tests, $TESTS_PASSED passed, $TESTS_FAILED failed"
if [ "$TESTS_FAILED" -gt 0 ]; then echo -e "\nFailures:$FAILURES"; exit 1; fi
echo "All tests passed."
