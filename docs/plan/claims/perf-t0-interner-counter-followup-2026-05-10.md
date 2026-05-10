# perf(solver): T0 interner counter — gate once, fix invariant comment

- **Date**: 2026-05-10
- **Branch**: `perf/t0-interner-counter-followup-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (Performance) — follow-up to merged #4955

## Intent

Address two valid Copilot review comments left on #4955 (interner
intern_calls/hits/misses wiring) that were not addressed before merge.

1. **Single gate, single counter pointer.** `intern()` previously called
   `tsz_common::perf_counters::counters()` (one `OnceLock` deref) and
   `enabled_fast()` (a second `OnceLock` deref via the `inc()` helper)
   once per outcome — up to 4 derefs per `intern()` call in the worst
   case. This refactor caches the counter pointer in a local
   `Option<&'static PerfCounters>` once at function entry, gated by
   `enabled_fast()`. Each subsequent disposition does a single
   `if let Some(c) = pc { c.field.fetch_add(1, Relaxed); }`.
   `intern_slow` takes the cached pointer as a parameter so it does
   not re-deref or re-check the gate. Disabled mode now pays exactly
   one `OnceLock` deref per `intern()` call; enabled mode pays one
   deref + N atomic increments.

2. **Accurate invariant comment.** The previous comment claimed
   "calls minus the early `ERROR` returns above". That was inaccurate:
   the poisoned check returns *before* `intern_calls` increments, but
   the `intern_slow` circuit breakers (max-types limit, u32-overflow)
   return *after*. Replace with a precise statement of the current
   semantics:

   ```
   intern_calls = intern_hits + intern_misses + slow_path_errors
   ```

   `slow_path_errors` is observable as the residual; not separately
   bucketed today. Each `intern_slow` ERROR exit got an inline comment
   explaining why it doesn't credit hit or miss.

## Files Touched

- `crates/tsz-solver/src/intern/core/interner.rs` (~70 LOC restructure
  in `intern()` and `intern_slow()`).

## Verification

- End-to-end attribution mode on a small generic-heavy fixture:
  `calls=3209, hits=2481, misses=728`, and `calls - hits - misses = 0`,
  confirming the invariant.
- Timing mode (`TSZ_PERF_COUNTERS` unset): all three counters return 0,
  `enabled: false`. Disabled-mode gating preserved.
- `cargo build -p tsz-solver` clean.
- `cargo nextest run -p tsz-common -p tsz-solver --lib` — 6145/6145 pass.
- `cargo clippy -p tsz-solver -p tsz-common --all-targets -- -D warnings`
  clean.

## No conformance / behavior impact

Pure instrumentation refactor. Counter values produced are identical to
before (verified by running both versions on the same fixture). The
only observable effect is reduced `OnceLock`-deref overhead in
attribution mode and slightly less branchy code in disabled mode.
