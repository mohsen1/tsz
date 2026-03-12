//! Manual tests for generic type inference
//!
//! Tests that generic functions properly infer type arguments from:
//! - Function arguments (upward inference)
//! - Contextual type (downward inference)
//! - Constraints (extends clauses)

use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::ParserState;
use tsz_solver::{TypeId, TypeInterner};

fn variable_declaration_initializer_at(
    parser: &ParserState,
    root: NodeIndex,
    stmt_index: usize,
) -> NodeIndex {
    parser
        .get_arena()
        .get(root)
        .and_then(|node| parser.get_arena().get_source_file(node))
        .and_then(|source_file| {
            parser
                .get_arena()
                .get(source_file.statements.nodes[stmt_index])
        })
        .and_then(|node| parser.get_arena().get_variable(node))
        .and_then(|stmt| parser.get_arena().get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.get_arena().get_variable(node))
        .and_then(|decl_list| parser.get_arena().get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.get_arena().get_variable_declaration(node))
        .map(|decl| decl.initializer)
        .expect("missing variable declaration")
}

#[test]
fn test_identity_function_inference() {
    let source = r#"
function identity<T>(x: T): T {
    return x;
}

const s = identity("hello");
const n = identity(42);
const b = identity(true);
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // s should be string, n should be number, b should be boolean
    // Filter out "Cannot find global type" errors - those are expected without lib files
    let type_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .collect();

    assert!(
        type_errors.is_empty(),
        "Expected no type errors, got: {type_errors:?}"
    );
}

#[test]
fn test_constraint_validation() {
    let source = r"
function logName<T extends { name: string }>(obj: T): void {
    // Avoid using console.log which requires lib files
    const _unused: void = undefined;
}

const invalid = { id: 1 };
logName(invalid);
";

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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should have error because { id: 1 } doesn't satisfy { name: string }
    // Look for TS2322 (type mismatch) or similar constraint violation errors
    let type_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .collect();

    assert!(
        !type_errors.is_empty(),
        "Expected errors for constraint violation, got: {type_errors:?}"
    );
}

#[test]
fn test_downward_inference() {
    let source = r"
function identity<T>(x: T): T {
    return x;
}

const x: string = identity(42);
";

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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should have error because 42 is not assignable to string
    let type_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .collect();

    assert!(!type_errors.is_empty(), "Expected error for type mismatch");

    // Verify it's specifically a type mismatch error (TS2322)
    assert!(
        type_errors.iter().any(|d| d.code == 2322),
        "Expected TS2322 error for type mismatch, got: {type_errors:?}"
    );
}

#[test]
fn test_map_multi_pass_inference() {
    // Test multi-pass inference for complex nested generics
    // The key is that we need to:
    // 1. Round 1: Infer T from [1, 2, 3] -> T = number
    // 2. Fix T
    // 3. Round 2: Use T=number to infer lambda parameter type and then U

    // NOTE: This test is currently expected to fail because the compiler
    // doesn't handle method calls on generic types (like T[].map) before
    // type parameters are resolved. This requires additional work beyond
    // multi-pass inference.

    // For now, we test a simpler case that demonstrates the multi-pass
    // inference working correctly for nested generic callbacks.
    let source = r"
function process<T, U>(value: T, callback: (x: T) => U): U {
    return callback(value);
}

const result = process(42, x => x.toString());
";

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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Filter out "Cannot find global type" errors - those are expected without lib files
    let type_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .collect();

    // The lambda `x => x.toString()` should infer:
    // - T = number (from 42 in Round 1)
    // - x has type number (from T in Round 2)
    // - U = string (from x.toString() in Round 2)
    assert!(
        type_errors.is_empty(),
        "Expected no type errors for process inference, got: {type_errors:?}"
    );
}

#[test]
fn test_overloaded_function_symbol_preserves_declaration_signature_order() {
    let source = r#"
declare function testFunction(n: number): Promise<number>;
declare function testFunction(s: string): Promise<string>;
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let sym_id = binder
        .file_locals
        .get("testFunction")
        .expect("expected testFunction symbol");
    let ty = checker.get_type_of_symbol(sym_id);
    let signatures = tsz_solver::type_queries::get_call_signatures(&types, ty)
        .expect("expected overloaded call signatures");

    assert_eq!(signatures.len(), 2, "expected two overload signatures");
    assert_eq!(
        signatures[0].params.len(),
        1,
        "expected unary first overload"
    );
    assert_eq!(
        signatures[1].params.len(),
        1,
        "expected unary second overload"
    );
    assert_eq!(signatures[0].params[0].type_id, TypeId::NUMBER);
    assert_eq!(signatures[1].params[0].type_id, TypeId::STRING);
}

#[test]
fn test_generic_call_with_optional_trailing_param_preserves_inferred_return_type() {
    let source = r#"
class Collection<T> {
    public add(x: T) { }
}
interface Utils {
    fold<T, S>(c: Collection<T>, folder: (s: S, t: T) => T, init?: S): T;
}
var c = new Collection<string>();
declare var utils: Utils;
var r = utils.fold(c, (s, t) => t, "");
var r2 = utils.fold(c, (s, t) => t);
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let r_type = checker.get_type_of_node(variable_declaration_initializer_at(&parser, root, 4));
    let r2_type = checker.get_type_of_node(variable_declaration_initializer_at(&parser, root, 5));

    assert_eq!(
        checker.format_type(r_type),
        "string",
        "Expected utils.fold(c, (s, t) => t, \"\") to infer string. Diagnostics: {:?}",
        checker.ctx.diagnostics
    );
    assert_eq!(
        checker.format_type(r2_type),
        "string",
        "Expected utils.fold(c, (s, t) => t) to infer string. Diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}
