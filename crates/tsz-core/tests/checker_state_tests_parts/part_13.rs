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
fn test_readonly_index_signature_element_access_assignment_2542() {
    // Error 2542: Index signature in type 'MyReadonlyMap' only permits reading.
    use crate::parser::ParserState;

    let source = r#"
interface MyReadonlyMap {
    readonly [key: string]: number;
}
let map: MyReadonlyMap = { a: 1 };
map["a"] = 2;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    let count_2542 = codes.iter().filter(|&&c| c == 2542).count();
    assert!(
        count_2542 >= 1,
        "Expected at least 1 error 2542 for readonly index signature assignment, got {count_2542} in: {codes:?}"
    );
}

#[test]
fn test_readonly_index_signature_variable_access_assignment_2542() {
    // Error 2542: Index signature in type 'ReadonlyMap' only permits reading.
    use crate::parser::ParserState;

    let source = r#"
interface ReadonlyMap {
    readonly [key: string]: number;
}
let map: ReadonlyMap = { a: 1 };
let key: string = "a";
map[key] = 2;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    let count_2542 = codes.iter().filter(|&&c| c == 2542).count();
    assert!(
        count_2542 >= 1,
        "Expected at least 1 error 2542 for readonly index signature assignment, got {count_2542} in: {codes:?}"
    );
}

#[test]
fn test_nonexistent_property_should_not_report_ts2540() {
    // P1 fix: Assigning to a non-existent property should report TS2339 (property doesn't exist)
    // but NOT TS2540 (cannot assign to readonly). This matches tsc behavior which checks
    // property existence before readonly status.
    use crate::parser::ParserState;

    let source = r#"
interface Person {
    readonly name: string;
}
let p: Person = { name: "Alice" };
// This property does not exist on Person - should get TS2339, NOT TS2540
p.nonexistent = "error";
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    println!("Diagnostics:");
    for diag in &checker.ctx.diagnostics {
        println!("  TS{}: {}", diag.code, diag.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should NOT have TS2540 for non-existent property
    let count_2540 = codes.iter().filter(|&&c| c == 2540).count();
    assert_eq!(
        count_2540, 0,
        "Should NOT report TS2540 for non-existent property, got {count_2540} in: {codes:?}"
    );

    // Should have TS2339 for non-existent property
    let count_2339 = codes.iter().filter(|&&c| c == 2339).count();
    assert!(
        count_2339 >= 1,
        "Should report TS2339 for non-existent property, got {count_2339} in: {codes:?}"
    );
}

#[test]
fn test_readonly_tuple_computed_index_assignment_2542() {
    // TS2542 for computed index on readonly tuple: v[0 + 1] = 1
    // The ReadonlyChecker must recognize ReadonlyType(Tuple) has a readonly number index.
    use crate::parser::ParserState;

    let source = r#"
declare var v: readonly [number, number, ...number[]];
v[0 + 1] = 1;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    let count_2542 = codes.iter().filter(|&&c| c == 2542).count();
    assert!(
        count_2542 >= 1,
        "Expected TS2542 for computed index on readonly tuple, got codes: {codes:?}"
    );
}

#[test]
fn test_delete_readonly_element_access_2542() {
    // TS2542 for delete on readonly element access: delete v[2]
    use crate::parser::ParserState;

    let source = r#"
declare var v: readonly [number, number, ...number[]];
delete v[2];
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    let count_2542 = codes.iter().filter(|&&c| c == 2542).count();
    assert!(
        count_2542 >= 1,
        "Expected TS2542 for delete on readonly tuple element, got codes: {codes:?}"
    );
}

#[test]
fn test_abstract_property_negative_errors() {
    // Test the full abstractPropertyNegative test case to verify expected errors
    use crate::parser::ParserState;

    let source = r#"
interface A {
    prop: string;
    m(): string;
}
abstract class B implements A {
    abstract prop: string;
    public abstract readonly ro: string;
    abstract get readonlyProp(): string;
    abstract m(): string;
    abstract get mismatch(): string;
    abstract set mismatch(val: number);
}
class C extends B {
    readonly ro = "readonly please";
    abstract notAllowed: string;
    get concreteWithNoBody(): string;
}
let c = new C();
c.ro = "error: lhs of assignment can't be readonly";
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    println!("Diagnostics:");
    for diag in &checker.ctx.diagnostics {
        println!("  TS{}: {}", diag.code, diag.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Expected errors:
    // - 2654: Non-abstract class 'C' is missing implementations
    // - 1253: Abstract properties can only appear within an abstract class
    // - 2540: Cannot assign to 'ro' because it is a read-only property
    // - 2676: Accessors must both be abstract or non-abstract (on mismatch getter/setter)

    // We should NOT have 2322 (accessor type compatibility) for abstract accessors
    let count_2322 = codes.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        count_2322, 0,
        "Should not produce 2322 errors for abstract accessor pairs"
    );

    // We should have the expected errors
    // 5+ missing abstract members use TS2655 (with "and N more" truncation)
    assert!(
        codes.contains(&2655),
        "Should have error 2655 for 5+ missing abstract implementations"
    );
    assert!(
        codes.contains(&1253),
        "Should have error 1253 for abstract in non-abstract class"
    );
    assert!(
        codes.contains(&2540),
        "Should have error 2540 for readonly assignment"
    );
}

#[test]
fn test_contextual_typing_for_function_parameters() {
    use tsz_solver::ContextualTypeContext;

    // Test that ContextualTypeContext can extract parameter types from function types
    let types = TypeInterner::new();

    // Create a function type: (x: string, y: number) => boolean
    use tsz_solver::{FunctionShape, ParamInfo};

    let func_shape = FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(types.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(types.intern_string("y")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let func_type = types.function(func_shape);

    // Create contextual context
    let ctx = ContextualTypeContext::with_expected(&types, func_type);

    // Test parameter type extraction
    assert_eq!(ctx.get_parameter_type(0), Some(TypeId::STRING));
    assert_eq!(ctx.get_parameter_type(1), Some(TypeId::NUMBER));
    assert_eq!(ctx.get_parameter_type(2), None); // Out of bounds

    // Test return type extraction
    assert_eq!(ctx.get_return_type(), Some(TypeId::BOOLEAN));
}

#[test]
fn test_contextual_typing_skips_this_parameter() {
    use crate::parser::ParserState;
    use crate::parser::syntax_kind_ext;
    use tsz_solver::TypeData;

    let source = r#"
function takesHandler(fn: (this: { value: number }, x: string) => void) {}
takesHandler(function(this: { value: number }, x) {
    let y: number = x;
});
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        .get_expression_statement(arena.get(expr_stmt_idx).expect("expr stmt node"))
        .expect("expr stmt data");
    let call_idx = expr_stmt.expression;
    let call_expr = arena
        .get_call_expr(arena.get(call_idx).expect("call node"))
        .expect("call expr");
    let args = call_expr.arguments.as_ref().expect("call arguments");
    let func_idx = *args.nodes.first().expect("function argument");

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
    checker.get_type_of_node(call_idx);

    let func_type = checker.get_type_of_node(func_idx);
    let Some(TypeData::Function(shape_id)) = checker.ctx.types.lookup(func_type) else {
        panic!("expected function type for argument");
    };
    let shape = checker.ctx.types.function_shape(shape_id);
    assert!(
        shape.this_type.is_some(),
        "expected this type on contextual function"
    );
    assert_eq!(
        shape.params.len(),
        1,
        "expected single parameter besides this"
    );
    assert_eq!(
        shape.params[0].type_id,
        TypeId::STRING,
        "expected contextual string parameter"
    );
}

#[test]
fn test_contextual_typing_for_variable_initializer() {
    use crate::parser::ParserState;

    let source = r#"
const handler: (x: string) => void = (x) => {
    let y: number = x;
};
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
        codes.contains(&2322),
        "Expected error 2322 (Type not assignable) from contextual typing, got: {codes:?}"
    );
}

#[test]
fn test_contextual_typing_overload_by_arity() {
    use crate::parser::ParserState;

    let source = r#"
function register(cb: (x: string) => void): void;
function register(cb: (x: number, y: boolean) => void, flag: boolean): void;
function register(cb: unknown, flag?: boolean) {}

register((x) => {
    let y: string = x;
});
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
        !codes.contains(&2322),
        "Did not expect error 2322 for overload-by-arity contextual typing, got: {codes:?}"
    );
}
