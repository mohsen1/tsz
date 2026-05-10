# perf(checker): T2.2 typeparam-memo follow-up — miss counter, test fixture, ROADMAP cleanup

- **Date**: 2026-05-10
- **Branch**: `perf/t2.2-typeparam-memo-followup-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (Performance) — Tier 2.2 follow-up to #4954

## Intent

Address three Copilot review comments left on #4954 (T2.2 typed
cross-file type-parameter memo) that were valid but not addressed
before merge. All three change shape, not behavior — the cache hit/miss
behavior on success paths is unchanged; only attribution accuracy and
test coverage shift.

## Changes

1. **Miss counter accuracy** (`crates/tsz-checker/src/state/type_environment/core.rs`,
   both `TypeEnvironmentCore` slow-path sites): increment
   `cross_file_type_params_cache_misses` on slow-path entry, not after
   successful extraction. Counting only on `Some(_)` undercounts misses
   when the slow path runs but extraction returns `None` (e.g.
   interface-name mismatch). The slow path *did* construct a child
   checker either way, which is exactly what the counter is supposed to
   attribute.

2. **Test fixture exercises real generics**
   (`crates/tsz-checker/tests/cross_file_type_params_cache_tests.rs`):
   the existing `no_constraint_no_default_generic_takes_arena_only_fast_path`
   test declared `interface Inner` and `interface Outer` with no type
   parameters at all, so the cache stayed empty for the wrong reason
   (no extraction happened). Add `Inner<T>` and `Outer<U>` so the
   assertion that "the arena-only fast path runs and the cache stays
   empty" is actually testable.

3. **ROADMAP claim cleanup** (`docs/plan/ROADMAP.md`): remove the
   inline `Active Implementation Claims` entry for the merged T2.2 PR.
   Per `docs/plan/claims/README.md` the per-PR claim file is the
   canonical home; the inline entry duplicated information already
   present in `docs/plan/claims/perf-t2.2-typeparam-memo.md` and
   violated the merge-conflict-reduction rule.

## Files Touched

- `crates/tsz-checker/src/state/type_environment/core.rs` (~30 LOC moved/edited)
- `crates/tsz-checker/tests/cross_file_type_params_cache_tests.rs` (~10 LOC)
- `docs/plan/ROADMAP.md` (-1 line)

## Verification

- `cargo nextest run -p tsz-checker -E 'test(cross_file_type_params_cache)'` — 3/3 pass.
- `cargo clippy -p tsz-checker --all-targets -- -D warnings` — clean.
- The miss-counter move is behavior-preserving for the cache itself;
  it only changes which counter bucket events land in (specifically,
  it adds the `None`-result path to misses, where it always belonged).

## No conformance impact

Pure attribution accuracy + test fixture quality + docs hygiene. No
checker/solver behavior change.
