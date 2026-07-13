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
    // tsc 7.0.2 renders one TS2770 last-overload header holding only the LAST
    // argument-error candidate (the number overload); the string candidate
    // does not appear (differential-verified against the pinned binary).
    let chain: Vec<(u8, u32, &str)> = diag
        .related_information
        .iter()
        .map(|r| (r.depth, r.code, r.message_text.as_str()))
        .collect();
    assert_eq!(
        chain,
        vec![
            (0, 2770, "The last overload gave the following error."),
            (
                1,
                2345,
                "Argument of type 'boolean' is not assignable to parameter of type 'number'."
            ),
        ],
        "expected the last-overload chain, got: {diag:?}"
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
fn ts2769_property_call_multi_arg_mismatch_anchors_property_token() {
    let source = r#"
interface I {
    h(s1: string, s2: number): string;
    h(s1: number, s2: string): number;
}

declare var x: I;
let z: string;
z=x.h(2,2);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2769)
        .expect("expected TS2769");

    let property_start = source.find("h(2,2)").expect("expected property call") as u32;
    assert_eq!(
        diag.start, property_start,
        "TS2769 should anchor at the overloaded property token when no single argument explains the failure"
    );
    assert_eq!(diag.length, 1, "TS2769 should cover only `h`");
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
fn ts2769_pipe_like_single_arity_overload_emits_ts2345_at_argument() {
    // When only ONE overload is arity-compatible and it fails by type, tsc emits
    // TS2345 directly at the failing argument (not TS2769). Verify tsz does the same.
    let source = r#"
interface PipeOp<A, B> {
    tag: [A, B];
}
interface Obs<T> {
    pipe<A>(op1: PipeOp<T, A>): A;
    pipe<A, B>(op1: PipeOp<T, A>, op2: PipeOp<A, B>): B;
    pipe<A, B, C>(op1: PipeOp<T, A>, op2: PipeOp<A, B>, op3: PipeOp<B, C>): C;
}
declare function mapOp(fn: (x: number) => string): PipeOp<number, string>;
declare function wrongOp(): PipeOp<number, number>;
declare var obs: Obs<number>;
obs.pipe(mapOp(x => x.toString()), wrongOp());
"#;

    let diagnostics = check_source_with_strict_null(source);
    // Only one arity-compatible overload → TS2345, not TS2769
    let ts2345: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345 (single arity-compatible overload path), got: {diagnostics:?}"
    );
    let diag = &ts2345[0];
    let failing_arg_start = source.rfind("wrongOp()").expect("wrongOp() not found") as u32;
    assert_eq!(
        diag.start, failing_arg_start,
        "TS2345 should anchor at wrongOp(), got start={}, length={}",
        diag.start, diag.length
    );
}

#[test]
fn ts2769_pipe_like_multi_arity_overload_anchors_failing_argument() {
    // When MULTIPLE overloads are arity-compatible and ALL fail on the SAME
    // argument, tsc emits TS2769 anchored at the failing argument — not the callee.
    // This models the rxjs `pipe(map(...), wrongOp())` pattern.
    //
    // Structural rule: when all overload failures share the same argument as
    // the mismatching node (same `actual_type`), anchor TS2769 at that argument.
    let source = r#"
interface PipeOp<A, B> {
    tag: [A, B];
}
interface Obs<T> {
    pipe<A>(op1: PipeOp<T, A>, op2: PipeOp<A, A>): A;
    pipe<A, B>(op1: PipeOp<T, A>, op2: PipeOp<A, B>): B;
}
declare function mapOp(fn: (x: number) => string): PipeOp<number, string>;
declare function wrongOp(): PipeOp<number, number>;
declare var obs: Obs<number>;
obs.pipe(mapOp(x => x.toString()), wrongOp());
"#;

    let diagnostics = check_source_with_strict_null(source);
    // Multiple arity-compatible overloads → TS2769
    let ts2769: Vec<_> = diagnostics.iter().filter(|d| d.code == 2769).collect();
    assert_eq!(
        ts2769.len(),
        1,
        "expected exactly one TS2769 (multiple arity-compatible overloads fail), got: {diagnostics:?}"
    );
    let diag = &ts2769[0];
    let failing_arg_start = source.rfind("wrongOp()").expect("wrongOp() not found") as u32;
    assert_eq!(
        diag.start, failing_arg_start,
        "TS2769 should anchor at wrongOp() (the shared failing argument), not at the callee. got start={}, length={}",
        diag.start, diag.length
    );

    // Name-variation witness: the same structural rule holds regardless of
    // what the interface, method, and function names are.
    let source2 = r#"
interface Op<X, Y> { tag: [X, Y]; }
interface Stream<T> {
    transform<X>(t1: Op<T, X>, t2: Op<X, X>): X;
    transform<X, Y>(t1: Op<T, X>, t2: Op<X, Y>): Y;
}
declare function buildOp(fn: (n: number) => string): Op<number, string>;
declare function badOp(): Op<number, number>;
declare var s: Stream<number>;
s.transform(buildOp(n => n.toString()), badOp());
"#;
    let diagnostics2 = check_source_with_strict_null(source2);
    let ts2769_2: Vec<_> = diagnostics2.iter().filter(|d| d.code == 2769).collect();
    assert_eq!(
        ts2769_2.len(),
        1,
        "name-variation: expected TS2769 anchored at badOp(), got: {diagnostics2:?}"
    );
    let failing2 = source2.rfind("badOp()").expect("badOp() not found") as u32;
    assert_eq!(
        ts2769_2[0].start, failing2,
        "name-variation: TS2769 should anchor at badOp(), got start={}",
        ts2769_2[0].start
    );
}

#[test]
fn ts2769_property_call_anchors_callee_when_different_args_fail_different_overloads() {
    // When each overload fails because a DISTINCT argument doesn't match that
    // overload's parameter — overload 1 rejects arg 0, overload 2 rejects arg 1
    // — there is no shared failing argument, so tsc anchors TS2769 at the
    // property callee rather than at any argument.
    //
    // Structural rule: `shared_overload_argument_anchor` returns `None` when
    // the set of type-mismatching arguments differs across overloads, causing
    // the anchor to fall back to the callee.
    let source = r#"
interface Obs {
    pipe(op1: string, op2: number): void;
    pipe(op1: number, op2: string): void;
}
declare var obs: Obs;
obs.pipe(42, 42);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let ts2769: Vec<_> = diagnostics.iter().filter(|d| d.code == 2769).collect();
    assert_eq!(
        ts2769.len(),
        1,
        "expected exactly one TS2769 for cross-overload argument failure, got: {diagnostics:?}"
    );
    let diag = &ts2769[0];
    // The callee in `obs.pipe(42, 42)` is `pipe` (a property access).
    let pipe_token_start = source.rfind("pipe(42").expect("pipe(42") as u32;
    let first_arg_start = source.rfind("42, 42)").expect("42, 42)") as u32;
    let second_arg_start = first_arg_start + "42, ".len() as u32;
    // TS2769 must NOT anchor at either argument.
    assert_ne!(
        diag.start, first_arg_start,
        "should not anchor at first arg when overloads fail on different arguments, got: {diag:?}"
    );
    assert_ne!(
        diag.start, second_arg_start,
        "should not anchor at second arg when overloads fail on different arguments, got: {diag:?}"
    );
    // TS2769 should anchor at or before the end of the `pipe` token.
    assert!(
        diag.start <= pipe_token_start + "pipe".len() as u32,
        "TS2769 should anchor at callee when failures diverge across args, got start={}",
        diag.start
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
fn overload_impl_sig_excluded_bool_arg_errors_at_call_site() {
    // If the impl `(x: any)` joined the candidate set, `foo(true)` would
    // silently succeed because `true` is assignable to `any`. The error must
    // be TS2769 anchored at the call site, not inside the impl.
    assert_no_diag_in_span_emits_ts2769(
        r#"
function foo(x: string): string;
function foo(x: number): number;
function foo(x: any): any { return x; }
foo(true);
"#,
        "function foo(x: any)",
        "function foo(x: any): any { return x; }",
        None,
    );
}

#[test]
fn overload_impl_sig_excluded_class_method() {
    // Class method variant: the implementation method `(x: any)` must not
    // appear as a callable overload from the outside.
    let source = r#"
class C {
    method(x: string): string;
    method(x: number): number;
    method(x: any): any { return x; }
}
const c = new C();
c.method(true);
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2769),
        "expected TS2769 for c.method(true), got: {diagnostics:?}"
    );
}

#[test]
fn overload_single_mismatch_reports_at_arg_not_impl_span() {
    // When exactly one overload has arity-compatible mismatches the checker
    // may take a single-mismatch fast path. The diagnostic must still be
    // anchored at the argument, not at any declaration inside the function.
    let source = r#"
declare function fn(value: string): string;
declare function fn(value: string, extra: number): number;
const x = fn(42);
"#;
    let diagnostics = check_source_with_strict_null(source);
    let arg_start = source.rfind("42").unwrap() as u32;
    let overload1_start = source.find("fn(value: string)").unwrap() as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| { (d.code == 2769 || d.code == 2345) && d.start == arg_start }),
        "error must be at the argument `42` (position {arg_start}), overload start: {overload1_start}, got: {diagnostics:?}"
    );
}

// ── Assignment-context + overloads ────────────────────────────────────────────────

/// `const s: string = fn(42)` — overload 2 returns number, assigning to string
/// should give TS2322 at `s`, NOT inside the implementation body.
#[test]
fn overload_return_mismatch_ts2322_anchored_at_variable() {
    let source = r#"
function fn(x: string): string;
function fn(x: number): number;
function fn(x: any): any { return x; }
const s: string = fn(42);
"#;
    let diagnostics = check_source_with_strict_null(source);
    // fn(42) matches overload 2 returning number; `const s: string` triggers TS2322.
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322, got: {diagnostics:?}"
    );
    // The anchor must be at `s` — find its position.
    let s_pos = source.find("const s").unwrap() as u32 + "const ".len() as u32;
    assert_eq!(
        ts2322[0].start, s_pos,
        "TS2322 must be anchored at `s`, not inside the implementation; got: {:?}",
        ts2322[0]
    );
}

/// `const b: boolean = fn(true)` — no overload matches boolean, so TS2769 fires.
/// After TS2769, there should be NO cascading TS2322 for the declaration.
/// (tsc suppresses the assignment error when the call already failed.)
#[test]
fn overload_no_match_ts2769_no_cascading_ts2322_on_decl() {
    let source = r#"
function fn(x: string): string;
function fn(x: number): number;
function fn(x: any): any { return x; }
const b: boolean = fn(true);
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2769),
        "expected TS2769 for fn(true), got: {diagnostics:?}"
    );
    // tsc does NOT cascade TS2322 on a variable declaration when the assigned
    // expression already produced TS2769. Verify we do not double-report.
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "TS2322 must not cascade after TS2769 on a variable declaration, got: {diagnostics:?}"
    );
}

/// Assignment-statement form: `x = fn(true)` — TS2769 fires; no cascading TS2322.
#[test]
fn overload_no_match_ts2769_no_cascading_ts2322_on_assignment_stmt() {
    let source = r#"
function fn(x: string): string;
function fn(x: number): number;
function fn(x: any): any { return x; }
let x: string;
x = fn(true);
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2769),
        "expected TS2769, got: {diagnostics:?}"
    );
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "TS2322 must not cascade after TS2769 in assignment statement, got: {diagnostics:?}"
    );
}

/// Overloaded generic: contextual return type should help select overload.
/// `const n: number = fnG("hello")` — overload 1 returns T=number when annotated.
#[test]
fn overload_generic_contextual_return_type_selects_correct_overload() {
    let source = r#"
function fnG<T>(x: string): T;
function fnG<T>(x: number): T;
function fnG<T>(x: any): T { return x; }
const n: number = fnG("hello");
"#;
    let diagnostics = check_source_with_strict_null(source);
    // With contextual type `number`, T=number should be inferred.
    // fn("hello") should succeed (string matches string param) and return number.
    // If contextual type is missing, T=unknown and string->number gives TS2322.
    // We accept both zero errors OR a single TS2322 at `n`.
    // Key invariant: no error inside the implementation body.
    let impl_start = source.find("function fnG<T>(x: any)").unwrap() as u32;
    let impl_end = impl_start + "function fnG<T>(x: any): T { return x; }".len() as u32;
    for d in &diagnostics {
        assert!(
            d.start < impl_start || d.start >= impl_end,
            "error must not be inside the implementation body, got: {d:?}"
        );
    }
}

/// Implementation-site diagnostic guard: class method overload.
/// Call that fails TS2769 must anchor at the call site, not inside the
/// implementation method's body or its declaring class span.
#[test]
fn overload_class_method_ts2769_anchored_at_call_not_impl_body() {
    let source = r#"
class Processor {
    run(x: string): string;
    run(x: number): number;
    run(x: any): any { return x; }
}
const p = new Processor();
p.run(true);
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2769),
        "expected TS2769 for p.run(true), got: {diagnostics:?}"
    );
    // Anchor must be at the call `p.run(true)`, specifically at `true`.
    let impl_start = source.find("run(x: any)").unwrap() as u32;
    let impl_end = impl_start + "run(x: any): any { return x; }".len() as u32;
    for d in diagnostics.iter().filter(|d| d.code == 2769) {
        assert!(
            d.start < impl_start || d.start >= impl_end,
            "TS2769 anchor inside implementation body, got: {d:?}"
        );
    }
}

/// Overload with impl having no return annotation: the impl is invisible to
/// callers. TS2769 must NOT include the implementation in failure list.
#[test]
fn overload_impl_no_return_annotation_excluded_from_failures() {
    let source = r#"
function transform(x: string): string;
function transform(x: number): number;
function transform(x: any) { return x; }
transform({});
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2769),
        "expected TS2769 for transform(object), got: {diagnostics:?}"
    );
    // Count related info: should have exactly 2 entries (one per overload, not 3).
    let ts2769: Vec<_> = diagnostics.iter().filter(|d| d.code == 2769).collect();
    assert_eq!(ts2769.len(), 1, "expected one TS2769, got: {diagnostics:?}");
    // The implementation starts at:
    let impl_start = source.find("function transform(x: any)").unwrap() as u32;
    let impl_end = impl_start + "function transform(x: any) { return x; }".len() as u32;
    for d in &diagnostics {
        assert!(
            d.start < impl_start || d.start >= impl_end,
            "no error should be anchored inside the implementation, got: {d:?}"
        );
    }
}

/// Assignment context propagates to narrow overload return when contextual type
/// is explicit: `const s: string[] = map(["a","b"], x => x)` should work.
/// (Array.map has overloads; the context string[] guides T inference.)
#[test]
fn overload_contextual_return_type_propagates_to_generic_arg_inference() {
    let source = r#"
function map<T, U>(arr: T[], fn: (x: T) => U): U[];
function map<T>(arr: T[]): T[];
function map<T, U>(arr: T[], fn?: (x: T) => U): (T | U)[] { return arr as any; }
const result: string[] = map(["a", "b"], x => x);
"#;
    let diagnostics = check_source_with_strict_null(source);
    // The call should succeed cleanly with correct contextual type propagation.
    let non_trivial: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345 || d.code == 2769)
        .collect();
    assert!(
        non_trivial.is_empty(),
        "expected no type errors for map with contextual string[] return, got: {non_trivial:?}"
    );
}

/// When all overloads return the same type, overload-failure recovery remains
/// that type. tsc still reports the real downstream declaration TS2322.
#[test]
fn overload_same_return_type_ts2769_reports_decl_ts2322() {
    let source = r#"
function fn(x: string): string;
function fn(x: number): string;
function fn(x: any): any { return ""; }
const n: number = fn(true);
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2769),
        "expected TS2769 for fn(true), got: {diagnostics:?}"
    );
    let n_pos = source.find("const n").unwrap() as u32 + "const ".len() as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2322 && d.start == n_pos),
        "same-return overload recovery should keep the real declaration TS2322 at `n`, got: {diagnostics:?}"
    );
}

/// Same as above but using assignment statement form rather than declaration.
#[test]
fn overload_same_return_type_ts2769_reports_assignment_ts2322() {
    let source = r#"
function fn(x: string): string;
function fn(x: number): string;
function fn(x: any): any { return ""; }
let n: number;
n = fn(true);
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2769),
        "expected TS2769, got: {diagnostics:?}"
    );
    let assignment_pos = source.rfind("n =").unwrap() as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2322 && d.start == assignment_pos),
        "same-return overload recovery should keep the real assignment TS2322, got: {diagnostics:?}"
    );
}

#[test]
fn nested_overload_failure_does_not_suppress_outer_decl_ts2322() {
    let source = r#"
function inner(x: string): string;
function inner(x: number): string;
function inner(x: any): any { return ""; }
function outer(value: any): string { return ""; }
const n: number = outer(inner(true));
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2769),
        "expected nested TS2769 for inner(true), got: {diagnostics:?}"
    );
    let n_pos = source.find("const n").unwrap() as u32 + "const ".len() as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2322 && d.start == n_pos),
        "nested overload failures must not suppress the real outer assignment TS2322 at `n`, got: {diagnostics:?}"
    );
}

#[test]
fn nested_overload_failure_does_not_suppress_outer_assignment_ts2322() {
    let source = r#"
function inner(x: string): string;
function inner(x: number): string;
function inner(x: any): any { return ""; }
function outer(value: any): string { return ""; }
let n: number;
n = outer(inner(true));
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2769),
        "expected nested TS2769 for inner(true), got: {diagnostics:?}"
    );
    let assignment_pos = source.rfind("n =").unwrap() as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2322 && d.start == assignment_pos),
        "nested overload failures must not suppress the real outer assignment TS2322, got: {diagnostics:?}"
    );
}

#[test]
fn overload_failure_initializer_preserves_real_ts2322() {
    let source = r#"
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Exclude<T, U> = T extends U ? never : T;
type Assign<T, U> = Omit<T, keyof U> & U;
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
declare const Object: {
    assign<T extends {}, U>(target: T, source: U): T & U;
    assign(target: object, ...sources: any[]): any;
};

class Base<T> {
    constructor(public t: T) { }
}

export class Foo<T> extends Base<T> {
    update(): Foo<Assign<T, { x: number }>> {
        const v: Assign<T, { x: number }> = Object.assign(this.t, { x: 1 });
        return new Foo(v);
    }
}
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2769),
        "expected TS2769 for Object.assign(this.t, ...), got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2322
            && d.message_text
                .contains("not assignable to type 'Assign<T, { x: number; }>'")),
        "the initializer overload failure must not hide the real TS2322, got: {diagnostics:?}"
    );
}

// ── Implementation-signature exclusion: structural rule ──────────────────────
//
// Body-less overload decls are the only externally visible call signatures.
// The implementation signature is never a candidate.

fn assert_no_diag_in_span_emits_ts2769(
    source: &str,
    impl_signature: &str,
    impl_full: &str,
    code_filter: Option<u32>,
) {
    let diagnostics = check_source_with_strict_null(source);
    let impl_start = source
        .find(impl_signature)
        .unwrap_or_else(|| panic!("impl signature {impl_signature:?} not found in source"))
        as u32;
    let impl_end = impl_start + impl_full.len() as u32;
    for diag in diagnostics
        .iter()
        .filter(|d| code_filter.is_none_or(|c| d.code == c))
    {
        assert!(
            diag.start < impl_start || diag.start >= impl_end,
            "error must not be anchored inside the impl, got: {diag:?}"
        );
    }
    assert!(
        diagnostics.iter().any(|d| d.code == 2769),
        "expected TS2769, got: {diagnostics:?}"
    );
}

#[test]
fn overload_impl_excluded_with_renamed_parameters_p_form() {
    assert_no_diag_in_span_emits_ts2769(
        r#"
function pickP(p: string): string;
function pickP(p: number): number;
function pickP(p: any): any { return p; }
pickP(true);
"#,
        "function pickP(p: any)",
        "function pickP(p: any): any { return p; }",
        None,
    );
}

#[test]
fn overload_impl_excluded_with_renamed_parameters_q_form() {
    assert_no_diag_in_span_emits_ts2769(
        r#"
function pickQ(q: string): string;
function pickQ(q: number): number;
function pickQ(q: any): any { return q; }
pickQ([]);
"#,
        "function pickQ(q: any)",
        "function pickQ(q: any): any { return q; }",
        None,
    );
}

#[test]
fn overload_impl_excluded_with_three_overload_signatures() {
    assert_no_diag_in_span_emits_ts2769(
        r#"
function makeRecord(value: string): { kind: "s" };
function makeRecord(value: number): { kind: "n" };
function makeRecord(value: boolean): { kind: "b" };
function makeRecord(value: any): { kind: string } { return { kind: "x" }; }
makeRecord({ nope: 1 });
"#,
        "function makeRecord(value: any)",
        "function makeRecord(value: any): { kind: string } { return { kind: \"x\" }; }",
        None,
    );
}

/// `unknown` impl widening — the rule keys off body presence, not type spelling.
#[test]
fn overload_impl_excluded_when_impl_widens_via_unknown() {
    assert_no_diag_in_span_emits_ts2769(
        r#"
function takeU(x: string): string;
function takeU(x: number): number;
function takeU(x: unknown): unknown { return x; }
takeU(true);
"#,
        "function takeU(x: unknown)",
        "function takeU(x: unknown): unknown { return x; }",
        None,
    );
}

#[test]
fn overload_impl_excluded_for_generic_overloads_t_variant() {
    assert_no_diag_in_span_emits_ts2769(
        r#"
function transformT<T>(value: string): T;
function transformT<T>(value: number): T;
function transformT<T>(value: any): T { return value; }
const s: string = transformT(true);
"#,
        "function transformT<T>(value: any)",
        "function transformT<T>(value: any): T { return value; }",
        None,
    );
}

/// Same generic rule with `K` — proves the matrix isn't pinned to `T`.
#[test]
fn overload_impl_excluded_for_generic_overloads_k_variant() {
    assert_no_diag_in_span_emits_ts2769(
        r#"
function transformK<K>(value: string): K;
function transformK<K>(value: number): K;
function transformK<K>(value: any): K { return value; }
const s: string = transformK(true);
"#,
        "function transformK<K>(value: any)",
        "function transformK<K>(value: any): K { return value; }",
        None,
    );
}

/// Single-signature function: the lone impl signature drives diagnostics.
/// Guards against the fix regressing the no-overloads path.
#[test]
fn single_signature_function_uses_impl_signature_when_no_overloads_present() {
    let source = r#"
function onlyImpl(x: string): string { return x; }
const n: number = onlyImpl("hi");
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "expected TS2322 from single impl signature, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 2769),
        "single-signature function must not trigger TS2769, got: {diagnostics:?}"
    );
}

/// Class-method end-to-end witness — impl excluded across `c.method` and `c["method"]`.
#[test]
fn class_method_impl_excluded_across_access_paths() {
    assert_no_diag_in_span_emits_ts2769(
        r#"
class Service {
    handle(req: string): string;
    handle(req: number): number;
    handle(req: any): any { return req; }
}
const s = new Service();
s.handle(true);
s["handle"](true);
"#,
        "handle(req: any)",
        "handle(req: any): any { return req; }",
        Some(2769),
    );
}
