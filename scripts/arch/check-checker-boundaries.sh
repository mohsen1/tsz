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

# AST-native architecture invariants (#13451). The ast-grep rules under
# `scripts/arch/ast-grep/` are the structural successors to the regex
# `pattern_checks` in `arch_guard_policy.toml`. Enforced wherever ast-grep (or
# npx, which fetches the pinned @ast-grep/cli) is available; skipped with a
# notice otherwise so environments without node/npx are not blocked. A dedicated
# `sg scan` CI job is the long-term home (#13451 proposal step 3).
if command -v ast-grep >/dev/null 2>&1 \
  || command -v sg >/dev/null 2>&1 \
  || command -v npx >/dev/null 2>&1; then
  echo "Running AST-native architecture invariants (ast-grep)..."
  scripts/arch/run-ast-grep.sh scan
else
  echo "ast-grep/npx not found; skipping AST-native architecture invariants." >&2
fi

# T2.1.A field-lifetime inventory: every CheckerContext field must be
# classified in `crates/tsz-checker/src/context/checker_context_lifetimes.toml`.
# See `docs/plan/PERFORMANCE_PLAN.md` §6.
python3 scripts/arch/checker_field_inventory.py
