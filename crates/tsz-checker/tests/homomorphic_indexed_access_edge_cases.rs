//! Edge cases for homomorphic mapped type indexed access.
//! Tests complex patterns beyond the basic K in keyof T -> H<T>[K].

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn check_es5(source: &str) -> Vec<Diagnostic> {
    let lib_files = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
}

fn ts2536(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags.iter().filter(|d| d.code == 2536).collect()
}

/// U extends keyof T as a type parameter constraint (not mapped iteration var)
#[test]
fn type_param_extends_keyof_indexes_required_t() {
    let diags = check_es5("type Test<T, U extends keyof T> = Required<T>[U];");
    assert!(
        ts2536(&diags).is_empty(),
        "Required<T>[U] where U extends keyof T must not emit TS2536: {diags:?}"
    );
}

/// Partial<T>[U] where U extends keyof T
#[test]
fn type_param_extends_keyof_indexes_partial_t() {
    let diags = check_es5("type Test<T, U extends keyof T> = Partial<T>[U];");
    assert!(
        ts2536(&diags).is_empty(),
        "Partial<T>[U] where U extends keyof T must not emit TS2536: {diags:?}"
    );
}

/// Readonly<T>[U] where U extends keyof T
#[test]
fn type_param_extends_keyof_indexes_readonly_t() {
    let diags = check_es5("type Test<T, U extends keyof T> = Readonly<T>[U];");
    assert!(
        ts2536(&diags).is_empty(),
        "Readonly<T>[U] where U extends keyof T must not emit TS2536: {diags:?}"
    );
}

/// K in keyof T & string constraint for the iteration variable
#[test]
fn mapped_with_string_keyof_intersection_constraint() {
    let diags = check_es5("type Test<T> = { [K in keyof T & string]: Required<T>[K] };");
    assert!(
        ts2536(&diags).is_empty(),
        "Required<T>[K] with K in keyof T & string must not emit TS2536: {diags:?}"
    );
}

/// Multiple levels: nested mapped type with homomorphic access
#[test]
fn nested_mapped_type_homomorphic_indexed_access() {
    let diags = check_es5("type Test<T> = { [K in keyof T]: { [J in keyof T]: Required<T>[K] } };");
    assert!(
        ts2536(&diags).is_empty(),
        "Nested Required<T>[K] with outer K in keyof T must not emit TS2536: {diags:?}"
    );
}

/// Tuple value: [K, Required<T>[K]] in ObjectEntries-like pattern
#[test]
fn tuple_with_homomorphic_value_no_ts2536() {
    let diags = check_es5(
        r#"type Pairs<T> = { [K in keyof T]-?: [K, Required<T>[K]] };
type Obj = { a: number; b?: string };
type P = Pairs<Obj>;"#,
    );
    assert!(
        ts2536(&diags).is_empty(),
        "Tuple [K, Required<T>[K]] with K in keyof T must not emit TS2536: {diags:?}"
    );
}

/// Function with generic T and K extends keyof T
#[test]
fn function_generic_k_extends_keyof_t_indexes_required() {
    let diags = check_es5(
        "function test<T, K extends keyof T>(key: K): Required<T>[K] { throw new Error(); }",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "Required<T>[K] where K extends keyof T in function signature must not emit TS2536: {diags:?}"
    );
}

fn ts2322(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags.iter().filter(|d| d.code == 2322).collect()
}

// Repro from TypeScript #28839 / keyofAndIndexedAccessErrors.ts.
//
// Structural rule: indexing a homomorphic mapped type whose value type is the
// uniform `V` (here a `Record<keyof T, V>` wrapped in `Partial`) at a key `K`
// provably within the mapped key space reduces to `V` (with `| undefined`
// applied by `Partial`'s optional modifier). `string` is assignable to
// `string | undefined`, so the assignment must not emit TS2322. Nested generic
// instantiation can alias the same `T` to two distinct interned type-parameter
// `TypeId`s; the reduction must compare `keyof T` by parameter identity.

/// `Partial<Record<keyof T, string>>[K]` (the #28839 repro) accepts `string`.
#[test]
fn partial_record_keyof_indexed_at_generic_key_accepts_string() {
    let diags = check_es5(
        "function f<T, K extends keyof T>() { let x: Partial<Record<keyof T, string>>[K] = \"hello\"; }",
    );
    assert!(
        ts2322(&diags).is_empty(),
        "Partial<Record<keyof T, string>>[K] = \"hello\" must not emit TS2322: {diags:?}"
    );
}

/// Same rule with renamed type parameters proves the fix is not name-keyed.
#[test]
fn partial_record_keyof_indexed_renamed_params_accepts_string() {
    let diags = check_es5(
        "function g<Elem, Idx extends keyof Elem>() { let v: Partial<Record<keyof Elem, string>>[Idx] = \"hi\"; }",
    );
    assert!(
        ts2322(&diags).is_empty(),
        "Partial<Record<keyof Elem, string>>[Idx] = \"hi\" must not emit TS2322: {diags:?}"
    );
}

/// Deeply nested `Partial<…Partial<Record<keyof T, string>>…>[K]` still reduces.
#[test]
fn nested_partial_record_keyof_indexed_accepts_string() {
    let diags = check_es5(
        "function h<T, K extends keyof T>() { let x: Partial<Partial<Partial<Record<keyof T, string>>>>[K] = \"hello\"; }",
    );
    assert!(
        ts2322(&diags).is_empty(),
        "nested Partial<Record<keyof T, string>>[K] = \"hello\" must not emit TS2322: {diags:?}"
    );
}

/// A user-defined homomorphic mapped alias over the generic Record behaves the
/// same as the built-in `Partial`, proving the rule is structural, not tied to
/// the `Partial` alias name.
#[test]
fn user_optional_mapped_over_record_keyof_indexed_accepts_string() {
    let diags = check_es5(
        "type MyOpt<X> = { [P in keyof X]?: X[P] }; function h<T, K extends keyof T>() { let x: MyOpt<Record<keyof T, string>>[K] = \"hello\"; }",
    );
    assert!(
        ts2322(&diags).is_empty(),
        "MyOpt<Record<keyof T, string>>[K] = \"hello\" must not emit TS2322: {diags:?}"
    );
}

/// A non-optional `Record<keyof T, number>[K]` value type is `number`, so
/// assigning a `string` MUST still be rejected — proves the reduction does not
/// over-accept. This is the fallback/negative case.
#[test]
fn record_keyof_indexed_number_value_rejects_string() {
    let diags = check_es5(
        "function h<T, K extends keyof T>() { let x: Record<keyof T, number>[K] = \"hello\"; }",
    );
    assert!(
        !ts2322(&diags).is_empty(),
        "Record<keyof T, number>[K] = \"hello\" must still emit TS2322: {diags:?}"
    );
}
