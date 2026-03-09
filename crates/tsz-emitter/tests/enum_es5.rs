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
        output.contains("E[E[\"A\"] = 3] = \"A\""),
        "Should constant-fold binary expression (1+2=3), got: {output}"
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

#[test]
fn test_cjs_exported_enum_iife_tail_folding() {
    // Verify that the IIFE tail `(E || (E = {}))` produced by emit_enum
    // can be folded into the CJS export form `(E || (exports.E = E = {}))`.
    // This matches tsc's compact output for `export enum E { ... }` under CommonJS.
    let output = emit_enum_legacy("enum E { A, B }");

    // The raw output should contain the plain IIFE tail (no exports binding)
    assert!(
        output.contains("(E || (E = {}))"),
        "Raw enum output should have plain IIFE tail, got: {output}"
    );

    // Apply the same string replacement used in transform_dispatch/module_emission_exports
    let name = "E";
    let from = format!("({name} || ({name} = {{}}))");
    let to = format!("({name} || (exports.{name} = {name} = {{}}))");
    let folded = output.replacen(&from, &to, 1);

    assert!(
        folded.contains("(E || (exports.E = E = {}))"),
        "Folded output should have CJS IIFE tail, got: {folded}"
    );
    // The replacement should only affect the IIFE tail, not the body
    assert!(
        folded.contains("E[E[\"A\"] = 0] = \"A\""),
        "Body should be unchanged after folding"
    );
}

#[test]
fn test_template_literal_enum_no_reverse_mapping() {
    // NoSubstitutionTemplateLiteral is syntactically string — no reverse mapping.
    // If A is a string literal and H = A, tsc folds H to the literal value "hello".
    let output = transform_enum("enum Foo { A = \"hello\", H = A }");
    assert!(
        output.contains("Foo[\"A\"] = \"hello\""),
        "String literal should not have reverse mapping, got: {output}"
    );
    assert!(
        output.contains("Foo[\"H\"] = \"hello\""),
        "Reference to string member should be folded to literal value, got: {output}"
    );
}

#[test]
fn test_string_concatenation_enum_no_reverse_mapping() {
    // "x" + expr is syntactically string — no reverse mapping
    let output = transform_enum("enum Foo { B = \"2\" + BAR }");
    assert!(
        output.contains("Foo[\"B\"] = \"2\" + BAR"),
        "String concat enum should not have reverse mapping, got: {output}"
    );
    assert!(
        !output.contains("Foo[Foo["),
        "Should not have reverse mapping pattern for string concat"
    );
}

#[test]
fn test_enum_member_self_reference_qualified() {
    // Sibling member references are constant-folded when evaluable (a=2, b=3, x=2+3=5)
    let output = transform_enum("enum Foo { a = 2, b = 3, x = a + b }");
    assert!(
        output.contains("Foo[Foo[\"x\"] = 5] = \"x\""),
        "Sibling member references should be constant-folded (2+3=5), got: {output}"
    );
}

#[test]
fn test_string_member_reference_no_reverse_mapping() {
    // H = A where A is string-valued — tsc folds to the literal value
    let output = transform_enum("enum Foo { A = \"alpha\", H = A }");
    assert!(
        output.contains("Foo[\"A\"] = \"alpha\""),
        "A should have no reverse mapping, got: {output}"
    );
    assert!(
        output.contains("Foo[\"H\"] = \"alpha\""),
        "H referencing string member A should be folded to literal value, got: {output}"
    );
}

#[test]
fn test_parenthesized_string_enum_no_reverse_mapping() {
    // Parenthesized string literal is still syntactically string
    let output = transform_enum("enum Foo { C = (\"hello\") }");
    assert!(
        !output.contains("Foo[Foo["),
        "Parenthesized string should not have reverse mapping, got: {output}"
    );
}

#[test]
fn test_numeric_enum_still_has_reverse_mapping() {
    // Numeric values should still get reverse mapping
    let output = transform_enum("enum Foo { F = BAR, G = 2 + BAR }");
    assert!(
        output.contains("Foo[Foo[\"F\"] = BAR] = \"F\""),
        "Non-string computed should have reverse mapping, got: {output}"
    );
    assert!(
        output.contains("Foo[Foo[\"G\"] = 2 + BAR] = \"G\""),
        "Numeric expression should have reverse mapping, got: {output}"
    );
}

#[test]
fn test_constant_folding_shift_operators() {
    // tsc evaluates 1 << 1 → 2, 1 << 2 → 4, etc.
    let output = transform_enum("enum E { A = 1 << 1, B = 1 << 2, C = 1 << 3 }");
    assert!(
        output.contains("E[E[\"A\"] = 2] = \"A\""),
        "1 << 1 should fold to 2, got: {output}"
    );
    assert!(
        output.contains("E[E[\"B\"] = 4] = \"B\""),
        "1 << 2 should fold to 4, got: {output}"
    );
    assert!(
        output.contains("E[E[\"C\"] = 8] = \"C\""),
        "1 << 3 should fold to 8, got: {output}"
    );
}

#[test]
fn test_constant_folding_member_reference() {
    // tsc resolves Color.Color to its numeric value
    let output = transform_enum("enum Color { Color, Thing = Color.Color }");
    assert!(
        output.contains("Color[Color[\"Color\"] = 0] = \"Color\""),
        "Auto-increment first member should be 0, got: {output}"
    );
    assert!(
        output.contains("Color[Color[\"Thing\"] = 0] = \"Thing\""),
        "Color.Color reference should fold to 0, got: {output}"
    );
}

#[test]
fn test_constant_folding_bitwise_ops() {
    let output = transform_enum("enum Flags { A = 1, B = 2, AB = A | B }");
    assert!(
        output.contains("Flags[Flags[\"AB\"] = 3] = \"AB\""),
        "A | B (1|2) should fold to 3, got: {output}"
    );
}

#[test]
fn test_constant_folding_complex_expression() {
    // (2 + 3) * 4 = 20
    let output = transform_enum("enum E { A = (2 + 3) * 4 }");
    assert!(
        output.contains("E[E[\"A\"] = 20] = \"A\""),
        "(2+3)*4 should fold to 20, got: {output}"
    );
}

#[test]
fn test_no_folding_for_non_constant_expressions() {
    // External function call cannot be folded
    let output = transform_enum("enum E { A = foo() }");
    assert!(
        output.contains("foo()"),
        "Non-constant expression should be preserved, got: {output}"
    );
}

#[test]
fn test_constant_folding_negative_values() {
    let output = transform_enum("enum E { A = -1, B = -2, C }");
    assert!(
        output.contains("E[E[\"A\"] = -1] = \"A\""),
        "Negative literal should be preserved, got: {output}"
    );
    assert!(
        output.contains("E[E[\"B\"] = -2] = \"B\""),
        "Negative literal should be preserved, got: {output}"
    );
    assert!(
        output.contains("E[E[\"C\"] = -1] = \"C\""),
        "Auto-increment after -2 should be -1, got: {output}"
    );
}
