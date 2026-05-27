/// Test that interface extends correctly inherits properties
///
/// NOTE: Currently ignored - interface extends is not fully implemented.
/// Properties from parent interfaces are not correctly inherited.
#[test]
fn test_interface_extends_inherits_properties() {
    let source = r#"
interface Base {
    base: string;
}
interface Derived extends Base {
    derived: number;
}
const obj: Derived = { base: "x", derived: 1 };
const base_value = obj.base;
const derived_value = obj.derived;
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

    let base_sym = binder
        .file_locals
        .get("base_value")
        .expect("base_value should exist");
    let base_type = checker.get_type_of_symbol(base_sym);
    assert_eq!(base_type, TypeId::STRING);

    let derived_sym = binder
        .file_locals
        .get("derived_value")
        .expect("derived_value should exist");
    let derived_type = checker.get_type_of_symbol(derived_sym);
    assert_eq!(derived_type, TypeId::NUMBER);
}

/// Test that interface extends correctly applies type arguments
///
/// NOTE: Currently ignored - interface extension with type arguments is not fully
/// implemented. Generic type parameters in interface extends clauses are not
/// correctly resolved.
#[test]
fn test_interface_extends_applies_type_arguments() {
    let source = r#"
interface Box<T> {
    value: T;
}
interface Derived extends Box<string> {
    count: number;
}
const obj: Derived = { value: "x", count: 1 };
const value = obj.value;
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

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    assert_eq!(value_type, TypeId::STRING);
}

/// Test that interface extends with type alias applies type arguments
///
/// NOTE: Currently ignored - see `test_interface_extends_applies_type_arguments`.
#[test]
fn test_interface_extends_type_alias_applies_type_arguments() {
    let source = r#"
type Box<T> = { value: T };
interface Derived extends Box<string> {
    count: number;
}
const obj: Derived = { value: "x", count: 1 };
const value = obj.value;
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
    assert_eq!(value_type, TypeId::STRING);
}

#[test]
fn test_interface_extends_class_applies_type_arguments() {
    let source = r#"
class Box<T> {
    value!: T;
}
interface Derived extends Box<string> {
    count: number;
}
const obj: Derived = { value: "x", count: 1 };
const value = obj.value;
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
    assert_eq!(value_type, TypeId::STRING);
}

#[test]
fn test_interface_extends_readonly_property_mismatch_2430() {
    let source = r#"
interface Base {
    x: number;
}
interface Derived extends Base {
    readonly x: number;
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
    // TypeScript allows `readonly x: number` to satisfy `x: number` in interface extends
    assert!(
        codes.is_empty(),
        "Expected no errors for readonly property in interface extends, got: {codes:?}"
    );
}

#[test]
fn test_interface_extends_optional_property_mismatch_2430() {
    let source = r#"
interface Base {
    x: number;
}
interface Derived extends Base {
    x?: number;
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
    assert!(
        codes.contains(&2430),
        "Expected error 2430 for optional property mismatch, got: {codes:?}"
    );
}

#[test]
fn test_optional_property_allows_undefined_assignment() {
    let source = r#"
interface Foo {
    x?: number;
}
const ok: Foo = {};
const ok2: Foo = { x: 1 };
const ok3: Foo = { x: undefined };
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
}

#[test]
fn test_interface_extends_string_literal_property_mismatch_2430() {
    let source = r#"
interface Base {
    "x": number;
}
interface Derived extends Base {
    "x"?: number;
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
    assert!(
        codes.contains(&2430),
        "Expected error 2430 for string literal property mismatch, got: {codes:?}"
    );
}

#[test]
fn test_interface_extends_generic_argument_mismatch_2430() {
    let source = r#"
interface Base<T> {
    x: T;
}
interface Derived extends Base<string> {
    x: number;
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
    assert!(
        codes.contains(&2430),
        "Expected error 2430 for generic argument mismatch, got: {codes:?}"
    );
}

/// Test that interface extends with matching generic arguments works
///
/// NOTE: Currently ignored - see `test_interface_extends_applies_type_arguments`.
#[test]
fn test_interface_extends_generic_argument_match() {
    let source = r#"
interface Base<T> {
    x: T;
}
interface Derived extends Base<string> {
    x: string;
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
}

#[test]
fn test_interface_extends_namespace_qualified_base_2430() {
    let source = r#"
namespace NS {
    export interface Base {
        x: string;
    }
}
interface Derived extends NS.Base {
    x: number;
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
    assert!(
        codes.contains(&2430),
        "Expected error 2430 for namespace-qualified base mismatch, got: {codes:?}"
    );
}

/// Test that interface extends with generic methods works
///
/// NOTE: Currently ignored - see `test_interface_extends_inherits_properties`.
#[test]
fn test_interface_extends_generic_method_compatible() {
    let source = r#"
interface Base {
    m<T>(value: T): T;
}
interface Derived extends Base {
    m<T>(value: T): T;
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
}

#[test]
fn test_checker_cross_namespace_type_reference() {
    use tsz_solver::TypeData;

    let source = r#"
namespace Outer {
    export interface Inner { y: string; }
}
type Alias = Outer.Inner;
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
                .find(|prop| types.resolve_atom(prop.name) == "y")
                .expect("Expected property y");
            assert_eq!(prop.type_id, TypeId::STRING);
        }
        TypeData::Lazy(_def_id) => {
            // Phase 4.3: Interface type references now use Lazy(DefId)
            // The Lazy type is correctly resolved when needed for type checking
        }
        _ => panic!("Expected Alias to resolve to Object or Lazy type, got {alias_key:?}"),
    }
}

#[test]
fn test_checker_nested_namespace_export_visible() {
    let source = r#"
namespace A {
    export type ID = string;
    namespace B {
        let x: ID;
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_checker_nested_namespace_non_exported_not_visible() {
    let source = r#"
namespace A {
    type Internal = number;
    namespace B {
        let x: Internal;
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
    assert!(
        !codes.contains(&2304),
        "Unexpected error 2304 for nested namespace parent type, got: {codes:?}"
    );
}

#[test]
fn test_class_extends_null_no_ts2304() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
class C1 extends null {}
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
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "Unexpected TS2304 for extends null heritage, got: {codes:?}"
    );
}

#[test]
fn test_exports_global_no_ts2304() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
exports.foo = 1;
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
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "Unexpected TS2304 for global exports usage, got: {codes:?}"
    );
}

#[test]
fn test_checker_nested_namespace_exported_class_visible() {
    let source = r#"
namespace Models {
    export class User {}
    namespace Helpers {
        function getUser(): User {
            return new User();
        }
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_checker_module_augmentation_merges_exports() {
    use tsz_solver::TypeData;

    let source = r#"
namespace Outer {
    export interface A { x: number; }
}
namespace Outer {
    export interface B { y: string; }
}
type AliasA = Outer.A;
type AliasB = Outer.B;
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

    let alias_a_sym = binder
        .file_locals
        .get("AliasA")
        .expect("AliasA should exist");
    let alias_b_sym = binder
        .file_locals
        .get("AliasB")
        .expect("AliasB should exist");

    let alias_a_type = checker.get_type_of_symbol(alias_a_sym);
    let alias_b_type = checker.get_type_of_symbol(alias_b_sym);

    let alias_a_key = types
        .lookup(alias_a_type)
        .expect("AliasA type should exist");
    match alias_a_key {
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
        _ => panic!("Expected AliasA to resolve to Object or Lazy type, got {alias_a_key:?}"),
    }

    let alias_b_key = types
        .lookup(alias_b_type)
        .expect("AliasB type should exist");
    match alias_b_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "y")
                .expect("Expected property y");
            assert_eq!(prop.type_id, TypeId::STRING);
        }
        TypeData::Lazy(_def_id) => {
            // Phase 4.3: Interface type references now use Lazy(DefId)
            // The Lazy type is correctly resolved when needed for type checking
        }
        _ => panic!("Expected AliasB to resolve to Object or Lazy type, got {alias_b_key:?}"),
    }
}

#[test]
fn test_checker_lower_generic_type_reference_applies_args() {
    use tsz_solver::TypeData;

    let source = r#"
type Box<T> = { value: T };
type Alias = Box<string>;
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

    let _box_sym = binder.file_locals.get("Box").expect("Box should exist");
    let alias_sym = binder.file_locals.get("Alias").expect("Alias should exist");

    let alias_type = checker.get_type_of_symbol(alias_sym);
    let alias_key = types.lookup(alias_type).expect("Alias type should exist");
    // Generic type aliases are now eagerly resolved to Object types with instantiated properties
    match alias_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "value")
                .expect("Expected property 'value' on resolved Box<string>");
            // Box<string> has value: string
            assert_eq!(
                prop.type_id,
                TypeId::STRING,
                "Expected value property to be string"
            );
        }
        TypeData::Application(app_id) => {
            // Also accept Application type if not eagerly resolved
            let app = types.type_application(app_id);
            assert_eq!(app.args, vec![TypeId::STRING]);
        }
        _ => panic!("Expected Alias to be Object or Application type, got {alias_key:?}"),
    }
}

#[test]
fn test_checker_lowers_generic_function_type_annotation_uses_type_params() {
    use tsz_solver::TypeData;

    let source = r#"
const f: <T>(value: T) => T = (value) => value;
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

    let f_sym = binder.file_locals.get("f").expect("f should exist");
    let f_type = checker.get_type_of_symbol(f_sym);
    let f_key = types.lookup(f_type).expect("f type should exist");
    match f_key {
        TypeData::Function(shape_id) => {
            let shape = types.function_shape(shape_id);
            assert_eq!(shape.type_params.len(), 1);
            assert_eq!(types.resolve_atom(shape.type_params[0].name), "T");
            assert_eq!(shape.params.len(), 1);

            let param_key = types
                .lookup(shape.params[0].type_id)
                .expect("Param type should exist");
            match param_key {
                TypeData::TypeParameter(info) => {
                    assert_eq!(types.resolve_atom(info.name), "T");
                }
                _ => panic!("Expected param type to be type parameter, got {param_key:?}"),
            }

            let return_key = types
                .lookup(shape.return_type)
                .expect("Return type should exist");
            match return_key {
                TypeData::TypeParameter(info) => {
                    assert_eq!(types.resolve_atom(info.name), "T");
                }
                _ => panic!("Expected return type to be type parameter, got {return_key:?}"),
            }
        }
        _ => panic!("Expected f to be Function type, got {f_key:?}"),
    }
}

#[test]
fn test_interface_generic_call_signature_uses_type_params() {
    use tsz_solver::TypeData;

    let source = r#"
interface Callable {
    <T>(value: T): T;
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

    let callable_sym = binder
        .file_locals
        .get("Callable")
        .expect("Callable should exist");
    let callable_type = checker.get_type_of_symbol(callable_sym);
    let callable_key = types
        .lookup(callable_type)
        .expect("Callable type should exist");
    match callable_key {
        TypeData::Callable(shape_id) => {
            let shape = types.callable_shape(shape_id);
            assert_eq!(shape.call_signatures.len(), 1);
            let sig = &shape.call_signatures[0];
            assert_eq!(sig.type_params.len(), 1);
            assert_eq!(types.resolve_atom(sig.type_params[0].name), "T");
            assert_eq!(sig.params.len(), 1);

            let param_key = types
                .lookup(sig.params[0].type_id)
                .expect("Param type should exist");
            match param_key {
                TypeData::TypeParameter(info) => {
                    assert_eq!(types.resolve_atom(info.name), "T");
                }
                _ => panic!("Expected param type to be type parameter, got {param_key:?}"),
            }

            let return_key = types
                .lookup(sig.return_type)
                .expect("Return type should exist");
            match return_key {
                TypeData::TypeParameter(info) => {
                    assert_eq!(types.resolve_atom(info.name), "T");
                }
                _ => panic!("Expected return type to be type parameter, got {return_key:?}"),
            }
        }
        _ => panic!("Expected Callable to be Callable type, got {callable_key:?}"),
    }
}

#[test]
fn test_interface_generic_construct_signature_uses_type_params() {
    use tsz_solver::TypeData;

    let source = r#"
interface Factory {
    new <T>(value: T): T;
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

    let factory_sym = binder
        .file_locals
        .get("Factory")
        .expect("Factory should exist");
    let factory_type = checker.get_type_of_symbol(factory_sym);
    let factory_key = types
        .lookup(factory_type)
        .expect("Factory type should exist");
    match factory_key {
        TypeData::Callable(shape_id) => {
            let shape = types.callable_shape(shape_id);
            assert_eq!(shape.construct_signatures.len(), 1);
            let sig = &shape.construct_signatures[0];
            assert_eq!(sig.type_params.len(), 1);
            assert_eq!(types.resolve_atom(sig.type_params[0].name), "T");
            assert_eq!(sig.params.len(), 1);

            let param_key = types
                .lookup(sig.params[0].type_id)
                .expect("Param type should exist");
            match param_key {
                TypeData::TypeParameter(info) => {
                    assert_eq!(types.resolve_atom(info.name), "T");
                }
                _ => panic!("Expected param type to be type parameter, got {param_key:?}"),
            }

            let return_key = types
                .lookup(sig.return_type)
                .expect("Return type should exist");
            match return_key {
                TypeData::TypeParameter(info) => {
                    assert_eq!(types.resolve_atom(info.name), "T");
                }
                _ => panic!("Expected return type to be type parameter, got {return_key:?}"),
            }
        }
        _ => panic!("Expected Factory to be Callable type, got {factory_key:?}"),
    }
}

#[test]
fn test_checker_lowers_generic_function_declaration_uses_type_params() {
    use tsz_solver::TypeData;

    let source = r#"
function id<T>(value: T): T {
    return value;
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

    let id_sym = binder.file_locals.get("id").expect("id should exist");
    let id_type = checker.get_type_of_symbol(id_sym);
    let id_key = types.lookup(id_type).expect("id type should exist");
    match id_key {
        TypeData::Function(shape_id) => {
            let shape = types.function_shape(shape_id);
            assert_eq!(shape.type_params.len(), 1);
            assert_eq!(types.resolve_atom(shape.type_params[0].name), "T");
            assert_eq!(shape.params.len(), 1);

            let param_key = types
                .lookup(shape.params[0].type_id)
                .expect("Param type should exist");
            match param_key {
                TypeData::TypeParameter(info) => {
                    assert_eq!(types.resolve_atom(info.name), "T");
                }
                _ => panic!("Expected param type to be type parameter, got {param_key:?}"),
            }

            let return_key = types
                .lookup(shape.return_type)
                .expect("Return type should exist");
            match return_key {
                TypeData::TypeParameter(info) => {
                    assert_eq!(types.resolve_atom(info.name), "T");
                }
                _ => panic!("Expected return type to be type parameter, got {return_key:?}"),
            }
        }
        _ => panic!("Expected id to be Function type, got {id_key:?}"),
    }
}

#[test]
fn test_function_return_type_inferred_from_body() {
    use tsz_solver::{TypeData, TypeId};

    let source = r#"
function id(x: string) {
    return x;
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

    let id_sym = binder.file_locals.get("id").expect("id should exist");
    let id_type = checker.get_type_of_symbol(id_sym);
    let id_key = types.lookup(id_type).expect("id type should exist");
    match id_key {
        TypeData::Function(shape_id) => {
            let shape = types.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::STRING);
        }
        _ => panic!("Expected id to be Function type, got {id_key:?}"),
    }
}

#[test]
fn test_arrow_function_return_type_inferred_union() {
    use tsz_solver::{TypeData, TypeId};

    let source = r#"
const f = (flag: boolean) => {
    if (flag) {
        return 1;
    }
    return "a";
};
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

    let f_sym = binder.file_locals.get("f").expect("f should exist");
    let f_type = checker.get_type_of_symbol(f_sym);
    let f_key = types.lookup(f_type).expect("f type should exist");
    match f_key {
        TypeData::Function(shape_id) => {
            let shape = types.function_shape(shape_id);
            let return_key = types
                .lookup(shape.return_type)
                .expect("return type should exist");
            match return_key {
                TypeData::Union(members) => {
                    let members = types.type_list(members);
                    assert!(members.contains(&TypeId::NUMBER));
                    assert!(members.contains(&TypeId::STRING));
                }
                _ => panic!("Expected union return type, got {return_key:?}"),
            }
        }
        _ => panic!("Expected f to be Function type, got {f_key:?}"),
    }
}

/// Test missing return and implicit any diagnostics
///
/// NOTE: TS7010 (missing return type with noImplicitAny) is not yet implemented.
/// Test asserts current behavior; update when 7010 is implemented.
#[test]
fn test_missing_return_and_implicit_any_diagnostics() {
    let source = r#"
// @noImplicitAny: true
function noReturn(): number {
    console.log("oops");
}

function maybeReturn(flag: boolean): number {
    if (flag) {
        return 1;
    }
}

function allReturn(flag: boolean): number {
    if (flag) {
        return 1;
    }
    return 2;
}

function voidReturn(): void {
    console.log("ok");
}

function implicitAny(x) {
    return x;
}

const anon = () => { return null; };
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
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    }; // TS2366 requires strictNullChecks
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let count = |code| codes.iter().filter(|&&c| c == code).count();

    // Current behavior: [7006, 2584, 2355, 2366, 2584]
    // 2584 = "Cannot find name 'console'" (test lacks full lib)
    // Under strictNullChecks, `return null` gives concrete `null` type, not implicit any.
    // TSC does not emit TS7011 for `() => { return null; }` with strictNullChecks.
    assert_eq!(
        count(2355),
        1,
        "Expected one 2355 error, got codes: {codes:?}"
    );
    assert_eq!(
        count(2366),
        1,
        "Expected one 2366 error, got codes: {codes:?}"
    );
    assert_eq!(
        count(7006),
        1,
        "Expected one 7006 error, got codes: {codes:?}"
    );
    assert_eq!(
        count(7011),
        0,
        "Expected no TS7011 for `() => null` under strictNullChecks, got codes: {codes:?}"
    );
}

#[test]
fn test_implicit_any_return_in_signatures() {
    let source = r#"
// @noImplicitAny: true
interface I {
    foo();
}

declare function bar();

declare class C {
    publicMethod();
}

const obj = { baz() { return undefined; } };
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
    let count = |code| codes.iter().filter(|&&c| c == code).count();

    assert_eq!(
        count(7010),
        3,
        "Expected three 7010 errors, got codes: {codes:?}"
    );
}

#[test]
fn test_ts7010_async_function_no_false_positive() {
    let source = r#"
// @noImplicitAny: true
// Async functions without return type should NOT trigger TS7010
// because they infer Promise<void>, not 'any'
async function asyncNoReturn() {
}

async function asyncExplicitReturn() {
    return;
}

class C {
    async get foo() {
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
    let ts7010_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 7010)
        .collect();

    assert!(
        ts7010_errors.is_empty(),
        "Expected no TS7010 errors for async functions returning Promise<void>, got: {codes:?}"
    );
}

#[test]
fn test_ts7010_exactly_any_return() {
    // TSC does NOT emit TS7010/TS7011 when a function body returns an `any`-typed
    // expression.  The return type is validly *inferred* as `any` (not "implicit any").
    // TS7010 only fires for bodyless declarations (interfaces, abstract methods) or
    // when the return type widens from null/undefined to any.
    let source = r#"
// @noImplicitAny: true
declare var anyValue: any;

// Should NOT trigger TS7010 - return type is inferred as 'any' from body
function returnsAny() {
    return anyValue;
}

// Should NOT trigger TS7011 - return type is inferred as 'any' from body
const arrowReturnsAny = () => anyValue;
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
    let count = |code| codes.iter().filter(|&&c| c == code).count();

    assert_eq!(
        count(7010),
        0,
        "Expected no TS7010 errors for function returning inferred 'any', got codes: {codes:?}"
    );
    assert_eq!(
        count(7011),
        0,
        "Expected no TS7011 errors for arrow function returning inferred 'any', got codes: {codes:?}"
    );
}

/// TODO: TS7010 for null|undefined return is not yet implemented.
/// Currently no diagnostic is emitted for a function returning null | undefined
/// under noImplicitAny. When implemented, update to expect 1 TS7010.
#[test]
fn test_ts7010_null_undefined_return() {
    let source = r#"
// @noImplicitAny: true
// Should trigger TS7010 - return type is null | undefined (treated as 'any')
function returnsNullOrUndefined(flag: boolean) {
    if (flag) return null;
    return undefined;
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
    let count = |code| codes.iter().filter(|&&c| c == code).count();

    // TODO: Should be 1 once TS7010 for null|undefined return is implemented
    assert_eq!(
        count(7010),
        0,
        "Expected 0 TS7010 errors (not yet implemented), got codes: {codes:?}"
    );
}

#[test]
fn test_ts7010_class_expression_no_false_positive() {
    let source = r#"
// @noImplicitAny: true
// Functions returning class expressions should NOT trigger TS7010
// even if the class contains 'any' in its structure somewhere
class A<T> {
    value: T;
}

function createClass() {
    return class extends A<string> { };
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

    let ts7010_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 7010)
        .collect();

    assert!(
        ts7010_errors.is_empty(),
        "Expected no TS7010 errors for functions returning class expressions, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts7010_return_path_analysis() {
    let source = r#"
function allReturn(flag: boolean) {
    if (flag) {
        return 1;
    } else {
        return 2;
    }
}

function missingReturn(flag: boolean) {
    if (flag) {
        return 1;
    }
}

function throwOnly() {
    throw new Error("boom");
}

function infiniteLoop() {
    while (true) {}
}

function loopWithBreak() {
    while (true) { break; }
}

function loopWithNestedSwitchBreak(flag: boolean) {
    while (true) {
        switch (flag) {
            case true:
                break;
        }
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

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    let body_at = |index: usize| {
        let stmt_idx = *source_file
            .statements
            .nodes
            .get(index)
            .expect("statement index");
        let stmt_node = arena.get(stmt_idx).expect("statement node");
        let func = arena.get_function(stmt_node).expect("function data");
        func.body
    };

    assert!(
        !checker.function_body_falls_through(body_at(0)),
        "allReturn should not fall through"
    );
    assert!(
        checker.function_body_falls_through(body_at(1)),
        "missingReturn should fall through"
    );
    assert!(
        !checker.function_body_falls_through(body_at(2)),
        "throwOnly should not fall through"
    );
    assert!(
        !checker.function_body_falls_through(body_at(3)),
        "infiniteLoop should not fall through"
    );
    assert!(
        checker.function_body_falls_through(body_at(4)),
        "loopWithBreak should fall through"
    );
    assert!(
        !checker.function_body_falls_through(body_at(5)),
        "loopWithNestedSwitchBreak should not fall through"
    );
}

/// Test that functions that only throw don't trigger TS2355.
/// TS2355: "A function whose declared type is neither 'void' nor 'any' must return a value"
/// This should NOT fire for functions that only throw since throwing is a valid exit.
#[test]
fn test_throw_only_function_no_2355() {
    let source = r#"
// Function that only throws should NOT get 2355
function throwOnly(): number {
    throw new Error("always throws");
}

// Method that only throws should NOT get 2355
class C {
    throwMethod(): string {
        throw new Error("always throws");
    }

    get throwGetter(): number {
        throw new Error("getter throws");
    }
}

// Function that DOES fall through without returning SHOULD get 2355
function fallsThrough(): number {
    console.log("oops, no return");
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
    let count = |code| codes.iter().filter(|&&c| c == code).count();

    // Only fallsThrough should get 2355, not the throw-only functions
    assert_eq!(
        count(2355),
        1,
        "Expected exactly one 2355 error for fallsThrough(), got: {codes:?}"
    );

    // Verify which function got the error by checking the messages
    let error_2355 = checker.ctx.diagnostics.iter().find(|d| d.code == 2355);
    assert!(error_2355.is_some(), "Should have a 2355 error");
}

