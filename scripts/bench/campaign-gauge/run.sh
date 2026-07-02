#!/usr/bin/env bash
# Committed gauge for the #14344/#14345 identity + materialize-once campaign.
#
# The campaign's asset is almost entirely flag-ON behavior that nothing
# exercises, and every direction-driving number was hand-run with no committed
# script (issue #15317). This script is the single source of truth for the
# composed substrate stack: it exports the exact flag set once, then
#   - flag-tests (GATING): runs the flag-ON tests designed to pass under the
#     stack — the hkt body-publication tests plus the channel-registry /
#     election-ordering unit tests;
#   - determinism (GATING): compiles a committed cross-file HKT-augmentation
#     fixture N times and asserts byte-identical diagnostics (the flap guard);
#   - census (NON-GATING): runs the full solver suite under the stack and banks
#     the pass/fail envelope — the 2^13 composition space is NOT green by design,
#     so this reports drift rather than gating.
#
# Usage:
#   scripts/bench/campaign-gauge/run.sh [all|flag-tests|determinism|census]
#
# Env overrides:
#   TSZ_GAUGE_REPEATS   determinism repeat count (default 3)
#   TSZ_BIN             path to a prebuilt `tsz` binary for the determinism check
#                       (default: build `-p tsz-cli --release` into target/release)
#   TSZ_FPTS_DIR        optional fp-ts fixture dir; when present, the fp-ts row is
#                       run REPEATS times and its diagnostic count asserted stable
#
# Exit non-zero on any suite failure, a determinism mismatch, or a fixture the
# gauge could build but that produced non-reproducible output.
set -euo pipefail

MODE="${1:-all}"
REPEATS="${TSZ_GAUGE_REPEATS:-3}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
FIXTURE_DIR="$ROOT/scripts/bench/campaign-gauge/fixture"

# --- The composed substrate stack (single source of truth) -------------------
# Keep in sync with CAMPAIGN_STORE_CHANNELS in
# crates/tsz-solver/src/def/core/campaign_channels.rs and the ledger at
# docs/plan/campaign-flag-ledger.md. The determinism-election override is forced
# ON explicitly so a channel read in tsz-checker (which the solver derivation
# cannot reach) still pins allocation order in this lane.
# The 12 substrate channels, in the same order as CAMPAIGN_STORE_CHANNELS (the
# const is the ordering authority), plus the explicit election override.
CAMPAIGN_FLAGS=(
  TSZ_INST_RESOLVER_REREDUCE
  TSZ_OPTIONB_STORE_RESOLVER
  TSZ_INFER_HKT_REDUCE
  TSZ_TYPEPARAM_DECL_IDENTITY
  TSZ_XARENA_BASE_DECL
  TSZ_XARENA_HERITAGE_TYPEARG
  TSZ_TYPEOF_URI_SELFLOOP
  TSZ_AUGMENTED_BODY_SYMBOL_REDIRECT
  TSZ_MODULE_AUG_SYMBOL_EDGE
  TSZ_MODULE_AUG_BODY_PUBLISH
  TSZ_ALPHA_NAME_PAIR
  TSZ_LAZY_REF_RELATION
  TSZ_DETERMINISTIC_STORE_ELECTION
)

export_stack() {
  for f in "${CAMPAIGN_FLAGS[@]}"; do
    export "$f=1"
  done
}

have_nextest() { command -v cargo-nextest >/dev/null 2>&1; }

# GATING: the flag-ON tests that are designed to pass under the composed stack.
# The hkt augmentation body-publication tests also run in the default suite via
# the deterministic Mode B priming pass; keep them here so the campaign stack
# continues exercising the same witness.
run_flag_tests() {
  echo "== campaign-gauge: designed-green flag-ON tests =="
  cargo test -p tsz-checker --test hkt_cross_file_augmentation_13653_repro
  # The campaign channel registry + the deterministic-election ordering
  # invariants (def::core::tests::from_semantic_defs_*). cargo test accepts a
  # single filter substring, so run the two families separately.
  cargo test -p tsz-solver --lib def::core::campaign_channels
  cargo test -p tsz-solver --lib from_semantic_defs
}

# NON-GATING census: the full solver suite under the composed stack. Many unit
# tests encode flag-OFF expectations, so the 2^13 composition space is NOT
# green — that is the point. We run with --no-fail-fast and print the pass/fail
# envelope into the job log, then always exit 0. This is a crash/hang smoke run
# plus a printed snapshot (there is no committed baseline to diff against yet);
# a hard crash/hang is surfaced (the process aborts / the job times out), while
# assertion deltas are informational.
run_census() {
  echo "== campaign-gauge: composed-stack solver census (non-gating) =="
  local out rc=0
  if have_nextest; then
    out="$(cargo nextest run -p tsz-solver --no-fail-fast 2>&1)" || rc=$?
    echo "$out" | grep -E "Summary|tests run|FAIL" | tail -20 || true
  else
    out="$(cargo test -p tsz-solver --lib --no-fail-fast 2>&1)" || rc=$?
    echo "$out" | grep -E "test result:" | tail -5 || true
  fi
  echo "campaign-gauge census: solver suite exit=$rc under the composed stack (non-gating)"
  return 0
}

resolve_bin() {
  if [[ -n "${TSZ_BIN:-}" && -x "${TSZ_BIN:-}" ]]; then
    echo "$TSZ_BIN"
    return
  fi
  echo "== campaign-gauge: building tsz-cli --release ==" >&2
  cargo build -p tsz-cli --release >&2
  # Same target-dir idiom the rest of the repo uses (.cargo/config.toml sets
  # .target; CARGO_TARGET_DIR overrides).
  echo "${CARGO_TARGET_DIR:-$ROOT/.target}/release/tsz"
}

hash_output() {
  # Stable, order-independent SHA-256 digest of the diagnostic lines (matches the
  # repo's sha256 convention with the macOS shasum fallback).
  LC_ALL=C sort | { sha256sum 2>/dev/null || shasum -a 256; } | awk '{print $1}'
}

run_determinism() {
  echo "== campaign-gauge: determinism ($REPEATS repeats) =="
  local bin
  bin="$(resolve_bin)"
  [[ -x "$bin" ]] || { echo "error: tsz binary not executable: $bin" >&2; exit 1; }

  local first="" cur=""
  for i in $(seq 1 "$REPEATS"); do
    # tsz exits non-zero whenever it emits diagnostics; under pipefail that would
    # abort before the comparison. We want the hash of whatever it printed
    # regardless of exit code — a flap that *adds* diagnostics must be caught by
    # the hash differing, not swallowed as a mid-substitution abort.
    cur="$("$bin" --noEmit -p "$FIXTURE_DIR/tsconfig.json" 2>&1 | hash_output || true)"
    echo "  fixture run $i: $cur"
    if [[ -z "$first" ]]; then
      first="$cur"
    elif [[ "$cur" != "$first" ]]; then
      echo "error: campaign fixture output is NON-DETERMINISTIC across runs" >&2
      exit 1
    fi
  done
  echo "  fixture deterministic: $first"

  if [[ -n "${TSZ_FPTS_DIR:-}" && -f "${TSZ_FPTS_DIR}/tsconfig.json" ]]; then
    echo "== campaign-gauge: fp-ts row ($REPEATS repeats) =="
    local fcount fbase=""
    for i in $(seq 1 "$REPEATS"); do
      fcount="$("$bin" --noEmit -p "$TSZ_FPTS_DIR/tsconfig.json" 2>&1 | grep -c 'error TS' || true)"
      echo "  fp-ts run $i: $fcount diagnostics"
      if [[ -z "$fbase" ]]; then
        fbase="$fcount"
      elif [[ "$fcount" != "$fbase" ]]; then
        echo "error: fp-ts diagnostic count is NON-DETERMINISTIC ($fbase vs $fcount)" >&2
        exit 1
      fi
    done
    echo "  fp-ts diagnostic count stable: $fbase"
  else
    echo "== campaign-gauge: fp-ts fixture absent (set TSZ_FPTS_DIR to enable) — skipped =="
  fi
}

cd "$ROOT"
export_stack
echo "campaign-gauge stack: ${CAMPAIGN_FLAGS[*]}"

case "$MODE" in
  flag-tests)  run_flag_tests ;;
  determinism) run_determinism ;;
  census)      run_census ;;
  all)         run_flag_tests; run_determinism; run_census ;;
  *)
    echo "usage: $0 [all|flag-tests|determinism|census]" >&2
    exit 2
    ;;
esac

echo "== campaign-gauge: OK =="
