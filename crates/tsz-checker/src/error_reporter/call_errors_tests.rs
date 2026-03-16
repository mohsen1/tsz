use crate::context::CheckerOptions;
use crate::test_utils::{check_source, check_source_diagnostics};

/// Alias: default options already have `strict_null_checks: true`.
fn check_source_with_strict_null(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_source_diagnostics(source)
}

fn check_source_without_strict_null(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict_null_checks: false,
            ..CheckerOptions::default()
        },
    )
}

#[test]
fn emits_ts2721_for_calling_null() {
    let diagnostics = check_source_with_strict_null("null();");
    assert!(
        diagnostics.iter().any(|d| d.code == 2721),
        "Expected TS2721 for `null()`, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn emits_ts2722_for_calling_undefined() {
    let diagnostics = check_source_with_strict_null("undefined();");
    assert!(
        diagnostics.iter().any(|d| d.code == 2722),
        "Expected TS2722 for `undefined()`, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn emits_ts2723_for_calling_null_or_undefined() {
    let diagnostics = check_source_with_strict_null("let f: null | undefined;\nf();");
    assert!(
        diagnostics.iter().any(|d| d.code == 2723),
        "Expected TS2723 for calling `null | undefined`, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn emits_ts2349_without_strict_null_checks() {
    // Without strictNullChecks, null/undefined are in every type's domain,
    // so we should get TS2349 (not callable) instead of TS2721/2722/2723.
    let diagnostics = check_source_without_strict_null("null();");
    let has_2349 = diagnostics.iter().any(|d| d.code == 2349);
    let has_272x = diagnostics.iter().any(|d| (2721..=2723).contains(&d.code));
    assert!(
        has_2349 && !has_272x,
        "Expected TS2349 (not TS272x) without strictNullChecks, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn emits_ts6234_not_ts2721_for_generic_getter_returning_null() {
    // When a generic class has a getter that returns null, calling it should
    // emit TS6234 (not callable because it's a get accessor), not TS2721
    // (cannot invoke object which is possibly null). The getter accessor
    // diagnostic takes priority over the nullish diagnostic.
    let diagnostics = check_source_with_strict_null(
        r#"
class C<T, U> {
    x: T;
    get y() {
        return null;
    }
    set y(v: U) { }
    fn() { return this; }
    constructor(public a: T, private b: U) { }
}
var c = new C(1, '');
var r6 = c.y();
"#,
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&6234),
        "Expected TS6234 for calling getter `c.y()`, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2721),
        "Should NOT emit TS2721 for calling getter on generic class, got: {codes:?}"
    );
}

#[test]
fn emits_ts6234_for_non_generic_getter_call() {
    // Non-generic class: calling a getter should emit TS6234
    let diagnostics = check_source_with_strict_null(
        r#"
class C {
    x: string;
    get y() {
        return 1;
    }
    set y(v) { }
    constructor(public a: number, private b: number) { }
}
var c = new C(1, 2);
var r6 = c.y();
"#,
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&6234),
        "Expected TS6234 for calling getter `c.y()`, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2721) && !codes.contains(&2349),
        "Should NOT emit TS2721 or TS2349 for getter call, got: {codes:?}"
    );
}

#[test]
fn emits_ts2722_for_optional_method_call() {
    // When an optional method is called without optional chaining,
    // its type includes undefined, so TS2722 should be emitted.
    let diagnostics = check_source_with_strict_null(
        r#"
interface Foo {
    optionalMethod?(x: number): string;
}
declare let foo: Foo;
foo.optionalMethod(1);
"#,
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2722),
        "Expected TS2722 for calling optional method without ?., got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
