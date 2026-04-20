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
fn test_use_before_assignment_for_of_initializer() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
function foo(items: number[]) {
    let x: number;
    for (x of items) {
        x;
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

    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED)
        .count();
    assert_eq!(
        count, 0,
        "Expected no use-before-assignment errors, got: {:?}",
        checker.ctx.diagnostics
    );
}

// Test for-in with external variable: `let k: string; for (k in obj) { k; }`
#[test]
fn test_use_before_assignment_for_in_initializer() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
function foo(obj: Record<string, number>) {
    let k: string;
    for (k in obj) {
        k;
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

    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED)
        .count();
    assert_eq!(
        count, 0,
        "Expected no use-before-assignment errors for for-in, got: {:?}",
        checker.ctx.diagnostics
    );
}

// =============================================================================
// TS2564 Additional Edge Case Tests (Worker 2)
// =============================================================================

/// Test that required properties without initialization emit TS2564
#[test]
fn test_ts2564_required_property_emits_error() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    name: string;
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
            strict_property_initialization: true,
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
        "Expected TS2564 for uninitialized property, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that properties with `undefined` in their type skip TS2564
#[test]
fn test_ts2564_union_with_undefined_skips_check() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    name: string | undefined;
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

    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 errors for undefined union, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that optional properties skip TS2564 check
#[test]
fn test_ts2564_optional_property_skips_check() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    name?: string;
    value?: number;
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

    // Optional properties should not have TS2564 errors
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 errors for optional properties, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that definite assignment assertion (!) skips TS2564 check
#[test]
fn test_ts2564_definite_assignment_assertion_skips_check() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    name!: string;
    value!: number;
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

    // Definite assignment assertion should not have TS2564 errors
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 errors for definite assignment assertion, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that properties with initializers skip TS2564 check
#[test]
fn test_ts2564_property_with_initializer_skips_check() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    name: string = "default";
    value: number = 42;
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

    // Properties with initializers should not have TS2564 errors
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 errors for properties with initializers, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that static properties skip TS2564 check (static fields have different semantics)
#[test]
fn test_ts2564_static_property_skips_check() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    static name: string;
    static value: number;
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

    // Static properties should not have TS2564 errors (different initialization semantics)
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 errors for static properties, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that properties assigned directly in constructor skip TS2564 check
#[test]
fn test_ts2564_simple_constructor_assignment() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    name: string;
    value: number;
    constructor() {
        this.name = "assigned";
        this.value = 123;
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

    // Properties assigned in constructor should not have TS2564 errors
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 errors for properties assigned in constructor, got: {:?}",
        checker.ctx.diagnostics
    );
}

// =============================================================================
// TS2564 Additional Edge Case Tests (Worker 4 - Fixed Bugs)
// =============================================================================

/// Test that switch statements without default case emit TS2564
#[test]
fn test_ts2564_switch_without_default_emits_error() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    value: number;
    constructor(type: number) {
        switch (type) {
            case 0:
                this.value = 0;
                break;
            case 1:
                this.value = 1;
                break;
            // No default case - might not execute
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            strict_property_initialization: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should have TS2564 because switch might not execute any case
    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2564)
        .count();
    assert_eq!(
        count, 1,
        "Expected TS2564 for switch without default, got: {:?}",
        checker.ctx.diagnostics
    );
}

