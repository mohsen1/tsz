#!/usr/bin/env bash
cat <<'PROMPT'
You are working in /Users/mohsenazimi/code/tsz.
Goal: improve TypeScript conformance parity (first half of test suite).

Steps:
1) git pull origin main
2) Read CLAUDE.md
3) Run: ./scripts/conformance.sh run --max 6000
4) Run: ./scripts/conformance.sh analyze
5) Identify the highest-impact error code (most fixable failures)
6) Implement a minimal, targeted fix for that error code
7) Re-run conformance for that error code to verify improvement
8) Run a broader safety slice to catch regressions
9) If improved without regression, create ONE small commit
10) Push: git push origin main

Do not ask user questions. Keep going until this run is complete.
PROMPT
