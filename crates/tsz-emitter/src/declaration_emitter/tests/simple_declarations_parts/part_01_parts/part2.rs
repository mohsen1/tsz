#[test]
fn test_js_function_declaration_type_alias_signature_preserves_non_type_jsdoc_comments() {
    let output = emit_js_dts(
        r#"
/**
 * @typedef {<T>(m : T) => T} IFn
 */

/**
 * Keep this function-level JSDoc.
 * @deprecated use next
 */
/** @type {IFn} */
export function inJs(l) {
  return l;
}
"#,
    );

    assert!(
        output.contains("export function inJs<T>(m: T): T;"),
        "Expected JSDoc @type function alias to emit as a function signature: {output}"
    );
    assert!(
        output.contains("@deprecated use next"),
        "Expected non-@type JSDoc comments to remain in declaration output: {output}"
    );
    assert!(
        !output.contains("@type {IFn}"),
        "Did not expect implementation-only @type comment in declaration output: {output}"
    );
}

#[test]
fn test_js_named_exports_fold_into_declarations() {
    let source = r#"
const x = 1;
function f() {}
export { x, f };
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export const x: 1;"),
        "Expected named-exported const to fold into an exported declaration: {output}"
    );
    assert!(
        output.contains("export function f(): void;"),
        "Expected named-exported function to fold into an exported declaration: {output}"
    );
    assert!(
        !output.contains("export { x, f };"),
        "Did not expect a redundant named export clause after folding: {output}"
    );
}

#[test]
fn test_js_named_export_interface_folds_into_declaration() {
    let source = r#"
interface G {}
export { G };
interface HH {}
export { HH as H };
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert!(
        output.contains("export interface G"),
        "Expected same-name JS interface export to fold into the declaration: {output}"
    );
    assert!(
        output.contains("interface HH"),
        "Expected renamed interface alias to keep its local declaration: {output}"
    );
    assert!(
        output.contains("export { HH as H };"),
        "Expected renamed interface alias to remain in the grouped export aliases: {output}"
    );
    assert!(
        !output.contains("export { G"),
        "Did not expect a redundant same-name export alias for G: {output}"
    );
}

#[test]
fn test_js_mixed_named_export_partitions_interface_and_value_specifiers() {
    let source = r#"
interface G {}
interface H {}
const x = 1;
export { G, H as HH, x };
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    let g_pos = output
        .find("export interface G")
        .expect("expected same-name interface export to fold into declaration");
    let h_pos = output
        .find("interface H")
        .expect("expected renamed interface to keep a local declaration");
    let x_pos = output
        .find("export const x: 1;")
        .expect("expected same-name value export to fold into declaration");
    let alias_pos = output
        .find("export { H as HH };")
        .expect("expected renamed interface alias to remain in trailing export aliases");

    assert!(
        g_pos < h_pos && h_pos < x_pos && x_pos < alias_pos,
        "Expected mixed export specifiers to be partitioned in tsc order: {output}"
    );
    assert!(
        !output.contains("export { G, H as HH, x };"),
        "Did not expect the original mixed export clause to be emitted: {output}"
    );
}

#[test]
fn test_js_interface_recovery_orders_construct_call_then_members() {
    let source = r#"
export interface C<T, U> {
    field: T & U;
    (): number;
    (x: T): U;
    new (): string;
    new (x: T): U;
    method(): number;
    optMethod?(): number;
}
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    let construct_pos = output
        .find("new (): string;")
        .unwrap_or_else(|| panic!("missing construct signature: {output}"));
    let call_pos = output
        .find("(): number;")
        .unwrap_or_else(|| panic!("missing call signature: {output}"));
    let field_pos = output
        .find("field: T & U;")
        .unwrap_or_else(|| panic!("missing field: {output}"));
    let method_pos = output
        .find("method(): number;")
        .unwrap_or_else(|| panic!("missing method: {output}"));

    assert!(
        construct_pos < call_pos && call_pos < field_pos && field_pos < method_pos,
        "Expected JS interface recovery to order construct signatures, call signatures, then source-order members: {output}"
    );
    assert!(
        !output.contains("optMethod"),
        "Expected optional JS recovered interface methods to be omitted like tsc: {output}"
    );
}

#[test]
fn test_js_named_export_function_preserves_jsdoc_signature_at_export_position() {
    let output = emit_js_dts(
        r#"
export function b() {}

/**
 * @param {{x: string}} a
 * @param {{y: typeof b}} b
 */
function g(a, b) {
    return a.x && b.y();
}

export { g };
"#,
    );

    assert!(
        output.contains("export function g(a: {\n    x: string;\n}, b: {\n    y: typeof import(\".\").b;\n}): void | \"\";"),
        "Expected folded JS export function to preserve JSDoc param and return types: {output}"
    );
    assert_eq!(
        output.matches("export function g(").count(),
        1,
        "Expected folded JS export function to be emitted once: {output}"
    );
    assert!(
        output.contains(
            "/**\n * @param {{x: string}} a\n * @param {{y: typeof b}} b\n */\nexport function g"
        ),
        "Expected folded JS export function to keep its JSDoc comment: {output}"
    );
}

#[test]
fn test_js_named_exports_preserve_explicit_export_order() {
    let source = r#"
function require() {}
const exports = {};
class Object {}
export const __esModule = false;
export { require, exports, Object };
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"export const __esModule: false;
export function require(): void;
export const exports: {};
export class Object {
}"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected explicit JS exports to stay ahead of folded named exports: {output}"
    );
}

#[test]
fn test_js_namespace_named_export_keeps_required_constructor_import_type() {
    let source = r#"
export const Something = 2;
export namespace A {
    export namespace B {
        const Something = require("fs").Something;
        const thing = new Something();
        export { thing };
    }
}
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert!(
        output.contains("export namespace A {\n    namespace B {\n        export { thing };\n        export let thing: import(\"fs\").Something;\n    }\n}"),
        "Expected namespace named export to emit a reusable import type after its export clause: {output}"
    );
}

#[test]
fn test_js_module_exports_object_uses_require_property_import_alias() {
    let source = r#"
const Something = require("fs").Something;
const thing = new Something();
module.exports = {
    thing
};
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert_eq!(
        output.trim(),
        "export const thing: Something;\nimport Something_1 = require(\"fs\");\nimport Something = Something_1.Something;"
    );
}

#[test]
fn test_js_module_exports_object_prefers_require_property_alias_over_inferred_type() {
    let source = r#"
const Something = require("fs").Something;
const thing = new Something();
module.exports = {
    thing
};
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    for (index, node) in parser.arena.nodes.iter().enumerate() {
        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && parser
                .arena
                .get_identifier(node)
                .is_some_and(|ident| ident.escaped_text == "thing")
        {
            type_cache.node_types.insert(index as u32, TypeId::STRING);
        }
    }
    let current_arena = Arc::new(parser.arena.clone());

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(current_arena, "test.js".to_string());
    let output = emitter.emit(root);

    assert_eq!(
        output.trim(),
        "export const thing: Something;\nimport Something_1 = require(\"fs\");\nimport Something = Something_1.Something;"
    );
}

#[test]
fn test_commonjs_export_elides_type_only_require_destructuring() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
const { Thing } = require("./module.js");

/** @typedef {import("./module.js").Thing} Thing */
class Main {
    /** @param {Thing} x */
    constructor(x) {}
}

module.exports = Main;
"#,
    );

    assert!(
        !output.contains("declare const Thing:"),
        "Expected type-only require destructuring to be elided: {output}"
    );
    assert!(
        output.contains("export = Main;"),
        "Expected CommonJS export assignment to remain: {output}"
    );
    assert!(
        output.contains("constructor(x: Thing);"),
        "Expected JSDoc import typedef to keep constructor parameter type: {output}"
    );
    assert!(
        output.contains("type Thing = import(\"./module.js\").Thing;"),
        "Expected public typedef to use import type instead of the local require binding: {output}"
    );
}
