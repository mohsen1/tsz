//! Tests for `{ [K in keyof T]: V }[keyof T]` evaluation — the solver must
//! substitute `V` when indexing a homomorphic mapped type with its own
//! constraint (`keyof T`).
//!
//! Structural rule: when `{ [K in C]: V }[I]` is evaluated and `I`
//! semantically matches the mapped constraint `C` (e.g. both are `keyof T`),
//! the template `V` is the result. This is the `KeyOf`-index path; prior to
//! this fix the non-union `KeyOf` node bypassed `generic_index_covering_mapped_constraint`
//! and returned `None`, leaving the expression unevaluated.
//!
//! Key invariant checked: the fix is keyed on semantic identity of `I` and `C`,
//! not on any name spelling or union structure.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    let lib_files = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    assert!(!lib_files.is_empty(), "es5.d.ts lib file not loaded");
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

fn has_code(diags: &[Diagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

const fn no_errors(diags: &[Diagnostic]) -> bool {
    diags.is_empty()
}

// ============================================================================
// Core: mapped-type keyof index access evaluates to the template
// ============================================================================

/// `{ [k in keyof T]: Spy }[keyof T]` must evaluate to `Spy`.
/// When `.and` is a `Function` property of `Spy`, accessing `.returnValue`
/// on `Function` must produce TS2339.
///
/// This is the `spyComparisonChecking.ts` pattern (accepted regression fixed).
/// The test avoids `for..of` (which requires ES2015 lib) and instead uses
/// a `declare const` key of the concrete type to isolate the evaluation.
#[test]
fn spy_obj_key_and_returnvalue_emits_ts2339() {
    let source = r#"
interface Spy extends Function {
    and: Function;
}
type SpyObj<T> = T & { [k in keyof T]: Spy; }
declare const spyObj: SpyObj<{foo(): void}>;
declare const key: keyof {foo(): void};
spyObj[key].and.returnValue(1);
"#;

    let diags = check_strict(source);
    assert!(
        has_code(&diags, 2339),
        "expected TS2339 for returnValue on Function, got: {diags:#?}"
    );
}

/// Same pattern with a different mapped-variable name (`P` instead of `k`)
/// to confirm the fix is not name-sensitive.
#[test]
fn spy_obj_key_and_returnvalue_emits_ts2339_p_name() {
    let source = r#"
interface Spy extends Function {
    and: Function;
}
type SpyObj<T> = T & { [P in keyof T]: Spy; }
declare const spyObj: SpyObj<{foo(): void}>;
declare const key: keyof {foo(): void};
spyObj[key].and.returnValue(1);
"#;

    let diags = check_strict(source);
    assert!(
        has_code(&diags, 2339),
        "expected TS2339 with mapped variable named P, got: {diags:#?}"
    );
}

/// Same pattern with a long descriptive mapped-variable name to confirm
/// the fix generalises beyond single-letter names.
#[test]
fn spy_obj_key_and_returnvalue_emits_ts2339_methodkey_name() {
    let source = r#"
interface Spy extends Function {
    and: Function;
}
type SpyObj<T> = T & { [MethodKey in keyof T]: Spy; }
declare const spyObj: SpyObj<{foo(): void}>;
declare const key: keyof {foo(): void};
spyObj[key].and.returnValue(1);
"#;

    let diags = check_strict(source);
    assert!(
        has_code(&diags, 2339),
        "expected TS2339 with mapped variable named MethodKey, got: {diags:#?}"
    );
}

// ============================================================================
// Direct evaluation: `{ [K in keyof T]: V }[keyof T]` assignability
// ============================================================================

/// Assigning `{ [k in keyof T]: number }[keyof T]` to `number` must not
/// produce TS2322 — the indexed access evaluates to `number`.
#[test]
fn mapped_keyof_index_access_assigns_to_value_type() {
    let source = r#"
function f<T>(obj: { [k in keyof T]: number }, key: keyof T): void {
    const x: number = obj[key];
}
"#;

    let diags = check_strict(source);
    assert!(
        no_errors(&diags),
        "{{ [k in keyof T]: number }}[keyof T] should assign to number without any errors, got: {diags:#?}"
    );
}

/// Same with type parameter named `Key` to rule out name-keying.
#[test]
fn mapped_keyof_index_access_assigns_to_value_type_key_name() {
    let source = r#"
function f<U>(obj: { [Key in keyof U]: string }, key: keyof U): void {
    const x: string = obj[key];
}
"#;

    let diags = check_strict(source);
    assert!(
        no_errors(&diags),
        "{{ [Key in keyof U]: string }}[keyof U] should assign to string without any errors, got: {diags:#?}"
    );
}

/// Using a type alias for the mapped type — the fix must work when the
/// object is referenced via an alias rather than inline.
#[test]
fn mapped_keyof_index_access_via_alias_assigns_to_value_type() {
    let source = r#"
type Box<T> = { [k in keyof T]: boolean };
function f<T>(obj: Box<T>, key: keyof T): void {
    const x: boolean = obj[key];
}
"#;

    let diags = check_strict(source);
    assert!(
        no_errors(&diags),
        "Box<T>[keyof T] via alias should assign to boolean without any errors, got: {diags:#?}"
    );
}

// ============================================================================
// Assignability mismatch: value type mismatch must produce TS2322
// ============================================================================

/// Assigning `{ [k in keyof T]: number }[keyof T]` to `string` must
/// produce TS2322 — the evaluation gives `number` which is not `string`.
#[test]
fn mapped_keyof_index_access_wrong_value_type_emits_ts2322() {
    let source = r#"
function f<T>(obj: { [k in keyof T]: number }, key: keyof T): void {
    const x: string = obj[key];
}
"#;

    let diags = check_strict(source);
    assert!(
        has_code(&diags, 2322),
        "{{ [k in keyof T]: number }}[keyof T] assigned to string should emit TS2322, got: {diags:#?}"
    );
}
