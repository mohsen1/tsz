#[test]
fn switch_clause_keyword_in_class_body_prefers_class_member_recovery() {
    let source = "class C {\n    case d = () => { yield 0; };\n}";
    let case_pos = source.find("case").expect("case") as u32;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();

    assert!(
        diags.iter().any(|diag| {
            diag.code
                == diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
                && diag.start == case_pos
        }),
        "expected TS1068 at misplaced `case`, got {diags:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::UNKNOWN_KEYWORD_OR_IDENTIFIER_DID_YOU_MEAN),
        "should not emit TS1435 suggestion after class-member recovery, got {diags:?}"
    );
}

#[test]
fn incomplete_constructor_return_type_reports_type_expected() {
    let source = "class C {\n    constructor(): }\n}";
    let close_brace_pos = source.find('}').expect("constructor close brace") as u32;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();

    assert!(
        diags.iter().any(|diag| {
            diag.code == diagnostic_codes::TYPE_EXPECTED && diag.start == close_brace_pos
        }),
        "expected TS1110 at missing constructor return type, got {diags:?}"
    );
    assert!(
        !diags.iter().any(|diag| {
            diag.code
                == diagnostic_codes::TYPE_ANNOTATION_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION
                && diag.start == close_brace_pos
        }),
        "constructor return-type recovery should not suppress TS1110 with TS1093 at the same position, got {diags:?}"
    );
}

#[test]
fn modifier_led_nested_class_member_recovery_prefers_ts1068_and_ts1128() {
    for source in [
        "class C {\n  public class D {\n}\n}",
        "class C {\n  public enum E {\n}\n}",
    ] {
        let (parser, _root) = parse_source(source);
        let diags = parser.get_diagnostics();
        let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(
                &diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
            ),
            "expected TS1068 for {source:?}, got {diags:?}"
        );
        assert!(
            codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
            "expected TS1128 for {source:?}, got {diags:?}"
        );
        assert!(
            !codes.contains(&diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER),
            "should not emit TS1434 after modifier-led nested declaration recovery for {source:?}, got {diags:?}"
        );
        assert!(
            !codes.contains(&diagnostic_codes::UNKNOWN_KEYWORD_OR_IDENTIFIER_DID_YOU_MEAN),
            "should not emit TS1435 after modifier-led nested declaration recovery for {source:?}, got {diags:?}"
        );
    }
}

#[test]
fn nested_class_recovery_does_not_treat_comment_close_brace_as_class_close() {
    let source = "class C {\n  // }\n  class D {}\n}\n";
    let comment_brace = source.find("// }").expect("comment brace") as u32 + 3;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();

    assert!(
        !diags.iter().any(|diag| {
            diag.code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED
                && diag.start == comment_brace
        }),
        "class-member recovery should only anchor TS1128 to real close-brace tokens, got {diags:?}"
    );
    assert!(
        diags.iter().any(|diag| {
            diag.code
                == diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
        }),
        "nested class declaration should still use class-member recovery, got {diags:?}"
    );
}

#[test]
fn modifier_led_try_block_in_class_body_prefers_ts1068() {
    let (parser, _root) = parse_source("class C {\n  public try {\n  }\n}");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(
            &diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
        ),
        "expected TS1068 for modifier-led try recovery, got {diags:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER),
        "should not emit TS1434 for modifier-led try recovery, got {diags:?}"
    );
}

#[test]
fn orphan_catch_recovery_inside_malformed_try_suppresses_lost_try_cascade() {
    let source = r#"
class Program {
    static Main() {
        try {
            var retValue: number = 0;
            if (retValue != 0 ^=  {
                return 1;
            }
             case  = call();
            if (retValue != 0) {
                return 1;
             ^
            retValue = call();
            if (retValue != 0) {
                return 1 &&
            }
            retValue = call ' );
            if (retValue != 0) {
                return 1;
            }
        }
        catch (e) {
        }
    }
}
"#;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "expected orphan catch recovery to report TS1005, got {diags:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::CATCH_OR_FINALLY_EXPECTED),
        "should not emit TS1472 after orphan catch recovery inside malformed try block, got {diags:?}"
    );
}

#[test]
fn bare_try_block_in_class_body_reparses_as_outer_statement() {
    let source = "class Foo {\n\n    try {\n\n        public bar = someInitThatMightFail();\n\n    } catch(e) {}\n\n\n\n    public baz() {\n\n        return this.bar;\n\n    }\n\n}\n";
    let (parser, _root) = parse_source(source);
    let line_map = LineMap::build(source);
    let diagnostics: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| {
            let pos = line_map.offset_to_position(diag.start, source);
            (
                diag.code,
                pos.line + 1,
                pos.character + 1,
                diag.message.as_str(),
            )
        })
        .collect();

    assert_eq!(
        diagnostics,
        vec![
            (
                diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
                3,
                5,
                "Unexpected token. A constructor, method, accessor, or property was expected.",
            ),
            (
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                5,
                9,
                "Declaration or statement expected.",
            ),
            (
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                11,
                5,
                "Declaration or statement expected.",
            ),
            (
                diagnostic_codes::EXPECTED,
                11,
                18,
                "';' expected.",
            ),
            (
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                17,
                1,
                "Declaration or statement expected.",
            ),
        ],
    );
}

#[test]
fn modifier_led_keyword_named_members_still_parse() {
    let (parser, _root) = parse_source("class C {\n  public class;\n  public enum() {}\n}");
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "valid keyword-named members should still parse after class-member recovery changes, got {diags:?}"
    );
}

#[test]
fn bare_var_statement_in_class_body_recovers_as_ts1068_then_ts1128() {
    let (parser, _root) = parse_source("class Foo2 {\n  var icecream = \"chocolate\";\n}");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![
            diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
        ],
        "bare variable statements in class bodies should recover as TS1068 + TS1128, got {diags:?}"
    );
}

#[test]
fn invalid_surrogate_escapes_in_class_member_names_report_ts1127() {
    let source = r"class C { \uD800\uDEA7: string; }";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();

    let invalid_positions: Vec<u32> = diags
        .iter()
        .filter(|d| d.code == diagnostic_codes::INVALID_CHARACTER)
        .map(|d| d.start)
        .collect();
    let backslash_positions: Vec<u32> = source
        .match_indices('\\')
        .map(|(pos, _)| pos as u32)
        .collect();

    assert_eq!(
        invalid_positions, backslash_positions,
        "invalid surrogate escapes should report TS1127 at each backslash, got {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code
            == diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED),
        "invalid surrogate escapes should not fall into class-member TS1068 recovery, got {diags:?}"
    );
}

#[test]
fn stray_at_before_enum_prefers_ts1109_over_decorator_recovery() {
    let source =
        "// @target: es2015\nnamespace M {\n   ¬\n   class C {\n   }\n   @\n   enum E {\n   ¬\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    let at_pos = source.find('@').unwrap() as u32;
    let enum_pos = source.find("enum E").unwrap() as u32;
    let eof_pos = source.len() as u32;
    assert!(
        codes.contains(&diagnostic_codes::INVALID_CHARACTER),
        "expected TS1127 for invalid characters, got {diags:?}"
    );
    let ts1109 = diags
        .iter()
        .find(|d| d.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for stray '@' before enum");
    assert_eq!(
        ts1109.start, enum_pos,
        "TS1109 should land on `enum`, not `@`: {diags:?}"
    );
    assert_ne!(
        ts1109.start, at_pos,
        "TS1109 should not be reported at the stray `@`: {diags:?}"
    );
    let ts1005 = diags
        .iter()
        .find(|d| d.code == diagnostic_codes::EXPECTED)
        .expect("expected TS1005 for the unclosed enum tail");
    assert_eq!(
        ts1005.start, eof_pos,
        "TS1005 should be emitted once at EOF for the missing `}}`: {diags:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::DECLARATION_EXPECTED),
        "should not emit TS1146 for stray '@' before enum, got {diags:?}"
    );
}

/// Test that 'await' as a label in a static block emits TS1109 (Expression expected),
/// matching tsc which treats `await` as a keyword in static blocks.
#[test]
fn test_await_label_in_static_block_emits_ts1109() {
    let source = r#"class C {
    static {
        await:
        break await;
    }
}"#;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();

    // Should emit TS1109 for 'await' as label in static block (matching tsc)
    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected TS1109 for 'await' as label in static block, got codes: {codes:?}"
    );
}
