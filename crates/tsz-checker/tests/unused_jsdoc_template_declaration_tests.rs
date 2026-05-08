//! Regression coverage for #3506 repro 1: `@template T` is a *declaration*
//! of a scoped type parameter, not a *use* of the surrounding scope's
//! `T` symbol. Treating it as a use silently suppressed TS6133 for
//! unrelated locals/imports/value declarations that happened to share
//! the same name.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn check_with_no_unused_locals(source: &str) -> Vec<u32> {
    let opts = CheckerOptions {
        no_unused_locals: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.ts", opts)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn jsdoc_template_does_not_mark_unrelated_value_local_as_used() {
    // From #3506 repro 1: a value `const T` is unused even though a
    // separate function later declares `@template T`. tsc emits
    // TS6133 for both `T` and `f`; tsz used to drop the `T`
    // diagnostic because the JSDoc-reference probe matched the
    // `@template T` substring.
    let source = r#"
export {};
const T = 1;
/** @template T */
function f() {}
"#;
    let codes = check_with_no_unused_locals(source);
    let unused_count = codes.iter().filter(|&&c| c == 6133).count();
    assert!(
        unused_count >= 2,
        "Expected at least two TS6133 (one for `T`, one for `f`); `@template T` must \
         not mark the unrelated value local as used. Got codes: {codes:?}"
    );
}

#[test]
fn jsdoc_template_with_longer_param_name_does_not_suppress_other_locals() {
    // The old probe used a raw substring scan: `@template T` would
    // also match `@template TypeParam` as a hit for a local named
    // `T`. Lock the new boundary-aware behaviour by checking that a
    // `@template TypeParam` on one function still leaves an unrelated
    // `const T` flagged as unused.
    let source = r#"
export {};
const T = 1;
/** @template TypeParam */
function f() {}
"#;
    let codes = check_with_no_unused_locals(source);
    let unused_count = codes.iter().filter(|&&c| c == 6133).count();
    assert!(
        unused_count >= 2,
        "Expected at least two TS6133; `@template TypeParam` must not mark the \
         shorter-named local `T` as used. Got codes: {codes:?}"
    );
}
