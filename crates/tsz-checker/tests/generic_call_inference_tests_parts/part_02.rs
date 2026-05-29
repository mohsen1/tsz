#[test]
fn conformance_probe_inferential_typing_with_function_type() {
    let source = r#"
declare function map<T, U>(x: T, f: (s: T) => U): U;
declare function identity<V>(y: V): V;

var s = map("", identity);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2345),
        "generic function argument identity should not produce TS2345. Got: {diags:#?}"
    );
}

#[test]
fn conformance_probe_generic_method_overspecialization() {
    let source = r#"
var names = ["list", "table1", "table2", "table3", "summary"];

interface HTMLElement {
    clientWidth: number;
    isDisabled: boolean;
}

declare var document: Document;
interface Document {
    getElementById(elementId: string): HTMLElement;
}

var elements = names.map(function (name) {
    return document.getElementById(name);
});

var xxx = elements.filter(function (e) {
    return !e.isDisabled;
});

var widths:number[] = elements.map(function (e) {
    return e.clientWidth;
});
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2344),
        "generic method overspecialization should not produce TS2344. Got: {diags:#?}"
    );
}

#[test]
fn conformance_probe_inference_does_not_add_undefined_or_null() {
    let source = r#"
interface NodeArray<T extends Node> extends ReadonlyArray<T> {}

interface Node {
    forEachChild<T>(cbNode: (node: Node) => T | undefined, cbNodeArray?: (nodes: NodeArray<Node>) => T | undefined): T | undefined;
}

declare function toArray<T>(value: T | T[]): T[];
declare function toArray<T>(value: T | readonly T[]): readonly T[];

function flatMapChildren<T>(node: Node, cb: (child: Node) => readonly T[] | T | undefined): readonly T[] {
    const result: T[] = [];
    node.forEachChild(child => {
        const value = cb(child);
        if (value !== undefined) {
            result.push(...toArray(value));
        }
    });
    return result;
}

function flatMapChildren2<T>(node: Node, cb: (child: Node) => readonly T[] | T | null): readonly T[] {
    const result: T[] = [];
    node.forEachChild(child => {
        const value = cb(child);
        if (value !== null) {
            result.push(...toArray(value));
        }
    });
    return result;
}
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2344),
        "inference should not add undefined or null to T. Got: {diags:#?}"
    );
}

#[test]
fn conformance_probe_infer_from_generic_function_return_types_2() {
    let source = r#"
type Mapper<T, U> = (x: T) => U;

declare function wrap<T, U>(cb: Mapper<T, U>): Mapper<T, U>;

declare function arrayize<T, U>(cb: Mapper<T, U>): Mapper<T, U[]>;

declare function combine<A, B, C>(f: (x: A) => B, g: (x: B) => C): (x: A) => C;

declare function foo(f: Mapper<string, number>): void;

let f1: Mapper<string, number> = s => s.length;
let f2: Mapper<string, number> = wrap(s => s.length);
let f3: Mapper<string, number[]> = arrayize(wrap(s => s.length));
let f4: Mapper<string, boolean> = combine(wrap(s => s.length), wrap(n => n >= 10));

foo(wrap(s => s.length));

let a1 = ["a", "b"].map(s => s.length);
let a2 = ["a", "b"].map(wrap(s => s.length));
let a3 = ["a", "b"].map(wrap(arrayize(s => s.length)));
let a4 = ["a", "b"].map(combine(wrap(s => s.length), wrap(n => n > 10)));
let a5 = ["a", "b"].map(combine(identity, wrap(s => s.length)));
let a6 = ["a", "b"].map(combine(wrap(s => s.length), identity));

class SetOf<A> {
  _store: A[];

  add(a: A) {
    this._store.push(a);
  }

  transform<B>(transformer: (a: SetOf<A>) => SetOf<B>): SetOf<B> {
    return transformer(this);
  }

  forEach(fn: (a: A, index: number) => void) {
      this._store.forEach((a, i) => fn(a, i));
  }
}

function compose<A, B, C, D, E>(
  fnA: (a: SetOf<A>) => SetOf<B>,
  fnB: (b: SetOf<B>) => SetOf<C>,
  fnC: (c: SetOf<C>) => SetOf<D>,
  fnD: (c: SetOf<D>) => SetOf<E>,
):(x: SetOf<A>) => SetOf<E>;
function compose<T>(...fns: ((x: T) => T)[]): (x: T) => T {
  return (x: T) => fns.reduce((prev, fn) => fn(prev), x);
}

function map<A, B>(fn: (a: A) => B): (s: SetOf<A>) => SetOf<B> {
  return (a: SetOf<A>) => {
    const b: SetOf<B> = new SetOf();
    a.forEach(x => b.add(fn(x)));
    return b;
  }
}

function filter<A>(predicate: (a: A) => boolean): (s: SetOf<A>) => SetOf<A> {
  return (a: SetOf<A>) => {
    const result = new SetOf<A>();
    a.forEach(x => {
      if (predicate(x)) result.add(x);
    });
   return result;
  }
}

const testSet = new SetOf<number>();
testSet.add(1);
testSet.add(2);
testSet.add(3);

const t1 = testSet.transform(
  compose(
    filter(x => x % 1 === 0),
    map(x => x + x),
    map(x => x + '!!!'),
    map(x => x.toUpperCase())
  )
)

declare function identity<T>(x: T): T;

const t2 = testSet.transform(
  compose(
    filter(x => x % 1 === 0),
    identity,
    map(x => x + '!!!'),
    map(x => x.toUpperCase())
  )
)
"#;
    let diags = relevant_default_lib_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diags, &[2322, 2345]),
        "higher-order inference should not produce extra TS2322/TS2345. Got: {diags:#?}"
    );
}

// ─── Const type parameter inference ─────────────────────────────────

#[test]
fn const_type_param_nested_array_in_object_no_false_ts2322() {
    // When a function has `const T` and multiple parameters, nested array
    // literals inside object literal arguments must be inferred as readonly
    // tuples, not plain arrays. Without const assertion flowing into nested
    // expressions, [1, 2] is typed as `number[]` which is not assignable to
    // the inferred `readonly [1, 2]`, producing a false TS2322.
    let source = r#"
declare function f<const T>(x: T, y?: string): T;
const a = f({ d: [1, 2] });
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "const type param with multi-param function should not produce false TS2322. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_single_param_nested_array_no_false_ts2322() {
    // Baseline: single-param const type param function should also work.
    let source = r#"
declare function f<const T>(x: T): T;
const a = f({ d: [1, 2] });
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "const type param with single-param function should not produce TS2322. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_empty_array_in_object_no_false_ts2322() {
    // Empty arrays inside object literals with const type params should be
    // typed as empty readonly tuples, not `never[]`.
    let source = r#"
declare function f<const T>(x: T, y?: string): T;
const a = f({ d: [] });
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "const type param with empty array should not produce false TS2322. Got: {diags:#?}"
    );
}

// ─── Issue #6261: const type param preserves literals across multiple params ──
//
// Structural rule: when a generic call has a `const` type parameter whose
// constraint does not allow a mutable array-like target, the literal shape of
// the argument expression must be the round-1 inference seed. The presence
// of additional non-const parameters, multiple type parameters, class
// constructors, or interface methods does not change this rule.

#[test]
fn const_type_param_class_constructor_preserves_object_literal() {
    // tsc preserves `g.value.x: 1` even though the constructor signature
    // includes the const type param via a property.
    let source = r#"
class ConstContainer<const T> { constructor(public value: T) {} }
const g = new ConstContainer({ x: 1 });
const _gx: 1 = g.value.x;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "class const type param should preserve literal property type. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_multiple_const_params_preserve_each_literal() {
    let source = r#"
function multiConst<const T, const U>(x: T, y: U): [T, U] { return [x, y]; }
const e = multiConst({ a: 1 }, { b: 2 });
const _e0a: 1 = e[0].a;
const _e1b: 2 = e[1].b;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "multiple const type params must each preserve literal property types. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_with_sibling_primitive_param_preserves_literal() {
    // The presence of a sibling non-const parameter (`y: number`) must not
    // cause T to be widened.
    let source = r#"
function f<const T>(x: T, y: number): T { return x; }
const r = f({ a: 1 }, 2);
const _ra: 1 = r.a;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "sibling primitive param must not break const literal preservation. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_when_const_param_is_second_preserves_literal() {
    // Position of the const-typed parameter must not matter.
    let source = r#"
function f<const T>(x: number, y: T): T { return y; }
const r = f(2, { b: 1 });
const _rb: 1 = r.b;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "const param at non-first position must still preserve literals. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_renamed_preserves_literal() {
    // Renaming the type parameter must not affect the rule (the fix is
    // structural, not name-driven).
    let source = r#"
function renamed<const P>(x: P, y: number): P { return x; }
const r = renamed({ a: 1 }, 2);
const _ra: 1 = r.a;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "renaming const type param must not break preservation. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_mixed_const_and_non_const_preserves_const_literal() {
    // `const T` preserves; `U` (non-const) widens normally.
    let source = r#"
function mixed<const T, U>(x: T, y: U): [T, U] { return [x, y]; }
const r = mixed({ a: 1 }, { b: 2 });
const _ra: 1 = r[0].a;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "const T should preserve literal even when sibling U is non-const. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_interface_method_preserves_literal() {
    let source = r#"
interface ConstMethod { process<const T>(value: T): T; }
declare const cm: ConstMethod;
const h = cm.process({ y: 2 });
const _hy: 2 = h.y;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "interface method with const type param should preserve literal. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_with_aliased_readonly_array_constraint_preserves_literal() {
    // An alias wrapping the readonly-array constraint must still trigger
    // literal preservation (the constraint is resolved before the
    // mutable-array check).
    let source = r#"
type ROArr = readonly unknown[];
function f<const T extends ROArr>(x: T, y: number): T { return x; }
const r = f([1, 2, 3], 0);
const _r: readonly [1, 2, 3] = r;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "aliased readonly-array constraint must still preserve literals. Got: {diags:#?}"
    );
}

#[test]
fn non_const_type_param_still_widens_object_literal_property() {
    // Negative case: without `const`, the literal property type must widen
    // (proves the fix is gated on `is_const`, not unconditional).
    let source = r#"
function f<T>(x: T, y: number): T { return x; }
const r = f({ a: 1 }, 2);
const _ra: 1 = r.a;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        has_diagnostic_code(&diags, 2322),
        "non-const T must still widen property literal to number. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_with_mutable_array_constraint_widens() {
    // Negative case: `const T extends unknown[]` (mutable array) should
    // widen because the constraint allows a mutable-array target. This
    // proves the (c) branch is gated on `constraint_allows_mutable_array_like`.
    let source = r#"
function f<const T extends unknown[]>(x: T): T { return x; }
const r = f([1, 2, 3]);
const _r: [1, 2, 3] = r;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        has_diagnostic_code(&diags, 2322),
        "mutable-array constraint must keep widening behavior. Got: {diags:#?}"
    );
}

// ─── Symbol-keyed property exclusion from string-index inference ─────────────

#[test]
fn object_values_with_symbol_keyed_intersection_no_false_ts2345() {
    // Regression: calling a function that expects T with a value inferred from
    // Object.values on a type that has both a unique-symbol property and a
    // string index signature must NOT include the symbol property value type
    // in the inferred T.
    //
    // Previously `true` was included in T from `{ [sym]?: true }`, causing
    // a false TS2345 where tsc emits none.
    //
    // Reproduces: unionTypeInference.ts repro from #32752
    let source = r#"
declare const sym: unique symbol;
type WithSym<T> = { [sym]?: true } & T;
declare function f<T>(x: WithSym<{ [s: string]: T }>): T;
declare const input: WithSym<{ [s: string]: string }>;
const result: string = f(input);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diags, &[2345, 2322]),
        "symbol property in intersection must not cause false type error. Got: {diags:#?}"
    );
}

// ─── Recursive homomorphic mapped type inference ─────────────────────────────

#[test]
fn recursive_homomorphic_mapped_against_self_referential_interface_no_unknown_property() {
    // Regression for `mappedTypeRecursiveInference.ts`.
    //
    // `Deep<T> = { [K in keyof T]: Deep<T[K]> }` applied to a self-referential
    // interface like `interface A { a: A }` must converge to a structural
    // candidate for T (so accesses `out.a`, `out.a.a` resolve to a real object
    // type, not `unknown`). Before the alias-cycle fix in
    // `reverse_infer_through_template`, every recursive expansion produced a
    // fresh mapped TypeId, so the per-template visited set never detected the
    // cycle and the depth cap was reached only after the instantiation depth
    // limit had already collapsed the template to `error`. The result was T =
    // `{ a: unknown }`, which raised a spurious TS18046 on `out.a.a`.
    let source = r#"
interface A { a: A }
declare let a: A;
type Deep<T> = { [K in keyof T]: Deep<T[K]> }
declare function foo<T>(deep: Deep<T>): T;
const out = foo(a);
out.a;
out.a.a;
out.a.a.a.a.a.a.a;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 18046),
        "recursive Deep<A> inference must not leave nested accesses as unknown. Got: {diags:#?}"
    );
    assert!(
        lacks_any_diagnostic_code(&diags, &[2345, 2322]),
        "recursive Deep<A> inference must not raise an assignability error for the self-referential source. Got: {diags:#?}"
    );
}

#[test]
fn recursive_homomorphic_mapped_against_index_signature_interface_no_unknown_property() {
    // Sibling to the named-property case: `interface B { [s: string]: B }`
    // reverse-maps via the index-signature path. Both paths must converge to a
    // structural candidate so `oub.b.a.n.a` is well-typed.
    let source = r#"
interface B { [s: string]: B }
declare let b: B;
type Deep<T> = { [K in keyof T]: Deep<T[K]> }
declare function foo<T>(deep: Deep<T>): T;
const oub = foo(b);
oub.b;
oub.b.b;
oub.b.a.n.a.n.a;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 18046),
        "recursive Deep<B> inference must not leave nested accesses as unknown. Got: {diags:#?}"
    );
    assert!(
        lacks_any_diagnostic_code(&diags, &[2345, 2322]),
        "recursive Deep<B> inference must not raise an assignability error for the self-referential source. Got: {diags:#?}"
    );
}

#[test]
fn recursive_homomorphic_mapped_with_nullable_property_lets_outer_check_reject_null() {
    // Companion case: when the recursively-inferred property has a `T1 | null`
    // source type, reverse inference falls back to `any` (not `unknown`) so
    // subsequent property accesses on the inferred T resolve without
    // TS18046, while the *outer* assignability check (e.g. against
    // `Deep<any>`) still rejects the `null` member and reports TS2345 for
    // the original `foo(...)` call, matching tsc's behaviour for
    // `XMLHttpRequest.responseXML: Document | null`.
    let source = r#"
type Deep<T> = { [K in keyof T]: Deep<T[K]> }
declare function foo<T>(deep: Deep<T>): T;
interface DocLike { url: string }
interface XLike {
    responseXML: DocLike | null;
}
declare let xhr: XLike;
const out = foo(xhr);
const ok = out.responseXML.url; // must NOT raise TS18046
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 18046),
        "Nullable property reverse inference must materialise as `any` so chained accesses are well-typed. Got: {diags:#?}"
    );
    assert!(
        has_diagnostic_code(&diags, 2345),
        "Outer Deep<...> assignability must still reject the `null` constituent. Got: {diags:#?}"
    );
}

#[test]
fn recursive_homomorphic_mapped_materializes_primitive_apparent_members() {
    // `mappedTypeRecursiveInference.ts` includes `XMLHttpRequest` primitive
    // properties such as `readyState: number` and `responseText: string`.
    // Reverse inference through `Deep<T>` must infer apparent primitive member
    // objects for those properties rather than collapsing them to `unknown`.
    // Nullable callback properties should still be uninformative (`unknown`),
    // unlike nullable object properties where we keep the existing `any`
    // approximation so chained property accesses do not raise TS18046.
    let source = r#"
type Deep<T> = { [K in keyof T]: Deep<T[K]> }
declare function foo<T>(deep: Deep<T>): T;
interface DocLike { url: string }
interface XLike {
    onreadystatechange: (() => void) | null;
    readonly readyState: number;
    readonly responseText: string;
    responseXML: DocLike | null;
}
declare let xhr: XLike;
const out = foo(xhr);
const ok = out.responseXML.url;
const readyShape: { toString: unknown } = out.readyState;
const textShape: { toString: unknown } = out.responseText;
const callbackNumber: number = out.onreadystatechange;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 18046),
        "Nullable object property should remain usable after recursive reverse inference. Got: {diags:#?}"
    );
    let ts2322_messages: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();
    assert!(
        ts2322_messages.len() == 1
            && ts2322_messages[0].contains("Type 'unknown' is not assignable to type 'number'"),
        "Only the nullable callback assignment should fail, proving it stayed `unknown` rather than `any`. Got: {diags:#?}"
    );
}

#[test]
fn recursive_homomorphic_mapped_with_nested_indexed_target_does_not_rewalk_target_param() {
    let source = r#"
type Deep<T> = { [K in keyof T]: Deep<T[K]> }
declare function foo<T>(deep: Deep<T>): T;
interface Payload {
    label: string;
    child: Payload;
}
interface XLike {
    response: Payload;
    responseText: string;
    readyState: number;
}
declare let xhr: XLike;
const out = foo(xhr);
const childLabel: string = out.response.child.label;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 18046),
        "nested Deep<T[K]> reverse inference must not leave chained accesses as unknown. Got: {diags:#?}"
    );
}

#[test]
fn recursive_homomorphic_mapped_against_builtin_xml_http_request_no_unknown_property() {
    // Mirrors TypeScript's `mappedTypeRecursiveInference.ts` DOM case.
    let source = r#"
interface A { a: A }
declare let a: A;
type Deep<T> = { [K in keyof T]: Deep<T[K]> }
declare function foo<T>(deep: Deep<T>): T;
const out = foo(a);
out.a;
out.a.a;
out.a.a.a.a.a.a.a;

interface B { [s: string]: B }
declare let b: B;
const oub = foo(b);
oub.b;
oub.b.b;
oub.b.a.n.a.n.a;

declare let xhr: XMLHttpRequest;
const out2 = foo(xhr);
out2.responseXML;
out2.responseXML.activeElement.className.length;
"#;
    let diags = relevant_default_lib_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 18046),
        "recursive Deep<XMLHttpRequest> inference must not leave DOM chained accesses as unknown. Got: {diags:#?}"
    );
    assert!(
        has_diagnostic_code(&diags, 2345),
        "recursive Deep<XMLHttpRequest> inference should still reject the nullable DOM callback property like tsc. Got: {diags:#?}"
    );
}

// ─── Higher-order function inference (HOFI) — tracks compiler/genericFunctionInference1.ts ─

/// Locks in the existing correct behavior: a generic source function with a
/// non-self-referential type-parameter constraint is accepted as the argument
/// of `pipe<A extends any[], B>(ab: (...args: A) => B)`. Inference should not
/// collapse the source's type parameter to `unknown` and reject the call.
///
/// `tsc` accepts each of the calls below; tsz currently does too. This test
/// exists to catch regressions if the inference path that handles non-recursive
/// constraints is reworked while addressing the recursive-constraint gap
/// captured by the `pipe_accepts_*_self_referential_*` ignored test below.
#[test]
fn pipe_accepts_generic_argument_with_simple_constraint() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function fooStr<T extends string>(x: T): T;
declare function fooNum<T extends number>(x: T): T;
declare function fooObj<T extends { other: number }>(x: T): T;
declare function fooBare<T>(x: T): T;

const a = pipe(fooStr);
const b = pipe(fooNum);
const c = pipe(fooObj);
const d = pipe(fooBare);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2345),
        "pipe(<T extends C>(x: T) => T) with non-self-referential C must not raise TS2345. Got: {diags:#?}"
    );
}

/// HOFI gap: a generic source function whose type-parameter constraint refers
/// back to the type parameter itself (`T extends { value: T }`) is rejected
/// when passed to `pipe<A extends any[], B>(ab: (...args: A) => B)`. tsc
/// accepts it and propagates the constraint into the result type
/// (`<T extends { value: T; }>(x: T) => T`).
///
/// Conformance test: `compiler/genericFunctionInference1.ts` (lines 20, 21,
/// 33, 34 — the recursive-constraint subset of the eight extra TS2345
/// diagnostics).
#[test]
fn pipe_accepts_generic_argument_with_self_referential_constraint() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function fooSelf<T extends { value: T }>(x: T): T;

const f = pipe(fooSelf);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2345),
        "pipe(<T extends {{ value: T }}>(x: T) => T) must not raise TS2345 once HOFI is implemented. Got: {diags:#?}"
    );
}

#[test]
fn type_literal_generic_method_retains_method_type_params_for_call_inference() {
    let source = r#"
type Matcher<T> = {
    with<P, R>(pattern: P, handler: (value: T) => R): Matcher<T>;
};

declare function match<T>(value: T): Matcher<T>;
declare function oneOf<T>(left: T, right: T): T;
declare const item: { kind: "issue"; priority: "low" | "medium" | "high" };

match(item).with({ kind: "issue", priority: oneOf("medium", "high") }, () => true);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2345),
        "generic methods declared in type literals must retain method type params for call inference. Got: {diags:#?}"
    );
}

// ─── Variadic tuple spread with type-assertion arguments ─────────────────────

#[test]
fn variadic_tuple_spread_type_assertion_preserves_literals() {
    let source = r#"
declare function concat<T extends readonly unknown[], U extends readonly unknown[]>(a: T, b: U): [...T, ...U];
const result = concat([1, 2] as [1, 2], ["a", "b"] as ["a", "b"]);
const _r: [1, 2, "a", "b"] = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "variadic tuple spread with type-asserted args must preserve literal types. Got: {diags:#?}"
    );
}

#[test]
fn variadic_tuple_spread_type_assertion_preserves_literals_renamed_params() {
    let source = r#"
declare function concat<K extends readonly unknown[], V extends readonly unknown[]>(a: K, b: V): [...K, ...V];
const result = concat([true, false] as [true, false], [42] as [42]);
const _r: [true, false, 42] = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "variadic tuple spread literal preservation must not depend on parameter names K/V. Got: {diags:#?}"
    );
}

#[test]
fn variadic_tuple_spread_without_assertion_widens_to_primitives() {
    let source = r#"
declare function concat<T extends readonly unknown[], U extends readonly unknown[]>(a: T, b: U): [...T, ...U];
const result = concat([1, 2], ["a", "b"]);
const _bad: [1, 2, "a", "b"] = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        has_diagnostic_code(&diags, 2322),
        "variadic tuple spread from fresh (non-asserted) tuple must widen literals. Got: {diags:#?}"
    );
}

#[test]
fn variadic_tuple_spread_three_way_with_assertions_preserves_literals() {
    let source = r#"
declare function concat3<A extends readonly unknown[], B extends readonly unknown[], C extends readonly unknown[]>(
    a: A, b: B, c: C
): [...A, ...B, ...C];
const r = concat3([1] as [1], ["x"] as ["x"], [true] as [true]);
const _check: [1, "x", true] = r;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "three-way variadic spread with asserted tuples must preserve all literals. Got: {diags:#?}"
    );
}

#[test]
fn conditional_type_parameter_default_evaluates_after_prior_arg_known() {
    let source = r#"
type Wrapper<T, W = T extends string ? number : boolean> = {
  value: T;
  wrapped: W;
};

type WrapStr = Wrapper<string>;
const ws: WrapStr = { value: "hello", wrapped: 42 };
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "conditional default depending on earlier known type parameter must evaluate. Got: {diags:#?}"
    );
}

// ─── Template literal type parameter inference (issue #6147) ─────────────────

/// f(x: prefix-T) where T extends string — call with matching literal should infer T.
#[test]
fn template_literal_infers_type_param_trailing_span() {
    let source = r#"
declare function f<T extends string>(x: `prefix-${T}`): T;
const result = f("prefix-hello");
const _check: "hello" = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "T should be inferred as \"hello\" from template literal argument. Got: {diags:#?}"
    );
}

/// Same rule with a renamed type parameter (`K`) to confirm no identifier is hardcoded.
#[test]
fn template_literal_infers_type_param_renamed() {
    let source = r#"
declare function get<K extends string>(x: `get-${K}`): K;
const result = get("get-name");
const _check: "name" = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "K should be inferred as \"name\" from template literal argument. Got: {diags:#?}"
    );
}

/// f(x: pre-T-suf) where T extends string — T is surrounded by text anchors.
#[test]
fn template_literal_infers_type_param_prefix_and_suffix() {
    let source = r#"
declare function f<T extends string>(x: `pre-${T}-suf`): T;
const result = f("pre-mid-suf");
const _check: "mid" = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "T should be inferred as \"mid\" from surrounded template. Got: {diags:#?}"
    );
}

/// f(x: T-U) where T and U extend string — two type params inferred from a separator-delimited literal.
#[test]
fn template_literal_infers_multiple_type_params() {
    let source = r#"
declare function f<T extends string, U extends string>(x: `${T}-${U}`): [T, U];
const result = f("hello-world");
const _check: ["hello", "world"] = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "T and U should be inferred from two-param template. Got: {diags:#?}"
    );
}

/// Same two-param rule using different names (`A`, `B`) to confirm generality.
#[test]
fn template_literal_infers_multiple_type_params_renamed() {
    let source = r#"
declare function split<A extends string, B extends string>(x: `${A}/${B}`): [A, B];
const result = split("foo/bar");
const _check: ["foo", "bar"] = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "A and B should be inferred from split template. Got: {diags:#?}"
    );
}

/// When the argument does not match the template pattern, a TS2345 error is expected.
#[test]
fn template_literal_type_param_mismatch_errors() {
    let source = r#"
declare function f<T extends string>(x: `prefix-${T}`): T;
f("wrong");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        has_diagnostic_code(&diags, 2345),
        "passing a non-matching literal should raise TS2345. Got: {diags:#?}"
    );
}

/// f(x: T-suffix) where T extends string — T is a leading span with a fixed suffix.
#[test]
fn template_literal_type_param_leading_span() {
    let source = r#"
declare function f<T extends string>(x: `${T}-suffix`): T;
const result = f("hello-suffix");
const _check: "hello" = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "T should be inferred as \"hello\" from leading template span. Got: {diags:#?}"
    );
}

// Nullish in T|null: nullish source provides no info about T (conformance: inferenceFromParameterlessLambda)

#[test]
fn nullish_in_t_or_null_param_does_not_infer_t_null() {
    let source = r#"
function withDefault<T>(value: T | null, f: () => T): T { return value ?? f(); }
const r1 = withDefault(null, () => 42);
const r2 = withDefault(null, () => "hello");
const r3 = withDefault(null, () => true);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "null in T|null should not yield T=null; T should come from callback return. Got: {diags:#?}"
    );
}

#[test]
fn nullish_undefined_in_t_or_undefined_param_does_not_infer_t() {
    let source = r#"
function maybe<T>(x: T | undefined, f: () => T): T { return x ?? f(); }
const r1 = maybe(undefined, () => 99);
const r2 = maybe(undefined, () => "hi");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "undefined in T|undefined should not constrain T; callback return supplies T. Got: {diags:#?}"
    );
}

#[test]
fn cross_nullish_source_still_infers_into_t() {
    let source = r#"
declare function f<T>(x: T | null): T;
const a = f(undefined);
const checkA: undefined = a;

declare function g<T>(x: T | undefined): T;
const b = g(null);
const checkB: null = b;
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.is_empty(),
        "opposite nullish source should infer into T, not be explained by the other nullish arm. Got: {diags:#?}"
    );
}

#[test]
fn non_nullish_in_t_or_null_still_constrains_t() {
    let source = r#"
function withDefault<T>(value: T | null, f: () => T): T { return value ?? f(); }
const r1 = withDefault(42, () => 0);
const r2 = withDefault("x", () => "y");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "non-null concrete arg should not produce false positives. Got: {diags:#?}"
    );
}

#[test]
fn nullish_in_t_or_null_with_nullable_union_source_still_constrains_t() {
    let source = r#"
function withDefault<T>(value: T | null, f: () => T): T { return value ?? f(); }
declare const maybeStr: string | null;
const r = withDefault(maybeStr, () => "fallback");
const _check: string = r;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "string|null source in T|null should still infer T=string. Got: {diags:#?}"
    );
}

#[test]
fn nullish_in_array_wrapped_t_or_null_does_not_infer_t() {
    let source = r#"
function orNull<T>(x: T[] | null, factory: () => T[]): T[] { return x ?? factory(); }
const r = orNull(null, () => [42]);
const _check: number[] = r;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "null in T[]|null should not infer T=null; callback supplies T[]=number[]. Got: {diags:#?}"
    );
}

#[test]
fn real_type_mismatch_still_errors_with_nullable_union_inference() {
    let source = r#"
function bar<T>(x: T, f: () => T): T { return f(); }
bar("hi", () => 42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.is_empty(),
        "type mismatch (string vs number for T) should still produce an error. Got: {diags:#?}"
    );
}

// ─── Issue #9714: Object.values literal preservation/widening ────────────────

/// Regression: `Object.values` over an `as const`-flagged source must preserve
/// the property literal types in the inferred element type.
///
/// Structural rule: when inference resolves T from an index signature
/// (`{ [s: string]: T }`) under a `priority_implies_combination` candidate
/// set, the result is `getUnionType(candidates, UnionReduction.Subtype)` —
/// a subtype-reduced union of the candidate types, NOT
/// `getCommonSupertype`/`best_common_type`. The latter collapses same-base
/// literal candidates (`1`, `2`) to their primitive (`number`), which is
/// only legal after the later `getWidenedType` step (gated on candidate
/// freshness).
#[test]
fn object_values_with_as_const_property_preserves_literal_element_type() {
    // Two name choices for the iteration var equivalent shape so a hardcoded
    // string match on `a`/`b` would break the test.
    let preserved_a_b = r#"
const o = { a: 1 as const, b: 2 as const };
const v = Object.values(o);
const probe: (1 | 2)[] = v;
"#;
    let preserved_x_y = r#"
const o = { x: "hi" as const, y: "lo" as const };
const v = Object.values(o);
const probe: ("hi" | "lo")[] = v;
"#;
    for source in [preserved_a_b, preserved_x_y] {
        let diags = relevant_default_lib_diagnostics(source);
        assert!(
            lacks_any_diagnostic_code(&diags, &[2322, 2345]),
            "Object.values must preserve `as const` property literals in inferred element. Got: {diags:#?}"
        );
    }
}

/// Regression: `Object.values` over an outer-`as const` source must preserve
/// the property literal types in the inferred element type, mirroring the
/// per-property `as const` case.
#[test]
fn object_values_with_whole_as_const_preserves_literal_element_type() {
    let source = r#"
const o = { a: 1, b: 2 } as const;
const v = Object.values(o);
const probe: readonly (1 | 2)[] = v;
const probe_mutable: (1 | 2)[] = v;
"#;
    let diags = relevant_default_lib_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diags, &[2322, 2345]),
        "Object.values on `{{...}} as const` must preserve property literals. Got: {diags:#?}"
    );
}

/// Negative control: `Object.values` over a plain fresh object literal widens
/// each property to its primitive (matching tsc's `getWidenedLiteralType` for
/// fresh literal candidates). The element type must therefore be
/// `(string | number)`, not the literal union `(1 | "x")`.
#[test]
fn object_values_with_fresh_object_literal_widens_element_type() {
    let source = r#"
const o = { a: 1, b: "x" };
const v = Object.values(o);
const probe: (string | number)[] = v;
"#;
    let diags = relevant_default_lib_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diags, &[2322, 2345]),
        "Object.values on a fresh object literal must widen literal element types. Got: {diags:#?}"
    );

    // Negative: assigning to the literal-preserving type should now fail
    // (would have been accepted before the fix when literals leaked through).
    let source_strict = r#"
const o = { a: 1, b: "x" };
const v = Object.values(o);
const probe: (1 | "x")[] = v;
"#;
    let diags = relevant_default_lib_diagnostics(source_strict);
    assert!(
        has_any_diagnostic_code(&diags, &[2322, 2345]),
        "fresh-literal Object.values output must NOT be assignable to a literal-preserving target. Got: {diags:#?}"
    );
}

/// Negative control: explicit `{ a: number; b: number }` source stays as
/// `number[]` (no change). Guards against an over-broad fix that would force
/// every Object.values invocation onto a literal-preserving path.
#[test]
fn object_values_with_explicit_primitive_property_type_stays_primitive() {
    let source = r#"
const o: { a: number; b: number } = { a: 1, b: 2 };
const v = Object.values(o);
const probe: number[] = v;
"#;
    let diags = relevant_default_lib_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diags, &[2322, 2345]),
        "Object.values on an explicitly-typed primitive object stays primitive. Got: {diags:#?}"
    );
}

// -----------------------------------------------------------------------------
// Co/contra-variant inference: covariant result must be widened before the
// contra-candidate assignability check.
//
// When T appears in a return-type position (`() => T`) its candidate carries
// ReturnType priority and skip_literal_widening is true, so the fresh object
// literal `{a:1, b:2}` is NOT widened inside resolve_from_candidates.
// tsc calls getWidenedType(covariantInference) BEFORE testing assignability
// to the contra-candidate – tsz must do the same so that fresh object literals
// like `{a:1,b:2}` pass structural checking against `{a:number}`.
// -----------------------------------------------------------------------------

#[test]
fn co_contra_fresh_object_widened_before_assignability_check() {
    // T appears in: produce: () => T  (covariant, ReturnType priority)
    //               consume: (t: T) => void  (contra)
    // produce() returns a fresh object literal {a:1, b:2}.
    // After widening: {a:number, b:number} IS assignable to {a:number}.
    // Covariant result should win; r.b should be valid.
    let source = r#"
declare function bar<T>(produce: () => T, consume: (t: T) => void): T;
const r = bar(() => ({ a: 1, b: 2 }), (t: { a: number }) => {});
const _b: number = r.b;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "fresh object literal covariant result should win after widening; r.b must be valid. Got: {diags:#?}"
    );
}

#[test]
fn co_contra_fresh_object_widened_different_type_param_names() {
    // Same structural rule with renamed type param (U instead of T).
    let source = r#"
declare function bar<U>(produce: () => U, consume: (u: U) => void): U;
const r = bar(() => ({ x: 1, y: 2 }), (u: { x: number }) => {});
const _y: number = r.y;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "type param name change should not affect widening behavior. Got: {diags:#?}"
    );
}

#[test]
fn co_contra_contra_wins_when_covariant_not_assignable() {
    // When covariant result (string) is NOT assignable to contra (number),
    // the contra-candidate should win and produce an error at the use site.
    let source = r#"
declare function bar<T>(produce: () => T, consume: (t: T) => void): T;
bar(() => "hello", (t: number) => {});
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        has_any_diagnostic_code(&diags, &[2345, 2322]),
        "string is not assignable to number; contra should win and a TS2345/TS2322 error should appear. Got: {diags:#?}"
    );
}

#[test]
fn co_contra_multiple_contra_resolved_via_intersection() {
    // Two contra-candidates → intersection.
    // produce returns {x:1, y:2}; consume1 expects {x:number}, consume2 expects {y:number}.
    // Widened covariant {x:number,y:number} IS assignable to both, so it wins.
    let source = r#"
declare function combine<T>(
    produce: () => T,
    consume1: (t: T) => void,
    consume2: (t: T) => void,
): T;
const r = combine(
    () => ({ x: 1, y: 2 }),
    (t: { x: number }) => {},
    (t: { y: number }) => {},
);
const _x: number = r.x;
const _y: number = r.y;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "widened covariant {{x:number,y:number}} should win against both contra-candidates. Got: {diags:#?}"
    );
}

#[test]
fn co_contra_aliased_shape_covariant_wins_after_widen() {
    // T via an alias; structural rule should be the same.
    let source = r#"
type HasA = { a: number };
declare function bar<T>(produce: () => T, consume: (t: T) => void): T;
const r = bar(() => ({ a: 1, b: 2 }), (t: HasA) => {});
const _b: number = r.b;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "aliased contra shape should not prevent covariant win after widening. Got: {diags:#?}"
    );
}

#[test]
fn co_contra_never_contra_does_not_override_covariant_literal() {
    let source = r#"
type A = { kind: "a" };
type B = { kind: "b" };
declare const a: A;
declare const b: B;
declare function fab(arg: A | B): void;
declare function foo<T>(x: { kind: T }, f: (arg: { kind: T }) => void): void;
foo(a, fab);
foo(b, fab);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "contra evidence that collapses to never must not override useful covariant literals. Got: {diags:#?}"
    );
}

#[test]
fn co_contra_primitive_literal_probe_preserves_union_assignability() {
    let source = r#"
declare const branch:
  <T, U extends T>(_: { test: T, if: (t: T) => t is U, then: (u: U) => void }) => void;
declare const x: "a" | "b";
branch({
  test: x,
  if: (t): t is "a" => t === "a",
  then: u => {
    let test1: "a" = u;
  }
});
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "primitive literal covariant evidence should be probed as the literal, not widened past its union constraint. Got: {diags:#?}"
    );
}

#[test]
fn contextual_generic_builder_chain_preserves_indexed_selection() {
    // Kysely-style builder reduction from #8773: each chained method call must
    // keep the selected schema/table instantiation while indexing into `S[K]`.
    let source = r#"
type Schema = {
    user: { id: number; name: string };
    post: { id: number; userId: number };
};

declare function build<S, K extends keyof S>(
    key: K,
): {
    select<P extends keyof S[K]>(prop: P): S[K][P];
};

const userId = build<Schema, "user">("user").select("id");
const userName = build<Schema, "user">("user").select("name");
const postUserId = build<Schema, "post">("post").select("userId");

const _userId: number = userId;
const _userName: string = userName;
const _postUserId: number = postUserId;
const bad: string = postUserId;
"#;
    let diags = relevant_diagnostics(source);
    assert_eq!(
        diagnostic_count(&diags, 2322),
        1,
        "post.userId should be number, so only the final string assignment should fail. Got: {diags:#?}"
    );
    assert!(
        lacks_any_diagnostic_code(&diags, &[2339, 2344, 2345, 7006]),
        "builder-chain contextual instantiation should not lose table keys or callback context. Got: {diags:#?}"
    );
}
