//! Top-level destructuring patterns of closure parameters (arrow / function
//! expressions) must validate against their externally-derived contextual
//! types (callback-shape contextual types and IIFE call arguments).
//!
//! Function declarations get this from `check_parameter_binding_pattern_defaults`
//! via the statement-callback bridge, but that path is gated to declarations.
//! Without the closure-side path, these IIFE/callback shapes silently pass:
//!
//!   (([]) => 0)({});           // tsc: TS2488 at the `[]` pattern
//!   (({}) => 0)(undefined);    // tsc: TS2532 at the `{}` pattern
//!   [1, 2, 3].map(([a, b]) => …);   // tsc: TS2488 at the `[a, b]` pattern
//!
//! These tests pin the fix that wires `check_destructuring_iterability` and
//! the empty-pattern nullish probe into the closure parameter pipeline.

use tsz_common::options::checker::CheckerOptions;

fn diags_strict(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
}

fn codes(diags: &[crate::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

#[test]
fn iife_arrow_array_pattern_against_object_emits_ts2488() {
    let diags = diags_strict("(([]) => 0)({});\n");
    assert!(
        codes(&diags).contains(&2488),
        "Expected TS2488 for `(([]) => 0)({{}})`; got: {diags:?}"
    );
}

#[test]
fn iife_arrow_empty_object_pattern_against_undefined_emits_ts2532() {
    let diags = diags_strict("(({}) => 0)(undefined);\n");
    assert!(
        codes(&diags).contains(&2532),
        "Expected TS2532 for `(({{}}) => 0)(undefined)`; got: {diags:?}"
    );
}

#[test]
fn callback_arrow_array_pattern_against_number_emits_ts2488() {
    // Map's callback receives the array element type (`number`).
    // Destructuring a `number` as `[a, b]` is not iterable.
    let diags = diags_strict(
        r#"
declare function callMap<T, U>(arr: T[], fn: (item: T) => U): U[];
callMap([1, 2, 3], ([a, b]) => 0);
"#,
    );
    assert!(
        codes(&diags).contains(&2488),
        "Expected TS2488 for callback array destructure of `number`; got: {diags:?}"
    );
}

#[test]
fn callback_arrow_empty_object_pattern_against_undefined_emits_ts2532() {
    let diags = diags_strict(
        r#"
declare function call(cb: (s: string | undefined) => void): void;
call(({}) => 0);
"#,
    );
    assert!(
        codes(&diags).contains(&2532),
        "Expected TS2532 for callback empty object destructure of possibly-undefined; got: {diags:?}"
    );
}

#[test]
fn iife_arrow_object_pattern_with_matching_property_does_not_regress_ts2488() {
    // Destructuring `{a: 1}` as `({a})` is fine — no iterability or property errors.
    let diags = diags_strict("(({a}) => a)({a: 1});\n");
    let cs = codes(&diags);
    assert!(
        !cs.contains(&2488),
        "Should not emit TS2488 for object pattern destructuring an object; got: {diags:?}"
    );
    assert!(
        !cs.contains(&2532),
        "Should not emit TS2532 for object destructure of a non-nullish object literal; got: {diags:?}"
    );
}

#[test]
fn iife_arrow_with_default_strips_undefined_for_iterability_check() {
    // The default initializer guards against undefined at runtime, so tsc does
    // NOT emit TS2532 here even though the IIFE arg is `undefined`. The
    // iterability check sees the default-stripped type.
    let diags = diags_strict(
        r#"
(({a = 1} = {a: 2}) => a)(undefined);
"#,
    );
    let cs = codes(&diags);
    assert!(
        !cs.contains(&2532),
        "Should not emit TS2532 when initializer covers undefined IIFE arg; got: {diags:?}"
    );
}

#[test]
fn function_declaration_array_pattern_no_double_ts2488() {
    // The pre-existing `check_parameter_binding_pattern_defaults` path emits
    // TS2488 for function declarations. The closure-only top-level helper
    // must NOT additionally fire here, or the diagnostic count would double.
    let diags = diags_strict(
        r#"
function bar([]: {}) { return 0; }
"#,
    );
    let ts2488_count = diags.iter().filter(|d| d.code == 2488).count();
    assert_eq!(
        ts2488_count, 1,
        "Expected exactly one TS2488 for the `[]: {{}}` parameter; got: {diags:?}"
    );
}
