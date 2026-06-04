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
fn test_tuple_object_index_signature_preserves_parameter_name() {
    let output = emit_dts("export type H = string | [string, { [key: string]: unknown }, ...H[]];");
    assert!(
        output.contains(
            "export type H = string | [string, {\n    [key: string]: unknown;\n}, ...H[]];"
        ),
        "Expected object index signature inside tuple to preserve its parameter name: {output}"
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
fn type_parameter_constraint_mapped_type_stays_inline() {
    let output = emit_dts(
        r#"
export const cf = <T extends { [P in K]: string; } & { cool: string }, K extends keyof T>(t: T, k: K) => {};
"#,
    );
    assert!(
        output.contains("cf: <T extends { [P in K]: string; } & {\n    cool: string;"),
        "Mapped type in a type-parameter constraint should stay inline: {output}"
    );
}

#[test]
fn type_parameter_constraint_mapped_type_stays_inline_with_renamed_key() {
    let output = emit_dts(
        r#"
export const pick = <Shape extends { [Key in Field | "extra"]: number; }, Field extends keyof Shape>(shape: Shape, field: Field) => {};
"#,
    );
    assert!(
        output.contains(
            "pick: <Shape extends { [Key in Field | \"extra\"]: number; }, Field extends keyof Shape>"
        ),
        "Mapped type constraint formatting must not depend on iteration variable names: {output}"
    );
}

#[test]
fn object_valued_type_parameter_constraint_mapped_type_stays_multiline() {
    let output = emit_dts(
        r#"
export type Example<T extends { [Key in keyof T]: { prop: any; } }> = {
    [Key in keyof T]: T[Key]["prop"];
};
"#,
    );
    assert!(
        output.contains("T extends {\n    [Key in keyof T]: {\n        prop: any;\n    };\n}"),
        "Object-valued mapped type constraints should keep structured multiline formatting: {output}"
    );
}

#[test]
fn top_level_mapped_type_alias_stays_multiline() {
    let output = emit_dts("export type Names<K extends string> = { [Key in K]: string; };");
    assert!(
        output.contains("{\n    [Key in K]: string;\n}"),
        "Top-level mapped type aliases should keep structured multiline formatting: {output}"
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

/// tsc 6.0 *preserves* source-level parentheses in annotation positions for
/// all type forms except `FunctionType`, `ConstructorType`, and bare
/// `InferType` (without a constraint). Primitive keywords like `(string)` and
/// composite types like `(string | number)` both round-trip with their parens
/// intact (see `tests::infer_paren_and_union_intersection`).

#[test]
fn parenthesized_simple_type_annotation_preserved() {
    // tsc 6.0 preserves source-level parens around a primitive keyword in
    // annotation position: `var x: (string)` stays `var x: (string)`.
    let output = emit_dts("export declare var x: (string);");
    assert!(
        output.contains("x: (string);"),
        "Expected parens preserved around keyword type: {output}"
    );
    // Renamed variable — prove the rule is not spelling-dependent.
    let output2 = emit_dts("export declare var value: (number);");
    assert!(
        output2.contains("value: (number);"),
        "Expected parens preserved around keyword type (renamed): {output2}"
    );
}

#[test]
fn parenthesized_union_type_annotation_stripped() {
    // `(string | number)` as a variable annotation — parens preserved
    let out = emit_dts("export declare var x: (string | number);");
    assert!(
        out.contains("x: (string | number)"),
        "Expected parenthesized union parens preserved: {out}"
    );
    // Same rule with different type names
    let out2 = emit_dts("export declare var y: (boolean | null);");
    assert!(
        out2.contains("y: (boolean | null)"),
        "Expected parenthesized union parens preserved for boolean|null: {out2}"
    );
}

#[test]
fn parenthesized_array_element_no_double_parens() {
    // `(string | number)[]` — the parens already wrap the union, structural
    // needs_parens must not add a second layer.
    let out = emit_dts("export declare var x: (string | number)[];");
    assert!(
        out.contains("(string | number)[]"),
        "Expected exactly one paren layer in array element: {out}"
    );
    assert!(
        !out.contains("((string | number))"),
        "Expected no double parens in array element: {out}"
    );
    // Conditional type element
    let out2 = emit_dts("export declare var y: (string extends number ? true : false)[];");
    assert!(
        out2.contains("(string extends number ? true : false)[]"),
        "Expected parenthesized conditional in array preserved: {out2}"
    );
    assert!(
        !out2.contains("(("),
        "Expected no double parens in conditional array element: {out2}"
    );
}

#[test]
fn parenthesized_union_member_no_double_parens() {
    // `((...) => void) | string` — function type inside union
    let out = emit_dts("export declare var f: ((x: number) => void) | string;");
    assert!(
        out.contains("((x: number) => void) | string"),
        "Expected parenthesized function member in union: {out}"
    );
    assert!(
        !out.contains("((("),
        "Expected no triple parens in union function member: {out}"
    );
}

#[test]
fn parenthesized_intersection_arm_no_double_parens() {
    // `(string | number) & object` — union inside intersection
    let out = emit_dts("export declare var x: (string | number) & object;");
    assert!(
        out.contains("(string | number) & object"),
        "Expected parenthesized union arm in intersection: {out}"
    );
    assert!(
        !out.contains("((string | number))"),
        "Expected no double parens in intersection arm: {out}"
    );
    // Conditional type arm
    let out2 = emit_dts("export declare var y: (string extends number ? A : B) & object;");
    assert!(
        out2.contains("(string extends number ? A : B) & object"),
        "Expected parenthesized conditional arm in intersection preserved: {out2}"
    );
}

#[test]
fn parenthesized_function_param_and_return_type_stripped() {
    // Parens on parameter and return type annotations are preserved verbatim
    // by tsc; they carry no semantic meaning but are kept in the output.
    let out = emit_dts("export declare function f(x: (string | number)): (boolean | null);");
    assert!(
        out.contains("x: (string | number)"),
        "Expected parens preserved on param type: {out}"
    );
    assert!(
        out.contains("): (boolean | null)"),
        "Expected parens preserved on return type: {out}"
    );
}

/// Mapped type with a union-of-named-tuples constraint must keep the
/// constraint on a single line, matching tsc output.
#[test]
fn mapped_type_named_tuple_union_constraint_stays_inline() {
    let output = emit_dts("export type M = { [K in [x: string] | [y: number]]: K };");
    // The constraint must appear inline.
    assert!(
        output.contains("[x: string] | [y: number]"),
        "Named-tuple union constraint must stay on a single line: {output}"
    );
    // Must not be split across lines.
    assert!(
        !output.contains("[x: string]\n") && !output.contains("[y: number]\n    |"),
        "Named-tuple union constraint must not be split across lines: {output}"
    );
}

/// Same fix applies when the iteration variable is named `P` instead of `K`.
/// Proves the fix is not sensitive to the iteration variable name.
#[test]
fn mapped_type_named_tuple_union_constraint_stays_inline_renamed_var() {
    let output = emit_dts("export type M = { [P in [a: string] | [b: number]]: P };");
    assert!(
        output.contains("[a: string] | [b: number]"),
        "Named-tuple union constraint must stay inline regardless of iteration var name: {output}"
    );
}

#[test]
fn mapped_type_keyof_noinfer_object_constraint_formats_multiline() {
    let output = emit_dts("export type M = { [K in keyof NoInfer<{ a: string, b: string }>]: K };");
    assert!(
        output.contains(
            "export type M = {\n    [K in keyof NoInfer<{\n        a: string;\n        b: string;\n    }>]: K;\n};"
        ),
        "Object type literals inside mapped constraints should use structured multiline formatting: {output}"
    );
}

#[test]
fn mapped_type_keyof_noinfer_object_constraint_formats_multiline_renamed_var() {
    let output =
        emit_dts("export type M = { [P in keyof NoInfer<{ left: number, right: boolean }>]: P };");
    assert!(
        output.contains(
            "export type M = {\n    [P in keyof NoInfer<{\n        left: number;\n        right: boolean;\n    }>]: P;\n};"
        ),
        "Object type literal formatting should not depend on mapped variable or property names: {output}"
    );
}

/// A union-of-named-tuples used as a top-level type alias must be preserved
/// in output (no regression for non-mapped-type positions).
#[test]
fn top_level_named_tuple_union_keeps_format() {
    let output = emit_dts("export type T = [x: string] | [y: number];");
    assert!(
        output.contains("[x: string]") && output.contains("[y: number]"),
        "Top-level named-tuple union must be present in output: {output}"
    );
}

/// Mapped type with an `as` clause containing a named-tuple union must
/// also stay on a single line.
#[test]
fn mapped_type_named_tuple_union_as_clause_stays_inline() {
    let output = emit_dts("export type M = { [K in string as [x: string] | [y: number]]: K };");
    assert!(
        output.contains("[x: string] | [y: number]"),
        "Named-tuple union in mapped-type as-clause must stay inline: {output}"
    );
}
