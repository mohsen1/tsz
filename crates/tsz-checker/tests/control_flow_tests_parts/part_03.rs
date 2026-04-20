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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

