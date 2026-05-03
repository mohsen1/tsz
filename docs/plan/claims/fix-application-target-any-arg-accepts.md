# fix(solver): treat `any` target arg as universal sink in same-base Application assignability when variance is unmeasured

- **Date**: 2026-05-03
- **Branch**: `fix/application-target-any-arg-accepts`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance — generic Application assignability, recursive aliases

## Intent

`check_application_variance` bailed (`return None`) whenever every type
parameter's variance was `Variance::empty()` -- the case for self-referential
aliases like `FlatArray<Arr, Depth>` whose recursive body forces the
variance probe to bottom out. Falling through to structural comparison then
expanded the recursive bodies and spuriously rejected assignments where the
target's arg was `any`. Conformance test
`flatArrayNoExcessiveStackDepth.ts` produced a spurious TS2322 on
`x = y;` where `x: FlatArray<Arr, any>` and `y: FlatArray<Arr, D>`.

When variance is unmeasurable for every parameter AND the target has at
least one `any` arg AND any-propagation is allowed, treat the comparison as
True. `any` is the universal sink under any-propagation regardless of
variance, so the structural expansion adds nothing useful.

## Files Touched

- `crates/tsz-solver/src/relations/relation_queries.rs` (~10 LOC)
- `crates/tsz-checker/src/tests/application_target_any_arg_assignability_tests.rs` (new, ~35 LOC)
- `crates/tsz-checker/src/lib.rs` (+3 LOC: register the new test module)

## Verification

- `cargo nextest run -p tsz-checker -p tsz-solver --lib` -- 8800 / 8800 passing
- `./scripts/conformance/conformance.sh run --filter "flatArrayNoExcessiveStackDepth"` -- 1 / 1 passing
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` -- net +3 tests, no regressions
