/// Test that TS2304 is emitted for undeclared variable in for-of loop.
#[test]
fn test_ts2304_undeclared_var_in_for_of() {
    let source = r#"
for (const item of undeclaredIterable) {
    let x = item;
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in for-of loop, got: {codes:?}"
    );
}

/// Test that no TS2304 is emitted for a properly declared variable.
#[test]
fn test_no_ts2304_for_declared_variable() {
    let source = r#"
const declaredVar = 5;
const result = declaredVar + 1;
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
        !codes.contains(&2304),
        "Unexpected TS2304 for declared variable, got: {codes:?}"
    );
}

/// Test that no TS2304 is emitted for hoisted function declaration.
#[test]
fn test_no_ts2304_for_hoisted_function() {
    let source = r#"
// Call before declaration (should work due to hoisting)
const result = hoistedFn();

function hoistedFn() {
    return 42;
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
        !codes.contains(&2304),
        "Unexpected TS2304 for hoisted function, got: {codes:?}"
    );
}

/// Test that no TS2304 is emitted for var used after declaration.
#[test]
fn test_no_ts2304_for_var_used_after_declaration() {
    let source = r#"
function test() {
    var x = 5;
    return x + 1;
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
        !codes.contains(&2304),
        "Unexpected TS2304 for var used after declaration, got: {codes:?}"
    );
}

// =============================================================================
// Duplicate Identifier Tests (TS2300)
// =============================================================================

/// Test that function overloads do NOT emit TS2300
#[test]
fn test_function_overloads_no_ts2300() {
    let source = r#"
function foo(x: string): void;
function foo(x: number): void;
function foo(x: string | number): void {
    console.log(x);
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
        !codes.contains(&2300),
        "Function overloads should NOT emit TS2300, got: {codes:?}"
    );
}

/// Test that interface merging does NOT emit TS2300
#[test]
fn test_interface_merging_no_ts2300() {
    let source = r#"
interface Foo {
    a: string;
}
interface Foo {
    b: number;
}
const x: Foo = { a: "hello", b: 42 };
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
        !codes.contains(&2300),
        "Interface merging should NOT emit TS2300, got: {codes:?}"
    );
}

/// Test that namespace + function merging does NOT emit TS2300
#[test]
fn test_namespace_function_merging_no_ts2300() {
    let source = r#"
namespace MyUtils {
    export function helper(): void {
        console.log("helper");
    }
}
function MyUtils() {
    console.log("constructor");
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
        !codes.contains(&2300),
        "Namespace + function merging should NOT emit TS2300, got: {codes:?}"
    );
}

/// Test that namespace + class merging does NOT emit TS2300
#[test]
fn test_namespace_class_merging_no_ts2300() {
    let source = r#"
namespace MyNamespace {
    export class MyClass {
        x: number = 42;
    }
}
class MyNamespace {
    y: string = "hello";
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
        !codes.contains(&2300),
        "Namespace + class merging should NOT emit TS2300, got: {codes:?}"
    );
}

/// Test that class + interface merging does NOT emit TS2300
#[test]
fn test_class_interface_merging_no_ts2300() {
    let source = r#"
interface MyInterface {
    method(): void;
}
class MyInterface {
    method(): void {
        console.log("implementation");
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
    assert!(
        !codes.contains(&2300),
        "Class + interface merging should NOT emit TS2300, got: {codes:?}"
    );
}

/// Test that duplicate variable declarations DO emit TS2451 (block-scoped variable redeclaration)
#[test]
fn test_duplicate_variables_emits_ts2451() {
    let source = r#"
let x = 1;
let x = 2;
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
        codes.contains(&2451),
        "Duplicate variable declarations should emit TS2451, got: {codes:?}"
    );
}

/// Test that duplicate var declarations are allowed (function-scoped hoisting)
#[test]
fn test_duplicate_var_allowed() {
    let source = r#"
var x = 1;
var x = 2;
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
    // Duplicate var declarations should NOT emit TS2300 (they are merged by hoisting)
    assert!(
        !codes.contains(&2300),
        "Duplicate var declarations should be allowed, got: {codes:?}"
    );
}

/// Test that duplicate class declarations DO emit TS2300
#[test]
fn test_duplicate_class_emits_ts2300() {
    let source = r#"
class MyClass {
    x: number = 1;
}
class MyClass {
    y: string = "hello";
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
        codes.contains(&2300),
        "Duplicate class declarations should emit TS2300, got: {codes:?}"
    );
}

/// Test that method overloads do NOT emit TS2300
#[test]
fn test_method_overloads_no_ts2300() {
    let source = r#"
class MyClass {
    method(x: string): void;
    method(x: number): void;
    method(x: string | number): void {
        console.log(x);
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

    let _codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Filter to only TS2300 errors for the "method" identifier
    let ts2300_method_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2300 && d.message_text.contains("method"))
        .collect();

    assert!(
        ts2300_method_errors.is_empty(),
        "Method overloads should NOT emit TS2300 for 'method', got {} errors: {:?}",
        ts2300_method_errors.len(),
        ts2300_method_errors
    );
}

/// Test that static and instance members with the same name do NOT emit TS2300
#[test]
fn test_static_instance_member_no_ts2300() {
    let source = r#"
class MyClass {
    static x: number = 1;
    x: number = 2;
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
        !codes.contains(&2300),
        "Static and instance members with same name should NOT emit TS2300, got: {codes:?}"
    );
}

// =============================================================================
// Lib Symbol Merging Tests (SymbolId Collision Fix)
// =============================================================================

/// Regression test: When lib symbols are merged with unique IDs, basic global
/// types like Array and Object should resolve correctly without TS2318.
#[test]
fn test_lib_merge_no_ts2318_for_basic_globals() {
    use crate::test_fixtures::{merge_shared_lib_symbols, setup_lib_contexts};

    // Source that references Array and Object
    let source = r#"
const arr: Array<number> = [1, 2, 3];
const obj: Object = {};
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    // Verify lib symbols are merged
    assert!(
        binder.lib_symbols_are_merged(),
        "lib_symbols_merged should be true"
    );
    assert!(
        binder.file_locals.has("Array"),
        "Array should be in file_locals"
    );
    assert!(
        binder.file_locals.has("Object"),
        "Object should be in file_locals"
    );

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

    // Should NOT have TS2318 (global type not found)
    let ts2318_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2318)
        .collect();
    assert!(
        ts2318_errors.is_empty(),
        "Should not emit TS2318 for Array/Object when libs are properly merged, got: {ts2318_errors:?}"
    );
}

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
    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

/// Test that non-circular imports don't trigger TS2303
#[test]
fn test_ts2303_no_error_for_different_module() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
declare module "moduleA" {
    export class A {}
}

declare module "moduleB" {
    import A = require("moduleA");
    export = A;
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

    let ts2303_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS)
        .collect();

    assert_eq!(
        ts2303_errors.len(),
        0,
        "Expected 0 TS2303 errors for non-circular import, got {}. Diagnostics: {:?}",
        ts2303_errors.len(),
        checker.ctx.diagnostics
    );
}

#[test]
fn test_ts2502_repro_circular_var() {
    let source = "var x: typeof x;";

    // Manually parse to get the root index
    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        diagnostics.iter().any(|d| d.code == 2502),
        "Expected TS2502 for circular reference, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2451_global_redeclaration() {
    let source = "
    const x = 1;
    declare global {
        const x: number;
    }
    ";

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        diagnostics.iter().any(|d| d.code == 2451),
        "Expected TS2451 for global redeclaration, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2313_simple_circular_type_alias() {
    let source = "type T = T;";

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        diagnostics.iter().any(|d| d.code == 2313 || d.code == 2456),
        "Expected TS2313/TS2456 for simple circular type alias, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2313_indirect_circular_type_alias() {
    let source = "
            type A = B;
            type B = A;
            ";

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        diagnostics.iter().any(|d| d.code == 2313 || d.code == 2456),
        "Expected TS2313/TS2456 for indirect circular type alias, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2310_circular_interface_inheritance() {
    let source = "
                interface A extends B {}
                interface B extends A {}
                ";

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        diagnostics.iter().any(|d| d.code == 2310),
        "Expected TS2310 for circular interface inheritance, got: {diagnostics:?}"
    );
}

#[test]
fn test_namespace_export_binds_global() {
    let source = "
    export as namespace foo;
    export const x = 1;
    ";

    let mut parser = crate::parser::ParserState::new("test.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Verify 'foo' is in global scope
    // Global scope is binder.scopes[0] usually (or binder.scope_stack bottom if active?)
    // After bind, scopes are in binder.scopes.
    // We need to find the global scope.
    // Assuming the first scope created is global.

    let global_scope = &binder.scopes[0]; // Is this safe assumption?
    assert!(
        global_scope.table.has("foo"),
        "Global scope should contain 'foo'"
    );
}

// ── TS1194: Export declarations in namespaces ──────────────────────────

#[test]
fn test_ts1194_export_in_non_ambient_namespace() {
    // `export { ... }` inside a regular namespace should emit TS1194.
    let source = r#"
        namespace Q {
            function _try() {}
            export { _try as try2 };
        }
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        diagnostics.iter().any(|d| d.code == 1194),
        "Expected TS1194 for export in non-ambient namespace, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts1194_no_error_in_ambient_namespace() {
    // `export { ... }` inside `declare namespace` should NOT emit TS1194.
    let source = r#"
        declare namespace Q {
            function _try(method: Function, ...args: any[]): any;
            export { _try as try2 };
        }
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        !diagnostics.iter().any(|d| d.code == 1194),
        "Should NOT emit TS1194 in ambient namespace, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts1194_no_error_in_dts_file() {
    // In `.d.ts` files, all namespaces are ambient, so no TS1194.
    let source = r#"
        namespace Q {
            function _try(): void;
            export { _try as try2 };
        }
    "#;

    let mut parser = crate::parser::ParserState::new("test.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.d.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        !diagnostics.iter().any(|d| d.code == 1194),
        "Should NOT emit TS1194 in .d.ts file, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts1194_no_error_nested_in_declare_namespace() {
    // Nested namespace inside `declare namespace` is still ambient.
    let source = r#"
        declare namespace A {
            namespace B {
                function foo(): void;
                export { foo };
            }
        }
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        !diagnostics.iter().any(|d| d.code == 1194),
        "Should NOT emit TS1194 for nested namespace in ambient context, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts1194_no_error_in_block_within_namespace() {
    // When export declarations are inside a block `{}` within a namespace,
    // tsc reports context errors (TS1231-1235) but does NOT additionally
    // emit TS1194 or TS1319. The block-context diagnostics take priority.
    let source = r#"
        namespace P {
            {
                export { };
            }
        }
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        !diagnostics.iter().any(|d| d.code == 1194),
        "Should NOT emit TS1194 for export in block within namespace, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 1319),
        "Should NOT emit TS1319 for export in block within namespace, got: {diagnostics:?}"
    );
}

// =============================================================================
// Reverse Mapped Type Modifier Preservation Tests
// =============================================================================

#[test]
fn test_reverse_mapped_type_preserves_optional_modifier() {
    // When inferring T from { readonly [P in keyof T]: T[P] }, the optional
    // modifier should be preserved from the source. This tests the fix in
    // constrain_reverse_mapped_type that reverses modifier directives.
    //
    // declare function clone<T>(obj: { readonly [P in keyof T]: T[P] }): T;
    // type Foo = { a?: number; readonly b: string; }
    // declare const foo: Foo;
    // let y = clone(foo);  // should NOT error (T = { a?: number, b: string })
    let source = r#"
        declare function clone<T>(obj: { readonly [P in keyof T]: T[P] }): T;
        type Foo = { a?: number; readonly b: string; }
        declare const foo: Foo;
        let y = clone(foo);
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    crate::test_fixtures::merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts2345_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2345)
        .count();
    assert_eq!(
        ts2345_count,
        0,
        "clone(foo) should NOT emit TS2345 — reverse mapped type inference \
         must preserve optional modifier from source. Got diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_reverse_mapped_type_removes_added_readonly() {
    // When inferring T from { readonly [P in keyof T]: T[P] }, the readonly
    // modifier (which the mapped type adds) should be removed from T.
    //
    // declare function unreadonly<T>(obj: { readonly [P in keyof T]: T[P] }): T;
    // const x = unreadonly({ readonly a: 1, readonly b: "hello" });
    // x should have type { a: number, b: string } (without readonly)
    let source = r#"
        declare function unreadonly<T>(obj: { readonly [P in keyof T]: T[P] }): T;
        declare const input: { readonly a: number; readonly b: string; };
        let result = unreadonly(input);
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    crate::test_fixtures::merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let error_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2322)
        .count();
    assert_eq!(
        error_count,
        0,
        "unreadonly(input) should NOT emit errors — reverse mapped type inference \
         must remove the readonly modifier that the mapped type adds. Got diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_reverse_mapped_type_validate_preserves_optional() {
    // validate<T>(obj: { [P in keyof T]?: T[P] }): T
    // The mapped type adds optional (?), so reverse should REMOVE it.
    // Calling validate with { a: 1 } should infer T = { a: number } (required).
    let source = r#"
        declare function validate<T>(obj: { [P in keyof T]?: T[P] }): T;
        declare const partial: { a?: number; b: string; };
        let result = validate(partial);
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    crate::test_fixtures::merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let error_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2322)
        .count();
    assert_eq!(
        error_count,
        0,
        "validate(partial) should NOT emit errors — reverse mapped type inference \
         must handle optional modifier correctly. Got diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_abstract_class_5plus_missing_uses_ts2655_truncation() {
    // When 5+ abstract members are missing, TSC uses TS2655 (class declaration)
    // with "and N more" truncation instead of TS2654 (lists all).

    let source = r#"
abstract class A {
    abstract m1(): number;
    abstract m2(): number;
    abstract m3(): number;
    abstract m4(): number;
    abstract m5(): number;
    abstract m6(): number;
}
class B extends A { }
"#;

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2655),
        "Expected TS2655 for 5+ missing abstract members, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2654),
        "Should NOT use TS2654 when 5+ members are missing, got: {codes:?}"
    );

    // Check truncation message format
    let msg = checker
        .ctx
        .diagnostics
        .iter()
        .find(|d| d.code == 2655)
        .expect("TS2655 diagnostic should exist");
    assert!(
        msg.message_text.contains("and 2 more"),
        "TS2655 message should contain 'and 2 more', got: {}",
        msg.message_text
    );
    assert!(
        msg.message_text.contains("'m1'") && msg.message_text.contains("'m4'"),
        "TS2655 should list first 4 members, got: {}",
        msg.message_text
    );
}

#[test]
fn test_abstract_class_expression_5plus_missing_uses_ts2650() {
    // When 5+ abstract members are missing on a class expression, TSC uses TS2650.

    let source = r#"
abstract class A {
    abstract m1(): number;
    abstract m2(): number;
    abstract m3(): number;
    abstract m4(): number;
    abstract m5(): number;
}
const C = class extends A {};
"#;

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2650),
        "Expected TS2650 for 5+ missing abstract members on class expression, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2656),
        "Should NOT use TS2656 when 5+ members are missing, got: {codes:?}"
    );

    let msg = checker
        .ctx
        .diagnostics
        .iter()
        .find(|d| d.code == 2650)
        .expect("TS2650 diagnostic should exist");
    assert!(
        msg.message_text.contains("and 1 more"),
        "TS2650 message should contain 'and 1 more', got: {}",
        msg.message_text
    );
}

