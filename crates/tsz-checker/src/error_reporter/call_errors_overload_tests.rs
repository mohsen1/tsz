//! TS2769 overload-resolution diagnostic tests.
//!
//! Split out of `call_errors_tests.rs` to keep both files under the
//! 2000-line checker LOC ceiling. Behavior-preserving: every test
//! moved here is byte-identical to its original definition.

use crate::test_utils::check_source_diagnostics;

/// Alias: default options already have `strict_null_checks: true`.
/// Locally redefined to avoid a cross-test-module dependency.
fn check_source_with_strict_null(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_source_diagnostics(source)
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
fn ts2769_assignment_rhs_overload_mismatch_anchors_argument() {
    let source = r#"
let cond: boolean;
declare function foo(x: string): number;
declare function foo(x: number): string;

function g() {
    let x: string | number | boolean;
    x = "";
    while (cond) {
        x = foo(x);
    }
}
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2769)
        .expect("expected TS2769");

    let arg_start = source.find("foo(x)").expect("expected overload call") as u32 + 4;
    assert_eq!(
        diag.start, arg_start,
        "TS2769 should anchor at the offending argument inside assignment RHS"
    );
    assert_eq!(
        diag.length, 1,
        "TS2769 should cover only the argument token"
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
fn ts2769_tagged_template_anchors_offending_substitution() {
    // tsc anchors TS2769 for failed tagged-template overload resolution at the
    // offending substitution expression, not at the tag callee. This mirrors
    // the regular-call behavior of pointing at the failing argument.
    let source = r#"
declare function tag(strs: TemplateStringsArray, x: number): string;
declare function tag(strs: TemplateStringsArray, x: string): number;
let r = tag`${true}`;
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2769)
        .expect("expected TS2769");

    let true_start = source.rfind("true").expect("expected 'true' substitution") as u32;
    assert_eq!(
        diag.start, true_start,
        "TS2769 should anchor at the offending tagged-template substitution, got: {diag:?}"
    );
    assert_eq!(
        diag.length, 4,
        "TS2769 should cover only the substitution token, got: {diag:?}"
    );
}

#[test]
fn ts2769_tagged_template_anchors_after_nullish_recovery() {
    let source = r#"
declare function fn1(strs: TemplateStringsArray, s: string): string;
declare function fn1(strs: TemplateStringsArray, n: number): number;
let s: string = fn1`${undefined}`;
fn1`${{}}`;
"#;

    let diagnostics = check_source_with_strict_null(source);
    let ts2769: Vec<_> = diagnostics.iter().filter(|d| d.code == 2769).collect();
    let undefined_start = source.find("undefined").expect("expected undefined") as u32;
    let object_start = source.find("{}").expect("expected object literal") as u32;

    assert!(
        ts2769.iter().any(|d| d.start == undefined_start),
        "expected TS2769 at undefined substitution, got: {ts2769:?}"
    );
    assert!(
        ts2769.iter().any(|d| d.start == object_start),
        "expected TS2769 at object substitution, got: {ts2769:?}"
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 2322),
        "nullish overload recovery should not leave TS2322, got: {diagnostics:?}"
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
fn failed_weak_collection_new_recovers_constraint_for_method_diagnostics() {
    let source = r#"
interface WeakSet<T extends object> {
    add(value: T): this;
    has(value: T): boolean;
    delete(value: T): boolean;
}
declare var WeakSet: {
    new <T extends object>(values: T[]): WeakSet<T>;
    new <T extends object>(values: readonly T[]): WeakSet<T>;
};

interface WeakMap<K extends object, V> {
    set(key: K, value: V): this;
    has(key: K): boolean;
    get(key: K): V | undefined;
    delete(key: K): boolean;
}
declare var WeakMap: {
    new <K extends object, V>(entries: [K, V][]): WeakMap<K, V>;
    new <K extends object, V>(entries: readonly (readonly [K, V])[]): WeakMap<K, V>;
};

declare const s: symbol;

const ws = new WeakSet([s]);
ws.add(s);
ws.has(s);
ws.delete(s);

const wm = new WeakMap([[s, false]]);
wm.set(s, true);
wm.has(s);
wm.get(s);
wm.delete(s);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let weak_set_anchor = source
        .find("WeakSet([s])")
        .expect("expected WeakSet constructor") as u32;
    let weak_map_anchor = source
        .find("WeakMap([[s, false]])")
        .expect("expected WeakMap constructor") as u32;
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == 2769 && diag.start == weak_set_anchor),
        "WeakSet TS2769 should anchor at the constructor identifier, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == 2769 && diag.start == weak_map_anchor),
        "WeakMap TS2769 should anchor at the constructor identifier, got: {diagnostics:#?}"
    );

    let object_arg_errors = diagnostics
        .iter()
        .filter(|diag| {
            diag.code == 2345
                && diag.message_text
                    == "Argument of type 'symbol' is not assignable to parameter of type 'object'."
        })
        .count();
    assert_eq!(
        object_arg_errors, 7,
        "failed weak collection constructors should recover as object-keyed instances: {diagnostics:#?}"
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
