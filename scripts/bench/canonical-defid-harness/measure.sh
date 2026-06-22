#!/usr/bin/env bash
#
# #14344 content-addressing flip — end-to-end validation harness.
# ==============================================================
# Measures the #13862 cross-arena wrong-decl identity collision witness:
#   (a) the #14520 `identity_collision_wrong_decl_suppressed` perf counter, and
#   (b) the md5 of the (sorted) diagnostic output,
# across RAYON_NUM_THREADS in {1,2,4,8,16}, with the `TSZ_CANONICAL_DEFID`
# flip flag both OFF (baseline) and ON.
#
# SUCCESS CRITERIA for the flip (#18 PR3 / dualenv brick-3 election wiring):
#   - flag-ON counter == 0 on every thread count (canonical identity removes the
#     collision: no def is resolved through the raw-symbol fallback to a
#     different-named def).
#   - flag-ON diagnostic md5 == flag-OFF diagnostic md5 on every thread count
#     (the flip is observationally byte-identical on output; it only fixes
#     identity, never changes a diagnostic).
#   - flag-OFF md5 is stable across thread counts (it is today: the harness
#     records it so a regression is visible).
#
# Until brick-3 lands, the flag is unrecognized and flag-ON == flag-OFF
# (counter stays at the baseline); that is EXPECTED — this script + the recorded
# baseline are what brick-3 validates against.
#
# Usage:
#   scripts/bench/canonical-defid-harness/measure.sh [fixture-tsconfig] [tsz-bin]
# Defaults: the multi-file DOM witness + .target/release/tsz.
#
# NOTE: deliberately NOT `set -e`/`pipefail` — tsz exits non-zero when it emits
# diagnostics and grep exits 1 on no match; both are normal here. Failures are
# handled explicitly (`${var:-NA}`, verdict column).
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"

TSCONFIG="${1:-$HERE/fixtures/dom_multi.tsconfig.json}"
TSZ_BIN="${2:-$ROOT/.target/release/tsz}"
THREADS=(1 2 4 8 16)

if [[ ! -x "$TSZ_BIN" ]]; then
  echo "error: tsz binary not found/executable: $TSZ_BIN" >&2
  echo "  build it first: cargo build --release -p tsz-cli" >&2
  exit 2
fi

# One compile run. Echoes "<counter>\t<diag_md5>".
#   $1 = thread count, $2 = canonical-defid flag value (0/1)
run_one() {
  local threads="$1" flag="$2"
  local counter diag_md5 raw diag_raw
  # tsz exits non-zero when it emits diagnostics, and grep exits 1 on no match;
  # neither is a harness error, so run these pipelines tolerant of both
  # (pipefail/-e off inside the subshell).
  # Counter side: needs TSZ_PERF_COUNTERS + --extendedDiagnostics to surface the
  # perf-counter dump; the label printed is "wrong-decl collisions".
  raw="$(RAYON_NUM_THREADS="$threads" TSZ_CANONICAL_DEFID="$flag" TSZ_PERF_COUNTERS=1 \
    TSZ_USE_EMBEDDED_LIBS=1 RUST_MIN_STACK=536870912 \
    "$TSZ_BIN" --noEmit --extendedDiagnostics -p "$TSCONFIG" 2>&1 || true)"
  counter="$(printf '%s\n' "$raw" | grep -iE "wrong-decl collisions" | grep -oE "[0-9]+" | head -1)"
  counter="${counter:-NA}"
  # Diagnostic side: a separate clean run (no extendedDiagnostics noise), sorted
  # so thread-order does not perturb the md5; only the diagnostic *set* matters.
  diag_raw="$(RAYON_NUM_THREADS="$threads" TSZ_CANONICAL_DEFID="$flag" \
    TSZ_USE_EMBEDDED_LIBS=1 RUST_MIN_STACK=536870912 \
    "$TSZ_BIN" --noEmit -p "$TSCONFIG" 2>&1 || true)"
  diag_md5="$(printf '%s\n' "$diag_raw" | grep -E "error TS" | sort | { md5 -q 2>/dev/null || md5sum | cut -d' ' -f1; })"
  printf '%s\t%s\n' "$counter" "$diag_md5"
}

echo "# #14344 flip validation harness"
echo "# fixture : $TSCONFIG"
echo "# tsz     : $TSZ_BIN"
echo "# columns : threads | OFF_counter | OFF_md5 | ON_counter | ON_md5 | verdict"
echo

off_md5_ref=""
fail=0
for t in "${THREADS[@]}"; do
  IFS=$'\t' read -r off_counter off_md5 < <(run_one "$t" 0) || true
  IFS=$'\t' read -r on_counter on_md5 < <(run_one "$t" 1) || true
  if [[ -z "$off_md5_ref" ]]; then off_md5_ref="$off_md5"; fi

  verdict="ok"
  # md5 must be stable across threads (flag OFF) and identical OFF==ON.
  if [[ "$off_md5" != "$off_md5_ref" ]]; then verdict="OFF-MD5-DRIFT"; fail=1; fi
  if [[ "$on_md5" != "$off_md5" ]]; then verdict="ON!=OFF-MD5"; fail=1; fi
  # When the flip is active, the counter must reach 0. (Pre-flip it stays at the
  # baseline and `ON==OFF`, which this harness reports without failing — the
  # caller compares against the recorded baseline.)
  printf '%-7s | %-11s | %s | %-10s | %s | %s\n' \
    "$t" "$off_counter" "$off_md5" "$on_counter" "$on_md5" "$verdict"
done

echo
if [[ "$fail" -eq 0 ]]; then
  echo "HARNESS OK: md5 stable across threads and OFF==ON (pre-flip baseline shape)."
else
  echo "HARNESS DRIFT: see verdict column — md5 instability is a regression."
fi
echo "FLIP SUCCESS = flag-ON counter==0 on every thread AND ON_md5==OFF_md5."
