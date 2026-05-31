//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — template recovery.

use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::parse_source;

/// Structural rule: when a template literal appears where an object-literal
/// property name is expected, tsc closes the object literal at the template,
/// recovers the template as a tagged-template tail on the object expression,
/// and parses anything after the bogus `: value` as separate statements. The
/// recovered AST must therefore contain a tagged-template expression (not a
/// property assignment whose name is the template), and the diagnostics must
/// be the TS1136 / TS1005 / TS1134 / TS1128 cascade.
fn assert_template_property_recovers_as_tagged_template(source: &str) {
    let (parser, _root) = parse_source(source);
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![1136, 1005, 1134, 1128],
        "expected the TS1136/TS1005/TS1134/TS1128 cascade for {source:?}, got {:?}",
        parser.get_diagnostics()
    );

    // The template must have recovered as a tagged template on the object
    // expression, proving the object literal closed before it. If the parser
    // instead absorbed the template as a property name, no tagged-template
    // node would exist.
    let arena = parser.get_arena();
    let has_tagged_template = arena
        .nodes
        .iter()
        .any(|n| n.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION);
    assert!(
        has_tagged_template,
        "expected a tagged-template expression in the recovered AST for {source:?}; \
         the object literal should close at the template, not absorb it as a property name"
    );
}

#[test]
fn template_literal_property_name_recovers_as_tagged_template_short() {
    // `templateStringInPropertyName1`-style: the only member is a template.
    assert_template_property_recovers_as_tagged_template("var x = {\n    `a`: 321\n}\n");
}

#[test]
fn template_literal_property_name_recovers_as_tagged_template_after_member() {
    // `templateStringInObjectLiteral`-style: a valid member precedes the
    // template member. Use a different iteration-variable-free shape and a
    // different template spelling to prove the rule is structural, not keyed
    // on the `b` spelling.
    assert_template_property_recovers_as_tagged_template(
        "var x = {\n    a: `abc${ 123 }def`,\n    `b`: 321\n}\n",
    );
}

#[test]
fn template_literal_property_name_recovers_as_tagged_template_with_substitutions() {
    // `templateStringInPropertyName2`-style: a substitution template head/tail
    // (TemplateHead, not NoSubstitutionTemplateLiteral). The `:` is now far
    // from the TS1136 position, exercising the non-suppressed path.
    assert_template_property_recovers_as_tagged_template(
        "var x = {\n    `abc${ 123 }def${ 456 }ghi`: 321\n}\n",
    );
}

#[test]
fn template_literal_property_name_recovery_does_not_leak_into_later_declaration() {
    // A template-literal property name inside a non-declaration object literal
    // (a call argument) must not flip the recovery flag for a subsequent,
    // well-formed `var ... : Type` declaration. If the flag leaked, the later
    // `:` would be misreported as a missing comma (TS1005) instead of being a
    // valid type annotation.
    let source = "foo({ `a`: 1 });\nvar y: number = 2;\n";
    let (parser, _root) = parse_source(source);
    let later_decl_errors: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| (d.start as usize) >= source.find("var y").unwrap())
        .map(|d| d.code)
        .collect();
    assert!(
        later_decl_errors.is_empty(),
        "well-formed `var y: number = 2;` must not inherit template-property recovery; \
         got {later_decl_errors:?}"
    );
}

#[test]
fn test_ts1125_tagged_template_does_not_emit_errors() {
    // Tagged templates (ES2018+) allow invalid escape sequences per spec.
    // tsc does NOT emit TS1125 for tagged templates — only for untagged templates.
    let source =
        r#"const x = tag`\u{hello} ${ 100 } \xtraordinary ${ 200 } wonderful ${ 300 } \uworld`;"#;
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    let ts1125_diagnostics: Vec<_> = diagnostics.iter().filter(|d| d.code == 1125).collect();

    // Tagged templates should NOT get TS1125 errors
    assert_eq!(
        ts1125_diagnostics.len(),
        0,
        "Expected 0 TS1125 errors for tagged template, got {}: {:?}",
        ts1125_diagnostics.len(),
        ts1125_diagnostics
    );
}

#[test]
fn test_ts1125_untagged_template_emits_errors() {
    // Untagged templates with invalid escape sequences DO get TS1125.
    let source =
        r#"const y = `\u{hello} ${ 100 } \xtraordinary ${ 200 } wonderful ${ 300 } \uworld`;"#;
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    let ts1125_diagnostics: Vec<_> = diagnostics.iter().filter(|d| d.code == 1125).collect();

    // We should get 3 TS1125 errors (for \u{hello}, \xtraordinary, \uworld)
    assert_eq!(
        ts1125_diagnostics.len(),
        3,
        "Expected 3 TS1125 errors (for \\u{{hello}}, \\xtraordinary, \\uworld), got {}: {:?}",
        ts1125_diagnostics.len(),
        ts1125_diagnostics
    );
}
