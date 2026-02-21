#!/usr/bin/env bash
cat <<'PROMPT'
You are working in /Users/mohsenazimi/code/tsz.
Goal: improve LSP correctness and fourslash test parity.

Steps:
1) git pull origin main
2) Read CLAUDE.md
3) Run: ./scripts/run-fourslash.sh --max 200
4) Analyze failures for highest-impact issues
5) Pick one high-signal LSP issue (hover/definition/references/completions/rename/diagnostics)
6) Implement the smallest targeted fix
7) Re-run fourslash tests to verify improvement
8) Run a broader safety check to catch regressions
9) If improved without regression, create ONE small commit
10) Push: git push origin main

Do not ask user questions. Keep going until this run is complete.
PROMPT
