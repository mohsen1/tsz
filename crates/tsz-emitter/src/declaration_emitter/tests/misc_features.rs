use super::*;
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

// =============================================================================
// 14. Numeric Literal Normalization
// =============================================================================

#[test]
fn test_normalize_numeric_literal_unchanged() {
    assert_eq!(DeclarationEmitter::normalize_numeric_literal("42"), "42");
    assert_eq!(
        DeclarationEmitter::normalize_numeric_literal("3.14"),
        "3.14"
    );
    assert_eq!(DeclarationEmitter::normalize_numeric_literal("0"), "0");
}

#[test]
fn test_normalize_numeric_literal_large_integer() {
    // Very large integers should be normalized through f64 round-trip
    let result = DeclarationEmitter::normalize_numeric_literal(
        "123456789123456789123456789123456789123456789123456789",
    );
    assert!(
        result.contains("e+"),
        "Expected scientific notation for very large number: {result}"
    );
}

// =============================================================================
// 15. Format JS Number
// =============================================================================

#[test]
fn test_format_js_number_infinity() {
    assert_eq!(
        DeclarationEmitter::format_js_number(f64::INFINITY),
        "Infinity"
    );
    assert_eq!(
        DeclarationEmitter::format_js_number(f64::NEG_INFINITY),
        "-Infinity"
    );
}

#[test]
fn test_format_js_number_nan() {
    assert_eq!(DeclarationEmitter::format_js_number(f64::NAN), "NaN");
}

#[test]
fn test_format_js_number_integers() {
    assert_eq!(DeclarationEmitter::format_js_number(0.0), "0");
    assert_eq!(DeclarationEmitter::format_js_number(42.0), "42");
    assert_eq!(DeclarationEmitter::format_js_number(-1.0), "-1");
}

#[test]
fn test_format_js_number_floats() {
    assert_eq!(DeclarationEmitter::format_js_number(3.15), "3.15");
    assert_eq!(DeclarationEmitter::format_js_number(0.5), "0.5");
}

#[test]
fn test_large_separated_numeric_literal_declaration_emit() {
    let output = emit_dts(
        r#"
export type X = 0x8000_0000_0000_0000;
export type Y = 0x7fff_ffff_ffff_ffff;
export const y: 0x8000_0000_0000_0000 = 0 as any;
"#,
    );

    assert!(
        output.contains("export type X = 9223372036854776000;"),
        "Expected large separated hex literal type X to use JS number text: {output}"
    );
    assert!(
        output.contains("export type Y = 9223372036854776000;"),
        "Expected large separated hex literal type Y to use JS number text: {output}"
    );
    assert!(
        output.contains("export declare const y: 9223372036854776000;"),
        "Expected large separated hex literal annotation to use JS number text: {output}"
    );
    assert!(
        !output.contains("9223372036854775807"),
        "Declaration output must not saturate through i64::MAX: {output}"
    );
}

// =============================================================================
// 16. Rest Parameters
// =============================================================================

#[test]
fn test_rest_parameter_in_function() {
    let output = emit_dts("export function sum(...nums: number[]): number { return 0; }");
    assert!(
        output.contains("...nums: number[]"),
        "Expected rest parameter: {output}"
    );
}

#[test]
fn test_flat_map_callback_returning_array_subclass_flattens_element_type() {
    let output = emit_dts(
        r#"
declare const foo: unknown[];
const bar = foo.flatMap(value => value as Foo);
interface Foo extends Array<string> {}
"#,
    );

    assert!(
        output.contains("declare const bar: string[];"),
        "flatMap callback returning Array subclass should emit flattened element type: {output}"
    );
}

#[test]
fn test_array_literal_of_function_expressions_drops_optional_param_subtypes() {
    // Regression for narrowingUnionToUnion: when inferring an array element
    // union from `[(x: T) => …, (x?: T) => …, …]` literals, the optional-
    // parameter form is a structural subtype of the required-parameter
    // form, so tsc's UnionReduction.Subtype drops the `?` arm. Mirror that
    // text-side: any function-typed arm whose only difference from another
    // arm is one or more `?:` parameters should be removed.
    let output = emit_dts(
        r#"
const TEST_CASES = [
    (value: string) => {},
    (value?: string) => {},
    (value: number) => {},
    (value?: number) => {},
];
"#,
    );
    let elem_text = output
        .lines()
        .find(|line| line.contains("TEST_CASES:"))
        .expect("TEST_CASES line missing");
    assert!(
        elem_text.contains("((value: string) => void)"),
        "Expected required-param string arm to remain: {output}"
    );
    assert!(
        elem_text.contains("((value: number) => void)"),
        "Expected required-param number arm to remain: {output}"
    );
    assert!(
        !elem_text.contains("(value?: string)"),
        "Optional-param string arm should be subsumed by required-param sibling: {output}"
    );
    assert!(
        !elem_text.contains("(value?: number)"),
        "Optional-param number arm should be subsumed by required-param sibling: {output}"
    );
}

#[test]
fn test_array_literal_of_function_expressions_paren_wraps_each_arm() {
    // Regression for narrowingUnionToUnion: when an array literal contains
    // multiple function expressions that don't all share an identical type,
    // each function-typed union arm must be parenthesized so the trailing
    // `=>` does not bind across the `|`. Without parens around each arm,
    // `(a: A) => void | (a: B) => void` parses as
    // `(a: A) => (void | (a: B) => void)`.
    let output = emit_dts(
        r#"
const TEST_CASES = [
    (value: string) => {},
    (value: number) => {},
];
"#,
    );
    assert!(
        output.contains("(((value: string) => void) | ((value: number) => void))[]"),
        "Expected each function-typed union arm to be parenthesized: {output}"
    );
}

// =============================================================================
// 17. Call / Construct Signatures in Interfaces
// =============================================================================

#[test]
fn test_call_signature_in_interface() {
    let output = emit_dts(
        r#"
    export interface Callable {
        (x: number): string;
    }
    "#,
    );
    assert!(
        output.contains("(x: number): string;"),
        "Expected call signature: {output}"
    );
}

#[test]
fn test_construct_signature_in_interface() {
    let output = emit_dts(
        r#"
    export interface Constructable {
        new (name: string): object;
    }
    "#,
    );
    assert!(
        output.contains("new (name: string): object;"),
        "Expected construct signature: {output}"
    );
}

// =============================================================================
// 18. Type Predicate (type guard)
// =============================================================================

#[test]
fn test_type_predicate_in_function() {
    let output = emit_dts(
        r#"
    export function isString(x: unknown): x is string {
        return typeof x === "string";
    }
    "#,
    );
    assert!(
        output.contains("x is string"),
        "Expected type predicate: {output}"
    );
}

#[test]
fn test_exported_function_returning_declared_conditional_call_preserves_return_type() {
    let output = emit_dts_with_usage_analysis(
        r#"
export declare function pick<T>(value: T): T extends () => infer R ? R : never;
export function wrap<T>(value: T) {
    return pick(value);
}
"#,
    );

    assert!(
        output.contains(
            "export declare function wrap<T>(value: T): T extends () => infer R ? R : never;"
        ),
        "Expected exported function to reuse declared helper conditional return type: {output}"
    );
}

#[test]
fn test_exported_function_returning_mapped_infer_call_expands_alias_return_type() {
    let output = emit_dts_with_usage_analysis(
        r#"
export type Boxed<T> = { value: T extends number ? T : string };
export declare function read<T>(value: T): T extends { [K in keyof Boxed<infer U>]: Boxed<infer U>[K] } ? U : never;
export function unwrap<T>(value: T) {
    return read(value);
}
"#,
    );

    assert!(
        output.contains(
            "export declare function unwrap<T>(value: T): T extends {\n    value: infer U extends number ? infer U : string;\n} ? U : never;"
        ),
        "Expected mapped alias helper return type to expand in declaration scope: {output}"
    );
}

#[test]
fn test_exported_function_returning_shadowed_helper_does_not_borrow_top_level_return_type() {
    let output = emit_dts_with_usage_analysis(
        r#"
export declare function pick<T>(value: T): T extends () => infer R ? R : never;
export function wrap<T>(value: T) {
    function pick(value: T) {
        return pick(value);
    }
    return pick(value);
}
"#,
    );

    let wrap_decl = output
        .lines()
        .find(|line| line.starts_with("export declare function wrap"))
        .unwrap_or_else(|| panic!("Expected exported wrap declaration: {output}"));
    assert!(
        !wrap_decl.contains("infer R"),
        "Expected shadowed local helper call not to reuse top-level pick return type: {output}"
    );
}

// =============================================================================
// 19. Default Parameter Values (stripped)
// =============================================================================

#[test]
fn test_default_parameter_values_omitted() {
    let output = emit_dts(
        r#"
    export function greet(name: string = "world"): void {}
    "#,
    );
    // Default values should be stripped; parameter should remain with its type
    assert!(
        output.contains("name"),
        "Expected parameter name preserved: {output}"
    );
    // The default value itself should not appear in the .d.ts
    assert!(
        !output.contains("\"world\""),
        "Default value should be stripped from .d.ts: {output}"
    );
}

// =============================================================================
// 20. Using declaration emits as const
// =============================================================================

#[test]
fn test_using_declaration_emits_const() {
    let output = emit_dts(r#"export using x: Disposable = getResource();"#);
    // `using` declarations emit as `const` in .d.ts
    assert!(
        output.contains("const x"),
        "Expected using declaration to emit as const: {output}"
    );
}

// =============================================================================
// 21. Void-returning function body inference
// =============================================================================

#[test]
fn test_void_body_function_infers_void_return() {
    let output = emit_dts(
        r#"
    export function doNothing() {
        console.log("hi");
    }
    "#,
    );
    assert!(
        output.contains("void"),
        "Expected void return type for function with no return: {output}"
    );
}

// =============================================================================
// 22. Side-effect imports preserved
// =============================================================================

#[test]
fn test_side_effect_import_preserved() {
    let output = emit_dts(r#"import "./polyfill";"#);
    assert!(
        output.contains("import \"./polyfill\""),
        "Expected side-effect import to be preserved: {output}"
    );
}

// =============================================================================
// 23. Literal type aliases
// =============================================================================

#[test]
fn test_literal_type_alias() {
    let output = emit_dts("export type Direction = 'up' | 'down' | 'left' | 'right';");
    assert!(
        output.contains("'up'") || output.contains("\"up\""),
        "Expected string literal type: {output}"
    );
}

// =============================================================================
// 24. Keyof type
// =============================================================================

#[test]
fn test_keyof_type() {
    let output = emit_dts("export type Keys<T> = keyof T;");
    assert!(output.contains("keyof T"), "Expected keyof type: {output}");
}

#[test]
fn test_indexed_access_typeof_object_is_parenthesized() {
    let output = emit_dts(
        r#"
const a = { a: "value of a" } as const;
export type Value = typeof a["a"];
"#,
    );
    assert!(
        output.contains("export type Value = (typeof a)[\"a\"];"),
        "typeof object in indexed access needs parens: {output}"
    );
}

#[test]
fn test_keyof_indexed_access_drops_unnecessary_source_parens() {
    let output = emit_dts(
        r#"
type A = { a: { b: string } };
export type Keys = keyof (A["a"]);
"#,
    );
    assert!(
        output.contains("export type Keys = keyof A[\"a\"];"),
        "keyof indexed access should not retain source-only parens: {output}"
    );
}

// =============================================================================
// 25. Type operator (readonly arrays)
// =============================================================================

#[test]
fn test_readonly_array_type() {
    let output = emit_dts("export type ReadonlyArr = readonly number[];");
    assert!(
        output.contains("readonly number[]"),
        "Expected readonly array type: {output}"
    );
}

// =============================================================================
// 26. Parenthesized type
// =============================================================================

#[test]
fn test_parenthesized_function_type_in_array() {
    let output = emit_dts("export type FnArray = ((x: number) => void)[];");
    assert!(
        output.contains("((x: number) => void)[]"),
        "Expected parenthesized function type in array: {output}"
    );
}

// =============================================================================
// 27. Computed property names
// =============================================================================

#[test]
fn test_computed_symbol_property() {
    let output = emit_dts(
        r#"
    export interface Iterable {
        [Symbol.iterator](): Iterator<any>;
    }
    "#,
    );
    assert!(
        output.contains("[Symbol.iterator]"),
        "Expected computed Symbol property: {output}"
    );
}

// =============================================================================
// 28. Export assignment (export =)
// =============================================================================

#[test]
fn test_export_equals() {
    let output = emit_dts(
        r#"
    declare const myLib: { version: string };
    export = myLib;
    "#,
    );
    assert!(
        output.contains("export = myLib;"),
        "Expected export = : {output}"
    );
}

#[test]
fn test_export_equals_import_equals_keeps_namespace_dependency() {
    let output = emit_dts_with_usage_analysis(
        r#"
    namespace m3 {
        export namespace m2 {
            export interface connectModule {
                (res, req, next): void;
            }
            export interface connectExport {
                use: (mod: connectModule) => connectExport;
                listen: (port: number) => void;
            }
        }

        export var server: {
            (): m2.connectExport;
            test1: m2.connectModule;
            test2(): m2.connectModule;
        };
    }

    import m = m3;
    export = m;
    "#,
    );

    let namespace_pos = output
        .find("declare namespace m3")
        .expect("Expected namespace dependency to be preserved");
    let import_pos = output
        .find("import m = m3;")
        .expect("Expected import equals alias to be emitted");
    let export_pos = output
        .find("export = m;")
        .expect("Expected export assignment to be emitted");

    assert!(
        namespace_pos < import_pos && import_pos < export_pos,
        "Expected namespace, import alias, and export assignment to preserve source order: {output}"
    );
}

#[test]
fn test_export_equals_import_equals_chain_keeps_namespace_dependency() {
    let output = emit_dts_with_usage_analysis(
        r#"
    namespace m {
        export namespace c {
            export class c {
            }
        }
    }

    import a = m.c;
    import b = a;
    export = b;
    "#,
    );

    let namespace_pos = output
        .find("declare namespace m")
        .expect("Expected namespace dependency to be preserved");
    let first_import_pos = output
        .find("import a = m.c;")
        .expect("Expected first import equals alias to be emitted");
    let second_import_pos = output
        .find("import b = a;")
        .expect("Expected chained import equals alias to be emitted");
    let export_pos = output
        .find("export = b;")
        .expect("Expected export assignment to be emitted");

    assert!(
        namespace_pos < first_import_pos
            && first_import_pos < second_import_pos
            && second_import_pos < export_pos,
        "Expected namespace, import chain, and export assignment to preserve source order: {output}"
    );
}

#[test]
fn test_exported_namespace_import_equals_uses_target_for_outer_inferred_type() {
    let output = emit_dts_with_usage_analysis(
        r#"
    export namespace x {
        export class c {
            foo(a: number) {
                return a;
            }
        }
    }

    export namespace m2 {
        export namespace m3 {
            export import c = x.c;
            export var cProp = new c();
        }
    }

    export var d = new m2.m3.c();
    "#,
    );

    assert!(
        output.contains("export declare var d: x.c;"),
        "Expected exported variable to use the import-equals target type: {output}"
    );
}

#[test]
fn test_import_type_with_resolution_mode_attributes_is_preserved() {
    let output = emit_dts_with_usage_analysis(
        r#"
    import type { RequireInterface } from "pkg" with { "resolution-mode": "require" };
    import { type RequireInterface as Req } from "pkg" with { "resolution-mode": "require" };

    export interface LocalInterface extends RequireInterface {}
    export interface Loc extends Req {}
    "#,
    );

    assert!(
        output.contains(
            r#"import type { RequireInterface } from "pkg" with { "resolution-mode": "require" };"#
        ),
        "Expected type-only import attributes to be preserved: {output}"
    );
    assert!(
        output.contains(
            r#"import { type RequireInterface as Req } from "pkg" with { "resolution-mode": "require" };"#
        ),
        "Expected named import attributes to be preserved: {output}"
    );
}

#[test]
fn test_import_type_alias_is_preserved_with_usage_analysis() {
    let output = emit_dts_with_usage_analysis(
        r#"
    import { type RequireInterface as Req } from "pkg";

    export interface Loc extends Req {}
    "#,
    );

    assert!(
        output.contains(r#"import { type RequireInterface as Req } from "pkg";"#),
        "Expected aliased type import to be preserved: {output}"
    );
}

#[test]
fn test_namespace_import_type_is_preserved_with_usage_analysis() {
    let source = r#"
    import * as ns from "pkg";
    export const value = ns;
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
        .find_map(|&stmt_idx| {
            let stmt_node = parser.arena.get(stmt_idx)?;
            if let Some(var_stmt) = parser.arena.get_variable(stmt_node) {
                return Some(var_stmt);
            }
            let export = parser.arena.get_export_decl(stmt_node)?;
            let clause_node = parser.arena.get(export.export_clause)?;
            parser.arena.get_variable(clause_node)
        })
        .expect("missing variable statement");
    let decl_list_idx = var_stmt.declarations.nodes[0];
    let decl_list = parser
        .arena
        .get(decl_list_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing declaration list");
    let decl_idx = decl_list.declarations.nodes[0];
    let decl = parser
        .arena
        .get(decl_idx)
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing declaration");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let ns_sym_id = binder
        .file_locals
        .get("ns")
        .expect("expected namespace import symbol");

    let interner = TypeInterner::new();
    let namespace_type = interner.module_namespace(SymbolRef(ns_sym_id.0));

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(decl.name.0, namespace_type);

    let current_arena = Arc::new(parser.arena.clone());
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(current_arena, "test.ts".to_string());
    let output = emitter.emit(root);

    assert!(
        output.contains(r#"import * as ns from "pkg";"#),
        "Expected namespace import to be preserved: {output}"
    );
    assert!(
        output.contains("export declare const value: typeof ns;"),
        "Expected exported value to use the namespace import alias type: {output}"
    );
}

#[test]
fn test_exported_namespace_import_initializer_preserves_typeof_alias() {
    let output = emit_dts_with_usage_analysis(
        r#"
    import * as ns from "pkg";
    export const value = ns;
    "#,
    );

    assert!(
        output.contains(r#"import * as ns from "pkg";"#),
        "Expected namespace import to survive usage analysis: {output}"
    );
    assert!(
        output.contains("export declare const value: typeof ns;"),
        "Expected exported namespace import initializer to emit typeof alias: {output}"
    );
}

#[test]
fn test_call_expression_recovers_return_type_from_callee_type() {
    let source = r#"
    export const a = helper.x();
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
        .find_map(|&stmt_idx| {
            let stmt_node = parser.arena.get(stmt_idx)?;
            if let Some(var_stmt) = parser.arena.get_variable(stmt_node) {
                return Some(var_stmt);
            }
            let export = parser.arena.get_export_decl(stmt_node)?;
            let clause_node = parser.arena.get(export.export_clause)?;
            parser.arena.get_variable(clause_node)
        })
        .expect("missing variable statement");
    let decl_list_idx = var_stmt.declarations.nodes[0];
    let decl_list = parser
        .arena
        .get(decl_list_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing declaration list");
    let decl_idx = decl_list.declarations.nodes[0];
    let decl = parser
        .arena
        .get(decl_idx)
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing declaration");
    let call = parser
        .arena
        .get(decl.initializer)
        .and_then(|node| parser.arena.get_call_expr(node))
        .expect("missing call expression");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let callee_type = interner.function(FunctionShape::new(Vec::new(), TypeId::STRING));

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(call.expression.0, callee_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("export declare const a: string;"),
        "Expected call expression to recover return type from callee type: {output}"
    );
}

#[test]
fn test_source_call_uses_cached_generic_return_alias_arguments() {
    let source = r#"
    type Boxified<T> = { [P in keyof T]: { value: T[P] } };
    type A = { a: string };
    type B = { b: string };
    function boxify<T>(obj: T) {
        throw new Error();
    }
    function f1(x: A | B | undefined) {
        return boxify(x);
    }
    "#;
    let (parser, root) = parse_test_source(source);

    let call_idx = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            if node.kind != syntax_kind_ext::CALL_EXPRESSION {
                return None;
            }
            let call = parser.arena.get_call_expr(node)?;
            (parser.arena.get_identifier_text(call.expression) == Some("boxify"))
                .then_some(NodeIndex(idx as u32))
        })
        .expect("missing boxify call");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let boxified_sym = binder
        .file_locals
        .get("Boxified")
        .expect("missing Boxified symbol");
    let boxify_sym = binder
        .file_locals
        .get("boxify")
        .expect("missing boxify symbol");

    let interner = TypeInterner::new();
    let type_param = tsz_solver::types::TypeParamInfo::simple(interner.intern_string("T"));
    let boxified_def = DefId(7010);
    let return_type = interner.application(
        interner.lazy(boxified_def),
        vec![interner.type_param(type_param)],
    );
    let function_type = interner.function(FunctionShape {
        type_params: vec![type_param],
        params: Vec::new(),
        this_type: None,
        return_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.def_to_symbol.insert(boxified_def, boxified_sym);
    type_cache.symbol_types.insert(boxify_sym, function_type);

    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let type_text = emitter
        .call_expression_source_return_type_text(call_idx)
        .expect("expected source call return type");

    assert_eq!(type_text, "Boxified<A | B | undefined>");
}

#[test]
fn test_function_return_prefers_object_literal_over_return_type_wrapper() {
    let source = r#"
    function f1(s: string) {
        return { a: 1, b: s };
    }
    "#;
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let f1_sym = binder.file_locals.get("f1").expect("missing f1 symbol");

    let interner = TypeInterner::new();
    let object_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![
            PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
            PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
        ],
        string_index: None,
        number_index: None,
        symbol: None,
    });
    let function_arg = interner.function(FunctionShape::new(Vec::new(), object_type));
    let return_type_def = DefId(7020);
    let return_type = interner.application(interner.lazy(return_type_def), vec![function_arg]);
    let function_type = interner.function(FunctionShape::new(Vec::new(), return_type));

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .def_to_name
        .insert(return_type_def, "ReturnType".to_string());
    type_cache.symbol_types.insert(f1_sym, function_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function f1(s: string): {\n    a: number;\n    b: string;\n};"),
        "Expected object literal return type to be emitted directly: {output}"
    );
    assert!(
        !output.contains("ReturnType<"),
        "Did not expect ReturnType wrapper in function declaration: {output}"
    );
}

#[test]
fn test_export_type_with_resolution_mode_attributes_is_preserved() {
    let output = emit_dts_with_usage_analysis(
        r#"
    export type { RequireInterface } from "pkg" with { "resolution-mode": "require" };
    "#,
    );

    assert!(
        output.contains(
            r#"export type { RequireInterface } from "pkg" with { "resolution-mode": "require" };"#
        ),
        "Expected export type attributes to be preserved: {output}"
    );
}

#[test]
fn test_export_json_attributes_are_stripped_from_declarations() {
    let output = emit_dts(r#"export { default as data } from "./dep.json" with { type: "json" };"#);

    assert!(
        output.contains(r#"export { default as data } from "./dep.json";"#),
        "Expected JSON export attribute to be stripped from declaration output: {output}"
    );
    assert!(
        !output.contains("with {"),
        "Did not expect non-resolution-mode attributes in declaration output: {output}"
    );
}

#[test]
fn test_inferred_printer_reduces_conditional_alias_applications() {
    use tsz_solver::types::{ConditionalType, TypeParamInfo};

    let (parser, _root) = parse_test_source("");

    let mut foreign_parser = ParserState::new(
        "lib.d.ts".to_string(),
        "type Select<T> = T extends string ? 1 : 2;".to_string(),
    );
    let _ = foreign_parser.parse_source_file();
    let alias_decl = foreign_parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            (node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION).then_some(NodeIndex(idx as u32))
        })
        .expect("missing conditional type alias declaration");

    let mut binder = BinderState::new();
    let select_sym = binder
        .symbols
        .alloc(symbol_flags::TYPE_ALIAS, "Select".to_string());
    binder
        .symbols
        .get_mut(select_sym)
        .expect("missing synthetic conditional alias symbol")
        .declarations
        .push(alias_decl);

    let interner = TypeInterner::new();
    let type_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let cond = interner.conditional(ConditionalType {
        check_type: interner.type_param(type_param),
        extends_type: TypeId::STRING,
        true_type: interner.literal_number(1.0),
        false_type: interner.literal_number(2.0),
        is_distributive: false,
    });

    let def_id = DefId(99);
    let app = interner.application(interner.lazy(def_id), vec![TypeId::STRING]);

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.def_to_symbol.insert(def_id, select_sym);
    type_cache.def_types.insert(def_id.0, cond);
    type_cache
        .def_type_params
        .insert(def_id.0, vec![type_param]);

    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);

    assert_eq!(emitter.print_type_id(app), "Select<string>");
    assert_eq!(emitter.print_type_id_for_inferred_declaration(app), "1");
}

#[test]
fn test_asserted_import_type_with_resolution_mode_attributes_is_preserved() {
    let output = emit_dts(
        r#"
    export type LocalInterface = import("pkg", { with: {"resolution-mode": "require"} }).RequireInterface;
    export const value = (null as any as import("pkg", { with: {"resolution-mode": "require"} }).RequireInterface);
    "#,
    );

    assert!(
        output.contains(
            r#"export type LocalInterface = import("pkg", { with: { "resolution-mode": "require" } }).RequireInterface;"#
        ),
        "Expected import type attributes to be formatted canonically in type aliases: {output}"
    );
    assert!(
        output.contains(
            r#"export declare const value: import("pkg", { with: { "resolution-mode": "require" } }).RequireInterface;"#
        ),
        "Expected asserted import type with attributes to be preserved on exported values: {output}"
    );
}

#[test]
fn test_import_type_non_string_argument_formats_object_as_type_literal() {
    let output = emit_dts(r#"export const x: import({x: 12}) = undefined as any;"#);

    assert!(
        output.contains("export declare const x: import({\n    x: 12;\n});"),
        "Expected non-string import type argument to be formatted as a type literal: {output}"
    );
}

#[test]
fn test_invalid_resolution_mode_attribute_is_dropped_and_unused_mixed_import_is_elided() {
    let output = emit_dts_with_usage_analysis(
        r#"
    import type { RequireInterface } from "pkg" with { "resolution-mode": "foobar" };
    import { ImportInterface } from "pkg" with { "resolution-mode": "import" };
    import { type RequireInterface as Req, RequireInterface as Req2 } from "pkg" with { "resolution-mode": "require" };

    export interface LocalInterface extends RequireInterface, ImportInterface {}
    "#,
    );

    assert!(
        output.contains(r#"import type { RequireInterface } from "pkg";"#),
        "Expected invalid resolution-mode attribute to be dropped: {output}"
    );
    assert!(
        output.contains(
            r#"import { ImportInterface } from "pkg" with { "resolution-mode": "import" };"#
        ),
        "Expected valid resolution-mode attribute to be preserved: {output}"
    );
    assert!(
        !output.contains("Req2"),
        "Expected unused mixed import bindings to be elided: {output}"
    );
}

// =============================================================================
// 29. Namespace export as
// =============================================================================

#[test]
fn test_star_export_as_namespace() {
    let output = emit_dts(r#"export * as utils from "./utils";"#);
    assert!(
        output.contains("export * as utils from"),
        "Expected namespace re-export: {output}"
    );
}

// =============================================================================
// 30. Asserts modifier in type predicate
// =============================================================================

#[test]
fn test_assertion_function() {
    let output = emit_dts(
        r#"
    export function assertDefined(val: unknown): asserts val {
        if (val == null) throw new Error();
    }
    "#,
    );
    assert!(
        output.contains("asserts val"),
        "Expected asserts modifier: {output}"
    );
}

#[test]
fn test_setter_parameter_asserts_this_predicate_is_rescued_from_source() {
    let output = emit_dts(
        r#"
    declare class Wat {
        set p2(x: asserts this is string);
    }
    "#,
    );

    assert!(
        output.contains("set p2(x: asserts this is string);"),
        "Expected setter parameter asserts predicate to be preserved: {output}"
    );
}

#[test]
fn test_const_identity_call_preserves_numeric_literal_initializer() {
    let output = emit_dts(
        r#"
function id<T>(x: T): T {
    return x;
}

const value = id(123);
"#,
    );

    assert!(
        output.contains("declare const value = 123;"),
        "Expected const identity call to preserve numeric literal initializer: {output}"
    );
}

#[test]
fn test_const_identity_call_preserves_negative_numeric_literal_initializer() {
    let output = emit_dts(
        r#"
function id<T>(x: T): T {
    return x;
}

const value = id(-123);
"#,
    );

    assert!(
        output.contains("declare const value = -123;"),
        "Expected const identity call to preserve negative numeric literal initializer: {output}"
    );
}

// =============================================================================
// 31. Multiple variable declarations on one line
// =============================================================================

#[test]
fn test_multiple_variable_declarators() {
    let output = emit_dts("export var x: number, y: string;");
    assert!(
        output.contains("x: number"),
        "Expected first variable: {output}"
    );
    assert!(
        output.contains("y: string"),
        "Expected second variable: {output}"
    );
}

#[test]
fn test_grouped_let_declarator_preserves_null_initializer_type() {
    let output = emit_dts(r#"let l9 = 0, l10: string = "", l11 = null;"#);
    assert!(
        output.contains("declare let l9: number, l10: string, l11: null;"),
        "Expected grouped let null initializer to emit null: {output}"
    );

    let const_output = emit_dts("const c = null;");
    assert!(
        const_output.contains("declare const c: any;"),
        "Expected const null initializer to keep tsc-compatible any: {const_output}"
    );
}

#[test]
fn test_type_only_same_name_interface_reference_does_not_emit_local_value_dependency() {
    let output = emit_dts_with_usage_analysis(
        r#"
export interface Component {
    play(): void;
}

declare function createComponent(): void;
const Component = createComponent();

export type ComponentDefinition = Partial<Component>;
"#,
    );

    assert!(
        output.contains("export type ComponentDefinition = Partial<Component>;"),
        "Expected exported type alias to remain: {output}"
    );
    assert!(
        !output.contains("declare const Component"),
        "Did not expect type-only Component reference to emit local const: {output}"
    );
}

#[test]
fn test_const_shadowing_non_exported_type_alias_emits_value_declaration() {
    // Regression for genericContextualTypes1: in a script-mode file (no
    // imports/exports) a `const fn: fn = …` whose name shadows a
    // non-exported `type fn = …` must still be emitted as `declare const`.
    // The earlier behavior treated the value-side const as "type-only
    // exported" because the shared symbol carried a type-alias declaration,
    // even though that type alias itself was not exported.
    let output = emit_dts_with_usage_analysis(
        r#"
type fn = <A>(a: A) => A;
const fn: fn = a => a;
"#,
    );
    assert!(
        output.contains("type fn = <A>(a: A) => A;"),
        "Expected type alias to remain: {output}"
    );
    assert!(
        output.contains("declare const fn: fn;"),
        "Expected value-side const shadowing the non-exported type alias to be emitted: {output}"
    );
}

#[test]
fn test_top_level_export_import_alias_preferred_over_qualified_target() {
    // Regression for internalAliasClassInsideTopLevelModuleWithExport:
    // when `export import xc = x.c;` is at the file root, references to the
    // class instance type should be emitted using the alias `xc`, not the
    // canonical target `x.c`. The alias-target rewrite previously kicked in
    // unconditionally for every exported import alias, so the printer's
    // correct `xc` output was being clobbered into `x.c`. Top-level aliases
    // are always in scope wherever the d.ts is consumed, so the rewrite
    // should only canonicalize aliases declared inside a namespace where
    // the local short name might not be reachable from an outer reference.
    let output = emit_dts_with_usage_analysis(
        r#"
export namespace x {
    export class c {
        foo(a: number) {
            return a;
        }
    }
}

export import xc = x.c;
export var cProp = new xc();
"#,
    );
    assert!(
        output.contains("export declare var cProp: xc;"),
        "Expected top-level export import alias to be preferred over its qualified target: {output}"
    );
}

#[test]
fn test_js_named_export_function_emitted_at_unfold_position_not_hoisted() {
    // Regression for nodeModulesAllowJsGeneratedNameCollisions: when a JS
    // function declaration's name appears in a folded `export { foo }`
    // statement, the unfold path emits `export function foo(): ...` at the
    // export statement's source position. Hoisting the same function to the
    // top of the file would emit it twice (once hoisted, once unfolded) and
    // also reorder it before sibling inline-exported declarations like
    // `export const __esModule = false`.
    let output = emit_js_dts_with_usage_analysis(
        r#"
function require() {}
const exports = {};
class Object {}
export const __esModule = false;
export {require, exports, Object};
"#,
    );
    assert_eq!(
        output.matches("export function require(): void;").count(),
        1,
        "Expected `export function require(): void;` to be emitted exactly once: {output}"
    );
    let esmodule_pos = output
        .find("export const __esModule")
        .expect("__esModule line missing");
    let require_pos = output
        .find("export function require")
        .expect("require line missing");
    assert!(
        esmodule_pos < require_pos,
        "Expected `__esModule` to be emitted before `require` (matching the source order of inline + folded exports): {output}"
    );
}

#[test]
fn test_export_assignment_keeps_uninitialized_value_declaration() {
    // Regression for privacyCheckExportAssignmentOnExportedGenericInterface1:
    // a `var X: T;` (no initializer, with type annotation) whose only public
    // API consumer is `export = X` was being filtered out by the
    // initializer-only-dependency check, because that check only looked at
    // `export { X }` specifiers and did not recognize commonjs
    // `export = X` as an exporter of the value-side name.
    let output = emit_dts_with_usage_analysis(
        r#"
namespace Foo {
    export interface A<T> {
    }
}
interface Foo<T> {
}
var Foo: new () => Foo.A<Foo<string>>;
export = Foo;
"#,
    );
    assert!(
        output.contains("declare var Foo:"),
        "Expected `declare var Foo` to be emitted when `export = Foo` is the consumer: {output}"
    );
    assert!(
        output.contains("export = Foo;"),
        "Expected the export assignment to be preserved: {output}"
    );
}

#[test]
fn test_inferred_const_initializer_call_preserves_local_alias() {
    // Regression for #3755: declaration emit was dropping a local type alias
    // that an `export const` *only* references through the inferred type of
    // its call-expression initializer. The emitted .d.ts referenced the
    // alias but never declared it, producing invalid output.
    let output = emit_dts_with_usage_analysis(
        r#"
type Box = { value: number };
function make(): Box { return { value: 1 }; }
export const item = make();
"#,
    );
    assert!(
        output.contains("type Box ="),
        "Expected the local `type Box` to be retained when `export const item = make()` \
         depends on it through the callee's declared return-type annotation: {output}"
    );
    assert!(
        output.contains("export declare const item: Box"),
        "Expected the inferred const to keep its alias-named annotation: {output}"
    );
}

#[test]
fn test_export_default_identifier_keeps_ambient_value_declaration() {
    // Regression for uniqueSymbolPropertyDeclarationEmit: a `declare const X`
    // (no initializer, with a value-side type annotation) whose only public
    // API consumer is `export default X` was being filtered out by the
    // initializer-only-dependency check. The check's name-export lookup
    // only considered `EXPORT_SPECIFIER` and `EXPORT_ASSIGNMENT` nodes;
    // tsz parses `export default X` as an `EXPORT_DECLARATION` with
    // `is_default_export: true` and the identifier in `export_clause`,
    // which neither path matched.
    let output = emit_dts_with_usage_analysis(
        r#"
declare const Op: {
  readonly or: unique symbol;
};

export default Op;
"#,
    );
    assert!(
        output.contains("declare const Op:"),
        "Expected `declare const Op` to be emitted when `export default Op` is the consumer: {output}"
    );
    assert!(
        output.contains("export default Op;"),
        "Expected the default export to be preserved: {output}"
    );
}

#[test]
fn test_destructuring_variable_declaration_groups_typed_bindings() {
    let source = r#"var [x, y] = [1, "hello"];"#;
    let (parser, root) = parse_test_source(source);
    let root_node = parser.arena.get(root).expect("missing root node");
    let stmt_idx = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file")
        .statements
        .nodes[0];
    let stmt = parser
        .arena
        .get(stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing variable statement");
    let decl_list = parser
        .arena
        .get(stmt.declarations.nodes[0])
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing declaration list");
    let decl = parser
        .arena
        .get(decl_list.declarations.nodes[0])
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing declaration");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(decl.initializer.0, tuple_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare var x: number, y: string;"),
        "Expected destructured bindings to emit in one typed declaration: {output}"
    );
}

#[test]
fn test_string_literal_tuple_binding_preserves_alias_and_return_union() {
    let output = emit_dts(
        r#"
type RexOrRaptor = "t-rex" | "raptor";
let [im, a, dinosaur]: ["I'm", "a", RexOrRaptor] = ["I'm", "a", "t-rex"];

function rawr(dino: RexOrRaptor) {
    if (dino === "t-rex") {
        return "ROAAAAR!";
    }
    if (dino === "raptor") {
        return "yip yip!";
    }
    throw "Unexpected " + dino;
}
"#,
    );

    assert!(
        output.contains("declare let im: \"I'm\", a: \"a\", dinosaur: RexOrRaptor;"),
        "Expected tuple destructuring to preserve the alias from the source tuple annotation: {output}"
    );
    assert!(
        output.contains("declare function rawr(dino: RexOrRaptor): \"ROAAAAR!\" | \"yip yip!\";"),
        "Expected string literal returns from guarded branches to emit as a union: {output}"
    );
}

#[test]
fn test_mutable_array_literal_binding_widens_homogeneous_literals() {
    let source = r#"
let [hello, brave] = ["Hello", "Brave"];
let [one, two] = [1, 2];
let [yes, no] = [true, false];
export let [ma, mb] = ["A", 1];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let tuple_types = [
        interner.tuple(vec![
            TupleElement {
                type_id: interner.literal_string("Hello"),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.literal_string("Brave"),
                name: None,
                optional: false,
                rest: false,
            },
        ]),
        interner.tuple(vec![
            TupleElement {
                type_id: interner.literal_number(1.0),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.literal_number(2.0),
                name: None,
                optional: false,
                rest: false,
            },
        ]),
        interner.tuple(vec![
            TupleElement {
                type_id: interner.literal_boolean(true),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.literal_boolean(false),
                name: None,
                optional: false,
                rest: false,
            },
        ]),
        interner.tuple(vec![
            TupleElement {
                type_id: interner.literal_string("A"),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.literal_number(1.0),
                name: None,
                optional: false,
                rest: false,
            },
        ]),
    ];

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    for (decl_idx, tuple_type) in variable_declarations_from_source(&parser, root)
        .into_iter()
        .zip(tuple_types)
    {
        let decl = parser
            .arena
            .get(decl_idx)
            .and_then(|node| parser.arena.get_variable_declaration(node))
            .expect("missing variable declaration");
        type_cache.node_types.insert(decl.initializer.0, tuple_type);
    }

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare let hello: string, brave: string;"),
        "Expected mutable string array binding literals to widen: {output}"
    );
    assert!(
        output.contains("declare let one: number, two: number;"),
        "Expected mutable number array binding literals to widen: {output}"
    );
    assert!(
        output.contains("declare let yes: boolean, no: boolean;"),
        "Expected mutable boolean array binding literals to widen: {output}"
    );
    assert!(
        output.contains("export declare let ma: string, mb: number;"),
        "Expected mutable mixed array binding literals to widen per binding: {output}"
    );
}

#[test]
fn test_const_asserted_array_literal_binding_preserves_literals() {
    let source = r#"let [hello, brave] = ["Hello", "Brave"] as const;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: interner.literal_string("Hello"),
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.literal_string("Brave"),
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let decl_idx = variable_declarations_from_source(&parser, root)
        .into_iter()
        .next()
        .expect("missing variable declaration");
    let decl = parser
        .arena
        .get(decl_idx)
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing variable declaration");
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(decl.initializer.0, tuple_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare let hello: \"Hello\", brave: \"Brave\";"),
        "Expected const-asserted array binding literals to stay literal: {output}"
    );
}

fn variable_declarations_from_source(parser: &ParserState, root: NodeIndex) -> Vec<NodeIndex> {
    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let mut declarations = Vec::new();
    for &stmt_idx in &source_file.statements.nodes {
        let Some(stmt_node) = parser.arena.get(stmt_idx) else {
            continue;
        };
        let variable_stmt_idx =
            if stmt_node.kind == tsz_parser::parser::syntax_kind_ext::EXPORT_DECLARATION {
                parser
                    .arena
                    .get_export_decl(stmt_node)
                    .map(|export| export.export_clause)
                    .unwrap_or(stmt_idx)
            } else {
                stmt_idx
            };
        let Some(stmt) = parser
            .arena
            .get(variable_stmt_idx)
            .and_then(|node| parser.arena.get_variable(node))
        else {
            continue;
        };
        for &decl_list_idx in &stmt.declarations.nodes {
            let Some(decl_list) = parser
                .arena
                .get(decl_list_idx)
                .and_then(|node| parser.arena.get_variable(node))
            else {
                continue;
            };
            declarations.extend(decl_list.declarations.nodes.iter().copied());
        }
    }
    declarations
}

#[test]
fn test_destructured_parameter_with_defaulted_property_uses_multiline_object_type() {
    let output = emit_dts("const k = ({ x: z = 'y' }) => {};");
    assert!(
        output.contains("declare const k: ({ x: z }: {\n    x?: string;\n}) => void;"),
        "Expected defaulted object binding parameter to emit a multiline object type: {output}"
    );
}

#[test]
fn test_destructured_parameter_defaulting_from_any_emits_any() {
    let output = emit_dts("var a; function f({ p: {} = a } = a) {}");
    assert!(
        output.contains("declare function f({ p: {} }?: any): void;"),
        "Expected destructured parameter defaulting from any to emit any: {output}"
    );
}

#[test]
fn test_returned_function_expression_preserves_destructured_typeof_alias_parameter() {
    let output = emit_dts(
        "type Named = { name: string }; function f({ name: alias }: Named) { return function(p: typeof alias) {} }",
    );
    assert!(
        output.contains("declare function f({ name: alias }: Named): (p: typeof alias) => void;"),
        "Expected returned function expression parameter to preserve typeof alias: {output}"
    );
}

#[test]
fn test_method_returning_non_null_null_widens_to_any() {
    let output = emit_dts(
        "type Named = { name: string }; class C { m({ name: alias }: Named, p: typeof alias) { return null!; } }",
    );
    assert!(
        output.contains("m({ name: alias }: Named, p: typeof alias): any;"),
        "Expected null! method return to emit any: {output}"
    );
}

#[test]
fn test_inferred_object_return_preserves_destructured_typeof_alias_member() {
    let output = emit_dts_with_binding(
        "type Named = { name: string }; function f({ name: alias }: Named) { type Named2 = { name: typeof alias }; return null! as Named2; }",
    );
    assert!(
        output
            .contains("declare function f({ name: alias }: Named): {\n    name: typeof alias;\n};"),
        "Expected asserted local alias return to preserve typeof destructured alias: {output}"
    );
}

#[test]
fn test_destructuring_parameter_properties_emit_individual_class_properties() {
    let source = "class C { constructor(public [x, y]: [string, number]) {} }";
    let (parser, root) = parse_test_source(source);
    let root_node = parser.arena.get(root).expect("missing root node");
    let stmt_idx = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file")
        .statements
        .nodes[0];
    let class_decl = parser
        .arena
        .get(stmt_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class declaration");
    let ctor_idx = class_decl.members.nodes[0];
    let ctor = parser
        .arena
        .get(ctor_idx)
        .and_then(|node| parser.arena.get_constructor(node))
        .expect("missing constructor");
    let param_idx = ctor.parameters.nodes[0];
    let param = parser
        .arena
        .get(param_idx)
        .and_then(|node| parser.arena.get_parameter(node))
        .expect("missing parameter");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(param.type_annotation.0, tuple_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("x: string;"),
        "Expected first destructured parameter property to be emitted: {output}"
    );
    assert!(
        output.contains("y: number;"),
        "Expected second destructured parameter property to be emitted: {output}"
    );
    assert!(
        !output.contains("[x, y]: [string, number];"),
        "Did not expect destructuring pattern to be emitted as a property name: {output}"
    );
}

// =============================================================================
// Method return type inference from arithmetic body expressions
// =============================================================================

#[test]
fn method_return_type_inferred_from_addition_of_number_properties() {
    let output = emit_dts_with_binding(
        r#"
class Calculator {
    public x: number;
    public add(b: number) {
        return this.x + b;
    }
}
"#,
    );
    assert!(
        output.contains("add(b: number): number;"),
        "Expected method return type to be inferred as number from this.x + b: {output}"
    );
}

#[test]
fn method_body_return_inference_survives_non_callable_cached_method_type() {
    let source = r#"
class Boxed {
    values() {
        return new Boxed();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let method_idx = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            (node.kind == syntax_kind_ext::METHOD_DECLARATION)
                .then_some(NodeIndex(idx as u32))
                .filter(|&method_idx| {
                    parser
                        .arena
                        .get(method_idx)
                        .and_then(|node| parser.arena.get_method_decl(node))
                        .and_then(|method| parser.arena.get_identifier_text(method.name))
                        == Some("values")
                })
        })
        .expect("missing values method");

    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(method_idx.0, TypeId::UNKNOWN);
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("values(): Boxed;"),
        "Expected body inference to recover the method return type when cache is non-callable: {output}"
    );
}

#[test]
fn method_return_type_inferred_from_subtraction() {
    let output = emit_dts_with_binding(
        r#"
class Calc {
    public value: number;
    public sub(b: number) {
        return this.value - b;
    }
}
"#,
    );
    assert!(
        output.contains("sub(b: number): number;"),
        "Expected method return type to be inferred as number from subtraction: {output}"
    );
}

#[test]
fn method_return_type_inferred_from_multiplication() {
    let output = emit_dts_with_binding(
        r#"
class Calc {
    public value: number;
    public mul(b: number) {
        return this.value * b;
    }
}
"#,
    );
    assert!(
        output.contains("mul(b: number): number;"),
        "Expected method return type to be inferred as number from multiplication: {output}"
    );
}

#[test]
fn static_method_return_type_inferred_from_addition() {
    let output = emit_dts_with_binding(
        r#"
class C {
    static s1: number;
    static add(b: number) {
        return C.s1 + b;
    }
}
"#,
    );
    assert!(
        output.contains("static add(b: number): number;"),
        "Expected static method return type to be inferred as number: {output}"
    );
}

#[test]
fn method_return_type_string_concatenation() {
    let output = emit_dts_with_binding(
        r#"
class Greeter {
    public name: string;
    public greet() {
        return "Hello, " + this.name;
    }
}
"#,
    );
    assert!(
        output.contains("greet(): string;"),
        "Expected method return type to be inferred as string from string concatenation: {output}"
    );
}

#[test]
fn reference_declared_type_annotation_resolves_property_declarations() {
    let output = emit_dts_with_binding(
        r#"
class Foo {
    public x: number;
    public getX() {
        return this.x;
    }
}
"#,
    );
    assert!(
        output.contains("getX(): number;"),
        "Expected method returning this.x to be inferred as number: {output}"
    );
}

#[test]
fn method_return_type_bitwise_operations_produce_number() {
    let output = emit_dts_with_binding(
        r#"
class BitOps {
    public a: number;
    public shiftLeft(n: number) {
        return this.a << n;
    }
    public bitwiseAnd(n: number) {
        return this.a & n;
    }
}
"#,
    );
    assert!(
        output.contains("shiftLeft(n: number): number;"),
        "Expected shift left to return number: {output}"
    );
    assert!(
        output.contains("bitwiseAnd(n: number): number;"),
        "Expected bitwise and to return number: {output}"
    );
}

// =============================================================================
// 23. Import-clause fallback heuristics (no usage tracking, e.g. --noCheck --noLib)
// =============================================================================

// Regression for #3337: with `--noCheck --noLib --declaration --emitDeclarationOnly`
// tsz dropped a regular default import even though the emitted `.d.ts` referenced
// the imported binding as a type. The fallback path must keep default imports for
// the same reason it keeps named imports — they may resolve a type reference in
// the declaration output.
#[test]
fn default_import_is_preserved_in_dts_fallback_when_used_as_type() {
    let output = emit_dts(
        r#"
import Foo from "./dep";
export let x: Foo;
"#,
    );

    assert!(
        output.contains(r#"import Foo from "./dep";"#),
        "Expected default import to be preserved in fallback dts emit: {output}"
    );
    assert!(
        output.contains("export declare let x: Foo;"),
        "Expected exported let to keep its `Foo` type annotation: {output}"
    );
}

#[test]
fn default_import_fallback_preserves_combined_default_and_named() {
    let output = emit_dts(
        r#"
import Foo, { Bar } from "./dep";
export let x: Foo;
export let y: Bar;
"#,
    );

    assert!(
        output.contains("Foo") && output.contains("Bar") && output.contains(r#""./dep""#),
        "Expected combined default + named imports to be preserved in fallback dts emit: {output}"
    );
}

#[test]
fn type_only_default_import_still_preserved_in_fallback() {
    let output = emit_dts(
        r#"
import type Foo from "./dep";
export let x: Foo;
"#,
    );

    assert!(
        output.contains("Foo") && output.contains(r#""./dep""#),
        "Expected type-only default import to still be preserved: {output}"
    );
}

#[test]
fn type_only_namespace_import_preserved_in_dts_fallback() {
    let output = emit_dts(
        r#"
import type * as ns from "./dep";
export interface Foo {
    x: string;
}
export type T = ns.Foo;
"#,
    );

    assert!(
        output.contains(r#"import type * as ns from "./dep";"#),
        "Expected type-only namespace import to be preserved: {output}"
    );
    assert!(
        output.contains("export type T = ns.Foo;"),
        "Expected exported type to reference preserved namespace import: {output}"
    );
}

#[test]
fn value_only_ambient_dependency_from_exported_initializer_is_elided() {
    let output = emit_dts_with_usage_analysis(
        r#"
declare const t: number;
export const out: number = t;
"#,
    );

    assert!(
        !output.contains("declare const t"),
        "Did not expect ambient initializer-only dependency to leak: {output}"
    );
    assert!(
        output.contains("export declare const out: number;"),
        "Expected exported declaration to remain: {output}"
    );
}

#[test]
fn ambient_value_dependency_used_in_exported_type_query_is_preserved() {
    let output = emit_dts_with_usage_analysis(
        r#"
declare const t: number;
export type T = typeof t;
"#,
    );

    assert!(
        output.contains("declare const t: number;"),
        "Expected ambient value referenced by exported typeof to remain: {output}"
    );
    assert!(
        output.contains("export type T = typeof t;"),
        "Expected exported type query to remain: {output}"
    );
}

#[test]
fn ambient_value_dependency_exported_by_specifier_is_preserved() {
    let output = emit_dts_with_usage_analysis(
        r#"
declare const t: number;
export { t };
"#,
    );

    assert!(
        output.contains("declare const t: number;"),
        "Expected ambient value exported by specifier to remain: {output}"
    );
    assert!(
        output.contains("export { t };"),
        "Expected export specifier to remain: {output}"
    );
}
