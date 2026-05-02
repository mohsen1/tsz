use super::*;

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        output.contains("Person[\"name\"]"),
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
        output.contains("type Add<A extends number, B extends number> = [\n    ...NTuple<A>,\n    ...NTuple<B>\n][\"length\"];"),
        "Expected variadic tuple indexed access to break across lines: {output}"
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
