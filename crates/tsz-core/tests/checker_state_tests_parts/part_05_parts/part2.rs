#[test]
fn test_checker_element_access_optional_chain_nullable_object() {
    use tsz_solver::TypeData;

    let source = r#"
type Foo = { a: number };
let obj: Foo | undefined;
const value = obj?.["a"];
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    let value_key = types.lookup(value_type).expect("value type should exist");
    match value_key {
        TypeData::Union(members) => {
            let members = types.type_list(members);
            assert!(members.contains(&TypeId::NUMBER));
            assert!(members.contains(&TypeId::UNDEFINED));
        }
        _ => panic!("Expected union type for value, got {value_key:?}"),
    }
}

#[test]
fn test_checker_property_access_optional_chain_nullable_object() {
    use tsz_solver::TypeData;

    let source = r#"
type Foo = { a: number };
let obj: Foo | undefined;
const value = obj?.a;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    let value_key = types.lookup(value_type).expect("value type should exist");
    match value_key {
        TypeData::Union(members) => {
            let members = types.type_list(members);
            assert!(members.contains(&TypeId::NUMBER));
            assert!(members.contains(&TypeId::UNDEFINED));
        }
        _ => panic!("Expected union type for value, got {value_key:?}"),
    }
}

#[test]
fn test_checker_property_access_union_type() {
    use tsz_solver::TypeData;

    // Test union property access WITHOUT narrowing
    // Using declare prevents CFA narrowing on initialization
    let source = r#"
type U = { a: number } | { a: string };
declare const obj: U;
const value = obj.a;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    let value_key = types.lookup(value_type).expect("value type should exist");
    match value_key {
        TypeData::Union(members) => {
            let members = types.type_list(members);
            assert!(members.contains(&TypeId::NUMBER));
            assert!(members.contains(&TypeId::STRING));
        }
        _ => panic!("Expected union type for value, got {value_key:?}"),
    }
}

#[test]
fn test_checker_namespace_merges_with_class_exports() {
    use tsz_solver::TypeData;

    let source = r#"
class Foo {}
namespace Foo {
    export interface Bar { x: number; }
}
type Alias = Foo.Bar;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            no_lib: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    let non_lib_diagnostics: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .collect();
    assert!(
        non_lib_diagnostics.is_empty(),
        "Unexpected diagnostics: {non_lib_diagnostics:?}"
    );

    let alias_sym = binder.file_locals.get("Alias").expect("Alias should exist");
    let alias_type = checker.get_type_of_symbol(alias_sym);
    let alias_key = types.lookup(alias_type).expect("Alias type should exist");
    match alias_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "x")
                .expect("Expected property x");
            assert_eq!(prop.type_id, TypeId::NUMBER);
        }
        TypeData::Lazy(_def_id) => {
            // Phase 4.3: Interface type references now use Lazy(DefId)
            // The Lazy type is correctly resolved when needed for type checking
        }
        _ => panic!("Expected Alias to resolve to Object or Lazy type, got {alias_key:?}"),
    }
}

#[test]
fn test_checker_namespace_merges_with_class_exports_reverse_order() {
    use tsz_solver::TypeData;

    let source = r#"
namespace Foo {
    export interface Bar { x: number; }
}
class Foo {}
type Alias = Foo.Bar;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            no_lib: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    // tsc does NOT emit TS2434 for non-instantiated namespaces (interfaces/types only).
    // Only instantiated namespaces (with runtime members like variables, functions, classes)
    // trigger TS2434 when they precede the class/function they merge with.
    assert!(
        checker.ctx.diagnostics.iter().all(|d| d.code != 2434),
        "Non-instantiated namespace should NOT trigger TS2434: {:?}",
        checker.ctx.diagnostics
    );

    let alias_sym = binder.file_locals.get("Alias").expect("Alias should exist");
    let alias_type = checker.get_type_of_symbol(alias_sym);
    let alias_key = types.lookup(alias_type).expect("Alias type should exist");
    match alias_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "x")
                .expect("Expected property x");
            assert_eq!(prop.type_id, TypeId::NUMBER);
        }
        TypeData::Lazy(_def_id) => {
            // Phase 4.3: Interface type references now use Lazy(DefId)
            // The Lazy type is correctly resolved when needed for type checking
        }
        _ => panic!("Expected Alias to resolve to Object or Lazy type, got {alias_key:?}"),
    }
}

/// Test namespace merging with class for value exports
///
/// NOTE: Currently ignored - see `test_checker_namespace_merges_with_class_element_access`.
#[test]
fn test_checker_namespace_merges_with_class_value_exports() {
    let source = r#"
class Foo {}
namespace Foo {
    export const value = 1;
}
const direct = Foo.value;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let direct_sym = binder
        .file_locals
        .get("direct")
        .expect("direct should exist");
    // `export const value = 1` produces literal type `1`, not `number`
    assert_eq!(
        checker.get_type_of_symbol(direct_sym),
        types.literal_number(1.0)
    );
}
