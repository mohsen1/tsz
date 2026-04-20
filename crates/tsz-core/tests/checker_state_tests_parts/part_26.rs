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
    use crate::parser::ParserState;

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
    use crate::parser::ParserState;

    // Absolute module names should be allowed in ambient declarations
    let source = r#"
declare module "absolute-module" {
    export function foo(): void;
}

declare module "@scoped/package" {
    export const bar: number;
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
    let error_5061_count = codes.iter().filter(|&&c| c == 5061).count();

    assert_eq!(
        error_5061_count, 0,
        "Expected no error 5061 for absolute module names, got: {codes:?}"
    );
}

#[test]
fn test_private_identifier_in_ambient_class_allowed() {
    use crate::parser::ParserState;

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
    let error_count = codes.iter().filter(|&&c| c == 18019).count();

    // Should NOT report TS18019 for private identifiers in ambient classes
    assert!(
        error_count == 0,
        "Expected 0 errors with code 18019 for private identifiers in ambient class, got {error_count} errors: {codes:?}"
    );
}

#[test]
fn test_private_identifier_in_non_ambient_class_ok() {
    use crate::parser::ParserState;

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
    let error_2819_count = codes.iter().filter(|&&c| c == 2819).count();

    assert_eq!(
        error_2819_count, 0,
        "Expected no error 2819 for private identifiers in non-ambient class, got: {codes:?}"
    );
}

#[test]
fn test_private_static_method_access_no_error() {
    use crate::parser::ParserState;

    // Private static methods should be accessible within the class
    let source = r#"
class A {
    static #foo(a: number) {}
    constructor() {
        A.#foo(30);
    }
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
    // TS2339 = "Property 'X' does not exist on type 'Y'"
    let error_2339_count = codes.iter().filter(|&&c| c == 2339).count();

    assert_eq!(
        error_2339_count, 0,
        "Expected no TS2339 error for private static method access, got errors: {codes:?}"
    );
}

#[test]
fn test_non_private_static_accessor_access_works() {
    use crate::parser::ParserState;

    // Non-private static accessors should be accessible from class reference
    let source = r#"
class A {
    static get quux(): number {
        return 42;
    }
}
let x = A.quux;
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
    // TS2339 = "Property 'X' does not exist on type 'Y'"
    let error_2339_count = codes.iter().filter(|&&c| c == 2339).count();

    assert_eq!(
        error_2339_count, 0,
        "Expected no TS2339 error for non-private static accessor access, got errors: {codes:?}"
    );
}

#[test]
fn test_private_static_accessor_access_no_error() {
    use crate::parser::ParserState;

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
    // TS2339 = "Property 'X' does not exist on type 'Y'"
    let error_2339_count = codes.iter().filter(|&&c| c == 2339).count();

    assert_eq!(
        error_2339_count, 0,
        "Expected no TS2339 error for private static accessor access, got errors: {codes:?}"
    );
}

#[test]
fn test_private_static_generator_method_access_no_error() {
    use crate::parser::ParserState;

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
    use crate::parser::ParserState;

    // Namespace declarations (without declare) can have any name, including relative-like names
    // This test ensures we only check ambient modules (declare module)
    let source = r#"
namespace MyNamespace {
    export function foo(): void {}
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
    let error_5061_count = codes.iter().filter(|&&c| c == 5061).count();

    assert_eq!(
        error_5061_count, 0,
        "Expected no error 5061 for namespace declarations (only ambient modules should error), got: {codes:?}"
    );
}

// ============== Top-level scope tests (fixes critical bug) ==============

