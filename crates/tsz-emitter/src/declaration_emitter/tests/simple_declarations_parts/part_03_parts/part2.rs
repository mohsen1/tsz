#[test]
fn test_js_commonjs_class_static_assignments_emit_typedef_and_namespace_exports() {
    let source = r#"
class Handler {
    static get OPTIONS() {
        return 1;
    }

    process() {
    }
}
Handler.statische = function() { }
const Strings = {
    a: "A",
    b: "B"
};

module.exports = Handler;
module.exports.Strings = Strings;

/**
 * @typedef {Object} HandlerOptions
 * @property {String} name
 * Should be able to export a type alias at the same time.
 */
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"export = Handler;
declare class Handler {
    static get OPTIONS(): number;
    process(): void;
}
declare namespace Handler {
    export { statische, Strings, HandlerOptions };
}
declare function statische(): void;
declare namespace Strings {
    let a: string;
    let b: string;
}
type HandlerOptions = {
    /**
     * Should be able to export a type alias at the same time.
     */
    name: string;
};"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected CommonJS class static assignments and typedefs to emit in source order: {output}"
    );
}

#[test]
fn test_jsdoc_property_typedef_quotes_non_identifier_names() {
    let source = r#"
/**
 * @typedef {Object} Options
 * @property {String} data-id
 * @property {Number} [max-count]
 */
exports.value = {};
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("\"data-id\": string;"),
        "Expected hyphenated JSDoc property name to be quoted: {output}"
    );
    assert!(
        output.contains("\"max-count\"?: number;"),
        "Expected optional hyphenated JSDoc property name to be quoted before ?: {output}"
    );
}

#[test]
fn test_jsdoc_property_typedef_preserves_alias_description() {
    let source = r#"
/**
 * Options for Foo.
 * @typedef {Object} FooOptions
 * @property {boolean} bar - Enables bar.
 */
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("/**\n * Options for Foo.\n */\ntype FooOptions = {"),
        "Expected typedef description to be preserved above the type alias: {output}"
    );
    assert!(
        output.contains("/**\n     * - Enables bar.\n     */\n    bar: boolean;"),
        "Expected property description to remain on the property: {output}"
    );
}

#[test]
fn test_jsdoc_typedef_same_line_link_description_is_preserved() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @typedef {Object} D1
 * @property {1} e Just link to {@link NS.R} this time
 * @property {1} m Wyatt Earp loved {@link N integers} I bet.
 */

/** @typedef {number} Attempt {@link https://wat} {@linkcode I think lingcod is better} {@linkplain or lutefisk}*/
"#,
    );

    assert!(
        output.contains(
            "/**\n * {@link https://wat} {@linkcode I think lingcod is better} {@linkplain or lutefisk}\n */\ntype Attempt = number;"
        ),
        "Expected same-line typedef link text to become alias JSDoc: {output}"
    );
    assert!(
        output.contains("/**\n     * Just link to {@link NS.R} this time\n     */\n    e: 1;"),
        "Expected property link tags to remain on object typedef members: {output}"
    );
    assert!(
        output
            .contains("/**\n     * Wyatt Earp loved {@link N integers} I bet.\n     */\n    m: 1;"),
        "Expected renamed-link property text to remain on object typedef members: {output}"
    );
}

#[test]
fn test_jsdoc_typedef_same_line_plain_description_is_preserved() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * Leading alias sentence.
 * @typedef {string} RenamedAlias trailing alias sentence.
 */
"#,
    );

    assert!(
        output.contains(
            "/**\n * Leading alias sentence.\n * trailing alias sentence.\n */\ntype RenamedAlias = string;"
        ),
        "Expected leading and same-line typedef descriptions to be preserved: {output}"
    );
}

#[test]
fn test_js_class_static_method_augmentation_emits_namespace_merge() {
    let source = r#"
export class Clazz {
    static method() { }
}

Clazz.method.prop = 5;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"export class Clazz {
}
export namespace Clazz {
    function method(): void;
    namespace method {
        let prop: number;
    }
}"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected JS static method augmentations to emit as a merged namespace: {output}"
    );
}

#[test]
fn test_js_reexports_from_same_module_are_grouped() {
    let source = r#"
export { default } from "fs";
export { default as foo } from "fs";
export { bar as baz } from "fs";
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export { default, default as foo, bar as baz } from \"fs\";"),
        "Expected JS re-exports from the same module to be grouped: {output}"
    );
    assert_eq!(
        output.matches(" from \"fs\";").count(),
        1,
        "Did not expect duplicate JS re-export lines after grouping: {output}"
    );
}

#[test]
fn test_method_declaration_emits_inferred_return_type() {
    let source = r#"
class C {
    add() {
        return 1;
    }
}
"#;
    let (parser, root) = parse_test_source(source);

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file data");
    };
    let Some(class_node) = parser.arena.get(source_file.statements.nodes[0]) else {
        panic!("missing class node");
    };
    let Some(class_decl) = parser.arena.get_class(class_node) else {
        panic!("missing class declaration");
    };
    let method_idx = class_decl.members.nodes[0];

    let interner = TypeInterner::new();
    let method_type = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(method_idx.0, method_type);

    let binder = BinderState::new();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("add(): number;"),
        "Expected inferred method return type: {output}"
    );
}

#[test]
fn test_property_declaration_infers_type_from_numeric_initializer_when_type_cache_missing() {
    let source = r#"
abstract class C {
    abstract prop = 1;
}
"#;
    let (parser, root) = parse_test_source(source);

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("abstract prop: number;"),
        "Expected inferred property type from initializer: {output}"
    );
}

#[test]
fn test_variable_declaration_infers_accessor_object_type_from_initializer_when_type_cache_missing()
{
    let source = r#"
export var basePrototype = {
  get primaryPath() {
    return 1;
  },
};
"#;
    let (parser, root) = parse_test_source(source);

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output
            .contains("export declare var basePrototype: {\n    readonly primaryPath: number;\n};"),
        "Expected multi-line object literal accessor inference with body type: {output}"
    );
}

#[test]
fn test_call_initializer_uses_source_function_return_shape_for_accessor_object() {
    let output = emit_dts_with_binding(
        r#"
function makePoint(x: number) {
    return {
        b: 10,
        get x() { return x; },
        set x(a: number) { this.b = a; }
    };
}
var /*4*/ point = makePoint(2);
point./*3*/x = 30;
"#,
    );

    assert!(
        output.contains("declare var /*4*/ point: {\n    b: number;\n    x: any;\n};")
            || output.contains("declare var /*4*/ point: {\n    b: number;\n    x: number;\n};"),
        "Expected call initializer to reuse source function return shape without synthetic anonymous members: {output}"
    );
    assert!(
        !output.contains("\n    : {"),
        "Did not expect a synthetic anonymous object member in call initializer output: {output}"
    );
}
