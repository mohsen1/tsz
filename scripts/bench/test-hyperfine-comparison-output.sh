#!/bin/bash
# Unit test for print_hyperfine_comparison_output (scripts/bench/lib/bench-vs-tsgo-prereqs.sh).
#
# #16196 fixed the structured-data half of "a killed/errored benchmark row
# still reports a fabricated Nx ratio" (row-utils.mjs's didNotFinish, wired
# through the JS reporting layer in #16779). #16779's own PR body named one
# residual it did not cover: hyperfine's own "Summary\n  X ran\n  N.NN times
# faster than Y" comparison text streams straight to stdout/the CI log
# whenever `--ignore-failure` lets a killed-by-timeout or non-zero-exit
# command finish "successfully" next to a clean one — before this script
# ever inspects an exit code, so the JSON-side gate can't intercept it. This
# is not reachable from Node (the JS suite that covers #16196/#16779), hence
# a standalone shell test.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/bench/lib/bench-vs-tsgo-prereqs.sh
source "$SCRIPT_DIR/lib/bench-vs-tsgo-prereqs.sh"

pass=0
fail=0

check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    pass=$((pass + 1))
    echo "  ok   $label"
  else
    fail=$((fail + 1))
    echo "  FAIL $label:"
    echo "    expected: $(printf '%q' "$expected")"
    echo "    actual:   $(printf '%q' "$actual")"
  fi
}

# A real capture of `hyperfine --style full --ignore-failure` comparing a
# command killed by the repo's timeout runner (tsz) against a clean one
# (tsgo) — reproduced against real hyperfine 1.18 output shape. Colored
# (hyperfine emits ANSI codes under --style full even when not a tty).
KILLED_TSZ_OUTPUT=$'\x1b[1mBenchmark \x1b[0m\x1b[1m1\x1b[0m: tsz\n  Time (\x1b[1;32mabs\x1b[0m \xe2\x89\xa1):        \x1b[1;32m 1.029 s\x1b[0m               [User: \x1b[34m0.011 s\x1b[0m, System: \x1b[34m0.012 s\x1b[0m]\n \n  \x1b[33mWarning\x1b[0m: Ignoring non-zero exit code.\n \n\x1b[1mBenchmark \x1b[0m\x1b[1m2\x1b[0m: tsgo\n  Time (\x1b[1;32mabs\x1b[0m \xe2\x89\xa1):        \x1b[1;32m 51.7 ms\x1b[0m               [User: \x1b[34m1.7 ms\x1b[0m, System: \x1b[34m0.0 ms\x1b[0m]\n \n\x1b[1mSummary\x1b[0m\n  \x1b[36mtsgo\x1b[0m ran\n\x1b[1;32m   19.89\x1b[0m times faster than \x1b[35mtsz\x1b[0m'

CLEAN_OUTPUT=$'\x1b[1mBenchmark \x1b[0m\x1b[1m1\x1b[0m: tsz\n  Time (\x1b[1;32mabs\x1b[0m \xe2\x89\xa1):        \x1b[1;32m 45.0 ms\x1b[0m\n \n\x1b[1mBenchmark \x1b[0m\x1b[1m2\x1b[0m: tsgo\n  Time (\x1b[1;32mabs\x1b[0m \xe2\x89\xa1):        \x1b[1;32m 51.7 ms\x1b[0m\n \n\x1b[1mSummary\x1b[0m\n  \x1b[36mtsz\x1b[0m ran\n\x1b[1;32m   1.15\x1b[0m times faster than \x1b[35mtsgo\x1b[0m'

# ok=true: hyperfine's output must pass through byte-identical, Summary and all.
actual="$(print_hyperfine_comparison_output "$CLEAN_OUTPUT" true)"
check "ok=true passes the Summary comparison through unchanged" "$CLEAN_OUTPUT" "$actual"

# ok=false (the #16196/#16779-residual case): the fabricated Summary/ratio
# line must never reach the output, but the truthful per-benchmark timing
# lines above it must survive.
actual="$(print_hyperfine_comparison_output "$KILLED_TSZ_OUTPUT" false)"
if printf '%s' "$actual" | grep -q "times faster"; then
  fail=$((fail + 1))
  echo "  FAIL ok=false must never print a fabricated 'times faster' ratio"
  echo "    actual: $actual"
else
  pass=$((pass + 1))
  echo "  ok   ok=false suppresses the fabricated 'times faster' ratio"
fi
actual_plain="$(printf '%s' "$actual" | sed 's/\x1b\[[0-9;]*m//g')"
case "$actual_plain" in
  *"Benchmark 1: tsz"*) pass=$((pass + 1)); echo "  ok   ok=false keeps the truthful tsz timing line" ;;
  *) fail=$((fail + 1)); echo "  FAIL ok=false dropped the truthful tsz timing line: $actual_plain" ;;
esac
case "$actual_plain" in
  *"Benchmark 2: tsgo"*) pass=$((pass + 1)); echo "  ok   ok=false keeps the truthful tsgo timing line" ;;
  *) fail=$((fail + 1)); echo "  FAIL ok=false dropped the truthful tsgo timing line: $actual_plain" ;;
esac
case "$actual" in
  *"#16196"*) pass=$((pass + 1)); echo "  ok   ok=false explains the suppression" ;;
  *) fail=$((fail + 1)); echo "  FAIL ok=false gave no explanation for the missing ratio: $actual" ;;
esac

# Defensive case: ok=false with no Summary block at all (should not happen on
# the real 2-command comparison path, but must not crash or mangle output).
NO_SUMMARY_OUTPUT=$'\x1b[1mBenchmark \x1b[0m\x1b[1m1\x1b[0m: tsz\n  Time (\x1b[1;32mabs\x1b[0m \xe2\x89\xa1):        \x1b[1;32m 45.0 ms\x1b[0m'
actual="$(print_hyperfine_comparison_output "$NO_SUMMARY_OUTPUT" false)"
check "ok=false with no Summary block passes output through unchanged" "$NO_SUMMARY_OUTPUT" "$actual"

echo
echo "print_hyperfine_comparison_output: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
