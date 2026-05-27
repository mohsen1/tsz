#[test]
fn test_js_multiple_leading_jsdoc_typedefs_before_function_all_emitted_first() {
    // Multiple @typedef comments before an exported function should all appear before the function.
    let output = emit_js_dts(
        r#"
/** @typedef {string} Key */
/** @typedef {number} Value */
/**
 * @param {Key} k
 * @returns {Value}
 */
export function lookup(k) {
  return 1;
}
"#,
    );

    assert!(
        output.contains("export type Key = string;"),
        "Expected Key typedef alias: {output}"
    );
    assert!(
        output.contains("export type Value = number;"),
        "Expected Value typedef alias: {output}"
    );
    let key_pos = output
        .find("export type Key =")
        .expect("Expected Key typedef");
    let value_pos = output
        .find("export type Value =")
        .expect("Expected Value typedef");
    let function_pos = output
        .find("export function lookup(")
        .expect("Expected lookup function");
    assert!(
        key_pos < function_pos,
        "Expected Key typedef before function: {output}"
    );
    assert!(
        value_pos < function_pos,
        "Expected Value typedef before function: {output}"
    );
}

#[test]
fn test_js_leading_jsdoc_typedef_not_emitted_as_raw_comment_before_function() {
    // The raw @typedef JSDoc block should NOT appear in the output when the typedef
    // alias has been emitted as export type — only the @param block should appear.
    let output = emit_js_dts(
        r#"
/** @typedef {string | null} OptStr */
/**
 * @param {OptStr} s
 */
export function process(s) {}
"#,
    );

    // The typedef comment must NOT appear as a raw JSDoc block before the function.
    assert!(
        !output.contains("/** @typedef {string | null} OptStr */"),
        "Raw @typedef comment should be suppressed when emitted as export type: {output}"
    );
    // The type alias must appear before the function.
    let alias_pos = output
        .find("export type OptStr =")
        .expect("Expected OptStr typedef alias");
    let function_pos = output
        .find("export function process(")
        .expect("Expected process function");
    assert!(
        alias_pos < function_pos,
        "Expected typedef alias before function: {output}"
    );
}

#[test]
fn test_js_script_typedef_before_variable_is_emitted_as_local_type() {
    let source = r#"
/** @typedef {{x: string}} LocalType */
const value = 1;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("type LocalType = {\n    x: string;\n};"),
        "Expected script typedef before variable statement to be emitted as a local type alias: {output}"
    );
    assert!(
        !output.contains("export type LocalType"),
        "Did not expect script typedef to be emitted as an exported type alias: {output}"
    );
}

#[test]
fn test_js_multiline_typedef_before_function_variable_is_emitted() {
    let source = r#"
/**
 * @typedef {{
 *   [id: string]: [Function, Function];
 * }} ResolveRejectMap
 */
/**
 * @param {ResolveRejectMap} handlers
 * @returns {Promise<any>}
 */
const send = handlers => Promise.resolve(handlers);
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function send(handlers: ResolveRejectMap): Promise<any>;"),
        "Expected JSDoc-annotated JS function variable to emit as a function declaration: {output}"
    );
    assert!(
        output.contains(
            "/**\n * @param {ResolveRejectMap} handlers\n * @returns {Promise<any>}\n */\ndeclare function send"
        ),
        "Expected function variable JSDoc to stay attached to synthetic declaration: {output}"
    );
    assert!(
        output.contains("type ResolveRejectMap = {\n    [id: string]: [Function, Function];\n};"),
        "Expected multiline JSDoc typedef alias to be emitted as a local type alias: {output}"
    );
}

#[test]
fn test_js_exported_arrow_without_jsdoc_emits_function_declaration() {
    let output = emit_js_dts_with_usage_analysis("export const id = (value) => value;");

    assert!(
        output.contains("export function id(value: any);"),
        "Expected exported JS arrow variable to emit as a function declaration: {output}"
    );
    assert!(
        !output.contains("export const id:"),
        "Did not expect exported JS arrow variable to stay as a const function type: {output}"
    );
}

#[test]
fn test_js_exported_jsx_arrow_destructured_computed_param_uses_literal_key() {
    let source = r#"
const dynPropName = "data-dyn";
export const ExampleFunctionalComponent = ({ "data-testid": dataTestId, [dynPropName]: dynProp }) => (
    <>Hello</>
);
"#;
    let mut parser = ParserState::new("test.jsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (_decl_idx, decl) = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            parser
                .arena
                .get_variable_declaration(node)
                .filter(|decl| {
                    parser.arena.get_identifier_text(decl.name)
                        == Some("ExampleFunctionalComponent")
                })
                .map(|decl| (NodeIndex(idx as u32), decl))
        })
        .expect("missing declaration");
    let init_node = parser
        .arena
        .get(decl.initializer)
        .expect("missing arrow initializer");
    let func = parser
        .arena
        .get_function(init_node)
        .expect("missing arrow function");
    let param_idx = func.parameters.nodes[0];
    let param = parser
        .arena
        .get(param_idx)
        .and_then(|node| parser.arena.get_parameter(node))
        .expect("missing parameter");
    let pattern = parser
        .arena
        .get(param.name)
        .and_then(|node| parser.arena.get_binding_pattern(node))
        .expect("missing object binding pattern");
    let computed_expr = parser
        .arena
        .get(pattern.elements.nodes[1])
        .and_then(|node| parser.arena.get_binding_element(node))
        .and_then(|element| parser.arena.get(element.property_name))
        .and_then(|node| parser.arena.get_computed_property(node))
        .map(|computed| computed.expression)
        .expect("missing computed binding property");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let data_testid = interner.intern_string("data-testid");
    let data_dyn = interner.intern_string("data-dyn");
    let param_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![
            PropertyInfo::new(data_testid, TypeId::ANY),
            PropertyInfo::new(data_dyn, TypeId::ANY),
        ],
        string_index: None,
        number_index: None,
        symbol: None,
    });
    let func_type = interner.function(FunctionShape::new(
        vec![ParamInfo::required(
            interner.intern_string("__0"),
            param_type,
        )],
        TypeId::ANY,
    ));
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(computed_expr.0, interner.literal_string("data-dyn"));
    type_cache.node_types.insert(decl.initializer.0, func_type);

    let current_arena = Arc::new(parser.arena.clone());
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(current_arena, "test.jsx".to_string());
    let output = emitter.emit(root);

    assert!(
        output.contains("export function ExampleFunctionalComponent"),
        "Expected exported JS arrow component to emit as a function declaration: {output}"
    );
    assert!(
        output.contains("\"data-dyn\": any;"),
        "Expected computed binding key to use its resolved literal property name: {output}"
    );
    assert!(
        output.contains("declare const dynPropName: \"data-dyn\";"),
        "Expected computed binding key declaration to remain nameable: {output}"
    );
    assert!(
        output.contains("): JSX.Element;"),
        "Expected concise JSX arrow return to emit JSX.Element: {output}"
    );
}

#[test]
fn test_js_default_exported_arrow_component_emits_function_namespace_members() {
    let source = r#"
/// <reference path="/.lib/react16.d.ts" preserve="true" />
import PropTypes from "prop-types";

const Widget = ({
}) => {
    return <div />;
};

Widget.propTypes = {
    count: PropTypes.number,
};

Widget.defaultProps = {
    tabs: undefined,
};

export default Widget;
"#;
    let mut parser = ParserState::new("test.jsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let current_arena = Arc::new(parser.arena.clone());

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(current_arena, "test.jsx".to_string());
    let output = emitter.emit(root);

    assert!(
        output.contains("/// <reference path=\"../../.lib/react16.d.ts\" preserve=\"true\" />"),
        "Expected preserved harness .lib references to be relativized for declaration emit: {output}"
    );
    assert!(
        output.contains("export default Widget;\ndeclare function Widget({}: {}): JSX.Element;"),
        "Expected default-exported JS arrow component to emit as a function declaration: {output}"
    );
    assert!(
        output.contains(
            "declare namespace Widget {\n    namespace propTypes {\n        let count: PropTypes.Requireable<number>;\n    }\n    namespace defaultProps {\n        let tabs: undefined;\n    }\n}"
        ),
        "Expected component static object assignments to emit as nested namespaces: {output}"
    );
    assert!(
        output.contains("import PropTypes from \"prop-types\";"),
        "Expected PropTypes import to be preserved when propTypes validators are emitted: {output}"
    );
}

#[test]
fn test_js_exported_computed_param_key_does_not_emit_synthetic_duplicate() {
    let output = emit_js_dts_with_usage_analysis(
        "export const key = \"x\";\nexport const f = ({ [key]: value }) => value;\n",
    );

    assert!(
        output.contains("export const key: \"x\";"),
        "Expected exported computed key const to emit through the normal export path: {output}"
    );
    assert!(
        output.contains("export function f({ [key]: value }"),
        "Expected exported JS arrow variable to emit as a function using the exported computed key: {output}"
    );
    assert!(
        output.contains("x: any;"),
        "Expected computed binding key to resolve to the literal property name: {output}"
    );
    assert_eq!(
        output.matches("const key:").count(),
        1,
        "Did not expect a duplicate synthetic local declaration for an already exported computed key: {output}"
    );
}

#[test]
fn test_js_multiline_typedef_before_variable_comment_is_preserved() {
    let source = r#"
/**
 * @typedef {{
 *   [id: string]: [Function, Function];
 * }} ResolveRejectMap
 */
let id = 0;

/**
 * @param {ResolveRejectMap} handlers
 * @returns {Promise<any>}
 */
const send = handlers => Promise.resolve(handlers);
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.starts_with(
            "/**\n * @typedef {{\n * [id: string]: [Function, Function];\n * }} ResolveRejectMap\n */\ndeclare let id: number;"
        ),
        "Expected source typedef comment to stay attached to the variable declaration: {output}"
    );
    assert!(
        output.contains("declare function send(handlers: ResolveRejectMap): Promise<any>;"),
        "Expected function variable to use the typedef alias: {output}"
    );
    assert!(
        output.contains("type ResolveRejectMap = {\n    [id: string]: [Function, Function];\n};"),
        "Expected typedef alias to still be emitted: {output}"
    );
}

#[test]
fn test_js_multiline_typedef_preserves_unstarred_source_lines() {
    let output = emit_js_dts(
        r#"
/**
 * @template T
 * @typedef {{
  value: {
    [K in keyof T]?: Box<T[K]>[]
  }
}} Box<T> */
/** @type {Box<{foo:string}>} */
const p = {};
"#,
    );

    // tsc joins single-line @type comments to the following declaration even when
    // they appear on a separate source line, so check for the joined form.
    assert!(
        output.starts_with(
            "/**\n * @template T\n * @typedef {{\n  value: {\n    [K in keyof T]?: Box<T[K]>[]\n  }\n}} Box<T> */\n/** @type {Box<{foo:string}>} */ declare const p: Box<{"
        ),
        "Expected unstarred typedef lines and following @type comment to preserve source text: {output}"
    );
    assert!(
        output.contains("type Box<T> = {\n    value: { [K in keyof T]?: Box<T[K]>[]; };\n};"),
        "Expected generic typedef name suffix to be folded into type parameters: {output}"
    );
}

#[test]
fn test_js_multiline_typedef_before_export_equals_function_variable_is_emitted() {
    let source = r#"
/**
 * @typedef {{
 *   [id: string]: [Function, Function];
 * }} ResolveRejectMap
 */
/**
 * @param {ResolveRejectMap} handlers
 * @returns {Promise<any>}
 */
const send = handlers => Promise.resolve(handlers);
module.exports = send;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let export_pos = output
        .find("export = send;")
        .expect("Expected CommonJS export-equals statement");
    let function_pos = output
        .find("declare function send(handlers: ResolveRejectMap): Promise<any>;")
        .expect("Expected synthetic function declaration for send");
    assert!(
        export_pos < function_pos,
        "Expected export= send to emit before the synthetic declaration in CommonJS mode: {output}"
    );
    assert!(
        output.contains("type ResolveRejectMap = {\n    [id: string]: [Function, Function];\n};"),
        "Expected multiline JSDoc typedef alias to be emitted alongside export= send: {output}"
    );
    assert_eq!(
        output.matches("export = send;").count(),
        1,
        "Did not expect duplicate export= send statements: {output}"
    );
}

#[test]
fn test_js_require_json_export_equals_infers_json_shape() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("tsz-json-require-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp json fixture dir");
    std::fs::write(
        dir.join("package.json"),
        r#"{
  "name": "pkg",
  "bin": {
    "cli": "./bin/cli.js",
  },
  "devDependencies": {
    "@ns/dep": "0.1.2"
  },
  "keywords": ["kw"],
  "config": {
    "o": ["a"]
  }
}"#,
    )
    .expect("write json fixture");

    let source = r#"
const j = require("./package.json");
module.exports = j;
"#;
    let index_path = dir.join("index.js");
    let mut parser = ParserState::new(
        index_path.to_string_lossy().into_owned(),
        source.to_string(),
    );
    let root = parser.parse_source_file();
    let current_arena = Arc::new(parser.arena.clone());
    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.set_current_arena(current_arena, index_path.to_string_lossy().into_owned());

    let output = emitter.emit(root);
    let _ = std::fs::remove_dir_all(&dir);

    let expected = r#"export = j;
declare const j: {
    name: string;
    bin: {
        cli: string;
    };
    devDependencies: {
        "@ns/dep": string;
    };
    keywords: string[];
    config: {
        o: string[];
    };
};
"#;
    assert_eq!(
        output, expected,
        "Expected CommonJS JSON require exports to infer the JSON data shape"
    );
}

#[test]
fn test_js_require_json_array_object_union_completes_sibling_properties() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "tsz-json-require-array-union-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create temp json fixture dir");
    std::fs::write(
        dir.join("data.json"),
        r#"{
  "items": [
    { "x": 12 },
    { "x": 12, "y": 12 },
    { "x": -1, "err": true }
  ]
}"#,
    )
    .expect("write json fixture");

    let source = r#"
const data = require("./data.json");
module.exports = data;
"#;
    let index_path = dir.join("index.js");
    let mut parser = ParserState::new(
        index_path.to_string_lossy().into_owned(),
        source.to_string(),
    );
    let root = parser.parse_source_file();
    let current_arena = Arc::new(parser.arena.clone());
    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.set_current_arena(current_arena, index_path.to_string_lossy().into_owned());

    let output = emitter.emit(root);
    let _ = std::fs::remove_dir_all(&dir);

    let expected = r#"items: ({
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
    })[];"#;
    assert!(
        output.contains(expected),
        "Expected JSON object-array union arms to include optional undefined siblings: {output}"
    );
}

#[test]
fn test_js_typedef_before_export_equals_function_declaration_stays_local() {
    let output = emit_js_dts(
        r#"
/** @typedef {string | number} Value */
/**
 * @param {Value} value
 * @returns {Value}
 */
function make(value) {
    return value;
}
module.exports = make;
"#,
    );

    let export_pos = output
        .find("export = make;")
        .expect("Expected CommonJS export assignment");
    let comment_pos = output
        .find("/** @typedef {string | number} Value */")
        .expect("Expected source typedef comment before function");
    let function_pos = output
        .find("declare function make(value: Value): Value;")
        .expect("Expected function declaration to use the typedef alias");
    let alias_pos = output
        .find("type Value = string | number;")
        .expect("Expected local typedef alias");

    assert!(
        export_pos < comment_pos && comment_pos < function_pos && function_pos < alias_pos,
        "Expected export= first, source typedef comment on the function, and local alias after it: {output}"
    );
    assert!(
        !output.contains("export type Value"),
        "CommonJS export= typedef alias should stay local: {output}"
    );
}

#[test]
fn test_js_typedef_before_export_equals_class_declaration_stays_local() {
    let output = emit_js_dts(
        r#"
/**
 * @typedef {string | number} Whatever
 */
class Conn {
    constructor() {}
    item = 3;
    method() {}
}
module.exports = Conn;
"#,
    );

    let export_pos = output
        .find("export = Conn;")
        .expect("Expected CommonJS export assignment");
    let comment_pos = output
        .find("/**\n * @typedef {string | number} Whatever\n */")
        .expect("Expected source typedef comment before class");
    let class_pos = output
        .find("declare class Conn")
        .expect("Expected class declaration");
    let namespace_pos = output
        .find("declare namespace Conn")
        .expect("Expected merged namespace declaration");
    let alias_pos = output
        .find("type Whatever = string | number;")
        .expect("Expected local typedef alias");

    assert!(
        export_pos < comment_pos
            && comment_pos < class_pos
            && class_pos < namespace_pos
            && namespace_pos < alias_pos,
        "Expected export=, commented class, namespace, then local alias: {output}"
    );
    assert!(
        output.contains("export { Whatever };"),
        "Expected namespace to re-export the local typedef alias: {output}"
    );
    assert!(
        !output.contains("export type Whatever"),
        "CommonJS export= typedef alias should stay local: {output}"
    );
}

#[test]
fn test_js_typedef_before_private_function_in_es_module_stays_exported() {
    let output = emit_js_dts(
        r#"
/** @typedef {string | number} Value */
function local(value) { return value; }
export const x = 1;
"#,
    );

    assert!(
        output.contains("export const x: 1;"),
        "Expected sibling ES export to stay exported: {output}"
    );
    assert!(
        output.contains("export type Value = string | number;"),
        "Expected top-level typedef before a private JS function to follow ES module export policy: {output}"
    );
    assert!(
        !output.contains("\ntype Value = string | number;"),
        "Did not expect ES module typedef to be consumed early as a local alias: {output}"
    );
}

#[test]
fn test_js_typedef_before_private_class_in_es_module_stays_exported() {
    let output = emit_js_dts(
        r#"
/** @typedef {string | number} Value */
class Local {
    method() {}
}
export const x = 1;
"#,
    );

    assert!(
        output.contains("export const x: 1;"),
        "Expected sibling ES export to stay exported: {output}"
    );
    assert!(
        output.contains("export type Value = string | number;"),
        "Expected top-level typedef before a private JS class to follow ES module export policy: {output}"
    );
    assert!(
        !output.contains("\ntype Value = string | number;"),
        "Did not expect ES module typedef to be consumed early as a local alias: {output}"
    );
}

#[test]
fn test_js_commonjs_named_object_export_emits_typedef_aliases_first() {
    let output = emit_js_dts(
        r#"
/** @typedef {'alpha'|'beta'} GroupIds */

/**
 * @typedef Group
 * @property {GroupIds} id
 * @property {string[]} names
 */

/** @type {{[P in GroupIds]: {id: P, label: string}}} */
const groups = {
    alpha: { id: 'alpha', label: 'Alpha' },
    beta: { id: 'beta', label: 'Beta' },
};

/** @type {Object<string, Group>} */
const nameToGroup = {};

module.exports = { groups, nameToGroup };
"#,
    );

    let ids_pos = output
        .find("export type GroupIds = \"alpha\" | \"beta\";")
        .expect("Expected exported GroupIds alias");
    let group_pos = output
        .find("export type Group = {")
        .expect("Expected exported Group alias");
    let groups_pos = output
        .find("export const groups: { [P in GroupIds]: {\n    id: P;\n    label: string;\n}; };")
        .expect("Expected mapped JSDoc object type to match tsc shape");
    let map_pos = output
        .find("export const nameToGroup: {\n    [x: string]: Group;\n};")
        .expect("Expected exported Object<string, Group> index signature");

    assert!(
        ids_pos < group_pos && group_pos < groups_pos && groups_pos < map_pos,
        "Expected exported typedef aliases before CommonJS named values: {output}"
    );
    assert!(
        output.contains("/** @typedef {'alpha'|'beta'} GroupIds */"),
        "Expected source typedef comments to remain attached to the original JS declarations: {output}"
    );
}

#[test]
fn test_js_commonjs_property_export_emits_typedef_aliases_first() {
    let output = emit_js_dts(
        r#"
/** @typedef {string | number} Token */
const value = "x";
exports.value = value;
"#,
    );

    let alias_pos = output
        .find("export type Token = string | number;")
        .expect("Expected CommonJS named export file to export the typedef alias");
    let value_pos = output
        .find("export const value: \"x\";")
        .expect("Expected named CommonJS value export");

    assert!(
        alias_pos < value_pos,
        "Expected typedef alias before direct CommonJS named export: {output}"
    );
}

#[test]
fn test_js_function_declaration_uses_jsdoc_signature_types() {
    let source = r#"
/**
 * @param {number} x
 * @returns {string}
 */
function format(x) {
  return String(x);
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function format(x: number): string;"),
        "Expected JSDoc function declaration types to flow into .d.ts emit: {output}"
    );
}

#[test]
fn test_js_function_declaration_emits_separate_jsdoc_overload_comments() {
    let output = emit_js_dts(
        r#"
/**
 * @overload
 * @param {number} value
 * @returns {'number'}
 */
/**
 * @overload
 * @param {string} value
 * @returns {'string'}
 */
/**
 * @param {unknown} value
 * @returns {string}
 */
function kind(value) {
  return typeof value;
}

/**
 * @template T
 * @param {T} value
 * @returns {T}
 */
const identity = value => value;

/**
 * @template T
 * @overload
 * @param {T[]} values
 * @returns {T[]}
 */
/**
 * @param {unknown[]} values
 * @returns {unknown[]}
 */
function copy(values) {
  return values.map(identity);
}
"#,
    );

    let kind_number = output
        .find("declare function kind(value: number): \"number\";")
        .expect("expected number overload");
    let kind_string = output
        .find("declare function kind(value: string): \"string\";")
        .expect("expected string overload");
    let copy = output
        .find("declare function copy<T>(values: T[]): T[];")
        .expect("expected generic overload");
    let identity = output
        .find("declare function identity<T>(value: T): T;")
        .expect("expected variable function declaration");

    assert!(
        kind_number < kind_string && kind_string < copy && copy < identity,
        "Expected JS function overloads to stay in function source order before function variables: {output}"
    );
    assert!(
        !output.contains("declare function kind(value: unknown): string;"),
        "Implementation signature should not be emitted for @overload JSDoc: {output}"
    );
}

#[test]
fn test_js_function_declaration_emits_combined_jsdoc_overload_comment() {
    let output = emit_js_dts(
        r#"
/**
 * @template T
 * @template U
 * @overload
 * @param {T[]} array
 * @param {(x: T) => U[]} mapper
 * @returns {U[]}
 *
 * @overload
 * @param {T[][]} array
 * @returns {T[]}
 *
 * @param {unknown[]} array
 * @param {(x: unknown) => unknown} mapper
 * @returns {unknown[]}
 */
function flatMap(array, mapper) {
  return [];
}
"#,
    );

    assert!(
        output.contains("declare function flatMap<T, U>(array: T[], mapper: (x: T) => U[]): U[];"),
        "Expected first overload from combined JSDoc comment: {output}"
    );
    assert!(
        output.contains("declare function flatMap<T, U>(array: T[][]): T[];"),
        "Expected second overload from combined JSDoc comment: {output}"
    );
    assert!(
        !output.contains("array: unknown[]"),
        "Implementation JSDoc tags after overloads should not become a declaration signature: {output}"
    );
}

#[test]
fn test_js_method_declaration_emits_jsdoc_overload_comments() {
    let output = emit_js_dts(
        r#"
/**
 * @template T
 */
class Box {
  /** @param {T} value */
  constructor(value) {
    this.value = value;
  }

  /**
   * @overload
   * @param {Box<number>} this
   * @returns {'number'}
   */
  /**
   * @overload
   * @param {Box<string>} this
   * @returns {'string'}
   */
  /**
   * @returns {string}
   */
  kind() {
    return typeof this.value;
  }
}
"#,
    );

    assert!(
        output.contains("kind(this: Box<number>): \"number\";"),
        "Expected number receiver overload: {output}"
    );
    assert!(
        output.contains("kind(this: Box<string>): \"string\";"),
        "Expected string receiver overload: {output}"
    );
    assert!(
        !output.contains("kind(): string;"),
        "Implementation method signature should not be emitted for JSDoc overloads: {output}"
    );
}

#[test]
fn test_js_constructor_declaration_emits_jsdoc_overloads_before_private_marker() {
    let output = emit_js_dts(
        r#"
export class Foo {
  #value;

  /**
   * @constructor
   * @overload
   * @param {string} value
   */
  /**
   * @constructor
   * @overload
   * @param {number} value
   */
  /** @constructor @param {string | number} value */
  constructor(value) {
    this.#value = value;
  }
}
"#,
    );

    let string_ctor = output
        .find("constructor(value: string);")
        .expect("expected string constructor overload");
    let number_ctor = output
        .find("constructor(value: number);")
        .expect("expected number constructor overload");
    let private_marker = output.find("#private;").expect("expected private marker");

    assert!(
        string_ctor < number_ctor && number_ctor < private_marker,
        "Expected constructor overloads before private marker: {output}"
    );
    assert!(
        !output.contains("string | number"),
        "Implementation constructor JSDoc should not become a signature: {output}"
    );
}

#[test]
fn test_ts_function_declaration_preserves_jsdoc_overload_comments() {
    let output = emit_dts(
        r#"
/**
 * @overload
 * @param {number} value
 * @returns {'number'}
 */
/**
 * @overload
 * @param {string} value
 * @returns {'string'}
 */
/**
 * @param {unknown} value
 * @returns {string}
 */
function kind(value: unknown): string {
  return typeof value;
}
"#,
    );

    assert!(
        output.contains("@overload"),
        "Expected TS @overload JSDoc comments to be preserved: {output}"
    );
    assert!(
        output.contains("declare function kind(value: unknown): string;"),
        "Expected TS implementation signature instead of JSDoc overload expansion: {output}"
    );
    assert!(
        !output.contains("declare function kind(value: number): \"number\";")
            && !output.contains("declare function kind(value: string): \"string\";"),
        "TS @overload JSDoc should not emit overload signatures: {output}"
    );
}

#[test]
fn test_ts_function_declaration_jsdoc_overload_keeps_implementation_param_names() {
    let output = emit_dts(
        r#"
/**
 * @overload
 * @param {number} x
 * @returns {number}
 */
/**
 * @overload
 * @param {string} x
 * @returns {string}
 */
/**
 * @param {unknown} x
 * @returns {unknown}
 */
function identity(x: unknown): unknown {
  return x;
}
"#,
    );

    assert!(
        output.contains("@overload"),
        "Expected TS @overload JSDoc comments to be preserved: {output}"
    );
    assert!(
        output.contains("declare function identity(x: unknown): unknown;"),
        "Expected TS implementation signature to be emitted: {output}"
    );
    assert!(
        !output.contains("identity(x: number)") && !output.contains("identity(x: string)"),
        "TS @overload JSDoc should not emit overload signatures: {output}"
    );
}

#[test]
fn test_ts_class_method_preserves_jsdoc_overload_comments() {
    let output = emit_dts(
        r#"
class Converter {
  /**
   * @overload
   * @param {number} x
   * @returns {string}
   */
  /**
   * @overload
   * @param {string} x
   * @returns {number}
   */
  /**
   * @param {unknown} x
   * @returns {unknown}
   */
  convert(x: unknown): unknown {
    return x;
  }
}
"#,
    );

    assert!(
        output.contains("@overload"),
        "Expected TS method @overload JSDoc comments to be preserved: {output}"
    );
    assert!(
        output.contains("convert(x: unknown): unknown;"),
        "Expected TS implementation method signature to be emitted: {output}"
    );
    assert!(
        !output.contains("convert(x: number): string;")
            && !output.contains("convert(x: string): number;"),
        "TS method @overload JSDoc should not emit overload signatures: {output}"
    );
}

#[test]
fn test_ts_class_constructor_preserves_jsdoc_overload_comments() {
    let output = emit_dts(
        r#"
class Wrapper {
  /**
   * @overload
   * @param {string} value
   */
  /**
   * @overload
   * @param {number} value
   */
  /** @param {unknown} value */
  constructor(value: unknown) {}
}
"#,
    );

    assert!(
        output.contains("@overload"),
        "Expected TS constructor @overload JSDoc comments to be preserved: {output}"
    );
    assert!(
        output.contains("constructor(value: unknown);"),
        "Expected TS implementation constructor signature to be emitted: {output}"
    );
    assert!(
        !output.contains("constructor(value: string);")
            && !output.contains("constructor(value: number);"),
        "TS constructor @overload JSDoc should not emit overload signatures: {output}"
    );
}

#[test]
fn test_js_object_namespace_emits_legacy_jsdoc_overload_member_comments() {
    let output = emit_js_dts(
        r#"
const example = {
  /**
   * @overload Example(value)
   *   Creates Example
   *   @param value [String]
   */
  constructor: function Example(value, options) {},
};
"#,
    );

    assert!(
        output.contains("declare namespace example"),
        "Expected object literal namespace declaration: {output}"
    );
    assert!(
        output.contains("@overload Example(value)"),
        "Expected legacy overload comment to be preserved: {output}"
    );
    assert!(
        output.contains("function constructor(value: any): any;"),
        "Expected legacy overload params to replace the implementation signature: {output}"
    );
    assert!(
        !output.contains("options:"),
        "Implementation-only parameters should not leak into the legacy overload: {output}"
    );
}

#[test]
fn test_js_object_namespace_aliases_multiple_legacy_constructor_overloads() {
    let output = emit_js_dts(
        r#"
const example = {
  /**
   * @overload Example(value)
   * @param value [String]
   * @param secret [String]
   * @overload Example(options)
   * @option options value [String]
   */
  constructor: function Example() {},
};
"#,
    );

    assert!(
        output.contains("export function constructor_1(value: any, secret: any): any;"),
        "Expected first legacy constructor overload to use an aliasable local name: {output}"
    );
    assert!(
        output.contains("export function constructor_1(): any;"),
        "Expected option-only legacy overload to fall back to no parameters: {output}"
    );
    assert!(
        output.contains("export { constructor_1 as constructor };"),
        "Expected constructor alias export after synthetic overloads: {output}"
    );
}

#[test]
fn test_js_object_namespace_malformed_legacy_overload_falls_back_to_no_params() {
    let output = emit_js_dts(
        r#"
const example = {
  /**
   * @overload evaluate(options = {}, [callback])
   * @param options [map]
   * @callback callback function (error, result)
   *   If callback is provided it will be called with evaluation result
   *   @param error [Error]
   *   @param result [String]
   */
  evaluate: function evaluate(options, callback) {},
};
"#,
    );

    assert!(
        output.contains("function evaluate(): any;"),
        "Expected malformed legacy overload call to fall back to a no-arg any signature: {output}"
    );
    assert!(
        !output.contains("options:"),
        "Malformed legacy overload params should not be trusted as a signature: {output}"
    );
    assert!(
        output.contains("type callback = (error: any, result: any) => any;"),
        "Expected nested legacy @callback alias to be emitted after the namespace: {output}"
    );
}

#[test]
fn test_js_function_variable_strips_jsdoc_satisfies_comment() {
    let output = emit_js_dts(
        r#"
/** @satisfies {(uuid: string) => void} */
export const fn1 = uuid => {};

/**
 * @satisfies {(a: string, ...args: never) => void}
 * @param {string} a
 */
export const fn2 = (a, b) => {};

/** @satisfies {(uuid: string) => void} */
export function fn3(uuid) {}
"#,
    );

    assert!(
        !output.contains("@satisfies {(uuid: string) => void} */\nexport function fn1"),
        "Expected synthetic function-variable JSDoc @satisfies comment to be stripped: {output}"
    );
    assert!(
        !output.contains("@satisfies {(a: string, ...args: never) => void}"),
        "Expected multiline synthetic function-variable @satisfies comment to be stripped: {output}"
    );
    assert!(
        output.contains("export function fn1(uuid: string): void;"),
        "Expected @satisfies parameter fallback to remain active: {output}"
    );
    assert!(
        output.contains("export function fn2(a: string, b: never): void;"),
        "Expected @param plus @satisfies inference to remain active: {output}"
    );
    assert!(
        output.contains(
            "/** @satisfies {(uuid: string) => void} */\nexport function fn3(uuid: any): void;"
        ),
        "Expected function declarations to preserve @satisfies comments: {output}"
    );
}

#[test]
fn test_jsdoc_typedef_same_file_typeof_export_stays_unqualified() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @satisfies {(uuid: string) => void} */
export const fn1 = uuid => {};

/** @typedef {Parameters<typeof fn1>} Foo */

/** @type Foo */
export const v1 = ["abc"];

/** @satisfies {(label: string) => void} */
export const renamed = label => {};

/** @typedef {ReturnType<typeof renamed>} Bar */
"#,
    );

    assert!(
        output.contains("export function fn1(uuid: string): void;"),
        "Expected @satisfies parameter fallback to keep exported const-function signature: {output}"
    );
    assert!(
        output.contains("export type Foo = Parameters<typeof fn1>;"),
        "Expected JSDoc typedef alias to keep same-file typeof reference unqualified: {output}"
    );
    assert!(
        output.contains("export type Bar = ReturnType<typeof renamed>;"),
        "Expected renamed same-file typeof reference to stay unqualified too: {output}"
    );
    assert!(
        !output.contains("typeof import(\".\").fn1")
            && !output.contains("typeof import(\".\").renamed"),
        "Same-file JSDoc typedef aliases should not self-import exported values: {output}"
    );
}

#[test]
fn test_js_function_declaration_emits_constrained_jsdoc_template() {
    let output = emit_js_dts(
        r#"
/**
 * @template {string} T
 * @param {T} x
 * @returns {T}
 */
export function id(x) {
  return x;
}
"#,
    );

    assert!(
        output.contains("export function id<T extends string>(x: T): T;"),
        "Expected constrained JSDoc template to emit as a type parameter constraint: {output}"
    );
    assert!(
        !output.contains("id<{string}, T>"),
        "Did not expect braced JSDoc constraint to emit as a fake type parameter: {output}"
    );
}

#[test]
fn test_js_function_declaration_uses_jsdoc_type_alias_signature() {
    let output = emit_js_dts(
        r#"
/**
 * @typedef {<T>(m : T) => T} IFn
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
        output.contains("export type IFn = <T>(m: T) => T;"),
        "Expected the JSDoc typedef alias to still be emitted: {output}"
    );
    assert!(
        !output.contains("@typedef"),
        "Did not expect source-only typedef comment to be duplicated before the function: {output}"
    );
    assert!(
        !output.contains("@type {IFn}"),
        "Did not expect implementation-only @type comment in declaration output: {output}"
    );
    let function_pos = output
        .find("export function inJs<T>(m: T): T;")
        .expect("Expected emitted function signature");
    let alias_pos = output
        .find("export type IFn = <T>(m: T) => T;")
        .expect("Expected emitted typedef alias");
    assert!(
        function_pos < alias_pos,
        "JSDoc @type alias-driven exported functions should emit before the pending alias: {output}"
    );
}

#[test]
fn test_js_function_declaration_uses_jsdoc_type_alias_signature_with_nested_commas() {
    let output = emit_js_dts(
        r#"
/**
 * @typedef {<T>(x: [T, number], y: { items: [T, string] }) => [T, string]} IFn
 */

/** @type {IFn} */
export function inJs(l) {
  return l;
}
"#,
    );

    assert!(
        output.contains(
            "export function inJs<T>(x: [T, number], y: { items: [T, string] }): [T, string];"
        ),
        "Expected nested tuple/object commas in JSDoc function typedef to parse as a single signature: {output}"
    );
    assert!(
        output.contains("export type IFn = <T>(x: [T, number], y: {")
            && output.contains("items: [T, string];")
            && output.contains("}) => [T, string];"),
        "Expected nested tuple/object commas to be preserved in emitted typedef alias structure: {output}"
    );
}

#[test]
fn test_js_function_declaration_uses_jsdoc_type_alias_signature_with_nested_function_param() {
    let output = emit_js_dts(
        r#"
/**
 * @typedef {(cb: (x: number) => string, value: number) => void} IFn2
 */

/** @type {IFn2} */
export function inJs(cb, value) {
  cb(value);
}
"#,
    );

    assert!(
        output.contains("export function inJs(cb: (x: number) => string, value: number): void;"),
        "Expected nested function parameter type to parse through closing paren matching: {output}"
    );
    assert!(
        output.contains("export type IFn2 = (cb: (x: number) => string, value: number) => void;"),
        "Expected emitted typedef alias to preserve nested function parameter type: {output}"
    );
}

#[test]
fn test_js_function_declaration_type_alias_signature_filters_renamed_typedef_comment() {
    let output = emit_js_dts(
        r#"
/**
 * @typedef {<Value>(input : Value) => Value} Mapper
 */

/** @type {Mapper} */
export function mapValue(value) {
  return value;
}
"#,
    );

    assert!(
        output.contains("export function mapValue<Value>(input: Value): Value;"),
        "Expected renamed JSDoc function alias to emit as a function signature: {output}"
    );
    assert!(
        output.contains("export type Mapper = <Value>(input: Value) => Value;"),
        "Expected renamed typedef alias to still be emitted: {output}"
    );
    assert!(
        !output.contains("@typedef"),
        "Did not expect renamed source-only typedef comment to be duplicated: {output}"
    );
    assert!(
        !output.contains("@type {Mapper}"),
        "Did not expect renamed implementation-only @type comment in declaration output: {output}"
    );
    let function_pos = output
        .find("export function mapValue<Value>(input: Value): Value;")
        .expect("Expected emitted function signature");
    let alias_pos = output
        .find("export type Mapper = <Value>(input: Value) => Value;")
        .expect("Expected emitted typedef alias");
    assert!(
        function_pos < alias_pos,
        "Renamed JSDoc @type alias-driven exported functions should emit before the pending alias: {output}"
    );
}

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

