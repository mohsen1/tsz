//! Tests for reverse mapped type inference through union and index signature types.
//!
//! When a generic function like `unboxify<T>(obj: Boxified<T>): T` is called
//! with an object whose properties have union types (e.g., Box<number> | Box<string>),
//! the reverse inference must distribute over union members to correctly infer T.
//!
//! Similarly, when the source object has index signatures (dictionary types),
//! the reverse inference must reverse through the index signature value type.

use crate::test_utils::check_source_codes;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper: parse, bind, check; return diagnostic codes.
fn check_and_get_codes(code: &str) -> Vec<u32> {
    check_source_codes(code)
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
#[ignore = "TODO: pre-existing perf hotspot — recursive reverse-mapped inference over \
             union templates (`Definition<T> = { [K in keyof T]: f(T[K]) | Definition<T[K]> }`). \
             The test passes correctly (~120s locally) but exceeds CI's runner budget. \
             Root cause is in `reverse_infer_through_template` Case 2 (Application template) in \
             tsz-solver/src/operations/constraints/reverse_mapped.rs — recursive expansion via \
             `expand_type_alias_application` + `evaluate_type` does redundant work and has no \
             memoization. Re-enable once a solver-side fix lands. Run manually with \
             `cargo nextest run -E 'test(reverse_mapped_union_template_definition_pattern)' --run-ignored`."]
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
    // The inferred `T` should be `{ p: unknown; q: unknown }`. Accessing
    // `out.q` should therefore produce TS18046 ("of type 'unknown'"), not
    // be silently typed `any` (which would happen if T collapsed to any).
    let code = r#"
type Deep<T> = { [K in keyof T]: Deep<T[K]> }
declare function foo<T>(deep: Deep<T>): T;
interface XHR { readonly p: any; readonly q: number; }
declare let xhr: XHR;
const out = foo(xhr);
out.q.toFixed(2);
"#;
    let codes = check_and_get_codes(code);
    // `out.q` should be `unknown`, so calling `.toFixed` on it is an error.
    // If T had collapsed to `any`, `out.q` would be `any` and the call
    // would silently succeed.
    assert!(
        codes.contains(&18046),
        "Expected TS18046 on `out.q` (which should be `unknown` after structural reverse-mapped inference); got: {codes:?}",
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
    let mut checker = crate::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
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
        crate::context::CheckerOptions {
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
fn reverse_mapped_tuple_context_homomorphic_mapped_type() {
    // reverseMappedTupleContext.ts test1/test2: array literals passed to functions
    // whose parameter is a bare homomorphic mapped type should be inferred as tuples.
    let code = r#"
type Func<T> = () => T;
type Schema = { [K in keyof T]: Func<T[K]> };

declare function create<T extends unknown[]>(schema: { [K in keyof T]: Func<T[K]> }): T;

const result1 = create([() => 1, () => "hello"]);
const x1: number = result1[0];
const x2: string = result1[1];
"#;
    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322: array literal should be inferred as tuple against homomorphic mapped type, got: {codes:?}"
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
