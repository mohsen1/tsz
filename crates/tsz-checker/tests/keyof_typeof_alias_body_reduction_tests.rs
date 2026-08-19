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
fn interface_alias_keyof_call_argument_keeps_keyof_spelling() {
    // The parameter annotation `k: K` keeps the deferred `keyof I` type all
    // the way to diagnostic display (`symbol_types.rs`'s
    // `preserve_deferred_keyof` already avoids reducing it at signature-build
    // time). The display gateway's named-operand fallback just didn't unwrap
    // a still-deferred interface/class operand reference, so it fell through
    // to a reduced-key-union display: fixed in
    // `keyof_type_alias_definition_display`.
    expect_code_with(
        r#"
interface I { a: number; c?: string }
type K = keyof I;
declare function f(k: K): void;
f("zz");
"#,
        2345,
        r#"Argument of type '"zz"' is not assignable to parameter of type 'keyof I'."#,
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

#[test]
fn generic_mapped_remap_alias_keyof_target_widens_source_keeps_keyof_spelling() {
    // The operand is a mapped type with key remapping over a
    // generic-dependent source (`keyof (Base & U)`), so the key set is
    // deferred. The 7.0.2 oracle widens the assignment-source literal at the
    // head and keeps the `keyof` spelling (`keyRemappingKeyofResult.ts`) —
    // the partially evaluated key set must NOT be treated as a literal
    // context even though its visible members are all unit types.
    expect_code_with(
        r#"
type Rec<K2 extends keyof any, V> = { [Q in K2]: V };
function fx<U>() {
    type Base = { [k: string]: any, alpha: any, beta: any } & U;
    type Pruned = { [P in keyof Base as {} extends Rec<P, any> ? never : P]: any };
    type Keys = keyof Pruned;
    let v: Keys;
    v = "other";
}
"#,
        2322,
        r#"Type 'string' is not assignable to type 'keyof Pruned'."#,
    );
}

#[test]
fn distributive_generic_mapped_remap_alias_keyof_target_widens_source() {
    // Same deferred-key-set rule through a distributive remapping conditional
    // (`getIndexType`'s other branch in the tsc original).
    expect_code_with(
        r#"
type Rec<K2 extends keyof any, V> = { [Q in K2]: V };
function gx<W>() {
    type Base2 = { [k: string]: any, gamma: any } & W;
    type NonIdx<Q extends keyof any> = {} extends Rec<Q, any> ? never : Q;
    type DistNonIdx<Q extends keyof any> = Q extends unknown ? NonIdx<Q> : never;
    type Pruned2 = { [P in keyof Base2 as DistNonIdx<P>]: any };
    type Keys2 = keyof Pruned2;
    let w: Keys2;
    w = "whatever";
}
"#,
        2322,
        r#"Type 'string' is not assignable to type 'keyof Pruned2'."#,
    );
}

#[test]
fn concrete_mapped_remap_alias_keyof_target_evaluates_to_literal_keyset() {
    // For a CONCRETE mapped type with key remapping, the 7.0.2 oracle
    // evaluates `keyof` fully to the literal key union — unlike the
    // generic-dependent form (`generic_mapped_remap_alias_keyof_target_widens_source_keeps_keyof_spelling`),
    // which stays deferred and keeps the `keyof Name` spelling.
    expect_code_with(
        r#"
type Rec<K2 extends keyof any, V> = { [Q in K2]: V };
type CBase = { [k: string]: any, alpha: any, beta: any };
type CPruned = { [P in keyof CBase as {} extends Rec<P, any> ? never : P]: any };
type CKeys = keyof CPruned;
declare let c: CKeys;
c = "other";
"#,
        2322,
        r#"Type '"other"' is not assignable to type '"alpha" | "beta"'."#,
    );
}
