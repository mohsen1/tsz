//! Tests for reverse mapped type inference through union and index signature types.
//!
//! When a generic function like `unboxify<T>(obj: Boxified<T>): T` is called
//! with an object whose properties have union types (e.g., Box<number> | Box<string>),
//! the reverse inference must distribute over union members to correctly infer T.
//!
//! Similarly, when the source object has index signatures (dictionary types),
//! the reverse inference must reverse through the index signature value type.

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper: parse, bind, check; return diagnostic codes.
fn check_and_get_codes(code: &str) -> Vec<u32> {
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
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
