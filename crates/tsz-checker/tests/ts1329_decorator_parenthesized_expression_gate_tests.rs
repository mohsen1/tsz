//! `decorator_has_zero_arg_factory_shape` declines to substitute the TS1329
//! "did you mean to call it first" hint when the decorator's own expression
//! is a `ParenthesizedExpression` (`@(() => {})`).
//!
//! Oracle-verified against pinned `typescript@7.0.2`: `isPotentiallyUncalledDecorator`
//! only fires for a bare identifier, property-access chain, or call
//! expression. A parenthesized decorator expression keeps the generic
//! TS1238/1239/1240/1241 arity elaboration instead, uniformly across class
//! and member decorators and both `experimentalDecorators` (legacy) and ES
//! (TC39 stage-3) decorator mode.
//!
//! Regression coverage for #17121, found while merging #17118 on top of
//! #17120 (the class-decorator PR that first wired this shared helper into
//! `class_decorators.rs` and exposed the pre-existing member-decorator gap
//! on that new path too).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_source, check_source_codes, check_source_codes_experimental_decorators,
};

fn legacy_codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            experimental_decorators: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

// ============================ class decorators ============================

#[test]
fn es_class_decorator_parenthesized_zero_arg_emits_ts1238_not_ts1329() {
    let codes = check_source_codes("@(() => {})\nclass C {}\n").to_vec();
    assert!(
        codes.contains(&1238) && !codes.contains(&1329),
        "expected TS1238 (not TS1329) for a parenthesized zero-arg ES class decorator; got: {codes:?}"
    );
}

#[test]
fn legacy_class_decorator_parenthesized_zero_arg_emits_ts1238_not_ts1329() {
    let codes = legacy_codes("function d() {}\n@(() => {}) class C {}\n");
    assert!(
        codes.contains(&1238) && !codes.contains(&1329),
        "expected TS1238 (not TS1329) for a parenthesized zero-arg legacy class decorator; got: {codes:?}"
    );
}

// =========================== member decorators =============================

#[test]
fn es_method_decorator_parenthesized_zero_arg_emits_ts1241_not_ts1329() {
    let codes = check_source_codes("class C {\n  @(() => {})\n  method() {}\n}\n").to_vec();
    assert!(
        codes.contains(&1241) && !codes.contains(&1329),
        "expected TS1241 (not TS1329) for a parenthesized zero-arg ES method decorator; got: {codes:?}"
    );
}

#[test]
fn legacy_method_decorator_parenthesized_zero_arg_emits_ts1241_not_ts1329() {
    let codes =
        check_source_codes_experimental_decorators("class C {\n  @(() => {})\n  method() {}\n}\n")
            .to_vec();
    assert!(
        codes.contains(&1241) && !codes.contains(&1329),
        "expected TS1241 (not TS1329) for a parenthesized zero-arg legacy method decorator; got: {codes:?}"
    );
}

// ================================ controls ==================================

#[test]
fn property_access_zero_arg_class_decorator_still_emits_ts1329() {
    // Negative control: a property-access chain (not parenthesized) is
    // exactly the shape `isPotentiallyUncalledDecorator` covers — the gate
    // must not suppress the existing TS1329 behavior for it.
    let codes = check_source_codes("const obj = { d() {} };\n@obj.d class C {}\n").to_vec();
    assert!(
        codes.contains(&1329) && !codes.contains(&1238),
        "expected TS1329 (not TS1238) for a non-parenthesized property-access zero-arg decorator; got: {codes:?}"
    );
}

#[test]
fn parenthesized_but_callable_class_decorator_stays_clean() {
    // Adjacent case: parenthesizing a decorator whose call signature already
    // matches the runtime shape must not itself introduce a diagnostic.
    let codes =
        check_source_codes("@((value: unknown, context: unknown) => {})\nclass C {}\n").to_vec();
    assert!(
        !codes.contains(&1238) && !codes.contains(&1329),
        "parenthesized but call-compatible ES class decorator must stay clean; got: {codes:?}"
    );
}
