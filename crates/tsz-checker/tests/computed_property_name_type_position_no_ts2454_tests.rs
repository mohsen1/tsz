//! A computed property name (`[expr]`) on a type-only element — a member of an
//! `interface` declaration or a type literal — is a type-level reference: the
//! property key it computes has no runtime control-flow node. `tsc` therefore
//! does NOT run definite-assignment (TS2454) or temporal-dead-zone (TS2448)
//! analysis on a value referenced in the name (e.g. a `unique symbol` const).
//!
//! tsz used to flow-check those references and emit a false TS2454 for the
//! `[sym]` member of `interface I { [sym]: T }` (witness: the prop-types
//! `nominalTypeHack` pattern from `propTypeValidatorInference.ts`). The fix
//! classifies the reference by the container of its enclosing computed name
//! (`name -> member -> container`): interface / type literal => type position
//! (suppress TS2454/TS2448); object literal / class => value position (keep the
//! flow check). Narrowing still runs in both, so literal-typed keys keep their
//! literal type.
//!
//! Owner layer: checker flow analysis
//! (`flow/flow_analysis/usage.rs::reference_in_type_position_computed_name`).

use tsz_checker::test_utils::check_source_codes;

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

// ---------------------------------------------------------------------------
// Type positions: no TS2454 / TS2448 (binder names varied per case).
// ---------------------------------------------------------------------------

#[test]
fn unique_symbol_computed_key_in_interface_no_ts2454() {
    let codes = check_source_codes(
        r#"
export const sym: unique symbol;
export interface Box<T> { [sym]?: T; }
export {};
"#,
    );
    assert_eq!(count(&codes, 2454), 0, "got: {codes:?}");
    assert_eq!(count(&codes, 2448), 0, "got: {codes:?}");
}

/// Renamed binder (anti-hardcoding): the const is not named `sym`.
#[test]
fn unique_symbol_computed_key_in_interface_renamed_binder() {
    let codes = check_source_codes(
        r#"
export const brandMarker: unique symbol;
export interface Schema<V> { [brandMarker]?: V; }
export {};
"#,
    );
    assert_eq!(count(&codes, 2454), 0, "got: {codes:?}");
}

#[test]
fn unique_symbol_computed_key_in_type_literal_alias_no_ts2454() {
    let codes = check_source_codes(
        r#"
export const tag: unique symbol;
export type Tagged = { [tag]: number };
export {};
"#,
    );
    assert_eq!(count(&codes, 2454), 0, "got: {codes:?}");
}

/// Declaration order: interface references the const before it is declared.
/// The type-level key still must not trigger TS2454/TS2448.
#[test]
fn computed_key_before_const_declaration_no_flow_error() {
    let codes = check_source_codes(
        r#"
export interface Wrapper<T> { [marker]?: T; }
export const marker: unique symbol;
export {};
"#,
    );
    assert_eq!(count(&codes, 2454), 0, "got: {codes:?}");
    assert_eq!(count(&codes, 2448), 0, "got: {codes:?}");
}

/// Nested type literal in a method-signature return position.
#[test]
fn nested_type_literal_computed_key_no_ts2454() {
    let codes = check_source_codes(
        r#"
export const inner: unique symbol;
export interface Outer<T> { make(): { [inner]: T }; }
export {};
"#,
    );
    assert_eq!(count(&codes, 2454), 0, "got: {codes:?}");
}

/// Method signature (not a property signature) computed name.
#[test]
fn method_signature_computed_name_no_ts2454() {
    let codes = check_source_codes(
        r#"
export const callKey: unique symbol;
export interface Callable { [callKey](): void; }
export {};
"#,
    );
    assert_eq!(count(&codes, 2454), 0, "got: {codes:?}");
}

/// Qualified-name computed key (`[Ns.member]`): the inner identifier must also
/// escape flow analysis.
#[test]
fn qualified_name_computed_key_in_interface_no_ts2454() {
    let codes = check_source_codes(
        r#"
declare namespace Keys { const id: unique symbol; }
export interface Holder { [Keys.id]: string; }
export {};
"#,
    );
    assert_eq!(count(&codes, 2454), 0, "got: {codes:?}");
}

/// A literal-typed `as const` key must keep its literal type through the
/// suppressed flow path — narrowing is preserved, only the diagnostic is gated.
#[test]
fn literal_const_computed_key_keeps_literal_type() {
    let codes = check_source_codes(
        r#"
const key = "feature" as const;
interface Flags { [key]: boolean; }
declare let k: keyof Flags;
const lit: "feature" = k;
"#,
    );
    assert_eq!(count(&codes, 2454), 0, "got: {codes:?}");
    // keyof Flags must still be the literal "feature": no TS2322 on the assignment.
    assert_eq!(count(&codes, 2322), 0, "got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Value positions: TS2454 must STILL fire (negative controls).
// ---------------------------------------------------------------------------

/// An object-literal computed key is a runtime value reference: a `let` used
/// before assignment in that position must still report TS2454, matching `tsc`.
#[test]
fn object_literal_computed_key_still_reports_ts2454() {
    let codes = check_source_codes(
        r#"
let runtimeKey: symbol;
const obj = { [runtimeKey]: 1 };
runtimeKey = Symbol();
"#,
    );
    assert!(
        count(&codes, 2454) >= 1,
        "value-position computed key must keep TS2454; got: {codes:?}"
    );
}

/// A plain use-before-assignment elsewhere in the file must be unaffected.
#[test]
fn plain_use_before_assignment_still_reports_ts2454() {
    let codes = check_source_codes(
        r#"
let value: number;
const echoed = value;
value = 1;
"#,
    );
    assert!(
        count(&codes, 2454) >= 1,
        "ordinary use-before-assignment must keep TS2454; got: {codes:?}"
    );
}
