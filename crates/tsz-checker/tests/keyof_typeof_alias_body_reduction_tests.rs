//! Fences for `keyof typeof X` written as a type-alias body.
//!
//! When a `keyof typeof X` type is stored as a type alias's body, tsc still
//! reduces it to the finite literal key union at use sites — identically to
//! the same type written inline in an annotation. tsz was leaving the alias
//! body as a deferred `KeyOf` (the enum-namespace / value-symbol resolution
//! only happened on the inline annotation walk), so the target never became
//! a reduced literal union: the source literal widened and the diagnostic
//! rendered the deferred form. Every expectation here is oracle-pinned
//! against the repo's pinned oracle, `typescript@7.0.2` via
//! `scripts/conformance/oracle.sh` (`--singleThreaded --stableTypeOrdering
//! true`, the exact invocation the conformance cache scores). 7.0.2 renders
//! these key unions in sorted member order, not declaration order — the 6.0
//! line disagrees, so never re-derive these strings from a container-global
//! 6.0 `tsc`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn strict_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        strict_function_types: true,
        ..CheckerOptions::default()
    }
}

fn expect_code_with(source: &str, code: u32, expected: &str) {
    let diagnostics = check_source(source, "test.ts", strict_options());
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == code)
        .unwrap_or_else(|| panic!("expected a TS{code} diagnostic, got: {diagnostics:?}"));
    assert!(
        diag.message_text.contains(expected),
        "expected message containing {expected:?}, got: {diag:?}"
    );
}

fn expect_clean(source: &str) {
    let diagnostics = check_source(source, "test.ts", strict_options());
    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics, got: {diagnostics:?}"
    );
}

#[test]
fn enum_keyof_typeof_alias_reduces_to_key_literal_union() {
    expect_code_with(
        r#"
enum Color { Red, Green }
type K = keyof typeof Color;
const bad: K = "nope";
"#,
        2322,
        r#"Type '"nope"' is not assignable to type '"Green" | "Red"'."#,
    );
}

#[test]
fn enum_keyof_typeof_alias_accepts_valid_member_name() {
    expect_clean(
        r#"
enum Color { Red, Green }
type K = keyof typeof Color;
const ok: K = "Red";
"#,
    );
}

#[test]
fn object_keyof_typeof_alias_reduces_to_key_literal_union() {
    expect_code_with(
        r#"
const obj = { a: 1, b: 2 };
type K = keyof typeof obj;
const bad: K = "nope";
"#,
        2322,
        r#"Type '"nope"' is not assignable to type '"a" | "b"'."#,
    );
}

#[test]
fn object_keyof_typeof_alias_accepts_valid_key() {
    expect_clean(
        r#"
const obj = { a: 1, b: 2 };
type K = keyof typeof obj;
const ok: K = "a";
"#,
    );
}

#[test]
fn enum_keyof_typeof_alias_chain_reduces() {
    expect_code_with(
        r#"
enum Direction { Up, Down }
type K1 = keyof typeof Direction;
type K2 = K1;
const bad: K2 = "nope";
"#,
        2322,
        r#"Type '"nope"' is not assignable to type '"Down" | "Up"'."#,
    );
}

#[test]
fn single_member_enum_keyof_typeof_alias_reduces() {
    expect_code_with(
        r#"
enum One { Only }
type K = keyof typeof One;
const bad: K = "nope";
"#,
        2322,
        r#"Type '"nope"' is not assignable to type '"Only"'."#,
    );
}

#[test]
fn inline_keyof_typeof_annotation_control_still_reduces() {
    // Positive control: the inline-annotation spelling already worked; the
    // alias fix must not disturb it.
    expect_code_with(
        r#"
enum Color { Red, Green }
const bad: keyof typeof Color = "nope";
"#,
        2322,
        r#"Type '"nope"' is not assignable to type '"Green" | "Red"'."#,
    );
}

#[test]
fn string_enum_keyof_typeof_alias_call_argument_reports_ts2345() {
    expect_code_with(
        r#"
enum StrE { A = "x", B = "y" }
type K = keyof typeof StrE;
declare function f(k: K): void;
f("zz");
"#,
        2345,
        r#"Argument of type '"zz"' is not assignable to parameter of type '"A" | "B"'."#,
    );
}

#[test]
fn string_enum_keyof_typeof_alias_call_argument_accepts_member_name() {
    expect_clean(
        r#"
enum StrE { A = "x", B = "y" }
type K = keyof typeof StrE;
declare function f(k: K): void;
f("A");
"#,
    );
}

#[test]
fn interface_alias_keyof_target_keeps_keyof_spelling_and_literal_source() {
    // Named type operand: the oracle keeps the `keyof I` spelling AND the
    // literal source (`"zz"`, not widened `string`).
    expect_code_with(
        r#"
interface I { a: number; c?: string }
type K = keyof I;
const x: K = "zz";
"#,
        2322,
        r#"Type '"zz"' is not assignable to type 'keyof I'."#,
    );
}

#[test]
fn inline_interface_keyof_target_control() {
    // Positive control: inline `keyof I` annotation — same oracle output as
    // the alias-mediated spelling above.
    expect_code_with(
        r#"
interface I { a: number; c?: string }
const x: keyof I = "zz";
"#,
        2322,
        r#"Type '"zz"' is not assignable to type 'keyof I'."#,
    );
}

#[test]
fn class_alias_keyof_target_keeps_keyof_spelling_and_literal_source() {
    expect_code_with(
        r#"
class C { m() {} n = 1 }
type K = keyof C;
const x: K = "zz";
"#,
        2322,
        r#"Type '"zz"' is not assignable to type 'keyof C'."#,
    );
}

#[test]
fn interface_alias_keyof_call_argument_known_residual_renders_reduced_union() {
    // KNOWN RESIDUAL: the pinned 7.0.2 oracle keeps the `keyof I` spelling
    // here (`...parameter of type 'keyof I'.`). tsz renders the reduced key
    // union because the parameter annotation `k: K` is evaluated to the
    // interned union when the signature is built, so by diagnostic time the
    // param type carries no `keyof` provenance to recover — and re-spelling a
    // bare literal union from a coincidental alias would repaint user-written
    // unions (the per-occurrence alias-identity residual already tracked on
    // the board). Fixing this needs the signature to preserve the written
    // alias reference, not a display-side patch. This pin asserts the current
    // behavior so a deliberate fix flips it consciously.
    expect_code_with(
        r#"
interface I { a: number; c?: string }
type K = keyof I;
declare function f(k: K): void;
f("zz");
"#,
        2345,
        r#"Argument of type '"zz"' is not assignable to parameter of type '"a" | "c"'."#,
    );
}

#[test]
fn enum_alias_use_before_enum_declaration_reduces() {
    // Hoisting order: the alias and its use precede the enum declaration in
    // source order, so nothing has walked the enum before the alias body is
    // forced.
    expect_code_with(
        r#"
type K = keyof typeof Later;
const bad: K = "nope";
enum Later { First, Second }
"#,
        2322,
        r#"Type '"nope"' is not assignable to type '"First" | "Second"'."#,
    );
}
