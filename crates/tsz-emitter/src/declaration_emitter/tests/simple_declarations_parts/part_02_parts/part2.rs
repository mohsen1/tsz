#[test]
fn test_var_array_initializer_with_property_assignment_emits_valid_array_type() {
    // Same regression as above but for property-style assignment
    // (`t.foo = 5`). Both element-access and property-access assignments
    // were triggering `collect_ts_late_bound_assignment_members`, which
    // in turn entered the broken `: {` write path.
    let output = emit_dts(
        r#"
var t = [1, 2, 3];
t.foo = 5;
"#,
    );
    assert!(
        output.contains("declare var t: number[];"),
        "Expected valid array type for var with property assignment, got: {output}"
    );
    assert!(
        !output.contains(": {\n    : "),
        "Did not expect partial broken object type in output: {output}"
    );
}

#[test]
fn test_const_array_initializer_with_index_assignment_emits_valid_array_type() {
    // Same as above for `const` declarations.
    let output = emit_dts(
        r#"
const t = [1, 2, 3];
t[0] = 5;
"#,
    );
    assert!(
        output.contains("declare const t"),
        "Expected valid array type for const with index assignment, got: {output}"
    );
    assert!(
        !output.contains(": {\n    : "),
        "Did not expect partial broken object type in output: {output}"
    );
}

#[test]
fn test_ts_late_bound_function_assignments_emit_namespace() {
    let source = r#"
export function foo() {}
foo.bar = 12;
const strMem = "strMemName";
foo[strMem] = "ok";
const dashStrMem = "dashed-str-mem";
foo[dashStrMem] = "ok";
const numMem = 42;
foo[numMem] = "ok";
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let func_idx = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            (node.kind == syntax_kind_ext::FUNCTION_DECLARATION).then_some(NodeIndex(idx as u32))
        })
        .expect("missing function declaration");
    let func_node = parser.arena.get(func_idx).expect("missing function node");
    let func = parser
        .arena
        .get_function(func_node)
        .expect("missing function data");
    let member_names: Vec<String> = emitter
        .collect_ts_late_bound_assignment_members(func.name)
        .into_iter()
        .map(|member| member.property_name_text)
        .collect();
    assert_eq!(
        member_names,
        vec!["bar", "strMemName", "\"dashed-str-mem\"", "42"],
        "Expected late-bound assignment collection to preserve declaration key text",
    );

    let output = emitter.emit(root);
    let expected = r#"export declare function foo(): void;
export declare namespace foo {
    var bar: number;
    var strMemName: string;
}"#;
    assert!(
        output.contains(expected),
        "Expected TS late-bound function assignments to emit a merged namespace: {output}"
    );
}

#[test]
fn test_mutable_generic_call_literal_result_widens_in_declaration_emit() {
    let source = r#"
function foo<T>(x: T) { return x; }
var x = foo(5);
"#;
    let (parser, root) = parse_test_source(source);
    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let get_var_decl = |stmt_idx: NodeIndex| {
        parser
            .arena
            .get(stmt_idx)
            .and_then(|node| parser.arena.get_variable(node))
            .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
            .and_then(|node| parser.arena.get_variable(node))
            .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
            .and_then(|node| parser.arena.get_variable_declaration(node))
            .expect("missing variable declaration")
    };
    let var_x_decl = get_var_decl(source_file.statements.nodes[1]);

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    let literal_five = tsz_solver::type_queries::create_number_literal_type(&interner, 5.0);
    type_cache
        .node_types
        .insert(var_x_decl.initializer.0, literal_five);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);
    assert!(
        output.contains("declare var x: number;"),
        "Expected mutable generic call literal result to widen in DTS: {output}"
    );
    assert!(
        !output.contains("declare var x: 5;"),
        "Did not expect mutable generic call literal result to stay narrow: {output}"
    );
}

#[test]
fn test_js_var_with_attached_jsdoc_preserves_mutable_declaration_kind() {
    let output = emit_js_dts(
        r#"
/** {@link https://example.test} */
var linked = true;

/** Plain docs */
var count = 1, label = "x";

var narrow = false;
"#,
    );

    assert!(
        output.contains("declare var linked: boolean;"),
        "Expected documented JS var boolean literal to widen as mutable: {output}"
    );
    assert!(
        output.contains("declare var count: number, label: string;"),
        "Expected documented JS var group to keep var and widen literals: {output}"
    );
    assert!(
        output.contains("declare const narrow: false;"),
        "Undocumented JS var promotion should stay unchanged: {output}"
    );
    assert!(
        !output.contains("declare const linked: true;"),
        "Did not expect attached JSDoc JS var to be promoted to const: {output}"
    );
}

#[test]
fn test_ts_late_bound_function_assignments_ignore_block_scoped_shadow() {
    let source = r#"
export function X() {}
if (Math.random()) {
  const X: { test?: any } = {};
  X.test = 1;
}

export function Y() {}
Y.test = "foo";
if (Math.random()) {
  const Y = function Y() {}
  Y.test = 42;
}
"#;

    let output = emit_dts_with_binding(source);
    let expected = r#"export declare function X(): void;
export declare function Y(): void;
export declare namespace Y {
    var test: string;
}"#;
    assert!(
        output.contains(expected),
        "Expected block-scoped shadow assignments to be ignored: {output}"
    );
}

#[test]
fn test_export_default_function_with_late_bound_assignment_emits_default_alias() {
    let source = r#"
export default function someFunc() {
    return "hello!";
}

someFunc.someProp = "yo";
"#;

    let output = emit_dts_with_usage_analysis(source);
    let expected = r#"declare function someFunc(): string;
declare namespace someFunc {
    var someProp: string;
}
export default someFunc;"#;
    assert!(
        output.contains(expected),
        "Expected default function expandos to emit through a merged namespace alias: {output}"
    );
}

#[test]
fn test_ts_late_bound_function_reserved_alias_avoids_existing_member_name() {
    let source = r#"
export function foo() {}
foo._a = 1;
foo.class = "hello";
"#;

    let output = emit_dts_with_usage_analysis(source);
    let expected = r#"export declare function foo(): void;
export declare namespace foo {
    export var _a: number;
    var _b: string;
    export { _b as class };
}"#;
    assert!(
        output.contains(expected),
        "Synthetic alias for reserved namespace members should skip real member names.\nOutput:\n{output}"
    );
}

#[test]
fn test_js_late_bound_function_reserved_alias_uses_keyword_name() {
    let source = r#"
function foo() {}
foo.null = true;

function bar() {}
bar.async = true;
bar.normal = false;

function baz() {}
baz.class = true;
baz.normal = false;
"#;

    let output = emit_js_dts_with_usage_analysis(source);
    let expected = r#"declare function foo(): void;
declare namespace foo {
    let _null: boolean;
    export { _null as null };
}
declare function bar(): void;
declare namespace bar {
    let async: boolean;
    let normal: boolean;
}
declare function baz(): void;
declare namespace baz {
    let _class: boolean;
    export { _class as class };
    let normal_1: boolean;
    export { normal_1 as normal };
}"#;
    assert!(
        output.contains(expected),
        "Expected JS reserved function expandos to use keyword aliases and avoid reused local names.\nOutput:\n{output}"
    );
}
