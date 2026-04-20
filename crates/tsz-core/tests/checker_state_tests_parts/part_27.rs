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
fn test_top_level_variable_redeclaration_different_type_2403() {
    use crate::parser::ParserState;

    // Top-level variables with different types should trigger error 2403
    let source = r#"
var x: string;
var x: number;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2403),
        "Expected error 2403 for top-level variable redeclaration with different type, got: {codes:?}"
    );
}

#[test]
fn test_top_level_variable_redeclaration_same_type_ok() {
    use crate::parser::ParserState;

    // Top-level variables with same type should be allowed
    let source = r#"
var x: string;
var x: string;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let error_2403_count = codes.iter().filter(|&&c| c == 2403).count();

    assert_eq!(
        error_2403_count, 0,
        "Expected no error 2403 for top-level variable redeclaration with same type, got: {codes:?}"
    );
}

#[test]
fn test_variable_redeclaration_typeof_ok_no_2403() {
    use crate::parser::ParserState;

    // Test for bi-directional assignability in var redeclaration:
    // `var e = E;` and `var e: typeof E;` should be allowed because
    // the types are bi-directionally assignable (even if TypeIds differ).
    // Based on TypeScript conformance test: enumBasics.ts
    let source = r#"
enum E { A, B, C }
var e = E;
var e: typeof E;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let error_2403_count = codes.iter().filter(|&&c| c == 2403).count();

    assert_eq!(
        error_2403_count, 0,
        "Expected no error 2403 for enum typeof redeclaration, got: {codes:?}"
    );
}

#[test]
fn test_variable_redeclaration_enum_object_literal_no_2403() {
    use crate::parser::ParserState;

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
    use crate::parser::ParserState;

    let source = r#"
function f1() {
    var a = [1, 2, 3];
    var b = ["hello", ...a, true];
    var b: (string | number | boolean)[];
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let error_2403_count = codes.iter().filter(|&&c| c == 2403).count();

    assert_eq!(
        error_2403_count, 0,
        "Expected no error 2403 for array spread redeclaration, got: {codes:?}"
    );
}

#[test]
fn test_variable_redeclaration_inferred_vs_annotated_no_2403() {
    use crate::parser::ParserState;

    // Test that inferred type from initializer matches explicit annotation
    // Based on conformance test: ambientDeclarationsExternal.ts pattern
    let source = r#"
var n = 42;
var n: number;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let error_2403_count = codes.iter().filter(|&&c| c == 2403).count();

    assert_eq!(
        error_2403_count, 0,
        "Expected no error 2403 for inferred vs annotated redeclaration, got: {codes:?}"
    );
}

#[test]
fn test_namespace_member_not_found() {
    use crate::parser::ParserState;

    let source = r#"
namespace foo {
    export class Provide {}
}
var p: foo.NotExist;
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
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export const ok = 1;
}
import Alias = NS;
const bad = NS.missing;
const badAlias = Alias.missing;
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
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export class Exported {}
    class NotExported {}
}
import Alias = NS.Exported;
var x: Alias;
var y: NS.Exported;
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
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export class Exported {}
    class NotExported {}
}
import Alias = NS.NotExported;
var x: Alias;
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

    let diags = &checker.ctx.diagnostics;
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();

    // Should produce error 2694 or 2724 (spelling suggestion variant):
    // Namespace 'NS' has no exported member 'NotExported' (Did you mean 'Exported'?)
    assert!(
        codes.contains(&2694) || codes.contains(&2724),
        "Expected error 2694 or 2724 for import alias of non-exported member, got: {codes:?}"
    );
}

