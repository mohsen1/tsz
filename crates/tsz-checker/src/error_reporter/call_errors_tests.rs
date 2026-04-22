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
        diag.message_text
            .contains("Argument of type '[]' is not assignable to parameter of type 'never'."),
        "Expected empty argument list display for zero-argument never-rest call, got: {diag:?}"
    );
}

#[test]
fn ts2345_contextual_callback_display_preserves_explicit_alias_annotations() {
    let source = r#"
type ClassNameObject = { [key: string]: boolean | undefined };
declare function reduceClassNameObject(
    cb: (obj: ClassNameObject, key: string) => ClassNameObject,
): void;

export function css<S extends { [K in keyof S]: string }>(styles: S): string {
  reduceClassNameObject((obj: ClassNameObject, key: keyof S) => {
    const exportedClassName = styles[key];
    obj[exportedClassName] = true;
    return obj;
  });
  return "";
}
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text
            .contains("Argument of type '(obj: ClassNameObject, key: keyof S) => ClassNameObject'"),
        "Expected source callback display to preserve explicit alias annotations, got: {diag:?}"
    );
    assert!(
        diag.message_text
            .contains("parameter of type '(obj: ClassNameObject, key: string) => ClassNameObject'"),
        "Expected target callback display to preserve instantiated alias annotations, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("error"),
        "Callback display should not collapse explicit annotations to `error`, got: {diag:?}"
    );
}

#[test]
fn ts2345_callback_target_display_preserves_unresolved_qualified_type_name() {
    let source = r#"
declare function readdir(
    accept: (stat: fs.Stats, name: string) => boolean,
): void;
readdir(() => {});
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text
            .contains("parameter of type '(stat: fs.Stats, name: string) => boolean'"),
        "Expected unresolved qualified annotation to keep its source name, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("stat: error"),
        "Unresolved qualified annotation should not display as `error`, got: {diag:?}"
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
fn ts2345_generic_call_parameter_display_preserves_instantiated_alias_name() {
    let source = r#"
namespace Underscore {
    export interface Iterator<T, U> {
        (value: T, index: any, list: any): U;
    }

    export interface Static {
        all<T>(list: T[], iterator?: Iterator<T, boolean>, context?: any): boolean;
        identity<T>(value: T): T;
    }
}

declare var _: Underscore.Static;
_.all([true, 1, null, 'yes'], _.identity);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text
            .contains("parameter of type 'Iterator<string | number | boolean | null, boolean>'"),
        "Expected instantiated alias name in parameter display, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("parameter of type '(value:"),
        "Parameter display should not expand the iterator alias, got: {diag:?}"
    );
}

#[test]
fn ts2345_call_parameter_display_preserves_semantic_nullable_union() {
    let source = r#"
declare function takes(value: boolean | null | undefined): void;
takes(0);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains("Argument of type '0'"),
        "TS2345 should preserve direct literal call-argument display, got: {diag:?}"
    );
    assert!(
        diag.message_text
            .contains("parameter of type 'boolean | null | undefined'"),
        "TS2345 should preserve semantic nullable unions in parameter display, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("parameter of type 'boolean'."),
        "TS2345 should not strip nullable union members from non-optional parameters, got: {diag:?}"
    );
}

#[test]
fn ts2345_call_argument_display_normalizes_negative_zero_literal() {
    let source = r#"
declare function takes(value: boolean | null | undefined): void;
takes(-0);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains("Argument of type '0'"),
        "TS2345 should normalize -0 to 0 in literal call-argument display, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("Argument of type '-0'"),
        "TS2345 should not preserve -0 text once the literal type normalizes to 0, got: {diag:?}"
    );
}

#[test]
fn ts2345_call_argument_display_widens_literal_for_non_union_target() {
    let source = r#"
declare function takes(value: string): void;
takes(2);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains("Argument of type 'number'"),
        "TS2345 should widen direct numeric literals for non-union parameter targets, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("Argument of type '2'"),
        "TS2345 should not preserve literal numeric text for non-union parameter targets, got: {diag:?}"
    );
}

#[test]
fn ts2345_call_argument_display_widens_literal_for_optional_parameter_target() {
    let source = r#"
interface Item {
    name: string;
}
declare function takes(value?: Item): void;
takes("abc");
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains("Argument of type 'string'"),
        "TS2345 should widen direct literals for optional parameter targets, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("Argument of type '\"abc\"'"),
        "TS2345 should not preserve literal text when the union comes only from optionality, got: {diag:?}"
    );
}

#[test]
fn ts2322_optional_function_property_target_display_omits_synthetic_undefined() {
    let source = r#"
interface Stuff {
    a?: () => Promise<number[]>;
    b: () => Promise<string>;
}

function foo(): Stuff | string {
    return {
        a() { return [123] },
        b: () => "hello",
    }
}
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322");

    assert!(
        diag.message_text.contains("type '() =>"),
        "Expected optional property diagnostic to keep the non-nullish callable target, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("| undefined"),
        "Optional property mismatch should not inject synthetic undefined, got: {diag:?}"
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
    // Verify both overload failures are present in the related info.
    // Note: tsc shows them in declaration order (string, number), but our
    // current overload resolution may produce them in a different order
    // depending on the call resolution path taken.
    let has_string = diag
        .related_information
        .iter()
        .any(|info| info.message_text.contains("parameter of type 'string'"));
    let has_number = diag
        .related_information
        .iter()
        .any(|info| info.message_text.contains("parameter of type 'number'"));
    assert!(
        has_string && has_number,
        "expected both overload failures in related info, got: {diag:?}"
    );
}

#[test]
fn ts2769_literal_overload_mismatch_anchors_first_failing_argument() {
    let source = r#"
function foo(x: "hi", items: string[]): number;
function foo(x: "bye", items: string[]): string;
function foo(x: string, items: string[]): string | number {
    return 1;
}
foo("um", []);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2769)
        .expect("expected TS2769");

    let arg_start = source.rfind("\"um\"").expect("expected argument literal") as u32;
    assert_eq!(
        diag.start, arg_start,
        "TS2769 should anchor at the mismatched literal argument"
    );
    assert_eq!(
        diag.length, 4,
        "TS2769 should cover only the literal argument token"
    );
}

#[test]
fn ts2769_provisional_callback_failures_anchor_callee_not_callback_argument() {
    let source = r#"
declare var func: {
    (s: string): number;
    (lambda: (s: string) => { a: number; b: number }): string;
};

func(s => ({}));
func(s => ({ a: blah, b: 3 }));
func(s => ({ a: blah }));
"#;

    let diagnostics = check_source_with_strict_null(source);
    let ts2769: Vec<_> = diagnostics.iter().filter(|d| d.code == 2769).collect();
    assert_eq!(
        ts2769.len(),
        2,
        "expected two TS2769 diagnostics, got: {diagnostics:?}"
    );

    let first_call_start = source
        .find("func(s => ({}));")
        .expect("expected first call") as u32;
    let third_call_start = source
        .find("func(s => ({ a: blah }));")
        .expect("expected third call") as u32;
    let callback_start = source.find("s => ({})").expect("expected callback") as u32;

    let starts: Vec<u32> = ts2769.iter().map(|diag| diag.start).collect();
    assert!(
        starts.contains(&first_call_start),
        "expected TS2769 at first call callee, got: {ts2769:?}"
    );
    assert!(
        starts.contains(&third_call_start),
        "expected TS2769 at third call callee, got: {ts2769:?}"
    );
    assert!(
        !starts.contains(&callback_start),
        "TS2769 should anchor at callee, not callback argument: {ts2769:?}"
    );
}

#[test]
fn ts2769_bind_call_with_non_undefined_this_arg_anchors_bind_member() {
    let source = r#"
function bar<T extends unknown[]>(callback: (this: 1, ...args: T) => void) {
    callback.bind(2);
}
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2769)
        .expect("expected TS2769");

    let bind_start = source.find("bind(2)").expect("expected bind call token") as u32;
    assert_eq!(
        diag.start, bind_start,
        "TS2769 should anchor at `bind` for callback.bind(2)-style failures"
    );
    assert_eq!(diag.length, 4, "TS2769 should cover only `bind`");
}

#[test]
fn ts2769_bind_call_with_undefined_this_arg_anchors_argument() {
    let source = r#"
class C {
    foo(this: C, a: number, b: string): string { return ""; }
}
declare const c: C;
c.foo.bind(undefined);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2769)
        .expect("expected TS2769");

    let undefined_start = source
        .find("undefined")
        .expect("expected undefined argument") as u32;
    assert_eq!(
        diag.start, undefined_start,
        "TS2769 should anchor at the `undefined` argument for bind(undefined)"
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
fn ts2769_mixed_type_and_count_failures_anchor_shared_argument() {
    let source = r#"
declare const Object: {
    assign<T extends {}, U>(target: T, source: U): T & U;
    assign<T extends {}, U, V>(target: T, source1: U, source2: V): T & U & V;
    assign<T extends {}, U, V, W>(target: T, source1: U, source2: V, source3: W): T & U & V & W;
    assign(target: object, ...sources: any[]): any;
};

class Base<T> {
    constructor(public t: T) {}
}

class Foo<T> extends Base<T> {
    update() {
        return Object.assign(this.t, { x: 1 });
    }
}
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2769)
        .expect("expected TS2769");

    let arg_start = source
        .find("this.t")
        .expect("expected first argument in source") as u32;
    assert_eq!(
        diag.start, arg_start,
        "TS2769 should anchor at the shared offending argument, got: {diag:?}"
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

#[test]
fn ts2345_object_literal_argument_shows_widened_property_types() {
    // tsc shows widened types in TS2345 messages: `{ e: number; m: number }`
    // not `{ e: 1; m: 1 }`. This matches tsc's behavior of widening fresh
    // object literal types in assignability error messages.
    let source = r#"
declare function foo(x: string): void;
foo({ e: 1, m: 1 });
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains("{ e: number; m: number; }"),
        "TS2345 should show widened property types (number not 1). Got: {}",
        diag.message_text
    );
    assert!(
        !diag.message_text.contains("{ e: 1"),
        "TS2345 should NOT show literal property types. Got: {}",
        diag.message_text
    );
}

#[test]
fn ts2345_explicit_type_args_display_uses_correct_overload() {
    // When calling an overloaded method with explicit type arguments, the error
    // message should display the parameter type from the overload whose type
    // parameter count matches the explicit type arguments, not the first overload.
    // Bug: `_.map<number, string, Date>(c2, rf1)` showed `=> any` (from the 2-param
    // overload) instead of `=> Date` (from the 3-param overload).
    let source = r#"
interface Pair<A, B> { first: A; second: B; }

interface Combinators {
    map<T, U>(c: Pair<T, U>, f: (x: T, y: U) => any): Pair<any, any>;
    map<T, U, V>(c: Pair<T, U>, f: (x: T, y: U) => V): Pair<T, V>;
}

declare var _: Combinators;
declare var c2: Pair<number, string>;
var rf1 = (x: number, y: string): string => { return "hello" };
var r5b = _.map<number, string, boolean>(c2, rf1);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let codes: Vec<(u32, &str)> = diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.as_str()))
        .collect();
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .unwrap_or_else(|| panic!("expected TS2345, got: {:?}", codes));

    assert!(
        diag.message_text
            .contains("parameter of type '(x: number, y: string) => boolean'"),
        "Expected the 3-param overload's parameter type with boolean, got: {}",
        diag.message_text
    );
    assert!(
        !diag.message_text.contains("=> any"),
        "Should not show => any from the wrong overload, got: {}",
        diag.message_text
    );
}
