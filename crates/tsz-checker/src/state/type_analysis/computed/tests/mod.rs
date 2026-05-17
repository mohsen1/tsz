use super::*;
use crate::query_boundaries::common::QueryDatabase;

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
    let types = tsz_solver::TypeInterner::new();
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

    let types = tsz_solver::TypeInterner::new();
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

fn checker_for_source(source: &str) -> (tsz_parser::parser::ParserState, tsz_binder::BinderState) {
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    let mut parser = ParserState::new("/test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    (parser, binder)
}

fn try_lower_holder_with_simple_interface_fastpath(source: &str) -> Option<TypeId> {
    use tsz_checker::context::CheckerOptions;

    let (parser, binder) = checker_for_source(source);
    let types = tsz_solver::TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "/test.ts".to_string(),
        CheckerOptions::default(),
    );
    let holder_sym = binder
        .file_locals
        .get("Holder")
        .expect("Holder interface should bind");
    let declarations = binder
        .get_symbol(holder_sym)
        .expect("Holder symbol should exist")
        .declarations
        .clone();

    checker.try_lower_simple_local_interface_object(
        holder_sym,
        &declarations,
        super::simple_local_interface::SimpleLocalInterfaceFacts {
            has_out_of_arena_decl: false,
            has_cross_file_same_index: false,
            has_local_interface_decl: true,
            has_local_interface_heritage_extends: false,
            has_local_computed_property_name: false,
            suppress_missing_interface_decl_reject: false,
        },
    )
}

fn expect_holder_property_from_fastpath(source: &str, message: &str) -> TypeId {
    let types = tsz_solver::TypeInterner::new();
    let item = types.intern_string("item");
    let (parser, binder) = checker_for_source(source);
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "/test.ts".to_string(),
        tsz_checker::context::CheckerOptions::default(),
    );
    let holder_sym = binder
        .file_locals
        .get("Holder")
        .expect("Holder interface should bind");
    let declarations = binder
        .get_symbol(holder_sym)
        .expect("Holder symbol should exist")
        .declarations
        .clone();
    let ty = checker
        .try_lower_simple_local_interface_object(
            holder_sym,
            &declarations,
            super::simple_local_interface::SimpleLocalInterfaceFacts {
                has_out_of_arena_decl: false,
                has_cross_file_same_index: false,
                has_local_interface_decl: true,
                has_local_interface_heritage_extends: false,
                has_local_computed_property_name: false,
                suppress_missing_interface_decl_reject: false,
            },
        )
        .expect(message);
    let item_ty =
        crate::query_boundaries::common::raw_property_type(types.as_type_database(), ty, item)
            .expect("fast-lowered Holder should retain the referenced property");
    assert_ne!(item_ty, TypeId::ANY, "property should not lower to any");
    assert_ne!(item_ty, TypeId::ERROR, "property should not lower to error");
    item_ty
}

#[test]
fn simple_local_interface_fastpath_accepts_non_generic_type_references() {
    expect_holder_property_from_fastpath(
        r#"
interface Leaf { value: string }
interface Holder { item: Leaf }
"#,
        "non-generic type references should stay on the simple interface fast path",
    );
}

#[test]
fn simple_local_interface_fastpath_accepts_qualified_non_generic_type_references() {
    expect_holder_property_from_fastpath(
        r#"
namespace N { export interface Leaf { value: string } }
interface Holder { item: N.Leaf }
"#,
        "qualified non-generic type references should stay on the simple interface fast path",
    );
}

#[test]
fn simple_local_interface_fastpath_accepts_non_generic_references_in_wrappers() {
    expect_holder_property_from_fastpath(
        r#"
interface Leaf { value: string }
interface Other { value: number }
interface Holder { item: Leaf[] | [Other] }
"#,
        "non-generic type references inside arrays, tuples, and unions should stay on the fast path",
    );
}

#[test]
fn simple_local_interface_fastpath_rejects_generic_bare_type_references() {
    assert!(
        try_lower_holder_with_simple_interface_fastpath(
            r#"
interface Box<T> { value: T }
interface Holder { item: Box }
"#,
        )
        .is_none(),
        "generic references without type arguments should fall back to the normal diagnostic path",
    );
}

#[test]
fn simple_local_interface_fastpath_rejects_qualified_generic_bare_type_references() {
    assert!(
        try_lower_holder_with_simple_interface_fastpath(
            r#"
namespace N { export interface Box<T> { value: T } }
interface Holder { item: N.Box }
"#,
        )
        .is_none(),
        "qualified generic references should fall back to the normal diagnostic path",
    );
}

#[test]
fn simple_local_interface_fastpath_rejects_explicit_type_arguments() {
    assert!(
        try_lower_holder_with_simple_interface_fastpath(
            r#"
interface Box<T> { value: T }
interface Holder { item: Box<string> }
"#,
        )
        .is_none(),
        "explicit type arguments should fall back to the normal diagnostic path",
    );
}

#[test]
fn simple_local_interface_fastpath_rejects_value_only_references() {
    assert!(
        try_lower_holder_with_simple_interface_fastpath(
            r#"
const Value = 1;
interface Holder { item: Value }
"#,
        )
        .is_none(),
        "value-only references should fall back to the normal diagnostic path",
    );
}
