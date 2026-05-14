use super::*;
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

// =============================================================================
// 5. Type Formatting
// =============================================================================

#[test]
fn test_union_type_in_declaration() {
    let output = emit_dts("export type Result = string | number | boolean;");
    assert!(
        output.contains("string | number | boolean"),
        "Expected union type: {output}"
    );
}

#[test]
fn test_type_printer_preserves_union_display_origin() {
    let interner = TypeInterner::new();
    let object_member = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let array_member = interner.array(TypeId::NUMBER);
    let union = interner.union(vec![array_member, object_member]);
    interner.replace_union_origin_for_display(union, vec![array_member, object_member]);

    let printed = crate::emitter::type_printer::TypePrinter::new(&interner).print_type(union);

    assert_eq!(printed, "number[] | { x: number }");
}

#[test]
fn test_type_printer_expands_object_union_missing_properties() {
    let interner = TypeInterner::new();
    let x = interner.intern_string("x");
    let y = interner.intern_string("y");
    let err = interner.intern_string("err");
    let first = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![PropertyInfo::new(x, TypeId::NUMBER)],
        string_index: None,
        number_index: None,
        symbol: None,
    });
    let second = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![
            PropertyInfo::new(x, TypeId::NUMBER),
            PropertyInfo::new(y, TypeId::NUMBER),
        ],
        string_index: None,
        number_index: None,
        symbol: None,
    });
    let third = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![
            PropertyInfo::new(x, TypeId::NUMBER),
            PropertyInfo::new(err, TypeId::BOOLEAN),
        ],
        string_index: None,
        number_index: None,
        symbol: None,
    });
    let union = interner.union_preserve_members(vec![first, second, third]);

    let printed = crate::emitter::type_printer::TypePrinter::new(&interner)
        .with_indent_level(0)
        .print_type(union);

    let expected = r#"{
    x: number;
    y?: undefined;
    err?: undefined;
} | {
    x: number;
    y: number;
    err?: undefined;
} | {
    x: number;
    err: boolean;
    y?: undefined;
}"#;
    assert_eq!(printed, expected);
}

#[test]
fn test_type_printer_prints_named_unique_symbol_as_typeof() {
    let source = "export const x = Symbol();\nexport const y = Symbol();\n";
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let x_sym = binder.file_locals.get("x").expect("missing x symbol");
    let y_sym = binder.file_locals.get("y").expect("missing y symbol");
    let interner = TypeInterner::new();
    let x_type = interner.unique_symbol(SymbolRef(x_sym.0));
    let y_type = interner.unique_symbol(SymbolRef(y_sym.0));
    let union = interner.union_preserve_members(vec![x_type, y_type]);

    let printed = crate::emitter::type_printer::TypePrinter::new(&interner)
        .with_symbols(&binder.symbols)
        .print_type(union);

    assert_eq!(printed, "typeof x | typeof y");
}

#[test]
fn test_intersection_type_in_declaration() {
    let output = emit_dts("export type Combined = { a: number } & { b: string };");
    assert!(output.contains("&"), "Expected intersection type: {output}");
}

#[test]
fn test_function_type_in_declaration() {
    let output = emit_dts("export type Callback = (x: number, y: string) => void;");
    assert!(
        output.contains("(x: number, y: string) => void"),
        "Expected function type: {output}"
    );
}

#[test]
fn test_global_class_name_shadowed_by_type_param_uses_global_this() {
    let source = "class A {}";
    let (parser, _root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, _root);

    let class_sym = binder.file_locals.get("A").expect("missing class symbol");
    let interner = TypeInterner::new();
    let def_id = DefId(9103);
    let type_id = interner.lazy(def_id);
    let mut type_cache = TypeCacheView::default();
    type_cache.def_to_symbol.insert(def_id, class_sym);
    let a_param = interner.intern_string("A");

    let printed = crate::emitter::type_printer::TypePrinter::new(&interner)
        .with_symbols(&binder.symbols)
        .with_type_cache(&type_cache)
        .with_outer_type_params(vec![a_param])
        .print_type(type_id);

    assert_eq!(printed, "globalThis.A");
}

#[test]
fn test_default_export_type_text_retains_local_type_alias_dependency() {
    let source = r#"
type Experiment<Name> = {
    name: Name;
};
declare const createExperiment: <Name extends string>(
    options: Experiment<Name>
) => Experiment<Name>;
"#;
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_used_symbols(FxHashMap::default());

    emitter.retain_local_type_names_for_public_api(r#"Experiment<"foo">"#);

    let experiment_sym = binder
        .file_locals
        .get("Experiment")
        .expect("missing Experiment symbol");
    let create_sym = binder
        .file_locals
        .get("createExperiment")
        .expect("missing createExperiment symbol");
    let used = emitter.used_symbols.as_ref().expect("missing used symbols");
    assert!(
        used.contains_key(&experiment_sym),
        "Expected type text to retain local type alias dependency"
    );
    assert!(
        !used.contains_key(&create_sym),
        "Did not expect type text retention to keep local value helpers"
    );
}

#[test]
fn test_node_modules_types_entry_uses_bare_package_specifier() {
    let root =
        std::env::temp_dir().join(format!("tsz-emitter-package-entry-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let package_root = root.join("node_modules/@babel/parser");
    let typings_dir = package_root.join("typings");
    std::fs::create_dir_all(&typings_dir).expect("create package dirs");
    std::fs::write(
        package_root.join("package.json"),
        r#"{"name":"@babel/parser","types":"./typings/babel-parser.d.ts"}"#,
    )
    .expect("write package json");
    std::fs::write(
        typings_dir.join("babel-parser.d.ts"),
        "export declare class PluginConfig {}",
    )
    .expect("write declaration");

    let arena = tsz_parser::parser::node::NodeArena::default();
    let emitter = DeclarationEmitter::new(&arena);
    let current_path = root.join("packages/compiler-sfc/src/index.ts");
    let source_path = typings_dir.join("babel-parser.d.ts");
    let specifier = emitter
        .package_specifier_for_node_modules_path(
            current_path.to_str().expect("current path utf8"),
            source_path.to_str().expect("source path utf8"),
        )
        .expect("package specifier");

    assert_eq!(specifier, "@babel/parser");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn test_static_super_method_call_preserves_return_type() {
    let output = emit_dts_with_binding(
        r#"
class C1 {
    protected static sx: number;
    protected static sf() {
        return this.sx;
    }
}
class C2 extends C1 {
    protected static sf() {
        return super.sf() + this.sx;
    }
}
class C3 extends C2 {
    static sf() {
        return super.sf();
    }
}
"#,
    );

    assert!(
        output.contains("protected static sf(): number;"),
        "Expected protected static super method return to be number: {output}"
    );
    assert!(
        output.contains("static sf(): number;"),
        "Expected public static super method return to be number: {output}"
    );
}

#[test]
fn test_inferred_generic_function_type_omits_synthesized_optional_undefined() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let f_name = interner.intern_string("f");
    let value_name = interner.intern_string("value");
    let t_param = tsz_solver::TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let u_param = tsz_solver::TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_param);
    let u_type = interner.type_param(u_param);
    let callback = interner.function(FunctionShape::new(
        vec![ParamInfo::optional(
            value_name,
            interner.union(vec![t_type, TypeId::UNDEFINED]),
        )],
        u_type,
    ));
    let outer = interner.function(FunctionShape {
        type_params: vec![t_param, u_param],
        params: vec![ParamInfo::required(f_name, callback)],
        this_type: None,
        return_type: u_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let printed = crate::emitter::type_printer::TypePrinter::new(&interner).print_type(outer);

    assert_eq!(printed, "<T, U>(f: (value?: T) => U) => U");
}

#[test]
fn test_returned_local_generic_function_renames_shadowed_type_param() {
    let source = r#"
const foo = <T,>(x: T) => {
    const inner = <T,>(y: T) => [x, y] as const;
    return inner;
};
"#;
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let var_stmt_idx = source_file.statements.nodes[0];
    let outer_func = parser
        .arena
        .get(var_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .and_then(|decl| parser.arena.get(decl.initializer))
        .and_then(|node| parser.arena.get_function(node))
        .expect("missing outer function");
    let interner = TypeInterner::new();
    let type_cache = TypeCacheView::default();
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let returned_identifier = emitter
        .function_body_unique_return_identifier(outer_func.body)
        .expect("missing returned identifier");
    let type_text = emitter
        .function_return_identifier_type_text(outer_func, returned_identifier)
        .expect("missing returned function type");

    assert_eq!(type_text, "<T_1>(y: T_1) => readonly [T, T_1]");
}

#[test]
fn test_direct_returned_function_expression_preserves_outer_alias() {
    let source = r#"
export function needsRenameForShadowing<T>() {
  type A = T;
  return function O<T>(t: A, t2: T) {
  }
}
"#;
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let func = parser
        .arena
        .nodes
        .iter()
        .find_map(|node| {
            parser.arena.get_function(node).filter(|func| {
                parser.arena.get_identifier_text(func.name) == Some("needsRenameForShadowing")
            })
        })
        .expect("missing function");
    let interner = TypeInterner::new();
    let type_cache = TypeCacheView::default();
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let type_text = emitter
        .direct_returned_function_expression_type_text(func)
        .expect("missing returned function type");

    assert_eq!(type_text, "<T_1>(t: T, t2: T_1) => void");
}

#[test]
fn test_direct_returned_function_expression_expands_rest_tuple_aliases() {
    let output = emit_dts(
        r#"
function f() {
    type A = [a: string];
    type B = [b: string];
    type C = [...A, ...A, ...B];

    return function fn(...args: C) { }
}
"#,
    );

    assert!(
        output.contains("declare function f(): (a: string, a_1: string, b: string) => void;"),
        "expected rest tuple alias to expand into positional parameters: {output}"
    );
}

#[test]
fn test_direct_returned_function_expression_rest_tuple_alias_avoids_existing_param_name_collision()
{
    let output = emit_dts(
        r#"
function f() {
    type T = [a: string, b: string];

    return function fn(a: number, ...args: T) { }
}
"#,
    );

    assert!(
        output.contains("declare function f(): (a: number, a_1: string, b: string) => void;"),
        "expected rest tuple expansion to avoid collisions with existing parameter names: {output}"
    );
}

#[test]
fn test_direct_returned_function_expression_expands_unlabeled_rest_tuple_alias_elements() {
    let output = emit_dts(
        r#"
function f() {
    type T = [string, number];

    return function fn(...args: T) { }
}
"#,
    );

    assert!(
        output.contains("declare function f(): (arg0: string, arg1: number) => void;"),
        "expected unlabeled rest tuple alias elements to expand into synthesized positional parameters: {output}"
    );
}

#[test]
fn test_returned_class_expression_preserves_extends_type_parameter() {
    let source = r#"
export type Constructor<T = {}> = new (...args: any[]) => T;

export function Timestamped<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        timestamp = Date.now();
    };
}
"#;
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let source_file = parser
        .arena
        .get(root)
        .and_then(|node| parser.arena.get_source_file(node))
        .expect("missing source file");
    let func = source_file
        .statements
        .nodes
        .iter()
        .find_map(|&stmt_idx| {
            let stmt_node = parser.arena.get(stmt_idx)?;
            if let Some(func) = parser.arena.get_function(stmt_node) {
                return Some(func);
            }
            let export = parser.arena.get_export_decl(stmt_node)?;
            let clause_node = parser.arena.get(export.export_clause)?;
            parser.arena.get_function(clause_node)
        })
        .expect("missing function");
    let return_expr = parser
        .arena
        .get(func.body)
        .and_then(|node| parser.arena.get_block(node))
        .and_then(|block| parser.arena.get(block.statements.nodes[0]))
        .and_then(|node| parser.arena.get_return_statement(node))
        .and_then(|ret| ret.expression.is_some().then_some(ret.expression))
        .expect("missing returned class expression");

    let interner = TypeInterner::new();
    let type_cache = TypeCacheView::default();
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let type_text = emitter
        .preferred_expression_type_text(return_expr)
        .expect("missing class expression type text");
    assert!(
        type_text.contains("} & TBase"),
        "Expected returned class expression to preserve extends type parameter: {type_text}"
    );
    assert!(
        type_text.contains("new (...args: any[]):"),
        "Expected mixin constructor to forward base constructor args: {type_text}"
    );
}

#[test]
fn test_returned_local_call_preserves_typeof_parameter_return() {
    let output = emit_dts_with_binding(
        r#"
export const g = (v: "outer") => {
    const f = (v: "inner") => () => null! as typeof v;
    const r = f(null!);
    return r;
};
"#,
    );

    assert!(
        output.contains(r#"export declare const g: (v: "outer") => () => "inner";"#),
        "Expected returned local call to preserve typeof parameter return: {output}"
    );
}

#[test]
fn test_function_variable_type_preserves_inline_parameter_comments() {
    let output = emit_dts(
        r#"
const fooFunc = function (/** foo */ value: string): string {
    return value;
};
const lambdaFoo = (/** left */ left: number, /** right */ right: number): number => left + right;
"#,
    );

    assert!(
        output.contains("declare const fooFunc: (/** foo */ value: string) => string;"),
        "Expected function expression parameter comment to be preserved: {output}"
    );
    assert!(
        output.contains(
            "declare const lambdaFoo: (/** left */ left: number, /** right */ right: number) => number;"
        ),
        "Expected arrow function parameter comments to be preserved: {output}"
    );
}

#[test]
fn test_js_function_declaration_prefers_returned_callable_object_type() {
    let source = r#"
function test(fn) {
    const composed = function (...args) { };
    return composed;
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let func_idx = source_file.statements.nodes[0];
    let func_node = parser.arena.get(func_idx).expect("missing function node");
    let func = parser
        .arena
        .get_function(func_node)
        .expect("missing function data");
    let body_node = parser.arena.get(func.body).expect("missing body node");
    let body = parser
        .arena
        .get_block(body_node)
        .expect("missing function body");
    let composed_stmt_idx = body.statements.nodes[0];
    let composed_decl = parser
        .arena
        .get(composed_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing composed declaration");
    let return_stmt_idx = body.statements.nodes[1];
    let return_stmt_node = parser
        .arena
        .get(return_stmt_idx)
        .expect("missing return node");
    let return_stmt = parser
        .arena
        .get_return_statement(return_stmt_node)
        .expect("missing return statement");

    let interner = TypeInterner::new();
    let fn_atom = interner.intern_string("fn");
    let args_atom = interner.intern_string("args");
    let name_atom = interner.intern_string("name");
    let any_array = interner.array(TypeId::ANY);
    let plain_return_type = interner.function(FunctionShape::new(
        vec![ParamInfo::rest(args_atom, any_array)],
        TypeId::VOID,
    ));
    let callable_return_type = interner.callable(CallableShape {
        call_signatures: vec![CallSignature::new(
            vec![ParamInfo::rest(args_atom, any_array)],
            TypeId::VOID,
        )],
        properties: vec![PropertyInfo::readonly(name_atom, TypeId::STRING)],
        ..Default::default()
    });
    let test_type = interner.function(FunctionShape::new(
        vec![ParamInfo::required(fn_atom, TypeId::ANY)],
        plain_return_type,
    ));

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(func_idx.0, test_type);
    type_cache.node_types.insert(func.name.0, test_type);
    type_cache
        .node_types
        .insert(composed_decl.name.0, callable_return_type);
    type_cache
        .node_types
        .insert(return_stmt.expression.0, callable_return_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function test(fn: any): {"),
        "Expected JS function signature: {output}"
    );
    assert!(
        output.contains("(...args: any[]): void;"),
        "Expected callable return signature to be preserved: {output}"
    );
    assert!(
        output.contains("readonly name: string;"),
        "Expected returned callable property to be preserved: {output}"
    );
}

#[test]
fn test_any_dataview_new_expression_falls_back_to_generic_type() {
    let source = "const dataView = new DataView(new ArrayBuffer(80));";
    let (parser, root) = parse_test_source(source);

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file data");
    };
    let var_stmt_idx = source_file.statements.nodes[0];
    let var_decl = parser
        .arena
        .get(var_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing dataView declaration");

    let interner = TypeInterner::new();
    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(var_decl.name.0, TypeId::ANY);

    let binder = BinderState::new();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare const dataView: DataView<ArrayBuffer>;"),
        "Expected DataView constructor fallback type: {output}"
    );
}

#[test]
fn test_static_method_property_access_emits_typeof() {
    let source = r#"
class C {
    static s1: number;
    static s2(b: number) {
        return C.s1 + b;
    }
}
var methodValue = C.s2;
"#;
    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let class_idx = source_file.statements.nodes[0];
    let class_decl = parser
        .arena
        .get(class_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class declaration");
    let var_stmt_idx = source_file.statements.nodes[1];
    let var_decl = parser
        .arena
        .get(var_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing variable declaration");
    let access = parser
        .arena
        .get(var_decl.initializer)
        .and_then(|node| parser.arena.get_access_expr(node))
        .expect("missing property access initializer");

    let interner = TypeInterner::new();
    let b_atom = interner.intern_string("b");
    let method_type = interner.function(FunctionShape::new(
        vec![ParamInfo::required(b_atom, TypeId::NUMBER)],
        TypeId::NUMBER,
    ));
    let constructor_type = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature::new(Vec::new(), TypeId::ANY)],
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: binder
            .get_node_symbol(class_decl.name)
            .or_else(|| binder.get_node_symbol(class_idx)),
        is_abstract: false,
    });

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(var_decl.name.0, method_type);
    type_cache
        .node_types
        .insert(access.expression.0, constructor_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare var methodValue: typeof C.s2;"),
        "Expected static method property access to emit typeof: {output}"
    );
}

#[test]
fn test_function_initializer_prefers_asserted_return_type_with_typeof_members() {
    let source = r#"
export const nImported = "nImported";
export const nNotImported = "nNotImported";
const nPrivate = "private";
export const o = (p1: typeof nImported, p2: typeof nNotImported, p3: typeof nPrivate) => null! as { foo: typeof nImported, bar: typeof nPrivate, baz: typeof nNotImported };
"#;

    let (parser, root) = parse_test_source(source);
    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let var_stmt = source_file
        .statements
        .nodes
        .iter()
        .filter_map(|&stmt_idx| {
            let stmt_node = parser.arena.get(stmt_idx)?;
            if let Some(var_stmt) = parser.arena.get_variable(stmt_node) {
                return Some(var_stmt);
            }
            let export = parser.arena.get_export_decl(stmt_node)?;
            let clause_node = parser.arena.get(export.export_clause)?;
            parser.arena.get_variable(clause_node)
        })
        .next_back()
        .expect("missing o variable statement");
    let var_decl = parser
        .arena
        .get(var_stmt.declarations.nodes[0])
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing o declaration");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let return_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![
            PropertyInfo::new(interner.intern_string("foo"), TypeId::STRING),
            PropertyInfo::new(interner.intern_string("bar"), TypeId::STRING),
            PropertyInfo::new(interner.intern_string("baz"), TypeId::STRING),
        ],
        string_index: None,
        number_index: None,
        symbol: None,
    });
    let function_type = interner.function(FunctionShape::new(Vec::new(), return_type));

    let mut type_cache = TypeCacheView::default();
    type_cache
        .node_types
        .insert(var_decl.initializer.0, function_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("foo: typeof nImported;"),
        "Expected asserted return type to preserve imported typeof member: {output}"
    );
    assert!(
        output.contains("bar: typeof nPrivate;"),
        "Expected asserted return type to preserve private typeof member: {output}"
    );
    assert!(
        output.contains("baz: typeof nNotImported;"),
        "Expected asserted return type to preserve non-imported typeof member: {output}"
    );
}

#[test]
fn test_const_call_initializer_does_not_collapse_to_literal_argument() {
    let source = r#"
type Box<T> = {
    get: () => T;
    set: (value: T) => void;
};
declare function box<T>(value: T): Box<T>;
const bn1 = box(0);
"#;
    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let alias_idx = source_file.statements.nodes[0];
    let alias = parser
        .arena
        .get(alias_idx)
        .and_then(|node| parser.arena.get_type_alias(node))
        .expect("missing Box alias");
    let var_stmt_idx = source_file.statements.nodes[2];
    let var_decl = parser
        .arena
        .get(var_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing variable declaration");

    let interner = TypeInterner::new();
    let box_def = tsz_solver::DefId(9002);
    let box_number = interner.application(interner.lazy(box_def), vec![TypeId::NUMBER]);

    let alias_sym = binder
        .get_node_symbol(alias.name)
        .or_else(|| binder.get_node_symbol(alias_idx))
        .expect("missing Box symbol");
    let mut type_cache = TypeCacheView::default();
    type_cache.def_to_symbol.insert(box_def, alias_sym);
    type_cache.node_types.insert(var_decl.name.0, box_number);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare const bn1: Box<number>;"),
        "Expected const call initializer to preserve resolved type: {output}"
    );
    assert!(
        !output.contains("declare const bn1 = 0;"),
        "Did not expect const call initializer to collapse to its literal argument: {output}"
    );
}

#[test]
fn test_inferred_function_array_preserves_new_expression_return_type() {
    let output = emit_dts_with_binding(
        r#"
class c {
    private p: string;
}

var y = [() => new c()];
"#,
    );

    assert!(
        output.contains("declare var y: (() => c)[];"),
        "Expected inferred function array to preserve class return type: {output}"
    );
    assert!(
        !output.contains("(() => any)[]"),
        "Did not expect arrow return type to fall back to any: {output}"
    );
}

#[test]
fn test_non_null_call_initializer_recovers_return_type() {
    let source = r#"
declare const fn: (() => string) | undefined;
const a = fn!();
"#;
    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let fn_stmt_idx = source_file.statements.nodes[0];
    let fn_decl = parser
        .arena
        .get(fn_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing fn declaration");
    let a_stmt_idx = source_file.statements.nodes[1];
    let a_decl = parser
        .arena
        .get(a_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing a declaration");
    let call = parser
        .arena
        .get(a_decl.initializer)
        .and_then(|node| parser.arena.get_call_expr(node))
        .expect("missing call initializer");
    let non_null = parser
        .arena
        .get(call.expression)
        .and_then(|node| parser.arena.get_unary_expr_ex(node))
        .expect("missing non-null callee");
    let interner = TypeInterner::new();
    let callable = interner.function(FunctionShape::new(Vec::new(), TypeId::STRING));

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(fn_decl.name.0, callable);
    type_cache
        .node_types
        .insert(non_null.expression.0, callable);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare const a: string;"),
        "Expected non-null call initializer to recover the inner callable return type: {output}"
    );
}

#[test]
fn test_dataview_new_expression_falls_back_without_type_cache() {
    let output = emit_dts("const dataView = new DataView(new ArrayBuffer(80));");
    assert!(
        output.contains("declare const dataView: DataView<ArrayBuffer>;"),
        "Expected DataView constructor fallback without type cache: {output}"
    );
}

#[test]
fn test_array_type_in_declaration() {
    let output = emit_dts("export type Numbers = number[];");
    assert!(output.contains("number[]"), "Expected array type: {output}");
}

#[test]
fn test_tuple_type_in_declaration() {
    let output = emit_dts("export type Pair = [string, number];");
    assert!(
        output.contains("[string, number]"),
        "Expected tuple type: {output}"
    );
}

#[test]
fn test_multiline_tuple_type_argument_preserves_tuple_breaks() {
    let output = emit_dts(
        r#"
export type Point = TypedObject<[
    {
        name: "x";
        type: "f64";
    },
    {
        name: "y";
        type: "f64";
    }
]>;
"#,
    );
    assert!(
        output.contains(
            "export type Point = TypedObject<[\n    {\n        name: \"x\";\n        type: \"f64\";\n    },\n    {\n        name: \"y\";\n        type: \"f64\";\n    }\n]>;"
        ),
        "Expected multiline tuple type argument to preserve tuple breaks: {output}"
    );
}

#[test]
fn test_single_line_tuple_type_argument_stays_compact() {
    let output = emit_dts("export type PairBox = Box<[string, number]>;");
    assert!(
        output.contains("export type PairBox = Box<[string, number]>;"),
        "Expected single-line tuple type argument to stay compact: {output}"
    );
}

#[test]
fn test_conditional_type_in_declaration() {
    let output = emit_dts("export type IsString<T> = T extends string ? true : false;");
    assert!(
        output.contains("T extends string ? true : false"),
        "Expected conditional type: {output}"
    );
}

#[test]
fn test_mapped_type_in_declaration() {
    let output = emit_dts("export type Readonly<T> = { readonly [K in keyof T]: T[K] };");
    assert!(
        output.contains("readonly"),
        "Expected mapped type with readonly: {output}"
    );
    assert!(
        output.contains("keyof T"),
        "Expected keyof in mapped type: {output}"
    );
}

#[test]
fn test_indexed_access_type() {
    let output = emit_dts("export type Name = Person['name'];");
    assert!(
        output.contains("Person['name']"),
        "Expected indexed access type: {output}"
    );
}

#[test]
fn test_indexed_access_variadic_tuple_breaks_multiline() {
    let output = emit_dts(
        r#"
type NTuple<N extends number, Tup extends unknown[] = []> =
    Tup['length'] extends N ? Tup : NTuple<N, [...Tup, unknown]>;

export type Add<A extends number, B extends number> =
    [...NTuple<A>, ...NTuple<B>]['length'];
"#,
    );
    assert!(
        output.contains("type Add<A extends number, B extends number> = [\n    ...NTuple<A>,\n    ...NTuple<B>\n]['length'];"),
        "Expected variadic tuple indexed access to break across lines: {output}"
    );
}

#[test]
fn test_function_initializer_signature_normalizes_string_literal_type_quotes() {
    let output = emit_dts(
        r#"
type O = { prop: string };
export const fn = (v: O['prop'], p: Omit<O, 'prop'>) => {};
"#,
    );
    assert!(
        output.contains(r#"export declare const fn: (v: O["prop"], p: Omit<O, "prop">) => void;"#),
        "Expected reconstructed function initializer signature to normalize string literal type quotes: {output}"
    );
}

#[test]
fn test_typeof_type() {
    let output = emit_dts("declare const x: number;\nexport type T = typeof x;");
    assert!(
        output.contains("typeof x"),
        "Expected typeof type: {output}"
    );
}
