//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — unicode escape recovery.

use crate::parser::test_fixture::{
    parse_source, parse_source_named, parse_source_with_language_version,
};
use tsz_common::ScriptTarget;
use tsz_common::diagnostics::diagnostic_codes;

#[test]
fn test_middle_dot_identifier_part_parses_without_ts1127() {
    let source = "const a·b = 1;\na·b;\n";
    let (parser, _root) = parse_source_named("middle-dot-identifier.ts", source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::INVALID_CHARACTER),
        "Expected U+00B7 to be accepted as an identifier continuation, got {diagnostics:?}"
    );
}

#[test]
fn test_invalid_unicode_escape_in_var_no_extra_semicolon_error() {
    let source = r"var arg\uxxxx";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert!(
        ts1127_count >= 1,
        "Expected TS1127 for invalid unicode escape, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1005_count, 0,
        "Expected no extra TS1005 for invalid unicode escape, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_invalid_unicode_escape_as_variable_name_no_var_decl_cascade() {
    let source = r"var \u0031a;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    let ts1123_count = diagnostics.iter().filter(|d| d.code == 1123).count();
    let ts1134_count = diagnostics.iter().filter(|d| d.code == 1134).count();

    assert!(
        ts1127_count >= 1,
        "Expected TS1127 for invalid unicode escape, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1123_count, 0,
        "Expected no TS1123 variable declaration cascade, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1134_count, 0,
        "Expected no TS1134 variable declaration cascade, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_escaped_combining_mark_as_variable_name_reports_ts1127() {
    let source = r"var \u0345 = 1;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 1127 && d.start == source.find('\\').unwrap() as u32),
        "Expected TS1127 at escaped combining mark, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn invalid_escaped_private_use_identifier_part_reports_ts1127() {
    let source = r"var _\uD4A5\u7204\uC316\uE59F = local;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let invalid_escape = source.find(r"\uE59F").expect("invalid escape") as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 1127 && d.start == invalid_escape),
        "Expected TS1127 at escaped private-use identifier part, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn invalid_surrogate_unicode_escapes_in_class_member_emit_ts1127() {
    let source = r"class C { \uD800\uDEA7: string; }";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let first_escape = source.find(r"\uD800").expect("first escape") as u32;
    let second_escape = source.find(r"\uDEA7").expect("second escape") as u32;
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::INVALID_CHARACTER, first_escape),
            (diagnostic_codes::INVALID_CHARACTER, second_escape),
        ],
        "invalid surrogate escapes in class member names should report scanner-shaped TS1127 diagnostics, got {diagnostics:?}",
    );
}

#[test]
fn invalid_surrogate_unicode_escapes_in_import_alias_emit_ts1127_without_cascade() {
    let source = r#"import { foo as \uD800\uDEA7 } from "mod";"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let first_escape = source.find(r"\uD800").expect("first escape") as u32;
    let second_escape = source.find(r"\uDEA7").expect("second escape") as u32;
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::INVALID_CHARACTER, first_escape),
            (diagnostic_codes::INVALID_CHARACTER, second_escape),
        ],
        "invalid surrogate escapes in import aliases should report scanner-shaped TS1127 diagnostics without parser cascades, got {diagnostics:?}",
    );
}

#[test]
fn es5_import_specifier_identifier_tail_reports_invalid_astral_without_comma_cascade() {
    let source = r#"import { _𐊧 as \uD800\uDEA7 } from "mod";"#;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let raw_astral = source.find('𐊧').expect("raw astral") as u32;
    let first_escape = source.find(r"\uD800").expect("first escape") as u32;
    let second_escape = source.find(r"\uDEA7").expect("second escape") as u32;
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::INVALID_CHARACTER, raw_astral),
            (diagnostic_codes::INVALID_CHARACTER, first_escape),
            (diagnostic_codes::INVALID_CHARACTER, second_escape),
        ],
        "ES5 import specifier invalid identifier tails should report scanner-shaped TS1127 diagnostics without comma recovery cascades, got {diagnostics:?}",
    );
}

#[test]
fn es5_astral_identifier_chars_recover_as_invalid_declaration_tail() {
    let source = "export var _𐊧 = new Foo();";
    let astral_pos = source.find('𐊧').expect("astral identifier char") as u32;
    let equals_pos = source.find('=').expect("equals") as u32;
    let new_pos = source.find("new").expect("new keyword") as u32;

    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::INVALID_CHARACTER && d.start == astral_pos),
        "ES5 astral identifier char must emit TS1127 at its source position, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(
            |d| d.code == diagnostic_codes::VARIABLE_DECLARATION_EXPECTED && d.start == equals_pos
        ),
        "ES5 astral identifier recovery must keep `=` visible for TS1134, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|d| d.code
            == diagnostic_codes::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME
            && d.start == new_pos),
        "ES5 astral identifier recovery must report TS1389 at `new`, got {diagnostics:?}"
    );
}

#[test]
fn es2015_astral_identifier_chars_remain_valid_identifier_parts() {
    let source = "export var _𐊧 = new Foo();";
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES2015);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::INVALID_CHARACTER),
        "ES2015 astral identifier chars should remain valid identifier parts, got {diagnostics:?}"
    );
}

#[test]
fn es2015_braced_astral_escape_remains_valid_identifier_start() {
    let source = r"export var \u{102A7} = new Foo();";
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES2015);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::INVALID_CHARACTER),
        "ES2015 braced astral identifier escape should scan as a valid identifier start, got {diagnostics:?}"
    );
}

#[test]
fn es5_braced_astral_escape_remains_invalid_identifier_start() {
    let source = r"export var \u{102A7} = new Foo();";
    let escape_pos = source.find(r"\u{102A7}").expect("unicode escape") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::INVALID_CHARACTER && d.start == escape_pos),
        "ES5 braced astral identifier escape should report TS1127 at the escape, got {diagnostics:?}"
    );
}

#[test]
fn es5_braced_astral_escape_after_identifier_recovers_inside_variable_list() {
    let source = r"export var _\u{102A7} = new Foo();";
    let escape_pos = source.find('\\').expect("unicode escape") as u32;
    let open_brace_pos = source.find('{').expect("open brace") as u32;
    let numeric_tail_pos = source.find("A7").expect("numeric literal tail") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::INVALID_CHARACTER, escape_pos),
            (diagnostic_codes::EXPECTED, open_brace_pos),
            (
                diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
                numeric_tail_pos,
            ),
        ],
        "ES5 escaped astral identifier tail should recover like tsc, got {diagnostics:?}"
    );
}

#[test]
fn es5_braced_astral_escape_after_identifier_recovers_across_same_line_trivia() {
    let source = r"export var _ /*tail*/ \u{102A7} = new Foo();";
    let escape_pos = source.find('\\').expect("unicode escape") as u32;
    let open_brace_pos = source.find('{').expect("open brace") as u32;
    let numeric_tail_pos = source.find("A7").expect("numeric literal tail") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::INVALID_CHARACTER, escape_pos),
            (diagnostic_codes::EXPECTED, open_brace_pos),
            (
                diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
                numeric_tail_pos,
            ),
        ],
        "same-line trivia before escaped astral debris should recover like tsc, got {diagnostics:?}"
    );
}

#[test]
fn es5_braced_astral_escape_after_import_alias_recovers_as_specifier_tail() {
    let source = r#"import { _x as _\u{102A7} } from "mod";"#;
    let escape_pos = source.find('\\').expect("unicode escape") as u32;
    let open_brace_pos = source.find(r"\u{102A7}").expect("unicode escape") as u32 + 2;
    let numeric_tail_pos = source.find("A7").expect("numeric literal tail") as u32;
    let close_brace_pos = source.find("} from").expect("specifier close brace") as u32;
    let from_pos = source.find("from").expect("from keyword") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::INVALID_CHARACTER, escape_pos),
            (diagnostic_codes::EXPECTED, open_brace_pos),
            (
                diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
                numeric_tail_pos,
            ),
            (
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                close_brace_pos,
            ),
            (
                diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                from_pos,
            ),
        ],
        "import alias escaped astral tail should recover like tsc, got {diagnostics:?}"
    );
}

#[test]
fn es5_braced_astral_escape_after_export_alias_recovers_as_specifier_tail() {
    let source = r#"export { _x as _\u{102A7} } from "mod";"#;
    let escape_pos = source.find('\\').expect("unicode escape") as u32;
    let open_brace_pos = source.find(r"\u{102A7}").expect("unicode escape") as u32 + 2;
    let numeric_tail_pos = source.find("A7").expect("numeric literal tail") as u32;
    let close_brace_pos = source.find("} from").expect("specifier close brace") as u32;
    let from_pos = source.find("from").expect("from keyword") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::INVALID_CHARACTER, escape_pos),
            (diagnostic_codes::EXPECTED, open_brace_pos),
            (
                diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
                numeric_tail_pos,
            ),
            (
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                close_brace_pos,
            ),
            (
                diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                from_pos,
            ),
        ],
        "export alias escaped astral tail should recover like tsc, got {diagnostics:?}"
    );
}

#[test]
fn reset_clears_braced_unicode_specifier_tail_recovery_state() {
    let source = r#"import { _x as _\u{102A7} } from "mod";"#;
    let (mut parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);
    assert!(
        parser.current_specifier_recovered_braced_unicode_escape_debris,
        "sanity check: first parse should exercise braced unicode specifier recovery"
    );

    parser.reset(
        "test.ts".to_string(),
        r#"import { value } from "mod";"#.to_string(),
    );

    assert!(
        !parser.current_specifier_recovered_braced_unicode_escape_debris,
        "reset should clear stale specifier recovery state"
    );
}

#[test]
fn es5_raw_astral_variable_name_reports_declaration_expected_at_type_tail() {
    let source = "declare var 𐊧: string;";
    let raw_astral = source.find('𐊧').expect("raw astral") as u32;
    let colon_pos = source.find(':').expect("colon") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert!(
        actual.contains(&(diagnostic_codes::INVALID_CHARACTER, raw_astral)),
        "ES5 raw astral declaration name should report TS1127 at the astral character, got {diagnostics:?}"
    );
    assert!(
        actual.contains(&(diagnostic_codes::VARIABLE_DECLARATION_EXPECTED, colon_pos)),
        "ES5 raw astral declaration recovery should report TS1134 at the type tail, got {diagnostics:?}"
    );
    assert!(
        !actual.contains(&(
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            colon_pos
        )),
        "ES5 raw astral declaration recovery should not reclassify the type tail as TS1128, got {diagnostics:?}"
    );
}

#[test]
fn es5_braced_astral_variable_name_reports_missing_comma_before_recovered_identifier() {
    let source = r"declare var \u{102A7}: string;";
    let escape_pos = source.find('\\').expect("unicode escape") as u32;
    let recovered_open_brace = source.find('{').expect("recovered open brace") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert!(
        actual.contains(&(diagnostic_codes::INVALID_CHARACTER, escape_pos)),
        "ES5 braced astral declaration name should report TS1127 at the escape, got {diagnostics:?}"
    );
    assert!(
        actual.contains(&(diagnostic_codes::EXPECTED, recovered_open_brace)),
        "ES5 braced astral declaration recovery should report TS1005 at the recovered braced tail, got {diagnostics:?}"
    );
    assert!(
        !actual.contains(&(diagnostic_codes::EXPECTED, escape_pos + 1)),
        "ES5 braced astral declaration recovery should not emit a duplicate TS1005 before the recovered identifier, got {diagnostics:?}"
    );
}

#[test]
fn es5_raw_astral_statement_assignment_reports_statement_expected_at_equals() {
    let source = "if (true) { 𐊧 = \"hello\"; }";
    let raw_astral = source.find('𐊧').expect("raw astral") as u32;
    let equals_pos = source.find('=').expect("equals") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert!(
        actual.contains(&(diagnostic_codes::INVALID_CHARACTER, raw_astral)),
        "ES5 raw astral statement assignment should report TS1127 at the astral character, got {diagnostics:?}"
    );
    assert!(
        actual.contains(&(
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            equals_pos
        )),
        "ES5 raw astral statement assignment should report TS1128 at the assignment tail, got {diagnostics:?}"
    );
}

#[test]
fn es5_braced_astral_statement_assignment_recovers_block_followed_by_equals() {
    let source = r#"if (true) { \u{102A7} = "hallo"; }"#;
    let escape_pos = source.find('\\').expect("unicode escape") as u32;
    let recovered_identifier = escape_pos + 1;
    let equals_pos = source.find('=').expect("equals") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert!(
        actual.contains(&(diagnostic_codes::INVALID_CHARACTER, escape_pos)),
        "ES5 braced astral statement assignment should report TS1127 at the escape, got {diagnostics:?}"
    );
    assert!(
        actual.contains(&(
            diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
            recovered_identifier
        )),
        "ES5 braced astral statement assignment should report TS1434 at the recovered identifier tail, got {diagnostics:?}"
    );
    assert!(
        actual.contains(&(
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED_THIS_FOLLOWS_A_BLOCK_OF_STATEMENTS_SO_IF_YOU_I,
            equals_pos
        )),
        "ES5 braced astral statement assignment should recover the braced tail as a block and report TS2809 at `=`, got {diagnostics:?}"
    );
}

#[test]
fn es2015_braced_astral_escape_remains_valid_in_class_and_member_access() {
    let source = r#"
class Foo {
    \u{102A7}: string;
    constructor() {
        this.\u{102A7} = " world";
    }
    methodA() {
        return this.\u{102A7};
    }
}
export var _\u{102A7} = new Foo().\u{102A7};
"#;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES2015);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::INVALID_CHARACTER),
        "ES2015 braced astral identifier escapes should remain valid across declarations, class members, and member access, got {diagnostics:?}"
    );
}

#[test]
fn es5_braced_astral_escape_reports_invalid_character_across_identifier_contexts() {
    let source = r#"
class Foo {
    \u{102A7}: string;
    constructor() {
        this.\u{102A7} = " world";
    }
}
export var _\u{102A7} = new Foo().\u{102A7};
"#;
    let expected_escape_positions: Vec<_> = source
        .match_indices(r"\u{102A7}")
        .map(|(pos, _)| pos as u32)
        .collect();
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    for escape_pos in expected_escape_positions {
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == diagnostic_codes::INVALID_CHARACTER && d.start == escape_pos),
            "ES5 braced astral identifier escape should report TS1127 at {escape_pos}, got {diagnostics:?}"
        );
    }
}

#[test]
fn es5_raw_astral_property_access_reports_invalid_character_but_preserves_name() {
    let source = "class Foo { methodA() { return this.𐊧; } }";
    let raw_astral = source.find('𐊧').expect("raw astral") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::INVALID_CHARACTER && d.start == raw_astral),
        "ES5 raw astral property access should report TS1127 at the property name, got {diagnostics:?}"
    );
}
