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
fn test_void_return_exception_assignability() {
    use crate::parser::ParserState;

    let source = r#"
type VoidFn = () => void;
const ok: VoidFn = () => "value";
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
fn test_literal_widening_for_mutable_bindings() {
    use crate::parser::ParserState;

    let source = r#"
let x = true;
const y = true;
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

    let x_sym = binder.file_locals.get("x").expect("x should exist");
    let y_sym = binder.file_locals.get("y").expect("y should exist");
    let x_type = checker.get_type_of_symbol(x_sym);
    let y_type = checker.get_type_of_symbol(y_sym);

    assert_eq!(x_type, TypeId::BOOLEAN);
    assert_eq!(y_type, types.literal_boolean(true));
}

#[test]
fn test_excess_property_in_call_argument() {
    use crate::parser::ParserState;

    let source = r#"
type Foo = { x: number };
function takesFoo(arg: Foo) {}
takesFoo({ x: 1, y: 2 });
const obj = { x: 1, y: 2 };
takesFoo(obj);
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
    let excess_count = codes.iter().filter(|&&code| code == 2353).count();
    assert_eq!(
        excess_count, 1,
        "Expected exactly one error 2353 (Excess property), got codes: {codes:?}"
    );
}

#[test]
fn test_array_literal_best_common_type() {
    use crate::parser::ParserState;
    use crate::parser::syntax_kind_ext;

    let source = r#"
const numbers = [1, 2];
const mixed = [1, "a"];
numbers;
mixed;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    let expr_stmts: Vec<_> = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .filter(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::EXPRESSION_STATEMENT)
        })
        .collect();
    assert_eq!(expr_stmts.len(), 2, "Expected two expression statements");

    let numbers_expr = arena
        .get_expression_statement(arena.get(expr_stmts[0]).expect("numbers expr node"))
        .expect("numbers expr");
    let mixed_expr = arena
        .get_expression_statement(arena.get(expr_stmts[1]).expect("mixed expr node"))
        .expect("mixed expr");

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
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let numbers_type = checker.get_type_of_node(numbers_expr.expression);
    let mixed_type = checker.get_type_of_node(mixed_expr.expression);

    let number_array = checker.ctx.types.array(TypeId::NUMBER);
    let number_or_string = checker
        .ctx
        .types
        .union(vec![TypeId::NUMBER, TypeId::STRING]);
    let mixed_array = checker.ctx.types.array(number_or_string);

    assert_eq!(numbers_type, number_array);
    assert_eq!(mixed_type, mixed_array);
}

#[test]
fn test_index_access_union_key_cross_product() {
    use crate::parser::ParserState;
    use crate::parser::syntax_kind_ext;

    let source = r#"
type A = { kind: "a"; val: 1 } | { kind: "b"; val: 2 };
declare const obj: A;
declare const key: "kind" | "val";
obj[key];
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    let expr_stmt_idx = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::EXPRESSION_STATEMENT)
        })
        .expect("expression statement");
    let expr_stmt = arena
        .get_expression_statement(arena.get(expr_stmt_idx).expect("expr node"))
        .expect("expression data");

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
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let access_type = checker.get_type_of_node(expr_stmt.expression);

    let lit_a = checker.ctx.types.literal_string("a");
    let lit_b = checker.ctx.types.literal_string("b");
    let lit_one = checker.ctx.types.literal_number(1.0);
    let lit_two = checker.ctx.types.literal_number(2.0);
    let expected = checker
        .ctx
        .types
        .union(vec![lit_a, lit_b, lit_one, lit_two]);

    assert_eq!(access_type, expected);
}

#[test]
fn test_checker_resolves_function_parameter_from_bound_state() {
    use crate::binder::SymbolTable;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parallel;

    let source = r#"
export function f(node: { body: number }) {
    if (node.body) {
        return node.body;
    }
    return node.body;
}
"#;

    let program = parallel::compile_files(vec![("test.ts".to_string(), source.to_string())]);
    let file = &program.files[0];

    let mut file_locals = SymbolTable::new();
    for (name, &sym_id) in program.file_locals[0].iter() {
        file_locals.set(name.clone(), sym_id);
    }
    for (name, &sym_id) in program.globals.iter() {
        if !file_locals.has(name) {
            file_locals.set(name.clone(), sym_id);
        }
    }

    let binder = BinderState::from_bound_state_with_scopes(
        program.symbols.clone(),
        file_locals,
        file.node_symbols.clone(),
        file.scopes.clone(),
        file.node_scope_ids.clone(),
    );

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &file.arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    checker.check_source_file(file.source_file);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "Unexpected 'Cannot find name' diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_excess_property_in_return_statement() {
    use crate::parser::ParserState;

    let source = r#"
type Foo = { x: number };
function makeFoo(): Foo {
    return { x: 1, y: 2 };
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
    let excess_count = codes.iter().filter(|&&code| code == 2353).count();
    assert_eq!(
        excess_count, 1,
        "Expected exactly one error 2353 (Excess property), got codes: {codes:?}"
    );
}

#[test]
fn test_checker_subtype_intrinsics() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict_function_types: true,
            ..Default::default()
        },
    );

    // Test intrinsic subtype relations
    // Any is assignable to everything
    assert!(checker.is_assignable_to(TypeId::ANY, TypeId::STRING));
    assert!(checker.is_assignable_to(TypeId::ANY, TypeId::NUMBER));

    // Everything is assignable to any
    assert!(checker.is_assignable_to(TypeId::STRING, TypeId::ANY));
    assert!(checker.is_assignable_to(TypeId::NUMBER, TypeId::ANY));

    // Everything is assignable to unknown
    assert!(checker.is_assignable_to(TypeId::STRING, TypeId::UNKNOWN));
    assert!(checker.is_assignable_to(TypeId::NUMBER, TypeId::UNKNOWN));

    // Never is assignable to everything
    assert!(checker.is_assignable_to(TypeId::NEVER, TypeId::STRING));
    assert!(checker.is_assignable_to(TypeId::NEVER, TypeId::NUMBER));

    // Nothing is assignable to never (except never)
    assert!(!checker.is_assignable_to(TypeId::STRING, TypeId::NEVER));
    assert!(!checker.is_assignable_to(TypeId::NUMBER, TypeId::NEVER));
    assert!(checker.is_assignable_to(TypeId::NEVER, TypeId::NEVER));
}

#[test]
fn test_checker_assignability_relation_cache_hit() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict_function_types: true,
            ..Default::default()
        },
    );

    assert!(!checker.is_assignable_to(TypeId::STRING, TypeId::NUMBER));
    // Re-running should stay stable and reuse solver-side relation machinery.
    assert!(!checker.is_assignable_to(TypeId::STRING, TypeId::NUMBER));
}

#[test]
fn test_checker_assignability_bivariant_cache_key_is_distinct() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict_function_types: true,
            ..Default::default()
        },
    );

    assert!(!checker.is_assignable_to(TypeId::STRING, TypeId::NUMBER));
    assert!(!checker.is_assignable_to_bivariant(TypeId::STRING, TypeId::NUMBER));

    let regular_flags = checker.ctx.pack_relation_flags();
    let bivariant_flags = regular_flags & !RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES;
    let regular_key =
        RelationCacheKey::assignability(TypeId::STRING, TypeId::NUMBER, regular_flags, 0);
    let bivariant_key =
        RelationCacheKey::assignability(TypeId::STRING, TypeId::NUMBER, bivariant_flags, 0);
    assert_ne!(
        regular_key, bivariant_key,
        "regular and bivariant assignability must use distinct relation cache keys"
    );
}

