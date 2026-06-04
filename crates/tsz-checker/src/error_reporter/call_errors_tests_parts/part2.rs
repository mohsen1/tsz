#[test]
fn ts2345_generic_arg_with_constrained_tp_passed_to_unconstrained_generic_param() {
    // tsc#11703: `take(g)` must emit TS2345 because `g: <U extends object>(x: U) => U`
    // has a stricter constraint than the unconstrained `<T>(x: T) => T` that `take` expects.
    // The mismatch is structural and cannot be resolved by outer inference.
    let source = r#"
declare function take<T>(f: (x: T) => T): void;
declare const g: <U extends object>(x: U) => U;
take(g);
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2345),
        "Expected TS2345 when passing a constrained-TP generic function to an \
         unconstrained-TP generic parameter, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn ts2345_generic_arg_constrained_tp_different_names() {
    // The same rule holds regardless of how the type parameters are named.
    // `<A extends string>(x: A) => A` passed to `<X>(x: X) => X` must also error.
    let source = r#"
declare function take<X>(f: (x: X) => X): void;
declare const g: <A extends string>(x: A) => A;
take(g);
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2345),
        "Expected TS2345 for constrained-TP generic arg regardless of type param names, \
         got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn no_ts2345_unconstrained_generic_arg_passed_to_unconstrained_generic_param() {
    // `<V>(x: V) => V` passed to `<T>(x: T) => T` is structurally compatible — no error.
    let source = r#"
declare function take<T>(f: (x: T) => T): void;
declare const h: <V>(x: V) => V;
take(h);
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        !diagnostics.iter().any(|d| d.code == 2345),
        "Expected no TS2345 when passing an unconstrained generic function to an \
         unconstrained generic parameter, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn no_ts2345_for_generic_component_props_contextual_inference() {
    let source = r#"
type ComponentProps<T> = T extends (props: infer P) => unknown ? P : never;
declare function wrapComponent<P>(component: (props: P) => unknown): (props: P) => unknown;
const WrappedComponent = wrapComponent(
  <T extends string = "span">(props: { as?: T | undefined; className?: string }) => null,
);
type RetrievedProps = ComponentProps<typeof WrappedComponent>;
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        !diagnostics.iter().any(|d| d.code == 2345),
        "Expected contextual generic props inference to stay deferrable, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn no_ts2345_for_higher_order_generic_callback_inference() {
    let source = r#"
declare function f2<T>(cb: <S extends number>(x: S) => T): T;
declare function f3<T>(cb: <S extends Array<S>>(x: S) => T): T;
let x2 = f2(x => x);
let x3 = f3(x => x);
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        !diagnostics.iter().any(|d| d.code == 2345),
        "Expected constrained higher-order callback inference to stay deferrable, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn no_ts2345_pipe_overload_with_interface_callable_operators() {
    for iface_name in &["Op", "Operator"] {
        let source = format!(
            r#"
interface {iface_name}<A, B> {{ (source: A): B; }}
declare function lift<A, B>(fn: (a: A) => B): {iface_name}<A, B>;
declare function pipe<A, B>(op1: {iface_name}<A, B>): {iface_name}<A, B>;
declare function pipe<A, B, C>(op1: {iface_name}<A, B>, op2: {iface_name}<B, C>): {iface_name}<A, C>;
const r = pipe(lift((x: number) => x + 1), lift((y: number) => y.toString()));
"#
        );
        let diagnostics = check_source_with_strict_null(&source);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == 2345 || d.code == 2322)
            .collect();
        assert!(
            errors.is_empty(),
            "pipe overload with interface callable {iface_name} must not produce TS2345/TS2322, \
             got: {:?}",
            errors
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn no_ts2345_pipe_overload_with_aliased_interface_callable_operators() {
    let source = r#"
interface OperatorFunction<T, R> { (source: T): R; }
declare function map<T, R>(fn: (t: T) => R): OperatorFunction<T, R>;
declare function filter<T>(pred: (t: T) => boolean): OperatorFunction<T, T>;
declare function pipe<A, B>(op1: OperatorFunction<A, B>): OperatorFunction<A, B>;
declare function pipe<A, B, C>(op1: OperatorFunction<A, B>, op2: OperatorFunction<B, C>): OperatorFunction<A, C>;
const myMap = map;
const myFilter = filter;
const result = pipe(myMap((x: number) => x + 1), myFilter(y => y > 0));
"#;
    let diagnostics = check_source_with_strict_null(source);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2322)
        .collect();
    assert!(
        errors.is_empty(),
        "pipe overload with aliased interface callable operators must not produce errors, got: {:?}",
        errors
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn outer_and_overload_share_tp_name_metadata_but_stay_distinct() {
    // Regression: when an outer generic function <T> calls an overloaded generic
    // function whose overloads also use <T>, both unconstrained and with identical
    // metadata, the checker must NOT collapse them via a structural-metadata guard.
    // TypeId identity (own_type_param_ids) is the correct discriminant.
    //
    // Two name variants guard against hardcoded-name regressions.
    for outer_name in &["T", "U"] {
        let source = format!(
            r#"
interface Box<{outer_name}> {{ (value: {outer_name}): {outer_name}; }}
declare function wrapOuter<{outer_name}>(x: {outer_name}): Box<{outer_name}>;
declare function combine<{outer_name}>(a: Box<{outer_name}>): Box<{outer_name}>;
declare function combine<{outer_name}, V>(a: Box<{outer_name}>, b: Box<V>): Box<V>;
const r = combine(wrapOuter(42), wrapOuter("hello"));
"#
        );
        let diagnostics = check_source_with_strict_null(&source);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == 2345 || d.code == 2322)
            .collect();
        assert!(
            errors.is_empty(),
            "outer <{outer_name}> and overload <{outer_name}> sharing name/metadata must not \
             produce TS2345/TS2322, got: {:?}",
            errors
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn constructor_inference_keeps_outer_type_params_as_source_candidates() {
    let source = r#"
export class Test<A, B> {
    constructor(public a: A, public b: B) { }

    test<C>(c: C): Test<B, C> {
        return new Test(this.b, c);
    }
}
"#;
    let diagnostics = check_source_with_strict_null(source);
    assert!(
        !diagnostics.iter().any(|d| d.code == 2345),
        "outer class TypeParams used as constructor arguments must stay real candidates, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn explicit_callback_param_annotation_keeps_tsc_argument_blame() {
    let source = r#"
class C<T> {
    foo2<T, U>(x: T, cb: (a: T) => U) {
        return cb(x);
    }
}

declare var c: C<number>;

function other<T, U>(t: T, u: U) {
    var r = c.foo2(1, (x: T) => '');
}
"#;
    let diagnostics = check_source_with_strict_null(source);
    let ts2345: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345
            .iter()
            .any(|d| d.message_text.contains("Argument of type 'number'")),
        "explicit callback annotation should keep tsc-style blame on the first argument, got: {:?}",
        ts2345.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
    assert!(
        !ts2345
            .iter()
            .any(|d| d.message_text.contains("(x: T) => string")),
        "explicit callback annotation must not be rewritten into placeholder-based callback blame, got: {:?}",
        ts2345.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Assert that `source` produces a single TS2345 whose head message contains
/// `expected_head` and whose related-information elaboration (each entry as
/// `(depth, message)`) equals `expected_related`. Centralizes the shape shared
/// by the same-generic argument-elaboration regression tests below.
fn assert_ts2345_elaboration(source: &str, expected_head: &str, expected_related: &[(u8, &str)]) {
    let diagnostics = check_source_with_strict_null(source);
    let head = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .unwrap_or_else(|| panic!("expected TS2345, got: {diagnostics:?}"));
    assert!(
        head.message_text.contains(expected_head),
        "unexpected TS2345 head: {head:?}"
    );
    let related: Vec<(u8, &str)> = head
        .related_information
        .iter()
        .map(|r| (r.depth, r.message_text.as_str()))
        .collect();
    assert_eq!(
        related, expected_related,
        "TS2345 must elaborate the differing type argument directly (no `Types of \
         property` wrapper), got: {related:?}"
    );
}

/// Same-generic call argument mismatch (`Wrap<string>` argument vs
/// `Wrap<number>` parameter): tsc elaborates the differing type *argument*
/// directly beneath the TS2345 head (`Type 'string' is not assignable to type
/// 'number'.`) with no intermediate `Types of property 'held' are
/// incompatible.` wrapper. This is the TS2345 sibling of the TS2322 assignment
/// elaboration; before the fix the call-argument path dropped it entirely.
/// Binder names deliberately differ from the canonical `Box`/`value` so the
/// elaboration cannot be keyed off identifier text.
#[test]
fn same_generic_call_argument_elaborates_type_argument_directly() {
    assert_ts2345_elaboration(
        "interface Wrap<Inner> { held: Inner }\n\
         declare function take(slot: Wrap<number>): void;\n\
         declare let supplied: Wrap<string>;\n\
         take(supplied);\n",
        "Argument of type 'Wrap<string>' is not assignable to parameter of type 'Wrap<number>'.",
        &[(0, "Type 'string' is not assignable to type 'number'.")],
    );
}

/// Nested same-generic call argument (`Wrap<Wrap<string>>` vs
/// `Wrap<Wrap<number>>`): the elaboration recurses, peeling one application
/// layer per indent level just like tsc.
#[test]
fn nested_same_generic_call_argument_elaborates_each_layer() {
    assert_ts2345_elaboration(
        "interface Wrap<Inner> { held: Inner }\n\
         declare function take(slot: Wrap<Wrap<number>>): void;\n\
         declare let supplied: Wrap<Wrap<string>>;\n\
         take(supplied);\n",
        "Argument of type 'Wrap<Wrap<string>>' is not assignable to parameter of type 'Wrap<Wrap<number>>'.",
        &[
            (
                0,
                "Type 'Wrap<string>' is not assignable to type 'Wrap<number>'.",
            ),
            (1, "Type 'string' is not assignable to type 'number'."),
        ],
    );
}

/// Same-generic constructor argument routes through the identical
/// argument-assignability path, so it carries the same elaboration.
#[test]
fn same_generic_constructor_argument_elaborates_type_argument_directly() {
    assert_ts2345_elaboration(
        "interface Wrap<Inner> { held: Inner }\n\
         class Receiver { constructor(slot: Wrap<number>) {} }\n\
         declare let supplied: Wrap<string>;\n\
         new Receiver(supplied);\n",
        "Argument of type 'Wrap<string>' is not assignable to parameter of type 'Wrap<number>'.",
        &[(0, "Type 'string' is not assignable to type 'number'.")],
    );
}
