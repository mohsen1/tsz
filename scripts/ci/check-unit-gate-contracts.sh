#!/usr/bin/env bash
# Cheap contract checks for the clean-slate unit gate. Active rewrite tests are
# strict: there is no inherited known-failures allowance. The retained legacy
# test tree is a disabled porting corpus and is therefore intentionally outside
# test reachability and nextest-override ratchets until a case is ported to the
# public service or CLI surface.
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

# Active rewrite tests are promises, not a known-failure inventory. Scan both
# active roots with multiline PCRE so whitespace and `cfg_attr(..., ignore)`
# forms cannot bypass the gate. The line anchor keeps comments and fixture
# strings from manufacturing matches.
if active_ignores="$(rg --pcre2 -U -n '^\s*#\s*\[[^]]*\bignore\b[^]]*\]' \
  crates/tsz-core/rewrite-tests \
  crates/tsz-cli/rewrite-tests)"; then
  printf '%s\n' "$active_ignores" >&2
  echo "active #[ignore] attributes are forbidden in rewrite tests" >&2
  exit 1
else
  ignore_scan_status=$?
  if [[ "$ignore_scan_status" -ne 1 ]]; then
    echo "failed to scan rewrite tests for active #[ignore] attributes" >&2
    exit "$ignore_scan_status"
  fi
fi

python3 scripts/ci/test_unit_nextest.py
python3 scripts/ci/test_full_ci_unit_gate.py
python3 scripts/ci/test_suite_metadata.py
python3 scripts/ci/test_ci_resources.py
node scripts/ci/test-gate-path-classifier.mjs
node scripts/ci/test-pr-ready-state.mjs
# Canonical fourslash verdicts must come only from tsz-server. This guard
# rejects native-LS substitution, fixture arbitration, and non-failing xfails.
node scripts/fourslash/session-client-truth.test.cjs
# The dependency-stub model can erase semantic work and manufacture green
# project rows. Keep both its parser and the committed non-growth ratchet in
# the strict gate so no new `any` surface is added silently.
node scripts/bench/test-project-fixture-stub-fidelity.mjs
node scripts/bench/project-fixture-stub-fidelity.mjs --check
# Timing is reachable only after exact schema-v2 TSZ/TypeScript 7 project
# evidence; fake compilers pin every former false-green escape hatch.
node scripts/bench/test-bench-vs-tsgo-project-evidence.mjs
# Project graph statistics must use the repository's verified TypeScript 7
# installation. A fresh CI checkout intentionally has no node_modules yet, so
# prepare the exact pin here instead of letting the test skip or resolve an
# ambient package.
./scripts/setup/ensure-pinned-typescript.sh scripts
TSC_TOOL_DIR_VALUE="$ROOT_DIR/scripts" \
  TSC_BIN_VALUE="$ROOT_DIR/scripts/node_modules/typescript/bin/tsc" \
  node scripts/bench/test-project-file-stats.mjs
# Publishing independently revalidates the same graph/diagnostic/exit and
# zero-stub evidence, so a hand-forged or legacy artifact cannot bypass the
# producer-side admission gate.
node scripts/bench/test-check-artifact-readiness.mjs
# Retained workflow and reporting guards stay in the active rewrite gate. They
# prevent performance observations from silently losing Pages readiness,
# historical winner transitions, or diagnostic-reduction ownership.
node scripts/bench/test-bench-workflow-github-prep.mjs
node scripts/bench/test-gh-pages-benchmark-artifact-gate.mjs
node scripts/bench/test-project-winner-regression-report.mjs
node scripts/bench/test-reduction-backlog.mjs
# Oracle setup/provenance is stderr-only. Compiler stdout remains an exact raw
# stream, and the seed gate fails closed if metadata ever re-enters it.
python3 scripts/conformance/test_oracle_stream_contract.py
python3 scripts/reset/test_seed_oracle_stream_contract.py
# Keep the retained emit comparison machinery executable even though broad
# emit scores are observational during R0.
python3 scripts/emit/test_run_binary_resolution.py
python3 scripts/ci/test_check_emit_regression_set.py
