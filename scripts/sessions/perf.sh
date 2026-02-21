#!/usr/bin/env bash
cat <<'PROMPT'
You are working in /Users/mohsenazimi/code/tsz.
Goal: make tsz 2x faster than tsgo across all benchmarks.

Steps:
1) git pull origin main
2) Read CLAUDE.md
3) Run: ./scripts/bench-vs-tsgo.sh --quick
4) Analyze the benchmark output carefully:

   PRIORITY 1 — Fix type-check failures first:
   If any benchmark file shows "tsz error" (tsz fails to type-check a file
   that tsgo handles), fix the type-checking bug BEFORE doing any perf work.
   A benchmark we can't even run is worse than a slow benchmark. These fixes
   count as your commit for this session.

   PRIORITY 2 — Optimize slowest benchmarks:
   Look at the ratio column. Our target is 2x faster than tsgo on every
   benchmark. Focus on:
   - Any benchmark where tsgo wins (ratio < 1.0) — these are regressions
   - Any benchmark where tsz wins but ratio < 2.0 — not yet at target
   - Pick the one with the worst ratio and investigate why it's slow

5) For type-check failures: diagnose the error, implement the minimal fix
   in checker/solver, verify the file now type-checks correctly
6) For perf work: profile the slow benchmark using flamegraph or sampling
   profiler, identify the hottest function, implement a targeted optimization
7) Write a unit test:
   - For type-check fixes: test the specific Rust logic you changed
   - For perf fixes: test correctness of the optimized path
   - Run: cargo nextest run -p <crate> to verify
8) Re-run: ./scripts/bench-vs-tsgo.sh --quick to verify improvement
   - Confirm no new "tsz error" entries appeared
   - Confirm the ratio improved (or at minimum didn't regress)
9) Run cargo nextest run to catch regressions
10) Create ONE small commit and push: git push origin main
11) Append any issues you investigated but punted on (too complex, needs
    architecture work, blocked by another issue, etc.) to
    docs/todos/perf.md — include function/module, current ratio, and a
    one-line reason why you skipped it.
12) git add docs/todos/perf.md (if changed) and amend or create a
    second commit, then push: git push origin main

Target: tsz should be ≥2x faster than tsgo on every benchmark. We are not
done until every row in the benchmark table shows tsz winning with ratio ≥2.0.

Do not ask user questions. Keep going until this run is complete.
PROMPT
