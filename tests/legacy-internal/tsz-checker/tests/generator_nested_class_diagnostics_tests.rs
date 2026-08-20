//! Regression coverage: an **unannotated** generator declaration must not
//! swallow the diagnostics owed by a class nested in its body.
//!
//! `infer_generator_declaration_yield_type`
//! (`types/function_type/generator_declaration_yield.rs`) runs a *suppressed*
//! speculative pass over the generator body to recover its inferred yield
//! type — real body diagnostics are meant to come from the later, real
//! declaration check. That pass walks the whole body, so any class it reaches
//! is checked (with diagnostics rolled back) and, before the fix, left marked
//! in `checked_classes`. The real pass then treated the class as
//! "already checked" and skipped it, silently dropping every diagnostic its
//! members owe: TS2322 on a member/property body, TS1308/TS1166 on a computed
//! name, TS2507 on the heritage clause. Annotating the generator's return
//! type disabled the speculative pass and hid the bug; only the inferred-yield
//! path tripped it.
//!
//! The fix snapshots `checked_classes`/`checking_classes` across the
//! speculative walk and restores them past the rollback, so a class checked
//! speculatively is re-checked for real. Every expectation below is pinned
//! against `tsc@5.9 --noEmit --strict --target es2022 --module esnext` and
//! cross-checked against the compiled `tsz` CLI.
//!
//! Binder names are varied across cases (`Holder`/`Bag`/`Inner`/`Widget`/`Cell`,
//! `gen`/`g`/`stream`/`pump`/`feed`) so the coverage rides on the structural
//! generator/class nesting, not on any particular identifier.

use crate::test_utils::check_source_codes_with_parse_health;

/// The unit harness has no lib, so an unannotated generator's inferred
/// `Generator<...>` return type reports missing-global-type noise
/// (TS2318/TS2468/TS2304 on `Generator`/`Iterator*`) that the compiled CLI,
/// which has a lib, never produces. Strip those so assertions read only the
/// class-body diagnostics under test — which is exactly the family the bug
/// dropped.
fn class_body_codes(mut codes: Vec<u32>) -> Vec<u32> {
    codes.retain(|&c| !matches!(c, 2318 | 2468 | 2304 | 2705 | 7025 | 7057));
    codes.sort_unstable();
    codes
}

#[test]
fn unannotated_generator_nested_class_member_body_reports_ts2322() {
    let codes = class_body_codes(check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { m(): string { return 42; } } }
"#,
    ));
    assert_eq!(codes, vec![2322], "got {codes:?}");
}

#[test]
fn unannotated_generator_nested_class_property_reports_ts2322() {
    let codes = class_body_codes(check_source_codes_with_parse_health(
        r#"
function* g() { class Bag { p: string = 42; } }
"#,
    ));
    assert_eq!(codes, vec![2322], "got {codes:?}");
}

#[test]
fn unannotated_generator_nested_class_heritage_reports_ts2507() {
    let codes = class_body_codes(check_source_codes_with_parse_health(
        r#"
function* stream() { class Inner extends 5 {} }
"#,
    ));
    assert_eq!(codes, vec![2507], "got {codes:?}");
}

#[test]
fn unannotated_generator_nested_class_computed_name_await_reports_ts1308() {
    // `await` in a class computed name is evaluated in the enclosing scope,
    // which here is a *generator* (non-async) — so TS1308 is correct, and the
    // dropped-class bug had been hiding it.
    let codes = class_body_codes(check_source_codes_with_parse_health(
        r#"
declare const key: string;
function* gen() { class Holder { [await key]() {} } }
"#,
    ));
    assert_eq!(codes, vec![1308], "got {codes:?}");
}

#[test]
fn unannotated_generator_nested_class_computed_property_yield_reports_ts1166() {
    // `[yield 1]` in a class *property* declaration: the generator context
    // makes `yield 1` a legal yield expression (no TS1163), but a class
    // property computed name still requires a literal/unique-symbol type
    // (TS1166). The bug dropped both this and the type check.
    let codes = class_body_codes(check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { [yield 1] = 2; } }
"#,
    ));
    assert_eq!(codes, vec![1166], "got {codes:?}");
}

#[test]
fn async_generator_nested_class_member_body_reports_ts2322() {
    // Async generators take the identical inferred-yield speculative path.
    let codes = class_body_codes(check_source_codes_with_parse_health(
        r#"
async function* pump() { class Widget { m(): string { return 42; } } }
"#,
    ));
    assert_eq!(codes, vec![2322], "got {codes:?}");
}

#[test]
fn unannotated_generator_class_under_nested_function_reports_ts2322() {
    // The speculative walk reaches every depth, so the leak dropped classes
    // arbitrarily deep — here inside a plain function inside the generator.
    let codes = class_body_codes(check_source_codes_with_parse_health(
        r#"
function* gen() { function h() { class Holder { m(): string { return 42; } } } }
"#,
    ));
    assert_eq!(codes, vec![2322], "got {codes:?}");
}

#[test]
fn unannotated_generator_class_under_block_reports_ts2322() {
    let codes = class_body_codes(check_source_codes_with_parse_health(
        r#"
function* gen() { { class Holder { m(): string { return 42; } } } }
"#,
    ));
    assert_eq!(codes, vec![2322], "got {codes:?}");
}

#[test]
fn unannotated_generator_with_yield_still_reports_nested_class_ts2322() {
    // A real yield in the body exercises the yield-collection side of the
    // speculative pass while a nested class error is still owed.
    let codes = class_body_codes(check_source_codes_with_parse_health(
        r#"
function* feed() { yield 1; class Cell { m(): string { return 42; } } }
"#,
    ));
    assert_eq!(codes, vec![2322], "got {codes:?}");
}

#[test]
fn annotated_generator_nested_class_still_reports_ts2322() {
    // The annotated path never ran the speculative pass, so it was already
    // correct; pin it so a future refactor can't regress the twin.
    let codes = class_body_codes(check_source_codes_with_parse_health(
        r#"
function* gen(): Generator<number> { class Holder { m(): string { return 42; } } }
"#,
    ));
    assert_eq!(codes, vec![2322], "got {codes:?}");
}

#[test]
fn unannotated_generator_yield_grammar_in_nested_class_stays_clean() {
    // Control: a class-computed-name `yield` that *is* legal (generator
    // context) must stay clean — the fix must not manufacture a diagnostic.
    let codes = class_body_codes(check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { [yield 1]() {} } }
"#,
    ));
    assert!(codes.is_empty(), "got {codes:?}");
}
