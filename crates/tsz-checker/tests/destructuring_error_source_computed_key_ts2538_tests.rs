//! Locks #17529: a computed-key object-binding destructure over an
//! **error-typed** source (an unresolved reference) must not emit a spurious
//! TS2538 ("Type '…' cannot be used as an index type") on top of the
//! source's own TS2304/TS2464. tsc treats an error-typed source like `any`
//! and skips `isValidIndexType`/symbol-index-signature validation entirely;
//! the fix in `crates/tsz-checker/src/state/variable_checking/destructuring.rs`
//! gates both the invalid-index-type check and the symbol-index-signature
//! check on `parent_type != TypeId::ERROR`, mirroring the sibling
//! matching-index-signature check that already excluded ERROR.
//!
//! Controls confirm the gate is scoped to an error *source*, not an error
//! *key*: an unresolved key over a concrete source still reports TS2538 (the
//! key is remapped ERROR -> ANY specifically so it keeps failing), and
//! `unknown` sources are untouched (tsc keeps TS2538 there).

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

/// `test_utils::check_source` leaves `CheckerContext::report_unresolved_imports`
/// at its `CheckerState::new` default of `false`, under which a value-position
/// unresolved identifier silently resolves to `any` instead of `error` (see
/// `resolve_truly_unknown_identifier`). Every production entry point sets the
/// flag `true`, so these tests build their own checker with it set — the
/// established idiom in `tests/name_resolution_boundary_tests.rs` and
/// `src/tests/position_invalid_default_export_expression_tests.rs`.
fn check(source: &str) -> Vec<Diagnostic> {
    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    let mut codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

#[test]
fn error_source_and_error_key_no_ts2538() {
    let diagnostics = check(
        r#"
const { [k]: v } = o;
"#,
    );
    assert!(
        !codes(&diagnostics).contains(&2538),
        "error-typed source (`o` unresolved) must suppress TS2538 for the \
         computed key, even when the key itself (`k`) is also unresolved. \
         Got: {diagnostics:?}"
    );
    assert!(
        codes(&diagnostics).contains(&2304),
        "both `k` and `o` are unresolved and should still report TS2304. \
         Got: {diagnostics:?}"
    );
}

#[test]
fn error_source_with_object_key_no_ts2538() {
    let diagnostics = check(
        r#"
const bag = {};
const { [bag]: v } = errsrc;
"#,
    );
    assert!(
        !codes(&diagnostics).contains(&2538),
        "an error-typed source must suppress TS2538 even when the computed \
         key resolves to a genuinely invalid index type (`{{}}`, reported \
         separately as TS2464). Got: {diagnostics:?}"
    );
    assert!(
        codes(&diagnostics).contains(&2304),
        "`errsrc` is unresolved and should still report TS2304. \
         Got: {diagnostics:?}"
    );
}

#[test]
fn error_source_with_unique_symbol_key_no_ts2538() {
    let diagnostics = check(
        r#"
declare const s: unique symbol;
const { [s]: v } = errsrc;
"#,
    );
    assert!(
        !codes(&diagnostics).contains(&2538),
        "an error-typed source must suppress the symbol-index-signature \
         TS2538 too (Block B), not just the invalid-index-type check \
         (Block A). Got: {diagnostics:?}"
    );
}

#[test]
fn error_key_over_concrete_source_still_reports_ts2538() {
    let diagnostics = check(
        r#"
declare const obj: { a: number };
const { [k]: v } = obj;
"#,
    );
    assert!(
        codes(&diagnostics).contains(&2538),
        "the gate must be scoped to an error *source*: an unresolved key \
         (`k`) over a concrete, non-error source must still report TS2538 \
         (the ERROR key type is intentionally remapped to ANY so it keeps \
         failing validity). Got: {diagnostics:?}"
    );
}

#[test]
fn unique_symbol_key_over_concrete_non_matching_source_still_reports_ts2538() {
    let diagnostics = check(
        r#"
declare const s: unique symbol;
declare const obj: { a: number };
const { [s]: v } = obj;
"#,
    );
    assert!(
        codes(&diagnostics).contains(&2538),
        "a concrete source with no matching symbol property must still \
         report TS2538 for a unique-symbol computed key. Got: {diagnostics:?}"
    );
}

#[test]
fn unknown_source_is_unaffected_by_the_error_source_gate() {
    let diagnostics = check(
        r#"
declare const u: unknown;
const { [k]: v } = u;
"#,
    );
    assert!(
        codes(&diagnostics).contains(&2538),
        "the fix gates on TypeId::ERROR specifically; an `unknown` source \
         must keep reporting TS2538, matching tsc. Got: {diagnostics:?}"
    );
}
