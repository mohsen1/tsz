
use super::*;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::ParserState;

fn create_parser(source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root_idx = parser.parse_source_file();
    (parser, root_idx)
}

#[test]
fn test_numeric_enum_es5() {
    let (parser, root_idx) = create_parser("enum E { A, B, C }");

    if let Some(root_node) = parser.arena.get(root_idx)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&enum_idx) = source_file.statements.nodes.first()
    {
        let mut transformer = EnumTransformer::new(&parser.arena);
        transformer.register_enum(enum_idx);
        let output = transformer.emit_enum_es5(enum_idx);

        assert!(output.contains("var E;"));
        assert!(output.contains("(function (E)"));
        assert!(output.contains("E[E[\"A\"] = 0] = \"A\""));
        assert!(output.contains("E[E[\"B\"] = 1] = \"B\""));
        assert!(output.contains("E[E[\"C\"] = 2] = \"C\""));
    }
}

#[test]
fn test_string_enum_no_reverse_mapping() {
    let (parser, root_idx) = create_parser(r#"enum S { A = "alpha", B = "beta" }"#);

    if let Some(root_node) = parser.arena.get(root_idx)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&enum_idx) = source_file.statements.nodes.first()
    {
        let mut transformer = EnumTransformer::new(&parser.arena);
        transformer.register_enum(enum_idx);
        let output = transformer.emit_enum_es5(enum_idx);

        // String enums should NOT have reverse mapping
        assert!(output.contains("S[\"A\"] = \"alpha\""));
        assert!(output.contains("S[\"B\"] = \"beta\""));
        assert!(
            !output.contains("S[S["),
            "String enum should not have reverse mapping"
        );
    }
}

#[test]
fn test_const_enum_erased() {
    let (parser, root_idx) = create_parser("const enum CE { A = 1, B = 2 }");

    if let Some(root_node) = parser.arena.get(root_idx)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&enum_idx) = source_file.statements.nodes.first()
    {
        let mut transformer = EnumTransformer::new(&parser.arena);
        transformer.register_enum(enum_idx);
        let output = transformer.emit_enum_es5(enum_idx);

        assert!(output.is_empty(), "Const enum should be erased by default");
    }
}

#[test]
fn test_const_enum_preserved() {
    let (parser, root_idx) = create_parser("const enum CE { A = 1, B = 2 }");

    if let Some(root_node) = parser.arena.get(root_idx)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&enum_idx) = source_file.statements.nodes.first()
    {
        let options = EnumTransformOptions {
            preserve_const_enums: true,
            ..Default::default()
        };
        let mut transformer = EnumTransformer::with_options(&parser.arena, options);
        transformer.register_enum(enum_idx);
        let output = transformer.emit_enum_es5(enum_idx);

        assert!(
            !output.is_empty(),
            "Const enum should be preserved with option"
        );
        assert!(output.contains("var CE;"));
    }
}

#[test]
fn test_const_enum_inlining() {
    let (parser, root_idx) = create_parser("const enum Direction { Up = 1, Down = 2 }");

    if let Some(root_node) = parser.arena.get(root_idx)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&enum_idx) = source_file.statements.nodes.first()
    {
        let mut transformer = EnumTransformer::new(&parser.arena);
        transformer.register_enum(enum_idx);
        transformer.evaluate_enum(enum_idx);

        // Try to inline
        let inlined_up = transformer.try_inline_const_enum_access("Direction", "Up");
        let inlined_down = transformer.try_inline_const_enum_access("Direction", "Down");

        assert_eq!(inlined_up, Some("1".to_string()));
        assert_eq!(inlined_down, Some("2".to_string()));
    }
}

#[test]
fn test_const_enum_inlining_with_comments() {
    let (parser, root_idx) = create_parser("const enum Flags { None = 0, Read = 1 }");

    if let Some(root_node) = parser.arena.get(root_idx)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&enum_idx) = source_file.statements.nodes.first()
    {
        let options = EnumTransformOptions {
            emit_comments: true,
            ..Default::default()
        };
        let mut transformer = EnumTransformer::with_options(&parser.arena, options);
        transformer.register_enum(enum_idx);
        transformer.evaluate_enum(enum_idx);

        let inlined = transformer.try_inline_const_enum_access("Flags", "Read");
        assert_eq!(inlined, Some("1 /* Read */".to_string()));
    }
}

#[test]
fn test_ambient_enum_erased() {
    let (parser, root_idx) = create_parser("declare enum E { A, B }");

    if let Some(root_node) = parser.arena.get(root_idx)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&enum_idx) = source_file.statements.nodes.first()
    {
        let mut transformer = EnumTransformer::new(&parser.arena);
        transformer.register_enum(enum_idx);
        let output = transformer.emit_enum_es5(enum_idx);

        assert!(output.is_empty(), "Declare enum should be erased");
    }
}

#[test]
fn test_computed_enum_values() {
    let (parser, root_idx) = create_parser("enum E { A = 1 << 2, B = 3 | 4 }");

    if let Some(root_node) = parser.arena.get(root_idx)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
        && let Some(&enum_idx) = source_file.statements.nodes.first()
    {
        let mut transformer = EnumTransformer::new(&parser.arena);
        transformer.register_enum(enum_idx);
        let values = transformer.evaluate_enum(enum_idx);

        assert_eq!(values.get("A"), Some(&EnumValue::Number(4))); // 1 << 2
        assert_eq!(values.get("B"), Some(&EnumValue::Number(7))); // 3 | 4
    }
}
