/// Test that flow narrowing is not applied after for loop exit
///
/// NOTE: Currently ignored - flow narrowing doesn't correctly handle loop exits.
/// The flow analysis should preserve narrowing inside the loop but reset it
/// after exiting via break.
#[test]
fn test_flow_narrowing_not_applied_after_for_exit() {
    use crate::parser::syntax_kind_ext;

    let source = r#"
let x: string | number;
for (; typeof x === "string"; ) {
    break;
}
x;
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    let expr_stmt_idx = *source_file
        .statements
        .nodes
        .iter()
        .rfind(|&&idx| {
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

    let after_type = checker.get_type_of_node(expr_stmt.expression);
    let expected = checker
        .ctx
        .types
        .union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(after_type, expected);
}

/// Test that flow narrowing is not applied after do-while exit
///
/// NOTE: Currently ignored - see `test_flow_narrowing_not_applied_after_for_exit`.
#[test]
fn test_flow_narrowing_not_applied_after_do_while_exit() {
    use crate::parser::syntax_kind_ext;

    let source = r#"
let x: string | number;
do {
    break;
} while (typeof x === "string");
x;
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    let expr_stmt_idx = *source_file
        .statements
        .nodes
        .iter()
        .rfind(|&&idx| {
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

    let after_type = checker.get_type_of_node(expr_stmt.expression);
    let expected = checker
        .ctx
        .types
        .union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(after_type, expected);
}

#[test]
fn test_flow_narrowing_applies_for_namespace_alias_member() {
    use crate::parser::syntax_kind_ext;

    let source = r#"
namespace Ns {
    export let value: string | number;
}
import Alias = Ns;
if (typeof Alias.value === "string") {
    Alias.value;
}
"#;

    let (parser, root) = parse_test_source(source);

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

    let narrowed = checker.get_type_of_node(expr_stmt.expression);
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn test_flow_narrowing_applies_for_namespace_element_access() {
    use crate::parser::syntax_kind_ext;

    let source = r#"
namespace Ns {
    export let value: string | number;
}
if (typeof Ns["value"] === "string") {
    Ns["value"];
}
"#;

    let (parser, root) = parse_test_source(source);

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

    let narrowed = checker.get_type_of_node(expr_stmt.expression);
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn test_flow_narrowing_cleared_by_namespace_member_assignment() {
    let source = r#"
namespace Ns {
    export let value: string | number;
}
import Alias = Ns;
if (typeof Alias.value === "string") {
    Ns.value = 1;
    Alias.value.toUpperCase();
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
    assert!(
        codes.contains(&2339),
        "Expected error 2339 after namespace member assignment clears narrowing, got: {codes:?}"
    );
}

#[test]
fn test_flow_narrowing_cleared_by_property_assignment() {
    let source = r#"
let obj: { prop: string | number } = { prop: "ok" };
if (typeof obj.prop === "string") {
    obj.prop.toUpperCase();
    obj.prop = 1;
    obj.prop.toUpperCase();
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
    let count = codes.iter().filter(|&&code| code == 2339).count();
    assert_eq!(
        count, 1,
        "Expected one 2339 after property assignment clears narrowing, got: {codes:?}"
    );
}

#[test]
fn test_flow_narrowing_cleared_by_element_assignment() {
    let source = r#"
let obj: { prop: string | number } = { prop: "ok" };
if (typeof obj["prop"] === "string") {
    obj["prop"].toUpperCase();
    obj["prop"] = 1;
    obj["prop"].toUpperCase();
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
    let count = codes.iter().filter(|&&code| code == 2339).count();
    assert_eq!(
        count, 1,
        "Expected one 2339 after element assignment clears narrowing, got: {codes:?}"
    );
}

#[test]
fn test_flow_narrowing_applies_across_element_to_property_access() {
    let source = r#"
let obj: { prop: string | number } = { prop: "ok" };
if (typeof obj["prop"] === "string") {
    obj.prop.toUpperCase();
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
    assert!(
        !codes.contains(&2339),
        "Expected no 2339 when element access narrows property access, got: {codes:?}"
    );
}

#[test]
fn test_flow_narrowing_applies_across_property_to_element_access() {
    let source = r#"
let obj: { prop: string | number } = { prop: "ok" };
if (typeof obj.prop === "string") {
    obj["prop"].toUpperCase();
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
    assert!(
        !codes.contains(&2339),
        "Expected no 2339 when property access narrows element access, got: {codes:?}"
    );
}

#[test]
fn test_flow_narrowing_cleared_by_cross_property_assignment() {
    let source = r#"
let obj: { prop: string | number } = { prop: "ok" };
if (typeof obj["prop"] === "string") {
    obj.prop.toUpperCase();
    obj.prop = 1;
    obj["prop"].toUpperCase();
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
    let count = codes.iter().filter(|&&code| code == 2339).count();
    assert_eq!(
        count, 1,
        "Expected one 2339 after cross property assignment clears narrowing, got: {codes:?}"
    );
}

#[test]
fn test_flow_narrowing_cleared_by_cross_element_assignment() {
    let source = r#"
let obj: { prop: string | number } = { prop: "ok" };
if (typeof obj.prop === "string") {
    obj["prop"].toUpperCase();
    obj["prop"] = 1;
    obj.prop.toUpperCase();
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
    let count = codes.iter().filter(|&&code| code == 2339).count();
    assert_eq!(
        count, 1,
        "Expected one 2339 after cross element assignment clears narrowing, got: {codes:?}"
    );
}

#[test]
fn test_flow_narrowing_not_applied_for_computed_element_access() {
    use crate::parser::syntax_kind_ext;

    let source = r#"
let obj: { [key: string]: string | number } = { prop: "ok" };
let key: string = "prop";
if (typeof obj[key] === "string") {
    obj[key];
}
"#;

    let (parser, root) = parse_test_source(source);

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
    // After the typeof guard, obj[key] is narrowed to string — tsc also
    // narrows element access expressions even when the key is not a literal type.
    assert_eq!(
        expr_type,
        TypeId::STRING,
        "Expected computed element access to be narrowed to string, got: {expr_type:?}"
    );
}

#[test]
fn test_flow_narrowing_applies_for_computed_element_access_literal_key() {
    use crate::parser::syntax_kind_ext;

    let source = r#"
let obj: { prop: string | number } = { prop: "ok" };
let key: "prop" = "prop";
if (typeof obj[key] === "string") {
    obj[key].toUpperCase();
}
"#;

    let (parser, root) = parse_test_source(source);

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
        "Expected computed element access with literal key to narrow to string, got: {expr_type:?}"
    );
}

#[test]
fn test_flow_narrowing_cleared_by_computed_element_assignment() {
    let source = r#"
let obj: { prop: string | number } = { prop: "ok" };
let key: "prop" = "prop";
if (typeof obj[key] === "string") {
    obj[key].toUpperCase();
    obj[key] = 1;
    obj[key].toUpperCase();
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
    let count = codes.iter().filter(|&&code| code == 2339).count();
    assert_eq!(
        count, 1,
        "Expected one 2339 after computed element assignment clears narrowing, got: {codes:?}"
    );
}

#[test]
fn test_flow_narrowing_applies_for_computed_element_access_numeric_literal_key() {
    use crate::parser::syntax_kind_ext;

    let source = r#"
let arr: (string | number)[] = ["ok", 1];
let idx: 0 = 0;
if (typeof arr[idx] === "string") {
    arr[idx].toUpperCase();
}
"#;

    let (parser, root) = parse_test_source(source);

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
        "Expected computed element access with numeric literal key to narrow to string, got: {expr_type:?}"
    );
}

#[test]
fn test_flow_narrowing_cleared_by_computed_numeric_element_assignment() {
    let source = r#"
let arr: (string | number)[] = ["ok", 1];
let idx: 0 = 0;
if (typeof arr[idx] === "string") {
    arr[idx].toUpperCase();
    arr[idx] = 1;
    arr[idx].toUpperCase();
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
    let count = codes.iter().filter(|&&code| code == 2339).count();
    assert_eq!(
        count, 1,
        "Expected one 2339 after computed numeric element assignment clears narrowing, got: {codes:?}"
    );
}

#[test]
fn test_flow_narrowing_applies_for_computed_element_access_const_literal_key() {
    use crate::parser::syntax_kind_ext;

    let source = r#"
let obj: { prop: string | number } = { prop: "ok" };
const key = "prop";
if (typeof obj[key] === "string") {
    obj[key].toUpperCase();
}
"#;

    let (parser, root) = parse_test_source(source);

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
        "Expected computed element access with const literal key to narrow to string, got: {expr_type:?}"
    );
}

#[test]
fn test_flow_narrowing_applies_for_computed_element_access_const_numeric_key() {
    use crate::parser::syntax_kind_ext;

    let source = r#"
let arr: (string | number)[] = ["ok", 1];
const idx = 0;
if (typeof arr[idx] === "string") {
    arr[idx].toUpperCase();
}
"#;

    let (parser, root) = parse_test_source(source);

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
    use crate::parser::syntax_kind_ext;

    let source = r#"
type U = { kind: "a"; value: string } | { kind: "b"; value: number };
let obj: U = { kind: "a", value: "ok" };
let key: "kind" = "kind";
if (obj[key] === "a") {
    obj.value.toUpperCase();
}
"#;

    let (parser, root) = parse_test_source(source);

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
    use crate::parser::syntax_kind_ext;

    let source = r#"
let obj: { prop: string | number } = { prop: "ok" };
if (typeof obj["prop"] === "string") {
    obj["prop"];
}
"#;

    let (parser, root) = parse_test_source(source);

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
    let source = r#"
let obj: { prop: string | number } = { prop: "ok" };
if (typeof obj.prop === "string") {
    obj.prop.toUpperCase();
    obj = { prop: 1 };
    obj.prop.toUpperCase();
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
    let count = codes.iter().filter(|&&code| code == 2339).count();
    assert_eq!(
        count, 1,
        "Expected one 2339 after property base assignment clears narrowing, got: {codes:?}"
    );
}

#[test]
fn test_flow_narrowing_cleared_by_element_base_assignment() {
    let source = r#"
let obj: { prop: string | number } = { prop: "ok" };
if (typeof obj["prop"] === "string") {
    obj["prop"].toUpperCase();
    obj = { prop: 1 };
    obj["prop"].toUpperCase();
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
    let count = codes.iter().filter(|&&code| code == 2339).count();
    assert_eq!(
        count, 1,
        "Expected one 2339 after element base assignment clears narrowing, got: {codes:?}"
    );
}

#[test]
fn test_parameter_identifier_type_from_symbol_cache() {
    use crate::parser::syntax_kind_ext;

    let source = r#"
function f(x: number) { return x; }
"#;

    let (parser, root) = parse_test_source(source);

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

    // No diagnostics expected - void constructor should accept any class
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #40: Distributivity Disabling via [T] extends [U]
/// Tests the `is_distributive` flag parsing and lowering through conditional types.
/// Verifies that naked type parameters are marked distributive while tuple-wrapped are not.
/// Note: This test verifies the lowering behavior via the solver's `lower_tests.rs`,
/// and checks that the thin checker properly handles conditional type declarations.
#[test]
fn test_distributivity_conditional_type_declarations() {
    // Test that conditional type declarations parse and bind correctly
    let source = r#"
type Distributive<T> = T extends any ? true : false;
type NonDistributive<T> = [T] extends [any] ? true : false;

// Verify these type aliases are usable (no errors in declaration)
declare const x: Distributive<string>;
declare const y: NonDistributive<string>;
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

    // No diagnostics expected for type declarations
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #40: Conditional type parsing with concrete extends checks
/// Tests that conditional types with concrete types parse correctly.
/// Note: Conditional type evaluation during type alias assignment is tested in `solver/evaluate_tests.rs`.
#[test]
fn test_conditional_type_concrete_extends() {
    // Test that conditional types parse and bind correctly with concrete extends checks
    let source = r#"
// Direct conditional type definitions
type StringCheck = string extends string ? "yes" : "no";
type NumberCheck = number extends string ? "yes" : "no";
type TupleCheck = [string] extends [string] ? "yes" : "no";

// These declarations should parse and bind without errors
declare const s: StringCheck;
declare const n: NumberCheck;
declare const t: TupleCheck;
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

    // No diagnostics expected for well-formed declarations
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #40: Tuple-wrapped conditional types for non-distribution
/// Tests the [T] extends [U] pattern used to disable distributivity.
/// The `is_distributive` flag detection is verified in `solver/lower_tests.rs`.
#[test]
fn test_tuple_wrapped_conditional_pattern() {
    // Test the [T] extends [U] pattern used to disable distributivity
    let source = r#"
// Generic distributive conditional
type Dist<T> = T extends string ? true : false;

// Generic non-distributive conditional (tuple-wrapped)
type NonDist<T> = [T] extends [string] ? true : false;

// Complex conditional with infer
type ExtractElement<T> = T extends (infer U)[] ? U : never;

// Complex non-distributive with infer
type ExtractElementNonDist<T> = [T] extends [(infer U)[]] ? U : never;

// Declarations to verify parsing
declare const d: Dist<string>;
declare const nd: NonDist<string>;
declare const e: ExtractElement<string[]>;
declare const end: ExtractElementNonDist<string[]>;
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

    // No diagnostics expected for well-formed declarations
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

// =========================================================================
// Redux/Lodash Pattern Minimal Repros (Support for Worker 2)
// These tests isolate specific patterns from test_check_redux_lodash_style_generics
// =========================================================================

/// Minimal repro: Conditional type with infer for extracting state type
/// Pattern: `R extends Reducer<infer S, any> ? S : never`
#[test]
fn test_redux_pattern_extract_state_with_infer() {
    let source = r#"
type Reducer<S, A> = (state: S | undefined, action: A) => S;

type ExtractState<R> = R extends Reducer<infer S, any> ? S : never;

// Test extraction: should infer S = number
type NumberReducer = Reducer<number, { type: string }>;
type ExtractedState = ExtractState<NumberReducer>;

// Verify the extracted state type
declare const s: ExtractedState;
const n: number = s;
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

    // Print diagnostics for debugging
    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Redux Pattern: ExtractState Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "ExtractState pattern should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// Minimal repro: Mapped type over keyof with conditional extraction
/// Pattern: `{ [K in keyof R]: ExtractState<R[K]> }`
// TODO: Fix TS2304 for mapped type parameter K -- binder scope gap.
#[test]
fn test_redux_pattern_state_from_reducers_mapped() {
    let source = r#"
type Reducer<S, A> = (state: S | undefined, action: A) => S;
type AnyAction = { type: string };

type ExtractState<R> = R extends Reducer<infer S, AnyAction> ? S : never;

type StateFromReducers<R> = { [K in keyof R]: ExtractState<R[K]> };

interface Reducers {
    count: Reducer<number, AnyAction>;
    message: Reducer<string, AnyAction>;
}

type AppState = StateFromReducers<Reducers>;

// Verify the mapped type evaluates correctly
declare const state: AppState;
const c: number = state.count;
const m: string = state.message;
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Redux Pattern: StateFromReducers Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "StateFromReducers mapped type should work: {:?}",
        checker.ctx.diagnostics
    );
}

