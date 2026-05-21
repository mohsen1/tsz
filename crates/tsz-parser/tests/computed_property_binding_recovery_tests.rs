//! Parser recovery for missing identifier after a computed-property key
//! in object binding patterns: the parser must emit exactly one TS1003
//! at the close-brace position, without cascading into TS1109/TS1005,
//! and the rule must hold across every binding-pattern call site.
//!
//! Regression coverage for issue #8678
//! (`computedPropertyBindingElementDeclarationNoCrash1.ts`).

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

fn parse_codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    parser.parse_diagnostics.iter().map(|d| d.code).collect()
}

fn assert_single_identifier_expected(source: &str) {
    let codes = parse_codes(source);
    assert_eq!(
        codes,
        vec![diagnostic_codes::IDENTIFIER_EXPECTED],
        "expected single TS1003 for {source:?}, got {codes:?}"
    );
}

#[test]
fn const_decl_with_bare_computed_key_missing_name_emits_single_identifier_expected() {
    let source = "const { [k]: } = x;";
    let (parser, _root) = parse_source(source);
    let diagnostics: Vec<(u32, u32, String)> = parser
        .parse_diagnostics
        .iter()
        .map(|d| (d.code, d.start, d.message.clone()))
        .collect();
    let close_brace = source.find('}').expect("close brace") as u32;

    assert_eq!(
        diagnostics,
        vec![(
            diagnostic_codes::IDENTIFIER_EXPECTED,
            close_brace,
            "Identifier expected.".to_string(),
        )],
        "expected single TS1003 at the close-brace position, got {diagnostics:?}"
    );
}

#[test]
fn const_decl_with_dotted_computed_key_missing_name_emits_single_identifier_expected() {
    assert_single_identifier_expected(
        "const { [Symbol.iterator]: } = { [Symbol.iterator]: () => null };",
    );
}

#[test]
fn const_decl_with_call_computed_key_missing_name_emits_single_identifier_expected() {
    assert_single_identifier_expected(
        "declare const k: any; declare const x: any; \
         declare function computed(): any; \
         const { [computed()]: , a } = x;",
    );
}

#[test]
fn function_parameter_object_binding_with_computed_key_missing_name() {
    assert_single_identifier_expected("function f({ [k]: }) {}");
}

#[test]
fn for_of_head_object_binding_with_computed_key_missing_name() {
    assert_single_identifier_expected("declare const arr: any[]; for (const { [k]: } of arr) {}");
}

#[test]
fn nested_object_binding_with_inner_computed_key_missing_name() {
    assert_single_identifier_expected("declare const bar: any; const { foo: { [k]: } } = bar;");
}

#[test]
fn renamed_variable_proves_recovery_is_not_keyed_on_identifier_spelling() {
    for source in [
        "const { [k]: } = x;",
        "const { [KEY]: } = x;",
        "const { [slot]: } = x;",
    ] {
        assert_single_identifier_expected(source);
    }
}

#[test]
fn missing_name_at_inner_position_does_not_drop_outer_destructuring_binding() {
    // The trailing `extra` element must still parse — exactly one TS1003,
    // no cascading TS1005/TS1109 from a desync past the recovery point.
    assert_single_identifier_expected(
        "declare const k: any; declare const x: any; const { [k]: , extra } = x;",
    );
}

#[test]
fn well_formed_computed_binding_emits_no_diagnostics() {
    let source = "declare const k: any; declare const x: any; const { [k]: bound } = x;";
    let codes = parse_codes(source);
    assert!(
        codes.is_empty(),
        "expected zero parser diagnostics for well-formed computed binding, got {codes:?}"
    );
}
