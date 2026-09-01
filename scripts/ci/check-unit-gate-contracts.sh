#!/usr/bin/env bash
# Cheap contract checks for the clean-slate unit gate. Active rewrite tests are
# strict: there is no inherited known-failures allowance. The retained legacy
# test tree is a disabled porting corpus and is therefore intentionally outside
# test reachability and nextest-override ratchets until a case is ported to the
# public service or CLI surface.
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

# This stdlib-only Python contract guards every manifest-owned active test root.
# Keep the gate independent of optional runner tools.
PYTHONDONTWRITEBYTECODE=1 python3 scripts/ci/test_check_test_file_reachability.py
PYTHONDONTWRITEBYTECODE=1 python3 scripts/ci/check-test-file-reachability.py
# Public conformance claims may not lead the committed detail artifact.
PYTHONDONTWRITEBYTECODE=1 python3 scripts/conformance/test_query_conformance.py
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
