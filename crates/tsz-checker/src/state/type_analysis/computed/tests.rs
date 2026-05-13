use super::*;
use crate::query_boundaries::common::{QueryDatabase, TypeInterner};

fn make_prop(name: tsz_common::Atom, declaration_order: u32) -> PropertyInfo {
    PropertyInfo {
        name,
        type_id: TypeId::ANY,
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }
}

#[test]
fn synthetic_namespace_default_normalization_preserves_default_before_augmentations() {
    let types = TypeInterner::new();
    let default_atom = types.intern_string("default");
    let configs_atom = types.intern_string("configs");
    let mut props = vec![make_prop(configs_atom, 0), make_prop(default_atom, 1)];

    CheckerState::normalize_namespace_export_declaration_order(&mut props);
    let namespace_type = types.factory().object(props);
    let shape = crate::query_boundaries::common::object_shape_for_type(&types, namespace_type)
        .expect("namespace type should have an object shape");
    let shape_props: Vec<_> = shape
        .properties
        .iter()
        .map(|prop| {
            (
                types.resolve_atom_ref(prop.name).to_string(),
                prop.declaration_order,
            )
        })
        .collect();

    assert_eq!(
        shape_props,
        vec![("configs".to_string(), 2), ("default".to_string(), 1)]
    );
}

#[test]
fn ordered_namespace_export_entries_follow_first_declaration_span() {
    use tsz_binder::{BinderState, SymbolTable, symbol_flags};
    use tsz_checker::context::{CheckerOptions, ScriptTarget};
    use tsz_parser::parser::ParserState;

    let mut parser = ParserState::new("/test.ts".to_string(), String::new());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let third = binder
        .symbols
        .alloc(symbol_flags::EXPORT_VALUE, "third".to_string());
    binder
        .symbols
        .get_mut(third)
        .expect("third symbol should exist")
        .add_declaration(NodeIndex::NONE, Some((30, 31)));

    let first = binder
        .symbols
        .alloc(symbol_flags::EXPORT_VALUE, "first".to_string());
    binder
        .symbols
        .get_mut(first)
        .expect("first symbol should exist")
        .add_declaration(NodeIndex::NONE, Some((10, 11)));

    let second = binder
        .symbols
        .alloc(symbol_flags::EXPORT_VALUE, "second".to_string());
    binder
        .symbols
        .get_mut(second)
        .expect("second symbol should exist")
        .add_declaration(NodeIndex::NONE, Some((20, 21)));

    let missing_span = binder
        .symbols
        .alloc(symbol_flags::EXPORT_VALUE, "missingSpan".to_string());

    let mut exports = SymbolTable::new();
    exports.set("third".to_string(), third);
    exports.set("missingSpan".to_string(), missing_span);
    exports.set("first".to_string(), first);
    exports.set("second".to_string(), second);

    let types = TypeInterner::new();
    let checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "/test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2020,
            ..CheckerOptions::default()
        },
    );

    let ordered = checker.ordered_namespace_export_entries(&exports);
    let names: Vec<_> = ordered
        .into_iter()
        .map(|(name, _)| name.to_string())
        .collect();
    assert_eq!(
        names,
        vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
            "missingSpan".to_string()
        ]
    );
}
