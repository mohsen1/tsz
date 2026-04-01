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

#[test]
fn ts2345_argument_mismatch_anchors_argument_node() {
    let source = r#"
declare function takes(value: string): void;
takes(123);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    let arg_start = source.find("123").expect("expected argument") as u32;
    assert_eq!(
        diag.start, arg_start,
        "TS2345 should anchor at the argument"
    );
    assert_eq!(diag.length, 3, "TS2345 should cover only the argument span");
}

#[test]
fn ts2345_zero_argument_never_rest_call_uses_empty_tuple_display() {
    let source = r#"
declare let foo: (...args: never) => void;
foo();
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains("Argument of type '[]' is not assignable to parameter of type 'never'."),
        "Expected empty argument list display for zero-argument never-rest call, got: {diag:?}"
    );
}

#[test]
fn ts2345_object_literal_contextual_typing_ignores_object_prototype_members() {
    let source = r#"
interface I {
    value: string;
    toString: (t: string) => string;
}
declare function f2(args: I): void;
f2({ value: '' });
"#;

    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics when only Object.prototype members are missing, got: {diagnostics:?}"
    );
}

#[test]
fn ts2345_object_literal_contextual_typing_still_reports_real_missing_property() {
    let source = r#"
interface I {
    value: string;
    toString: (t: string) => string;
}
declare function f2(args: I): void;
f2({ toString: (s: string) => s });
"#;

    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2345),
        "expected TS2345 when a real required property is missing, got: {diagnostics:?}"
    );
}

#[test]
fn object_literal_call_argument_uses_shared_epc_rules_for_generic_intersections() {
    let source = r#"
declare function take<T>(value: { nested: T & { a: number } }): void;
take({ nested: { a: 1, extra: 2 } });
"#;

    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.is_empty(),
        "generic intersections should capture extra nested properties without TS2353/TS2345, got: {diagnostics:?}"
    );
}

#[test]
fn contextual_object_literal_assertion_does_not_emit_early_excess_property_errors() {
    let source = r#"
var foo = <{ id: number; }> { id: 4, name: "as" };
"#;

    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.is_empty(),
        "type assertions should not emit early object-literal TS2353 diagnostics, got: {diagnostics:?}"
    );
}

#[test]
fn ts2769_overload_related_information_keeps_overload_order() {
    let source = r#"
declare function fn(value: string): void;
declare function fn(value: number): void;
fn(true);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2769)
        .expect("expected TS2769");

    let arg_start = source.rfind("true").expect("expected argument") as u32;
    assert_eq!(
        diag.start, arg_start,
        "TS2769 should anchor at the argument for plain overload calls"
    );
    assert_eq!(
        diag.length, 4,
        "TS2769 should cover only the argument token"
    );
    assert!(
        diag.related_information.len() >= 2,
        "expected overload related info, got: {diag:?}"
    );
    assert!(
        diag.related_information[0]
            .message_text
            .contains("parameter of type 'string'"),
        "expected the first overload failure first, got: {diag:?}"
    );
    assert!(
        diag.related_information[1]
            .message_text
            .contains("parameter of type 'number'"),
        "expected the second overload failure second, got: {diag:?}"
    );
}

#[test]
fn ts2769_array_literal_overload_mismatch_anchors_nested_property() {
    let source = r#"
function foo(bar:{a:number;}[]):string;
function foo(bar:{a:boolean;}[]):number;
function foo(bar:{a:any;}[]):any{ return bar }
var x = foo([{a:'bar'}]);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2769)
        .expect("expected TS2769");

    let prop_start = source
        .rfind("a:'bar'")
        .expect("expected offending property") as u32;
    assert_eq!(
        diag.start, prop_start,
        "TS2769 should anchor at the offending nested property, got: {diag:?}"
    );
    assert_eq!(
        diag.length, 1,
        "TS2769 should cover only the property token"
    );
}

#[test]
fn ts2769_array_literal_missing_property_anchors_object_literal() {
    let source = r#"
function foo(bar:{a:number;}[]):string;
function foo(bar:{a:boolean;}[]):number;
function foo(bar:{a:any;}[]):any{ return bar }
var x = foo([{}]);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2769)
        .expect("expected TS2769");

    let object_start = source.rfind("{}").expect("expected object literal") as u32;
    assert_eq!(
        diag.start, object_start,
        "TS2769 should anchor at the object literal with the missing property, got: {diag:?}"
    );
    assert_eq!(
        diag.length, 2,
        "TS2769 should cover the empty object literal"
    );
}

#[test]
fn ts2345_single_arity_overload_mismatch_does_not_emit_ts2769() {
    let source = r#"
declare function fn(value: string): void;
declare function fn(value: number, extra: number): void;
fn(true);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2345),
        "expected TS2345 for the single arity-compatible overload, got: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&2769),
        "should not emit TS2769 when only one overload survives arity filtering, got: {diagnostics:?}"
    );

    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");
    let arg_start = source.find("true").expect("expected argument") as u32;
    assert_eq!(
        diag.start, arg_start,
        "TS2345 should anchor at the argument"
    );
    assert_eq!(diag.length, 4, "TS2345 should cover only the argument span");
}

#[test]
fn ts2769_multiple_arity_compatible_mismatches_stay_overload_errors() {
    let source = r#"
declare function fn(value: 1): void;
declare function fn<T extends 1>(value: T): void;
fn(2);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2769),
        "expected TS2769 when multiple arity-compatible overloads fail, got: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&2345),
        "should not collapse multiple arity-compatible overload failures to TS2345, got: {diagnostics:?}"
    );
}

#[test]
fn ts2769_array_best_common_type_keeps_nullable_member() {
    let source = r#"
class Box {
    take(value: boolean): number;
    take(value: string): number;
    take(value: number): number;
    take(value: any): any { return value; }
}

<number>(new Box().take([4, 2, undefined][0]));
"#;

    let diagnostics = check_source_with_strict_null(source);
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2769),
        "expected TS2769 when array BCT preserves undefined, got: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&2345),
        "multi-overload nullable mismatch should stay TS2769, got: {diagnostics:?}"
    );
}

#[test]
fn ts2554_excess_argument_span_starts_at_first_excess_argument() {
    let source = r#"
declare function takes(): void;
takes(1, 2);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2554)
        .expect("expected TS2554");

    let first_excess = source.find("1, 2").expect("expected excess arguments") as u32;
    assert_eq!(
        diag.start, first_excess,
        "TS2554 should start at the first excess argument"
    );
    assert_eq!(
        diag.length, 4,
        "TS2554 should cover the contiguous excess-argument span"
    );
}
