//! Regression tests for optional/required property elaboration and
//! union-source failure elaboration (issue #10913).
//!
//! Two structural rules are exercised:
//!
//! 1. The TS2327 line ("Property 'x' is optional in type 'S' but required in
//!    type 'T'.") is only emitted when the property *types* are otherwise
//!    assignable, so optionality is the sole reason the relation fails. When
//!    the read types are themselves incompatible — the common strict-mode case
//!    where an optional `x?: T` contributes `T | undefined` that is not
//!    assignable to a required `x: T` — tsc reports the
//!    "Types of property 'x' are incompatible." chain that exposes the root
//!    mismatch, and tsz must do the same instead of collapsing it to TS2327.
//!
//! 2. A union **source** that is not assignable because one member fails is
//!    elaborated with the first failing member beneath the union-to-target
//!    line (`Type 'A | B' is not assignable to type 'T'.` ->
//!    `Type 'B' is not assignable to type 'T'.`), keeping the root mismatch
//!    visible rather than stopping at the bare union line.
//!
//! Both rules are structural (they hold for any property/identifier spelling),
//! so the matrix below varies the iteration-variable and property names.

use crate::context::CheckerOptions;
use crate::test_utils::{check_with_options, strict_checker_options};

/// Full elaboration text (primary message plus every related-information line)
/// of the single error with `code` in `source`, checked under strict options.
fn elaboration(source: &str, code: u32) -> String {
    elaboration_with(source, code, strict_checker_options())
}

fn elaboration_with(source: &str, code: u32, options: CheckerOptions) -> String {
    let diags = check_with_options(source, options);
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected exactly one TS{code}. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let mut lines = vec![matching[0].message_text.clone()];
    lines.extend(
        matching[0]
            .related_information
            .iter()
            .map(|info| info.message_text.clone()),
    );
    lines.join("\n")
}

/// An optional source property whose `| undefined` is not assignable to the
/// required target type must surface the type-incompatibility chain, not the
/// TS2327 optional/required shortcut.
#[test]
fn optional_incompatible_type_reports_type_chain_not_ts2327() {
    let text = elaboration(
        r#"
declare const x: { a?: number };
const r: { a: number } = x;
"#,
        2322,
    );
    assert!(
        text.contains("Types of property 'a' are incompatible."),
        "Expected the type-incompatibility chain. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'number | undefined' is not assignable to type 'number'."),
        "Expected the optional read type to appear. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'undefined' is not assignable to type 'number'."),
        "Expected the root `undefined` mismatch to be visible. Got: {text:?}"
    );
    assert!(
        !text.contains("is optional in type"),
        "TS2327 must not fire when the property types are incompatible. Got: {text:?}"
    );
}

/// When the target property type accepts `undefined`, optionality is the only
/// incompatibility, so TS2327 is the correct elaboration.
#[test]
fn optional_compatible_type_reports_ts2327() {
    let text = elaboration(
        r#"
declare const x: { a?: number };
const r: { a: number | undefined } = x;
"#,
        2322,
    );
    assert!(
        text.contains("is optional in type") && text.contains("but required in type"),
        "Expected the TS2327 optional/required line. Got: {text:?}"
    );
    assert!(
        !text.contains("Types of property 'a' are incompatible."),
        "Compatible property types must not emit the type-incompatibility chain. Got: {text:?}"
    );
}

/// Under exactOptionalPropertyTypes the optional read type is not widened with
/// `undefined`, so the types match and TS2327 is again correct.
#[test]
fn optional_exact_optional_property_types_reports_ts2327() {
    let mut options = strict_checker_options();
    options.exact_optional_property_types = true;
    let text = elaboration_with(
        r#"
declare const x: { a?: number };
const r: { a: number } = x;
"#,
        2322,
        options,
    );
    assert!(
        text.contains("is optional in type"),
        "Expected TS2327 under exactOptionalPropertyTypes. Got: {text:?}"
    );
}

/// The rule is independent of the property spelling.
#[test]
fn optional_incompatible_type_chain_is_name_independent() {
    for prop in ["a", "value", "weirdName"] {
        let src = format!(
            "declare const x: {{ {prop}?: number }};\nconst r: {{ {prop}: number }} = x;\n"
        );
        let text = elaboration(&src, 2322);
        assert!(
            text.contains(&format!("Types of property '{prop}' are incompatible."))
                && text.contains("Type 'undefined' is not assignable to type 'number'."),
            "Property '{prop}' should elaborate the root mismatch. Got: {text:?}"
        );
        assert!(
            !text.contains("is optional in type"),
            "Property '{prop}' must not collapse to TS2327. Got: {text:?}"
        );
    }
}

/// A direct union source elaborates the first failing member beneath the
/// union-to-target line.
#[test]
fn union_source_elaborates_failing_member() {
    let text = elaboration(
        r#"
declare const x: string | number;
const y: string = x;
"#,
        2322,
    );
    assert!(
        text.contains("Type 'string | number' is not assignable to type 'string'.")
            && text.contains("Type 'number' is not assignable to type 'string'."),
        "Expected the failing union member beneath the union line. Got: {text:?}"
    );
}

/// A required union-typed property elaborates its failing member nested beneath
/// the `Types of property` line.
#[test]
fn union_typed_property_elaborates_member() {
    let text = elaboration(
        r#"
declare const x: { a: number | undefined };
const r: { a: number } = x;
"#,
        2322,
    );
    assert!(
        text.contains("Types of property 'a' are incompatible.")
            && text.contains("Type 'number | undefined' is not assignable to type 'number'.")
            && text.contains("Type 'undefined' is not assignable to type 'number'."),
        "Expected the full union-member chain under the property line. Got: {text:?}"
    );
}

/// A homomorphic mapped type with a key-remapping `as` clause (the idiomatic
/// `Getters`/prefix-rename pattern) preserves the source optionality, and the
/// resulting optional/required mismatch elaborates the root, independent of the
/// iteration-variable name.
#[test]
fn renamed_mapped_getter_preserves_optional_and_elaborates_root() {
    for iter in ["K", "P", "Prop"] {
        let src = format!(
            r#"
type Src = {{ a?: number }};
type Rename<T> = {{ [{iter} in keyof T as `p_${{string & {iter}}}`]: T[{iter}] }};
declare const x: Rename<Src>;
const r: {{ p_a: number }} = x;
"#
        );
        let text = elaboration(&src, 2322);
        assert!(
            text.contains("Types of property 'p_a' are incompatible.")
                && text.contains("Type 'undefined' is not assignable to type 'number'."),
            "Iteration var '{iter}' should preserve optionality and elaborate the root. Got: {text:?}"
        );
    }
}

/// A two-level nested optional mismatch collapses the property path into a
/// single `The types of 'o.a' are incompatible between these types.` line, then
/// renders the union/member chain beneath it.
#[test]
fn nested_optional_collapses_property_path() {
    let text = elaboration(
        r#"
declare const x: { o: { a?: number } };
const r: { o: { a: number } } = x;
"#,
        2322,
    );
    assert!(
        text.contains("The types of 'o.a' are incompatible between these types."),
        "Expected the collapsed dotted-path line. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'undefined' is not assignable to type 'number'."),
        "Expected the root mismatch beneath the collapsed path. Got: {text:?}"
    );
}
