    use super::*;
    use tsz_parser::parser::ParserState;

    fn check_enum(source: &str) -> Vec<Diagnostic> {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&enum_idx) = source_file.statements.nodes.first()
        {
            let mut checker = EnumChecker::new(&parser.arena);
            checker.check_enum_declaration(enum_idx);
            return checker.take_diagnostics();
        }
        Vec::new()
    }

    #[test]
    fn test_no_errors_for_valid_enum() {
        let diagnostics = check_enum("enum E { A, B, C }");
        assert!(diagnostics.is_empty(), "Should have no errors");
    }

    #[test]
    fn test_duplicate_member_error() {
        let diagnostics = check_enum("enum E { A, B, A }");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, diagnostic_codes::DUPLICATE_IDENTIFIER);
        assert!(diagnostics[0].message_text.contains("Duplicate identifier"));
    }

    #[test]
    fn test_valid_const_enum() {
        let diagnostics = check_enum("const enum E { A = 1, B = 2, C = A | B }");
        assert!(
            diagnostics.is_empty(),
            "Valid const enum should have no errors"
        );
    }

    #[test]
    fn test_valid_string_enum() {
        let diagnostics = check_enum(r#"enum E { A = "a", B = "b" }"#);
        assert!(
            diagnostics.is_empty(),
            "Valid string enum should have no errors"
        );
    }

    #[test]
    fn test_mixed_enum_needs_initializer() {
        // After a string member, following members need initializers
        let diagnostics = check_enum(r#"enum E { A = "a", B }"#);
        // B should require an initializer since A is a string
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == diagnostic_codes::ENUM_MEMBER_MUST_HAVE_INITIALIZER),
            "Should error on member without initializer after string member"
        );
    }

    #[test]
    fn test_const_enum_with_expressions() {
        let diagnostics = check_enum("const enum E { A = 1 + 2, B = ~3, C = (4 * 5) }");
        assert!(
            diagnostics.is_empty(),
            "Const enum with simple expressions should be valid"
        );
    }
