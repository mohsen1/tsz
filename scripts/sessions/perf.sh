#!/usr/bin/env bash
cat <<'PROMPT'
You are working in /Users/mohsenazimi/code/tsz.
Goal: improve performance and reduce hot-path overhead.

Steps:
1) git pull origin main
2) Read CLAUDE.md
3) Run: ./scripts/bench-vs-tsgo.sh --quick
4) Profile hotspots using flamegraph or sampling profiler
5) Identify the highest-impact optimization opportunity
6) Implement a targeted optimization (avoid over-engineering)
7) Re-run benchmarks to verify improvement
8) Run cargo nextest run to catch regressions
9) If improved without regression, create ONE small commit
10) Push: git push origin main

Do not ask user questions. Keep going until this run is complete.
PROMPT
