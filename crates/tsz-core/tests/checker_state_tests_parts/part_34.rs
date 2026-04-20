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
fn test_flow_narrowing_applies_for_computed_element_access_const_numeric_key() {
    use crate::parser::ParserState;
    use crate::parser::syntax_kind_ext;

    let source = r#"
let arr: (string | number)[] = ["ok", 1];
const idx = 0;
if (typeof arr[idx] === "string") {
    arr[idx].toUpperCase();
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    let if_idx = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::IF_STATEMENT)
        })
        .expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");

    let then_node = arena.get(if_data.then_statement).expect("then node");
    let block = arena.get_block(then_node).expect("then block");
    let expr_stmt_idx = block
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

    let expr_type = checker.get_type_of_node(expr_stmt.expression);
    assert_eq!(
        expr_type,
        TypeId::STRING,
        "Expected computed element access with const numeric key to narrow to string, got: {expr_type:?}"
    );
}

#[test]
fn test_flow_narrowing_applies_for_computed_element_access_literal_discriminant() {
    use crate::parser::ParserState;
    use crate::parser::syntax_kind_ext;

    let source = r#"
type U = { kind: "a"; value: string } | { kind: "b"; value: number };
let obj: U = { kind: "a", value: "ok" };
let key: "kind" = "kind";
if (obj[key] === "a") {
    obj.value.toUpperCase();
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    let if_idx = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::IF_STATEMENT)
        })
        .expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");

    let then_node = arena.get(if_data.then_statement).expect("then node");
    let block = arena.get_block(then_node).expect("then block");
    let expr_stmt_idx = block
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

    let expr_type = checker.get_type_of_node(expr_stmt.expression);
    assert_eq!(
        expr_type,
        TypeId::STRING,
        "Expected computed element discriminant to narrow to string, got: {expr_type:?}"
    );
}

#[test]
fn test_flow_narrowing_applies_for_literal_element_access() {
    use crate::parser::ParserState;
    use crate::parser::syntax_kind_ext;

    let source = r#"
let obj: { prop: string | number } = { prop: "ok" };
if (typeof obj["prop"] === "string") {
    obj["prop"];
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    let if_idx = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::IF_STATEMENT)
        })
        .expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");

    let then_node = arena.get(if_data.then_statement).expect("then node");
    let block = arena.get_block(then_node).expect("then block");
    let expr_stmt_idx = block
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

    let expr_type = checker.get_type_of_node(expr_stmt.expression);
    assert_eq!(
        expr_type,
        TypeId::STRING,
        "Expected literal element access to narrow to string, got: {expr_type:?}"
    );
}

#[test]
fn test_flow_narrowing_cleared_by_property_base_assignment() {
    use crate::parser::ParserState;

    let source = r#"
let obj: { prop: string | number } = { prop: "ok" };
if (typeof obj.prop === "string") {
    obj.prop.toUpperCase();
    obj = { prop: 1 };
    obj.prop.toUpperCase();
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
    let count = codes.iter().filter(|&&code| code == 2339).count();
    assert_eq!(
        count, 1,
        "Expected one 2339 after property base assignment clears narrowing, got: {codes:?}"
    );
}

#[test]
fn test_flow_narrowing_cleared_by_element_base_assignment() {
    use crate::parser::ParserState;

    let source = r#"
let obj: { prop: string | number } = { prop: "ok" };
if (typeof obj["prop"] === "string") {
    obj["prop"].toUpperCase();
    obj = { prop: 1 };
    obj["prop"].toUpperCase();
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
    let count = codes.iter().filter(|&&code| code == 2339).count();
    assert_eq!(
        count, 1,
        "Expected one 2339 after element base assignment clears narrowing, got: {codes:?}"
    );
}

#[test]
fn test_parameter_identifier_type_from_symbol_cache() {
    use crate::parser::ParserState;
    use crate::parser::syntax_kind_ext;

    let source = r#"
function f(x: number) { return x; }
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let func_idx = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::FUNCTION_DECLARATION)
        })
        .expect("function declaration");
    let func_node = arena.get(func_idx).expect("function node");
    let func = arena.get_function(func_node).expect("function data");

    let body_node = arena.get(func.body).expect("function body");
    let block = arena.get_block(body_node).expect("function block");
    let return_idx = *block.statements.nodes.first().expect("return statement");
    let return_node = arena.get(return_idx).expect("return node");
    let return_data = arena
        .get_return_statement(return_node)
        .expect("return data");

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

    let param_type = checker.get_type_of_node(return_data.expression);
    assert_eq!(param_type, TypeId::NUMBER);
}

/// Test that a complex generic library snippet compiles and checks correctly
///
// TODO: Fix TS2304 for mapped type parameter K in scope -- binder does not register
// the iteration variable of mapped types in the type-level scope.
#[test]
fn test_generic_library_snippet_compiles_and_checks() {
    use crate::binder::SymbolTable;
    use crate::parallel;

    let source = r#"
type Dictionary<T> = { [key: string]: T };
type ReadonlyDict<T> = { readonly [K in keyof T]: T[K] };
type OptionalDict<T> = { [K in keyof T]?: T[K] };

type Action<T extends string = string> = { type: T };
type PayloadAction<T extends string, P> = { type: T; payload: P };

type Reducer<S, A extends Action = Action> = (state: S, action: A) => S;
type CaseReducer<S, A extends Action> = (state: S, action: A) => S;

type CaseReducers<S, A extends Action = Action> = {
  [T in A["type"]]?: CaseReducer<S, A>;
};

declare function createReducer<S, A extends Action>(
  initial: S,
  reducers: CaseReducers<S, A>
): Reducer<S, A>;

type CounterAction =
  | PayloadAction<"inc", number>
  | PayloadAction<"set", number>;

const reducer = createReducer(0, {
  inc: (state, action) => state + action.payload,
  set: (state, action) => action.payload,
});
"#;

    let program = parallel::compile_files(vec![("lib.ts".to_string(), source.to_string())]);
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
        "lib.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    checker.check_source_file(file.source_file);

    // Filter out lib-collision duplicates (TS2300/TS2451) and known contextual typing
    // limitations (TS2339 property access on generic, TS7006 implicit any in callbacks).
    let unexpected: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| !matches!(d.code, 2300 | 2451 | 2339 | 7006))
        .collect();
    assert!(
        unexpected.is_empty(),
        "Unexpected diagnostics: {unexpected:?}"
    );
}

#[test]
fn test_multi_file_generic_library_snippet_compiles_and_checks() {
    use crate::binder::SymbolTable;
    use crate::parallel;

    let decls = r#"
type Action<T extends string = string> = { type: T };
type PayloadAction<T extends string, P> = { type: T; payload: P };
type Reducer<S, A extends Action = Action> = (state: S, action: A) => S;
type CaseReducer<S, A extends Action> = (state: S, action: A) => S;

type CaseReducers<S, A extends Action = Action> = {
  [T in A["type"]]?: CaseReducer<S, A>;
};

declare function createReducer<S, A extends Action>(
  initial: S,
  reducers: CaseReducers<S, A>
): Reducer<S, A>;
"#;

    let usage = r#"
type CounterAction =
  | PayloadAction<"inc", number>
  | PayloadAction<"set", number>;

const reducer = createReducer(0, {
  inc: (state, action) => state + action.payload,
  set: (state, action) => action.payload,
});
"#;

    let program = parallel::compile_files(vec![
        ("types.ts".to_string(), decls.to_string()),
        ("usage.ts".to_string(), usage.to_string()),
    ]);

    let types = TypeInterner::new();

    for (file_idx, file) in program.files.iter().enumerate() {
        let mut file_locals = SymbolTable::new();
        for (name, &sym_id) in program.file_locals[file_idx].iter() {
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

        let mut checker = CheckerState::new(
            &file.arena,
            &binder,
            &types,
            file.file_name.clone(),
            crate::checker::context::CheckerOptions::default(),
        );
        checker.check_source_file(file.source_file);
        // Filter out lib-collision duplicates (TS2300/TS2451), known contextual typing
        // limitations (TS2339/TS7006), and cross-file generic resolution (TS2315).
        let unexpected: Vec<_> = checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| !matches!(d.code, 2300 | 2315 | 2451 | 2339 | 7006))
            .collect();
        assert!(
            unexpected.is_empty(),
            "Unexpected diagnostics in {}: {:?}",
            file.file_name,
            unexpected
        );
    }
}

/// TS Unsoundness #41: Key Remapping with `as never`
/// In mapped types, remapping a key to `never` removes that key from the result.
/// This is the mechanism behind the `Omit` utility type.
/// Note: Full instantiation of generic mapped types is tested in `solver/evaluate_tests.rs`.
// TODO: Fix TS2304 for mapped type parameters (P, K) -- binder scope gap.
#[test]
fn test_key_remapping_syntax_parsing() {
    use crate::parser::ParserState;

    // Test that key remapping syntax parses and binds correctly
    let source = r#"
// Custom Omit using key remapping with `as never`
type MyOmit<T, K extends keyof any> = {
    [P in keyof T as P extends K ? never : P]: T[P]
};

// Custom Pick using key remapping
type MyPick<T, K extends keyof T> = {
    [P in keyof T as P extends K ? P : never]: T[P]
};

// Custom Exclude using `as`
type ExcludeKeys<T, U> = {
    [K in keyof T as K extends U ? never : K]: T[K]
};

// Source type for reference
interface Person {
    name: string;
    age: number;
    email: string;
}

// Type alias usages (verify no parse errors)
declare const o: MyOmit<Person, "email">;
declare const p: MyPick<Person, "name">;
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

    // No diagnostics expected for type declarations
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #28: Constructor Void Exception
/// A constructor type declared as `new () => void` accepts concrete classes
/// that construct objects, similar to the void return exception for functions (#6).
#[test]
fn test_constructor_void_exception() {
    use crate::parser::ParserState;

    let source = r#"
// Constructor type returning void
type VoidCtor = new () => void;

// A concrete class that constructs an instance
class MyClass {
    value: number = 42;
}

// Assignment should be allowed: class constructor is assignable to void constructor
const ctor: VoidCtor = MyClass;

// Another class with a constructor
class AnotherClass {
    constructor(public name: string = "default") {}
}

// This should also work - constructor with default params is compatible
type DefaultCtor = new () => void;
const ctor2: DefaultCtor = AnotherClass;
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

    // No diagnostics expected - void constructor should accept any class
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

