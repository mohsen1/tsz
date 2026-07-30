use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::test_fixtures::{merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::construction::TypeInterner;

fn check_source_with_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<crate::checker::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn check_source_with_libs(source: &str) -> Vec<crate::checker::diagnostics::Diagnostic> {
    check_source_with_options(source, CheckerOptions::default())
}

#[test]
fn strict_bind_call_apply_bind_this_arg_mismatch_uses_ts2769() {
    let source = r#"
class C {
    foo(this: this, a: number, b: string): string { return ""; }
}
declare let c: C;
c.foo.bind(undefined);
"#;
    let diagnostics = check_source_with_libs(source);
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2769)
        .expect("expected TS2769");

    let arg_start = source
        .find("undefined")
        .expect("expected undefined argument") as u32;
    assert_eq!(
        diag.start, arg_start,
        "TS2769 should anchor at the thisArg, got: {diag:?}"
    );
    assert_eq!(diag.length, "undefined".len() as u32);
}

// TS2769 anchor position: TypeScript 7 anchors at the receiver because the
// final generic bind overload fails its implicit method-`this` relation.
#[test]
fn strict_bind_call_apply_bind_generic_this_arg_mismatch_uses_ts2769() {
    let source = r#"
function bar<T extends unknown[]>(callback: (this: 1, ...args: T) => void) {
    callback.bind(2);
}
"#;
    let diagnostics = check_source_with_libs(source);

    assert_eq!(
        diagnostics.len(),
        1,
        "expected one overload diagnostic, got: {diagnostics:?}"
    );
    let diag = &diagnostics[0];
    assert_eq!(diag.code, 2769);
    let receiver_start = source.find("callback.bind").expect("expected receiver") as u32;
    assert_eq!(
        diag.start, receiver_start,
        "generic bind receiver mismatch should anchor at `callback`, got: {diag:?}"
    );
    assert_eq!(diag.length, "callback".len() as u32);
    let related_codes: Vec<_> = diag
        .related_information
        .iter()
        .map(|related| related.code)
        .collect();
    assert_eq!(
        related_codes,
        vec![2770, 2684, 2328, 2322, 5075],
        "unexpected generic receiver failure chain: {diag:?}"
    );
    assert!(
        diag.related_information.iter().any(|related| {
            related
                .message_text
                .contains("(this: 1, ...args: unknown[]) => void")
        }),
        "receiver failure should retain the constraint-instantiated target: {diag:?}"
    );
}

#[test]
fn strict_bind_call_apply_bare_outer_rest_receiver_anchor_matrix() {
    let source = r#"
type Wrapped<A extends unknown[]> = (this: 1, ...args: A) => void;
type Identity<A extends unknown[]> = A;
type Conditional<A extends unknown[]> = A extends unknown[] ? A : never;
function renamed<Values extends unknown[]>(handler: (this: 1, ...args: Values) => void) {
    handler.bind(2);
}
function aliased<Values extends unknown[]>(wrapped: Wrapped<Values>) {
    wrapped.bind(2);
}
function defaulted<Values extends unknown[] = [string]>(
    fallback: (this: 1, ...args: Values) => void,
) {
    fallback.bind(2);
}
function noInferWrapped<Values extends unknown[]>(
    noInferFn: (this: 1, ...args: NoInfer<Values>) => void,
) {
    noInferFn.bind(2);
}
function restAlias<Values extends unknown[]>(
    aliasRestFn: (this: 1, ...args: Identity<Values>) => void,
) {
    aliasRestFn.bind(2);
}
function conditionalRest<Values extends unknown[]>(
    conditionalRestFn: (this: 1, ...args: Conditional<Values>) => void,
) {
    conditionalRestFn.bind(2);
}
function syntheticNameCollision<TThis extends unknown[]>(
    collision: (this: 1, ...args: TThis) => void,
) {
    collision.bind(2);
}
function matching<Values extends unknown[]>(ok: (this: 1, ...args: Values) => void) {
    ok.bind(1);
}
"#;
    let diagnostics = check_source_with_libs(source);
    let mut actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start, diag.length))
        .collect();
    actual.sort_unstable();

    let mut expected = [
        "handler.bind(2)",
        "wrapped.bind(2)",
        "fallback.bind(2)",
        "noInferFn.bind(2)",
        "aliasRestFn.bind(2)",
        "conditionalRestFn.bind(2)",
        "collision.bind(2)",
    ]
    .map(|needle| {
        (
            2769,
            source.find(needle).expect("expected bind receiver") as u32,
            needle.find('.').expect("expected member access") as u32,
        )
    })
    .to_vec();
    expected.sort_unstable();
    assert_eq!(actual, expected, "unexpected generic receiver anchors");
}

#[test]
fn strict_bind_receiver_anchor_crosses_deep_transparent_alias_chains() {
    let mut source = String::from(
        "type Identity<A extends unknown[]> = A;\n\
         type Layer0<A extends unknown[]> = A;\n",
    );
    for layer in 1..48 {
        source.push_str(&format!(
            "type Layer{layer}<A extends unknown[]> = Layer{}<A>;\n",
            layer - 1
        ));
    }
    // Keep the end-to-end source case deep enough to exercise repeated alias
    // expansion without turning this compiler integration test into a parser
    // stress lane. The solver query suite crosses the former 256-use cutoff
    // directly.
    let repeated = "Identity<".repeat(48) + "Values" + &">".repeat(48);
    source.push_str(
        "function distinct<Values extends unknown[]>(\n\
             distinctFn: (this: 1, ...args: Layer47<Values>) => void,\n\
         ) {\n\
             distinctFn.bind(2);\n\
         }\n",
    );
    source.push_str(&format!(
        "function repeated<Values extends unknown[]>(\n\
             repeatedFn: (this: 1, ...args: {repeated}) => void,\n\
         ) {{\n\
             repeatedFn.bind(2);\n\
         }}\n"
    ));

    let diagnostics = check_source_with_libs(&source);
    let mut actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start, diag.length))
        .collect();
    actual.sort_unstable();
    let mut expected = ["distinctFn.bind(2)", "repeatedFn.bind(2)"]
        .map(|needle| {
            (
                2769,
                source.find(needle).expect("expected bind receiver") as u32,
                needle.find('.').expect("expected member access") as u32,
            )
        })
        .to_vec();
    expected.sort_unstable();
    assert_eq!(
        actual, expected,
        "deep transparent aliases must preserve the outer rest binder"
    );
}

#[test]
fn strict_bind_call_apply_non_bare_rest_failures_anchor_this_arg() {
    let source = r#"
type Identity<A extends unknown[]> = A;
type NonIdentity<A extends unknown[]> = A extends [] ? A : never;
function fixed<Value>(fixedFn: (this: 1, value: Value) => void) {
    fixedFn.bind(2);
}
function tupled<Values extends unknown[]>(tupleFn: (this: 1, ...args: [...Values]) => void) {
    tupleFn.bind(2);
}
function arrayIntersection<Values extends unknown[]>(
    arrayIntersectionFn: (this: 1, ...args: Values & unknown[]) => void,
) {
    arrayIntersectionFn.bind(2);
}
function tupleIntersection<Values extends unknown[]>(
    tupleIntersectionFn: (this: 1, ...args: Values & []) => void,
) {
    tupleIntersectionFn.bind(2);
}
function unionWrapped<Values extends unknown[]>(
    unionWrappedFn: (this: 1, ...args: Values | []) => void,
) {
    unionWrappedFn.bind(2);
}
function conditionalNonIdentity<Values extends unknown[]>(
    conditionalFn: (this: 1, ...args: NonIdentity<Values>) => void,
) {
    conditionalFn.bind(2);
}
function receiverOwned(ownedFn: <Inner extends unknown[]>(this: 1, ...args: Inner) => void) {
    ownedFn.bind(2);
}
function receiverOwnedAlias(
    ownedAlias: <Inner extends unknown[]>(this: 1, ...args: Identity<Inner>) => void,
) {
    ownedAlias.bind(2);
}
function receiverOwnedCollision(
    ownedCollision: <TThis extends unknown[]>(this: 1, ...args: TThis) => void,
) {
    ownedCollision.bind(2);
}
function predicateOnlyCollision<TThis extends object>(
    predicateCollision: (this: 1, value: unknown) => value is TThis,
) {
    predicateCollision.bind(2);
}
function predicateAliasCollision<TThis extends object>() {
    type Alias = TThis;
    const predicateAlias =
        null as unknown as (this: 1, value: unknown) => value is Alias;
    predicateAlias.bind(2);
}
function anyRest<Values extends any[]>(anyFn: (this: 1, ...args: Values) => void) {
    anyFn.bind(2);
}
"#;
    let diagnostics = check_source_with_libs(source);
    let mut actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start, diag.length))
        .collect();
    actual.sort_unstable();

    let mut expected = [
        "fixedFn.bind(2)",
        "tupleFn.bind(2)",
        "arrayIntersectionFn.bind(2)",
        "tupleIntersectionFn.bind(2)",
        "unionWrappedFn.bind(2)",
        "conditionalFn.bind(2)",
        "ownedFn.bind(2)",
        "ownedAlias.bind(2)",
        "ownedCollision.bind(2)",
        "predicateCollision.bind(2)",
        "predicateAlias.bind(2)",
        "anyFn.bind(2)",
    ]
    .map(|needle| {
        (
            2769,
            (source.find(needle).expect("expected bind call")
                + needle.find('2').expect("expected thisArg")) as u32,
            1,
        )
    })
    .to_vec();
    expected.sort_unstable();
    assert_eq!(actual, expected, "unexpected explicit thisArg anchors");
}

// `.call` on a generic function whose type parameter is constrained by its
// `this`-type parameter (`K extends keyof T`, `this: T`) must reproduce tsc's
// `never` collapse: `keyof T` is fixed with `T` still unknown, so `K` becomes
// `keyof unknown` = `never` and the supplied argument fails (TS2345).
#[test]
fn strict_call_collapses_this_dependent_type_param_to_never() {
    let source = r#"
function f<T, K extends keyof T>(this: T, key: K): void {}
declare const o: { a: number };
f.call(o, "a");
"#;
    let diagnostics = check_source_with_libs(source);
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2345)
        .unwrap_or_else(|| panic!("expected TS2345 never collapse, got: {diagnostics:?}"));
    assert!(
        diag.message_text.contains("never"),
        "expected the collapsed parameter to render as 'never', got: {diag:?}"
    );
}

// Renaming the binders must not change the outcome — the collapse is driven by
// the structural `this`-type dependency, not identifier text.
#[test]
fn strict_call_collapse_is_binder_name_agnostic() {
    let source = r#"
function pluck<Obj, Prop extends keyof Obj>(this: Obj, key: Prop): Obj[Prop] {
    return this[key];
}
declare const o: { a: number };
pluck.call(o, "a");
"#;
    let diagnostics = check_source_with_libs(source);
    assert!(
        diagnostics.iter().any(|diag| diag.code == 2345),
        "expected TS2345 regardless of binder names, got: {diagnostics:?}"
    );
}

// `.apply` passes the rest args as a tuple; the collapsed `never` element makes
// the tuple argument fail too (surfaced as the element-level mismatch against
// `never`). The behavioral fix is the collapse; assert an error fires and names
// `never` rather than pinning the exact code.
#[test]
fn strict_apply_collapses_this_dependent_type_param_to_never() {
    let source = r#"
function f<T, K extends keyof T>(this: T, key: K): void {}
declare const o: { a: number };
f.apply(o, ["a"]);
"#;
    let diagnostics = check_source_with_libs(source);
    assert!(
        diagnostics
            .iter()
            .any(|diag| matches!(diag.code, 2345 | 2322) && diag.message_text.contains("never")),
        "expected apply collapse to reject against 'never', got: {diagnostics:?}"
    );
}

// Control: a direct call infers in natural order (`T` from the object arg, `K`
// from the key arg), so there is no collapse and no diagnostic — matching tsc.
#[test]
fn strict_direct_call_does_not_collapse() {
    let source = r#"
function g<T, K extends keyof T>(obj: T, key: K): void {}
declare const o: { a: number };
g(o, "a");
"#;
    let diagnostics = check_source_with_libs(source);
    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2345),
        "direct call must not collapse, got: {diagnostics:?}"
    );
}

// Control: when the constrained type parameter does NOT reference the
// `this`-type parameter (concrete `this`), there is no collapse — tsc clean.
#[test]
fn strict_call_this_independent_constraint_does_not_collapse() {
    let source = r#"
function h<K extends keyof { a: number }>(this: { a: number }, key: K): void {}
declare const o: { a: number };
h.call(o, "a");
"#;
    let diagnostics = check_source_with_libs(source);
    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2345),
        "this-independent constraint must not collapse, got: {diagnostics:?}"
    );
}

// Control: `.bind` defers the rest-arg check to the bound function's later
// invocation, so binding alone produces no collapse diagnostic — tsc clean.
#[test]
fn strict_bind_does_not_collapse_at_bind_site() {
    let source = r#"
function f<T, K extends keyof T>(this: T, key: K): void {}
declare const o: { a: number };
f.bind(o);
"#;
    let diagnostics = check_source_with_libs(source);
    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2345),
        "bind must not collapse at the bind site, got: {diagnostics:?}"
    );
}

// --- Overloaded receivers (issue #15635) ---------------------------------
//
// Structural rule: `tsc` resolves `.call`/`.apply`/`.bind` under
// `strictBindCallApply` against the single generic method signature of the
// lib `CallableFunction`/`NewableFunction` type, and its signature-list
// inference aligns source and target signatures from the end — an overloaded
// receiver is therefore modeled by its LAST overload only (the documented
// strictBindCallApply caveat). An argument mismatch surfaces as one plain
// TS2345 against the last overload's instantiated parameter, never as a
// synthesized TS2769 expansion of the receiver's own overload set.
// All behaviors below are differentially verified against tsc 6.0.2.

/// The issue #15635 witness (with a lib-independent mismatch argument): a
/// `.call` on an overloaded function reports a single TS2345 against the last
/// overload's parameter, anchored at the argument — not TS2769.
#[test]
fn strict_call_on_overloaded_function_reports_single_ts2345_from_last_overload() {
    let source = r#"
interface Ctx { tag: string }
declare function m(this: Ctx, a: string): void;
declare function m(this: Ctx, a: number): void;
declare const flag: boolean;
m.call({ tag: "x" }, flag);
"#;
    let diagnostics = check_source_with_libs(source);
    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2769),
        "overloaded `.call` must not expand the receiver overload set into \
         TS2769, got: {diagnostics:?}"
    );
    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2345)
        .collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345, got: {diagnostics:?}"
    );
    let diag = ts2345[0];
    assert!(
        diag.message_text.contains("'number'"),
        "the error must name the LAST overload's parameter type, got: {diag:?}"
    );
    let arg_start = source.rfind("flag").expect("argument position") as u32;
    assert_eq!(
        diag.start, arg_start,
        "TS2345 should anchor at the mismatching argument, got: {diag:?}"
    );
}

/// The documented tsc caveat: an argument that matches only an EARLIER
/// overload still fails, because only the last overload is modeled.
#[test]
fn strict_call_overloaded_receiver_rejects_earlier_overload_argument() {
    let diagnostics = check_source_with_libs(
        r#"
interface Ctx { tag: string }
declare function m(this: Ctx, a: string): void;
declare function m(this: Ctx, a: number): void;
m.call({ tag: "x" }, "str");
"#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == 2345 && diag.message_text.contains("'number'")),
        "argument matching only the first overload must be rejected against \
         the last overload's parameter, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2769),
        "no TS2769 expansion, got: {diagnostics:?}"
    );
}

/// Control: an argument matching the last overload is accepted.
#[test]
fn strict_call_overloaded_receiver_accepts_last_overload_argument() {
    let diagnostics = check_source_with_libs(
        r#"
interface Ctx { tag: string }
declare function m(this: Ctx, a: string): void;
declare function m(this: Ctx, a: number): void;
m.call({ tag: "x" }, 42);
"#,
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diag| matches!(diag.code, 2345 | 2769 | 2322)),
        "argument matching the last overload must be accepted, got: {diagnostics:?}"
    );
}

/// Renamed binders with the overload order REVERSED: the reported parameter
/// tracks the last declaration (`string` here), proving the rule is
/// "last overload", not "first" and not keyed to any type or name.
#[test]
fn strict_call_last_overload_rule_is_order_and_name_agnostic() {
    let diagnostics = check_source_with_libs(
        r#"
interface Env { id: number }
declare function pick(this: Env, x: boolean): void;
declare function pick(this: Env, x: string): void;
declare const n: number;
pick.call({ id: 1 }, n);
"#,
    );
    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2345)
        .collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345, got: {diagnostics:?}"
    );
    assert!(
        ts2345[0].message_text.contains("'string'"),
        "the LAST declared overload's parameter must be reported, got: {:?}",
        ts2345[0]
    );
    assert!(!diagnostics.iter().any(|diag| diag.code == 2769));
}

/// The `thisArg` parameter also comes from the last overload.
#[test]
fn strict_call_this_arg_comes_from_last_overload() {
    let diagnostics = check_source_with_libs(
        r#"
declare function k(this: { a: string }, x: string): void;
declare function k(this: { b: number }, x: number): void;
declare const wrongThis: { a: string };
k.call(wrongThis, 42);
"#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == 2741 && diag.message_text.contains("{ b: number; }")),
        "thisArg must be checked against the LAST overload's this type \
         (promoted TS2741 head per tsc 7.0.2), got: {diagnostics:?}"
    );
    assert!(!diagnostics.iter().any(|diag| diag.code == 2769));
}

/// `.apply` on an overloaded receiver: the tuple/element mismatch is against
/// the last overload's parameter list, with no TS2769 expansion; a tuple
/// matching the last overload is accepted.
#[test]
fn strict_apply_on_overloaded_function_uses_last_overload() {
    let bad = check_source_with_libs(
        r#"
interface Ctx { tag: string }
declare function m(this: Ctx, a: string): void;
declare function m(this: Ctx, a: number): void;
declare const flag: boolean;
m.apply({ tag: "x" }, [flag]);
"#,
    );
    assert!(
        !bad.iter().any(|diag| diag.code == 2769),
        "overloaded `.apply` must not expand into TS2769, got: {bad:?}"
    );
    assert!(
        bad.iter()
            .any(|diag| matches!(diag.code, 2345 | 2322) && diag.message_text.contains("'number'")),
        "the args tuple must be checked against the last overload, got: {bad:?}"
    );

    let good = check_source_with_libs(
        r#"
interface Ctx { tag: string }
declare function m(this: Ctx, a: string): void;
declare function m(this: Ctx, a: number): void;
m.apply({ tag: "x" }, [42]);
"#,
    );
    assert!(
        !good
            .iter()
            .any(|diag| matches!(diag.code, 2345 | 2769 | 2322)),
        "args tuple matching the last overload must be accepted, got: {good:?}"
    );
}

/// `.bind` partial application on an overloaded receiver sees only the last
/// overload: a prefix argument that matches only the first overload fails,
/// one matching the last overload binds cleanly and stays callable.
#[test]
fn strict_bind_partial_application_uses_last_overload() {
    let bad = check_source_with_libs(
        r#"
interface Ctx { tag: string }
declare function m(this: Ctx, a: string): void;
declare function m(this: Ctx, a: number): void;
m.bind({ tag: "x" }, "str");
"#,
    );
    assert!(
        bad.iter().any(|diag| matches!(diag.code, 2345 | 2769)),
        "bind prefix argument matching only the first overload must be \
         rejected, got: {bad:?}"
    );

    let good = check_source_with_libs(
        r#"
interface Ctx { tag: string }
declare function m(this: Ctx, a: string): void;
declare function m(this: Ctx, a: number): void;
const bound = m.bind({ tag: "x" }, 42);
bound();
"#,
    );
    assert!(
        !good
            .iter()
            .any(|diag| matches!(diag.code, 2345 | 2769 | 2322 | 2554)),
        "bind prefix argument matching the last overload must be accepted \
         and the bound function callable, got: {good:?}"
    );
}

/// Overloaded class constructors go through the `NewableFunction` model with
/// the same last-overload rule.
#[test]
fn strict_call_on_overloaded_constructor_uses_last_construct_signature() {
    let bad = check_source_with_libs(
        r#"
class Box {
  constructor(v: string);
  constructor(v: number);
  constructor(v: unknown) {}
}
declare const s: string;
Box.call(new Box(1), s);
"#,
    );
    assert!(
        !bad.iter().any(|diag| diag.code == 2769),
        "overloaded constructor `.call` must not expand into TS2769, got: {bad:?}"
    );
    assert!(
        bad.iter()
            .any(|diag| diag.code == 2345 && diag.message_text.contains("'number'")),
        "the argument must be checked against the LAST construct signature, \
         got: {bad:?}"
    );

    let good = check_source_with_libs(
        r#"
class Box {
  constructor(v: string);
  constructor(v: number);
  constructor(v: unknown) {}
}
Box.call(new Box(1), 42);
"#,
    );
    assert!(
        !good
            .iter()
            .any(|diag| matches!(diag.code, 2345 | 2769 | 2322)),
        "argument matching the last construct signature must be accepted, \
         got: {good:?}"
    );
}

/// A hybrid receiver with both call and construct signatures resolves
/// `.call` through `CallableFunction` — construct signatures contribute no
/// method candidates (tsc: call signatures shadow `NewableFunction`).
#[test]
fn strict_call_on_hybrid_receiver_uses_call_signatures_only() {
    let bad = check_source_with_libs(
        r#"
interface Hybrid {
  (a: string): void;
  new (a: number): object;
}
declare const h: Hybrid;
h.call(undefined, 42);
"#,
    );
    assert!(
        bad.iter()
            .any(|diag| diag.code == 2345 && diag.message_text.contains("'string'")),
        "hybrid `.call` must check against the CALL signature, got: {bad:?}"
    );
    assert!(!bad.iter().any(|diag| diag.code == 2769));

    let good = check_source_with_libs(
        r#"
interface Hybrid {
  (a: string): void;
  new (a: number): object;
}
declare const h: Hybrid;
h.call(undefined, "ok");
"#,
    );
    assert!(
        !good
            .iter()
            .any(|diag| matches!(diag.code, 2345 | 2769 | 2322)),
        "hybrid `.call` matching the call signature must be accepted, got: {good:?}"
    );
}

/// Overloaded methods on an interface receiver follow the same rule.
#[test]
fn strict_call_on_overloaded_interface_method_uses_last_overload() {
    let diagnostics = check_source_with_libs(
        r#"
interface Ops {
  run(this: Ops, a: string): void;
  run(this: Ops, a: number): void;
}
declare const ops: Ops;
declare const flag2: boolean;
ops.run.call(ops, flag2);
"#,
    );
    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2345)
        .collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345, got: {diagnostics:?}"
    );
    assert!(
        ts2345[0].message_text.contains("'number'"),
        "interface method `.call` must use the last overload, got: {:?}",
        ts2345[0]
    );
    assert!(!diagnostics.iter().any(|diag| diag.code == 2769));
}

/// With `strictBindCallApply: false` the loose `Function.call` fallback
/// applies and no argument checking happens at all.
#[test]
fn loose_call_on_overloaded_function_is_unchecked() {
    let diagnostics = check_source_with_options(
        r#"
interface Ctx { tag: string }
declare function m(this: Ctx, a: string): void;
declare function m(this: Ctx, a: number): void;
declare const flag: boolean;
m.call({ tag: "x" }, flag);
"#,
        CheckerOptions {
            strict: false,
            strict_function_types: false,
            strict_bind_call_apply: false,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diag| matches!(diag.code, 2345 | 2769 | 2322)),
        "with strictBindCallApply off the call must be unchecked, got: {diagnostics:?}"
    );
}

#[test]
fn strict_bind_call_apply_apply_tuple_argument_display_stays_unnamed() {
    let diagnostics = check_source_with_libs(
        r#"
declare function foo(a: number, b: string): string;
foo.apply(undefined, [10]);
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2345)
        .expect("expected TS2345");
    assert!(
        diag.message_text.contains("Argument of type '[number]'"),
        "expected unnamed tuple source display, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("[a: number]"),
        "actual tuple display should not inherit contextual names, got: {diag:?}"
    );
}
