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
7) Write a unit test or benchmark test for the optimization:
   - Add #[test] functions verifying the optimized code path works correctly
   - If applicable, add a #[bench] or criterion benchmark to track the
     specific hot path
   - Focus on correctness of the optimization (e.g., cache hits, fast-path
     triggers, equivalence with the slow path)
   - Run: cargo nextest run -p <crate> to verify your test passes
8) Re-run benchmarks to verify improvement
9) Run cargo nextest run to catch regressions
10) If improved without regression, create ONE small commit (include the test)
11) Append any issues you investigated but punted on (too complex, needs
    architecture work, blocked by another issue, etc.) to
    docs/todos/perf.md — include function/module and a one-line reason
    why you skipped it.
12) git add docs/todos/perf.md (if changed) and amend or create a
    second commit, then push: git push origin main

Do not ask user questions. Keep going until this run is complete.
PROMPT
