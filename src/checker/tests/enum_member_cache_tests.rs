use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::solver::{TypeInterner, TypeKey};

#[test]
fn test_enum_member_types_prefilled_after_enum_resolution() {
    let source = r#"
enum E { A, B, C }
const a = E.A;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let enum_sym_id = binder
        .file_locals
        .get("E")
        .expect("enum symbol E should be bound");
    let enum_symbol = binder
        .get_symbol(enum_sym_id)
        .expect("enum symbol E should be available");
    let exports = enum_symbol
        .exports
        .as_ref()
        .expect("enum symbol E should expose member exports");

    for member_name in ["A", "B", "C"] {
        let member_sym_id = exports
            .get(member_name)
            .expect("enum member should exist in exports");
        let member_type = *checker
            .ctx
            .symbol_types
            .get(&member_sym_id)
            .expect("enum member type should be prefilled in symbol cache");
        assert!(
            matches!(types.lookup(member_type), Some(TypeKey::Enum(_, _))),
            "enum member {member_name} should resolve to TypeKey::Enum"
        );
    }
}
