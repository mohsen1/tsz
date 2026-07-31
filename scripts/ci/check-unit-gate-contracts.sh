#!/usr/bin/env bash
# Single owner of the cheap per-merge repo-hygiene contract checks: the
# known-failures unit-gate baseline growth/integrity gate and its wiring
# contract tests (#15646), plus the orphaned-test-file reachability guard
# (#16013). Both the ci.yml cheap-guards step and full-ci.sh run_lint invoke
# this script, so the two tiers cannot drift.
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

# In CI the growth gate must hard-fail when the base cannot be fetched — a
# transient network failure must not silently degrade the ratchet to
# integrity-only. Local/sandboxed runs (no $CI) keep the warn-and-skip
# tolerance.
growth_args=(--fetch-base)
if [[ -z "${CI:-}" ]]; then
  growth_args+=(--allow-unavailable-base)
fi
python3 scripts/ci/check-known-failures-growth.py "${growth_args[@]}"
python3 scripts/ci/test_check_known_failures_growth.py
python3 scripts/ci/test_unit_nextest.py
python3 scripts/ci/test_full_ci_unit_gate.py
node scripts/ci/test-known-failures-check.mjs
python3 scripts/ci/check-test-file-reachability.py
python3 scripts/ci/test_check_test_file_reachability.py
