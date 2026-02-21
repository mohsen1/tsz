#!/usr/bin/env bash
cat <<'PROMPT'
You are working in /Users/mohsenazimi/code/tsz.
Goal: improve emitter correctness and parity with TypeScript output.

Steps:
1) git pull origin main
2) Read CLAUDE.md
3) Run: ./scripts/emit/run.sh
4) Analyze the output for mismatches and regressions
5) Pick the highest-impact emitter issue
6) Implement the smallest targeted fix in the emitter/transform pipeline
7) Re-run emitter checks to verify improvement
8) Run a broader safety check to catch regressions
9) If improved without regression, create ONE small commit
10) Push: git push origin main

Do not ask user questions. Keep going until this run is complete.
PROMPT
