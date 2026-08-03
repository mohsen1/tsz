//! Regression tests for #16251: `ComputedPropertyName`'s `end` used to be read via
//! `token_end()` *after* `parse_expected(CloseBracketToken)` had already advanced the
//! scanner past `]`, so the node's span over-extended into whatever token followed the
//! `]` instead of stopping at it. `parse_property_name` is shared by class members,
//! object literal members, and interface/type-literal members, so one fix covers all
//! of them — these cases exercise that shared path from each container, plus a
//! non-trivial (call) expression form, not just an identifier chain.

use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::{assert_span, assert_span_on, parse_source};

#[test]
fn class_method_computed_name_ends_before_the_parameter_list() {
    // Before the fix, the node's span extended through the following `(`.
    let source = "class Foo { [Symbol.iterator]() {} }";
    assert_span(
        source,
        syntax_kind_ext::COMPUTED_PROPERTY_NAME,
        "[Symbol.iterator]",
    );
}

#[test]
fn class_bodyless_method_no_substitution_template_name_ends_before_the_parameter_list() {
    // The exact shape from #16251's own repro: `declare class C { get [`abc`](); }`.
    let source = "declare class C { get [`abc`](); }";
    assert_span(source, syntax_kind_ext::COMPUTED_PROPERTY_NAME, "[`abc`]");
}

#[test]
fn object_literal_computed_name_ends_before_the_colon() {
    let source = "const o = { [`abc`]: 1 };";
    assert_span(source, syntax_kind_ext::COMPUTED_PROPERTY_NAME, "[`abc`]");
}

#[test]
fn interface_computed_name_with_call_expression_ends_before_the_colon() {
    // A non-trivial expression form (call), not just an identifier/template.
    let source = "interface I { [foo()]: string; }";
    assert_span(source, syntax_kind_ext::COMPUTED_PROPERTY_NAME, "[foo()]");
}

#[test]
fn type_literal_computed_name_ends_before_the_semicolon() {
    let source = "type T = { [Symbol.iterator]: string };";
    assert_span(
        source,
        syntax_kind_ext::COMPUTED_PROPERTY_NAME,
        "[Symbol.iterator]",
    );
}

#[test]
fn two_computed_names_in_the_same_container_each_end_correctly() {
    // Guards against a fix that only works for the first occurrence, or that
    // leaks state between sibling members.
    let source = "class Foo { [a](): void {} [b](): void {} }";
    let (parser, _) = parse_source(source);
    for expected in ["[a]", "[b]"] {
        assert_span_on(
            &parser,
            source,
            syntax_kind_ext::COMPUTED_PROPERTY_NAME,
            expected,
        );
    }
}

#[test]
fn missing_close_bracket_recovery_does_not_regress() {
    // Error-recovery path (no `]`): `parse_expected` does not advance the scanner when
    // the expected token is absent, so this shape was never affected by the bug, but it
    // is worth pinning so a future change to the fix doesn't silently regress it.
    let source = "const o = { [a: 1 };";
    let (parser, _) = parse_source(source);
    assert!(
        !parser.get_diagnostics().is_empty(),
        "missing ']' should still be reported"
    );
}
