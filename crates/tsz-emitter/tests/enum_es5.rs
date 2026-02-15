use super::*;
use tsz_parser::parser::ParserState;

fn transform_enum(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&enum_idx) = source_file.statements.nodes.first()
    {
        let mut transformer = EnumES5Transformer::new(&parser.arena);
        if let Some(ir) = transformer.transform_enum(enum_idx) {
            return IRPrinter::emit_to_string(&ir);
        }
    }
    String::new()
}

fn emit_enum_legacy(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&enum_idx) = source_file.statements.nodes.first()
    {
        let mut emitter = EnumES5Emitter::new(&parser.arena);
        return emitter.emit_enum(enum_idx);
    }
    String::new()
}

#[test]
fn test_numeric_enum() {
    let output = transform_enum("enum E { A, B, C }");
    assert!(output.contains("var E;"), "Should declare var E");
    assert!(output.contains("(function (E)"), "Should have IIFE");
    assert!(
        output.contains("E[E[\"A\"] = 0] = \"A\""),
        "Should have reverse mapping for A"
    );
    assert!(
        output.contains("E[E[\"B\"] = 1] = \"B\""),
        "Should have reverse mapping for B"
    );
    assert!(
        output.contains("E[E[\"C\"] = 2] = \"C\""),
        "Should auto-increment C"
    );
}

#[test]
fn test_enum_with_initializer() {
    let output = transform_enum("enum E { A = 10, B, C = 20 }");
    assert!(
        output.contains("E[E[\"A\"] = 10] = \"A\""),
        "A should be 10"
    );
    assert!(
        output.contains("E[E[\"B\"] = 11] = \"B\""),
        "B should be 11 (auto-increment)"
    );
    assert!(
        output.contains("E[E[\"C\"] = 20] = \"C\""),
        "C should be 20"
    );
}

#[test]
fn test_string_enum() {
    let output = transform_enum("enum S { A = \"alpha\", B = \"beta\" }");
    assert!(output.contains("var S;"), "Should declare var S");
    assert!(
        output.contains("S[\"A\"] = \"alpha\";"),
        "String enum no reverse mapping"
    );
    assert!(
        output.contains("S[\"B\"] = \"beta\";"),
        "String enum no reverse mapping"
    );
    // Should NOT contain reverse mapping pattern
    assert!(
        !output.contains("S[S["),
        "String enums should not have reverse mapping"
    );
}

#[test]
fn test_const_enum_erased() {
    let output = transform_enum("const enum CE { A = 0 }");
    assert!(
        output.trim().is_empty(),
        "Const enums should be erased: {output}"
    );
}

#[test]
fn test_legacy_emitter_produces_same_output() {
    // Test that the legacy wrapper produces the same output
    let new_output = transform_enum("enum E { A, B = 2 }");
    let legacy_output = emit_enum_legacy("enum E { A, B = 2 }");
    assert_eq!(
        new_output, legacy_output,
        "Legacy and new output should match"
    );
}

#[test]
fn test_enum_with_binary_expression() {
    let output = transform_enum("enum E { A = 1 + 2, B }");
    assert!(output.contains("var E;"), "Should declare var E");
    assert!(
        output.contains("E[E[\"A\"] = 1 + 2] = \"A\""),
        "Should handle binary expression"
    );
    assert!(
        output.contains("E[E[\"B\"] = 4] = \"B\""),
        "Should auto-increment after computed value (A=3, so B=4)"
    );
}

#[test]
fn test_enum_with_unary_expression() {
    let output = transform_enum("enum E { A = -5 }");
    assert!(output.contains("var E;"), "Should declare var E");
    assert!(
        output.contains("E[E[\"A\"] = -5] = \"A\""),
        "Should handle unary expression"
    );
}

#[test]
fn test_enum_with_property_access() {
    let output = transform_enum("enum E { A = E.B }");
    assert!(output.contains("var E;"), "Should declare var E");
    // Property access should be preserved
    assert!(output.contains("E.B"), "Should preserve property access");
}
