//! Regression tests for issue #16213: a string-literal member name must keep
//! its own source quote character in the implicit-any diagnostics
//! (TS7008/TS7010/TS7032/TS7033), instead of being re-quoted to a fixed
//! convention (or, for TS7008, losing its quotes altogether).
//!
//! Structural rule: `tsc`'s `declarationNameToString` is `getTextOfNode` — the
//! name node's verbatim source spelling, whichever quote character the author
//! wrote. tsz's `get_member_name_display_text` (TS7008's direct caller) forced
//! single quotes around the literal's unquoted value, and `property_name_for_error`
//! (TS7010/7032/7033's caller) resolved the semantic *key* via `get_property_name`
//! first — correct for identity, but bare and unquoted, which is wrong for a
//! message. Every expected string below is pinned against an in-container
//! `typescript@7.0.2` oracle (`--noEmit --strict --pretty false --target es2022
//! --lib es2022`).

use crate::test_utils::{check_source, diagnostic_code_messages};
use tsz_common::checker_options::CheckerOptions;

fn messages(source: &str) -> Vec<(u32, String)> {
    let diags = check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );
    diagnostic_code_messages(diags)
}

#[test]
fn ts7008_double_quoted_class_property_keeps_double_quotes() {
    assert_eq!(
        messages(r#"declare class C1 { "foo"; }"#),
        vec![(
            7008,
            "Member '\"foo\"' implicitly has an 'any' type.".to_string()
        )]
    );
}

#[test]
fn ts7008_single_quoted_class_property_keeps_single_quotes() {
    // Verbatim source spelling is `'foo'`; the message template's own `'{0}'`
    // wrap around that produces the doubled quote marks — this is tsc's own
    // rendering for this input, not a bug.
    assert_eq!(
        messages("declare class C2 { 'foo'; }"),
        vec![(
            7008,
            "Member ''foo'' implicitly has an 'any' type.".to_string()
        )]
    );
}

#[test]
fn ts7008_embedded_escaped_quote_preserved_verbatim() {
    assert_eq!(
        messages(r#"declare class C3 { "a\"b"; }"#),
        vec![(
            7008,
            "Member '\"a\\\"b\"' implicitly has an 'any' type.".to_string()
        )]
    );
}

#[test]
fn ts7008_plain_identifier_property_unaffected() {
    assert_eq!(
        messages("declare class C4 { foo; }"),
        vec![(
            7008,
            "Member 'foo' implicitly has an 'any' type.".to_string()
        )]
    );
}

#[test]
fn ts7008_numeric_property_unaffected() {
    assert_eq!(
        messages("declare class C5 { 0x10; }"),
        vec![(
            7008,
            "Member '0x10' implicitly has an 'any' type.".to_string()
        )]
    );
}

#[test]
fn ts7033_getter_in_declare_class_keeps_source_quotes() {
    assert_eq!(
        messages(r#"declare class C6 { get "foo"(); }"#),
        vec![(
            7033,
            "Property '\"foo\"' implicitly has type 'any', because its get accessor lacks a return type annotation."
                .to_string()
        )]
    );
}

#[test]
fn ts7033_getter_in_interface_keeps_source_quotes() {
    assert_eq!(
        messages(r#"interface I7 { get "foo"(); }"#),
        vec![(
            7033,
            "Property '\"foo\"' implicitly has type 'any', because its get accessor lacks a return type annotation."
                .to_string()
        )]
    );
}

#[test]
fn ts7032_setter_in_declare_class_keeps_source_quotes() {
    let msgs = messages(r#"declare class C8 { set "foo"(v); }"#);
    assert!(
        msgs.contains(&(
            7032,
            "Property '\"foo\"' implicitly has type 'any', because its set accessor lacks a parameter type annotation."
                .to_string()
        )),
        "unexpected diagnostics: {msgs:?}"
    );
}

#[test]
fn ts7010_bodyless_method_in_declare_class_keeps_source_quotes() {
    assert_eq!(
        messages(r#"declare class C9 { "foo"(); }"#),
        vec![(
            7010,
            "'\"foo\"', which lacks return-type annotation, implicitly has an 'any' return type."
                .to_string()
        )]
    );
}

#[test]
fn annotated_get_set_pair_in_type_literal_stays_clean_control() {
    // Both accessors are annotated, so neither TS7032 nor TS7033 fires — this
    // control proves the fix does not spuriously introduce a diagnostic on an
    // already-typed string-literal-named accessor pair.
    assert_eq!(
        messages(r#"type T10 = { get "foo"(): number; set "foo"(v: number); };"#),
        Vec::<(u32, String)>::new()
    );
}
