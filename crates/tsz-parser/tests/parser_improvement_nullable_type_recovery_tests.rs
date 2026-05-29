//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — nullable type recovery.

use crate::parser::test_fixture::{parse_source, parse_source_named};

#[test]
fn test_postfix_question_emits_ts17019() {
    // `string?` should emit TS17019, not TS1005 or TS1110
    let source = "let x: string?;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts17019_count = diagnostics.iter().filter(|d| d.code == 17019).count();
    assert!(
        ts17019_count >= 1,
        "Expected TS17019 for postfix '?' on type, got diagnostics: {diagnostics:?}"
    );
    // Should NOT emit TS1005 or TS1110 cascade
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
    let ts1110_count = diagnostics.iter().filter(|d| d.code == 1110).count();
    assert_eq!(
        ts1005_count, 0,
        "Should not emit TS1005 for nullable type, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1110_count, 0,
        "Should not emit TS1110 for nullable type, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_prefix_question_emits_ts17020() {
    // `?string` should emit TS17020, not TS1110
    let source = "let x: ?string;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts17020_count = diagnostics.iter().filter(|d| d.code == 17020).count();
    assert!(
        ts17020_count >= 1,
        "Expected TS17020 for prefix '?' on type, got diagnostics: {diagnostics:?}"
    );
    // Should NOT emit TS1110 cascade
    let ts1110_count = diagnostics.iter().filter(|d| d.code == 1110).count();
    assert_eq!(
        ts1110_count, 0,
        "Should not emit TS1110 for nullable type, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_prefix_question_simplifies_ts17020_suggestions() {
    for (input, expected) in [
        ("unknown", "unknown"),
        ("never", "never"),
        ("void", "void"),
        ("undefined", "null | undefined"),
        ("null", "null | undefined"),
        ("number", "number | null | undefined"),
    ] {
        let source = format!("let x: ?{input};");
        let (parser, _root) = parse_source_named(&format!("{input}.ts"), &source);

        let diagnostic = parser
            .get_diagnostics()
            .iter()
            .find(|d| d.code == 17020)
            .unwrap_or_else(|| {
                panic!(
                    "Expected TS17020 for ?{input}, got {:?}",
                    parser.get_diagnostics()
                )
            });
        assert_eq!(
            diagnostic.message,
            format!(
                "'?' at the start of a type is not valid TypeScript syntax. Did you mean to write '{expected}'?"
            ),
            "wrong TS17020 suggestion for ?{input}"
        );
    }
}

#[test]
fn test_multiple_nullable_types() {
    // Multiple nullable types in different positions
    let source = r"
function f(x: string?): ?number {
    return null;
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts17019_count = diagnostics.iter().filter(|d| d.code == 17019).count();
    let ts17020_count = diagnostics.iter().filter(|d| d.code == 17020).count();
    assert!(
        ts17019_count >= 1,
        "Expected at least 1 TS17019 for postfix '?', got diagnostics: {diagnostics:?}"
    );
    assert!(
        ts17020_count >= 1,
        "Expected at least 1 TS17020 for prefix '?', got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_nullable_type_in_type_predicate() {
    // `x is ?string` should emit TS17020
    let source = "function f(x: any): x is ?string { return true; }";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts17020_count = diagnostics.iter().filter(|d| d.code == 17020).count();
    assert!(
        ts17020_count >= 1,
        "Expected TS17020 for '?string' in type predicate, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_nullable_type_no_cascade() {
    // Nullable type should not cause cascading errors
    let source = r#"
let a: string? = "hello";
let b: ?number = 42;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    // Should only have TS17019 and TS17020, no cascade
    let cascade_codes: Vec<u32> = diagnostics
        .iter()
        .filter(|d| d.code == 1005 || d.code == 1109 || d.code == 1110 || d.code == 1128)
        .map(|d| d.code)
        .collect();
    assert!(
        cascade_codes.is_empty(),
        "Nullable types should not cause cascading errors, got: {cascade_codes:?}. All: {diagnostics:?}"
    );
}

#[test]
fn test_invalid_nonnullable_type_recovery_reports_ts17019_and_ts17020() {
    let source = r#"
function f1(a: string): a is string! { return true; }
function f2(a: string): a is !string { return true; }
const a = 1 as any!;
const b = 1 as !any;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![17019, 17020, 17019, 17020],
        "Expected TS17019/TS17020 recovery for invalid non-nullable type syntax, got diagnostics: {diagnostics:?}"
    );
}
