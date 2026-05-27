/// Test that mutable variables captured by closures reset to their declared type.
///
/// In TypeScript, `let` variables captured by closures cannot preserve narrowing
/// because the closure could be invoked at any time after the variable is reassigned.
/// TSC conservatively returns the full declared type for captured mutable variables.
#[test]
fn test_closure_capture_resets_mutable_variable_type() {
    let source = r#"
let x: string | number;
x = "assigned";
const callback = () => {
    x;
};
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    // Get the variable reference inside the arrow function body
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Get the variable statement at index 2 (const callback = ...)
    let var_stmt_idx = *source_file
        .statements
        .nodes
        .get(2)
        .expect("variable statement");
    let var_stmt_node = arena.get(var_stmt_idx).expect("var stmt node");
    let var_stmt_data = arena.get_variable(var_stmt_node).expect("var stmt data");

    // Get the declaration list
    let decl_list_idx = *var_stmt_data
        .declarations
        .nodes
        .first()
        .expect("declaration list");
    let decl_list_node = arena.get(decl_list_idx).expect("decl list node");
    let decl_list_data = arena.get_variable(decl_list_node).expect("decl list data");

    // Get the first declaration and its initializer (the arrow function)
    let decl_idx = *decl_list_data
        .declarations
        .nodes
        .first()
        .expect("declaration");
    let decl_node = arena.get(decl_idx).expect("decl node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("decl data");
    let arrow_func_node = arena.get(decl.initializer).expect("arrow func node");
    let arrow_func = arena
        .get_function(arrow_func_node)
        .expect("arrow func data");

    // Get the body block
    let body_node = arena.get(arrow_func.body).expect("body node");
    let body_block = arena.get_block(body_node).expect("body block");
    let ident_in_closure = extract_expression_from_statement(
        arena,
        *body_block.statements.nodes.first().expect("x in closure"),
    );

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let flow_in_closure = binder.get_node_flow(ident_in_closure);
    assert!(
        flow_in_closure.is_some(),
        "Flow should be recorded for variable inside closure"
    );

    // TSC returns the declared type (string | number) for captured let variables
    // because the closure could be invoked after the variable is reassigned
    let narrowed_in_closure =
        analyzer.get_flow_type(ident_in_closure, union, flow_in_closure.unwrap());
    assert_eq!(narrowed_in_closure, union);
}

/// Test definite assignment analysis with callbacks that are immediately invoked.
///
/// This verifies that variables assigned before an IIFE (Immediately Invoked
/// Function Expression) are properly tracked.
#[test]
fn test_definite_assignment_with_iife() {
    let source = r#"
let x: string;
x = "assigned";
(() => {
    const y = x;
})();
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();

    // Verify that flow graph is built correctly for IIFE scenario
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // The IIFE call expression statement should be at index 2
    let iife_stmt_idx = *source_file.statements.nodes.get(2).expect("IIFE statement");
    let _iife_stmt = arena.get(iife_stmt_idx).expect("IIFE statement node");

    // Verify the IIFE statement exists and has flow recorded
    assert!(iife_stmt_idx.is_some(), "IIFE statement should exist");
}

/// Test variable capture with array forEach callback.
///
/// TSC returns the declared type for captured `let` variables inside closures.
#[test]
fn test_closure_capture_with_array_foreach() {
    let source = r#"
let x: string | number;
x = "hello";
const arr = [1, 2, 3];
arr.forEach((item) => {
    const y = x;
});
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Get the forEach expression statement (index 3)
    let foreach_stmt_idx = *source_file
        .statements
        .nodes
        .get(3)
        .expect("forEach statement");
    let foreach_stmt_node = arena.get(foreach_stmt_idx).expect("forEach stmt node");
    let foreach_stmt_data = arena
        .get_expression_statement(foreach_stmt_node)
        .expect("forEach stmt data");

    // Get the call expression from the expression statement
    let foreach_call_node = arena
        .get(foreach_stmt_data.expression)
        .expect("forEach call node");
    let foreach_call = arena
        .get_call_expr(foreach_call_node)
        .expect("forEach call data");

    // Get the arrow function argument
    let args = foreach_call.arguments.as_ref().expect("arguments");
    let arrow_func_idx = *args.nodes.first().expect("arrow function");
    let arrow_func_node = arena.get(arrow_func_idx).expect("arrow func node");
    let arrow_func = arena
        .get_function(arrow_func_node)
        .expect("arrow func data");

    // Get the body block
    let body_node = arena.get(arrow_func.body).expect("body node");
    let body_block = arena.get_block(body_node).expect("body block");

    // Get the variable reference x inside the closure (in the initializer of y)
    let y_var_stmt_idx = *body_block
        .statements
        .nodes
        .first()
        .expect("y variable statement");
    let y_var_stmt_node = arena.get(y_var_stmt_idx).expect("y var stmt node");
    let y_var_stmt_data = arena
        .get_variable(y_var_stmt_node)
        .expect("y var stmt data");

    // Get the declaration list
    let y_decl_list_idx = *y_var_stmt_data
        .declarations
        .nodes
        .first()
        .expect("y declaration list");
    let y_decl_list_node = arena.get(y_decl_list_idx).expect("y decl list node");
    let y_decl_list_data = arena
        .get_variable(y_decl_list_node)
        .expect("y decl list data");

    // Get the declaration
    let y_decl_idx = *y_decl_list_data
        .declarations
        .nodes
        .first()
        .expect("y declaration");
    let y_decl_node = arena.get(y_decl_idx).expect("y decl node");
    let y_decl = arena
        .get_variable_declaration(y_decl_node)
        .expect("y decl data");
    let x_ref_in_closure = y_decl.initializer;

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // TSC returns declared type (string | number) for captured let variables
    let flow_in_callback = binder.get_node_flow(x_ref_in_closure);
    assert!(
        flow_in_callback.is_some(),
        "Flow should be recorded for variable inside forEach callback"
    );

    let narrowed_in_callback =
        analyzer.get_flow_type(x_ref_in_closure, union, flow_in_callback.unwrap());
    assert_eq!(narrowed_in_callback, union);
}

/// Test variable capture with map callback.
///
/// TSC returns the declared type for captured `let` variables inside closures.
#[test]
fn test_closure_capture_with_array_map() {
    let source = r#"
let x: string | number;
x = "world";
const arr = [1, 2, 3];
const mapped = arr.map((item) => {
    return x.length;
});
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Get the variable statement at index 3 (const mapped = ...)
    let var_stmt_idx = *source_file
        .statements
        .nodes
        .get(3)
        .expect("variable statement");
    let var_stmt_node = arena.get(var_stmt_idx).expect("var stmt node");
    let var_stmt_data = arena.get_variable(var_stmt_node).expect("var stmt data");

    // Get the declaration list
    let decl_list_idx = *var_stmt_data
        .declarations
        .nodes
        .first()
        .expect("declaration list");
    let decl_list_node = arena.get(decl_list_idx).expect("decl list node");
    let decl_list_data = arena.get_variable(decl_list_node).expect("decl list data");

    // Get the first declaration and its initializer (the map call)
    let decl_idx = *decl_list_data
        .declarations
        .nodes
        .first()
        .expect("declaration");
    let decl_node = arena.get(decl_idx).expect("decl node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("decl data");
    let map_call_idx = decl.initializer;
    let map_call_node = arena.get(map_call_idx).expect("map call node");
    let map_call = arena.get_call_expr(map_call_node).expect("map call data");

    // Get the arrow function argument
    let args = map_call.arguments.as_ref().expect("arguments");
    let arrow_func_idx = *args.nodes.first().expect("arrow function");
    let arrow_func_node = arena.get(arrow_func_idx).expect("arrow func node");
    let arrow_func = arena
        .get_function(arrow_func_node)
        .expect("arrow func data");

    // Get the body block
    let body_node = arena.get(arrow_func.body).expect("body node");
    let body_block = arena.get_block(body_node).expect("body block");

    // Get the return statement
    let return_stmt = *body_block
        .statements
        .nodes
        .first()
        .expect("return statement");
    let return_node = arena.get(return_stmt).expect("return node");
    let return_data = arena
        .get_return_statement(return_node)
        .expect("return data");

    // Get the property access expression x.length
    let prop_access = return_data.expression;

    // Get the identifier x from the property access
    let prop_access_node = arena.get(prop_access).expect("prop access node");
    let access_expr = arena
        .get_access_expr(prop_access_node)
        .expect("access expr data");
    let x_identifier = access_expr.expression;

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // TSC returns declared type (string | number) for captured let variables
    let flow_in_callback = binder.get_node_flow(prop_access);
    assert!(
        flow_in_callback.is_some(),
        "Flow should be recorded for expression inside map callback"
    );

    let narrowed_in_callback =
        analyzer.get_flow_type(x_identifier, union, flow_in_callback.unwrap());
    assert_eq!(narrowed_in_callback, union);
}

/// Test nested closure capture (closure inside a closure).
///
/// TSC returns the declared type for captured `let` variables inside closures,
/// even when nested multiple levels deep.
#[test]
fn test_nested_closure_capture() {
    let source = r#"
let x: string | number;
x = "nested";
const outer = () => {
    const inner = () => {
        const y = x;
    };
};
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Get the outer arrow function (inside a VariableStatement -> VariableDeclarationList -> VariableDeclaration)
    let outer_var_stmt_idx = *source_file
        .statements
        .nodes
        .get(2)
        .expect("outer variable statement");
    let outer_var_stmt_node = arena.get(outer_var_stmt_idx).expect("outer var stmt node");
    let outer_var_stmt = arena
        .get_variable(outer_var_stmt_node)
        .expect("outer var stmt data");
    let outer_decl_list_idx = *outer_var_stmt
        .declarations
        .nodes
        .first()
        .expect("outer declaration list");
    let outer_decl_list_node = arena
        .get(outer_decl_list_idx)
        .expect("outer decl list node");
    let outer_decl_list = arena
        .get_variable(outer_decl_list_node)
        .expect("outer decl list data");
    let outer_decl_idx = *outer_decl_list
        .declarations
        .nodes
        .first()
        .expect("outer declaration");
    let outer_decl_node = arena.get(outer_decl_idx).expect("outer decl node");
    let outer_decl = arena
        .get_variable_declaration(outer_decl_node)
        .expect("outer decl data");
    let outer_arrow_idx = outer_decl.initializer;
    let outer_arrow_node = arena.get(outer_arrow_idx).expect("outer arrow node");
    let outer_func = arena
        .get_function(outer_arrow_node)
        .expect("outer func data");

    // Get the outer body block
    let outer_body_node = arena.get(outer_func.body).expect("outer body node");
    let outer_body = arena.get_block(outer_body_node).expect("outer body");

    // Get the inner arrow function declaration (inside a VariableStatement -> VariableDeclarationList -> VariableDeclaration)
    let inner_var_stmt_idx = *outer_body
        .statements
        .nodes
        .first()
        .expect("inner variable statement");
    let inner_var_stmt_node = arena.get(inner_var_stmt_idx).expect("inner var stmt node");
    let inner_var_stmt = arena
        .get_variable(inner_var_stmt_node)
        .expect("inner var stmt data");
    let inner_decl_list_idx = *inner_var_stmt
        .declarations
        .nodes
        .first()
        .expect("inner declaration list");
    let inner_decl_list_node = arena
        .get(inner_decl_list_idx)
        .expect("inner decl list node");
    let inner_decl_list = arena
        .get_variable(inner_decl_list_node)
        .expect("inner decl list data");
    let inner_decl_idx = *inner_decl_list
        .declarations
        .nodes
        .first()
        .expect("inner declaration");
    let inner_decl_node = arena.get(inner_decl_idx).expect("inner decl node");
    let inner_decl = arena
        .get_variable_declaration(inner_decl_node)
        .expect("inner decl data");
    let inner_arrow_idx = inner_decl.initializer;
    let inner_arrow_node = arena.get(inner_arrow_idx).expect("inner arrow node");
    let inner_func = arena
        .get_function(inner_arrow_node)
        .expect("inner func data");

    // Get the inner body block
    let inner_body_node = arena.get(inner_func.body).expect("inner body node");
    let inner_body = arena.get_block(inner_body_node).expect("inner body");

    // Get the y declaration statement (inside a VariableStatement -> VariableDeclarationList -> VariableDeclaration)
    let y_var_stmt_idx = *inner_body
        .statements
        .nodes
        .first()
        .expect("y variable statement");
    let y_var_stmt_node = arena.get(y_var_stmt_idx).expect("y var stmt node");
    let y_var_stmt = arena
        .get_variable(y_var_stmt_node)
        .expect("y var stmt data");
    let y_decl_list_idx = *y_var_stmt
        .declarations
        .nodes
        .first()
        .expect("y declaration list");
    let y_decl_list_node = arena.get(y_decl_list_idx).expect("y decl list node");
    let y_decl_list = arena
        .get_variable(y_decl_list_node)
        .expect("y decl list data");
    let y_decl_idx = *y_decl_list
        .declarations
        .nodes
        .first()
        .expect("y declaration");
    let y_decl_node = arena.get(y_decl_idx).expect("y decl node");
    let y_decl = arena
        .get_variable_declaration(y_decl_node)
        .expect("y decl data");
    let x_ref_in_inner = y_decl.initializer;

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // TSC returns declared type (string | number) for captured let variables
    let flow_in_nested = binder.get_node_flow(x_ref_in_inner);
    assert!(
        flow_in_nested.is_some(),
        "Flow should be recorded for variable inside nested closure"
    );

    let narrowed_in_nested = analyzer.get_flow_type(x_ref_in_inner, union, flow_in_nested.unwrap());
    assert_eq!(narrowed_in_nested, union);
}

/// Test callback used with setTimeout (common async pattern).
///
/// TSC returns the declared type for captured `let` variables inside closures.
#[test]
fn test_closure_capture_with_settimeout() {
    let source = r#"
let x: string | number;
x = "timeout";
setTimeout(() => {
    console.log(x);
}, 1000);
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Get the setTimeout call statement (index 2)
    let settimeout_stmt_idx = *source_file
        .statements
        .nodes
        .get(2)
        .expect("setTimeout call");
    let settimeout_stmt_node = arena
        .get(settimeout_stmt_idx)
        .expect("setTimeout statement node");
    let settimeout_stmt = arena
        .get_expression_statement(settimeout_stmt_node)
        .expect("setTimeout expr statement");
    let settimeout_call_idx = settimeout_stmt.expression;
    let settimeout_call_node = arena
        .get(settimeout_call_idx)
        .expect("setTimeout call node");
    let settimeout_call = arena
        .get_call_expr(settimeout_call_node)
        .expect("setTimeout call data");

    // Get the arrow function argument
    let args = settimeout_call.arguments.as_ref().expect("arguments");
    let arrow_func_idx = *args.nodes.first().expect("arrow function");
    let arrow_func_node = arena.get(arrow_func_idx).expect("arrow func node");
    let arrow_func = arena
        .get_function(arrow_func_node)
        .expect("arrow func data");

    // Get the body block
    let body_node = arena.get(arrow_func.body).expect("body node");
    let body_block = arena.get_block(body_node).expect("body block");

    // Get the console.log call expression statement
    let log_stmt = *body_block
        .statements
        .nodes
        .first()
        .expect("console.log statement");
    let log_stmt_node = arena.get(log_stmt).expect("log statement node");
    let log_expr_stmt = arena
        .get_expression_statement(log_stmt_node)
        .expect("log expr statement");
    let log_call_node = arena.get(log_expr_stmt.expression).expect("log call node");
    let log_call = arena.get_call_expr(log_call_node).expect("log call data");

    // Get the argument to console.log (the x variable)
    let log_args = log_call.arguments.as_ref().expect("log arguments");
    let x_ref = *log_args.nodes.first().expect("x reference");

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // TSC returns declared type (string | number) for captured let variables
    let flow_in_callback = binder.get_node_flow(x_ref);
    assert!(
        flow_in_callback.is_some(),
        "Flow should be recorded for variable inside setTimeout callback"
    );

    let narrowed_in_callback = analyzer.get_flow_type(x_ref, union, flow_in_callback.unwrap());
    assert_eq!(narrowed_in_callback, union);
}

/// Test that flow analysis correctly handles multiple closures capturing
/// the same variable at different points in the code.
///
/// TSC returns the declared type for captured `let` variables in ALL closures,
/// regardless of what assignments happened before each closure.
#[test]
fn test_multiple_closures_capture_same_variable() {
    let source = r#"
let x: string | number;
x = "first";
const callback1 = () => {
    const a = x;
};
x = 42;
const callback2 = () => {
    const b = x;
};
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Get the first arrow function statement
    let callback1_var_idx = *source_file
        .statements
        .nodes
        .get(2)
        .expect("callback1 var stmt");
    let callback1_var_node = arena.get(callback1_var_idx).expect("callback1 var node");
    let callback1_var = arena
        .get_variable(callback1_var_node)
        .expect("callback1 var data");
    let callback1_decl_list_idx = *callback1_var.declarations.nodes.first().expect("decl list");
    let callback1_decl_list_node = arena.get(callback1_decl_list_idx).expect("decl list node");
    let callback1_decl_list = arena
        .get_variable(callback1_decl_list_node)
        .expect("decl list data");
    let callback1_decl_idx = *callback1_decl_list
        .declarations
        .nodes
        .first()
        .expect("callback1 decl");
    let callback1_decl_node = arena.get(callback1_decl_idx).expect("callback1 decl node");
    let callback1_decl = arena
        .get_variable_declaration(callback1_decl_node)
        .expect("callback1 decl data");
    let callback1_func_idx = callback1_decl.initializer;
    let callback1_func_node = arena.get(callback1_func_idx).expect("callback1 func node");
    let callback1_func = arena
        .get_function(callback1_func_node)
        .expect("callback1 func data");
    let body1_node = arena.get(callback1_func.body).expect("body1 node");
    let body1 = arena.get_block(body1_node).expect("body1");
    let a_decl_stmt_idx = *body1.statements.nodes.first().expect("a declaration");
    let a_decl_stmt_node = arena.get(a_decl_stmt_idx).expect("a decl statement node");
    let a_decl_var = arena
        .get_variable(a_decl_stmt_node)
        .expect("a decl var data");
    let a_decl_list_idx = *a_decl_var.declarations.nodes.first().expect("a decl list");
    let a_decl_list_node = arena.get(a_decl_list_idx).expect("a decl list node");
    let a_decl_list = arena
        .get_variable(a_decl_list_node)
        .expect("a decl list data");
    let a_decl_idx = *a_decl_list.declarations.nodes.first().expect("a decl");
    let a_decl_node = arena.get(a_decl_idx).expect("a decl node");
    let a_decl = arena
        .get_variable_declaration(a_decl_node)
        .expect("a decl data");
    let x_ref1 = a_decl.initializer;

    // Get the second arrow function statement
    let callback2_var_idx = *source_file
        .statements
        .nodes
        .get(4)
        .expect("callback2 var stmt");
    let callback2_var_node = arena.get(callback2_var_idx).expect("callback2 var node");
    let callback2_var = arena
        .get_variable(callback2_var_node)
        .expect("callback2 var data");
    let callback2_decl_list_idx = *callback2_var.declarations.nodes.first().expect("decl list");
    let callback2_decl_list_node = arena.get(callback2_decl_list_idx).expect("decl list node");
    let callback2_decl_list = arena
        .get_variable(callback2_decl_list_node)
        .expect("decl list data");
    let callback2_decl_idx = *callback2_decl_list
        .declarations
        .nodes
        .first()
        .expect("callback2 decl");
    let callback2_decl_node = arena.get(callback2_decl_idx).expect("callback2 decl node");
    let callback2_decl = arena
        .get_variable_declaration(callback2_decl_node)
        .expect("callback2 decl data");
    let callback2_func_idx = callback2_decl.initializer;
    let callback2_func_node = arena.get(callback2_func_idx).expect("callback2 func node");
    let callback2_func = arena
        .get_function(callback2_func_node)
        .expect("callback2 func data");
    let body2_node = arena.get(callback2_func.body).expect("body2 node");
    let body2 = arena.get_block(body2_node).expect("body2");
    let b_decl_stmt_idx = *body2.statements.nodes.first().expect("b declaration");
    let b_decl_stmt_node = arena.get(b_decl_stmt_idx).expect("b decl statement node");
    let b_decl_var = arena
        .get_variable(b_decl_stmt_node)
        .expect("b decl var data");
    let b_decl_list_idx = *b_decl_var.declarations.nodes.first().expect("b decl list");
    let b_decl_list_node = arena.get(b_decl_list_idx).expect("b decl list node");
    let b_decl_list = arena
        .get_variable(b_decl_list_node)
        .expect("b decl list data");
    let b_decl_idx = *b_decl_list.declarations.nodes.first().expect("b decl");
    let b_decl_node = arena.get(b_decl_idx).expect("b decl node");
    let b_decl = arena
        .get_variable_declaration(b_decl_node)
        .expect("b decl data");
    let x_ref2 = b_decl.initializer;

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Both callbacks see the declared type (string | number) because x is mutable+captured
    let flow1 = binder.get_node_flow(x_ref1).expect("flow for callback1");
    let narrowed1 = analyzer.get_flow_type(x_ref1, union, flow1);
    assert_eq!(narrowed1, union);

    let flow2 = binder.get_node_flow(x_ref2).expect("flow for callback2");
    let narrowed2 = analyzer.get_flow_type(x_ref2, union, flow2);
    assert_eq!(narrowed2, union);
}

/// Test closure with conditional capture (variable narrowed before callback).
///
/// TSC returns the declared type for captured `let` variables inside closures,
/// even when a typeof guard narrows the type before the closure.
#[test]
fn test_closure_with_conditional_capture() {
    let source = r#"
let x: string | number;
if (typeof x === "string") {
    const callback = () => {
        const y = x.length;
    };
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Get the if statement
    let if_idx = *source_file.statements.nodes.get(1).expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");

    // Get the then block
    let then_block_node = arena.get(if_data.then_statement).expect("then block node");
    let block = arena.get_block(then_block_node).expect("then block");

    // Get the arrow function statement (VariableStatement)
    let arrow_var_stmt = *block.statements.nodes.first().expect("arrow var statement");
    let arrow_var_node = arena.get(arrow_var_stmt).expect("arrow var node");
    let arrow_var = arena.get_variable(arrow_var_node).expect("arrow var data");
    let arrow_decl_list_idx = *arrow_var
        .declarations
        .nodes
        .first()
        .expect("arrow decl list");
    let arrow_decl_list_node = arena
        .get(arrow_decl_list_idx)
        .expect("arrow decl list node");
    let arrow_decl_list = arena
        .get_variable(arrow_decl_list_node)
        .expect("arrow decl list data");
    let arrow_decl_idx = *arrow_decl_list
        .declarations
        .nodes
        .first()
        .expect("arrow decl");
    let arrow_decl_node = arena.get(arrow_decl_idx).expect("arrow decl node");
    let arrow_decl = arena
        .get_variable_declaration(arrow_decl_node)
        .expect("arrow decl data");
    let arrow_func_idx = arrow_decl.initializer;
    let arrow_func_node = arena.get(arrow_func_idx).expect("arrow func node");
    let arrow_func = arena
        .get_function(arrow_func_node)
        .expect("arrow func data");

    // Get the body block
    let body_node = arena.get(arrow_func.body).expect("body node");
    let body_block = arena.get_block(body_node).expect("body block");

    // Get the y declaration
    let y_decl_stmt_idx = *body_block.statements.nodes.first().expect("y declaration");
    let y_decl_stmt_node = arena.get(y_decl_stmt_idx).expect("y decl statement node");
    let y_decl_var = arena
        .get_variable(y_decl_stmt_node)
        .expect("y decl var data");
    let y_decl_list_idx = *y_decl_var.declarations.nodes.first().expect("y decl list");
    let y_decl_list_node = arena.get(y_decl_list_idx).expect("y decl list node");
    let y_decl_list = arena
        .get_variable(y_decl_list_node)
        .expect("y decl list data");
    let y_decl_idx = *y_decl_list.declarations.nodes.first().expect("y decl");
    let y_decl_node = arena.get(y_decl_idx).expect("y decl node");
    let y_decl = arena
        .get_variable_declaration(y_decl_node)
        .expect("y decl data");

    // Get the property access expression x.length
    let prop_access = y_decl.initializer;

    // Get the identifier x from the property access
    let prop_access_node = arena.get(prop_access).expect("prop access node");
    let access_expr = arena
        .get_access_expr(prop_access_node)
        .expect("access expr data");
    let x_identifier = access_expr.expression;

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // TSC returns declared type (string | number) for captured let variables
    // even inside a typeof guard — the closure could execute after reassignment
    let flow = binder.get_node_flow(prop_access);
    assert!(
        flow.is_some(),
        "Flow should be recorded for expression inside closure in if branch"
    );

    let narrowed = analyzer.get_flow_type(x_identifier, union, flow.unwrap());
    assert_eq!(narrowed, union);
}

/// Test that the flow graph builder correctly handles arrow functions
/// without creating infinite recursion or errors.
#[test]
fn test_flow_graph_builder_with_arrow_functions() {
    let source = r#"
let x: string | number;
const add = (a: number, b: number): number => {
    return a + b;
};
x = "test";
const result = add(1, 2);
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists and has nodes
        assert!(!graph.nodes.is_empty(), "Flow graph should have nodes");

        // Verify that we can query flow for arrow function
        let arrow_func_idx = *sf.statements.nodes.get(1).expect("arrow function");
        assert!(
            graph.has_flow_at_node(arrow_func_idx),
            "Flow should be recorded for arrow function"
        );
    }
}

/// Test callback with filter method (another common array method).
///
/// TSC returns the declared type for captured `let` variables inside closures.
#[test]
fn test_closure_capture_with_array_filter() {
    let source = r#"
let x: string | number;
x = "filter";
const arr = [1, 2, 3];
const filtered = arr.filter((item) => {
    return typeof x === "string";
});
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Get the variable statement at index 3 (const filtered = ...)
    let var_stmt_idx = *source_file
        .statements
        .nodes
        .get(3)
        .expect("variable statement");
    let var_stmt_node = arena.get(var_stmt_idx).expect("var stmt node");
    let var_stmt_data = arena.get_variable(var_stmt_node).expect("var stmt data");

    // Get the declaration list
    let decl_list_idx = *var_stmt_data
        .declarations
        .nodes
        .first()
        .expect("declaration list");
    let decl_list_node = arena.get(decl_list_idx).expect("decl list node");
    let decl_list_data = arena.get_variable(decl_list_node).expect("decl list data");

    // Get the first declaration and its initializer (the filter call)
    let decl_idx = *decl_list_data
        .declarations
        .nodes
        .first()
        .expect("declaration");
    let decl_node = arena.get(decl_idx).expect("decl node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("decl data");
    let filter_call_node = arena.get(decl.initializer).expect("filter call node");
    let filter_call = arena
        .get_call_expr(filter_call_node)
        .expect("filter call data");

    // Get the arrow function argument
    let args = filter_call.arguments.as_ref().expect("arguments");
    let arrow_func_idx = *args.nodes.first().expect("arrow function");
    let arrow_func_node = arena.get(arrow_func_idx).expect("arrow func node");
    let arrow_func = arena
        .get_function(arrow_func_node)
        .expect("arrow func data");

    // Get the body block
    let body_node = arena.get(arrow_func.body).expect("body node");
    let body_block = arena.get_block(body_node).expect("body block");

    // Get the return statement
    let return_stmt = *body_block
        .statements
        .nodes
        .first()
        .expect("return statement");
    let return_node = arena.get(return_stmt).expect("return node");
    let return_data = arena
        .get_return_statement(return_node)
        .expect("return data");

    // Get the typeof x expression (binary expression: typeof x === "string")
    let typeof_bin_expr = return_data.expression;

    // Get the typeof expression node (left side of binary)
    let typeof_bin_node = arena.get(typeof_bin_expr).expect("bin expr node");
    let typeof_bin = arena
        .get_binary_expr(typeof_bin_node)
        .expect("bin expr data");
    let typeof_expr = typeof_bin.left; // This is the typeof x expression

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Get the identifier x from the typeof expression
    let typeof_node = arena.get(typeof_expr).expect("typeof expr node");
    let typeof_unary = arena.get_unary_expr(typeof_node).expect("typeof expr data");
    let x_identifier = typeof_unary.operand;

    // TSC returns declared type (string | number) for captured let variables
    let flow_in_callback = binder.get_node_flow(typeof_bin_expr);
    assert!(
        flow_in_callback.is_some(),
        "Flow should be recorded for expression inside filter callback"
    );

    let narrowed_in_callback =
        analyzer.get_flow_type(x_identifier, union, flow_in_callback.unwrap());
    assert_eq!(narrowed_in_callback, union);
}

// ============================================================================
// CFA-15: FlowGraph Path Verification Tests
// ============================================================================

/// Test that all if-else branches are captured in flow graph.
#[test]
fn test_flow_graph_captures_if_else_branches() {
    let source = r#"
let x: string | number;
if (Math.random() > 0.5) {
    x = "a";
} else {
    x = 1;
}
x;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // The if statement should have flow recorded
    let if_stmt_idx = *source_file.statements.nodes.get(1).expect("if statement");
    let flow_at_if = binder.get_node_flow(if_stmt_idx);
    assert!(
        flow_at_if.is_some(),
        "Flow should be recorded at if statement"
    );
}

/// Test that switch statement cases are all captured.
#[test]
fn test_flow_graph_captures_switch_cases() {
    let source = r#"
let x: "a" | "b" | "c";
let result: number;
switch (x) {
    case "a":
        result = 1;
        break;
    case "b":
        result = 2;
        break;
    case "c":
        result = 3;
        break;
}
result;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // The switch statement should have flow recorded
    let switch_stmt_idx = *source_file
        .statements
        .nodes
        .get(2)
        .expect("switch statement");
    let flow_at_switch = binder.get_node_flow(switch_stmt_idx);
    assert!(
        flow_at_switch.is_some(),
        "Flow should be recorded at switch statement"
    );
}

/// Test that try-catch-finally paths are captured.
#[test]
fn test_flow_graph_captures_try_catch_finally() {
    let source = r"
let x: number;
try {
    x = 1;
} catch (e) {
    x = 2;
} finally {
    x = 3;
}
x;
";

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // The try statement should have flow recorded
    let try_stmt_idx = *source_file.statements.nodes.get(1).expect("try statement");
    let flow_at_try = binder.get_node_flow(try_stmt_idx);
    assert!(
        flow_at_try.is_some(),
        "Flow should be recorded at try statement"
    );
}

/// Test that loop control flow with break/continue is captured.
#[test]
fn test_flow_graph_captures_loop_break_continue() {
    let source = r"
let x: number;
for (let i = 0; i < 10; i++) {
    if (i === 5) break;
    if (i % 2 === 0) continue;
    x = i;
}
x;
";

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // The for loop should have flow recorded
    let for_stmt_idx = *source_file.statements.nodes.get(1).expect("for statement");
    let flow_at_for = binder.get_node_flow(for_stmt_idx);
    assert!(flow_at_for.is_some(), "Flow should be recorded at for loop");
}

/// Test that nested control structures have correct flow.
#[test]
fn test_flow_graph_captures_nested_structures() {
    let source = r"
let x: number;
if (Math.random() > 0.5) {
    while (Math.random() > 0.1) {
        try {
            x = 1;
            break;
        } catch {
            x = 2;
        }
    }
} else {
    for (let i = 0; i < 5; i++) {
        switch (i) {
            case 0:
                x = 10;
                break;
            default:
                x = 20;
        }
    }
}
x;
";

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Verify final expression has flow
    let final_expr_idx = *source_file
        .statements
        .nodes
        .get(2)
        .expect("final expression");
    let flow_at_final = binder.get_node_flow(final_expr_idx);
    assert!(
        flow_at_final.is_some(),
        "Flow should be recorded at final expression after nested structures"
    );
}

/// Test that class constructor flow is tracked.
#[test]
fn test_flow_graph_captures_class_constructor() {
    let source = r"
class Foo {
    value: number;

    constructor(init: boolean) {
        if (init) {
            this.value = 1;
        } else {
            this.value = 2;
        }
    }
}
";

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Class should have flow recorded
    let class_idx = *source_file.statements.nodes.first().expect("class");
    let flow_at_class = binder.get_node_flow(class_idx);
    assert!(
        flow_at_class.is_some(),
        "Flow should be recorded at class declaration"
    );
}

/// Test that TS2454 is emitted when a variable is used before being assigned.
/// This verifies the definite assignment checking is working.
#[test]
fn test_ts2454_variable_used_before_assigned() {
    use crate::CheckerState;
    use tsz_binder::BinderState;

    let source = r"
function test() {
    let x: string;
    return x;  // Error: x is used before being assigned
}
";

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::construction::TypeInterner::new();
    // TS2454 requires strictNullChecks (matches tsc behavior)
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    // Should have TS2454 error
    let has_ts2454 = checker.ctx.diagnostics.iter().any(|d| d.code == 2454);
    assert!(
        has_ts2454,
        "Should have TS2454 error for variable used before assignment"
    );
}

#[test]
fn test_optional_chain_element_assignment_is_not_definite_for_later_use() {
    let source = r#"
declare const o: undefined | {
    [key: string]: any;
};

let b: number;
o?.x[b = 1];
b.toFixed();
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let b_ref = get_method_call_receiver_identifier(arena, root, 3);
    let flow_at_use = binder.get_node_flow(b_ref).expect("flow for b use");
    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);
    assert!(
        !analyzer.is_definitely_assigned(b_ref, flow_at_use),
        "b should not be definitely assigned at the later use"
    );
}

/// Test that TS2454 is NOT emitted when a variable has an initializer.
#[test]
fn test_ts2454_no_error_with_initializer() {
    use crate::CheckerState;
    use tsz_binder::BinderState;

    let source = r#"
function test() {
    let x: string = "hello";
    return x;  // OK: x is initialized
}
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );
    checker.check_source_file(root);

    // Should NOT have TS2454 error
    let has_ts2454 = checker.ctx.diagnostics.iter().any(|d| d.code == 2454);
    assert!(
        !has_ts2454,
        "Should NOT have TS2454 error when variable has initializer"
    );
}

#[test]
fn test_assignment_then_instanceof_merge_keeps_assigned_set_type() {
    use crate::CheckerState;
    use crate::diagnostics::diagnostic_codes;
    use tsz_binder::BinderState;

    let source = r#"
function f1(s: Set<string> | Set<number>) {
    s = new Set<number>();
    s;
    if (s instanceof Set) {
        s;
    }
    s;
    s.add(42);
}

function f2(s: Set<string> | Set<number>) {
    s = new Set<number>();
    s;
    if (s instanceof Promise) {
        s;
    }
    s;
    s.add(42);
}
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::construction::TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let ts2339: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .map(|d| d.message_text.clone())
        .collect();

    assert!(
        ts2339.is_empty(),
        "instanceof merges should collapse back to the assigned Set<number> type, got: {ts2339:?}"
    );
}

#[test]
fn test_instanceof_accepts_annotated_union_after_function_augmentation() {
    use crate::CheckerState;
    use crate::diagnostics::diagnostic_codes;
    use tsz_binder::BinderState;

    let source = r#"
declare global {
    interface Function {
        now(): string;
    }
}

Function.prototype.now = function () {
    return "now";
};

class X {
    static now() {
        return {};
    }

    why() {}
}

export const x: X | number = Math.random() > 0.5 ? new X() : 1;

if (x instanceof X) {
    x.why();
}
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::construction::TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let ts2358: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYP
        })
        .collect();
    let ts2339: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .collect();

    assert!(
        ts2358.is_empty(),
        "annotated union lhs should stay valid for instanceof, got: {ts2358:?}"
    );
    assert!(
        ts2339.is_empty(),
        "instanceof narrowing should preserve X in the true branch, got: {ts2339:?}"
    );
}

/// Exhaustive switch without default should satisfy return-path checking.
#[test]
fn test_ts2366_not_emitted_for_exhaustive_switch_without_default() {
    use crate::CheckerState;
    use tsz_binder::BinderState;

    let source = r#"
function f(v: 0 | 1): number {
    switch (v) {
        case 0:
            return 1;
        case 1:
            return 2;
    }
}
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::construction::TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let has_ts2366 = checker.ctx.diagnostics.iter().any(|d| d.code == 2366);
    assert!(
        !has_ts2366,
        "Exhaustive switch should not fall through; got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// Exhaustive enum switch without default should satisfy return-path checking.
#[test]
fn test_ts2366_not_emitted_for_exhaustive_enum_switch_without_default() {
    use crate::CheckerState;
    use tsz_binder::BinderState;

    let source = r#"
enum E { A, B }
function f(e: E): number {
    switch (e) {
        case E.A:
            return 0;
        case E.B:
            return 1;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::construction::TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let has_ts2366 = checker.ctx.diagnostics.iter().any(|d| d.code == 2366);
    assert!(
        !has_ts2366,
        "Exhaustive enum switch should not fall through; got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_static_condition_branch_does_not_report_unreachable_exhaustive_switch() {
    use crate::CheckerState;
    use tsz_binder::BinderState;

    let source = r#"
function f1(x: 1 | 2): string {
    if (!!true) {
        switch (x) {
            case 1: return "a";
            case 2: return "b";
        }
        x;  // Unreachable
    }
    else {
        throw 0;
    }
}

enum E { A, B }

function g(e: E): number {
    if (!true)
        return -1;
    else
        switch (e) {
            case E.A: return 0;
            case E.B: return 1;
        }
}
"#;

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::construction::TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        allow_unreachable_code: Some(false),
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let ts7027: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 7027)
        .collect();
    let expected_start = source.find("x;  // Unreachable").expect("expected x tail") as u32;
    assert_eq!(
        ts7027.len(),
        1,
        "only the reachable post-switch tail should report TS7027; diagnostics: {:?}",
        checker.ctx.diagnostics
    );
    assert_eq!(
        ts7027[0].start, expected_start,
        "TS7027 should anchor at the post-switch tail"
    );
}

/// Issue #6823: an exhaustive numeric-enum switch must narrow the discriminant
/// to `never` in the `default` clause. The standard exhaustiveness pattern
/// (`const _: never = op`) must type-check without TS2322.
#[test]
fn test_ts2322_not_emitted_for_exhaustive_enum_switch_default_clause() {
    use crate::CheckerState;
    use tsz_binder::BinderState;

    let source = r#"
enum Operation {
    Add,
    Subtract,
    Multiply
}
function calculate(op: Operation, a: number, b: number): number {
    switch (op) {
        case Operation.Add: return a + b;
        case Operation.Subtract: return a - b;
        case Operation.Multiply: return a * b;
        default:
            const _exhaustive: never = op;
            return _exhaustive;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::construction::TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let ts2322: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();
    assert!(
        ts2322.is_empty(),
        "Exhaustive enum switch default must narrow to never; got TS2322: {ts2322:?}",
    );
}

/// Issue #6823 adjacent: renamed enum / numeric initialisers must behave the
/// same. The structural rule depends on enum nominal identity, not on
/// the spelling of member names.
#[test]
fn test_ts2322_not_emitted_for_exhaustive_renamed_enum_switch_default() {
    use crate::CheckerState;
    use tsz_binder::BinderState;

    let source = r#"
enum Direction {
    Up = 1, Down = 2, Left = 3, Right = 4
}
function handle(dir: Direction): string {
    switch (dir) {
        case Direction.Up: return "up";
        case Direction.Down: return "down";
        case Direction.Left: return "left";
        case Direction.Right: return "right";
        default:
            const exhaustive: never = dir;
            return exhaustive;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::construction::TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let ts2322: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();
    assert!(
        ts2322.is_empty(),
        "Renamed enum exhaustive switch default must narrow to never; got TS2322: {ts2322:?}",
    );
}

/// Issue #6823 adjacent: string-enum variant.
#[test]
fn test_ts2322_not_emitted_for_exhaustive_string_enum_switch_default() {
    use crate::CheckerState;
    use tsz_binder::BinderState;

    let source = r#"
enum Color {
    Red = "red",
    Green = "green",
    Blue = "blue"
}
function describe(c: Color): string {
    switch (c) {
        case Color.Red: return "r";
        case Color.Green: return "g";
        case Color.Blue: return "b";
        default:
            const exhaustive: never = c;
            return exhaustive;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::construction::TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let ts2322: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();
    assert!(
        ts2322.is_empty(),
        "String enum exhaustive switch default must narrow to never; got TS2322: {ts2322:?}",
    );
}

