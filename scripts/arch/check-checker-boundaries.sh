#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

REPORT_DIR="$ROOT_DIR/artifacts/architecture"
REPORT_PATH="$REPORT_DIR/arch_guard_report.json"
REPORT_MD_PATH="$REPORT_DIR/arch_guard_report.md"

mkdir -p "$REPORT_DIR"
python3 scripts/arch/arch_guard.py --json-report "$REPORT_PATH"
python3 scripts/arch/render_architecture_report.py
echo "Architecture guard report: $REPORT_PATH"
echo "Architecture markdown report: $REPORT_MD_PATH"

# T2.1.A field-lifetime inventory: every CheckerContext field must be
# classified in `crates/tsz-checker/src/context/checker_context_lifetimes.toml`.
# See `docs/plan/PERFORMANCE_PLAN.md` §6.
python3 scripts/arch/checker_field_inventory.py
