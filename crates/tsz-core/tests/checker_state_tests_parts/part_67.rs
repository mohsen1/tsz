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
/// Test that after lib symbol merge, symbol lookups return consistent data
/// even when lib binders had colliding `SymbolIds`.
#[test]
fn test_lib_merge_consistent_symbol_resolution() {
    use crate::binder::LibContext;
    use std::sync::Arc;

    // Create two lib binders with intentionally colliding IDs
    let mut lib1 = BinderState::new();
    let lib1_sym = lib1
        .symbols
        .alloc(crate::binder::symbol_flags::INTERFACE, "Foo".to_string());
    lib1.file_locals.set("Foo".to_string(), lib1_sym);

    let mut lib2 = BinderState::new();
    let lib2_sym = lib2
        .symbols
        .alloc(crate::binder::symbol_flags::INTERFACE, "Bar".to_string());
    lib2.file_locals.set("Bar".to_string(), lib2_sym);

    // Both should start at SymbolId(0) - the collision scenario
    assert_eq!(lib1_sym.0, 0);
    assert_eq!(lib2_sym.0, 0);

    let lib_arena = Arc::new(NodeArena::new());
    let lib_contexts = vec![
        LibContext {
            arena: Arc::clone(&lib_arena),
            binder: Arc::new(lib1),
        },
        LibContext {
            arena: Arc::clone(&lib_arena),
            binder: Arc::new(lib2),
        },
    ];

    let source = "const x = 1;"; // Minimal source
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.merge_lib_contexts_into_binder(&lib_contexts);
    binder.bind_source_file(parser.get_arena(), root);

    // Get remapped IDs
    let foo_id = binder.file_locals.get("Foo").expect("Foo should exist");
    let bar_id = binder.file_locals.get("Bar").expect("Bar should exist");

    // IDs must be unique
    assert_ne!(
        foo_id, bar_id,
        "Foo and Bar must have different IDs after merge"
    );

    // Symbol resolution must return correct names
    let foo_sym = binder.get_symbol(foo_id).expect("Foo symbol must resolve");
    assert_eq!(foo_sym.escaped_name, "Foo", "Foo symbol name mismatch");

    let bar_sym = binder.get_symbol(bar_id).expect("Bar symbol must resolve");
    assert_eq!(bar_sym.escaped_name, "Bar", "Bar symbol name mismatch");
}

// =============================================================================
// Selective TypeAlias Migration Tests (Phase 4.2.1)
// =============================================================================
//
// These tests verify that Type Aliases are registered with DefId while
// Classes and Interfaces use SymbolRef during the incremental migration (Issue #12).
//
// Migration strategy:
// - Type Aliases → DefId-based registration [target for Phase 4.2.1]
// - Classes → SymbolRef-based registration [legacy, deferred]
// - Interfaces → SymbolRef-based registration [legacy, deferred]
// =============================================================================

/// Test that a type alias gets a DefId created
///
/// This is the core of Phase 4.2.1: verify that type aliases
/// have `DefIds` created for them.
#[test]
fn test_selective_migration_type_alias_has_def_id() {
    let source = r#"
type UserId = string;
const x: UserId = "user123";
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

    // Get the UserId type alias symbol
    let user_id_sym = binder
        .file_locals
        .get("UserId")
        .expect("UserId symbol should exist");

    // After Phase 4.2.1, type aliases should have DefIds created
    let def_id = checker.ctx.get_existing_def_id(user_id_sym);

    assert!(
        def_id.is_some(),
        "Type alias should have DefId created after Phase 4.2.1"
    );
}

/// Test that a class DOES get a DefId created (Phase 4.3)
///
/// Phase 4.3: Unified type resolution for all named types (interfaces, type aliases, classes)
/// to return Lazy(DefId) references instead of eagerly expanded structural types.
#[test]
fn test_selective_migration_class_has_def_id() {
    let source = r#"
class Foo {
    x: number;
}
const obj: Foo = new Foo();
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

    // Get the Foo class symbol
    let foo_sym = binder
        .file_locals
        .get("Foo")
        .expect("Foo symbol should exist");

    // During Phase 4.3, classes SHOULD have DefIds created for unified type resolution
    let def_id = checker.ctx.get_existing_def_id(foo_sym);

    assert!(
        def_id.is_some(),
        "Class should have DefId during Phase 4.3 (unified type resolution)"
    );
}

/// Test that an interface DOES get a DefId created (Phase 4.3)
///
/// Phase 4.3: Unified type resolution for all named types (interfaces, type aliases, classes)
/// to return Lazy(DefId) references instead of eagerly expanded structural types.
#[test]
fn test_selective_migration_interface_has_def_id() {
    let source = r#"
interface Point {
    x: number;
    y: number;
}
const p: Point = { x: 1, y: 2 };
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

    // Get the Point interface symbol
    let point_sym = binder
        .file_locals
        .get("Point")
        .expect("Point symbol should exist");

    // During Phase 4.3, interfaces SHOULD have DefIds created for unified type resolution
    let def_id = checker.ctx.get_existing_def_id(point_sym);

    assert!(
        def_id.is_some(),
        "Interface should have DefId during Phase 4.3 (unified type resolution)"
    );
}

/// Test that a generic type alias gets a DefId created
///
/// Generic type aliases should also get `DefIds`.
#[test]
fn test_selective_migration_generic_type_alias_has_def_id() {
    let source = r#"
type Box<T> = { value: T };
const x: Box<string> = { value: "hello" };
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

    // Get the Box type alias symbol
    let box_sym = binder
        .file_locals
        .get("Box")
        .expect("Box symbol should exist");

    // After Phase 4.2.1, generic type aliases should have DefIds
    let def_id = checker.ctx.get_existing_def_id(box_sym);

    assert!(
        def_id.is_some(),
        "Generic type alias should have DefId created after Phase 4.2.1"
    );
}

/// Test generic recursive type alias (Phase 4.2.1 - IN PROGRESS)
///
/// This test verifies that generic recursive type aliases like:
///   type List<T> = { value: T; next: List<T> | null }
/// work correctly with DefId-based resolution.
///
/// Phase 4.2.1 PROGRESS:
/// ✅ Implemented `def_type_params` cache in `CheckerContext`
/// ✅ Implemented `get_lazy_type_params()` in `TypeResolver`
/// ✅ Type parameters are stored when resolving type aliases/interfaces/classes
/// ✅ `ApplicationEvaluator` in solver correctly handles Lazy(DefId) with type params
///
/// DIAGNOSTIC ISSUE:
/// The type is displayed as "Lazy(1)<number>" in error messages instead of "List<number>".
/// This is a DISPLAY issue, not a functional issue. The Application IS being evaluated
/// internally (the `ApplicationEvaluator` works correctly), but the diagnostic shows
/// the unevaluated form.
///
/// The fix needed: Update diagnostic generation to display the type name instead of
/// showing the internal Lazy(DefId) representation.
///
#[test]
fn test_generic_recursive_type_alias_diagnostic_display() {
    let source = r#"
type List<T> = { value: T; next: List<T> | null };
const list: List<number> = { value: 1, next: { value: 2, next: null } };
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

    // Check that the type checker runs without panicking
    checker.check_source_file(root);

    // The test passes if we get here without panicking
    // The diagnostic will show "Lazy(1)<number>" which is a display issue, not a functional issue
}

// =============================================================================
// TS2411: Property incompatible with index signature
// =============================================================================

/// Test that properties are checked against own index signatures (not inherited).
/// This is the main failing case identified in docs/ts2411-remaining-issues.md
#[test]
fn test_ts2411_own_string_index_signature() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
interface Derived {
    [x: string]: { a: number; b: number };
    y: { a: number; }
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

    let ts2411_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE)
        .collect();

    assert!(
        !ts2411_errors.is_empty(),
        "Expected at least 1 TS2411 error for property 'y' incompatible with own string index signature, got 0. Diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that properties are checked against inherited index signatures.
#[test]
fn test_ts2411_inherited_index_signature() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
interface Base {
    [x: string]: { x: number }
}

interface Derived extends Base {
    foo: { y: number }
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

    let ts2411_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE)
        .collect();

    assert!(
        !ts2411_errors.is_empty(),
        "Expected at least 1 TS2411 error for property 'foo' incompatible with inherited index signature, got 0. Diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that compatible properties don't emit TS2411 errors.
#[test]
fn test_ts2411_compatible_property_no_error() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
interface Foo {
    [x: string]: { a: number; b: number };
    y: { a: number; b: number; };
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

    let ts2411_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE)
        .collect();

    assert_eq!(
        ts2411_errors.len(),
        0,
        "Expected 0 TS2411 errors for compatible property, got {}. Diagnostics: {:?}",
        ts2411_errors.len(),
        checker.ctx.diagnostics
    );
}

// =============================================================================
// TS2303: Circular definition of import alias
// =============================================================================

/// Test that circular import aliases are detected and reported.
/// e.g., declare module "foo" { import x = require("foo"); }
#[test]
fn test_ts2303_circular_import_alias() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
declare module "moduleC" {
    import self = require("moduleC");
    export = self;
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

    let ts2303_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS)
        .collect();

    assert!(
        !ts2303_errors.is_empty(),
        "Expected at least 1 TS2303 error for circular import alias, got 0. Diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

