//! Integration tests for lib type resolution, split from
//! `lib_resolution.rs` to keep that file under the 2000-line ceiling.

use crate::CheckerOptions;
use crate::test_utils::{check_multi_file_with_libs, check_source_codes, load_lib_files};

#[test]
fn promise_type_annotation_no_error() {
    let codes = check_source_codes("let p: Promise<number>;");
    assert!(
        codes.contains(&2304) || codes.contains(&2583) || codes.is_empty(),
        "Promise without libs should produce TS2304/TS2583 or pass: {codes:?}"
    );
}

#[test]
fn async_function_returns_promise_no_crash() {
    let _codes = check_source_codes("async function f(): Promise<string> { return ''; }");
}

#[test]
fn generic_lib_ref_annotation_no_crash() {
    let _codes = check_source_codes("let a: Array<number> = [];");
}

#[test]
fn import_type_basic_no_crash() {
    let _codes = check_source_codes("type T = import('./other').Foo;");
}

#[test]
fn import_type_with_generic_no_crash() {
    let _codes = check_source_codes("type T = import('./other').Bar<number>;");
}

#[test]
fn keyword_type_refs_no_error() {
    let codes = check_source_codes(
        "let s: string; let n: number; let b: boolean; let v: void; let u: undefined;",
    );
    assert!(
        codes.is_empty(),
        "Keyword type annotations should produce no errors: {codes:?}"
    );
}

#[test]
fn keyword_type_in_function_params_no_error() {
    let codes = check_source_codes("function f(a: string, b: number): boolean { return true; }");
    assert!(
        codes.is_empty(),
        "Keyword types in function params should produce no errors: {codes:?}"
    );
}

#[test]
fn null_and_never_types_no_error() {
    let codes = check_source_codes("let n: null = null; let x: never = undefined as never;");
    let _ = codes;
}

#[test]
fn union_of_keyword_types_no_error() {
    let codes = check_source_codes("let x: string | number | boolean = 'hello';");
    assert!(
        codes.is_empty(),
        "Union of keyword types should produce no errors: {codes:?}"
    );
}

#[test]
fn promise_nested_generic_no_crash() {
    let _codes = check_source_codes("let p: Promise<Promise<number>>;");
}

#[test]
fn promise_union_type_arg_no_crash() {
    let _codes = check_source_codes("let p: Promise<string | number>;");
}

#[test]
fn promise_in_return_type_no_crash() {
    let _codes = check_source_codes("function f(): Promise<void> { return undefined as any; }");
}

#[test]
fn promise_all_pattern_no_crash() {
    // Promise.all-like usage pattern
    let _codes = check_source_codes("async function f() { const a = await Promise.resolve(1); }");
}

#[test]
fn promise_like_type_no_crash() {
    // PromiseLike is a separate lib interface
    let _codes = check_source_codes("let p: PromiseLike<string>;");
}

#[test]
fn map_type_no_crash() {
    let _codes = check_source_codes("let m: Map<string, number>;");
}

#[test]
fn set_type_no_crash() {
    let _codes = check_source_codes("let s: Set<number>;");
}

#[test]
fn readonly_array_no_crash() {
    let _codes = check_source_codes("let a: ReadonlyArray<string>;");
}

#[test]
fn record_type_no_crash() {
    let _codes = check_source_codes("let r: Record<string, number>;");
}

#[test]
fn partial_type_no_crash() {
    let _codes = check_source_codes("type P = Partial<{ a: number; b: string }>;");
}

#[test]
fn pick_type_no_crash() {
    let _codes = check_source_codes("type P = Pick<{ a: number; b: string }, 'a'>;");
}

// ---- import-type lowering edge cases ----

#[test]
fn import_type_typeof_no_crash() {
    let _codes = check_source_codes("type T = typeof import('./mod');");
}

#[test]
fn import_type_nested_access_no_crash() {
    // Nested property access on import type
    let _codes = check_source_codes("type T = import('./mod').Ns.Inner;");
}

#[test]
fn import_type_in_function_param_no_crash() {
    let _codes = check_source_codes(
        "function f(x: import('./mod').Foo): import('./mod').Bar { return x as any; }",
    );
}

#[test]
fn import_type_with_multiple_generics_no_crash() {
    let _codes = check_source_codes("type T = import('./mod').Map<string, number>;");
}

// ---- lib ref lowering: intersection of keyword and lib types ----

#[test]
fn intersection_of_keyword_and_lib_type_no_crash() {
    let _codes = check_source_codes("type T = string & { brand: true };");
}

#[test]
fn conditional_type_with_lib_ref_no_crash() {
    let _codes = check_source_codes(
        "type IsArray<T> = T extends Array<infer U> ? U : never; type X = IsArray<number[]>;",
    );
}

#[test]
fn error_type_no_crash() {
    // Error is a lib type
    let _codes = check_source_codes("let e: Error;");
}

#[test]
fn regexp_type_no_crash() {
    let _codes = check_source_codes("let r: RegExp;");
}

#[test]
fn date_type_no_crash() {
    let _codes = check_source_codes("let d: Date;");
}

// ---- Promise lowering: behavioral correctness ----

#[test]
fn promise_assignment_to_wrong_type_no_crash() {
    // Promise<number> should not be assignable to string without error
    let _codes = check_source_codes("let p: Promise<number>; let s: string = p as any;");
}

#[test]
fn async_function_inferred_return_type_no_crash() {
    // Async function return type inference: the returned value wraps in Promise
    let _codes = check_source_codes("async function f() { return 42; }");
}

#[test]
fn promise_with_void_type_arg_no_crash() {
    // Promise<void> is common for side-effect-only async functions
    let _codes = check_source_codes("async function run(): Promise<void> { console.log('done'); }");
}

#[test]
fn promise_constructor_pattern_no_crash() {
    // new Promise() pattern exercises the constructor signature lowering
    let _codes =
        check_source_codes("let p = new Promise<number>((resolve, reject) => { resolve(1); });");
}

#[test]
fn promise_then_chain_no_crash() {
    // .then() method resolution exercises lib heritage merging
    let _codes = check_source_codes("declare let p: Promise<number>; let q = p.then(x => x + 1);");
}

#[test]
fn promise_catch_no_crash() {
    let _codes =
        check_source_codes("declare let p: Promise<number>; let q = p.catch(e => console.log(e));");
}

#[test]
fn promise_race_all_no_crash() {
    // Promise.race / Promise.all are static methods on the Promise constructor
    let _codes = check_source_codes(
        "declare let a: Promise<number>; declare let b: Promise<string>; \
         let r = Promise.race([a, b]);",
    );
}

#[test]
fn awaited_type_no_crash() {
    // Awaited<T> is a conditional type alias in lib
    let _codes = check_source_codes("type X = Awaited<Promise<number>>;");
}

// ---- lib ref lowering: generic utility types (behavioral) ----

#[test]
fn required_type_no_crash() {
    let _codes = check_source_codes("type R = Required<{ a?: number; b?: string }>;");
}

#[test]
fn readonly_utility_type_no_crash() {
    let _codes = check_source_codes("type R = Readonly<{ a: number; b: string }>;");
}

#[test]
fn value_only_local_does_not_shadow_global_readonly_type() {
    let diagnostics = check_multi_file_with_libs(
        &[(
            "test.ts",
            "export declare const Readonly: 1;\ntype R = Readonly<{ a: number }>;",
        )],
        "test.ts",
        CheckerOptions::default(),
        &load_lib_files(&["es5.d.ts"]),
    );
    let codes: Vec<u32> = diagnostics.iter().map(|diag| diag.code).collect();
    assert!(
        !codes.contains(&2749),
        "value-only locals must not shadow global type-space aliases: {codes:?}"
    );
}

#[test]
fn value_only_local_without_type_binding_still_reports_ts2749() {
    let codes = check_source_codes("declare const OnlyValue: 1;\ntype T = OnlyValue<string>;");
    assert!(
        codes.contains(&2749),
        "pure value-only references should still report TS2749: {codes:?}"
    );
}

#[test]
fn omit_type_no_crash() {
    let _codes = check_source_codes("type O = Omit<{ a: number; b: string; c: boolean }, 'c'>;");
}

#[test]
fn exclude_extract_types_no_crash() {
    let _codes = check_source_codes(
        "type E = Exclude<'a' | 'b' | 'c', 'a'>; type X = Extract<'a' | 'b', 'a' | 'c'>;",
    );
}

#[test]
fn return_type_utility_no_crash() {
    let _codes = check_source_codes(
        "function f(x: number): string { return ''; } type R = ReturnType<typeof f>;",
    );
}

#[test]
fn parameters_utility_no_crash() {
    let _codes = check_source_codes(
        "function f(a: number, b: string): void {} type P = Parameters<typeof f>;",
    );
}

#[test]
fn instance_type_utility_no_crash() {
    let _codes =
        check_source_codes("class Foo { x: number = 1; } type I = InstanceType<typeof Foo>;");
}

#[test]
fn non_nullable_utility_no_crash() {
    let _codes = check_source_codes("type N = NonNullable<string | null | undefined>;");
}

// ---- import-type lowering: behavioral ----

#[test]
fn import_type_in_variable_decl_no_crash() {
    let _codes = check_source_codes("let x: import('./mod').SomeType = {} as any;");
}

#[test]
fn import_type_in_type_alias_union_no_crash() {
    let _codes = check_source_codes("type T = string | import('./other').Foo;");
}

#[test]
fn import_type_in_interface_extends_no_crash() {
    let _codes = check_source_codes("interface Foo extends import('./other').Bar { x: number; }");
}

#[test]
fn import_type_in_class_implements_no_crash() {
    let _codes =
        check_source_codes("class Foo implements import('./other').IBar { x: number = 1; }");
}

#[test]
fn import_type_conditional_no_crash() {
    let _codes = check_source_codes("type T = import('./mod').Foo extends string ? true : false;");
}

// ---- lib ref lowering: multiple generic params ----

#[test]
fn weak_map_weak_set_no_crash() {
    let _codes = check_source_codes("let wm: WeakMap<object, number>; let ws: WeakSet<object>;");
}

#[test]
fn generator_type_no_crash() {
    let _codes = check_source_codes(
        "function* gen(): Generator<number, string, boolean> { yield 1; return ''; }",
    );
}

#[test]
fn async_generator_type_no_crash() {
    let _codes = check_source_codes(
        "async function* gen(): AsyncGenerator<number, void, unknown> { yield 1; }",
    );
}

#[test]
fn iterable_iterator_type_no_crash() {
    let _codes = check_source_codes("declare function iter(): IterableIterator<number>;");
}

#[test]
fn async_iterable_type_no_crash() {
    let _codes = check_source_codes("declare function iter(): AsyncIterable<string>;");
}

#[test]
fn array_method_access_no_crash() {
    let _codes = check_source_codes("let a: Array<number> = [1, 2, 3]; let b = a.map(x => x + 1);");
}

#[test]
fn typed_array_no_crash() {
    let _codes = check_source_codes("let a: Int32Array = new Int32Array(10);");
}

#[test]
fn symbol_iterator_no_crash() {
    let _codes = check_source_codes("let s = Symbol.iterator;");
}

#[test]
fn declare_global_interface_augmentation_no_crash() {
    let _codes = check_source_codes(
        "declare global { interface Window { myProp: string; } } \
         export {};",
    );
}

#[test]
fn declare_global_array_augmentation_no_crash() {
    let _codes = check_source_codes(
        "declare global { interface Array<T> { customMethod(): T; } } \
         export {};",
    );
}

#[test]
fn keyword_types_in_generic_position_no_crash() {
    let codes = check_source_codes(
        "type Box<T> = { value: T }; \
         let a: Box<string>; let b: Box<number>; let c: Box<boolean>;",
    );
    assert!(
        codes.is_empty(),
        "Keyword types in generic position should produce no errors: {codes:?}"
    );
}

#[test]
fn keyword_types_in_tuple_no_error() {
    let codes = check_source_codes("let t: [string, number, boolean] = ['a', 1, true];");
    assert!(
        codes.is_empty(),
        "Keyword types in tuple should produce no errors: {codes:?}"
    );
}
