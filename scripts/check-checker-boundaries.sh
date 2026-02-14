#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

REPORT_DIR="$ROOT_DIR/artifacts/architecture"
REPORT_PATH="$REPORT_DIR/arch_guard_report.json"

mkdir -p "$REPORT_DIR"
python3 scripts/arch_guard.py --json-report "$REPORT_PATH"
echo "Architecture guard report: $REPORT_PATH"
