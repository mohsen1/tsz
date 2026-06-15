#!/usr/bin/env bash
# Run the AST-native architecture invariants (ast-grep) over the repo (#13451).
#
# This is the AST-native successor to the regex `pattern_checks` in
# `scripts/arch/arch_guard_policy.toml`. It scans real tree-sitter Rust syntax,
# so a forbidden token inside a comment, string literal, or `trace!(...)` macro
# is never a match.
#
# Usage:
#   scripts/arch/run-ast-grep.sh           # scan: fail (exit 1) on any violation
#   scripts/arch/run-ast-grep.sh test      # run the rule pass/fail fixtures
#   scripts/arch/run-ast-grep.sh scan      # explicit scan (same as no arg)
#
# ast-grep is pinned to AST_GREP_VERSION for reproducibility (cf. the repo's
# baseline-staleness traps with unpinned tooling). The runner prefers a
# locally installed `ast-grep`/`sg` binary at the pinned version; otherwise it
# falls back to `npx -p @ast-grep/cli@<version> ast-grep`, which downloads the
# pinned package on demand without a persistent install.
set -euo pipefail

AST_GREP_VERSION="0.43.0"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SGCONFIG="scripts/arch/ast-grep/sgconfig.yml"

cd "$ROOT_DIR"

# Resolve an ast-grep runner pinned to AST_GREP_VERSION.
resolve_ast_grep() {
  local candidate
  for candidate in ast-grep sg; do
    if command -v "$candidate" >/dev/null 2>&1; then
      if "$candidate" --version 2>/dev/null | grep -q "${AST_GREP_VERSION}\$"; then
        AST_GREP=("$candidate")
        return 0
      fi
    fi
  done
  if command -v npx >/dev/null 2>&1; then
    # npx invokes the `ast-grep` bin from the @ast-grep/cli package; the package
    # name differs from the binary name, so -p is required.
    AST_GREP=(npx --yes -p "@ast-grep/cli@${AST_GREP_VERSION}" ast-grep)
    return 0
  fi
  echo "error: ast-grep ${AST_GREP_VERSION} not found and npx unavailable." >&2
  echo "       install with: npm install -g @ast-grep/cli@${AST_GREP_VERSION}" >&2
  return 127
}

resolve_ast_grep

mode="${1:-scan}"
case "$mode" in
  test)
    exec "${AST_GREP[@]}" test -c "$SGCONFIG"
    ;;
  scan)
    # --error promotes every rule's severity to error so any match exits 1.
    exec "${AST_GREP[@]}" scan -c "$SGCONFIG" --error
    ;;
  *)
    echo "usage: $0 [scan|test]" >&2
    exit 2
    ;;
esac
