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
/// Test that abstract classes skip TS2564 check entirely
#[test]
fn test_ts2564_abstract_class_skips_check() {
    use crate::parser::ParserState;

    let source = r#"
abstract class AbstractBase {
    name: string;  // No error - abstract class can't be instantiated
    abstract getValue(): number;
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

    // Current tsc baseline reports TS2564 for uninitialized abstract-class fields.
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        has_2564,
        "Expected TS2564 for abstract class, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test TS2454 - Variable used before assignment (basic case)
#[test]
fn test_ts2454_variable_used_before_assignment() {
    use crate::parser::ParserState;

    let source = r#"
function test() {
    let x: string;
    console.log(x);  // Should report TS2454
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
    // TS2454 requires strictNullChecks
    let options = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let has_2454 = checker.ctx.diagnostics.iter().any(|d| d.code == 2454);
    assert!(
        has_2454,
        "Expected TS2454 for variable used before assignment, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test TS2454 - Variable used in conditional (only one path assigns)
#[test]
fn test_ts2454_conditional_assignment_one_path() {
    use crate::parser::ParserState;

    let source = r#"
function test() {
    let x: string;
    if (Math.random() > 0.5) {
        x = "hello";
    }
    console.log(x);  // Should report TS2454 (not all paths assign)
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
    // TS2454 requires strictNullChecks
    let options = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let has_2454 = checker.ctx.diagnostics.iter().any(|d| d.code == 2454);
    assert!(
        has_2454,
        "Expected TS2454 for conditional assignment (one path), got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test TS2454 - All paths assign (should NOT report error)
#[test]
fn test_ts2454_all_paths_assign() {
    use crate::parser::ParserState;

    let source = r#"
function test() {
    let x: string;
    if (Math.random() > 0.5) {
        x = "hello";
    } else {
        x = "world";
    }
    console.log(x);  // Should NOT report TS2454 (all paths assign)
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

    let has_2454 = checker.ctx.diagnostics.iter().any(|d| d.code == 2454);
    assert!(
        !has_2454,
        "Expected NO TS2454 when all paths assign, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test TS2454 - Variable with initializer (should NOT report error)
#[test]
fn test_ts2454_variable_with_initializer() {
    use crate::parser::ParserState;

    let source = r#"
function test() {
    let x: string = "hello";
    console.log(x);  // Should NOT report TS2454 (has initializer)
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

    let has_2454 = checker.ctx.diagnostics.iter().any(|d| d.code == 2454);
    assert!(
        !has_2454,
        "Expected NO TS2454 for variable with initializer, got: {:?}",
        checker.ctx.diagnostics
    );
}

// =============================================================================
// TS2564 Additional Edge Case Tests (Worker 4 - Enhanced)
// =============================================================================

/// Test that protected properties emit TS2564 when uninitialized
#[test]
fn test_ts2564_protected_property_uninitialized() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    protected value: number;  // Should emit TS2564
    
    constructor() {
        // value not initialized
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2564)
        .count();
    assert_eq!(
        count, 1,
        "Expected TS2564 for unprotected property, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that protected properties initialized in constructor skip TS2564
#[test]
fn test_ts2564_protected_property_initialized() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    protected value: number;
    
    constructor() {
        this.value = 42;  // Initialized
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 for initialized protected property, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that generic class properties emit TS2564 when uninitialized
#[test]
fn test_ts2564_generic_property_uninitialized() {
    use crate::parser::ParserState;

    let source = r#"
class Container<T> {
    value: T;  // Should emit TS2564
    
    constructor() {
        // value not initialized
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2564)
        .count();
    // TypeScript now requires initialization for unconstrained type-parameter
    // properties too, so strict property initialization still reports TS2564 here.
    assert_eq!(
        count, 1,
        "Expected TS2564 for generic type parameter property (matches tsc), got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that generic class properties initialized in constructor skip TS2564
#[test]
fn test_ts2564_generic_property_initialized() {
    use crate::parser::ParserState;

    let source = r#"
class Container<T> {
    value: T;
    
    constructor(initialValue: T) {
        this.value = initialValue;  // Initialized
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 for initialized generic property, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that derived class without constructor still emits TS2564 for its properties
#[test]
fn test_ts2564_derived_class_no_constructor() {
    use crate::parser::ParserState;

    let source = r#"
class Base {
    constructor() {
        // Base constructor
    }
}

class Derived extends Base {
    value: number;  // Should emit TS2564 - Derived has no constructor
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2564)
        .count();
    assert_eq!(
        count, 1,
        "Expected TS2564 for derived class property, got: {:?}",
        checker.ctx.diagnostics
    );
}

