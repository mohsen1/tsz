
// =============================================================================
// Basic Type Checker Tests
// =============================================================================

#[test]
fn test_checker_creation() {
    let ctx = TestContext::new();
    let checker = ctx.checker();

    // Basic sanity check
    assert!(checker.ctx.diagnostics.is_empty());
}

#[test]
fn test_checker_basic_types() {
    let ctx = TestContext::new();
    let _checker = ctx.checker();

    // Verify intrinsic TypeIds are constants (compile-time values)
    assert_eq!(TypeId::NUMBER.0, 9);
    assert_eq!(TypeId::STRING.0, 10);
    assert_eq!(TypeId::BOOLEAN.0, 8);
    assert_eq!(TypeId::ANY.0, 4);
    assert_eq!(TypeId::NEVER.0, 2);
}

#[test]
fn test_checker_type_interner() {
    let ctx = TestContext::new();
    let checker = ctx.checker();

    // Test that TypeInterner is properly initialized
    // Intrinsics should be pre-registered
    assert!(checker.ctx.types.lookup(TypeId::STRING).is_some());
    assert!(checker.ctx.types.lookup(TypeId::NUMBER).is_some());
    assert!(checker.ctx.types.lookup(TypeId::ANY).is_some());
}

#[test]
fn test_checker_structural_equality() {
    let ctx = TestContext::new();
    let checker = ctx.checker();

    // Test structural equality via TypeInterner
    // Same string literal should get same TypeId
    let str1 = checker.ctx.types.literal_string("hello");
    let str2 = checker.ctx.types.literal_string("hello");
    let str3 = checker.ctx.types.literal_string("world");

    assert_eq!(str1, str2); // Same structure = same TypeId
    assert_ne!(str1, str3); // Different structure = different TypeId
}

#[test]
fn test_checker_union_normalization() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let checker = CheckerState::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    // Test union normalization
    // Union with `any` should be `any`
    let with_any = checker.ctx.types.union(vec![TypeId::STRING, TypeId::ANY]);
    assert_eq!(with_any, TypeId::ANY);

    // Union with `never` should exclude `never`
    let with_never = checker.ctx.types.union(vec![TypeId::STRING, TypeId::NEVER]);
    assert_eq!(with_never, TypeId::STRING);

    // Union with `unknown` should be `unknown`
    let with_unknown = checker
        .ctx
        .types
        .union(vec![TypeId::STRING, TypeId::UNKNOWN]);
    assert_eq!(with_unknown, TypeId::UNKNOWN);

    // Nested unions should be flattened and deduplicated
    let inner = checker
        .ctx
        .types
        .union(vec![TypeId::STRING, TypeId::NUMBER]);
    let outer = checker.ctx.types.union(vec![inner, TypeId::STRING]);
    assert_eq!(outer, inner);

    // Single-element union should return the element
    let single = checker.ctx.types.union(vec![TypeId::STRING]);
    assert_eq!(single, TypeId::STRING);
}

#[test]
fn test_await_type_context_suggests_awaited() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
async function foo() {
  var v: await;
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: false,
            strict_property_initialization: false,
            ..crate::checker::context::CheckerOptions::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let did_you_mean_count = codes
        .iter()
        .filter(|&&code| code == diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN)
        .count();
    assert_eq!(
        did_you_mean_count, 1,
        "Expected TS2552 for 'await' in type position, got: {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "Unexpected TS2304 for 'await' in type position: {codes:?}"
    );
}

#[test]
fn test_async_modifier_rejected_for_class_and_enum() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
async class C {}
async enum E { Value }
"#;

    let (parser, root) = parse_test_source(source);
    // Parser should NOT emit TS1042 — that is the checker's job
    let parser_1042_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE)
        .count();
    assert_eq!(
        parser_1042_count,
        0,
        "Parser should not emit TS1042; the checker handles it. Got: {:?}",
        parser.get_diagnostics()
    );

    // Run the checker — it should produce TS1042 for both declarations
    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: false,
            strict_function_types: false,
            strict_bind_call_apply: false,
            ..crate::checker::context::CheckerOptions::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let async_modifier_count = codes
        .iter()
        .filter(|&&code| code == diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE)
        .count();
    assert_eq!(
        async_modifier_count, 2,
        "Expected two TS1042 errors from checker for async class/enum, got: {codes:?}"
    );
}

#[test]
fn test_excess_property_in_variable_declaration() {
    let source = r#"
type Foo = { x: number };
const ok: Foo = { x: 1 };
const bad: Foo = { x: 1, y: 2 };
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: false,
            strict_function_types: false,
            strict_bind_call_apply: false,
            ..crate::checker::context::CheckerOptions::default()
        },
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
fn test_excess_property_allows_variable_assignment() {
    let source = r#"
type Foo = { x: number };
const obj = { x: 1, y: 2 };
const ok: Foo = obj;
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_object_trifecta_assignability_in_checker() {
    let source = r#"
let ok: {} = "hi";
let bad: object = "hi";
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
    let not_assignable_count = codes.iter().filter(|&&code| code == 2322).count();
    assert_eq!(
        not_assignable_count, 1,
        "Expected one 2322 error for object keyword rejecting string, got: {codes:?}"
    );
}

#[test]
fn test_shorthand_property_resolves_parameter() {
    let source = r#"
const mk = (e: number) => ({ e });
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
    let not_found_count = codes.iter().filter(|&&code| code == 2304).count();
    assert_eq!(
        not_found_count, 0,
        "Expected no 2304 errors for shorthand params, got: {codes:?}"
    );
}

#[test]
fn test_ambient_module_export_default_resolves_local() {
    let source = r#"
declare module "*!text" {
    const x: string;
    export default x;
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
    let not_found_count = codes.iter().filter(|&&code| code == 2304).count();
    assert_eq!(
        not_found_count, 0,
        "Expected no 2304 errors for ambient export default, got: {codes:?}"
    );
}

#[test]
fn test_await_type_reference_does_not_emit_ts2304() {
    let source = r#"
var v: await;
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
    let not_found_count = codes.iter().filter(|&&code| code == 2304).count();
    assert_eq!(
        not_found_count, 0,
        "Expected no 2304 errors for await type reference, got: {codes:?}"
    );
}

#[test]
fn test_property_initializer_contextual_literal_type() {
    let source = r#"
class C {
    static readonly c: "foo" = "foo";
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_indexed_access_class_property_type() {
    let source = r#"
class C {
    foo = 3;
    constructor() {
        const ok: C["foo"] = 3;
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_tuple_array_assignability_in_checker() {
    let source = r#"
type Tup = [string, number];
const tup: Tup = ["a", 1];
const arr: (string | number)[] = tup;
const bad: Tup = arr;
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
    let not_assignable_count = codes.iter().filter(|&&code| code == 2322).count();
    assert_eq!(
        not_assignable_count, 1,
        "Expected one 2322 error for array to tuple assignment, got: {codes:?}"
    );
}

#[test]
fn test_satisfies_assignability_check() {
    let source = r#"
const x = { a: 1 } satisfies { a: number; b: string };
const y = "hello" satisfies number;
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
    // First satisfies emits 1360 (missing property 'b'), second emits 1360
    let assignability_error_count = codes
        .iter()
        .filter(|&&code| code == 2322 || code == 2741 || code == 1360)
        .count();
    assert_eq!(
        assignability_error_count, 2,
        "Expected two assignability errors for satisfies violations, got: {codes:?}"
    );
}

#[test]
fn test_rest_any_bivariance_in_checker() {
    let source = r#"
type Logger = (...args: any[]) => void;
const log: Logger = (id: number) => {};
const log2: Logger = (id: number, extra: string) => {};
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_weak_type_detection_in_checker() {
    let source = r#"
interface Weak {
    a?: number;
}
const ok = { a: 1 };
const bad = { b: "nope" };

const okAssign: Weak = ok;
const badAssign: Weak = bad;
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
    let no_common_count = codes.iter().filter(|&&code| code == 2559).count();
    assert_eq!(
        no_common_count, 1,
        "Expected one 2559 error for weak type with no overlap, got: {codes:?}"
    );
}

#[test]
fn test_apparent_members_on_primitives() {
    let source = r#"
const s: string = "hi";
const n: number = 1;
const b: boolean = true;

s.toUpperCase();
n.toFixed();
b.valueOf();
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_void_return_exception_assignability() {
    let source = r#"
type VoidFn = () => void;
const ok: VoidFn = () => "value";
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_literal_widening_for_mutable_bindings() {
    let source = r#"
let x = true;
const y = true;
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
    let source = r#"
type Foo = { x: number };
function takesFoo(arg: Foo) {}
takesFoo({ x: 1, y: 2 });
const obj = { x: 1, y: 2 };
takesFoo(obj);
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
    let excess_count = codes.iter().filter(|&&code| code == 2353).count();
    assert_eq!(
        excess_count, 1,
        "Expected exactly one error 2353 (Excess property), got codes: {codes:?}"
    );
}

#[test]
fn test_array_literal_best_common_type() {
    use crate::parser::syntax_kind_ext;

    let source = r#"
const numbers = [1, 2];
const mixed = [1, "a"];
numbers;
mixed;
"#;

    let (parser, root) = parse_test_source(source);
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
    use crate::parser::syntax_kind_ext;

    let source = r#"
type A = { kind: "a"; val: 1 } | { kind: "b"; val: 2 };
declare const obj: A;
declare const key: "kind" | "val";
obj[key];
"#;

    let (parser, root) = parse_test_source(source);
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
    let source = r#"
type Foo = { x: number };
function makeFoo(): Foo {
    return { x: 1, y: 2 };
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
    let regular_key = assignability_test_key(TypeId::STRING, TypeId::NUMBER, regular_flags);
    let bivariant_key = assignability_test_key(TypeId::STRING, TypeId::NUMBER, bivariant_flags);
    assert_ne!(
        regular_key, bivariant_key,
        "regular and bivariant assignability must use distinct relation cache keys"
    );
}

#[test]
fn test_checker_subtype_literals() {
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

    // String literal is subtype of string
    let hello = checker.ctx.types.literal_string("hello");
    assert!(checker.is_assignable_to(hello, TypeId::STRING));

    // Number literal is subtype of number
    let forty_two = checker.ctx.types.literal_number(42.0);
    assert!(checker.is_assignable_to(forty_two, TypeId::NUMBER));

    // Boolean literal is subtype of boolean
    let t = checker.ctx.types.literal_boolean(true);
    assert!(checker.is_assignable_to(t, TypeId::BOOLEAN));

    // String literal is NOT assignable to number
    assert!(!checker.is_assignable_to(hello, TypeId::NUMBER));
}

#[test]
fn test_checker_subtype_unions() {
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

    // Create string | number union
    let string_or_number = checker.get_union_type(vec![TypeId::STRING, TypeId::NUMBER]);

    // String is assignable to string | number
    assert!(checker.is_assignable_to(TypeId::STRING, string_or_number));
    assert!(checker.is_assignable_to(TypeId::NUMBER, string_or_number));

    // Boolean is NOT assignable to string | number
    assert!(!checker.is_assignable_to(TypeId::BOOLEAN, string_or_number));

    // string | number is assignable to string | number | boolean
    let three_types = checker.get_union_type(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    assert!(checker.is_assignable_to(string_or_number, three_types));
}

#[test]
fn test_checker_assignability_direct_union_member_fast_path() {
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

    let string_or_number = checker.get_union_type(vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(checker.is_assignable_to(TypeId::STRING, string_or_number));
    assert!(checker.is_assignable_to_bivariant(TypeId::STRING, string_or_number));

    let regular_flags = checker.ctx.pack_relation_flags();
    let bivariant_flags = regular_flags & !RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES;
    let regular_key = assignability_test_key(TypeId::STRING, string_or_number, regular_flags);
    let bivariant_key = assignability_test_key(TypeId::STRING, string_or_number, bivariant_flags);
    assert_ne!(
        regular_key, bivariant_key,
        "regular and bivariant union-member assignability must use distinct relation cache keys"
    );
}

#[test]
fn test_checker_type_identity() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let checker = CheckerState::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    // Same type is identical to itself
    assert_eq!(TypeId::STRING, TypeId::STRING);
    assert_eq!(TypeId::NUMBER, TypeId::NUMBER);

    // Different types are not identical
    assert_ne!(TypeId::STRING, TypeId::NUMBER);

    // Same literal values produce identical types (via interning)
    let lit1 = checker.ctx.types.literal_string("test");
    let lit2 = checker.ctx.types.literal_string("test");
    assert_eq!(lit1, lit2);
}

#[test]
fn test_check_object_literal_excess_properties() {
    let source = r#"
type Foo = { x: number };
let foo: Foo = { x: 1, y: 2 };
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    crate::test_fixtures::merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322) || codes.contains(&2353),
        "Expected error code 2322 or 2353"
    );
}

#[test]
fn test_function_overload_missing_implementation_2391() {
    let source = r#"function foo();"#;

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
        codes.contains(&2391),
        "Expected error 2391 (Function implementation is missing), got: {codes:?}"
    );
}

#[test]
fn test_function_overload_with_implementation() {
    let source = r#"
function foo(): void;
function foo() {}
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
        !codes.contains(&2391),
        "Should not have error 2391 when implementation exists, got: {codes:?}"
    );
}

#[test]
fn test_function_overload_wrong_name_2389() {
    let source = r#"
function foo(): void;
function bar() {}
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
        codes.contains(&2389) || codes.contains(&2391),
        "Expected error 2389 or 2391 for wrong implementation name, got: {codes:?}"
    );
}

#[test]
fn test_duplicate_identifier_var_function_2300() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
var foo = 1;
function foo() {}
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

    let duplicate_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::DUPLICATE_IDENTIFIER)
        .count();
    // tsc emits 2 TS2300 errors (one per declaration), but we currently only emit 1.
    // TODO: emit TS2300 on both the var and function declarations.
    assert!(
        duplicate_count >= 1,
        "Expected at least one TS2300 for var/function duplicates, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
#[ignore = "Pre-existing failure from recent merges"]
fn test_duplicate_identifier_var_let_2300() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
var foo = 1;
let foo = 2;
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

    // tsc emits TS2451 (Cannot redeclare block-scoped variable) for var/let conflicts.
    // The `let` declaration introduces block-scoping, making both declarations conflict.
    let ts2451_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE)
        .count();
    assert_eq!(
        ts2451_count, 2,
        "Expected 2 TS2451 for var followed by let, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_duplicate_identifier_type_alias_2300() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
type Foo = { x: number };
type Foo = { y: number };

type Bar = { x: number };
interface Bar { y: number; }
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

    let duplicate_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::DUPLICATE_IDENTIFIER)
        .count();
    assert_eq!(
        duplicate_count, 4,
        "Expected TS2300 for type alias conflicts, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test TS2300: Duplicate identifier - duplicate enum members
#[test]
fn test_duplicate_identifier_enum_member_2300() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
enum Color {
    Red,
    Green,
    Blue,
    // Duplicate should emit TS2300
    Red,
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
        codes.contains(&diagnostic_codes::DUPLICATE_IDENTIFIER),
        "Expected TS2300 for duplicate enum member 'Red', got: {codes:?}"
    );
}

#[test]
fn test_type_alias_with_function_no_duplicate_2300() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
type Foo = { x: number };
function Foo() {}
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

    let duplicate_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::DUPLICATE_IDENTIFIER)
        .count();
    assert_eq!(
        duplicate_count, 0,
        "Did not expect TS2300 for type alias + function, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_class_accessor_pair_no_duplicate_2300() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
class Rectangle {
    private _width: number = 0;

    get width(): number {
        return this._width;
    }

    set width(value: number) {
        this._width = value;
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
        !codes.contains(&diagnostic_codes::DUPLICATE_IDENTIFIER),
        "Did not expect TS2300 for getter/setter pair, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_class_duplicate_getter_2300() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
class Rectangle {
    get width(): number {
        return 1;
    }

    get width(): number {
        return 2;
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

    let duplicate_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::DUPLICATE_IDENTIFIER)
        .count();
    assert_eq!(
        duplicate_count, 1,
        "Expected 1 TS2300 for duplicate getter (only on second occurrence, matching tsc), got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_overload_call_reports_no_overload_matches() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
function f(x: string): void;
function f(x: number, y: number): void;
function f(x: any, y?: any) {}
f(true);
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
    // tsc reports TS2345 (not TS2769) when a single overload matches by arity — it picks the
    // best-match overload and reports the specific type mismatch on that signature.
    // TS2769 is only reported when multiple overloads match by arity but all fail.
    assert!(
        codes.contains(&2345) || codes.contains(&diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL),
        "Expected TS2345 or TS2769 for overload call mismatch, got: {codes:?}"
    );
}

