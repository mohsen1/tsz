//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — template recovery.

use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::parse_source;

/// Collect the diagnostic codes for `source` in emission order.
fn diagnostic_codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

/// Structural rule: inside a template-literal *type*, each `${...}` substitution
/// is a full type expression terminated by `}`. tsc's `parseLiteralOfTemplateSpan`
/// only re-scans the brace as a template middle/tail when the substitution
/// actually closes with `}`. If a syntax error leaves the parser parked on some
/// other token, tsc emits `'}' expected` (TS1005) and synthesizes a *missing*
/// `TemplateTail` without consuming the token — so the rest of the source is
/// recovered from its real position. tsz must not blindly re-scan a template
/// token from a non-`}` position, which would reinterpret unrelated source as
/// template text and lose token boundaries during recovery.
///
/// `type R = ` + "`${A B}`" closes the substitution after `A`; the stray `B`
/// must trigger the `TS1005` / `TS1128` / `TS1160` recovery cascade exactly as
/// tsc produces, rather than being silently absorbed as template text.
#[test]
fn template_type_substitution_unterminated_by_stray_token_recovers_like_tsc() {
    assert_eq!(
        diagnostic_codes("type R = `${A B}`;"),
        vec![1005, 1128, 1160],
        "stray token after a template-type substitution type must emit the \
         '}}' expected / declaration-expected / unterminated-template cascade"
    );
}

/// The same rule must hold regardless of the binder spelling and across a
/// statement-terminator stray token, proving the recovery is structural and not
/// keyed on a particular identifier or token.
#[test]
fn template_type_substitution_stray_token_recovery_is_structural() {
    // A different binder name and a `;` (instead of an identifier) as the stray
    // token: still `'}' expected` + declaration-expected + unterminated.
    assert_eq!(
        diagnostic_codes("type Mapped = `${Outer;}`;"),
        vec![1005, 1128, 1160],
    );
}

/// A failed type parse inside the substitution (no valid type at all) reports
/// `Type expected` (TS1110) at the offending token; tsc's same-position
/// deduplication then suppresses the redundant `'}' expected`, leaving the
/// declaration-expected / unterminated cascade. The previous tsz behavior
/// swallowed everything after the bogus substitution into a phantom tail and
/// emitted only the single TS1110.
#[test]
fn template_type_substitution_empty_or_invalid_recovers_like_tsc() {
    assert_eq!(diagnostic_codes("type X = `${,}`;"), vec![1110, 1128, 1160]);
    // A bare, well-formed empty substitution `${}` reports only `Type expected`,
    // since the `}` legitimately closes the (empty) substitution.
    assert_eq!(diagnostic_codes("type Y = `${}`;"), vec![1110]);
}

/// Multi-span template types must recover at the first broken span and not
/// cascade phantom tails through the remaining spans.
#[test]
fn template_type_multi_span_recovers_at_first_broken_span() {
    assert_eq!(
        diagnostic_codes("type Q = `pre${A}mid${,}post`;"),
        vec![1110, 1128, 1160],
    );
}

/// Regression guard: a *valid* constrained-`infer` template-literal type in a
/// conditional `extends` branch — the exact surface family in the originating
/// benchmark row — must still parse with zero diagnostics. The fix only changes
/// the error-recovery branch, so the well-formed grammar stays untouched.
#[test]
fn template_type_constrained_infer_extends_branch_parses_clean() {
    for source in [
        "type Foo<T> = T extends `${infer A extends string}` ? A : never;",
        "type Split<S> = S extends `${infer H extends string}${infer R}` ? H : never;",
        "type Joined = `a${string}b${number}c`;",
        "type Nested<T> = T extends `x${T extends string ? `${infer A}` : never}y` ? A : never;",
    ] {
        assert!(
            diagnostic_codes(source).is_empty(),
            "expected no parse errors for {source:?}, got {:?}",
            diagnostic_codes(source),
        );
    }
}

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
