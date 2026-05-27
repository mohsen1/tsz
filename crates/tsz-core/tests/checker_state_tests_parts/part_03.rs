#[test]
fn test_readonly_index_signature_element_access_assignment_2542() {
    // Error 2542: Index signature in type 'MyReadonlyMap' only permits reading.

    let source = r#"
interface MyReadonlyMap {
    readonly [key: string]: number;
}
let map: MyReadonlyMap = { a: 1 };
map["a"] = 2;
"#;

    let (parser, root) = parse_test_source(source);

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

    let source = r#"
interface ReadonlyMap {
    readonly [key: string]: number;
}
let map: ReadonlyMap = { a: 1 };
let key: string = "a";
map[key] = 2;
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

    let source = r#"
interface Person {
    readonly name: string;
}
let p: Person = { name: "Alice" };
// This property does not exist on Person - should get TS2339, NOT TS2540
p.nonexistent = "error";
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

    let source = r#"
declare var v: readonly [number, number, ...number[]];
v[0 + 1] = 1;
"#;

    let (parser, root) = parse_test_source(source);

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

    let source = r#"
declare var v: readonly [number, number, ...number[]];
delete v[2];
"#;

    let (parser, root) = parse_test_source(source);

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
    use tsz_solver::computation::ContextualTypeContext;

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
    use crate::parser::syntax_kind_ext;
    use tsz_solver::TypeData;

    let source = r#"
function takesHandler(fn: (this: { value: number }, x: string) => void) {}
takesHandler(function(this: { value: number }, x) {
    let y: number = x;
});
"#;

    let (parser, root) = parse_test_source(source);

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
    let source = r#"
const handler: (x: string) => void = (x) => {
    let y: number = x;
};
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
        codes.contains(&2322),
        "Expected error 2322 (Type not assignable) from contextual typing, got: {codes:?}"
    );
}

#[test]
fn test_contextual_typing_overload_by_arity() {
    let source = r#"
function register(cb: (x: string) => void): void;
function register(cb: (x: number, y: boolean) => void, flag: boolean): void;
function register(cb: unknown, flag?: boolean) {}

register((x) => {
    let y: string = x;
});
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
        !codes.contains(&2322),
        "Did not expect error 2322 for overload-by-arity contextual typing, got: {codes:?}"
    );
}

#[test]
fn test_contextual_typing_for_object_properties() {
    use tsz_solver::computation::ContextualTypeContext;

    // Test that ContextualTypeContext can extract property types from object types
    let types = TypeInterner::new();

    // Create an object type: { name: string, age: number }
    use tsz_solver::PropertyInfo;

    let obj_type = types.object(vec![
        PropertyInfo {
            name: types.intern_string("name"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo {
            name: types.intern_string("age"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
    ]);

    // Create contextual context
    let ctx = ContextualTypeContext::with_expected(&types, obj_type);

    // Test property type extraction
    assert_eq!(ctx.get_property_type("name"), Some(TypeId::STRING));
    assert_eq!(ctx.get_property_type("age"), Some(TypeId::NUMBER));
    assert_eq!(ctx.get_property_type("unknown"), None);
}

#[test]
fn test_contextual_property_type_infers_callback_param() {
    let source = r#"
type Handler = { cb: (x: number) => void };
const h: Handler = { cb: x => x.toUpperCase() };
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
        codes.contains(&2339),
        "Expected error 2339 for contextual property param mismatch, got: {codes:?}"
    );
}

#[test]
fn test_ts2339_any_property_access_no_error() {
    let source = r#"
let value: any;
value.foo;
value.bar();
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2339),
        "Did not expect 2339 for property access on any, got: {codes:?}"
    );
}

#[test]
fn test_ts2339_unknown_property_access_after_narrowing() {
    let source = r#"
let value: unknown = {};
value.foo;
const obj: object = value as object;
obj.foo;
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

    // value.foo → TS18046 ("'value' is of type 'unknown'.")
    let ts18046_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18046)
        .count();
    assert_eq!(
        ts18046_count, 1,
        "Expected one TS18046 error for unknown.foo, got: {:?}",
        checker.ctx.diagnostics
    );
    // obj.foo → TS2339 ("Property 'foo' does not exist on type 'object'.")
    let ts2339_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();
    assert_eq!(
        ts2339_count, 1,
        "Expected one TS2339 error for object.foo, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_ts2339_catch_binding_unknown() {
    let source = r#"
// @strict: true
function f() {
    try {
    } catch ({ x }) {
    }
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

    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();
    assert!(
        count >= 1,
        "Expected at least one 2339 for catch destructuring from unknown, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_ts2339_union_optional_property_access() {
    let source = r#"
type A = { foo?: string };
type B = { foo: string };

function read(value: A | B) {
    return value.foo;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2339),
        "Did not expect 2339 for optional property on union, got: {codes:?}"
    );
}

#[test]
fn test_ts2339_class_static_inheritance() {
    let source = r#"
class Base {
    static foo: number;
}

class Derived extends Base {}

Derived.foo;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2339),
        "Did not expect 2339 for inherited static property access, got: {codes:?}"
    );
}

#[test]
fn test_ts2339_class_instance_object_members() {
    let source = r#"
class C {
    x: number = 1;
}

const c = new C();
c.toString();
c.hasOwnProperty("x");
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2339),
        "Did not expect 2339 for Object prototype member access, got: {codes:?}"
    );
}

#[test]
fn test_ts2339_this_missing_property_in_class() {
    let source = r#"
class C {
    constructor() {
        this.missing;
    }
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2339),
        "Expected 2339 for missing property on this, got: {codes:?}"
    );
}

#[test]
fn test_ts2339_static_property_access_from_instance() {
    let source = r#"
class C {
    static foo: number;
    static get bar() { return 1; }
    value = 1;
}

const c = new C();
c.foo;
c.bar;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2339) || codes.contains(&2576),
        "Expected TS2339/TS2576 for static property access on instance, got: {codes:?}"
    );
}

#[test]
fn test_ts2339_computed_name_this_missing_static() {
    let source = r#"
class C {
    static [this.missing] = 123;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // tsc emits TS2465 ("'this' keyword is not allowed in class element computed names")
    // rather than TS2339 when 'this' is used in a computed property name — the property
    // access is not type-checked once the illegal 'this' is detected.
    assert!(
        codes.contains(&2465),
        "Expected 2465 for 'this' in computed name, got: {codes:?}"
    );
}

#[test]
fn test_ts2339_computed_name_this_in_class_expression() {
    let source = r#"
class C {
    static readonly c: "foo" = "foo";
    static bar = class Inner {
        static [this.c] = 123;
        [this.c] = 123;
    }
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // tsc emits TS2465 for each 'this' in computed property names within the inner class,
    // not TS2339 — property access is not type-checked on an illegal 'this'.
    let count = codes.iter().filter(|&&c| c == 2465).count();
    assert_eq!(
        count, 2,
        "Expected two 2465 errors for class expression computed this, got: {codes:?}"
    );
}

#[test]
fn test_ts2339_private_name_missing_on_index_signature() {
    let source = r#"
class A {
    [k: string]: any;
    #foo = 3;
    constructor() {
        this.#f = 3;
    }
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Currently emits TS18013 (not yet TS2339) for missing private name with index signature.
    let has_private_error = codes.iter().any(|&c| c == 2339 || c == 18013);
    assert!(
        has_private_error,
        "Expected a private-name error (2339 or 18013), got: {codes:?}"
    );
}

#[test]
fn test_ts2339_private_name_in_expression_typo() {
    let source = r#"
class Foo {
    #field = 1;
    check(v: any) {
        const ok = #field in v;
        const bad = #fiel in v;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    // Parser may emit diagnostics for private name `in` expressions; that's fine.

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

    // TODO: TS2339 is not yet emitted for misspelled private names in `in` expressions.
    // Currently no checker diagnostic is produced; the test verifies no crash occurs.
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let _count = codes.iter().filter(|&&c| c == 2339).count();
    // When TS2339 for private names is implemented, assert count == 1 here.
}

#[test]
fn test_ts2339_class_interface_merge() {
    let source = r#"
interface C {
    x: number;
}

class C {
    y = 1;
}

const c = new C();
c.x;
c.y;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2339),
        "Did not expect 2339 for class/interface merge, got: {codes:?}"
    );
}

#[test]
fn test_strict_null_checks_property_access() {
    use tsz_solver::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};
    use tsz_solver::{PropertyInfo, TypeId, Visibility};

    // Test property access on nullable types
    let types = TypeInterner::new();

    // Create object type: { x: number }
    let obj_type = types.object(vec![PropertyInfo {
        name: types.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // Create union type: { x: number } | null
    let nullable_obj = types.union(vec![obj_type, TypeId::NULL]);

    let evaluator = PropertyAccessEvaluator::new(&types);

    // Access property on nullable type should return PossiblyNullOrUndefined
    let result = evaluator.resolve_property_access(nullable_obj, "x");
    match result {
        PropertyAccessResult::PossiblyNullOrUndefined {
            property_type,
            cause,
        } => {
            // Should have property_type = number
            assert_eq!(property_type, Some(TypeId::NUMBER));
            // Cause should be null
            assert_eq!(cause, TypeId::NULL);
        }
        _ => panic!("Expected PossiblyNullOrUndefined, got {result:?}"),
    }
}

#[test]
fn test_strict_null_checks_undefined_type() {
    use tsz_solver::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};
    use tsz_solver::{PropertyInfo, TypeId, Visibility};

    // Test property access on possibly undefined types
    let types = TypeInterner::new();

    // Create object type: { y: string }
    let obj_type = types.object(vec![PropertyInfo {
        name: types.intern_string("y"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // Create union type: { y: string } | undefined
    let possibly_undefined = types.union(vec![obj_type, TypeId::UNDEFINED]);

    let evaluator = PropertyAccessEvaluator::new(&types);

    // Access property on possibly undefined type
    let result = evaluator.resolve_property_access(possibly_undefined, "y");
    match result {
        PropertyAccessResult::PossiblyNullOrUndefined {
            property_type,
            cause,
        } => {
            assert_eq!(property_type, Some(TypeId::STRING));
            assert_eq!(cause, TypeId::UNDEFINED);
        }
        _ => panic!("Expected PossiblyNullOrUndefined, got {result:?}"),
    }
}

#[test]
fn test_strict_null_checks_both_null_and_undefined() {
    use tsz_solver::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};
    use tsz_solver::{PropertyInfo, TypeData, TypeId, Visibility};

    // Test property access on type that is both null and undefined
    let types = TypeInterner::new();

    // Create object type: { z: boolean }
    let obj_type = types.object(vec![PropertyInfo {
        name: types.intern_string("z"),
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // Create union type: { z: boolean } | null | undefined
    let nullable_undefined = types.union(vec![obj_type, TypeId::NULL, TypeId::UNDEFINED]);

    let evaluator = PropertyAccessEvaluator::new(&types);

    // Access property on possibly null or undefined type
    let result = evaluator.resolve_property_access(nullable_undefined, "z");
    match result {
        PropertyAccessResult::PossiblyNullOrUndefined {
            property_type,
            cause,
        } => {
            assert_eq!(property_type, Some(TypeId::BOOLEAN));
            // Cause should be a union of null | undefined
            let cause_key = types.lookup(cause);
            match cause_key {
                Some(TypeData::Union(members)) => {
                    let members = types.type_list(members);
                    assert!(members.contains(&TypeId::NULL), "Cause should contain null");
                    assert!(
                        members.contains(&TypeId::UNDEFINED),
                        "Cause should contain undefined"
                    );
                }
                _ => panic!("Expected cause to be union of null | undefined"),
            }
        }
        _ => panic!("Expected PossiblyNullOrUndefined, got {result:?}"),
    }
}

#[test]
fn test_strict_null_checks_non_nullable_success() {
    use tsz_solver::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};
    use tsz_solver::{PropertyInfo, TypeId, Visibility};

    // Test that non-nullable types succeed normally
    let types = TypeInterner::new();

    // Create object type: { x: number }
    let obj_type = types.object(vec![PropertyInfo {
        name: types.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    let evaluator = PropertyAccessEvaluator::new(&types);

    // Access property on non-nullable type should succeed
    let result = evaluator.resolve_property_access(obj_type, "x");
    match result {
        PropertyAccessResult::Success {
            type_id: prop_type, ..
        } => {
            assert_eq!(prop_type, TypeId::NUMBER);
        }
        _ => panic!("Expected Success, got {result:?}"),
    }
}

#[test]
fn test_strict_null_checks_null_only() {
    use tsz_solver::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};

    // Test accessing property directly on null type
    let types = TypeInterner::new();

    let evaluator = PropertyAccessEvaluator::new(&types);

    let result = evaluator.resolve_property_access(TypeId::NULL, "anything");
    match result {
        PropertyAccessResult::PossiblyNullOrUndefined {
            property_type,
            cause,
        } => {
            assert_eq!(property_type, None);
            assert_eq!(cause, TypeId::NULL);
        }
        _ => panic!("Expected PossiblyNullOrUndefined, got {result:?}"),
    }
}

// ============== Symbol type checking tests ==============

#[test]
fn test_symbol_constructor_call_signature() {
    // Skip test - lib loading was removed
    // Tests that need lib files should use the TestContext API
}

#[test]
fn test_symbol_constructor_too_many_args() {
    // Skip test - lib loading was removed
    // Tests that need lib files should use the TestContext API
}

#[test]
fn test_variable_redeclaration_same_type() {
    // Test that redeclaring a variable with the same type is allowed
    let source = r#"function test() {
    var x: string;
    var x: string;
}"#;

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

    // Should have no errors - same type is allowed
    assert_eq!(checker.ctx.diagnostics.len(), 0);
}

#[test]
fn test_variable_redeclaration_different_type_2403() {
    // Test that redeclaring a variable with different type causes error TS2403
    // Must be inside a function where local scopes are active
    let source = r#"function test() {
    var x: string;
    var x: number;
}"#;

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

    // Should have error 2403: Subsequent variable declarations must have the same type
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2403),
        "Expected error 2403 for variable redeclaration, got: {codes:?}"
    );
}

#[test]
fn test_variable_self_reference_no_2403() {
    // Self-references in a var initializer should not trigger TS2403.
    let source = r#"function test() {
    var x = {
        x,
        parent: x
    };
}"#;

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
        !codes.contains(&2403),
        "Expected no error 2403 for self-referential var initializer, got: {codes:?}"
    );
}

#[test]
fn test_param_var_redecl_ts2403() {
    // TS2403: var redeclaration of optional parameter with different type
    // `options?: number` has type `number | undefined`, var declares `number`
    let source = r#"class C {
    constructor(options?: number) {
        var options = (options || 0);
    }
}"#;

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
        codes.contains(&2403),
        "Expected TS2403 for parameter/var type mismatch, got: {codes:?}"
    );
}

#[test]
fn test_symbol_property_access_description() {
    use tsz_solver::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};

    // Test accessing .description on symbol type
    let types = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&types);

    let result = evaluator.resolve_property_access(TypeId::SYMBOL, "description");
    match result {
        PropertyAccessResult::Success {
            type_id: prop_type, ..
        } => {
            // description should be string | undefined
            let key = types.lookup(prop_type).expect("Property type should exist");
            match key {
                TypeData::Union(members) => {
                    let members = types.type_list(members);
                    assert_eq!(members.len(), 2);
                    assert!(members.contains(&TypeId::STRING));
                    assert!(members.contains(&TypeId::UNDEFINED));
                }
                _ => panic!("Expected union type for description, got: {key:?}"),
            }
        }
        _ => panic!("Expected Success for symbol.description, got: {result:?}"),
    }
}

#[test]
fn test_symbol_property_access_methods() {
    use tsz_solver::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};

    // Test accessing methods on symbol type
    let types = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&types);

    // toString and valueOf should use the symbol apparent method return types.
    let result_to_string = evaluator.resolve_property_access(TypeId::SYMBOL, "toString");
    match result_to_string {
        PropertyAccessResult::Success {
            type_id: prop_type, ..
        } => {
            let Some(TypeData::Function(shape_id)) = types.lookup(prop_type) else {
                panic!("Expected symbol.toString to resolve to function type");
            };
            let shape = types.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::STRING);
        }
        _ => panic!("Expected Success for symbol.toString, got: {result_to_string:?}"),
    }

    let result_value_of = evaluator.resolve_property_access(TypeId::SYMBOL, "valueOf");
    match result_value_of {
        PropertyAccessResult::Success {
            type_id: prop_type, ..
        } => {
            let Some(TypeData::Function(shape_id)) = types.lookup(prop_type) else {
                panic!("Expected symbol.valueOf to resolve to function type");
            };
            let shape = types.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::SYMBOL);
        }
        _ => panic!("Expected Success for symbol.valueOf, got: {result_value_of:?}"),
    }
}

#[test]
fn test_symbol_property_not_found() {
    use tsz_solver::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};

    // Test accessing non-existent property on symbol type
    let types = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&types);
    let name_atom = types.intern_string("nonexistent");

    let result = evaluator.resolve_property_access(TypeId::SYMBOL, "nonexistent");
    match result {
        PropertyAccessResult::PropertyNotFound {
            type_id,
            property_name,
        } => {
            assert_eq!(type_id, TypeId::SYMBOL);
            assert_eq!(property_name, name_atom);
        }
        _ => panic!("Expected PropertyNotFound for unknown property, got: {result:?}"),
    }
}

// ============== Property access from index signature tests (error 4111) ==============

#[test]
fn test_property_access_from_index_signature_4111() {
    let source = r#"
interface StringMap {
    [key: string]: number;
}
const obj: StringMap = {} as any;
const val = obj.someProperty;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        no_property_access_from_index_signature: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&4111),
        "Expected error 4111 for property access from index signature, got: {codes:?}"
    );
}

#[test]
fn test_explicit_property_no_error_4111() {
    let source = r#"
interface MixedType {
    explicitProp: string;
    [key: string]: string | number;
}
const obj: MixedType = {} as any;
const val = obj.explicitProp;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        no_property_access_from_index_signature: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&4111),
        "Should not have error 4111 for explicit property"
    );
}

/// TODO: Property access from index signature on mixed unions incorrectly emits TS4111.
/// When a union has one member with an explicit property and another with an index
/// signature, tsc does NOT emit TS4111 for the explicit property. Currently we do emit it.
/// When this is fixed, update to assert !codes.contains(&4111).
#[test]
fn test_union_with_index_signature_4111() {
    let source = r#"
type Mixed = { x: number } | { [key: string]: number };
const obj: Mixed = {} as any;
const val = obj.x;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        no_property_access_from_index_signature: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Should NOT emit 4111 when union member has explicit property 'x'.
    // Previously incorrectly emitted TS4111 for mixed union with index signature;
    // fixed by preserving union index access diagnostics.
    assert!(
        !codes.contains(&4111),
        "Expected no TS4111 when union member has explicit property 'x', got: {codes:?}"
    );
}

#[test]
fn test_checker_lowers_full_source_file() {
    use tsz_solver::TypeData;

    let source = r#"
interface Foo { x: number; }
type Bar = Foo | string;
type Baz = [string, number];
type Qux = { [key: string]: Foo };
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let foo_sym = binder.file_locals.get("Foo").expect("Foo should exist");
    let bar_sym = binder.file_locals.get("Bar").expect("Bar should exist");
    let baz_sym = binder.file_locals.get("Baz").expect("Baz should exist");
    let qux_sym = binder.file_locals.get("Qux").expect("Qux should exist");

    let foo_type = checker.get_type_of_symbol(foo_sym);
    let foo_key = types.lookup(foo_type).expect("Foo type should exist");
    match foo_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "x")
                .expect("Expected property x");
            assert_eq!(prop.type_id, TypeId::NUMBER);
        }
        _ => panic!("Expected Foo to be Object type, got {foo_key:?}"),
    }

    let bar_type = checker.get_type_of_symbol(bar_sym);
    let bar_key = types.lookup(bar_type).expect("Bar type should exist");
    match bar_key {
        TypeData::Union(members) => {
            let members = types.type_list(members);
            assert_eq!(members.len(), 2);
            assert!(members.contains(&TypeId::STRING));
            // The non-string member may be a lazy type reference to Foo
            // (TypeData::Lazy) or the resolved Object type. Either is valid.
            let non_string_member = *members.iter().find(|&&m| m != TypeId::STRING).unwrap();
            if non_string_member != foo_type {
                // If not the same TypeId, verify it's a lazy reference (unevaluated Foo)
                let member_key = types
                    .lookup(non_string_member)
                    .expect("member type should exist");
                assert!(
                    matches!(member_key, TypeData::Lazy(_)),
                    "Non-string member should be foo_type or a Lazy reference, got {member_key:?}"
                );
            }
        }
        _ => panic!("Expected Bar to be Union type, got {bar_key:?}"),
    }

    let baz_type = checker.get_type_of_symbol(baz_sym);
    let baz_key = types.lookup(baz_type).expect("Baz type should exist");
    match baz_key {
        TypeData::Tuple(elements) => {
            let elements = types.tuple_list(elements);
            assert_eq!(elements.len(), 2);
            assert_eq!(elements[0].type_id, TypeId::STRING);
            assert_eq!(elements[1].type_id, TypeId::NUMBER);
        }
        _ => panic!("Expected Baz to be Tuple type, got {baz_key:?}"),
    }

    let qux_type = checker.get_type_of_symbol(qux_sym);
    let qux_key = types.lookup(qux_type).expect("Qux type should exist");
    match qux_key {
        TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let string_index = shape
                .string_index
                .as_ref()
                .expect("Expected string index signature");
            assert_eq!(string_index.key_type, TypeId::STRING);
            let value_key = types
                .lookup(string_index.value_type)
                .expect("Index value type should exist");
            match value_key {
                TypeData::Lazy(_def_id) => {} // Phase 4.2: Now uses Lazy(DefId) instead of Ref(SymbolRef)
                _ => panic!("Expected Foo lazy type, got {value_key:?}"),
            }
        }
        _ => panic!("Expected Qux to be ObjectWithIndex type, got {qux_key:?}"),
    }
}

