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
fn test_symbol_constructor_call_signature() {
    // Skip test - lib loading was removed
    // Tests that need lib files should use the TestContext API
}

#[test]
fn test_symbol_constructor_too_many_args() {
    // Skip test - lib loading was removed
    // Tests that need lib files should use the TestContext API
}

#[test]
fn test_variable_redeclaration_same_type() {
    use crate::parser::ParserState;

    // Test that redeclaring a variable with the same type is allowed
    let source = r#"function test() {
    var x: string;
    var x: string;
}"#;

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

    // Should have no errors - same type is allowed
    assert_eq!(checker.ctx.diagnostics.len(), 0);
}

#[test]
fn test_variable_redeclaration_different_type_2403() {
    use crate::parser::ParserState;

    // Test that redeclaring a variable with different type causes error TS2403
    // Must be inside a function where local scopes are active
    let source = r#"function test() {
    var x: string;
    var x: number;
}"#;

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

    // Should have error 2403: Subsequent variable declarations must have the same type
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2403),
        "Expected error 2403 for variable redeclaration, got: {codes:?}"
    );
}

#[test]
fn test_variable_self_reference_no_2403() {
    use crate::parser::ParserState;

    // Self-references in a var initializer should not trigger TS2403.
    let source = r#"function test() {
    var x = {
        x,
        parent: x
    };
}"#;

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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2403),
        "Expected no error 2403 for self-referential var initializer, got: {codes:?}"
    );
}

#[test]
fn test_param_var_redecl_ts2403() {
    use crate::parser::ParserState;

    // TS2403: var redeclaration of optional parameter with different type
    // `options?: number` has type `number | undefined`, var declares `number`
    let source = r#"class C {
    constructor(options?: number) {
        var options = (options || 0);
    }
}"#;

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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2403),
        "Expected TS2403 for parameter/var type mismatch, got: {codes:?}"
    );
}

#[test]
fn test_symbol_property_access_description() {
    use tsz_solver::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};

    // Test accessing .description on symbol type
    let types = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&types);

    let result = evaluator.resolve_property_access(TypeId::SYMBOL, "description");
    match result {
        PropertyAccessResult::Success {
            type_id: prop_type, ..
        } => {
            // description should be string | undefined
            let key = types.lookup(prop_type).expect("Property type should exist");
            match key {
                TypeData::Union(members) => {
                    let members = types.type_list(members);
                    assert_eq!(members.len(), 2);
                    assert!(members.contains(&TypeId::STRING));
                    assert!(members.contains(&TypeId::UNDEFINED));
                }
                _ => panic!("Expected union type for description, got: {key:?}"),
            }
        }
        _ => panic!("Expected Success for symbol.description, got: {result:?}"),
    }
}

#[test]
fn test_symbol_property_access_methods() {
    use tsz_solver::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};

    // Test accessing methods on symbol type
    let types = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&types);

    // toString and valueOf should return ANY for now (function types are complex)
    let result_to_string = evaluator.resolve_property_access(TypeId::SYMBOL, "toString");
    match result_to_string {
        PropertyAccessResult::Success {
            type_id: prop_type, ..
        } => {
            assert_eq!(prop_type, TypeId::ANY);
        }
        _ => panic!("Expected Success for symbol.toString, got: {result_to_string:?}"),
    }

    let result_value_of = evaluator.resolve_property_access(TypeId::SYMBOL, "valueOf");
    match result_value_of {
        PropertyAccessResult::Success {
            type_id: prop_type, ..
        } => {
            assert_eq!(prop_type, TypeId::ANY);
        }
        _ => panic!("Expected Success for symbol.valueOf, got: {result_value_of:?}"),
    }
}

#[test]
fn test_symbol_property_not_found() {
    use tsz_solver::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};

    // Test accessing non-existent property on symbol type
    let types = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&types);
    let name_atom = types.intern_string("nonexistent");

    let result = evaluator.resolve_property_access(TypeId::SYMBOL, "nonexistent");
    match result {
        PropertyAccessResult::PropertyNotFound {
            type_id,
            property_name,
        } => {
            assert_eq!(type_id, TypeId::SYMBOL);
            assert_eq!(property_name, name_atom);
        }
        _ => panic!("Expected PropertyNotFound for unknown property, got: {result:?}"),
    }
}

// ============== Property access from index signature tests (error 4111) ==============

#[test]
fn test_property_access_from_index_signature_4111() {
    use crate::parser::ParserState;

    let source = r#"
interface StringMap {
    [key: string]: number;
}
const obj: StringMap = {} as any;
const val = obj.someProperty;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        no_property_access_from_index_signature: true,
        ..Default::default()
    };
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
    assert!(
        codes.contains(&4111),
        "Expected error 4111 for property access from index signature, got: {codes:?}"
    );
}

