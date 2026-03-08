#!/usr/bin/env bash
# timeout: 10800
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cat <<PROMPT
You are working in $REPO_ROOT.
Goal: make tsz ≥2x faster than tsgo on EVERY benchmark shown at https://tsz.dev/benchmarks/
WITHOUT breaking tests, conformance, or code maintainability.

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
KNOWN REGRESSIONS (from tsz.dev/benchmarks)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

These are the benchmarks where tsz is LOSING or below the 2x target.
Attack them in this priority order:

  TSGO WINS (tsz is slower — top priority):
  - ts-toolbelt Object/Invert.ts  — TSGO 2.0x faster (tsz 562ms vs tsgo 287ms)
  - ts-essentials deep readonly.ts — TSGO 1.1x faster (tsz 324ms vs tsgo 290ms)

  BELOW 2x TARGET (tsz wins but not enough):
  - ts-toolbelt Any/Compute.ts — tsz only 1.1x faster (249ms vs 284ms)
  - ts-essentials paths.ts     — tsz only 1.6x faster (174ms vs 287ms)

All of these are external library benchmarks involving deeply recursive/mapped
types and lib type-environment lowering. The root causes are architectural:
  1. Merged lib-interface lowering is re-computed per lookup (no cross-arena cache)
  2. Recursive mapped-type evaluation has no memoization across segments
  3. Optional-chain property resolution does redundant work

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
KNOWN DEAD ENDS (do NOT re-investigate these)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Read docs/todos/perf.md for the full list. Key traps to avoid:

  DO NOT chase conformance timeouts under --workers 16:
  - correlatedUnions.ts, resolvingClassDeclarationWhenInBaseTypeResolution.ts,
    nonInferrableTypePropagation1.ts — these only time out under full worker
    contention. Single-test runs pass in ~6-8s. The problem is contention-
    sensitive, NOT an algorithmic blowup. 40+ perf.md entries were wasted on this.

  DO NOT try these micro-optimizations (already exhausted):
  - RefCell -> Cell conversions (already done)
  - Heritage symbol memoization (already done)
  - Class constructor/instance caches (already done)
  - Flow step budget tuning (already done, fragile)
  - Optional-chain property-access fast paths (already done, sub-2x remains)
  - env_eval_cache + widen_type memoization (already done, big win already captured)

  The remaining gains require ARCHITECTURAL changes, not micro-optimizations:
  - Cross-arena lib-lowering cache (crates/tsz-lowering/src/lower.rs)
  - Persistent per-process lib type data (avoid rebuilding TypeEnvironment per file)
  - Solver-level memoization of repeated mapped-type chain instantiation

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 1 — QUICK BASELINE (5 minutes max)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

1) git pull origin main
2) Read CLAUDE.md and docs/todos/perf.md
3) Verify pre-commit hooks: ls -la .git/hooks/pre-commit
   If missing: ./scripts/setup.sh
   NEVER use --no-verify on commit.
4) Run ONLY the targeted benchmarks (skip full conformance — it takes minutes):

   ./scripts/bench-vs-tsgo.sh --quick --filter 'Object/Invert|deep readonly|Any/Compute|paths\\.ts'

   Record the ratios. These are your targets.
   Also run unit tests for the crates you'll touch:
   cargo nextest run -p tsz-checker -p tsz-solver -p tsz-lowering --no-fail-fast 2>&1 | tail -5

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 2 — PROFILE (not guess)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

5) Pick the worst-ratio benchmark from Phase 1. Profile it:

   # Build the dist binary with debug info for profiling:
   CARGO_TARGET_DIR=.target-bench cargo build --profile dist -p tsz-cli
   # You may need: RUSTFLAGS="-C force-frame-pointers=yes"

   # Profile with samply (preferred — gives interactive flamegraph):
   samply record .target-bench/dist/tsz --noEmit <benchmark-file>

   # Alternative: cargo-flamegraph
   cargo flamegraph --profile dist -p tsz-cli -- --noEmit <benchmark-file>

   The external benchmark files are at:
   - .target-bench/external/ts-toolbelt/sources/Object/Invert.ts
   - .target-bench/external/ts-toolbelt/sources/Any/Compute.ts
   - .target-bench/external/ts-essentials/lib/deep-readonly.ts
   - .target-bench/external/ts-essentials/lib/paths.ts
   (Run bench-vs-tsgo.sh once first to clone these repos)

6) In the profile, look for:
   - Functions consuming >10% of total time
   - Repeated calls to the same function with the same arguments (cache miss)
   - Deep recursion stacks (>50 frames of the same function)
   - Allocation hotspots (many small Vec/HashMap allocations in tight loops)

   Write down the top 3 hottest call stacks before proceeding.

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 3 — IMPLEMENT (one optimization at a time)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

7) Based on the profile, implement ONE focused optimization:

   LIKELY ROOT CAUSES for the known regressions:

   a) Lib-lowering cache miss (paths.ts, deep readonly):
      - TypeLowering::lower_merged_interface_declarations is called repeatedly
        for the same lib interfaces (Array, ReadonlyArray, etc.)
      - FIX: add a cache keyed by (declaration NodeIndex, lib file) that
        persists across multiple type-environment lookups in the same check run
      - Location: crates/tsz-lowering/src/lower.rs

   b) Recursive mapped-type re-evaluation (Object/Invert.ts):
      - Mapped types like Invert<T> expand T's properties, and each property
        evaluation may re-evaluate the same mapped type with the same key
      - FIX: add solver-level memoization for mapped-type member evaluation
        keyed by (mapped_type_id, property_name)
      - Location: crates/tsz-solver/src/operations/ or evaluation modules

   c) Type-environment rebuild per symbol (Any/Compute.ts):
      - build_type_environment does upfront symbol/type population
      - FIX: share pre-computed lib type data across evaluations
      - Location: crates/tsz-checker/src/state/state_type_environment.rs

   MAINTAINABILITY RULES:
   - One optimization per commit. Keep changes minimal.
   - No complex unsafe code unless gain >20%.
   - No code duplication for speed.
   - Prefer algorithmic improvements over micro-optimizations.
   - Respect architecture: solver owns type computation, checker is thin.
   - No feature flags or conditional compilation.

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 4 — VERIFY (mandatory)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

8) Re-run the targeted benchmarks:
   ./scripts/bench-vs-tsgo.sh --quick --filter 'Object/Invert|deep readonly|Any/Compute|paths\\.ts'

   ✓ REQUIRED: Target benchmark ratio improved
   ✓ REQUIRED: No other benchmark regressed >10%

9) Run unit tests for affected crates:
   cargo nextest run -p <crates-you-changed> --no-fail-fast

   ✓ REQUIRED: No test regressions

10) Run a broader benchmark check (catches cross-benchmark regressions):
    ./scripts/bench-vs-tsgo.sh --quick

    ✓ REQUIRED: No new "tsz error" entries
    ✓ DESIRED: All benchmarks still ≥2x where they were before

11) If you changed solver/checker logic, also verify conformance:
    ./scripts/conformance.sh run 2>&1 | tail -20

    ✓ REQUIRED: Pass rate same or better

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 5 — COMMIT & ITERATE
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

12) Only after Phase 4 passes:
    NEVER use --no-verify. Let pre-commit hook run.
    Commit with before/after numbers in the message.
    Then push: git push origin main

13) If time remains, go back to Phase 2 with the next worst benchmark.
    Iterate: profile → optimize → verify → commit.

14) Before finishing, update docs/todos/perf.md with:
    - What you investigated and the outcome
    - Any new dead ends discovered (with module/function/ratio)
    - What the next session should try

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
REGRESSION TABLE (print before every commit)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Benchmark               | Before    | After     | Status
  ------------------------|-----------|-----------|--------
  Target benchmark        | X.XXx     | X.XXx     | improved?
  Unit tests              | XXX pass  | XXX pass  | same/better?
  Other benchmarks        | no regress| no regress| ok?

ALL rows must be green before committing.

Do not ask user questions. Keep going until this run is complete.
PROMPT
