#[test]
fn test_js_nested_module_exports_object_emits_namespace_with_import_alias() {
    let source = r#"
const Something = require("fs").Something;
module.exports.A = {}
module.exports.A.B = {
    thing: new Something()
}
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert_eq!(
        output.trim(),
        "export namespace A {\n    namespace B {\n        let thing: Something;\n    }\n}\nimport Something_1 = require(\"fs\");\nimport Something = Something_1.Something;"
    );
}

#[test]
fn test_js_exported_object_literal_namespace_records_new_expression_import_alias() {
    let source = r#"
const Something = require("fs").Something;
const ns = {
    thing: new Something()
};
export { ns };
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert!(
        output.contains("export namespace ns {\n    let thing: Something;\n}"),
        "Expected exported object literal to emit as a namespace with a constructor member: {output}"
    );
    assert!(
        output.contains(
            "import Something_1 = require(\"fs\");\nimport Something = Something_1.Something;"
        ),
        "Expected new-expression constructor type to record its require-property import alias: {output}"
    );
}

#[test]
fn test_js_require_property_import_alias_avoids_existing_module_alias_name() {
    let source = r#"
const Something = require("fs").Something;
const Something_1 = 1;
const thing = new Something();
module.exports = {
    thing,
    Something_1
};
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert!(
        output.contains("export const Something_1: 1;"),
        "Expected real exported binding to keep its name: {output}"
    );
    assert!(
        output.contains(
            "import Something_2 = require(\"fs\");\nimport Something = Something_2.Something;"
        ),
        "Require-property module alias should skip the real Something_1 binding: {output}"
    );
    assert!(
        !output.contains("import Something_1 = require(\"fs\");"),
        "Synthetic module alias must not collide with the real Something_1 binding: {output}"
    );
}

#[test]
fn test_js_commonjs_export_equals_require_default_alias_preserves_typeof_surface() {
    let source = r#"
const m = require("./exporter");
module.exports = m.default;
module.exports.memberName = "thing";
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert_eq!(
        output.trim(),
        "declare const _exports: typeof m.default;\nexport = _exports;\nimport m = require(\"./exporter\");"
    );
}

#[test]
fn test_js_export_import_equals_drops_export_keyword() {
    let source = "export import fs2 = require(\"fs\");";
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("import fs2 = require(\"fs\");"),
        "Expected JS export import= to emit as plain import=: {output}"
    );
    assert!(
        !output.contains("export import fs2"),
        "Did not expect JS export import= to keep the export keyword: {output}"
    );
}

#[test]
fn test_js_import_meta_url_infers_string() {
    let source = r#"
const x = import.meta.url;
export { x };
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export const x: string;"),
        "Expected import.meta.url to emit as string in JS declarations: {output}"
    );
}

#[test]
fn test_ts_import_meta_url_infers_string() {
    let source = r#"
const x = import.meta.url;
export { x };
"#;
    let (parser, root) = parse_test_source(source);

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare const x: string;"),
        "Expected import.meta.url to emit as string in TS declarations: {output}"
    );
}

#[test]
fn test_js_top_level_await_literal_preserves_literal_type() {
    let source = r#"
const x = await 1;
export { x };
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export const x: 1;"),
        "Expected top-level await of a literal to preserve the literal type: {output}"
    );
}

#[test]
fn test_js_function_using_arguments_emits_rest_param() {
    let source = r#"
function f(x) {
    arguments;
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function f(x: any, ...args: any[]): void;"),
        "Expected JS functions that reference arguments to gain a synthetic rest param: {output}"
    );
}

#[test]
fn test_js_object_literal_functions_emit_namespace() {
    let source = r#"
const foo = {
    f1: (params) => {}
};
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"declare namespace foo {
    function f1(params: any): void;
}"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected namespace-like JS object literals to emit as declare namespaces: {output}"
    );
}

#[test]
fn test_js_object_literal_values_emit_namespace_members() {
    let source = r#"
const Strings = {
    a: "A",
    b: "B"
};
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"declare namespace Strings {
    let a: string;
    let b: string;
}"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected JS object literal values to emit as namespace members: {output}"
    );
}

#[test]
fn test_js_exported_object_literal_property_reference_member_emits_import_alias() {
    let output = emit_js_dts_with_usage_analysis(
        r##"
export const colors = {
    royalBlue: "#6400e4",
};

export const brandColors = {
    purple: colors.royalBlue,
};
"##,
    );

    let expected = r##"export namespace colors {
    let royalBlue: string;
}
export namespace brandColors {
    import purple = colors.royalBlue;
    export { purple };
}"##;
    assert_eq!(
        output.trim(),
        expected,
        "Expected exported object literal property references to stay namespace-shaped: {output}"
    );
}

#[test]
fn test_js_exported_object_literal_ordinary_property_access_member_emits_value() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
const s = "x";

export const ns = {
    len: s.length,
};
"#,
    );

    let expected = r#"export namespace ns {
    let len: number;
}"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected ordinary property accesses to emit typed namespace values, not import aliases: {output}"
    );
}

#[test]
fn test_js_class_zero_arg_constructor_is_omitted() {
    let source = r#"
export class Preferences {
    constructor() {}
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        !output.contains("constructor();"),
        "Expected zero-arg JS constructors to be omitted from declaration emit: {output}"
    );
}

#[test]
fn test_js_function_like_class_zero_arg_constructor_is_omitted() {
    let source = r#"
function C1() {
    this.prop = 1;
}
C1.prototype.method = function () {};
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert!(
        output.contains("declare class C1"),
        "Expected JS function-like class surface: {output}"
    );
    assert!(
        !output.contains("constructor();"),
        "Expected zero-arg synthetic JS function-like constructors to be omitted: {output}"
    );
}

#[test]
fn test_js_subclass_zero_arg_constructor_is_emitted() {
    let source = r#"
export class Super {
    /**
     * @param {string} firstArg
     * @param {string} secondArg
     */
    constructor(firstArg, secondArg) { }
}

export class Sub extends Super {
    constructor() {
        super('first', 'second');
    }
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("constructor();"),
        "Expected zero-arg JS constructor in subclass to be emitted in declaration: {output}"
    );
}

#[test]
fn test_js_export_equals_emits_before_target_declaration() {
    let source = r#"
const a = {};
export = a;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.starts_with("export = a;\ndeclare const a: {};"),
        "Expected JS export= to emit before its target declaration: {output}"
    );
    assert_eq!(
        output.matches("export = a;").count(),
        1,
        "Did not expect duplicate JS export= statements: {output}"
    );
}

#[test]
fn test_js_module_exports_emits_before_target_declaration() {
    let source = r#"
const a = {};
module.exports = a;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.starts_with("export = a;\ndeclare const a: {};"),
        "Expected JS module.exports assignment to emit as export=: {output}"
    );
    assert_eq!(
        output.matches("export = a;").count(),
        1,
        "Did not expect duplicate JS export= statements: {output}"
    );
}

#[test]
fn test_js_module_exports_typed_object_keeps_value_declaration_with_usage_analysis() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @typedef {{x: number}} Item
 */
/**
 * @type {Item}
 */
const x = { x: 12 };
module.exports = x;
"#,
    );

    assert!(
        output.starts_with("export = x;"),
        "Expected CommonJS export assignment to stay before its value declaration: {output}"
    );
    assert!(
        output.contains("const x: Item;"),
        "Expected typed export-equals object root to emit as a value declaration: {output}"
    );
    assert!(
        !output.contains("namespace x {\n    let x: number;"),
        "Did not expect the object-literal namespace shortcut for an export-equals root: {output}"
    );
    assert!(
        output.contains("declare namespace x {\n    export { Item };\n}"),
        "Expected export-equals root namespace to re-export local typedef aliases: {output}"
    );
    assert!(
        output.contains("type Item = {\n    x: number;\n};"),
        "Expected local JSDoc typedef dependency to remain available: {output}"
    );
}

#[test]
fn test_js_module_exports_function_emits_before_hoisted_jsdoc() {
    let source = r#"
/**
 * @param {number} timeout
 */
function Timer(timeout) {
    this.timeout = timeout;
}
module.exports = Timer;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.starts_with("export = Timer;\n/**"),
        "Expected JS module.exports export= to precede hoisted function JSDoc: {output}"
    );
    assert!(
        output.contains("declare function Timer(timeout: number): void;"),
        "Expected hoisted function declaration to keep its JSDoc signature: {output}"
    );
    assert_eq!(
        output.matches("export = Timer;").count(),
        1,
        "Did not expect duplicate JS export= statements: {output}"
    );
}

#[test]
fn test_js_export_equals_class_keeps_typedef_local_and_after_surface() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @typedef {string | number} Whatever
 */
class Conn {
    constructor() {}
}
module.exports = Conn;
"#,
    );

    assert!(
        output.starts_with("export = Conn;\n/**"),
        "Expected class export= to precede leading typedef JSDoc: {output}"
    );
    assert!(
        output.contains("\ntype Whatever = string | number;"),
        "Expected CommonJS typedef alias to remain local and trailing: {output}"
    );
    assert!(
        !output.contains("export type Whatever"),
        "Did not expect CommonJS typedef alias to emit as exported top-level type: {output}"
    );
}

#[test]
fn test_js_module_exports_function_with_typedef_members() {
    let output = emit_js_dts(
        r#"
/**
 * @typedef Options
 * @property {string} opt
 */

/**
 * @param {Options} options
 */
module.exports = function loader(options) {}
"#,
    );

    let expected = r#"declare namespace _exports {
    export { Options };
}
declare function _exports(options: Options): void;
export = _exports;
type Options = {
    opt: string;
};"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected export= function to retain local typedef namespace members: {output}"
    );
}

#[test]
fn test_export_equals_namespace_keeps_local_type_dependencies() {
    let source = r#"
namespace X {
    interface A {
        kind: 'a';
    }

    interface B {
        kind: 'b';
    }

    export type C = A | B;
}

export = X;
"#;

    let output = emit_dts_with_usage_analysis(source);

    assert!(
        output.contains("interface A {\n        kind: 'a';\n    }"),
        "Expected local namespace interface A used by exported alias to be retained: {output}"
    );
    assert!(
        output.contains("interface B {\n        kind: 'b';\n    }"),
        "Expected local namespace interface B used by exported alias to be retained: {output}"
    );
    assert!(
        output.contains("export type C = A | B;"),
        "Expected exported namespace alias to be retained: {output}"
    );
    assert!(
        output.contains("export {};"),
        "Expected mixed exported and local namespace members to emit a scope marker: {output}"
    );
}

#[test]
fn test_namespace_shadowed_default_export_uses_self_import_type_names() {
    let (parser, _root) = parse_test_source("");
    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.current_namespace_self_import_alias = Some("me".to_string());
    emitter.current_namespace_shadowed_default_name = Some("MyComponent".to_string());
    emitter.current_namespace_self_export_names.extend([
        "Things".to_string(),
        "Props".to_string(),
        "MyComponent".to_string(),
    ]);

    let qualified = emitter.qualify_current_namespace_self_type_text("Things<Props, MyComponent>");

    assert_eq!(qualified, "me.Things<me.Props, me.default>");
}

#[test]
fn test_js_exports_assignment_emits_named_exports_and_filters_locals() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
exports.j = 1;
exports.k = void 0;
var o = {};
function C() {
    this.p = 1;
}
"#,
    );

    assert!(
        output.contains("export const j:"),
        "Expected CommonJS named export value declaration: {output}"
    );
    assert!(
        !output.contains("declare var o:"),
        "Did not expect non-exported locals to leak into JS module declarations: {output}"
    );
    assert!(
        !output.contains("declare function C"),
        "Did not expect non-exported helper declarations to leak into JS module declarations: {output}"
    );
    assert!(
        !output.contains("export const k:"),
        "Did not expect void exports to synthesize declarations: {output}"
    );
}

#[test]
fn test_js_commonjs_keyword_named_exports_emit_aliases() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
exports.class = 123;
exports.for = "loop";
"#,
    );

    assert!(
        output.contains("declare const _class: 123;"),
        "Expected reserved export name to use a local alias: {output}"
    );
    assert!(
        output.contains("declare const _for: \"loop\";"),
        "Expected reserved export name to use a local alias: {output}"
    );
    assert!(
        output.contains("export { _class as class, _for as for };"),
        "Expected reserved export aliases to be grouped: {output}"
    );
    assert!(
        !output.contains("export const class"),
        "Did not expect invalid keyword binding declaration: {output}"
    );
    assert!(
        !output.contains("export const for"),
        "Did not expect invalid keyword binding declaration: {output}"
    );
}

#[test]
fn test_js_module_exports_object_keyword_name_and_namespace_members() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
var x = 12;
module.exports = {
    extends: "base",
    more: {
        others: ["strs"]
    },
    x
};
"#,
    );

    assert!(
        output.contains("export var x: number;"),
        "Expected shorthand object export to keep JS var widening: {output}"
    );
    assert!(
        output.contains("declare let _extends: string;"),
        "Expected reserved object export name to use a local alias: {output}"
    );
    assert!(
        output.contains("export declare namespace more {\n    let others: string[];\n}"),
        "Expected nested object member to emit as an export namespace: {output}"
    );
    assert!(
        output.contains("export { _extends as extends };"),
        "Expected reserved object export alias to be grouped: {output}"
    );
    assert!(
        !output.contains("export const x: 12;"),
        "Did not expect the JS var export to remain const-narrowed: {output}"
    );
    assert!(
        !output.contains("export const extends"),
        "Did not expect invalid keyword binding declaration: {output}"
    );
}

#[test]
fn test_object_shorthand_preserves_declared_identifier_type_after_narrowing() {
    let source = r#"
class RoyalGuard {
    isLeader(): this is LeadGuard {
        return this instanceof LeadGuard;
    }
    isFollower(): this is FollowerGuard {
        return this instanceof FollowerGuard;
    }
}

class LeadGuard extends RoyalGuard {
    lead(): void {}
}

class FollowerGuard extends RoyalGuard {
    follow(): void {}
}

let guard: RoyalGuard = new FollowerGuard();

if (guard.isLeader()) {
    guard.lead();
} else if (guard.isFollower()) {
    guard.follow();
}

var holder = { guard };

if (holder.guard.isLeader()) {
    holder.guard;
} else {
    holder.guard;
}
"#;
    let output = emit_dts_with_usage_analysis(source);

    assert!(
        output.contains("declare var holder: {\n    guard: RoyalGuard;\n};"),
        "Expected shorthand object member to use the declared annotation instead of a narrowed flow type: {output}"
    );
}

#[test]
fn test_object_shorthand_definite_assignment_uses_non_nullish_declared_type() {
    let source = r#"
const a: string | undefined = 'ff';
const foo = { a! };

const b: string | undefined = 'plain';
const plain = { b };

const c: number | null | undefined = 1;
const numeric = { c! };
"#;
    let output = emit_dts_with_usage_analysis(source);
    let (parser, _) = parse_test_source(source);
    let shorthand_exclamation_count = parser
        .arena
        .nodes
        .iter()
        .filter_map(|node| parser.arena.get_shorthand_property(node))
        .filter(|data| data.exclamation_token_pos != 0)
        .count();
    assert_eq!(
        shorthand_exclamation_count, 2,
        "Expected parser recovery to preserve two shorthand definite-assignment markers"
    );

    assert!(
        output.contains("declare const foo: {\n    a: string;\n};"),
        "Expected recovered `{{a!}}` shorthand to use the non-nullish declared type: {output}"
    );
    assert!(
        output.contains("declare const plain: {\n    b: string | undefined;\n};"),
        "Expected plain shorthand to preserve the declared union type: {output}"
    );
    assert!(
        output.contains("declare const numeric: {\n    c: number;\n};"),
        "Expected recovered shorthand to remove both null and undefined from top-level unions: {output}"
    );
}

#[test]
fn test_object_shorthand_mixed_members_keep_resolved_member_types() {
    let source = r#"
class RoyalGuard {
    isLeader(): this is LeadGuard {
        return this instanceof LeadGuard;
    }
}

class LeadGuard extends RoyalGuard {
    lead(): void {}
}

let guard: RoyalGuard = new LeadGuard();
var holder = { guard, generated: 1 };
"#;
    let output = emit_dts_with_usage_analysis(source);

    assert!(
        output
            .contains("declare var holder: {\n    guard: RoyalGuard;\n    generated: number;\n};"),
        "Expected mixed object literal to preserve declared shorthand member and resolved generated member: {output}"
    );
}

#[test]
fn test_js_commonjs_bracket_string_exports_emit_named_declarations() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
exports["foo"] = 1;
module.exports["bar"] = "x";
"#,
    );

    assert!(
        output.contains("export const foo: 1;"),
        "Expected bracket string exports to emit named declarations: {output}"
    );
    assert!(
        output.contains("export const bar: \"x\";"),
        "Expected module.exports bracket string exports to emit named declarations: {output}"
    );
}

#[test]
fn test_js_commonjs_element_access_invalid_export_alias() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
function D() {}
exports["D"] = D;
/** alias comment should stay attached to the skipped source statement */
exports["Does not work yet"] = D;
"#,
    );

    assert!(
        output.contains("export function D(): void;"),
        "Expected valid element access export to emit the local function: {output}"
    );
    assert!(
        output.contains("export { D as _Does_not_work_yet };"),
        "Expected invalid element access export name to emit a sanitized alias: {output}"
    );
    assert!(
        !output.contains("alias comment should stay attached"),
        "Did not expect skipped alias statement comments to leak into output: {output}"
    );
}

#[test]
fn test_jsdoc_object_param_properties_type_destructured_parameter() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {object} opts
 * @param {number} opts.a
 * @param {number} [opts.b]
 * @returns {number}
 */
function foo({ a, b }) {
    return a + (b ?? 0);
}
"#,
    );

    assert!(
        output.contains(
            "declare function foo({ a, b }: {\n    a: number;\n    b?: number | undefined;\n}): number;"
        ),
        "Expected JSDoc object property tags to type the destructured parameter: {output}"
    );
}

#[test]
fn test_jsdoc_nested_object_param_properties_type_destructured_parameter() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {Object} opts
 * @param {string?} opts.reason
 * @param {Object} opts.suberr
 * @param {string?} opts.suberr.reason
 * @param {string?} opts.suberr.code
 */
function foo({ reason, suberr }) {}
"#,
    );

    assert!(
        output.contains(
            "declare function foo({ reason, suberr }: {\n    reason: string | null;\n    suberr: {\n        reason: string | null;\n        code: string | null;\n    };\n}): void;"
        ),
        "Expected nested JSDoc object property tags to type the destructured parameter: {output}"
    );
}

#[test]
fn test_js_exports_assignment_skips_chained_void_zero_preinit() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
exports.y = exports.x = void 0;
exports.x = 1;
exports.y = 2;
"#,
    );

    assert!(
        output.contains("export const x: 1;"),
        "Expected x export declaration to survive past the void-zero preinit: {output}"
    );
    assert!(
        output.contains("export const y: 2;"),
        "Expected y export declaration to survive past the void-zero preinit: {output}"
    );
    assert!(
        !output.contains("export const y: undefined;"),
        "Did not expect chained void-zero preinit to synthesize an undefined export: {output}"
    );
}

#[test]
fn test_js_commonjs_define_property_exports_emit_named_declarations() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
exports.named = 1;
Object.defineProperty(exports, "myProp", { value: 42, writable: true });
Object.defineProperty(module.exports, "ro", { value: "fixed" });
"#,
    );

    assert!(
        output.contains("export const named: 1;"),
        "Expected assignment-shaped CommonJS export declaration: {output}"
    );
    assert!(
        output.contains("export const myProp: number;"),
        "Expected Object.defineProperty(exports, ...) declaration: {output}"
    );
    assert!(
        output.contains("export const ro: string;"),
        "Expected Object.defineProperty(module.exports, ...) declaration: {output}"
    );
}

#[test]
fn test_js_commonjs_define_property_only_export_marks_public_api() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
Object.defineProperty(exports, "only", { value: 42 });
var local = 123;
"#,
    );

    assert!(
        output.contains("export const only: number;"),
        "Expected defineProperty-only CommonJS export declaration: {output}"
    );
    assert!(
        !output.contains("declare var local:"),
        "Did not expect local declarations to leak from a defineProperty-only module: {output}"
    );
}

#[test]
fn test_js_commonjs_define_property_function_exports() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
function fn() {}

Object.defineProperty(module.exports, "fn", { value: fn });
Object.defineProperty(module.exports, "alias", { value: module.exports.fn });
Object.defineProperty(module.exports.fn, "self", { value: module.exports.fn });
"#,
    );

    assert!(
        output.contains("export function fn(): void;"),
        "Expected local function defineProperty export: {output}"
    );
    assert!(
        output.contains("export function alias(): void;"),
        "Expected defineProperty export alias to reuse the function signature: {output}"
    );
    assert!(
        output.contains("export namespace fn {\n    function self(): void;\n}"),
        "Expected defineProperty namespace member function declaration: {output}"
    );
    assert!(
        !output.contains("declare function fn"),
        "Did not expect the consumed local function to be emitted separately: {output}"
    );
}

#[test]
fn test_js_commonjs_define_property_function_export_keeps_jsdoc_at_export_site() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
Object.defineProperty(module.exports, "a", { value: function a() {} });

/**
 * @param {number} value
 * @return {string}
 */
function d(value) { return ""; }
Object.defineProperty(module.exports, "d", { value: d });
"#,
    );

    let expected = r#"export function a(): void;
/**
 * @param {number} value
 * @return {string}
 */
export function d(value: number): string;"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected defineProperty-exported local function JSDoc to stay with the synthetic export: {output}"
    );
}

#[test]
fn test_js_esm_syntax_ignores_commonjs_named_exports() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export const x = 0;
module.exports.y = 0;
"#,
    );

    assert!(
        output.contains("export const x: 0;"),
        "Expected native ESM export to remain: {output}"
    );
    assert!(
        !output.contains("export const y:"),
        "Did not expect CommonJS assignment to become a named export in an ESM JS file: {output}"
    );
}

#[test]
fn test_js_exports_assignment_marks_same_name_function_exported() {
    let output = emit_js_dts(
        r#"
function foo() {}
exports.foo = foo;
"#,
    );

    assert!(
        output.contains("export function foo(): void;"),
        "Expected same-name CommonJS export to reuse the function declaration: {output}"
    );
}

#[test]
fn test_js_commonjs_object_export_function_infers_binary_return_from_jsdoc_param() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {string} a
 */
function bar(a) {
    return a + a;
}

module.exports = { bar };
"#,
    );

    assert!(
        output.contains("export function bar(a: string): string;"),
        "Expected JSDoc parameter type to infer the CommonJS function return: {output}"
    );
}

#[test]
fn test_js_commonjs_object_export_preserves_documented_source_order() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * const doc comment
 */
const x = (a) => {
    return "";
};

/**
 * function doc comment
 */
function b() {
    return 0;
}

module.exports = { x, b };
"#,
    );

    let x_pos = output
        .find("/**\n * const doc comment\n */\nexport function x(a: any): string;")
        .unwrap_or_else(|| panic!("Expected documented exported function x: {output}"));
    let b_pos = output
        .find("/**\n * function doc comment\n */\nexport function b(): number;")
        .unwrap_or_else(|| panic!("Expected documented exported function b: {output}"));
    assert!(
        x_pos < b_pos,
        "Expected module.exports object declarations to preserve source order: {output}"
    );
}

#[test]
fn test_jsdoc_enum_object_literal_emits_type_and_namespace() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @enum {string} */
export const Target = {
    START: "start",
    /** @type {number} */
    OK_I_GUESS: 2
};

/** @enum {function(number): number} */
export const Fs = {
    ADD1: n => n + 1,
    SUB1: n => n - 1
};

/** @enum {?} */
export const Unknowns = { ANY: 1 };

/** @enum {Array} */
export const Lists = { EMPTY: [] };

/** @enum {Promise} */
export const Tasks = { DONE: Promise.resolve() };

/** @enum {function(Array): Promise} */
export const AsyncFns = { RUN: values => Promise.resolve(values) };
"#,
    );

    assert!(
        output.contains("export type Target = string;\nexport namespace Target {"),
        "Expected JSDoc enum value to emit type plus namespace: {output}"
    );
    assert!(
        output.contains("let START: string;"),
        "Expected enum members to use the enum base type: {output}"
    );
    assert!(
        output.contains("let OK_I_GUESS: number;"),
        "Expected member @type to override the enum base type: {output}"
    );
    assert!(
        output.contains("export type Fs = (arg0: number) => number;"),
        "Expected function enum base type to normalize to arrow function syntax: {output}"
    );
    assert!(
        output.contains("function ADD1(n: any): any;")
            && output.contains("function SUB1(n: any): any;"),
        "Expected function enum members to emit as namespace functions: {output}"
    );
    assert!(
        output.contains("export type Unknowns = any;"),
        "Expected standalone Closure unknown enum type to normalize to any: {output}"
    );
    assert!(
        output.contains("export type Lists = any[];"),
        "Expected bare Array enum type to normalize to any[]: {output}"
    );
    assert!(
        output.contains("export type Tasks = Promise<any>;"),
        "Expected bare Promise enum type to normalize to Promise<any>: {output}"
    );
    assert!(
        output.contains("export type AsyncFns = (arg0: any[]) => Promise<any>;"),
        "Expected enum function type to use JSDoc function normalization: {output}"
    );
}

#[test]
fn test_jsdoc_missing_generic_arguments_default_to_any() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {Array=} values
 */
function takesArray(values) {}

/** @param {Promise} promise */
function takesPromise(promise) {}

/** @param {function(Array)} callback */
function takesCallback(callback) {}

/**
 * @return {?Promise}
 */
function maybePromise() {
    return null;
}
"#,
    );

    assert!(
        output.contains("declare function takesArray(values?: any[] | undefined): void;"),
        "Expected optional bare Array to become any[] | undefined: {output}"
    );
    assert!(
        output.contains("declare function takesPromise(promise: Promise<any>): void;"),
        "Expected bare Promise to become Promise<any>: {output}"
    );
    assert!(
        output.contains("declare function takesCallback(callback: (arg0: any[]) => any): void;"),
        "Expected function(Array) to use any[] and default any return: {output}"
    );
    assert!(
        output.contains("declare function maybePromise(): Promise<any> | null;"),
        "Expected nullable bare Promise return to become Promise<any> | null: {output}"
    );
}

#[test]
fn test_js_commonjs_default_function_export_is_renamed_to_default_alias() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
exports.default = function (x) {
    return x;
};
"#,
    );

    assert!(
        output.contains("declare function _default(x: any);"),
        "Expected CJS default function export to use a synthetic default alias: {output}"
    );
    assert!(
        output.contains("export default _default;"),
        "Expected CJS default export to emit a default alias line: {output}"
    );
    assert!(
        !output.contains("export function default"),
        "Expected reserved default export name to be rewritten: {output}"
    );
}

#[test]
fn test_js_commonjs_named_function_export_is_not_static_augmentation_skip() {
    let output = emit_js_dts(
        r#"
module.exports.foo = function foo() {}
module.exports.foo.label = "ok";
"#,
    );

    assert!(
        output.contains("export function foo(): void;"),
        "Expected direct CommonJS function exports to emit a named function declaration: {output}"
    );
    assert!(
        !output.trim().eq("export {};"),
        "CommonJS function export should not be swallowed as a skipped static-method augmentation: {output}"
    );
}

#[test]
fn test_js_commonjs_function_expandos_emit_as_namespace_exports() {
    let source = r#"
function foo() {}
foo.foo = foo;
foo.default = foo;
module.exports = foo;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"export = foo;
declare function foo(): void;
declare namespace foo {
    export { foo };
    export { foo as default };
}"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected CommonJS function expandos to emit as namespace exports: {output}"
    );
}

#[test]
fn test_js_function_value_expandos_emit_merged_namespace_members() {
    let output = emit_js_dts(
        r#"
export function foo() {}
foo.label = "ok";
"#,
    );

    assert!(
        output.contains("export namespace foo {\n    let label: string;\n}"),
        "Expected JS function value expandos to emit as merged namespace members: {output}"
    );
}

#[test]
fn test_js_commonjs_named_function_value_expandos_emit_namespace_members() {
    let output = emit_js_dts(
        r#"
module.exports.foo = function foo() {}
module.exports.foo.label = "ok";
"#,
    );

    assert!(
        output.contains("export namespace foo {\n    let label: string;\n}"),
        "Expected CommonJS named function value expandos to emit as merged namespace members: {output}"
    );
}

#[test]
fn test_js_function_class_expandos_emit_namespace_aliases() {
    let output = emit_js_dts(
        r#"
export function foo() {}
foo.Widget = class {
    value() {}
};
"#,
    );

    assert!(
        output.contains("export namespace foo {\n    export { Widget };\n}"),
        "Expected JS function class expandos to emit as merged namespace aliases: {output}"
    );
    assert!(
        output.contains("declare class Widget {\n    value(): void;\n}"),
        "Expected JS function class expandos to emit a reusable class declaration: {output}"
    );
}

#[test]
fn test_js_commonjs_named_function_class_expandos_emit_namespace_aliases() {
    let output = emit_js_dts(
        r#"
module.exports.foo = function foo() {}
module.exports.foo.Widget = class {
    value() {}
};
"#,
    );

    assert!(
        output.contains("export namespace foo {\n    export { Widget };\n}"),
        "Expected CommonJS named function class expandos to emit namespace aliases: {output}"
    );
    assert!(
        output.contains("declare class Widget {\n    value(): void;\n}"),
        "Expected CommonJS named function class expandos to emit a reusable class declaration: {output}"
    );
}

#[test]
fn test_js_commonjs_named_function_self_alias_emits_import_export_namespace_member() {
    let output = emit_js_dts(
        r#"
module.exports.foo = function foo() {}
module.exports.foo.self = module.exports.foo;
"#,
    );

    assert!(
        output.contains("export namespace foo {\n    import self = foo;\n    export { self };\n}"),
        "Expected CommonJS named function self aliases to use an import alias inside the namespace: {output}"
    );
}

#[test]
fn test_js_function_like_class_emits_companion_class() {
    let output = emit_js_dts(
        r#"
/**
 * @param {number} x
 * @param {number} y
 */
export function Point(x, y) {
    if (!(this instanceof Point)) return new Point(x, y);
    this.x = x;
    this.y = y;
}
"#,
    );

    assert!(
        output.contains("export function Point(x: number, y: number): Point;"),
        "Expected constructor-style JS function to return its companion class: {output}"
    );
    assert!(
        output.contains("export class Point {"),
        "Expected constructor-style JS function to emit a companion class: {output}"
    );
    assert!(
        output.contains("x: number | undefined;") && output.contains("y: number | undefined;"),
        "Expected this-assigned properties to be recovered on the companion class: {output}"
    );
}

#[test]
fn test_js_function_class_merge_omits_non_constructor_signature() {
    let output = emit_js_dts(
        r#"
function C1() {
    /**
     * @param {number} x
     * @param {number} y
     * @returns {number}
     */
    this.prop = function (x, y) {
        return x + y;
    };
}

/**
 * @param {number} x
 * @param {number} y
 * @returns {number}
 */
C1.prototype.method = function (x, y) {
    return x + y;
};
"#,
    );

    assert!(
        output.contains("declare function C1(): void;\ndeclare class C1 {"),
        "Expected JS function plus prototype members to merge with a companion class: {output}"
    );
    assert!(
        !output.contains("constructor();"),
        "Expected companion class not to duplicate the already-emitted function signature as a constructor: {output}"
    );
    assert!(
        output.contains("prop: (x: number, y: number) => number;")
            && output.contains("method(x: number, y: number): number;"),
        "Expected constructor-assigned and prototype members to remain in the companion class: {output}"
    );
}

#[test]
fn test_js_class_static_expando_namespace_members_are_ambient_members() {
    let output = emit_js_dts(
        r#"
function C1() {}

/**
 * @param {number} x
 * @param {number} y
 * @returns {number}
 */
C1.staticProp = function (x, y) {
    return x + y;
};

class C2 {}

/**
 * @param {number} x
 * @param {number} y
 * @returns {number}
 */
C2.staticProp = function (x, y) {
    return x + y;
};
"#,
    );

    assert!(
        output.contains("declare namespace C1 {\n    /**")
            && output.contains("    function staticProp(x: number, y: number): number;\n}"),
        "Expected function static expando to emit as an ambient namespace member: {output}"
    );
    assert!(
        output.contains("declare namespace C2 {\n    /**")
            && output.contains("    function staticProp(x: number, y: number): number;\n}"),
        "Expected class static expando to emit as an ambient namespace member: {output}"
    );
    assert!(
        !output.contains("export function staticProp"),
        "Did not expect explicit export on ambient namespace members: {output}"
    );
}

#[test]
fn test_var_array_initializer_with_index_assignment_emits_valid_array_type() {
    // Regression: `var t = [1, 2, 3]; t[0] = 5;` previously emitted
    // `declare var t: {\n    : number[];` — invalid TypeScript, because
    // the late-bound expando function path wrote the opening `: {` before
    // checking whether the initializer was a function. For an array
    // literal the function then bailed out, leaving a partial brace in
    // the output. After the fix the initializer shape is probed first
    // and the late-bound path is skipped entirely.
    let output = emit_dts(
        r#"
var t = [1, 2, 3];
t[0] = 5;
"#,
    );
    assert!(
        output.contains("declare var t: number[];"),
        "Expected valid array type for var with index assignment, got: {output}"
    );
    assert!(
        !output.contains(": {\n    : "),
        "Did not expect partial broken object type in output: {output}"
    );
    assert!(
        !output.contains("Array<>"),
        "Did not expect raw Array<> token: {output}"
    );
}

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

