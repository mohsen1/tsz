use super::*;
use tsz_parser::parser::ParserState;

fn evaluate_enum(source: &str) -> FxHashMap<String, EnumValue> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&enum_idx) = source_file.statements.nodes.first()
    {
        let mut evaluator = EnumEvaluator::new(&parser.arena);
        return evaluator.evaluate_enum(enum_idx);
    }
    FxHashMap::default()
}

#[test]
fn test_numeric_enum_auto_increment() {
    let values = evaluate_enum("enum E { A, B, C }");
    assert_eq!(values.get("A"), Some(&EnumValue::Number(0)));
    assert_eq!(values.get("B"), Some(&EnumValue::Number(1)));
    assert_eq!(values.get("C"), Some(&EnumValue::Number(2)));
}

#[test]
fn test_numeric_enum_explicit_values() {
    let values = evaluate_enum("enum E { A = 10, B, C = 20, D }");
    assert_eq!(values.get("A"), Some(&EnumValue::Number(10)));
    assert_eq!(values.get("B"), Some(&EnumValue::Number(11)));
    assert_eq!(values.get("C"), Some(&EnumValue::Number(20)));
    assert_eq!(values.get("D"), Some(&EnumValue::Number(21)));
}

#[test]
fn test_string_enum() {
    let values = evaluate_enum(r#"enum E { A = "alpha", B = "beta" }"#);
    assert_eq!(
        values.get("A"),
        Some(&EnumValue::String("alpha".to_string()))
    );
    assert_eq!(
        values.get("B"),
        Some(&EnumValue::String("beta".to_string()))
    );
}

#[test]
fn test_computed_binary_expression() {
    let values = evaluate_enum("enum E { A = 1, B = 2, C = A + B }");
    assert_eq!(values.get("A"), Some(&EnumValue::Number(1)));
    assert_eq!(values.get("B"), Some(&EnumValue::Number(2)));
    assert_eq!(values.get("C"), Some(&EnumValue::Number(3)));
}

#[test]
fn test_bitwise_operations() {
    let values = evaluate_enum("enum E { A = 1, B = 2, C = A | B, D = A & B, E = A ^ B }");
    assert_eq!(values.get("A"), Some(&EnumValue::Number(1)));
    assert_eq!(values.get("B"), Some(&EnumValue::Number(2)));
    assert_eq!(values.get("C"), Some(&EnumValue::Number(3))); // 1 | 2
    assert_eq!(values.get("D"), Some(&EnumValue::Number(0))); // 1 & 2
    assert_eq!(values.get("E"), Some(&EnumValue::Number(3))); // 1 ^ 2
}

#[test]
fn test_unary_operators() {
    let values = evaluate_enum("enum E { A = 5, B = -A, C = ~A }");
    assert_eq!(values.get("A"), Some(&EnumValue::Number(5)));
    assert_eq!(values.get("B"), Some(&EnumValue::Number(-5)));
    assert_eq!(values.get("C"), Some(&EnumValue::Number(!5)));
}

#[test]
fn test_shift_operators() {
    let values = evaluate_enum("enum E { A = 1 << 4, B = 16 >> 2, C = -16 >>> 2 }");
    assert_eq!(values.get("A"), Some(&EnumValue::Number(16)));
    assert_eq!(values.get("B"), Some(&EnumValue::Number(4)));
    // Unsigned right shift
    let expected_c = ((-16i64 as u64) >> 2) as i64;
    assert_eq!(values.get("C"), Some(&EnumValue::Number(expected_c)));
}

#[test]
fn test_parenthesized_expression() {
    let values = evaluate_enum("enum E { A = (1 + 2) * 3 }");
    assert_eq!(values.get("A"), Some(&EnumValue::Number(9)));
}

#[test]
fn test_self_reference() {
    let values = evaluate_enum("enum E { A = 1, B = E.A + 1 }");
    assert_eq!(values.get("A"), Some(&EnumValue::Number(1)));
    assert_eq!(values.get("B"), Some(&EnumValue::Number(2)));
}

#[test]
fn test_hex_literal() {
    let values = evaluate_enum("enum E { A = 0xFF, B = 0x10 }");
    assert_eq!(values.get("A"), Some(&EnumValue::Number(255)));
    assert_eq!(values.get("B"), Some(&EnumValue::Number(16)));
}

#[test]
fn test_mixed_string_breaks_auto_increment() {
    let values = evaluate_enum(r#"enum E { A = 1, B = "b", C }"#);
    assert_eq!(values.get("A"), Some(&EnumValue::Number(1)));
    assert_eq!(values.get("B"), Some(&EnumValue::String("b".to_string())));
    // After string, auto-increment fails - C becomes computed or error
    // In TypeScript, this is actually an error, but we produce Computed
    assert!(matches!(values.get("C"), Some(EnumValue::Computed) | None));
}

#[test]
fn test_enum_value_to_js_literal() {
    assert_eq!(EnumValue::Number(42).to_js_literal(), "42");
    assert_eq!(EnumValue::Number(-5).to_js_literal(), "-5");
    assert_eq!(
        EnumValue::String("hello".to_string()).to_js_literal(),
        "\"hello\""
    );
    assert_eq!(
        EnumValue::String("say \"hi\"".to_string()).to_js_literal(),
        "\"say \\\"hi\\\"\""
    );
}

#[test]
fn test_const_enum_values() {
    let values = evaluate_enum("const enum E { A = 1, B = 2, C = A | B }");
    assert_eq!(values.get("A"), Some(&EnumValue::Number(1)));
    assert_eq!(values.get("B"), Some(&EnumValue::Number(2)));
    assert_eq!(values.get("C"), Some(&EnumValue::Number(3)));
}
