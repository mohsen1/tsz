// Tests for Checker - Type checker using `NodeArena` and Solver
//
// This module contains comprehensive type checking tests organized into categories:
// - Basic type checking (creation, intrinsic types, type interning)
// - Type compatibility and assignability
// - Excess property checking
// - Function overloads and call resolution
// - Generic types and type inference
// - Control flow analysis
// - Error diagnostics
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
fn test_parameter_property_in_constructor_overload_2369() {
    use crate::parser::ParserState;
    // Constructor overload signatures should error on parameter properties
    let source = r#"
class C {
    constructor(public p1: string);
    constructor(public p2: number) {}
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
    // Should have exactly one 2369 error for the overload, not for the implementation
    let count_2369 = codes.iter().filter(|&&c| c == 2369).count();
    assert_eq!(
        count_2369, 1,
        "Expected exactly 1 error 2369 for constructor overload, got {count_2369} from: {codes:?}"
    );
}

#[test]
fn test_parameter_property_in_constructor_implementation_ok() {
    use crate::parser::ParserState;
    // Constructor implementations are allowed to have parameter properties
    let source = r#"
class C {
    constructor(public x: string) {}
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
    assert!(
        !codes.contains(&2369),
        "Should not have error 2369 in constructor implementation, got: {codes:?}"
    );
}

#[test]
fn test_class_name_any_error_2414() {
    use crate::parser::ParserState;

    // Test that class name 'any' produces error 2414
    let code = "class any {}";
    let mut parser = ParserState::new("test.ts".to_string(), code.to_string());
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
        codes.contains(&2414),
        "Expected error 2414 (Class name cannot be 'any'), got: {codes:?}"
    );
}

#[test]
fn test_local_variable_scope_resolution() {
    use crate::parser::ParserState;

    // Test that local variables inside functions are properly resolved
    // This should NOT produce "Cannot find name 'x'" error
    let code = r#"
        function test() {
            let x: number = 1;
            let y = x + 1;
        }
    "#;
    let mut parser = ParserState::new("test.ts".to_string(), code.to_string());
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

    // Should have no "Cannot find name" errors (2304)
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Should not have 'Cannot find name' error for local variable, got: {codes:?}"
    );
}

#[test]
fn test_for_loop_variable_scope() {
    use crate::parser::ParserState;

    // Test that for loop variables are properly scoped
    let code = r#"
        function test() {
            for (let i = 0; i < 10; i++) {
                let x = i * 2;
            }
        }
    "#;
    let mut parser = ParserState::new("test.ts".to_string(), code.to_string());
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

    // Should have no "Cannot find name" errors (2304) for loop variable 'i'
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Should not have 'Cannot find name' error for loop variable, got: {codes:?}"
    );
}

#[test]
fn test_object_literal_properties_resolve_locals() {
    use crate::parser::ParserState;

    let source = r#"
function test() {
    const foo = 1;
    const bar = 2;
    const obj = { foo, baz: bar };
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
    assert!(
        !codes.contains(&2304),
        "Should not have 'Cannot find name' error for object literal locals, got: {codes:?}"
    );
}

#[test]
fn test_export_default_in_ambient_module_resolves_local() {
    use crate::parser::ParserState;

    let source = r#"
declare module "foo" {
    const x: string;
    export default x;
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
    assert!(
        !codes.contains(&2304),
        "Should not have 'Cannot find name' error in ambient export default, got: {codes:?}"
    );
}

#[test]
fn test_missing_identifier_emits_2304() {
    use crate::parser::ParserState;

    let source = r#"
let x = MissingName;
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for unresolved identifier, got: {codes:?}"
    );
}

#[test]
fn test_missing_type_reference_emits_2304() {
    use crate::parser::ParserState;

    let source = r#"
let x: MissingType;
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
        codes.contains(&2304),
        "Expected TS2304 for unresolved type reference, got: {codes:?}"
    );
}

/// Test that in a module file (has import), `declare module "x"` with body is
/// treated as a module augmentation, which emits TS2664 when the target module
/// doesn't exist. The import statement itself also emits TS2307.
#[test]
fn test_ts2307_import_with_module_augmentation() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
import { value } from "dep";

declare module "dep" {
    export const value: number;
}

value;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // In an external module (file with import), `declare module "dep" { ... }` is a module
    // augmentation. Since "dep" doesn't exist, this emits TS2664 (Invalid module name in
    // augmentation). The import also emits TS2307 for the unresolved module.
    // Note: The declared_modules check in check_import_declaration prevents TS2307 because
    // the binder registers "dep" in declared_modules when it sees `declare module "dep"`.
    // So we only get TS2664 for the invalid augmentation.
    assert!(
        codes.contains(
            &diagnostic_codes::INVALID_MODULE_NAME_IN_AUGMENTATION_MODULE_CANNOT_BE_FOUND
        ),
        "Expected TS2664 for invalid module augmentation, got: {codes:?}"
    );
}
