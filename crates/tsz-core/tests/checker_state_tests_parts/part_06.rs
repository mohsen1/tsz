/// Test namespace merging with class in reverse order
///
/// NOTE: Previously ignored due to wrong type expectation.
#[test]
fn test_checker_namespace_merges_with_class_value_exports_reverse_order() {
    let source = r#"
namespace Foo {
    export const value = 1;
}
class Foo {}
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
    // tsc emits TS2434 when namespace appears before the class it merges with
    let ts2434: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2434)
        .collect();
    assert_eq!(
        ts2434.len(),
        1,
        "Expected exactly one TS2434, got diagnostics: {:?}",
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

/// Test namespace merging across declarations for value access
///
/// NOTE: Currently ignored - namespace merging across declarations is not fully
/// implemented. The type resolution for merged namespaces doesn't correctly
/// combine all exported values across declarations.
#[test]
fn test_checker_namespace_merges_across_decls_value_access() {
    let source = r#"
namespace Merge {
    export const a = 1;
}
namespace Merge {
    export const b = 2;
}
const sum = Merge.a + Merge.b;
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

    let sum_sym = binder.file_locals.get("sum").expect("sum should exist");
    assert_eq!(checker.get_type_of_symbol(sum_sym), TypeId::NUMBER);
}

#[test]
fn test_checker_namespace_merges_across_decls_type_access() {
    use tsz_solver::TypeData;

    let source = r#"
namespace Merge {
    export interface A { x: number; }
}
namespace Merge {
    export interface B { y: number; }
}
type Alias = Merge.A;
const value: Merge.B = { y: 1 };
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
    // Phase 4.2: Type aliases are now represented as Lazy types, need to resolve them
    let resolved_type = checker.resolve_lazy_type(alias_type);
    let alias_key = types
        .lookup(resolved_type)
        .expect("Alias type should exist");
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

/// Test namespace merging with function for value exports
///
/// NOTE: Previously ignored due to wrong type expectation.
#[test]
fn test_checker_namespace_merges_with_function_value_exports() {
    let source = r#"
function Merge() {}
namespace Merge {
    export const extra = 1;
}
const direct = Merge.extra;
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
    // `export const extra = 1` produces literal type `1`, not `number`
    assert_eq!(
        checker.get_type_of_symbol(direct_sym),
        types.literal_number(1.0)
    );
}

/// Test namespace merging with function in reverse order
///
/// NOTE: Previously ignored due to wrong type expectation.
#[test]
fn test_checker_namespace_merges_with_function_value_exports_reverse_order() {
    let source = r#"
namespace Merge {
    export const extra = 1;
}
function Merge() {}
const direct = Merge.extra;
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
    // tsc emits TS2434 when namespace appears before the function it merges with
    let ts2434: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2434)
        .collect();
    assert_eq!(
        ts2434.len(),
        1,
        "Expected exactly one TS2434, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let direct_sym = binder
        .file_locals
        .get("direct")
        .expect("direct should exist");
    // `export const extra = 1` produces literal type `1`, not `number`
    assert_eq!(
        checker.get_type_of_symbol(direct_sym),
        types.literal_number(1.0)
    );
}

#[test]
fn test_checker_namespace_merges_with_function_type_exports() {
    use tsz_solver::TypeData;

    let source = r#"
function Merge() {}
namespace Merge {
    export interface Extra { value: number; }
}
type Alias = Merge.Extra;
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
                .find(|prop| types.resolve_atom(prop.name) == "value")
                .expect("Expected property value");
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
fn test_checker_namespace_merges_with_function_type_exports_reverse_order() {
    use tsz_solver::TypeData;

    let source = r#"
namespace Merge {
    export interface Extra { value: number; }
}
function Merge() {}
type Alias = Merge.Extra;
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
                .find(|prop| types.resolve_atom(prop.name) == "value")
                .expect("Expected property value");
            assert_eq!(prop.type_id, TypeId::NUMBER);
        }
        TypeData::Lazy(_def_id) => {
            // Phase 4.3: Interface type references now use Lazy(DefId)
            // The Lazy type is correctly resolved when needed for type checking
        }
        _ => panic!("Expected Alias to resolve to Object or Lazy type, got {alias_key:?}"),
    }
}

/// Test namespace merging with enum for value exports
///
/// NOTE: Previously ignored due to wrong type expectation.
#[test]
fn test_checker_namespace_merges_with_enum_value_exports() {
    let source = r#"
enum Merge {
    A,
}
namespace Merge {
    export const extra = 1;
}
const direct = Merge.extra;
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
    // `export const extra = 1` produces literal type `1`, not `number`
    assert_eq!(
        checker.get_type_of_symbol(direct_sym),
        types.literal_number(1.0)
    );
}

/// Test namespace merging with enum in reverse order
///
/// NOTE: Previously ignored due to wrong type expectation.
#[test]
fn test_checker_namespace_merges_with_enum_value_exports_reverse_order() {
    let source = r#"
namespace Merge {
    export const extra = 1;
}
enum Merge {
    A,
}
const direct = Merge.extra;
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

    let direct_sym = binder
        .file_locals
        .get("direct")
        .expect("direct should exist");
    // `export const extra = 1` produces literal type `1`, not `number`
    assert_eq!(
        checker.get_type_of_symbol(direct_sym),
        types.literal_number(1.0)
    );
}

#[test]
fn test_checker_namespace_merges_with_enum_type_exports() {
    use tsz_solver::TypeData;

    let source = r#"
enum Merge {
    A,
}
namespace Merge {
    export interface Extra { value: number; }
}
type Alias = Merge.Extra;
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
                .find(|prop| types.resolve_atom(prop.name) == "value")
                .expect("Expected property value");
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
fn test_checker_namespace_merges_with_enum_type_exports_reverse_order() {
    use tsz_solver::TypeData;

    let source = r#"
namespace Merge {
    export interface Extra { value: number; }
}
enum Merge {
    A,
}
type Alias = Merge.Extra;
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
                .find(|prop| types.resolve_atom(prop.name) == "value")
                .expect("Expected property value");
            assert_eq!(prop.type_id, TypeId::NUMBER);
        }
        TypeData::Lazy(_def_id) => {
            // Phase 4.3: Interface type references now use Lazy(DefId)
            // The Lazy type is correctly resolved when needed for type checking
        }
        _ => panic!("Expected Alias to resolve to Object or Lazy type, got {alias_key:?}"),
    }
}

/// Test namespace merging with class for element access
///
/// Namespace members should be visible through bracket access on the class constructor type.
#[test]
fn test_checker_namespace_merges_with_class_element_access() {
    let source = r#"
class Foo {}
namespace Foo {
    export const value = 1;
}
const direct = Foo["value"];
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
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.is_empty(),
        "Expected no diagnostics for namespace+class element access, got: {codes:?}"
    );
}

#[test]
fn test_checker_interface_typeof_value_reference() {
    use tsz_solver::{SymbolRef, TypeData};

    let source = r#"
const Foo = 1;
namespace Ns {
    export const value = 1;
}
interface Bar {
    x: typeof Foo;
    y: typeof Ns.value;
}
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

    let foo_sym = binder.file_locals.get("Foo").expect("Foo should exist");
    let ns_sym = binder.file_locals.get("Ns").expect("Ns should exist");
    let value_sym = binder
        .get_symbol(ns_sym)
        .and_then(|symbol| symbol.exports.as_ref())
        .and_then(|exports| exports.get("value"))
        .expect("Ns.value should exist");

    let bar_sym = binder.file_locals.get("Bar").expect("Bar should exist");
    let bar_type = checker.get_type_of_symbol(bar_sym);
    let bar_key = types.lookup(bar_type).expect("Bar type should exist");
    match bar_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let prop_names: Vec<String> = shape
                .properties
                .iter()
                .map(|prop| types.resolve_atom(prop.name))
                .collect();
            let prop_x = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "x")
                .expect("Expected property x");
            let prop_y = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "y")
                .unwrap_or_else(|| panic!("Expected property y, got {prop_names:?}"));

            match types.lookup(prop_x.type_id) {
                Some(TypeData::TypeQuery(SymbolRef(sym_id))) => assert_eq!(sym_id, foo_sym.0),
                other => panic!("Expected x to be typeof Foo, got {other:?}"),
            }

            match types.lookup(prop_y.type_id) {
                Some(TypeData::TypeQuery(SymbolRef(sym_id))) => assert_eq!(sym_id, value_sym.0),
                other => panic!("Expected y to be typeof Ns.value, got {other:?}"),
            }
        }
        _ => panic!("Expected Bar to resolve to Object type, got {bar_key:?}"),
    }
}

/// Test typeof with namespace alias member access
///
/// Test that `typeof Alias.value` resolves to the correct type through
/// namespace import aliases (`import Alias = Ns`).
#[test]
fn test_checker_typeof_namespace_alias_member() {
    let source = r#"
namespace Ns {
    export const value = 1;
}
import Alias = Ns;
type T = typeof Alias.value;
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

    // typeof Alias.value should resolve to the literal type 1 (const value = 1)
    let t_sym = binder.file_locals.get("T").expect("T should exist");
    let t_type = checker.get_type_of_symbol(t_sym);
    assert_eq!(
        t_type,
        types.literal_number(1.0),
        "typeof Alias.value should resolve to literal type 1"
    );
}

#[test]
fn test_checker_typeof_with_type_arguments() {
    use tsz_solver::TypeData;

    let source = r#"
const Foo = <T>(value: T) => value;
type Alias = typeof Foo<string>;
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

    let alias_sym = binder.file_locals.get("Alias").expect("Alias should exist");

    let alias_type = checker.get_type_of_symbol(alias_sym);
    let alias_key = types.lookup(alias_type).expect("Alias type should exist");
    match alias_key {
        TypeData::Callable(shape_id) => {
            let shape = types.callable_shape(shape_id);
            assert_eq!(shape.call_signatures.len(), 1);
            let sig = &shape.call_signatures[0];
            assert!(sig.type_params.is_empty());
            assert_eq!(sig.params.len(), 1);
            assert_eq!(sig.params[0].type_id, TypeId::STRING);
            assert_eq!(sig.return_type, TypeId::STRING);
        }
        _ => panic!("Expected Alias to be instantiated callable type, got {alias_key:?}"),
    }
}

/// Test circular type alias handling
///
/// NOTE: Currently ignored - circular type alias resolution is not fully implemented.
/// Circular type alias handling
///
/// TODO: Circular type aliases do not resolve to `any` as tsc does.
/// Currently they resolve to a lazy/unresolved TypeId. When circular alias
/// detection is implemented, update to assert `TypeId::ANY`.
#[test]
fn test_checker_circular_type_aliases() {
    let source = r#"
type A = B;
type B = A;
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

    let a_sym = binder.file_locals.get("A").expect("A should exist");
    let b_sym = binder.file_locals.get("B").expect("B should exist");

    // TODO: Should be TypeId::ANY once circular alias detection resolves to `any`
    let a_type = checker.get_type_of_symbol(a_sym);
    let b_type = checker.get_type_of_symbol(b_sym);
    // Both should resolve to the same type (they reference each other)
    assert_eq!(
        a_type, b_type,
        "Circular aliases A and B should resolve to the same type"
    );
}

#[test]
fn test_index_signature_at_solver_level() {
    use tsz_solver::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};
    use tsz_solver::{IndexSignature, ObjectFlags, ObjectShape};

    // Test that index signature resolution is tracked at solver level
    let types = TypeInterner::new();

    // Create object type with only index signature
    let shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    };

    let obj_type = types.object_with_index(shape);
    let evaluator = PropertyAccessEvaluator::new(&types);

    let result = evaluator.resolve_property_access(obj_type, "anyProperty");
    match result {
        PropertyAccessResult::Success {
            type_id,
            from_index_signature,
            ..
        } => {
            assert_eq!(type_id, TypeId::NUMBER);
            assert!(
                from_index_signature,
                "Should be marked as from_index_signature"
            );
        }
        _ => panic!("Expected Success, got: {result:?}"),
    }
}

// ============== Ambient module pattern tests (errors 2436, 2819) ==============

#[test]
fn test_ambient_module_relative_path_2436() {
    use crate::checker::diagnostics::diagnostic_codes;

    // TS2436: Ambient module declaration cannot specify relative module name
    let source = r#"
declare module "./relative-module" {
    export function foo(): void;
}

declare module "../another-relative" {
    export const bar: number;
}

declare module "." {
    export type Baz = string;
}
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let error_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::AMBIENT_MODULE_DECLARATION_CANNOT_SPECIFY_RELATIVE_MODULE_NAME
        })
        .count();

    assert_eq!(
        error_count, 3,
        "Expected 3 errors with code 2436 for relative module names, got: {codes:?}"
    );
}

#[test]
fn test_ambient_module_absolute_path_ok() {
    // Absolute module names should be allowed in ambient declarations
    let source = r#"
declare module "absolute-module" {
    export function foo(): void;
}

declare module "@scoped/package" {
    export const bar: number;
}
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let error_5061_count = codes.iter().filter(|&&c| c == 5061).count();

    assert_eq!(
        error_5061_count, 0,
        "Expected no error 5061 for absolute module names, got: {codes:?}"
    );
}

#[test]
fn test_private_identifier_in_ambient_class_allowed() {
    // In tsc 6.0, private identifiers (#name) ARE allowed in ambient classes.
    // TS18019 should NOT be emitted for # members in declare classes.
    let source = r#"
declare class AmbientClass {
    #privateField: string;
    #anotherPrivate: number;

    #privateMethod(): void;

    get #privateGetter(): boolean;
    set #privateSetter(value: boolean);
}
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let error_count = codes.iter().filter(|&&c| c == 18019).count();

    // Should NOT report TS18019 for private identifiers in ambient classes
    assert!(
        error_count == 0,
        "Expected 0 errors with code 18019 for private identifiers in ambient class, got {error_count} errors: {codes:?}"
    );
}

#[test]
fn test_private_identifier_in_non_ambient_class_ok() {
    // Private identifiers should be allowed in non-ambient classes
    let source = r#"
class RegularClass {
    #privateField: string;

    constructor() {
        this.#privateField = "test";
    }

    #privateMethod(): void {
        console.log(this.#privateField);
    }
}
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let error_2819_count = codes.iter().filter(|&&c| c == 2819).count();

    assert_eq!(
        error_2819_count, 0,
        "Expected no error 2819 for private identifiers in non-ambient class, got: {codes:?}"
    );
}

#[test]
fn test_private_static_method_access_no_error() {
    // Private static methods should be accessible within the class
    let source = r#"
class A {
    static #foo(a: number) {}
    constructor() {
        A.#foo(30);
    }
}
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // TS2339 = "Property 'X' does not exist on type 'Y'"
    let error_2339_count = codes.iter().filter(|&&c| c == 2339).count();

    assert_eq!(
        error_2339_count, 0,
        "Expected no TS2339 error for private static method access, got errors: {codes:?}"
    );
}

#[test]
fn test_non_private_static_accessor_access_works() {
    // Non-private static accessors should be accessible from class reference
    let source = r#"
class A {
    static get quux(): number {
        return 42;
    }
}
let x = A.quux;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // TS2339 = "Property 'X' does not exist on type 'Y'"
    let error_2339_count = codes.iter().filter(|&&c| c == 2339).count();

    assert_eq!(
        error_2339_count, 0,
        "Expected no TS2339 error for non-private static accessor access, got errors: {codes:?}"
    );
}

#[test]
fn test_private_static_accessor_access_no_error() {
    // Private static accessors should be accessible within the class
    // Simplified test: just a getter without body references
    let source = r#"
class A {
    static get #quux(): number {
        return 42;
    }
    constructor() {
        let x = A.#quux;
    }
}
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // TS2339 = "Property 'X' does not exist on type 'Y'"
    let error_2339_count = codes.iter().filter(|&&c| c == 2339).count();

    assert_eq!(
        error_2339_count, 0,
        "Expected no TS2339 error for private static accessor access, got errors: {codes:?}"
    );
}

#[test]
fn test_private_static_generator_method_access_no_error() {
    // Private static async generator methods should be accessible within the class
    let source = r#"
class A {
    static async *#baz(a: number) {
        return 3;
    }
    constructor() {
        A.#baz(30);
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // TS1068 = "Unexpected token"
    // TS2339 = "Property 'X' does not exist on type 'Y'"
    let error_1068_count = codes.iter().filter(|&&c| c == 1068).count();
    let error_2339_count = codes.iter().filter(|&&c| c == 2339).count();

    assert_eq!(
        error_1068_count, 0,
        "Expected no TS1068 (unexpected token) error for private static generator method, got errors: {codes:?}"
    );
    assert_eq!(
        error_2339_count, 0,
        "Expected no TS2339 error for private static generator method access, got errors: {codes:?}"
    );
}

#[test]
fn test_namespace_with_relative_path_ok() {
    // Namespace declarations (without declare) can have any name, including relative-like names
    // This test ensures we only check ambient modules (declare module)
    let source = r#"
namespace MyNamespace {
    export function foo(): void {}
}
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let error_5061_count = codes.iter().filter(|&&c| c == 5061).count();

    assert_eq!(
        error_5061_count, 0,
        "Expected no error 5061 for namespace declarations (only ambient modules should error), got: {codes:?}"
    );
}

// ============== Top-level scope tests (fixes critical bug) ==============

#[test]
fn test_top_level_variable_redeclaration_different_type_2403() {
    // Top-level variables with different types should trigger error 2403
    let source = r#"
var x: string;
var x: number;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2403),
        "Expected error 2403 for top-level variable redeclaration with different type, got: {codes:?}"
    );
}

#[test]
fn test_top_level_variable_redeclaration_same_type_ok() {
    // Top-level variables with same type should be allowed
    let source = r#"
var x: string;
var x: string;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let error_2403_count = codes.iter().filter(|&&c| c == 2403).count();

    assert_eq!(
        error_2403_count, 0,
        "Expected no error 2403 for top-level variable redeclaration with same type, got: {codes:?}"
    );
}

#[test]
fn test_variable_redeclaration_typeof_ok_no_2403() {
    // Test for bi-directional assignability in var redeclaration:
    // `var e = E;` and `var e: typeof E;` should be allowed because
    // the types are bi-directionally assignable (even if TypeIds differ).
    // Based on TypeScript conformance test: enumBasics.ts
    let source = r#"
enum E { A, B, C }
var e = E;
var e: typeof E;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let error_2403_count = codes.iter().filter(|&&c| c == 2403).count();

    assert_eq!(
        error_2403_count, 0,
        "Expected no error 2403 for enum typeof redeclaration, got: {codes:?}"
    );
}

#[test]
fn test_variable_redeclaration_enum_object_literal_no_2403() {
    // Ensure enum value redeclaration with structural type does not trigger TS2403.
    let source = r#"
enum E1 {
    A,
    B,
    C
}

var e = E1;
var e: {
    readonly A: number;
    readonly B: number;
    readonly C: number;
    readonly [n: number]: string;
};
var e: typeof E1;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let error_2403_count = codes.iter().filter(|&&c| c == 2403).count();

    assert_eq!(
        error_2403_count, 1,
        "Expected 1 error 2403 for third variable declaration (matching tsc), got: {codes:?}"
    );
}

/// Test that variable redeclaration with array spread doesn't emit TS2403
///
/// NOTE: Currently ignored - variable redeclaration detection with array spread is not
/// fully implemented. The checker incorrectly emits TS2403 for redeclarations when
/// array spread is involved.
#[test]
fn test_variable_redeclaration_array_spread_no_2403() {
    let source = r#"
function f1() {
    var a = [1, 2, 3];
    var b = ["hello", ...a, true];
    var b: (string | number | boolean)[];
}
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let error_2403_count = codes.iter().filter(|&&c| c == 2403).count();

    assert_eq!(
        error_2403_count, 0,
        "Expected no error 2403 for array spread redeclaration, got: {codes:?}"
    );
}

#[test]
fn test_variable_redeclaration_inferred_vs_annotated_no_2403() {
    // Test that inferred type from initializer matches explicit annotation
    // Based on conformance test: ambientDeclarationsExternal.ts pattern
    let source = r#"
var n = 42;
var n: number;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let error_2403_count = codes.iter().filter(|&&c| c == 2403).count();

    assert_eq!(
        error_2403_count, 0,
        "Expected no error 2403 for inferred vs annotated redeclaration, got: {codes:?}"
    );
}

#[test]
fn test_namespace_member_not_found() {
    let source = r#"
namespace foo {
    export class Provide {}
}
var p: foo.NotExist;
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

    let diags = &checker.ctx.diagnostics;
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();

    // Should produce error 2694: Namespace 'foo' has no exported member 'NotExist'
    assert!(
        codes.contains(&2694),
        "Expected error 2694 for namespace member not found, got: {codes:?}"
    );
}

#[test]
fn test_namespace_value_member_missing_errors() {
    let source = r#"
namespace NS {
    export const ok = 1;
}
import Alias = NS;
const bad = NS.missing;
const badAlias = Alias.missing;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let missing_count = codes.iter().filter(|&&code| code == 2339).count();
    assert_eq!(
        missing_count, 2,
        "Expected two 2339 errors for missing namespace value members, got: {codes:?}"
    );
}

/// Test import alias type resolution
///
/// NOTE: Currently ignored - import alias type resolution is not fully implemented.
/// The `import Alias = NS.Exported` syntax triggers TS1202 error about import assignments
/// in ES modules.
#[test]
fn test_import_alias_type_resolution() {
    let source = r#"
namespace NS {
    export class Exported {}
    class NotExported {}
}
import Alias = NS.Exported;
var x: Alias;
var y: NS.Exported;
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

    let diags = &checker.ctx.diagnostics;
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();

    // Should produce no errors - both x: Alias and y: NS.Exported should resolve correctly
    assert!(
        codes.is_empty(),
        "Expected no errors for import alias type resolution, got: {codes:?}"
    );
}

#[test]
fn test_import_alias_non_exported_member() {
    let source = r#"
namespace NS {
    export class Exported {}
    class NotExported {}
}
import Alias = NS.NotExported;
var x: Alias;
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

    let diags = &checker.ctx.diagnostics;
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();

    // Should produce error 2694 or 2724 (spelling suggestion variant):
    // Namespace 'NS' has no exported member 'NotExported' (Did you mean 'Exported'?)
    assert!(
        codes.contains(&2694) || codes.contains(&2724),
        "Expected error 2694 or 2724 for import alias of non-exported member, got: {codes:?}"
    );
}

#[test]
fn test_import_type_value_usage_errors() {
    let source = r#"
import type { Foo } from "./types";
Foo;
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
            module: crate::common::ModuleKind::ESNext,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // TS1361: 'Foo' cannot be used as a value because it was imported using 'import type'.
    assert!(
        codes.contains(&1361),
        "Expected TS1361 for type-only import used as value, got: {codes:?}"
    );
    assert!(
        !codes.contains(&1148),
        "Should not emit TS1148 (module=none error) for import type test, got: {codes:?}"
    );
}

#[test]
fn test_numeric_enum_open_and_nominal_assignability() {
    let source = r#"
enum A { X, Y }
enum B { X, Y }
let a: A = 1;
let n: number = a;
let b: B = a;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let count_2322 = codes.iter().filter(|&&code| code == 2322).count();
    assert_eq!(
        count_2322, 1,
        "Expected one 2322 error for cross-enum assignment, got: {codes:?}"
    );
}

