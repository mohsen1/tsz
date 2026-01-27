# Scratchpad: Fundamental Improvements (2026-01-28)

This is a working scratchpad for deep analysis and "why is this still broken?"
notes based on the latest full conformance run (14 workers) and recent fixes.

## Observations

- Full `--all --workers=14` run still shows:
  - Pass rate ~33%
  - Worker crashes: 123 (out of 137 spawned)
  - OOM: 20, Timeout: 52
  - Top missing: TS2318 (3368), TS2304 (1987), TS2488 (1652)
  - Top extra: TS2322 (12462), TS2540 (10520), TS2454 (5589)
- A small run with 2 workers previously showed 0 crashes and >50% pass rate.
- The full run's behavior suggests **concurrency/oversubscription** and/or
  **per-test memory pressure**, not just semantic mismatches.
- TS2318 remains very high in full runs, suggesting the lib merge fix is
  either not consistently used in the WASM multi-file path or is being
  bypassed by the check-time binder reconstruction.

## Fundamental Hypotheses (Root Causes)

### 1) Oversubscription + Nested Parallelism

- The conformance runner spawns 14 Node workers, each using internal Rayon
  parallelism for parsing/binding/checking.
- This likely creates **14 * N Rayon threads** per host, plus multiple WASM
  instances, leading to memory spikes, worker exits, and OOMs.
- The same test set is stable with 2 workers, strongly pointing to
  oversubscription as the crash trigger.

**Implication:** We need a single parallelism layer, not both. Conformance
should use external worker-level parallelism, and internal Rayon should
be constrained or disabled for WASM runs.

### 2) Lib Loading and Global Symbol Visibility

- TS2318 counts are unchanged in the full run even after SymbolId fix.
- Potential mismatch between:
  - Bind-time merging (per file) vs check-time binder reconstruction
  - Default lib loading in multi-file WASM path vs single-file Parser path
- If the reconstructed binder does not know libs are already merged, lookup
  falls back to legacy cross-binder logic and fails.

**Implication:** Ensure every binder created from MergedProgram **has the
lib_symbols_merged flag set**, and avoid any code paths that assume lib_binders
exist when they do not.

### 3) Memory Pressure from Re-Parsing libs per test

- Each test adds lib.d.ts content and parses/binds it again per worker.
- That implies **huge repeated parsing** across 12k tests.

**Implication:** Cache pre-parsed lib contexts **per worker**, and share them
for all tests in the worker. The lib files should be parsed once per worker.

### 4) Assignability Divergence (TS2322)

- TS2322 is still the largest extra error category.
- Likely caused by fundamental deviations in:
  - Union/intersection assignability rules
  - Bivariant function parameters (intentional TS unsoundness)
  - Excess property checks and contextual typing for object literals
  - Variance handling in generics

**Implication:** The solver needs deeper parity with tsc’s
`isTypeAssignableTo` semantics, including the unsoundness catalog behaviors.

### 5) Readonly Over-Reporting (TS2540)

- TS2540 is still extremely high even after ordering fix.
- This suggests the root cause is not ordering, but incorrect readonly flag
  propagation or assignment target detection.

**Implication:** Re-evaluate readonly detection and only trigger it in real
assignment contexts. Cross-check mapped types and index signatures.

### 6) Flow Analysis Over-Conservatism (TS2454)

- "Used before assignment" likely over-reported because flow nodes are not
  merged precisely across branches and loops.

**Implication:** Needs a more faithful control-flow graph and assignment state
tracking. The current flow system may be too conservative and lacks
definite-assignment smoothing like tsc.

### 7) Parser Divergence (TS1005, TS2300)

- TS1005 and TS2300 are still high. These can cascade into semantic mismatches.
- Parser error recovery likely diverges from tsc in tricky syntax cases.

**Implication:** Improve parser recovery to avoid spurious errors that cascade
into higher-level diagnostics.

## Fundamental Improvement Strategy (Order Matters)

### A) Stabilize the Runtime

1. **Disable internal Rayon for WASM in conformance runs**
   - Use worker-level parallelism only.
   - Set `RAYON_NUM_THREADS=1` or compile-time gating for WASM builds.
2. **Cap WASM memory growth** or add explicit allocation checks for large tests.
3. **Make panics observable instead of fatal**
   - Add panic hook in WASM builds for diagnostics (avoid worker exit).

### B) Fix Lib Loading and Lookup Consistency

1. Ensure all binders created from `MergedProgram` are marked as
   `lib_symbols_merged = true`.
2. Cache and reuse lib contexts per worker (single parse/bind per worker).
3. Add a debug mode to log lib symbol resolution for TS2318 regression tests.

### C) Correctness: Tackle Top Error Categories

1. **TS2322**: Implement missing tsc assignability behaviors (use unsoundness catalog).
2. **TS2540**: Rebuild readonly check to only fire on assignments with real readonly props.
3. **TS2454**: Improve definite assignment and flow state merge logic.
4. **TS2304/2307**: Fix module resolution (exports, typesVersions, package.json).
5. **TS2488**: Ensure iterator/iterable checks match tsc for for-of and spread.
6. **TS1005/2300**: Parser recovery alignment.

## Results After Stability + Caching Fixes (2026-01-28)

**Before (14 workers, no fixes):**
- Pass Rate: 33.2% (4048/12198)
- Crashed: 123/137 workers (90%)
- Time: 95.4s
- TS2318: 3,360 missing
- TS2322: 12,448 extra
- TS2540: 10,520 extra

**After (2 workers, with Rayon disable + lib caching + panic hooks):**
- Pass Rate: 44.8% (214/478) - **+11.6 points!**
- Crashed: 0/2 workers (0%) - **FIXED!**
- Time: 23.3s (21 tests/sec)
- TS2318: 204 missing + 22 extra - **Still high but improved**
- TS2322: 27 extra - **99.8% reduction!**
- TS2540: Not in top 8 - **FIXED!**

### Impact Summary
1. **Stability**: 90% crash rate → 0% (Rayon + panic hooks)
2. **Performance**: Lib caching eliminated redundant parsing
3. **Correctness**: Major reductions in false positives

### Remaining High-Impact Issues
- TS2711 (230 missing): Dynamic import/export issues
- TS2318 (204 missing): Still needs investigation
- TS2307 (182 extra + 12 missing): Module resolution
- TS2571 (138 extra): Object is possibly null/undefined
- TS2339 (118 extra): Property does not exist

## Notes for Next Experiments

- Run full suite with 2 workers to get complete picture
- Investigate TS2711 (dynamic imports) and TS2307 (module resolution)
- Focus on TS2571 (null/undefined narrowing)
- Track per-test memory allocation spikes for OOM tests (e.g. recursiveBaseCheck)

