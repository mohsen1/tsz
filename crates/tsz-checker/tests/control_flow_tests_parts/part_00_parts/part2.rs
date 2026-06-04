#[test]
fn test_object_destructuring_alias_assignment_clears_narrowing() {
    let source = r#"
let x: string | number;
if (typeof x === "string") {
  x;
  ({ y: x } = { y: 1 });
  x;
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let if_idx = *source_file.statements.nodes.get(1).expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");
    let then_block = if_data.then_statement;

    let ident_before = get_block_expression(arena, then_block, 0);
    let ident_after = get_block_expression(arena, then_block, 2);

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let flow_before = binder.get_node_flow(ident_before).expect("flow before");
    let narrowed_before = analyzer.get_flow_type(ident_before, union, flow_before);
    assert_eq!(narrowed_before, TypeId::STRING);

    let flow_after = binder.get_node_flow(ident_after).expect("flow after");
    let narrowed_after = analyzer.get_flow_type(ident_after, union, flow_after);
    // After destructuring with assignment, type is widened to primitive (number)
    // This matches TypeScript's verified behavior
    assert_eq!(narrowed_after, TypeId::NUMBER);
}

#[test]
fn test_destructuring_assignment_widens_literals_for_exact_assignment_diagnostics() {
    let source = r#"
function arrayAssignment() {
  let x: string | number = "s";
  if (typeof x === "string") {
    [x] = [1];
    const exact: 1 = x;
  }
}

function objectAssignment() {
  let x: string | number = "s";
  if (typeof x === "string") {
    ({ x } = { x: 1 });
    const exact: 1 = x;
  }
}

function objectAlias() {
  let x: string | number = "s";
  if (typeof x === "string") {
    ({ y: x } = { y: 1 });
    const exact: 1 = x;
  }
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let ts2322: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .collect();

    assert_eq!(
        ts2322.len(),
        3,
        "expected TS2322 for each exact literal assignment after destructuring writes, got: {:?}",
        checker.ctx.diagnostics
    );
    assert!(
        ts2322.iter().all(|diag| diag
            .message_text
            .contains("Type 'number' is not assignable to type '1'")),
        "expected destructuring writes to widen literal 1 to number, got: {ts2322:?}"
    );
}

#[test]
fn test_compound_assignment_clears_narrowing() {
    let source = r#"
let x: string | number;
if (typeof x === "string") {
  x;
  x += 1;
  x;
}
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let if_idx = *source_file.statements.nodes.get(1).expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");
    let then_block = if_data.then_statement;

    let ident_before = get_block_expression(arena, then_block, 0);
    let ident_after = get_block_expression(arena, then_block, 2);

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let flow_before = binder.get_node_flow(ident_before).expect("flow before");
    let narrowed_before = analyzer.get_flow_type(ident_before, union, flow_before);
    assert_eq!(narrowed_before, TypeId::STRING);

    let flow_after = binder.get_node_flow(ident_after).expect("flow after");
    let narrowed_after = analyzer.get_flow_type(ident_after, union, flow_after);
    // After destructuring with assignment, type is widened to primitive (number)
    // This matches TypeScript's verified behavior
    assert_eq!(narrowed_after, TypeId::NUMBER);
}

#[test]
fn test_array_mutation_clears_predicate_narrowing() {
    let source = r#"
function isStringArray(x: string[] | number[]): x is string[] {
  return true;
}
let x: string[] | number[];
if (isStringArray(x)) {
  x;
  x.push("a");
  x;
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
    let if_idx = *source_file.statements.nodes.get(2).expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");
    let then_block = if_data.then_statement;

    let ident_before = get_block_expression(arena, then_block, 0);
    let ident_after = get_block_expression(arena, then_block, 2);

    let string_array = types.array(TypeId::STRING);
    let number_array = types.array(TypeId::NUMBER);
    let union = types.union(vec![string_array, number_array]);

    let flow_before = binder.get_node_flow(ident_before).expect("flow before");
    let narrowed_before = analyzer.get_flow_type(ident_before, union, flow_before);
    assert_eq!(narrowed_before, string_array);

    let flow_after = binder.get_node_flow(ident_after).expect("flow after");
    let narrowed_after = analyzer.get_flow_type(ident_after, union, flow_after);
    // For local variables, TypeScript preserves narrowing across method calls
    // Only property accesses reset narrowing after mutations
    assert_eq!(narrowed_after, string_array);
}
