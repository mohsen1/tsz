//! Tests for reverse mapped type inference through union and index signature types.
//!
//! When a generic function like `unboxify<T>(obj: Boxified<T>): T` is called
//! with an object whose properties have union types (e.g., Box<number> | Box<string>),
//! the reverse inference must distribute over union members to correctly infer T.
//!
//! Similarly, when the source object has index signatures (dictionary types),
//! the reverse inference must reverse through the index signature value type.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn check_and_get_codes(code: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_source_codes(code)
}

fn check_source_diagnostics(code: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    tsz_checker::test_utils::check_source_diagnostics(code)
}

#[test]
fn reverse_mapped_union_property_no_false_ts2339() {
    // When properties have union types like Box<number> | Box<string> | Box<boolean>,
    // reverse inference through Box<T[P]> should distribute over the union
    // and produce T = { a: number | string | boolean, ... }.
    let code = r#"
type Box<T> = { value: T; }
type Boxified<T> = { [P in keyof T]: Box<T[P]>; }
function unboxify<T extends object>(obj: Boxified<T>): T {
    return {} as T;
}
function makeRecord<T, K extends string>(obj: { [P in K]: T }) {
    return obj;
}
function box<T>(x: T): Box<T> { return { value: x }; }
function f5() {
    let b = makeRecord({
        a: box(42),
        b: box("hello"),
        c: box(true)
    });
    let v = unboxify(b);
    let x: string | number | boolean = v.a;
}
"#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339 for property access on reverse-inferred type, got: {codes:?}"
    );
}

#[test]
fn reverse_mapped_index_signature_no_false_ts7053() {
    // When the source has a string index signature (dictionary type),
    // reverse inference should reverse through the template for the index
    // signature value type, producing T with a string index signature.
    let code = r#"
type Box<T> = { value: T; }
type Boxified<T> = { [P in keyof T]: Box<T[P]>; }
function unboxify<T extends object>(obj: Boxified<T>): T {
    return {} as T;
}
function makeDictionary<T>(obj: { [x: string]: T }) {
    return obj;
}
function box<T>(x: T): Box<T> { return { value: x }; }
function f6(s: string) {
    let b = makeDictionary({
        a: box(42),
        b: box("hello"),
        c: box(true)
    });
    let v = unboxify(b);
    let x: string | number | boolean = v[s];
}
"#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&7053),
        "Expected no TS7053 for index access on reverse-inferred dictionary type, got: {codes:?}"
    );
}

#[test]
fn reverse_mapped_simple_box_properties() {
    // The basic homomorphic mapped type inference should still work:
    // { a: Box<number>, b: Box<string> } through Boxified<T> → T = { a: number, b: string }
    let code = r#"
type Box<T> = { value: T; }
type Boxified<T> = { [P in keyof T]: Box<T[P]>; }
function unboxify<T extends object>(obj: Boxified<T>): T {
    return {} as T;
}
function box<T>(x: T): Box<T> { return { value: x }; }
function test() {
    let b = {
        a: box(42),
        b: box("hello"),
        c: box(true)
    };
    let v = unboxify(b);
    let x: number = v.a;
}
"#;
    let codes = check_and_get_codes(code);
    assert!(!codes.contains(&2339), "Expected no TS2339, got: {codes:?}");
    assert!(!codes.contains(&2322), "Expected no TS2322, got: {codes:?}");
}

#[test]
fn reverse_mapped_union_template_definition_pattern() {
    // When the mapped type template is a union like `(() => T[K]) | Definition<T[K]>`,
    // reverse inference should try each union member. For `() => number` as source,
    // the function member `() => T[K]` should reverse to T[K] = number.
    let code = r#"
type Schema = Record<string, unknown> | readonly unknown[];
type Definition<T> = {
  [K in keyof T]: (() => T[K]) | Definition<T[K]>;
};
declare function create<T extends Schema>(definition: Definition<T>): T;
const created = create({
  a: () => 1,
  b: [() => ""],
});
"#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2345),
        "Expected no TS2345 for union template reverse inference, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for union template reverse inference, got: {codes:?}"
    );
}

#[test]
fn reverse_mapped_object_template_nested_properties() {
    // When the mapped type template is an object like `{ items: Wrap<T[K]> }`,
    // reverse inference should recurse through matching properties to find the
    // target placeholder.
    let code = r#"
type Wrap<T extends string[]> = { [K in keyof T]: T[K]; };
declare function test<T extends Record<string, string[]>>(obj: {
  [K in keyof T]: { items: Wrap<T[K]>; };
}): T;
const result = test({
  x: { items: ["a", "b"] },
  y: { items: ["c"] },
});
"#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2345),
        "Expected no TS2345 for object template reverse inference, got: {codes:?}"
    );
}

#[test]
fn reverse_mapped_reducer_pattern_no_false_ts2322() {
    // Repro from reverseMappedTypeInferenceSameSource1.ts:
    // When a generic function accepts `ReducersMapObject<S, A>` which is
    // `{ [K in keyof S]: Reducer<S[K], A> }`, inference should reverse
    // `Reducer<number>` → `S[K] = number` → `S = { counter1: number }`.
    let code = r#"
type Action<T extends string = string> = { type: T };
interface UnknownAction extends Action { [extraProps: string]: unknown }
type Reducer<S = any, A extends Action = UnknownAction> = (
  state: S | undefined,
  action: A,
) => S;

type ReducersMapObject<S = any, A extends Action = UnknownAction> = {
  [K in keyof S]: Reducer<S[K], A>;
};

interface ConfigureStoreOptions<S = any, A extends Action = UnknownAction> {
  reducer: Reducer<S, A> | ReducersMapObject<S, A>;
}

declare function configureStore<S = any, A extends Action = UnknownAction>(
  options: ConfigureStoreOptions<S, A>,
): void;

const counterReducer1: Reducer<number> = () => 0;
const store2 = configureStore({
  reducer: {
    counter1: counterReducer1,
  },
});
"#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for reducer-pattern reverse mapped inference, got: {codes:?}"
    );
}

#[test]
fn reverse_mapped_recursive_with_index_signature() {
    // Regression test: recursive mapped type `Deep<T> = { [K in keyof T]: Deep<T[K]> }`
    // inferred against an interface with only an index signature `{ [s: string]: B }`
    // should produce T = B (coinductively), NOT T = unknown.
    //
    // Before the fix, reverse_infer_through_template's Case 6 (Mapped template)
    // only iterated named properties, skipping index signatures entirely.
    // For `interface B { [s: string]: B }` with no named properties, the
    // reversal loop never ran, producing no candidate for T → T = unknown.
    let code = r#"
interface B { [s: string]: B }
declare let b: B;
type Deep<T> = { [K in keyof T]: Deep<T[K]> }
declare function foo<T>(deep: Deep<T>): T;
const oub = foo(b);
oub.b;
"#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&18046),
        "Expected no TS18046 ('is of type unknown') for index-signature reverse mapped inference, got: {codes:?}"
    );
}

#[test]
fn reverse_mapped_recursive_any_property_does_not_collapse_t_to_any() {
    // Regression test for mappedTypeRecursiveInference.ts:
    // when a homomorphic mapped type `Deep<T> = { [K in keyof T]: Deep<T[K]> }`
    // is inferred against an object source whose property has type `any`,
    // T must be inferred as the structural reverse-mapped object
    // (e.g. `{ p: unknown }`), NOT as `T = any`.
    //
    // Before the fix, reverse-mapped inference produced the structural
    // candidate `{ p: unknown }` at HomomorphicMappedType priority, but the
    // post-reverse `constrain_template_against_properties` then ran
    // `constrain_types(any, Deep<T[K]>)` which propagated `any` to T via the
    // `T[K]` placeholder at the OUTER call's higher priority — overriding
    // the reverse-mapped result and collapsing T to `any`. The downstream
    // assignability check then failed with the wrong inferred parameter
    // (`Deep<any>` instead of `Deep<{ p: unknown }>`).
    //
    // The fix excludes the homomorphic type parameter from the var map
    // when running the residual property-template inference: tsc treats
    // reverse inference as the sole inference path for the homomorphic
    // parameter, and we now mirror that.
    let code = r#"
type Deep<T> = { [K in keyof T]: Deep<T[K]> }
declare function foo<T>(deep: Deep<T>): T;
interface XHR { readonly p: any; }
declare let xhr: XHR;
const out = foo(xhr);
type _Probe = typeof out;
const probe: { p: unknown } = out;
"#;
    // The `probe` annotation forces tsz to compare `T` (typeof out) against
    // `{ p: unknown }`. If the fix is correct, T resolves to `{ p: unknown }`
    // and there are no diagnostics. If T collapses to `any`, the structural
    // check would still permit the assignment (any is assignable everywhere),
    // so the strongest signal is that `foo(xhr)` does NOT produce a TS2345
    // about `Deep<any>` and that no `Index signature for type 'string'`
    // diagnostic is emitted (which is the symptom of T collapsing to any
    // for sources without a string index).
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2345),
        "Expected no TS2345 (T should be structural `{{ p: unknown }}`, not `any` which would force the parameter to `Deep<any>` and break assignability), got: {codes:?}",
    );
}

#[test]
fn reverse_mapped_recursive_any_property_with_extra_props_keeps_structural_t() {
    // Companion test: even when the source has multiple properties — some
    // `any`-typed and some not — the homomorphic parameter T must remain a
    // structural object derived from reverse mapping, not collapse to `any`.
    //
    // The inferred `T` should remain structural rather than collapsing to
    // `any`. Primitive properties may materialize as apparent primitive member
    // objects, but accessing/calling through them must still not silently
    // succeed as `any`.
    let code = r#"
type Deep<T> = { [K in keyof T]: Deep<T[K]> }
declare function foo<T>(deep: Deep<T>): T;
interface XHR { readonly p: any; readonly q: number; }
declare let xhr: XHR;
const out = foo(xhr);
out.q.toFixed(2);
"#;
    let codes = check_and_get_codes(code);
    // Calling through `out.q.toFixed` should remain an error. If T had collapsed
    // to `any`, `out.q` would be `any` and the call would silently succeed.
    assert!(
        codes.contains(&18046) || codes.contains(&2349),
        "Expected an error on `out.q.toFixed(...)`, proving structural reverse-mapped inference did not collapse T to any; got: {codes:?}",
    );
}

#[test]
fn reverse_mapped_preserves_source_property_declaration_order_in_ts2353() {
    // Regression test for reverseMappedTypeLimitedConstraint.ts:
    // the excess-property (TS2353) diagnostic must render the inferred
    // target type with properties in the *source argument's declaration
    // order*, not in atom-id order.
    //
    // tsc baseline: `{ x: number; y: "y"; }`
    // tsz pre-fix:  `{ y: "y"; x: number; }` (atom-id order leaking through
    //               `constrain_reverse_mapped_type` dropping declaration_order)
    //
    // The `checked_` call below triggers reverse-mapped inference for U
    // through `{ [K in keyof U & keyof T]: U[K] }` with T = {x: number, y: string}
    // and the argument adding an excess `z` property.
    let code = r#"
const checkType_ = <T>() => <U extends T>(value: { [K in keyof U & keyof T]: U[K] }) => value;
const checked_ = checkType_<{x: number, y: string}>()({
  x: 1 as number,
  y: "y",
  z: "z",
});
"#;
    let mut parser = ParserState::new("test.ts".to_string(), code.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let ts2353: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2353)
        .collect();
    assert!(
        !ts2353.is_empty(),
        "expected TS2353 excess-property diagnostic for `z`, got: {:#?}",
        checker.ctx.diagnostics
    );
    let msg = ts2353[0].message_text.as_str();
    // The target type must render `x` before `y` — matching the argument
    // literal's declaration order (which flows through U's inferred shape).
    let x_pos = msg
        .find("x:")
        .expect("x: must appear in target type display");
    let y_pos = msg
        .find("y:")
        .expect("y: must appear in target type display");
    assert!(
        x_pos < y_pos,
        "expected `x` to appear before `y` in TS2353 target type; got message: {msg}"
    );
    assert!(
        msg.contains("{ x: number; y: \"y\"; }"),
        "expected target type to match tsc baseline `{{ x: number; y: \"y\"; }}`, got message: {msg}"
    );
}

#[test]
#[ignore = "const-generic TS2353 display requires literal TConfig inference, not upper-bound substitution — tracked as known gap"]
fn reverse_mapped_const_generic_ts2353_omits_outer_readonly_in_target_display() {
    // `const` type-parameter inference records readonly flags on the captured
    // object literal, but tsc's TS2353 target display omits those flags at the
    // outer excess-property target while preserving nested readonly literals.
    let code = r#"
interface ProvidedActor {
  src: string;
  logic: () => Promise<unknown>;
}
type DistributeActors<TActor> = TActor extends { src: infer TSrc } ? { src: TSrc } : never;
interface MachineConfig<TActor extends ProvidedActor> {
  invoke: DistributeActors<TActor>;
}
declare function createXMachine<
  const TConfig extends MachineConfig<TActor>,
  TActor extends ProvidedActor = ProvidedActor,
>(config: {[K in keyof MachineConfig<any> & keyof TConfig]: TConfig[K] }): TConfig;

createXMachine({
  invoke: {
    src: "whatever",
  },
  extra: 10
});
"#;
    let mut parser = ParserState::new("test.ts".to_string(), code.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    let mut checker = crate::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );
    checker.check_source_file(root);

    let ts2353 = checker
        .ctx
        .diagnostics
        .iter()
        .find(|diag| diag.code == 2353)
        .expect("expected TS2353 for excess property");
    let msg = ts2353.message_text.as_str();
    assert!(
        msg.contains("{ invoke: { readonly src: \"whatever\"; }; }"),
        "expected outer target display to omit readonly while preserving nested readonly; got: {msg}"
    );
    assert!(
        !msg.contains("{ readonly invoke:"),
        "outer excess-property target must not print readonly; got: {msg}"
    );
}

#[test]
fn reverse_mapped_finite_recursive_alias_drills_to_leaf() {
    // Regression test for reverseMappedTypeDeepDeclarationEmit.ts.
    //
    // When a homomorphic mapped type's template recursively references itself
    // via a type alias indirection (here through `Validator<T>`), reverse
    // inference must continue drilling into nested object sources until a
    // non-recursive leaf (e.g. a function type) is reached. The previous
    // recursion guard short-circuited on ANY re-entry of Case 6, returning
    // the source unchanged and producing
    //   `V = { Test: { Test1: { Test2: NativeTypeValidator<string> } } }`
    // when the correct inferred T is
    //   `V = { Test: { Test1: { Test2: string } } }`.
    //
    // The fix tracks `(template, source_value)` pairs in the recursion chain
    // and only converges to the source when the SAME pair re-occurs (true
    // structural recursion like `interface A { a: A }`). Distinct sub-objects
    // are still allowed to recurse, so finite sources reverse-map all the way
    // down. We assert the correct inference by writing a `string`-typed leaf
    // back into the inferred shape: if T collapsed early to
    // `NativeTypeValidator<string>`, the assignment would emit TS2322.
    let code = r#"
type Validator<T> = NativeTypeValidator<T> | ObjectValidator<T>
type NativeTypeValidator<T> = (n: any) => T | undefined
type ObjectValidator<O> = {
  [K in keyof O]: Validator<O[K]>
}
declare const SimpleStringValidator: NativeTypeValidator<string>;
declare const ObjValidator: <V>(validatorObj: ObjectValidator<V>) => (o: any) => V;
const test = {
  Test: {
    Test1: {
      Test2: SimpleStringValidator
    },
  }
}
const validatorFunc = ObjValidator(test);
const outputExample: { Test: { Test1: { Test2: string } } } = validatorFunc({
  Test: {
    Test1: {
      Test2: "hi"
    },
  }
});
"#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 (V should reverse-infer all the way to leaf `string`, \
         not stop at `NativeTypeValidator<string>`), got: {codes:?}"
    );
}

#[test]
fn reverse_mapped_tuple_context_application_contextual_type() {
    // reverseMappedTupleContext.ts: array literals passed to functions whose
    // parameter is a generic type alias application (Definition<Schema>) should also
    // be inferred as tuples, even though the contextual type is an Application, not
    // a bare Mapped type.
    let code = r#"
type Func<T> = () => T;
type Definition<T extends unknown[]> = { [K in keyof T]: Func<T[K]> };

declare function create<T extends unknown[]>(schema: Definition<T>): T;

const result = create([() => 1, [() => "hello"]]);
const x: number = result[0];
"#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2345),
        "Expected no TS2345: array literal should be inferred as tuple when contextual type is a homomorphic Application, got: {codes:?}"
    );
}

#[test]
fn homomorphic_mapped_rest_tuple_context_preserves_element_types() {
    // homomorphicMappedTypeWithNonHomomorphicInstantiationSpreadable1.ts:
    // `[...HandleOptions<T[K]>]` is an all-rest tuple context whose rest element
    // is a homomorphic mapped type application. It still needs tuple inference
    // so reverse mapped inference can recover a distinct T[K] per array element.
    let code = r#"
type HandleOptions<O> = {
    [I in keyof O]: {
        value: O[I];
    };
};

declare function func1<
    T extends Record<string, readonly any[]>,
>(fields: {
    [K in keyof T]: {
        label: string;
        options: [...HandleOptions<T[K]>];
    };
}): T;

const result = func1({
    prop: {
        label: "first",
        options: [
            { value: 123 },
            { value: "foo" },
        ],
    },
    other: {
        label: "second",
        options: [
            { value: "bar" },
            { value: true },
        ],
    },
});
"#;
    let diagnostics = check_source_diagnostics(code);
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for homomorphic mapped rest tuple inference, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.start, d.length, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn reverse_mapped_tuple_context_no_false_positive_for_non_homomorphic() {
    // Non-homomorphic mapped types (where the constraint is not `keyof T`) should
    // NOT force tuple inference — the array literal should remain an array.
    let code = r#"
declare function process<T>(items: { [K in 'a' | 'b']: T }): T;
// This should not cause errors — non-homomorphic mapped types don't trigger tuple forcing.
const obj = process({ a: 1, b: 2 });
"#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2322) && !codes.contains(&2345),
        "Expected no type errors for non-homomorphic mapped type context, got: {codes:?}"
    );
}

// ============================================================================
// Intersection-constrained mapped type tests
// ============================================================================
// Structural rule: when a mapped type has constraint `keyof T & keyof C` (where T
// is a type parameter and C is a concrete limit type), the mapped type iterates
// only over keys present in BOTH T and C. This means:
// 1. `{ [K in keyof T & keyof C]: T[K] }` is NOT assignable to T (it's a narrowed
//    projection, not T itself), and should show the full type in the TS2322 message.
// 2. Excess properties beyond C's keys are detected on the inferred T.
// 3. The pattern generalises across renamed type parameters (K/P/Q).

#[test]
fn intersection_constraint_mapped_not_assignable_to_type_param() {
    // `{ [K in keyof T & keyof Limit]: T[K] }` is NOT assignable to T.
    // tsc reports TS2322 with the full mapped type in the message, not the limit type.
    let code = r#"
type Limit = { x: number; y: string };
declare function g<T>(obj: { [K in keyof T & keyof Limit]: T[K] }): T;
function returnsLimitedMap<T>(limited: { [K in keyof T & keyof Limit]: T[K] }): T {
    return limited; // TS2322: '{ [K in keyof T & keyof Limit]: T[K]; }' not assignable to 'T'
}
"#;
    let codes = check_and_get_codes(code);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 when returning intersection-constrained mapped type as T, got: {codes:?}"
    );
}

#[test]
fn intersection_constraint_mapped_excess_property_detected() {
    // When calling `g({ x: 1, y: "y", extra: true })` where g takes
    // `{ [K in keyof T & keyof Limit]: T[K] }`, the extra property should be
    // flagged since Limit only has x and y.
    let code = r#"
type Limit = { x: number; y: string };
declare function g<T>(obj: { [K in keyof T & keyof Limit]: T[K] }): T;
declare function h<U>(obj: { [P in keyof U & keyof Limit]: U[P] }): U;
g({ x: 1, y: "y", extra: true });
h({ x: 1, y: "y", extra: true });
"#;
    let codes = check_and_get_codes(code);
    assert!(
        codes.contains(&2353),
        "Expected TS2353 for excess property 'extra' on intersection-constrained mapped type, got: {codes:?}"
    );
}

#[test]
fn intersection_constraint_mapped_valid_call_no_error() {
    // Calling with exactly the keys in the limit type should have no errors.
    let code = r#"
type Limit = { x: number; y: string };
declare function g<T>(obj: { [K in keyof T & keyof Limit]: T[K] }): T;
const result = g({ x: 1, y: "hello" });
"#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2322) && !codes.contains(&2353) && !codes.contains(&2345),
        "Expected no errors for valid intersection-constrained mapped type call, got: {codes:?}"
    );
}

#[test]
fn intersection_constraint_mapped_renamed_type_param_same_behavior() {
    // Structural rule must be independent of the type parameter name.
    // Both K/T and P/U should behave identically.
    let code_t = r#"
type Limit = { x: number; y: string };
declare function withT<T>(obj: { [K in keyof T & keyof Limit]: T[K] }): T;
withT({ x: 1, y: "y", extra: true });
"#;
    let code_u = r#"
type Limit = { x: number; y: string };
declare function withU<U>(obj: { [Q in keyof U & keyof Limit]: U[Q] }): U;
withU({ x: 1, y: "y", extra: true });
"#;
    let codes_t = check_and_get_codes(code_t);
    let codes_u = check_and_get_codes(code_u);
    assert!(
        codes_t.contains(&2353),
        "Expected TS2353 with T/K naming, got: {codes_t:?}"
    );
    assert!(
        codes_u.contains(&2353),
        "Expected TS2353 with U/Q naming (structural, not name-based), got: {codes_u:?}"
    );
}

#[test]
fn intersection_constraint_mapped_aliased_same_behavior() {
    // The structural rule applies even when the mapped type is behind an alias.
    let code = r#"
type Limit = { x: number; y: string };
type LimitedMap<T> = { [K in keyof T & keyof Limit]: T[K] };
declare function g<T>(obj: LimitedMap<T>): T;
g({ x: 1, y: "y", extra: true });
"#;
    let codes = check_and_get_codes(code);
    assert!(
        codes.contains(&2353),
        "Expected TS2353 for excess property via aliased intersection-constrained mapped type, got: {codes:?}"
    );
}

#[test]
fn intersection_constraint_mixed_params_not_deferred_incorrectly() {
    // `{ [K in keyof T & keyof Limit]: U[K] }` indexes a DIFFERENT type parameter
    // than the one in the keyof arm. The deferral guard must NOT fire here — T and U
    // are structurally distinct type parameters even though both are generic.
    // tsc accepts a valid call and rejects an excess-property call the same as for
    // the homomorphic form, so we verify no spurious errors on the valid shape.
    let code_valid = r#"
type Limit = { x: number; y: string };
declare function f<T, U>(obj: { [K in keyof T & keyof Limit]: U[K] }): U;
f<{ x: number; y: string }, { x: number; y: string }>({ x: 1, y: "hello" });
"#;
    // The two-param form with a distinct template object should not produce
    // spurious errors on an otherwise-valid argument shape.
    let codes_valid = check_and_get_codes(code_valid);
    assert!(
        !codes_valid.contains(&2322),
        "Spurious TS2322 on valid mixed-param intersection-constrained mapped type, got: {codes_valid:?}"
    );
}

// ============================================================================
// Issue #8707: reverse-mapped inference through intersection-constrained T
// ============================================================================

#[test]
fn reverse_mapped_through_intersection_constrained_type_param() {
    // When T extends A & B, reverse-mapped inference through { [K in keyof T]: () => T[K] }
    // must produce T = { a: number; b: string }, not just one side.
    // tsc correctly infers T from all properties; tsz was losing members from one side.
    let code = r#"
interface A { a: number }
interface B { b: string }
type Project<T> = { [K in keyof T]: () => T[K] };
declare function f<T extends A & B>(p: Project<T>): T;
const r = f({ a: () => 1, b: () => "x" });
const _a: number = r.a;
const _b: string = r.b;
"#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339: both .a and .b must be accessible on reverse-inferred T, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322: inferred T must be assignable to both interface members, got: {codes:?}"
    );
}

#[test]
fn reverse_mapped_through_unconstrained_type_param_all_props_inferred() {
    // Unconstrained T: all properties in the source object must be inferred into T,
    // not just the first one. Reverse inference runs independently for each property.
    let code = r#"
type Project<T> = { [K in keyof T]: () => T[K] };
declare function f<T>(p: Project<T>): T;
const r = f({ a: () => 1, b: () => "x" });
const _a: number = r.a;
const _b: string = r.b;
"#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339 accessing r.a and r.b — all properties must be inferred into T, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 on assignment of r.a and r.b, got: {codes:?}"
    );
}

#[test]
fn reverse_mapped_through_interface_intersection_full_inference() {
    // T extends multiple interfaces, each contributing different key-value pairs.
    // The inferred T must contain all keys from all interfaces.
    let code = r#"
interface WithX { x: string }
interface WithY { y: number }
interface WithZ { z: boolean }
type Project<T> = { [K in keyof T]: () => T[K] };
declare function create<T extends WithX & WithY & WithZ>(p: Project<T>): T;
const result = create({ x: () => "hi", y: () => 42, z: () => true });
const _x: string = result.x;
const _y: number = result.y;
const _z: boolean = result.z;
"#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2339),
        "Expected all three properties accessible on result, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 on all three property assignments, got: {codes:?}"
    );
}

#[test]
fn reverse_mapped_many_properties_all_inferred() {
    // Reverse inference per property is independent of how many properties the source has:
    // adding more properties cannot cause earlier ones to be dropped from inferred T.
    // Uses a renamed alias (`Wrap` not `Project`) to confirm the rule is name-independent.
    let code = r#"
type Wrap<T> = { [K in keyof T]: () => T[K] };
declare function wrap<T>(w: Wrap<T>): T;
const result = wrap({
    a: () => 1,
    b: () => "hello",
    c: () => true,
    d: () => null,
});
const a: number = result.a;
const b: string = result.b;
const c: boolean = result.c;
const d: null = result.d;
"#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2339),
        "All properties (a, b, c, d) must be accessible on result, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "All property types must match the inferred T shape, got: {codes:?}"
    );
}
