/// Test switch statement fallthrough and default clause narrowing.
///
/// NOTE: Currently ignored - switch clause fallthrough narrowing is not fully
/// implemented. The flow graph records fallthrough antecedents, but the
/// `SWITCH_CLAUSE` handler in `check_flow` doesn't correctly union types from
/// fallthrough paths.
#[test]
fn test_switch_fallthrough_and_default_narrowing() {
    let source = r#"
let x: "a" | "b" | "c";
switch (x) {
  case "a":
    x;
  case "b":
    x;
    break;
  default:
    x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let switch_idx = get_switch_statement(arena, root, 1);
    let ident_case_a = get_switch_clause_expression(arena, switch_idx, 0);
    let ident_case_b = get_switch_clause_expression(arena, switch_idx, 1);
    let ident_default = get_switch_clause_expression(arena, switch_idx, 2);

    let lit_a = types.literal_string("a");
    let lit_b = types.literal_string("b");
    let lit_c = types.literal_string("c");
    let union = types.union(vec![lit_a, lit_b, lit_c]);

    let flow_a = binder.get_node_flow(ident_case_a).expect("flow for case a");
    let narrowed_a = analyzer.get_flow_type(ident_case_a, union, flow_a);
    assert_eq!(narrowed_a, lit_a);

    let flow_b = binder.get_node_flow(ident_case_b).expect("flow for case b");
    let narrowed_b = analyzer.get_flow_type(ident_case_b, union, flow_b);
    let expected_b = types.union(vec![lit_a, lit_b]);
    assert_eq!(narrowed_b, expected_b);

    let flow_default = binder
        .get_node_flow(ident_default)
        .expect("flow for default");
    let narrowed_default = analyzer.get_flow_type(ident_default, union, flow_default);
    assert_eq!(narrowed_default, lit_c);
}

#[test]
fn test_switch_discriminant_narrowing() {
    let source = r#"
let x: { kind: "a" } | { kind: "b" };
switch (x.kind) {
  case "a":
    x;
    break;
  default:
    x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let kind_name = types.intern_string("kind");
    let lit_a = types.literal_string("a");
    let lit_b = types.literal_string("b");

    let member_a = types.object(vec![PropertyInfo {
        name: kind_name,
        type_id: lit_a,
        write_type: lit_a,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);
    let member_b = types.object(vec![PropertyInfo {
        name: kind_name,
        type_id: lit_b,
        write_type: lit_b,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let union = types.union(vec![member_a, member_b]);

    let switch_idx = get_switch_statement(arena, root, 1);
    let ident_case_a = get_switch_clause_expression(arena, switch_idx, 0);
    let ident_default = get_switch_clause_expression(arena, switch_idx, 1);

    let flow_case_a = binder.get_node_flow(ident_case_a).expect("flow for case a");
    let narrowed_case_a = analyzer.get_flow_type(ident_case_a, union, flow_case_a);
    assert_eq!(narrowed_case_a, member_a);

    let flow_default = binder
        .get_node_flow(ident_default)
        .expect("flow for default");
    let narrowed_default = analyzer.get_flow_type(ident_default, union, flow_default);
    assert_eq!(narrowed_default, member_b);
}

#[test]
fn test_switch_default_does_not_narrow_unrelated_reference() {
    let source = r#"
let x: "a" | "b";
let y: string | number;
switch (x) {
  case "a":
    y;
    break;
  default:
    y;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let switch_idx = get_switch_statement(arena, root, 2);
    let ident_default = get_switch_clause_expression(arena, switch_idx, 1);

    let string_or_number = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let flow_default = binder
        .get_node_flow(ident_default)
        .expect("flow for default");
    let narrowed_default = analyzer.get_flow_type(ident_default, string_or_number, flow_default);
    assert_eq!(narrowed_default, string_or_number);
}

#[test]
fn test_switch_true_duplicate_case_narrows_to_never() {
    let source = r#"
let shape: { kind: "circle", radius: number } | { kind: "square", sideLength: number };
switch (true) {
  case shape.kind === "circle":
    shape;
    break;
  case shape.kind === "circle":
    shape;
    break;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let kind_name = types.intern_string("kind");
    let radius_name = types.intern_string("radius");
    let side_name = types.intern_string("sideLength");
    let lit_circle = types.literal_string("circle");
    let lit_square = types.literal_string("square");

    let circle = types.object(vec![
        PropertyInfo {
            name: kind_name,
            type_id: lit_circle,
            write_type: lit_circle,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: radius_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 1,
            is_string_named: false,
        },
    ]);
    let square = types.object(vec![
        PropertyInfo {
            name: kind_name,
            type_id: lit_square,
            write_type: lit_square,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: side_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 1,
            is_string_named: false,
        },
    ]);
    let union = types.union(vec![circle, square]);

    let switch_idx = get_switch_statement(arena, root, 1);
    let first_case_expr = get_switch_clause_expression(arena, switch_idx, 0);
    let second_case_expr = get_switch_clause_expression(arena, switch_idx, 1);

    let flow_first = binder
        .get_node_flow(first_case_expr)
        .expect("flow for case 1");
    let narrowed_first = analyzer.get_flow_type(first_case_expr, union, flow_first);
    assert_eq!(narrowed_first, circle);

    let flow_second = binder
        .get_node_flow(second_case_expr)
        .expect("flow for case 2");
    let narrowed_second = analyzer.get_flow_type(second_case_expr, union, flow_second);
    assert_eq!(narrowed_second, TypeId::NEVER);
}

#[test]
fn test_switch_true_case_uses_previous_false_constraints() {
    let source = r#"
let x: 1 | 2 | "a";
switch (true) {
  case x === 1:
    x;
    break;
  case typeof x === "number":
    x;
    break;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let lit_one = types.literal_number(1.0);
    let lit_two = types.literal_number(2.0);
    let lit_a = types.literal_string("a");
    let union = types.union(vec![lit_one, lit_two, lit_a]);

    let switch_idx = get_switch_statement(arena, root, 1);
    let first_case_expr = get_switch_clause_expression(arena, switch_idx, 0);
    let second_case_expr = get_switch_clause_expression(arena, switch_idx, 1);

    let flow_first = binder
        .get_node_flow(first_case_expr)
        .expect("flow for case 1");
    let narrowed_first = analyzer.get_flow_type(first_case_expr, union, flow_first);
    assert_eq!(narrowed_first, lit_one);

    let flow_second = binder
        .get_node_flow(second_case_expr)
        .expect("flow for case 2");
    let narrowed_second = analyzer.get_flow_type(second_case_expr, union, flow_second);
    assert_eq!(narrowed_second, lit_two);
}

#[test]
fn test_switch_true_fallthrough_clause_types() {
    let source = r#"
let x:
  | { kind: "a"; aProps: string }
  | { kind: "b"; bProps: string }
  | { kind: "c"; cProps: string };
switch (true) {
  default:
    const never: never = x;
  case x.kind === "a":
    x;
    // fallthrough
  case x.kind === "b":
    x;
    // fallthrough
  case x.kind === "c":
    x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let kind_name = types.intern_string("kind");
    let a_props_name = types.intern_string("aProps");
    let b_props_name = types.intern_string("bProps");
    let c_props_name = types.intern_string("cProps");
    let lit_a = types.literal_string("a");
    let lit_b = types.literal_string("b");
    let lit_c = types.literal_string("c");

    let member_a = types.object(vec![
        PropertyInfo {
            name: kind_name,
            type_id: lit_a,
            write_type: lit_a,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: a_props_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 1,
            is_string_named: false,
        },
    ]);

    let member_b = types.object(vec![
        PropertyInfo {
            name: kind_name,
            type_id: lit_b,
            write_type: lit_b,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: b_props_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 1,
            is_string_named: false,
        },
    ]);

    let member_c = types.object(vec![
        PropertyInfo {
            name: kind_name,
            type_id: lit_c,
            write_type: lit_c,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: c_props_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 1,
            is_string_named: false,
        },
    ]);

    let union = types.union(vec![member_a, member_b, member_c]);
    let switch_idx = get_switch_statement(arena, root, 1);
    let case_b_expr = get_switch_clause_expression(arena, switch_idx, 2);
    let case_c_expr = get_switch_clause_expression(arena, switch_idx, 3);

    let flow_b = binder.get_node_flow(case_b_expr).expect("flow for case b");
    let narrowed_b = analyzer.get_flow_type(case_b_expr, union, flow_b);
    let expected_b = types.union(vec![member_a, member_b]);
    assert_eq!(narrowed_b, expected_b);

    let flow_c = binder.get_node_flow(case_c_expr).expect("flow for case c");
    let narrowed_c = analyzer.get_flow_type(case_c_expr, union, flow_c);
    let expected_c = types.union(vec![member_a, member_b, member_c]);
    assert_eq!(narrowed_c, expected_c);
}

#[test]
fn test_instanceof_narrows_to_object_union_members() {
    let source = r"
let x: string | { a: number };
if (x instanceof Foo) {
  x;
} else {
  x;
}
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let prop_a = types.intern_string("a");
    let obj_type = types.object(vec![PropertyInfo {
        name: prop_a,
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
    }]);
    let union = types.union(vec![TypeId::STRING, obj_type]);

    let ident_then = get_if_branch_expression(arena, root, 1, true);
    let ident_else = get_if_branch_expression(arena, root, 1, false);

    let flow_then = binder.get_node_flow(ident_then).expect("flow then");
    let flow_else = binder.get_node_flow(ident_else).expect("flow else");

    let narrowed_then = analyzer.get_flow_type(ident_then, union, flow_then);
    assert_eq!(narrowed_then, obj_type);

    let narrowed_else = analyzer.get_flow_type(ident_else, union, flow_else);
    assert_eq!(narrowed_else, union);
}

#[test]
fn test_in_operator_narrows_required_property() {
    let source = r#"
let x: { a: number } | { b: string };
if ("a" in x) {
  x;
} else {
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let prop_a = types.intern_string("a");
    let prop_b = types.intern_string("b");

    let type_a = types.object(vec![PropertyInfo {
        name: prop_a,
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
    }]);
    let type_b = types.object(vec![PropertyInfo {
        name: prop_b,
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
    }]);
    let union = types.union(vec![type_a, type_b]);

    let ident_then = get_if_branch_expression(arena, root, 1, true);
    let ident_else = get_if_branch_expression(arena, root, 1, false);

    let flow_then = binder.get_node_flow(ident_then).expect("flow then");
    let flow_else = binder.get_node_flow(ident_else).expect("flow else");

    let narrowed_then = analyzer.get_flow_type(ident_then, union, flow_then);
    assert_eq!(narrowed_then, type_a);

    let narrowed_else = analyzer.get_flow_type(ident_else, union, flow_else);
    assert_eq!(narrowed_else, type_b);
}

#[test]
fn test_in_operator_optional_property_keeps_false_branch_union() {
    let source = r#"
let x: { a?: number } | { b: string };
if ("a" in x) {
  x;
} else {
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let prop_a = types.intern_string("a");
    let prop_b = types.intern_string("b");

    let type_a = types.object(vec![PropertyInfo {
        name: prop_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);
    let type_b = types.object(vec![PropertyInfo {
        name: prop_b,
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
    }]);
    let union = types.union(vec![type_a, type_b]);

    let ident_then = get_if_branch_expression(arena, root, 1, true);
    let ident_else = get_if_branch_expression(arena, root, 1, false);

    let flow_then = binder.get_node_flow(ident_then).expect("flow then");
    let flow_else = binder.get_node_flow(ident_else).expect("flow else");

    let narrowed_then = analyzer.get_flow_type(ident_then, union, flow_then);
    assert_eq!(narrowed_then, type_a);

    let narrowed_else = analyzer.get_flow_type(ident_else, union, flow_else);
    assert_eq!(narrowed_else, union);
}

#[test]
fn test_in_operator_private_identifier_narrows_required_property() {
    let source = r##"
let x: { "#a": number } | { b: string };
if (#a in x) {
  x;
} else {
  x;
}
"##;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let prop_a = types.intern_string("#a");
    let prop_b = types.intern_string("b");

    let type_a = types.object(vec![PropertyInfo {
        name: prop_a,
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
    }]);
    let type_b = types.object(vec![PropertyInfo {
        name: prop_b,
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
    }]);
    let union = types.union(vec![type_a, type_b]);

    let ident_then = get_if_branch_expression(arena, root, 1, true);
    let ident_else = get_if_branch_expression(arena, root, 1, false);

    let flow_then = binder.get_node_flow(ident_then).expect("flow then");
    let flow_else = binder.get_node_flow(ident_else).expect("flow else");

    let narrowed_then = analyzer.get_flow_type(ident_then, union, flow_then);
    assert_eq!(narrowed_then, type_a);

    let narrowed_else = analyzer.get_flow_type(ident_else, union, flow_else);
    assert_eq!(narrowed_else, type_b);
}

