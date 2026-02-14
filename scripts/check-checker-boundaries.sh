#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

REPORT_DIR="$ROOT_DIR/artifacts/architecture"
REPORT_PATH="$REPORT_DIR/arch_guard_report.json"
REPORT_MD_PATH="$REPORT_DIR/arch_guard_report.md"

mkdir -p "$REPORT_DIR"
python3 scripts/arch_guard.py --json-report "$REPORT_PATH"
python3 scripts/render_architecture_report.py
echo "Architecture guard report: $REPORT_PATH"
echo "Architecture markdown report: $REPORT_MD_PATH"
