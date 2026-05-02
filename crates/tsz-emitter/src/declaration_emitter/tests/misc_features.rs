use super::*;

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
fn test_inferred_printer_reduces_conditional_alias_applications() {
    use tsz_solver::types::{ConditionalType, TypeParamInfo};

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let _ = parser.parse_source_file();

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
fn test_destructuring_variable_declaration_groups_typed_bindings() {
    let source = r#"var [x, y] = [1, "hello"];"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
