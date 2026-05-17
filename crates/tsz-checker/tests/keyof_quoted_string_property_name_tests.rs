//! `{ "1": ... }` and `{ 1: ... }` are semantically different key spaces:
//! the first yields a string-literal key `"1"`, the second yields a
//! number-literal key `1`. Both parse to the same `Atom`, so the
//! distinguishing fact lives on `PropertyInfo::is_string_named`, which
//! must be derived from the property name's AST node kind
//! (`StringLiteral` directly, or a `COMPUTED_PROPERTY_NAME` whose
//! expression is a `StringLiteral`).

use tsz_checker::test_utils::check_source_code_messages as compile_and_get_diagnostics;

fn ts_codes(diagnostics: &[(u32, String)]) -> Vec<u32> {
    let mut codes: Vec<u32> = diagnostics.iter().map(|(c, _)| *c).collect();
    codes.sort_unstable();
    codes
}

/// `keyof` over a type literal with a single quoted numeric key must yield the
/// *string* literal `"1"`, not the number literal `1`.
#[test]
fn keyof_type_literal_quoted_numeric_key_is_string_literal() {
    let source = r#"
type Foo = { "1": number };
type K = keyof Foo;
// Distributive conditional: "yes" if K is string-shaped, "no" otherwise.
type IsString = K extends string ? "yes" : "no";
const a: IsString = "yes";
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        ts_codes(&diagnostics).is_empty(),
        "no errors expected; got: {diagnostics:#?}"
    );
}

/// Same rule with a different bare-numeric spelling and a different alias
/// name to confirm the fix is structural, not keyed to `"1"`/`Foo`.
#[test]
fn keyof_type_literal_quoted_numeric_key_is_string_literal_renamed() {
    let source = r#"
type Bar = { "404": string };
type K = keyof Bar;
type IsString = K extends string ? "yes" : "no";
const a: IsString = "yes";
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        ts_codes(&diagnostics).is_empty(),
        "no errors expected; got: {diagnostics:#?}"
    );
}

/// Bare-numeric keys (no quotes) must remain *number* literals: `{ 1: ... }`
/// has `keyof` of `1`, not `"1"`. This is the contrast case that makes the
/// previous tests meaningful — a fix that always emits string literals would
/// pass the above but fail this one.
#[test]
fn keyof_type_literal_bare_numeric_key_is_number_literal() {
    let source = r#"
type Foo = { 1: number };
type K = keyof Foo;
type IsNumber = K extends number ? "yes" : "no";
const a: IsNumber = "yes";
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        ts_codes(&diagnostics).is_empty(),
        "no errors expected; got: {diagnostics:#?}"
    );
}

/// Quoted method signatures flow through the type-literal method-overload
/// merge path, not the plain property-signature path. They must preserve the
/// same string-key fact for `keyof`.
#[test]
fn keyof_type_literal_quoted_numeric_method_key_is_string_literal() {
    let source = r#"
type Foo = { "1"(): number };
const ok: keyof Foo = "1";
const bad: keyof Foo = 1;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2322: Vec<&(u32, String)> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for bare number key; got: {diagnostics:#?}"
    );
}

/// Same key-space rule must hold for interfaces (not just type literals).
/// Interfaces and type literals flow through different lowering paths
/// internally; the structural fix must cover both.
///
/// Use distributive `K extends string` here (not `K extends "1"`) because the
/// distributive conditional over a union would otherwise produce
/// `"yes" | "no"` for a multi-key union.
#[test]
fn keyof_interface_quoted_numeric_key_is_string_literal() {
    let source = r#"
interface Foo { "1": number; "2": string }
type K = keyof Foo;
type IsString = K extends string ? "yes" : "no";
const a: IsString = "yes";
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        ts_codes(&diagnostics).is_empty(),
        "no errors expected; got: {diagnostics:#?}"
    );
}

/// Assigning a string literal `"1"` to `keyof Foo` for an interface with
/// quoted numeric keys must be accepted. The bug previously rejected this
/// because `keyof Foo` was resolving to `1 | 2` (number literals).
#[test]
fn quoted_numeric_interface_key_accepts_string_literal_assignment() {
    let source = r#"
interface Foo { "1": number; "2": string }
const a: keyof Foo = "1";
const b: keyof Foo = "2";
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        ts_codes(&diagnostics).is_empty(),
        "no errors expected; got: {diagnostics:#?}"
    );
}

/// Contrast: a quoted-key interface must REJECT a bare numeric literal at
/// `keyof Foo`, because the keys are `"1" | "2"` (strings), not `1 | 2`.
#[test]
fn quoted_numeric_interface_key_rejects_bare_number_assignment() {
    let source = r#"
interface Foo { "1": number; "2": string }
const a: keyof Foo = 1;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2322: Vec<&(u32, String)> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "expected TS2322 for `1` not assignable to keyof Foo; got: {diagnostics:#?}"
    );
}

/// Nested indexed access through a quoted-key property: `Foo["a"]["1"]`
/// must propagate the inner literal `123` end-to-end.
#[test]
fn indexed_access_through_quoted_numeric_key_reaches_inner_literal() {
    let source = r#"
interface Foo {
  a: {
    "1": 123;
    "2": string;
  };
}
type Inner = Foo["a"]["1"];
const ok: Inner = 123;
const bad: Inner = 124;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2322: Vec<&(u32, String)> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 (on the wrong literal); got: {diagnostics:#?}"
    );
    let (_, msg) = ts2322[0];
    assert!(
        msg.contains("'124'") && msg.contains("'123'"),
        "TS2322 should compare 124 against 123, got: {msg}"
    );
}
