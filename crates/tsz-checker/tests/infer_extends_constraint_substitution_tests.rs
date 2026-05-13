//! Tests for `infer A extends keyof T` constraint substitution during instantiation.
//!
//! When a conditional type has an `infer` variable with a constraint that references
//! type parameters (e.g., `infer A extends keyof T`), the constraint must be properly
//! substituted when the outer type parameters are instantiated. Previously, the
//! constraint TypeId was not substituted, causing `keyof T` to reference a stale
//! type parameter instead of the concrete type, making the infer pattern fail.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source, check_source_with_libs, load_lib_files};

const LIB_NAMES: &[&str] = &[
    "es5.d.ts",
    "es2015.d.ts",
    "es2015.core.d.ts",
    "es2015.collection.d.ts",
    "es2015.iterable.d.ts",
    "es2015.generator.d.ts",
    "es2015.promise.d.ts",
    "es2015.proxy.d.ts",
    "es2015.reflect.d.ts",
    "es2015.symbol.d.ts",
    "es2015.symbol.wellknown.d.ts",
];

fn strict_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..Default::default()
    }
}

fn check_strict(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    check_source(source, "test.ts", strict_options())
}

fn check_strict_with_libs(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    let libs = load_lib_files(LIB_NAMES);
    check_source_with_libs(source, "test.ts", strict_options(), &libs)
}

fn has_error(diags: &[tsz_checker::diagnostics::Diagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

/// `infer A extends keyof T` should work when T is a substituted type parameter.
/// `GetPath<T, P>` recursively walks a path through an object type.
#[test]
fn test_infer_extends_keyof_in_conditional_type() {
    let source = r#"
type Obj = { a: { b: { c: "123" } } };

type GetPath<T, P> =
    P extends readonly [] ? T :
    P extends readonly [infer A extends keyof T, ...infer Rest] ? GetPath<T[A], Rest> :
    never;

type Result = GetPath<Obj, readonly ['a', 'b', 'c']>;

declare let r: Result;
let n: number = r;  // should be TS2322: Type '"123"' is not assignable to type 'number'
"#;
    let diags = check_strict(source);
    assert!(
        has_error(&diags, 2322),
        "Expected TS2322 because GetPath<Obj, ['a', 'b', 'c']> should evaluate to '\"123\"', not 'never'. Got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `infer A extends keyof T` in a tuple pattern should match correctly.
#[test]
fn test_infer_extends_keyof_tuple_pattern() {
    let source = r#"
type Obj = { a: number; b: string };

type FirstKey<T, P> = P extends [infer A extends keyof T, ...infer Rest] ? A : "no_match";
type R = FirstKey<Obj, ["a", "b"]>;

declare let r: R;
let c: "no_match" = r;  // should error: R is "a", not "no_match"
"#;
    let diags = check_strict(source);
    assert!(
        has_error(&diags, 2322),
        "Expected TS2322 because FirstKey<Obj, [\"a\", \"b\"]> should be \"a\", not \"no_match\". Got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Non-tuple `infer extends keyof T` should also work after substitution.
#[test]
fn test_infer_extends_keyof_non_tuple() {
    let source = r#"
type Obj = { a: number; b: string };

type Test<T, X> = X extends infer A extends keyof T ? A : "no_match";
type R = Test<Obj, "a">;

declare let r: R;
let c: "no_match" = r;  // should error: R is "a", not "no_match"
"#;
    let diags = check_strict(source);
    assert!(
        has_error(&diags, 2322),
        "Expected TS2322 because Test<Obj, \"a\"> should be \"a\", not \"no_match\". Got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// When infer constraint is a concrete type (not referencing type params),
/// it should continue to work correctly.
#[test]
fn test_infer_extends_concrete_constraint_still_works() {
    let source = r#"
type Test<X> = X extends infer A extends string ? A : "no_match";
type R = Test<"hello">;

declare let r: R;
let c: "no_match" = r;  // should error: R is "hello"
"#;
    let diags = check_strict(source);
    assert!(
        has_error(&diags, 2322),
        "Expected TS2322 for concrete constraint case. Got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn test_array_element_infer_extends_object_accepts_object_element() {
    let source = r#"
type ExtractElement<T> = T extends (infer U extends object)[] ? U : never;
type Elem = ExtractElement<Array<{ name: string }>>;

const elem: Elem = { name: "test" };
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "Expected Array<object> constrained infer to preserve the object element type. Got: {diags:?}"
    );
}

#[test]
fn test_array_element_infer_extends_object_rejects_primitive_element() {
    let source = r#"
type ExtractElement<T> = T extends (infer U extends object)[] ? U : never;
type Elem = ExtractElement<string[]>;

const elem: Elem = "test";
"#;
    let diags = check_strict(source);
    assert!(
        has_error(&diags, 2322),
        "Expected string[] constrained infer to evaluate to never. Got: {diags:?}"
    );
}

#[test]
fn test_array_element_infer_extends_object_rejects_non_array_application() {
    let source = r#"
type ExtractElement<T> = T extends (infer U extends object)[] ? U : never;
type Elem = ExtractElement<Promise<{ name: string }>>;

const elem: Elem = { name: "test" };
"#;
    let diags = check_strict_with_libs(source);
    assert!(
        has_error(&diags, 2322),
        "Expected Promise<object> constrained array infer to evaluate to never. Got: {diags:?}"
    );
}

/// Template literal `infer N extends number` should parse matching string
/// captures into numeric literals. This keeps tuple string keys like "0" and
/// "1" usable as ordinal indices.
#[test]
fn test_template_literal_infer_extends_number_extracts_tuple_indices() {
    let source = r#"
type IndexFor<S extends string> = S extends `${infer N extends number}` ? N : never;
type Extract<T, U> = T extends U ? T : never;
type IndicesOf<T> = IndexFor<Extract<keyof T, string>>;

declare function getIndex<I extends IndicesOf<[{ name: "x" }, { name: "y" }]>>(index: I): void;

getIndex(0);
getIndex(1);
getIndex(2);
"#;
    let diags = check_strict(source);
    let ts2345 = diags.iter().filter(|d| d.code == 2345).count();
    assert_eq!(
        ts2345, 1,
        "Expected only getIndex(2) to emit TS2345; valid tuple indices 0 and 1 should be accepted. Got: {diags:#?}"
    );
    let message = diags
        .iter()
        .find(|d| d.code == 2345)
        .map(|d| d.message_text.as_str())
        .unwrap_or("");
    assert!(
        message.contains("parameter of type '0 | 1'"),
        "Expected invalid tuple index diagnostic to display the evaluated index union, got: {message}"
    );
}

#[test]
fn test_template_literal_infer_extends_number_direct_union() {
    let source = r#"
type IndexFor<S extends string> = S extends `${infer N extends number}` ? N : never;
type R = IndexFor<"0" | "1">;

declare function getIndex<I extends R>(index: I): void;

getIndex(0);
getIndex(1);
getIndex(2);
"#;
    let diags = check_strict(source);
    let ts2345 = diags.iter().filter(|d| d.code == 2345).count();
    assert_eq!(
        ts2345, 1,
        "Expected only getIndex(2) to emit TS2345 for direct string numeric keys. Got: {diags:#?}"
    );
}

#[test]
fn test_template_literal_infer_extends_number_after_extract() {
    let source = r#"
type IndexFor<S extends string> = S extends `${infer N extends number}` ? N : never;
type Extract<T, U> = T extends U ? T : never;
type R = IndexFor<Extract<"0" | "1" | "length", string>>;

declare function getIndex<I extends R>(index: I): void;

getIndex(0);
getIndex(1);
getIndex(2);
"#;
    let diags = check_strict(source);
    let ts2345 = diags.iter().filter(|d| d.code == 2345).count();
    assert_eq!(
        ts2345, 1,
        "Expected only getIndex(2) to emit TS2345 after Extract. Got: {diags:#?}"
    );
}

#[test]
fn test_extract_keyof_string_preserves_unique_symbol_inference_candidate() {
    let source = r#"
type Extract<T, U> = T extends U ? T : never;

declare function getProperty2<T, K extends keyof T>(obj: T, key: Extract<K, string>): T[K];

declare const s: unique symbol;
interface StrNum {
    first: string;
    second: number;
    [s]: string;
}
declare const obj: StrNum;
let prop: string;

prop = getProperty2(obj, "first");
prop = getProperty2(obj, s);
"#;
    let diags = check_strict(source);
    let ts2345: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "Expected only the unique-symbol Extract<K, string> argument to emit TS2345. Got: {diags:#?}"
    );
    assert!(
        ts2345[0].message_text.contains("parameter of type 'never'"),
        "The invalid argument should instantiate Extract<typeof s, string> to never, got: {}",
        ts2345[0].message_text
    );
    assert!(
        diags.iter().all(|d| d.code != 2322),
        "T[typeof s] should remain string, so the call result must not cascade into TS2322. Got: {diags:#?}"
    );
}

#[test]
fn test_path_keys_accepts_readonly_tuple_numeric_string_index() {
    let source = r#"
type PropType<T, Path extends string> =
    string extends Path ? unknown :
    Path extends keyof T ? T[Path] :
    Path extends `${infer K}.${infer R}` ? K extends keyof T ? PropType<T[K], R> : unknown :
    unknown;

type PathKeys<T> =
    unknown extends T ? never :
    T extends readonly any[] ? Extract<keyof T, `${number}`> | SubKeys<T, Extract<keyof T, `${number}`>> :
    T extends object ? Extract<keyof T, string> | SubKeys<T, Extract<keyof T, string>> :
    never;

type SubKeys<T, K extends string> = K extends keyof T ? `${K}.${PathKeys<T[K]>}` : never;

declare function getProp<T, P extends PathKeys<T>>(obj: T, path: P): PropType<T, P>;

const obj2 = {
    name: 'John',
    age: 42,
    cars: [
        { make: 'Ford', age: 10 },
        { make: 'Trabant', age: 35 }
    ]
} as const;

getProp(obj2, 'cars.1.make');
"#;
    let diags = check_strict_with_libs(source);
    let ts2345: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345.is_empty(),
        "Expected readonly tuple numeric string path to be accepted without TS2345. Got: {diags:#?}"
    );
}

/// When infer constraint fails (value doesn't match keyof T), should get false branch.
#[test]
fn test_infer_extends_keyof_constraint_fails_correctly() {
    let source = r#"
type Obj = { a: number };

type Test<T, X> = X extends infer A extends keyof T ? A : "no_match";
type R = Test<Obj, "z">;  // "z" is not keyof Obj

declare let r: R;
let c: "no_match" = r;  // should NOT error: R should be "no_match"
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "Should NOT get TS2322: 'z' is not keyof Obj, so result should be 'no_match'. Got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Tuple pattern `[infer F extends C, ...Array<X>]` — issue #6179
//
// Structural rule: when matching a `Tuple` source against an `Array(P)` pattern
// inside a conditional's tuple rest position, the tuple is structurally an
// array whose element type is the union of its element types. Infer variables
// inside `P` (and the prefix infer's constraint check) must use that union.
// ---------------------------------------------------------------------------

/// The reported repro: `[infer F extends string, ...unknown[]]` against a
/// concrete tuple should infer F as the first element when that element
/// satisfies the constraint.
#[test]
fn test_infer_extends_string_with_unknown_array_rest_picks_first_string() {
    let source = r#"
type FirstString<T> = T extends [infer F extends string, ...unknown[]] ? F : never;
type FS = FirstString<["a", 1, 2]>;
const fs: FS = "a";
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "FirstString<[\"a\",1,2]> should be \"a\", not never. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Same rule with `number` constraint and a different first element type —
/// proves the fix is not specific to `string`.
#[test]
fn test_infer_extends_number_with_unknown_array_rest_picks_first_number() {
    let source = r#"
type FirstNumber<T> = T extends [infer F extends number, ...unknown[]] ? F : never;
type FN = FirstNumber<[1, "x", 2]>;
const n: FN = 1;
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "FirstNumber<[1,\"x\",2]> should be 1, not never. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Rename invariance: the same rule must apply when the infer variable has a
/// different name. If a name appears in the fix, this regression catches it.
#[test]
fn test_infer_extends_string_rest_array_rename_invariance() {
    let source = r#"
type Head<T> = T extends [infer X extends string, ...unknown[]] ? X : never;
type H = Head<["abc", 1, 2]>;
const h: H = "abc";
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "Renamed-X variant should still infer the first string element. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// When the prefix infer's constraint fails (first element does not satisfy
/// `extends string`), evaluation must fall through to the false branch
/// rather than incorrectly binding the wrong element.
#[test]
fn test_infer_extends_string_rest_array_constraint_failure_takes_false_branch() {
    let source = r#"
type FirstString<T> = T extends [infer F extends string, ...unknown[]] ? F : "no_match";
type R = FirstString<[42, "y"]>;
const r: R = "no_match";
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "First element 42 doesn't extend string → R should be \"no_match\". Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Infer variable in the rest pattern's array element position must bind to
/// the union of the remaining tuple element types.
#[test]
fn test_infer_in_array_rest_pattern_binds_to_union_of_remaining_tuple_elements() {
    let source = r#"
type TailUnion<T> = T extends [unknown, ...(infer R)[]] ? R : never;
type U = TailUnion<["a", 1, true]>;
declare let u: U;
const a: 1 | true = u;
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "TailUnion<[\"a\",1,true]> should bind R to 1 | true. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Standalone Array pattern (no tuple wrapper) against a Tuple source:
/// `T extends (infer X)[] ? X : never` with `T = [1, "a"]` should bind
/// `X = 1 | "a"`.
#[test]
fn test_array_pattern_against_tuple_source_binds_element_union() {
    let source = r#"
type Elem<T> = T extends (infer X)[] ? X : never;
type E = Elem<[1, "a"]>;
declare let e: E;
const r: 1 | "a" = e;
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "Elem<[1,\"a\"]> should be 1 | \"a\". Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Existing array-vs-array path must still work unchanged.
#[test]
fn test_array_pattern_against_array_source_still_works() {
    let source = r#"
type Elem<T> = T extends (infer X)[] ? X : never;
type E = Elem<string[]>;
const r: E = "ok";
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "Elem<string[]> should still be string. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
