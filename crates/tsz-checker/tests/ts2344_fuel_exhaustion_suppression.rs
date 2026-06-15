//! Regression test for #13609: false TS2344 from key-space constraint
//! evaluation (type-fest `ApplyDefaultOptions` / `RequiredKeysOf` /
//! `OptionalKeysOf`) under per-file evaluation-fuel exhaustion.
//!
//! Root cause: a cold per-file checker re-evaluates the deep key-space helper
//! chain from scratch and exhausts the per-file `MAX_EVALUATION_FUEL` budget.
//! The bail returns `TypeId::ERROR`, which the surrounding `Omit`/`Record`/
//! `Simplify` then absorb into a structurally-degraded — but error-free —
//! constraint bound, so the existing `contains_error_type` guard no longer
//! catches it and a false "does not satisfy" (TS2344) is reported. `tsc`
//! surfaces excessively-deep instantiation as TS2589 and never derives a false
//! TS2344 from it, so the TS2344 emitter must suppress once the per-file fuel
//! budget is exhausted. This also explains the schedule-dependence reported in
//! the issue: whichever file warms the shared eval caches first stays under the
//! budget and is exempt, while every cold sibling exhausts it.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::diagnostic_code_messages;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeId;
use tsz_solver::construction::TypeInterner;

/// Larger than `MAX_EVALUATION_FUEL` (2_000_000) so the per-file budget reads
/// as exhausted without depending on the exact crate-private constant.
const FUEL_OVER_BUDGET: u32 = 3_000_000;

/// Control: with fuel available, an unsatisfied constraint still reports
/// TS2344. This guards against the suppression below over-firing (e.g. if the
/// fuel flag were stuck on).
#[test]
fn ts2344_emitted_when_fuel_available() {
    let mut parser = ParserState::new("test.ts".to_string(), "type T = string;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    types.reset_evaluation_fuel();
    assert!(!types.is_evaluation_fuel_exhausted());
    // `string` does not satisfy a `number` constraint.
    checker.error_type_constraint_not_satisfied(TypeId::STRING, TypeId::NUMBER, root);

    let ts2344: Vec<_> = diagnostic_code_messages(checker.ctx.diagnostics)
        .into_iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert_eq!(
        ts2344.len(),
        1,
        "control: TS2344 should be emitted when fuel is available, got: {ts2344:?}"
    );
}

/// Regression: once the per-file evaluation-fuel budget is exhausted, the
/// constraint result is unreliable (the bound may have been truncated to a
/// degraded form), so no TS2344 must be emitted.
#[test]
fn ts2344_suppressed_when_fuel_exhausted() {
    let mut parser = ParserState::new("test.ts".to_string(), "type T = string;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    types.reset_evaluation_fuel();
    types.consume_evaluation_fuel(FUEL_OVER_BUDGET);
    assert!(types.is_evaluation_fuel_exhausted());
    // Same unsatisfied (`string` vs `number`) constraint as the control.
    checker.error_type_constraint_not_satisfied(TypeId::STRING, TypeId::NUMBER, root);

    let ts2344: Vec<_> = diagnostic_code_messages(checker.ctx.diagnostics)
        .into_iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344.is_empty(),
        "TS2344 must be suppressed under evaluation-fuel exhaustion (#13609), got: {ts2344:?}"
    );
}
