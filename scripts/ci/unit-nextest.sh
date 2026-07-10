#!/usr/bin/env bash
# unit-nextest.sh — single source of truth for the delta-verified unit suite
# (#15646).
#
# Runs the unit nextest passes under the `signoff` nextest profile (fail-fast
# disabled, junit on) and the `ci-unit` cargo profile, collecting one junit
# report per pass into --junit-dir. With --gate it then adjudicates the union
# of those reports against scripts/ci/known-failures.txt via
# scripts/ci/known-failures-check.mjs and exits with the gate's verdict —
# THAT check is the pass/fail gate; the passes themselves only distinguish
# "tests ran to completion" from infrastructure failure.
#
# Pass structure (mirrors the memory constraints documented in Cargo.toml's
# [profile.ci-unit] note):
#   * one "general" pass over every requested package except tsz-checker;
#   * tsz-checker's declared [[test]] targets in batches (default 40), so no
#     single cargo invocation links an unbounded target set. The tsz-checker
#     lib-test target is NEVER built: compiling it exceeds a 32 GiB runner
#     even at codegen-units=1/opt-level=0/debug=false, so its in-crate unit
#     tests stay outside this gate (as they always were in the unit CI job).
#   * each batch's test binaries are pruned after its junit is collected
#     (see the note at prune_batch_artifacts; disable with
#     --keep-batch-artifacts on machines with ~70 GB of free target-dir
#     space that want incremental relink reuse across runs).
#
# The two callers intentionally select different populations feeding the ONE
# shared baseline: the unit CI job passes the time-budgeted core package set
# (full-ci.sh `_UNIT_TEST_PACKAGES`), while signoff.sh passes
# --workspace-minus-checker (a superset that adds tsz-cli, tsz-lowering,
# tsz-wasm, ...). The baseline is reconciled from the superset; the gate's
# semantics tolerate subsets (a baselined test absent from a run is neither a
# new failure nor a shrink), so a green CI run and a green signoff run both
# mean "no NEW failures in what actually ran".
#
# Exit codes:
#   0 - every pass completed and recorded a junit report; with --gate, the
#       known-failures check also found no new failures
#   1 - (--gate only) the known-failures check found new failures
#   2 - bad arguments, or (--gate) gate configuration error
#   3 - a pass completed but its junit report is missing (treated as infra)
#   * - the first infrastructure failure rc from cargo/nextest (build error,
#       nextest crash); remaining passes are not attempted
#
# Per-pass rc classification (rc-aggregation kept for detection/logging,
# issues #15404/#15646): 0 = green, 100 = test failures (recorded, run
# continues), 4 = no tests in selection (no junit expected), anything else =
# infrastructure failure.
#
# Usage:
#   scripts/ci/unit-nextest.sh --junit-dir DIR \
#     ( --packages "pkg [pkg...]" | --workspace-minus-checker ) \
#     [--gate] [--allow-no-reports] [--skip-checker-integration] \
#     [--checker-batch-size N] [--keep-batch-artifacts]
#
# Environment:
#   CARGO_BUILD_JOBS               forwarded as --build-jobs when set
#   UNIT_NEXTEST_TEST_THREADS      forwarded as --test-threads for the
#                                  general pass when set
#   CHECKER_NEXTEST_TEST_THREADS   forwarded as --test-threads for the
#                                  checker batches when set (hosted runners
#                                  cap this; memory-heavy checker tests can
#                                  be SIGKILLed at num-cpus there)
#   TSZ_CI_CHECKER_TEST_BATCH_SIZE default checker batch size (40)
#   TSZ_KEEP_BATCH_ARTIFACTS=1     same as --keep-batch-artifacts (for flows
#                                  with fixed command lines, e.g. signoff.sh)

set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

usage() {
  # Print the header comment block (everything up to the first blank line).
  sed -n '2,/^$/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

JUNIT_DIR=""
PACKAGES=""
WORKSPACE_MINUS_CHECKER=0
SKIP_CHECKER_INTEGRATION=0
GATE=0
ALLOW_NO_REPORTS=0
KEEP_BATCH_ARTIFACTS="${TSZ_KEEP_BATCH_ARTIFACTS:-0}"
CHECKER_BATCH_SIZE="${TSZ_CI_CHECKER_TEST_BATCH_SIZE:-40}"

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --junit-dir)
      JUNIT_DIR="${2:?--junit-dir needs a value}"
      shift 2
      ;;
    --packages)
      PACKAGES="${2:?--packages needs a value}"
      shift 2
      ;;
    --workspace-minus-checker)
      WORKSPACE_MINUS_CHECKER=1
      shift
      ;;
    --gate)
      GATE=1
      shift
      ;;
    --allow-no-reports)
      ALLOW_NO_REPORTS=1
      shift
      ;;
    --skip-checker-integration)
      SKIP_CHECKER_INTEGRATION=1
      shift
      ;;
    --checker-batch-size)
      CHECKER_BATCH_SIZE="${2:?--checker-batch-size needs a value}"
      shift 2
      ;;
    --keep-batch-artifacts)
      KEEP_BATCH_ARTIFACTS=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument '$1'" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$JUNIT_DIR" ]]; then
  echo "error: --junit-dir is required" >&2
  exit 2
fi
if [[ "$WORKSPACE_MINUS_CHECKER" == "1" && -n "$PACKAGES" ]]; then
  echo "error: --packages and --workspace-minus-checker are mutually exclusive" >&2
  exit 2
fi
if [[ "$WORKSPACE_MINUS_CHECKER" == "0" && -z "$PACKAGES" ]]; then
  echo "error: one of --packages or --workspace-minus-checker is required" >&2
  exit 2
fi
if ! [[ "$CHECKER_BATCH_SIZE" =~ ^[0-9]+$ ]] || (( CHECKER_BATCH_SIZE < 1 )); then
  echo "error: --checker-batch-size must be a positive integer" >&2
  exit 2
fi

mkdir -p "$JUNIT_DIR"
rm -f "$JUNIT_DIR"/*.xml

METADATA_JSON="$(cargo metadata --no-deps --format-version 1)"
CARGO_TARGET_DIRECTORY="$(printf '%s' "$METADATA_JSON" | jq -r '.target_directory')"

# The signoff junit report location. nextest resolves the configured junit
# path relative to its per-profile store directory. Observed with nextest
# 0.9.137 in this repo: the store roots at workspace-root `target/nextest`
# even though the cargo target dir is `.target` — so the workspace-root
# candidate is the live one; the cargo-target-dir variant stays as a fallback
# for nextest versions that root the store under the target directory.
JUNIT_CANDIDATES=(
  "$ROOT_DIR/target/nextest/signoff/junit.xml"
  "$CARGO_TARGET_DIRECTORY/nextest/signoff/junit.xml"
)

COMMON_FLAGS=()
if [[ -n "${CARGO_BUILD_JOBS:-}" ]]; then
  COMMON_FLAGS+=(--build-jobs "$CARGO_BUILD_JOBS")
fi

PASS_RC_LOG=""
TEST_FAILURE_PASSES=0
COLLECTED=0

# Run one nextest pass and collect its junit into $JUNIT_DIR/<label>.xml.
# Returns nonzero only for infrastructure failures (see header).
run_pass() {
  local label="$1"
  shift
  local rc=0
  rm -f "${JUNIT_CANDIDATES[@]}"
  echo "info: unit-nextest pass '$label' starting"
  set +e
  cargo nextest run --profile signoff --cargo-profile ci-unit "$@"
  rc="$?"
  set -e
  case "$rc" in
    0|100)
      ;;
    4)
      # No tests matched the selection (possible under a narrowed package
      # override). Nothing to collect; not an error.
      echo "info: unit-nextest pass '$label' selected no tests (rc=4)"
      PASS_RC_LOG="$PASS_RC_LOG $label=4"
      return 0
      ;;
    *)
      echo "error: unit-nextest pass '$label' failed with infrastructure rc=$rc" >&2
      return "$rc"
      ;;
  esac
  local src=""
  local candidate
  for candidate in "${JUNIT_CANDIDATES[@]}"; do
    if [[ -f "$candidate" ]]; then
      src="$candidate"
      break
    fi
  done
  if [[ -z "$src" ]]; then
    echo "error: unit-nextest pass '$label' exited rc=$rc but no junit report" \
      "was found at: ${JUNIT_CANDIDATES[*]}" >&2
    return 3
  fi
  mv "$src" "$JUNIT_DIR/$label.xml"
  COLLECTED=$((COLLECTED + 1))
  PASS_RC_LOG="$PASS_RC_LOG $label=$rc"
  if [[ "$rc" == "100" ]]; then
    TEST_FAILURE_PASSES=$((TEST_FAILURE_PASSES + 1))
    echo "info: unit-nextest pass '$label' had test failures (rc=100);" \
      "recorded in $JUNIT_DIR/$label.xml for the known-failures gate"
  fi
  return 0
}

checker_selected=0
general_args=()
if [[ "$WORKSPACE_MINUS_CHECKER" == "1" ]]; then
  checker_selected=1
  general_args=(--workspace --exclude tsz-checker)
else
  package=""
  for package in $PACKAGES; do
    if [[ "$package" == "tsz-checker" ]]; then
      checker_selected=1
    else
      general_args+=(-p "$package")
    fi
  done
fi

if (( ${#general_args[@]} > 0 )); then
  general_flags=()
  if [[ -n "${UNIT_NEXTEST_TEST_THREADS:-}" ]]; then
    general_flags+=(--test-threads "$UNIT_NEXTEST_TEST_THREADS")
  fi
  run_pass general \
    ${COMMON_FLAGS[@]+"${COMMON_FLAGS[@]}"} \
    ${general_flags[@]+"${general_flags[@]}"} \
    "${general_args[@]}"
fi

if (( checker_selected )) && [[ "$SKIP_CHECKER_INTEGRATION" == "0" ]]; then
  # See the header: batches keep any single cargo invocation's link set
  # bounded, and pruning keeps the run's peak disk bounded. Each checker
  # test binary statically links the checker+solver stack and measures
  # ~78 MB even at ci-unit (debug=false): all 787 targets together would
  # need ~61 GB, which no hosted runner can keep resident. After a batch's
  # junit is collected its binaries are never needed again, so prune them
  # before the next batch — peak disk stays at libs + one batch (~3 GB)
  # instead of growing monotonically to ENOSPC partway through the run.
  batch_names=()
  batch_index=0
  prune_batch_artifacts() {
    if [[ "$KEEP_BATCH_ARTIFACTS" == "1" ]]; then
      return 0
    fi
    local prune_paths=()
    local test_name=""
    for test_name in "${batch_names[@]}"; do
      prune_paths+=("$CARGO_TARGET_DIRECTORY/ci-unit/deps/$test_name"-*)
    done
    rm -f "${prune_paths[@]}"
  }
  checker_flags=()
  if [[ -n "${CHECKER_NEXTEST_TEST_THREADS:-}" ]]; then
    checker_flags+=(--test-threads "$CHECKER_NEXTEST_TEST_THREADS")
  fi
  run_checker_batch() {
    local batch_args=()
    local test_name=""
    for test_name in "${batch_names[@]}"; do
      batch_args+=(--test "$test_name")
    done
    batch_index=$((batch_index + 1))
    echo "info: checker integration batch $batch_index (${#batch_names[@]} targets): ${batch_names[*]}"
    run_pass "$(printf 'checker-%02d' "$batch_index")" \
      ${COMMON_FLAGS[@]+"${COMMON_FLAGS[@]}"} \
      ${checker_flags[@]+"${checker_flags[@]}"} \
      -p tsz-checker "${batch_args[@]}"
    prune_batch_artifacts
  }
  test_name=""
  while IFS= read -r test_name; do
    [[ -n "$test_name" ]] || continue
    batch_names+=("$test_name")
    if (( ${#batch_names[@]} >= CHECKER_BATCH_SIZE )); then
      run_checker_batch
      batch_names=()
    fi
  done < <(printf '%s' "$METADATA_JSON" | jq -r '.packages[]
      | select(.name == "tsz-checker")
      | .targets[]
      | select(.kind[]? == "test")
      | .name' | sort)
  if (( ${#batch_names[@]} > 0 )); then
    run_checker_batch
  fi
elif (( checker_selected )); then
  echo "info: skipping checker integration passes (--skip-checker-integration)"
fi

echo "unit-nextest: collected $COLLECTED junit report(s) in $JUNIT_DIR;" \
  "$TEST_FAILURE_PASSES pass(es) with test failures; per-pass rc:${PASS_RC_LOG:- none}"

if (( GATE )); then
  gate_args=(--junit-dir "$JUNIT_DIR")
  if [[ "$ALLOW_NO_REPORTS" == "1" ]]; then
    gate_args+=(--allow-no-reports)
  fi
  node scripts/ci/known-failures-check.mjs "${gate_args[@]}"
fi
