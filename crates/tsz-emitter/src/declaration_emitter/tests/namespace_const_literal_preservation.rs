//! Declaration emit: a `const` whose initializer is a property-access into a
//! namespace value member (`N.deep`, `N.M.deep`) preserves the member's fresh
//! literal initializer (`= 1`) the same way a plain identifier alias
//! (`const c = a`) does, instead of degrading to the widened type annotation
//! (`: number`).
//!
//! Owner: `crates/tsz-emitter/src/declaration_emitter/helpers/literal_initializers.rs`
//! (`const_literal_initializer_text_deep_inner` `PropertyAccess` branch +
//! `const_variable_initializer_for_symbol` annotation guard).
//!
//! Regression: #14772.

use super::*;

/// `export const x = N.deep` where `deep` is a fresh-literal `const` member:
/// tsc preserves `= 1`. Binder names are varied across cases so no fixture
/// name drives the logic.
#[test]
fn namespace_const_member_number_literal_is_preserved() {
    let output =
        emit_dts_with_binding("namespace N { export const deep = 1; }\nexport const x = N.deep;");
    assert!(
        output.contains("export declare const x = 1;"),
        "expected namespace member literal to be inlined as `= 1`: {output}"
    );
    assert!(
        !output.contains("const x: number"),
        "must not degrade to the widened `: number` annotation: {output}"
    );
}

/// Deeply-qualified member (`N.M.deep`) resolves through the nested namespace
/// export chain and preserves its literal.
#[test]
fn nested_namespace_const_member_number_literal_is_preserved() {
    let output = emit_dts_with_binding(
        "namespace Outer { export namespace Inner { export const depth = 5; } }\nexport const value = Outer.Inner.depth;",
    );
    assert!(
        output.contains("export declare const value = 5;"),
        "expected nested namespace member literal to be inlined as `= 5`: {output}"
    );
    assert!(
        !output.contains("const value: number") && !output.contains("const value: any"),
        "must not degrade to `: number` / `: any`: {output}"
    );
}

/// String and boolean literal members preserve `= "hi"` / `= true`.
#[test]
fn namespace_const_member_string_and_boolean_literals_are_preserved() {
    let string_output = emit_dts_with_binding(
        "namespace Conf { export const label = \"hi\"; }\nexport const tag = Conf.label;",
    );
    assert!(
        string_output.contains("export declare const tag = \"hi\";"),
        "expected string member literal `= \"hi\"`: {string_output}"
    );

    let bool_output = emit_dts_with_binding(
        "namespace Flags { export const enabled = true; }\nexport const active = Flags.enabled;",
    );
    assert!(
        bool_output.contains("export declare const active = true;"),
        "expected boolean member literal `= true`: {bool_output}"
    );
}

/// A consumer declared *inside* another namespace preserves the member literal
/// (`const x = 1;` rather than `const x: number;`).
#[test]
fn namespace_const_member_preserved_when_consumer_is_namespace_local() {
    let output = emit_dts_with_binding(
        "namespace Src { export const seed = 1; }\nnamespace Dst { export const x = Src.seed; }",
    );
    assert!(
        output.contains("const x = 1;"),
        "expected namespace-local consumer to inline `= 1`: {output}"
    );
    assert!(
        !output.contains("const x: number"),
        "must not degrade to `: number`: {output}"
    );
}

/// Control: an *annotated* member (`export const deep: number = 1`) widens the
/// declared type, so the reference must keep the `: number` annotation — the
/// literal-vs-annotated-member distinction.
#[test]
fn annotated_namespace_const_member_keeps_widened_annotation() {
    let output = emit_dts_with_binding(
        "namespace N { export const deep: number = 1; }\nexport const x = N.deep;",
    );
    assert!(
        output.contains("export declare const x: number;"),
        "an annotated member must keep `: number`, not inline `= 1`: {output}"
    );
    assert!(
        !output.contains("const x = 1;"),
        "must not inline an annotated member's initializer: {output}"
    );
}

/// A non-`const` member (`export let`) widens to its base type, so the
/// reference is annotated, not inlined.
#[test]
fn non_const_namespace_member_is_not_inlined() {
    let output =
        emit_dts_with_binding("namespace N { export let deep = 1; }\nexport const x = N.deep;");
    assert!(
        output.contains("export declare const x: number;"),
        "a `let` member widens, so the reference keeps `: number`: {output}"
    );
    assert!(
        !output.contains("const x = 1;"),
        "must not inline a non-const member: {output}"
    );
}

/// Adjacent: a plain identifier alias of a fresh literal is still preserved
/// (do not regress the #3449 behavior).
#[test]
fn identifier_alias_of_fresh_literal_is_still_preserved() {
    let output = emit_dts_with_binding("const a = 1;\nexport const c = a;");
    assert!(
        output.contains("export declare const c = 1;"),
        "identifier alias of a fresh literal must stay inlined `= 1`: {output}"
    );
}

/// Broad fix: an identifier alias of an *annotated* const must fall back to the
/// widened annotation (`: number`), matching tsc — previously tsz wrongly
/// inlined `= 1`.
#[test]
fn identifier_alias_of_annotated_const_keeps_widened_annotation() {
    let output = emit_dts_with_binding("const a: number = 1;\nexport const c = a;");
    assert!(
        output.contains("export declare const c: number;"),
        "alias of an annotated const must keep `: number`, not inline `= 1`: {output}"
    );
    assert!(
        !output.contains("const c = 1;"),
        "must not inline the annotated const's initializer through an alias: {output}"
    );
}

/// Coexistence: an enum-member access (`E.A`) is still rendered as the member
/// reference `= E.A` (handled by the enum-access path, not the new namespace
/// branch).
#[test]
fn enum_member_access_still_renders_member_reference() {
    let output = emit_dts_with_binding("enum E { A, B }\nexport const a = E.A;");
    assert!(
        output.contains("export declare const a = E.A;"),
        "enum-member access must stay `= E.A`: {output}"
    );
}
