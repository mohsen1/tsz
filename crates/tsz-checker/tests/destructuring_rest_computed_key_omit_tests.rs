//! Locks tsc-parity for `Omit<T, K>` construction when object destructuring
//! has a computed property key binding element as a non-rest sibling.
//!
//! Rule: when `const { [expr]: v, ...rest } = obj` where `obj: T` (a type
//! parameter) and `expr` has type `K`, the rest type is `Omit<T, K>`.
//!
//! When `expr` resolves to a string-literal type, the literal name is
//! incorporated into the concrete exclusion set (same as for explicit keys).
//!
//! Related: issue 6143.

use tsz_checker::test_utils::check_source_code_messages as checker_diagnostics;
use tsz_common::diagnostics::diagnostic_codes;

// ------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------

fn ts2322_messages(diags: &[(u32, String)]) -> Vec<String> {
    diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, m)| m.clone())
        .collect()
}

// Preamble with minimal `Omit`/`Pick`/`Exclude` definitions for tests that
// do not pull in the full standard library.
const OMIT_DEFS: &str = r#"
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type Pick<T, K extends keyof T> = { [P in K]: T[P]; };
type Exclude<T, U> = T extends U ? never : T;
"#;

// ------------------------------------------------------------------
// Tests
// ------------------------------------------------------------------

/// Reported repro from issue #6143: `omitKey` helper using a computed
/// destructuring key with a generic `K extends keyof T`.
#[test]
fn computed_key_rest_generic_omit_key_function_no_error() {
    let source = format!(
        r#"
{OMIT_DEFS}
function omitKey<T extends object, K extends keyof T>(obj: T, key: K): Omit<T, K> {{
    const {{ [key]: _, ...rest }} = obj;
    return rest;
}}

const result = omitKey({{ a: 1, b: 2, c: 3 }}, "b");
"#
    );
    let diags = checker_diagnostics(&source);
    let errs = ts2322_messages(&diags);
    assert!(
        errs.is_empty(),
        "Expected no TS2322 for generic omitKey — rest should be Omit<T, K>. Got: {errs:?}"
    );
}

/// Same rule, different type-parameter name — proves the fix is structural,
/// not tied to the specific identifier spelling.
#[test]
fn computed_key_rest_alternate_type_param_names_no_error() {
    let source = format!(
        r#"
{OMIT_DEFS}
function removeKey<Source extends object, Prop extends keyof Source>(
    source: Source,
    prop: Prop,
): Omit<Source, Prop> {{
    const {{ [prop]: _, ...remainder }} = source;
    return remainder;
}}
"#
    );
    let diags = checker_diagnostics(&source);
    let errs = ts2322_messages(&diags);
    assert!(
        errs.is_empty(),
        "Expected no TS2322 with alternate param names. Got: {errs:?}"
    );
}

/// Static key + computed key: `{ a, [key]: _, ...rest }`.
/// The rest type should be `Omit<T, "a" | K>`.
#[test]
fn static_and_computed_key_combined_no_error() {
    let source = format!(
        r#"
{OMIT_DEFS}
function omitTwo<T extends {{ a: unknown }}, K extends keyof T>(
    obj: T,
    key: K,
): Omit<T, "a" | K> {{
    const {{ a: _a, [key]: _k, ...rest }} = obj;
    return rest;
}}
"#
    );
    let diags = checker_diagnostics(&source);
    let errs = ts2322_messages(&diags);
    assert!(
        errs.is_empty(),
        "Expected no TS2322 when combining static and computed key exclusions. Got: {errs:?}"
    );
}

/// Static string-literal computed key `{ ['b']: _, ...rest }` on a concrete
/// object type — the `'b'` property should be excluded from the rest type.
#[test]
fn concrete_source_string_literal_computed_key_excludes_property() {
    let source = r#"
const obj = { a: 1, b: 2, c: 3 };
const { ['b']: _, ...rest } = obj;
// rest should have type { a: number; c: number } — NOT include 'b'.
const _check: { a: number; c: number } = rest;
"#;
    let diags = checker_diagnostics(source);
    let errs = ts2322_messages(&diags);
    assert!(
        errs.is_empty(),
        "Expected no TS2322 for string-literal computed key on concrete type. Got: {errs:?}"
    );
}

/// Union source + computed string-literal key: each union member should have
/// the dynamically-keyed property excluded from the rest type.
#[test]
fn union_source_string_literal_computed_key_excludes_property() {
    let source = r#"
declare const obj: { a: number; b: string } | { a: boolean; b: number };
const { ['a']: _, ...rest } = obj;
// rest should be { b: string } | { b: number } — 'a' excluded from both.
const _check: { b: string } | { b: number } = rest;
"#;
    let diags = checker_diagnostics(source);
    let errs = ts2322_messages(&diags);
    assert!(
        errs.is_empty(),
        "Expected no TS2322 for string-literal computed key on union source. Got: {errs:?}"
    );
}

/// Ensure the original static-key behavior is preserved: `{ a, ...rest }` on a
/// concrete type still correctly excludes `a` from the rest type.
#[test]
fn static_key_rest_concrete_type_unchanged() {
    let source = r#"
const obj = { a: 1, b: 2, c: 3 };
const { a: _, ...rest } = obj;
const _check: { b: number; c: number } = rest;
"#;
    let diags = checker_diagnostics(source);
    let errs = ts2322_messages(&diags);
    assert!(
        errs.is_empty(),
        "Static-key rest on concrete type should still work. Got: {errs:?}"
    );
}
