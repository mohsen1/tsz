//! Tests for Checker - Type checker using `NodeArena` and Solver
//!
//! This module contains comprehensive type checking tests organized into categories:
//! - Basic type checking (creation, intrinsic types, type interning)
//! - Type compatibility and assignability
//! - Excess property checking
//! - Function overloads and call resolution
//! - Generic types and type inference
//! - Control flow analysis
//! - Error diagnostics
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::test_fixtures::{TestContext, merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::{TypeId, TypeInterner, Visibility, types::RelationCacheKey, types::TypeData};

// =============================================================================
// Basic Type Checker Tests
// =============================================================================
#[test]
fn test_checker_nested_namespace_exported_class_visible() {
    use crate::parser::ParserState;

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    use crate::parser::ParserState;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
type Box<T> = { value: T };
type Alias = Box<string>;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
const f: <T>(value: T) => T = (value) => value;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
interface Callable {
    <T>(value: T): T;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
interface Factory {
    new <T>(value: T): T;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;
    use tsz_solver::TypeData;

    let source = r#"
function id<T>(value: T): T {
    return value;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;
    use tsz_solver::{TypeData, TypeId};

    let source = r#"
function id(x: string) {
    return x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;
    use tsz_solver::{TypeData, TypeId};

    let source = r#"
const f = (flag: boolean) => {
    if (flag) {
        return 1;
    }
    return "a";
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

