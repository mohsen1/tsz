#!/usr/bin/env bash
cat <<'PROMPT'
You are working in /Users/mohsenazimi/code/tsz.
Goal: make tsz 2x faster than tsgo across all benchmarks — WITHOUT breaking
tests, conformance, or code maintainability.

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 1 — ESTABLISH BASELINES (before any changes)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

1) git pull origin main
2) Read CLAUDE.md
3) Record baselines — you MUST capture these numbers before changing anything:

   a) Run: cargo nextest run --no-fail-fast 2>&1 | tail -5
      Record the total tests passed/failed/skipped.

   b) Run: ./scripts/conformance.sh run 2>&1 | tail -20
      Record the conformance pass rate (e.g. "8941/12574 (71.1%)").

   c) Run: ./scripts/bench-vs-tsgo.sh --quick
      Record benchmark ratios.

   Write down these three baselines — you will compare against them later.

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 2 — IDENTIFY WORK
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

4) Analyze the benchmark output carefully:

   PRIORITY 1 — Fix type-check failures first:
   If any benchmark file shows "tsz error" (tsz fails to type-check a file
   that tsgo handles), fix the type-checking bug BEFORE doing any perf work.
   A benchmark we can't even run is worse than a slow benchmark.

   PRIORITY 2 — Optimize slowest benchmarks:
   Look at the ratio column. Our target is 2x faster than tsgo on every
   benchmark. Focus on:
   - Any benchmark where tsgo wins (ratio < 1.0) — these are regressions
   - Any benchmark where tsz wins but ratio < 2.0 — not yet at target
   - Pick the one with the worst ratio and investigate why it's slow

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 3 — IMPLEMENT (with maintainability constraints)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

5) For type-check failures: diagnose the error, implement the minimal fix
   in checker/solver, verify the file now type-checks correctly.

6) For perf work: profile the slow benchmark using flamegraph or sampling
   profiler, identify the hottest function, implement a targeted optimization.

   MAINTAINABILITY RULES — Every optimization MUST follow these:
   - Keep changes minimal and focused. One optimization per commit.
   - Do NOT introduce complex unsafe code unless absolutely necessary and
     the gain is >20%. Document why it's safe.
   - Do NOT inline large functions or duplicate code for speed. If a
     function is hot, optimize its internals, don't copy-paste it.
   - Do NOT add feature flags, conditional compilation, or runtime switches
     for optimizations. Just make the fast path the only path.
   - Prefer algorithmic improvements (better data structures, caching,
     avoiding redundant work) over micro-optimizations.
   - If an optimization makes the code significantly harder to read or
     maintain, document it clearly with a comment explaining the tradeoff.
   - Respect the architecture: solver owns type computation, checker is
     thin orchestration. Do NOT move logic across boundaries for speed.

7) Write a unit test:
   - For type-check fixes: test the specific Rust logic you changed
   - For perf fixes: test correctness of the optimized path
   - Run: cargo nextest run -p <crate> to verify

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 4 — VERIFY (mandatory, non-negotiable)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Before committing, you MUST pass ALL of these checks. If any check fails,
fix the regression before proceeding. Do NOT commit with regressions.

8) Run: cargo nextest run --no-fail-fast
   ✓ REQUIRED: Same or more tests passing compared to baseline from step 3a.
   ✗ BLOCKER: If any previously-passing test now fails, fix it before continuing.

9) Run: ./scripts/conformance.sh run 2>&1 | tail -20
   ✓ REQUIRED: Conformance pass rate must be >= baseline from step 3b.
   ✗ BLOCKER: If conformance % dropped, your change broke type-checking
     correctness. Revert or fix before continuing.

10) Re-run: ./scripts/bench-vs-tsgo.sh --quick
    ✓ REQUIRED: No new "tsz error" entries appeared.
    ✓ REQUIRED: The targeted ratio improved (or at minimum didn't regress).
    ✓ DESIRED: No other benchmark ratio regressed by more than 5%.

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 5 — COMMIT & REPORT
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

11) Only after ALL checks in Phase 4 pass:
    Create ONE small commit. Include in the commit message:
    - What was optimized/fixed and why
    - Before/after benchmark numbers for the targeted benchmark
    - Conformance: <before> → <after> (should be same or better)
    - Tests: <before> → <after> (should be same or better)
    Then push: git push origin main

12) Append any issues you investigated but punted on (too complex, needs
    architecture work, blocked by another issue, etc.) to
    docs/todos/perf.md — include function/module, current ratio, and a
    one-line reason why you skipped it.

13) git add docs/todos/perf.md (if changed) and amend or create a
    second commit, then push: git push origin main

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
REGRESSION SUMMARY TABLE (print before committing)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Print a table like this before every commit:

  Check              | Baseline     | Current      | Status
  -------------------|--------------|--------------|--------
  Unit tests         | XXX passed   | XXX passed   | ✓ / ✗
  Conformance        | XX.X%        | XX.X%        | ✓ / ✗
  Target benchmark   | X.XXx ratio  | X.XXx ratio  | ✓ / ✗
  Other benchmarks   | (no regress) | (no regress) | ✓ / ✗

ALL rows must show ✓ before you commit. If any row shows ✗, fix it first.

Target: tsz should be ≥2x faster than tsgo on every benchmark. We are not
done until every row in the benchmark table shows tsz winning with ratio ≥2.0.

Do not ask user questions. Keep going until this run is complete.
PROMPT
