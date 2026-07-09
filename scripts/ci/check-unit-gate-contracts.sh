#!/usr/bin/env bash
# Single owner of the known-failures unit-gate contract checks (#15646):
# the baseline growth/integrity gate plus the contract tests for the gate
# wiring. Both the ci.yml cheap-guards step and full-ci.sh run_lint invoke
# this script, so the two tiers cannot drift.
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

python3 scripts/ci/check-known-failures-growth.py \
  --fetch-base --allow-unavailable-base
python3 scripts/ci/test_check_known_failures_growth.py
python3 scripts/ci/test_unit_nextest.py
python3 scripts/ci/test_full_ci_unit_gate.py
node scripts/ci/test-known-failures-check.mjs
