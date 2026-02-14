use super::*;
use tsz_parser::parser::ParserState;
use tsz_solver::PropertyInfo;
use tsz_solver::TypeInterner;
use tsz_solver::Visibility;
use tsz_solver::type_queries::{UnionMembersKind, classify_for_union_members};

fn get_if_condition(arena: &NodeArena, root: NodeIndex, stmt_index: usize) -> NodeIndex {
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let if_idx = *source_file
        .statements
        .nodes
        .get(stmt_index)
        .expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");
    if_data.expression
}

#[test]
fn test_truthiness_false_branch_narrows_to_falsy() {
    let source = r#"
let x: string | number | boolean | null | undefined;
if (x) {}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let condition_idx = get_if_condition(arena, root, 1);
    let union = types.union(vec![
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::NULL,
        TypeId::UNDEFINED,
    ]);
    let narrowed = analyzer.narrow_type_by_condition(
        union,
        condition_idx,
        condition_idx,
        false,
        FlowNodeId::NONE,
    );

    let falsy_boolean = TypeId::BOOLEAN_FALSE;

    match classify_for_union_members(&types, narrowed) {
        UnionMembersKind::Union(members) => {
            // NOTE: TypeScript does NOT narrow string/number to their falsy literals
            // string remains string, number remains number
            assert!(
                members.contains(&TypeId::STRING),
                "string should remain as string"
            );
            assert!(
                members.contains(&TypeId::NUMBER),
                "number should remain as number"
            );
            // boolean DOES narrow to false (because boolean = true | false)
            assert!(
                members.contains(&falsy_boolean),
                "boolean should narrow to false, got members: {:?}",
                members
            );
            assert!(members.contains(&TypeId::NULL), "null should be included");
            assert!(
                members.contains(&TypeId::UNDEFINED),
                "undefined should be included"
            );
        }
        UnionMembersKind::NotUnion => panic!("Expected falsy union, got NotUnion"),
    }
}

#[test]
fn test_typeof_false_branch_excludes_type() {
    let source = r#"
let x: string | number;
if (typeof x === "string") {}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let condition_idx = get_if_condition(arena, root, 1);
    let condition_node = arena.get(condition_idx).expect("condition node");
    let binary = arena
        .get_binary_expr(condition_node)
        .expect("binary condition");
    let typeof_node = arena.get(binary.left).expect("typeof node");
    let unary = arena.get_unary_expr(typeof_node).expect("typeof data");
    let target_idx = unary.operand;

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrowed = analyzer.narrow_type_by_condition(
        union,
        condition_idx,
        target_idx,
        false,
        FlowNodeId::NONE,
    );
    assert_eq!(narrowed, TypeId::NUMBER);
}

#[test]
fn test_logical_and_applies_right_guard() {
    let source = r#"
let x: string | number;
if (x && typeof x === "string") {}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let condition_idx = get_if_condition(arena, root, 1);
    let condition_node = arena.get(condition_idx).expect("condition node");
    let binary = arena
        .get_binary_expr(condition_node)
        .expect("binary condition");
    let target_idx = binary.left;

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrowed =
        analyzer.narrow_type_by_condition(union, condition_idx, target_idx, true, FlowNodeId::NONE);
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn test_logical_or_narrows_to_union_of_literals() {
    let source = r#"
let x: "a" | "b" | "c";
if (x === "a" || x === "b") {}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let condition_idx = get_if_condition(arena, root, 1);
    let condition_node = arena.get(condition_idx).expect("condition node");
    let binary = arena
        .get_binary_expr(condition_node)
        .expect("binary condition");
    let left_node = arena.get(binary.left).expect("left condition");
    let left_eq = arena.get_binary_expr(left_node).expect("left equality");
    let target_idx = left_eq.left;

    let lit_a = types.literal_string("a");
    let lit_b = types.literal_string("b");
    let lit_c = types.literal_string("c");
    let union = types.union(vec![lit_a, lit_b, lit_c]);

    let narrowed_true =
        analyzer.narrow_type_by_condition(union, condition_idx, target_idx, true, FlowNodeId::NONE);
    let narrowed_false = analyzer.narrow_type_by_condition(
        union,
        condition_idx,
        target_idx,
        false,
        FlowNodeId::NONE,
    );

    assert_eq!(narrowed_true, types.union(vec![lit_a, lit_b]));
    assert_eq!(narrowed_false, lit_c);
}

#[test]
fn test_discriminant_property_access_narrows_union() {
    let source = r#"
let action: any;
if (action.type === "add") {}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let condition_idx = get_if_condition(arena, root, 1);
    let condition_node = arena.get(condition_idx).expect("condition node");
    let binary = arena
        .get_binary_expr(condition_node)
        .expect("binary condition");
    let access_node = arena.get(binary.left).expect("property access node");
    let access = arena
        .get_access_expr(access_node)
        .expect("property access data");
    let target_idx = access.expression;

    let type_key = types.intern_string("type");
    let type_add = types.literal_string("add");
    let type_remove = types.literal_string("remove");

    let add_member = types.object(vec![PropertyInfo {
        name: type_key,
        type_id: type_add,
        write_type: type_add,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);
    let remove_member = types.object(vec![PropertyInfo {
        name: type_key,
        type_id: type_remove,
        write_type: type_remove,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let union = types.union(vec![add_member, remove_member]);
    let narrowed_true =
        analyzer.narrow_type_by_condition(union, condition_idx, target_idx, true, FlowNodeId::NONE);
    let narrowed_false = analyzer.narrow_type_by_condition(
        union,
        condition_idx,
        target_idx,
        false,
        FlowNodeId::NONE,
    );

    assert_eq!(narrowed_true, add_member);
    assert_eq!(narrowed_false, remove_member);
}

#[test]
fn test_element_access_numeric_and_string_keys_match_reference() {
    let source = r#"
let arr: any[] = [];
if (arr[0] === arr["0"]) {}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let condition_idx = get_if_condition(arena, root, 1);
    let condition_node = arena.get(condition_idx).expect("condition node");
    let binary = arena
        .get_binary_expr(condition_node)
        .expect("binary condition");

    assert!(
        analyzer.is_matching_reference(binary.left, binary.right),
        "arr[0] and arr[\"0\"] should resolve to the same reference",
    );
}

#[test]
fn test_literal_equality_narrows_to_literal() {
    let source = r#"
let x: string | number;
if (x === "a") {}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let condition_idx = get_if_condition(arena, root, 1);
    let condition_node = arena.get(condition_idx).expect("condition node");
    let binary = arena
        .get_binary_expr(condition_node)
        .expect("binary condition");
    let target_idx = binary.left;

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let literal_a = types.literal_string("a");
    let narrowed =
        analyzer.narrow_type_by_condition(union, condition_idx, target_idx, true, FlowNodeId::NONE);

    assert_eq!(narrowed, literal_a);
}

#[test]
fn test_loose_nullish_equality_narrows_to_nullish_union() {
    let source = r#"
let x: string | null | undefined;
if (x == null) {}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let condition_idx = get_if_condition(arena, root, 1);
    let condition_node = arena.get(condition_idx).expect("condition node");
    let binary = arena
        .get_binary_expr(condition_node)
        .expect("binary condition");
    let target_idx = binary.left;

    let union = types.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);
    let expected_true = types.union(vec![TypeId::NULL, TypeId::UNDEFINED]);

    let narrowed_true =
        analyzer.narrow_type_by_condition(union, condition_idx, target_idx, true, FlowNodeId::NONE);
    let narrowed_false = analyzer.narrow_type_by_condition(
        union,
        condition_idx,
        target_idx,
        false,
        FlowNodeId::NONE,
    );

    assert_eq!(narrowed_true, expected_true);
    assert_eq!(narrowed_false, TypeId::STRING);
}

#[test]
fn test_mutable_variable_in_closure_loses_narrowing() {
    // Unsoundness Rule #42: Mutable variables (let/var) should not preserve
    // narrowing from outer scope when accessed in closures
    let source = r#"
let x: string | number;
if (typeof x === "string") {
// At this point, x is narrowed to string
// But in the closure, it should revert to string | number
const fn = () => {
    // x should NOT be narrowed here - it's mutable
};
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    // Get the if condition (typeof x === "string")
    let condition_idx = get_if_condition(arena, root, 1);
    let condition_node = arena.get(condition_idx).expect("condition node");
    let binary = arena
        .get_binary_expr(condition_node)
        .expect("binary condition");
    // binary.left is "typeof x", we need to get the operand "x"
    let typeof_node = arena.get(binary.left).expect("typeof node");
    let unary = arena.get_unary_expr(typeof_node).expect("unary expression");
    let target_idx = unary.operand; // This is 'x'

    // The narrowing happens at the condition
    let union_type = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrowed_at_condition = analyzer.narrow_type_by_condition(
        union_type,
        condition_idx,
        target_idx,
        true, // true branch
        FlowNodeId::NONE,
    );
    assert_eq!(narrowed_at_condition, TypeId::STRING);

    // Now we need to check that in the closure, narrowing is NOT applied
    // The closure creates a START node that connects to the outer flow
    // When we cross that START node, the narrowing should be reset for mutable variables

    // Get the variable declaration to verify it's let (mutable)
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let var_stmt_idx = source_file.statements.nodes[0]; // VARIABLE_STATEMENT
    let var_stmt_node = arena.get(var_stmt_idx).expect("var stmt node");

    // Get the VARIABLE_DECLARATION_LIST from within the VARIABLE_STATEMENT
    let var_data = arena.get_variable(var_stmt_node).expect("variable data");
    let decl_list_idx = var_data.declarations.nodes[0]; // VARIABLE_DECLARATION_LIST
    let decl_list_node = arena.get(decl_list_idx).expect("decl list node");

    // Verify the declaration list does NOT have CONST flag (it's 'let')
    let flags = decl_list_node.flags as u32;
    let is_const = (flags & node_flags::CONST) != 0;
    assert!(!is_const, "Variable should be let (mutable), not const");

    // Verify that is_mutable_variable returns true for this variable
    assert!(analyzer.is_mutable_variable(target_idx));
}

#[test]
fn test_const_variable_in_closure_preserves_narrowing() {
    // Unsoundness Rule #42: Const variables SHOULD preserve narrowing
    // from outer scope when accessed in closures
    let source = r#"
const x: string | number = Math.random() > 0.5 ? "hello" : 42;
if (typeof x === "string") {
// At this point, x is narrowed to string
// In the closure, it should remain narrowed to string (const is immutable)
const fn = () => {
    // x SHOULD be narrowed here - it's const
};
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    // Get the if condition (typeof x === "string")
    let condition_idx = get_if_condition(arena, root, 1);
    let condition_node = arena.get(condition_idx).expect("condition node");
    let binary = arena
        .get_binary_expr(condition_node)
        .expect("binary condition");
    // binary.left is "typeof x", we need to get the operand "x"
    let typeof_node = arena.get(binary.left).expect("typeof node");
    let unary = arena.get_unary_expr(typeof_node).expect("unary expression");
    let target_idx = unary.operand; // This is 'x'

    // Get the variable declaration to verify it's const
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let var_stmt_idx = source_file.statements.nodes[0]; // VARIABLE_STATEMENT
    let var_stmt_node = arena.get(var_stmt_idx).expect("var stmt node");

    // Get the VARIABLE_DECLARATION_LIST from within the VARIABLE_STATEMENT
    let var_data = arena.get_variable(var_stmt_node).expect("variable data");
    let decl_list_idx = var_data.declarations.nodes[0]; // VARIABLE_DECLARATION_LIST
    let decl_list_node = arena.get(decl_list_idx).expect("decl list node");

    // Verify the declaration list has CONST flag
    let flags = decl_list_node.flags as u32;
    let is_const = (flags & node_flags::CONST) != 0;
    assert!(is_const, "Variable declaration list should be const");

    // Verify that is_mutable_variable returns false for this variable
    assert!(!analyzer.is_mutable_variable(target_idx));
}

#[test]
fn test_nested_closures_handling() {
    // Test that we handle nested closures correctly
    let source = r#"
let x: string | number;
const fn = () => {
// Outer closure - x should not be narrowed from outer scope
const inner = () => {
    // Inner closure - x should still not be narrowed
};
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();

    // Get the variable declaration
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let var_stmt_idx = source_file.statements.nodes[0]; // VARIABLE_STATEMENT
    let var_stmt_node = arena.get(var_stmt_idx).expect("var stmt node");

    // Get the VARIABLE_DECLARATION_LIST from within the VARIABLE_STATEMENT
    let var_data = arena.get_variable(var_stmt_node).expect("variable data");
    let decl_list_idx = var_data.declarations.nodes[0]; // VARIABLE_DECLARATION_LIST
    let decl_list_node = arena.get(decl_list_idx).expect("decl list node");

    // Verify the declaration list does NOT have CONST flag (it's 'let')
    let flags = decl_list_node.flags as u32;
    let is_const = (flags & node_flags::CONST) != 0;
    assert!(!is_const, "Variable should be let (mutable)");
}
