#!/usr/bin/env bash
# Cheap contract checks for the clean-slate unit gate. Active rewrite tests are
# strict: there is no inherited known-failures allowance. The retained legacy
# test tree is a disabled porting corpus and is therefore intentionally outside
# test reachability and nextest-override ratchets until a case is ported to the
# public service or CLI surface.
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

python3 scripts/ci/test_unit_nextest.py
python3 scripts/ci/test_full_ci_unit_gate.py
python3 scripts/ci/test_suite_metadata.py
python3 scripts/ci/test_ci_resources.py
node scripts/ci/test-gate-path-classifier.mjs
node scripts/ci/test-pr-ready-state.mjs
# Keep the retained emit comparison machinery executable even though broad
# emit scores are observational during R0.
python3 scripts/ci/test_check_emit_regression_set.py
