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

#[test]
fn anyof_empty_object_matches_never_index_falsy_pattern() {
    let source = r#"
type Falsy = 0 | '' | false | [] | { [K in any]: never } | undefined | null;
type AnyOf<T extends readonly unknown[]> = T extends readonly [infer F, ...infer R]
  ? F extends Falsy ? AnyOf<R> : true
  : false;

type AO1 = AnyOf<[0, '', false, [], {}, null, undefined]>;
const ao1: AO1 = false;
"#;

    let diags = check_strict(source);
    assert!(
        diags.is_empty(),
        "expected AnyOf tuple of falsy members to evaluate to false, got diagnostics: {diags:?}"
    );
}

#[test]
fn template_literal_middle_infer_matches_known_substring() {
    let source = r#"
type DropString<S extends string, T extends string> =
  S extends `${infer Before}${T}${infer After}`
    ? `${Before}${After}`
    : S;

type DS1 = DropString<'hello', 'l'>;
const ds1: DS1 = 'helo';
"#;

    let diags = check_strict(source);
    assert!(
        diags.is_empty(),
        "expected DropString<'hello', 'l'> to evaluate to 'helo', got diagnostics: {diags:?}"
    );
}

#[test]
fn template_literal_trailing_infer_matches_union_suffix() {
    let source = r#"
type Whitespace = ' ' | '\t' | '\n';

type TrimRight<S extends string> =
  S extends `${infer Rest}${Whitespace}`
    ? TrimRight<Rest>
    : S;

type TR1 = TrimRight<'hello  '>;
const tr1: TR1 = 'hello';
"#;

    let diags = check_strict(source);
    assert!(
        diags.is_empty(),
        "expected TrimRight<'hello  '> to evaluate to 'hello', got diagnostics: {diags:?}"
    );
}

#[test]
fn template_literal_type_parameter_delimiters_match() {
    let source = r#"
type Param<S extends string, L extends string, R extends string> =
  S extends `${L}${infer X}${R}` ? X : never;

type P1 = Param<"(hello)", "(", ")">;
const p1: P1 = "hello";
"#;

    let diags = check_strict(source);
    assert!(
        diags.is_empty(),
        "expected Param<'(hello)', '(', ')'> to evaluate to 'hello', got diagnostics: {diags:?}"
    );
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

#[test]
fn test_constrained_infer_preserves_tuple_head_literal() {
    let source = r#"
type FirstString<T> = T extends [infer F extends string, ...any[]] ? F : never;
type FS1 = FirstString<["hello", 1, true]>;

const fs1: FS1 = "hello";
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "Expected constrained tuple-head infer to preserve the literal type. Got: {diags:#?}"
    );
}

#[test]
fn named_function_parameter_infer_extracts_first_arg() {
    let source = r#"
type FirstArg<T> = T extends (first: infer F, ...args: any[]) => any ? F : never;
type FA = FirstArg<(a: number, b: string) => void>;

const fa: FA = 42;
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "Expected named function parameter infer to extract number. Got: {diags:#?}"
    );
}

#[test]
fn test_constrained_infer_preserves_function_return_literal() {
    let source = r#"
type ReturnString<T> = T extends (...args: any[]) => (infer R extends string) ? R : never;
type RS1 = ReturnString<() => "hello">;

const rs1: RS1 = "hello";
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "Expected constrained function-return infer to preserve the literal type. Got: {diags:#?}"
    );
}

#[test]
fn named_single_function_parameter_infer_extracts_arg() {
    let source = r#"
type Arg<T> = T extends (x: infer X) => any ? X : never;
type A = Arg<(value: string) => void>;

const a: A = "value";
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "Expected single named function parameter infer to extract string. Got: {diags:#?}"
    );
}

#[test]
fn test_constrained_infer_preserves_object_property_literal() {
    let source = r#"
type ExtractValue<T> = T extends { value: infer V extends string | number } ? V : never;
type EV1 = ExtractValue<{ value: "test" }>;

const ev1: EV1 = "test";
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "Expected constrained object-property infer to preserve the literal type. Got: {diags:#?}"
    );
}

#[test]
fn test_constrained_infer_extracts_tuple_length_literal() {
    let source = r#"
type LengthOf<T> = T extends { length: infer L extends number } ? L : never;
type R = LengthOf<[1, 2, 3]>;

const ok: R = 3;
const bad: R = 4;
"#;
    let diags = check_strict(source);
    let ts2322 = diags.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322, 1,
        "Expected constrained tuple length infer to preserve literal 3 and reject 4. Got: {diags:#?}"
    );
}

#[test]
fn test_constrained_infer_extracts_readonly_tuple_length_literal_with_renamed_binder() {
    let source = r#"
type SizeOf<T> = T extends { length: infer Size extends number } ? Size : never;
type R = SizeOf<readonly ["a", "b"]>;

const ok: R = 2;
const bad: R = 3;
"#;
    let diags = check_strict(source);
    let ts2322 = diags.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322, 1,
        "Expected constrained readonly tuple length infer to preserve literal 2 and reject 3. Got: {diags:#?}"
    );
}

#[test]
fn test_constrained_infer_extracts_array_length_as_number() {
    let source = r#"
type LengthOf<T> = T extends { length: infer L extends number } ? L : never;
type R = LengthOf<string[]>;

const ok: R = 123;
const bad: R = "nope";
"#;
    let diags = check_strict(source);
    let ts2322 = diags.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322, 1,
        "Expected constrained array length infer to produce number and reject string. Got: {diags:#?}"
    );
}

#[test]
fn named_method_parameter_infer_extracts_arg() {
    let source = r#"
type MethodArg<T> = T extends { method(arg: infer A): any } ? A : never;
type A = MethodArg<{ method(value: boolean): void }>;

const a: A = true;
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "Expected named method parameter infer to extract boolean. Got: {diags:#?}"
    );
}
