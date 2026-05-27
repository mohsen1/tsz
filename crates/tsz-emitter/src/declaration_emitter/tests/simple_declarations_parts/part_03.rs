#[test]
fn test_js_late_bound_function_alias_generation_avoids_existing_namespace_members() {
    let source = r#"
export const normal = 1;
export function foo() {}
foo.normal = false;
foo.normal_1 = true;
"#;

    let output = emit_js_dts_with_usage_analysis(source);
    let expected = r#"export function foo(): void;
export namespace foo {
    let normal_2: boolean;
    export { normal_2 as normal };
    let normal_1: boolean;
}"#;
    assert!(
        output.contains(expected),
        "Expected namespace alias generation to skip existing member names when resolving collisions: {output}"
    );
}

#[test]
fn test_ts_late_bound_arrow_assignments_preserve_key_text_and_types() {
    let source = r#"
const c = "C";
const num = 1;
const numStr = "10";
const withWhitespace = "foo bar";
const emoji = "🤷‍♂️";
export const arrow = () => {};
arrow["B"] = "bar";
export const arrow2 = () => {};
arrow2[c] = 100;
export const arrow3 = () => {};
arrow3[77] = 0;
export const arrow4 = () => {};
arrow4[num] = 0;
export const arrow5 = () => {};
arrow5["101"] = 0;
export const arrow6 = () => {};
arrow6[numStr] = 0;
export const arrow7 = () => {};
arrow7["qwe rty"] = 0;
export const arrow8 = () => {};
arrow8[withWhitespace] = 0;
export const arrow9 = () => {};
arrow9[emoji] = 0;
"#;

    let output = emit_dts_with_usage_analysis(source);
    let expected = r#"export declare const arrow: {
    (): void;
    B: string;
};
export declare const arrow2: {
    (): void;
    C: number;
};
export declare const arrow3: {
    (): void;
    77: number;
};
export declare const arrow4: {
    (): void;
    1: number;
};
export declare const arrow5: {
    (): void;
    "101": number;
};
export declare const arrow6: {
    (): void;
    "10": number;
};
export declare const arrow7: {
    (): void;
    "qwe rty": number;
};
export declare const arrow8: {
    (): void;
    "foo bar": number;
};
export declare const arrow9: {
    (): void;
    "\uD83E\uDD37\u200D\u2642\uFE0F": number;
};"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected TS late-bound arrow assignments to preserve declaration key text and types: {output}"
    );
}

#[test]
fn test_callable_export_expando_function_property_emits_method_signature() {
    let source = r#"
export interface Point {
    readonly x: number;
    readonly y: number;
}

export const Point = (x: number, y: number): Point => ({ x, y });
Point.zero = (): Point => Point(0, 0);
"#;

    let output = emit_dts_with_usage_analysis(source);

    assert!(
        output.contains("zero(): Point;"),
        "Expected function-valued expando on callable export to use method syntax: {output}"
    );
    assert!(
        !output.contains("zero: () => Point;"),
        "Expected not to emit function-valued expando as property syntax: {output}"
    );
}

#[test]
fn test_js_commonjs_exported_arrow_function_preserves_any_return_type() {
    let source = r#"
const donkey = (ast) => ast;
function funky(declaration) { return false; }
module.exports = donkey;
module.exports.funky = funky;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file");
    };
    let var_stmt_idx = source_file.statements.nodes[0];
    let var_stmt = parser
        .arena
        .get(var_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing variable statement");
    let decl_list = parser
        .arena
        .get(var_stmt.declarations.nodes[0])
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
    let ast_atom = interner.intern_string("ast");
    let donkey_type = interner.function(FunctionShape::new(
        vec![ParamInfo::required(ast_atom, TypeId::ANY)],
        TypeId::ANY,
    ));

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(decl.name.0, donkey_type);
    type_cache
        .node_types
        .insert(decl.initializer.0, donkey_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function donkey(ast: any): any;"),
        "Expected concise-arrow CommonJS export to preserve any return type: {output}"
    );
    assert!(
        output.contains("declare namespace donkey {\n    export { funky };\n}"),
        "Expected secondary CommonJS function export to merge into the export= namespace: {output}"
    );
    assert!(
        !output.contains("declare function donkey(ast: any): void;"),
        "Did not expect concise-arrow CommonJS export to collapse to void: {output}"
    );
}

#[test]
fn test_js_commonjs_prototype_and_static_assignments_emit_synthetic_declarations() {
    let source = r#"
module.exports = MyClass;

function MyClass() {}
MyClass.staticMethod = function() {}
MyClass.prototype.method = function() {}
MyClass.staticProperty = 123;

/**
 * Callback to be invoked when test execution is complete.
 *
 * @callback DoneCB
 * @param {number} failures - Number of failures that occurred.
 */
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"export = MyClass;
declare function MyClass(): void;
declare class MyClass {
    method(): void;
}
declare namespace MyClass {
    export { staticMethod, staticProperty, DoneCB };
}
declare function staticMethod(): void;
declare var staticProperty: number;
/**
 * Callback to be invoked when test execution is complete.
 */
type DoneCB = (failures: number) => any;"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected CommonJS static/prototype assignments to emit synthetic declarations: {output}"
    );
}

#[test]
fn test_js_exports_assignment_marks_same_name_class_exported() {
    let output = emit_js_dts(
        r#"
class K {}
exports.K = K;
"#,
    );

    assert!(
        output.contains("export class K"),
        "Expected same-name CommonJS export to reuse the class declaration: {output}"
    );
}

#[test]
fn test_js_commonjs_property_access_export_reuses_assigned_initializer_type() {
    let source = r#"
var NS = {};
NS.K = class {};
exports.K = NS.K;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file");
    };
    let class_expr = parser
        .arena
        .get(source_file.statements.nodes[1])
        .and_then(|node| parser.arena.get_expression_statement(node))
        .map(|stmt| {
            parser
                .arena
                .skip_parenthesized_and_assertions_and_comma(stmt.expression)
        })
        .and_then(|expr| {
            parser
                .arena
                .get(expr)
                .and_then(|node| parser.arena.get_binary_expr(node))
        })
        .map(|binary| {
            parser
                .arena
                .skip_parenthesized_and_assertions_and_comma(binary.right)
        })
        .expect("missing assigned class expression");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let constructor_type = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature::new(Vec::new(), TypeId::ANY)],
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(class_expr.0, constructor_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("export var K: new () => any;"),
        "Expected property-access CommonJS export to reuse the assigned initializer type: {output}"
    );
    assert!(
        !output.contains("declare class K"),
        "Did not expect an intermediate namespace class expando to leak beside the CommonJS export: {output}"
    );
}

#[test]
fn test_js_commonjs_named_class_expression_emits_exported_class() {
    let output = emit_js_dts(
        r#"
exports.K = class K {
    values() {}
};
"#,
    );

    assert!(
        output.contains("export class K {"),
        "Expected named CommonJS class expression to emit as an exported class: {output}"
    );
    assert!(
        output.contains("values(): void;"),
        "Expected named CommonJS class expression members to be preserved: {output}"
    );
    assert!(
        !output.contains("export var K: {"),
        "Did not expect named CommonJS class expression to lower as a constructor object: {output}"
    );
}

#[test]
fn test_exported_class_expression_method_object_types_keep_tsc_indent() {
    let output = emit_dts_with_usage_analysis(
        r#"
export var circularReference = class C {
    static getTags(c: C): C { return c }
    tags(c: C): C { return c }
}
"#,
    );

    assert!(
        output.contains(
            "    getTags(c: {\n        tags(c: /*elided*/ any): /*elided*/ any;\n    }): {\n        tags(c: /*elided*/ any): /*elided*/ any;\n    };"
        ),
        "Expected method parameter and return object types to use one member-relative indent level: {output}"
    );
    assert!(
        !output.contains("\n                tags(c:"),
        "Did not expect multiline method object types to be reindented by the declaration writer: {output}"
    );
}

#[test]
fn test_namespaced_class_expression_method_object_types_keep_tsc_indent() {
    let output = emit_dts_with_usage_analysis(
        r#"
export namespace Boxed {
    export var circularReference = class C {
        static getTags(c: C): C { return c }
        tags(c: C): C { return c }
    }
}
"#,
    );

    assert!(
        output.contains(
            "        getTags(c: {\n            tags(c: /*elided*/ any): /*elided*/ any;\n        }): {\n            tags(c: /*elided*/ any): /*elided*/ any;\n        };"
        ),
        "Expected namespaced method object types to stay relative to the namespace member indent: {output}"
    );
    assert!(
        !output.contains("\n                    tags(c:"),
        "Did not expect nested namespace declaration writer indentation to be added twice: {output}"
    );
}

#[test]
fn test_js_commonjs_class_expression_method_body_survives_non_callable_cache() {
    let source = r#"
exports.K = class K {
    values() {
        return new K();
    }
};
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let interner = TypeInterner::new();
    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(method_idx.0, TypeId::UNKNOWN);
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("values(): K;"),
        "Expected CommonJS class expression method return type to fall back to body inference: {output}"
    );
}

#[test]
fn test_object_literal_computed_numeric_names_prefer_syntax_shape() {
    let output = emit_dts(
        r#"
var v = {
  [-1]: {},
  [+1]: {},
  [~1]: {},
  [!1]: {}
};
"#,
    );

    assert!(
        output.contains("[-1]: {};"),
        "Expected negative computed numeric literal to survive in fallback object typing: {output}"
    );
    assert!(
        !output.contains("\"-1\": {};"),
        "Did not expect canonical string form to survive once syntax override is applied: {output}"
    );
    assert!(
        output.contains("1: {};"),
        "Expected unary-plus computed numeric literal to normalize to a numeric property: {output}"
    );
    assert!(
        !output.contains("\"-2\": {};"),
        "Did not expect canonicalized synthetic numeric names to leak into the object type: {output}"
    );
    assert!(
        !output.contains("[~1]: {}"),
        "Did not expect non-emittable computed names to survive fallback object typing: {output}"
    );
}

#[test]
fn test_js_module_exports_object_literal_with_computed_names_emits_export_equals_surface() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
const TopLevelSym = Symbol();
const InnerSym = Symbol();
module.exports = {
    [TopLevelSym](x = 12) {
        return x;
    },
    items: {
        [InnerSym]: (arg = { x: 12 }) => arg.x
    }
};
"#,
    );

    assert!(
        output.contains("declare const _exports: {"),
        "Expected anonymous CommonJS object export to materialize a synthetic export root: {output}"
    );
    assert!(
        output.contains("[TopLevelSym]"),
        "Expected computed symbol member to survive on synthetic export root: {output}"
    );
    assert!(
        output.contains("items: {"),
        "Expected nested object member to survive on synthetic export root: {output}"
    );
    assert!(
        output.contains("export = _exports;"),
        "Expected synthetic CommonJS object export to end with export=: {output}"
    );
}

#[test]
fn test_js_module_exports_new_expression_emits_typed_export_equals_surface() {
    let output = emit_js_dts(
        r#"
class Foo {}
module.exports = new Foo();
"#,
    );

    assert!(
        output.contains("declare const _exports: Foo;"),
        "Expected anonymous CommonJS value export to synthesize a typed export root: {output}"
    );
    assert!(
        output.contains("export = _exports;"),
        "Expected anonymous CommonJS value export to emit export=: {output}"
    );
    assert!(
        output.contains("declare class Foo"),
        "Expected the supporting class declaration to remain in the output: {output}"
    );
}

#[test]
fn test_js_module_exports_new_expression_with_expando_emits_single_let_export() {
    // Regression: `module.exports = new Foo(); module.exports.additional = X;`
    // previously emitted `additional` TWICE — once via the secondary-member
    // path (during `module.exports = new Foo()` emission) and again via the
    // deferred value-export path (when the statement visitor later reached
    // the `module.exports.additional = X` statement). Fix removes the
    // statement from the deferred export maps before secondary emission so
    // the visitor skips it. Also `export const` was emitted where tsc emits
    // `export let` for CommonJS class-instance exports.
    let output = emit_js_dts_with_usage_analysis(
        r#"
class Foo {
    static stat = 10;
    member = 10;
}
module.exports = new Foo();
module.exports.additional = 20;
"#,
    );
    // The `additional` property must appear exactly once in the export
    // surface. (`emit_js_dts_with_usage_analysis` may wrap output in
    // additional preamble lines; the count of literal occurrences is the
    // stable check.)
    let occurrences = output.matches("additional").count();
    assert_eq!(
        occurrences, 1,
        "Expected `additional` to appear exactly once in the .d.ts, got {occurrences}.\nOutput:\n{output}"
    );
    assert!(
        output.contains("export let additional: 20;"),
        "Expected `export let additional: 20;` (CommonJS class-instance exports use `let`): {output}"
    );
    assert!(
        output.contains("export let member: number;"),
        "Expected `export let member: number;` from class instance widening: {output}"
    );
}

#[test]
fn test_js_module_exports_object_literal_plus_secondary_promotes_named_exports() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
const Strings = {
    a: "A",
    b: "B"
};
module.exports = {
    thing: "ok",
    also: "ok",
    desc: {
        item: "ok"
    }
};
module.exports.Strings = Strings;
"#,
    );

    assert!(
        output.contains("export declare let thing: string;"),
        "Expected anonymous CommonJS object members to become named exports when secondary module.exports members exist: {output}"
    );
    assert!(
        output.contains("export declare let also: string;"),
        "Expected sibling literal members to become named exports: {output}"
    );
    assert!(
        output.contains("export namespace Strings {"),
        "Expected secondary module.exports identifier exports to mark their source declaration as exported: {output}"
    );
    assert!(
        output.contains("export declare namespace desc {"),
        "Expected nested object members to become exported namespaces: {output}"
    );
    assert!(
        !output.contains("export = _exports;"),
        "Did not expect anonymous module.exports object roots with secondary members to stay on the synthetic export= path: {output}"
    );
}

#[test]
fn test_js_exported_object_literal_empty_object_member_emits_namespace_value() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
const x = {
    grey: {}
};
export { x };
"#,
    );

    assert!(
        output.contains("export namespace x {\n    let grey: {};\n}"),
        "Expected named JS object exports with empty object members to emit as namespaces: {output}"
    );
    assert!(
        !output.contains("export const x:"),
        "Did not expect named JS object exports with empty object members to fall back to const object types: {output}"
    );
}

#[test]
fn test_js_commonjs_named_object_alias_empty_object_member_emits_namespace_value() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
const chalk = {
    grey: {}
};
module.exports.chalk = chalk;
"#,
    );

    assert!(
        output.contains("export namespace chalk {\n    let grey: {};\n}"),
        "Expected CommonJS named object aliases with empty object members to emit as namespaces: {output}"
    );
    assert!(
        !output.contains("export const chalk:"),
        "Did not expect CommonJS named object aliases with empty object members to fall back to const object types: {output}"
    );
}

#[test]
fn test_js_module_exports_anonymous_class_expression_uses_exports_class_surface() {
    let output = emit_js_dts(
        r#"
module.exports = class {
    /**
     * @param {number} p
     */
    constructor(p) {
        this.t = 12 + p;
    }
};
"#,
    );

    assert!(
        output.contains("export = exports;"),
        "Expected anonymous CommonJS class exports to target the synthetic exports class surface: {output}"
    );
    assert!(
        output.contains("declare class exports {"),
        "Expected anonymous CommonJS class exports to emit a named class surface: {output}"
    );
    assert!(
        output.contains("constructor(p: number);"),
        "Expected constructor JSDoc to flow through the synthetic exports class surface: {output}"
    );
    assert!(
        output.contains("t: number;"),
        "Expected instance properties to survive the synthetic exports class surface: {output}"
    );
}

#[test]
fn test_js_module_exports_anonymous_class_secondary_class_emits_once() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
module.exports = class {
    constructor(p) {
        this.t = 12 + p;
    }
};
module.exports.Sub = class {
    constructor() {
        this.instance = new module.exports(10);
    }
};
"#,
    );

    assert!(
        output.contains("declare namespace exports {\n    export { Sub };\n}"),
        "Expected secondary anonymous class exports to be aliased through the export= namespace: {output}"
    );
    assert!(
        output.contains("declare class Sub {"),
        "Expected secondary anonymous class exports to emit a local class declaration: {output}"
    );
    assert!(
        !output.contains("export class Sub"),
        "Did not expect the secondary class assignment to also emit as a named export: {output}"
    );
    assert!(
        output.contains("instance: import(\".\");"),
        "Expected `new module.exports()` instance fields to use the module self-import surface: {output}"
    );
}

#[test]
fn test_js_commonjs_constructor_function_prototype_object_emits_single_class() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @constructor */
module.exports.MyClass = function() {
    this.x = 1;
};
module.exports.MyClass.prototype = {
    a: function() {
    }
};
"#,
    );

    assert!(
        output.contains("export class MyClass {\n    a: () => void;\n}"),
        "Expected CommonJS constructor functions with prototype object literals to emit as a single class surface: {output}"
    );
    assert!(
        !output.contains("export function MyClass"),
        "Did not expect the constructor function assignment to emit beside the class: {output}"
    );
    assert!(
        !output.contains("constructor();") && !output.contains("x: number;"),
        "Did not expect constructor-body properties to leak when tsc uses the prototype object surface: {output}"
    );
}

#[test]
fn test_js_commonjs_export_assignment_inside_closure_emits_export_surface() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
function foo() {
    module.exports = exports = function (o) {
        return o;
    };
    const m = function () {
    }
    exports.methods = m;
}
"#,
    );

    assert!(
        output.contains("declare function _exports(o: any): any;"),
        "Expected closure CommonJS root assignment to emit export= function surface: {output}"
    );
    assert!(
        output.contains("declare namespace _exports {\n    export { m as methods };\n}"),
        "Expected closure CommonJS secondary exports to attach to the synthetic namespace: {output}"
    );
    assert!(
        output.contains("export = _exports;\ndeclare function m(): void;"),
        "Expected local function secondary export target to be emitted after export=: {output}"
    );
    assert!(
        !output.contains("declare function foo"),
        "Did not expect enclosing helper closure to leak as the declaration surface: {output}"
    );
}

#[test]
fn test_jsdoc_type_tags_on_const_null_preserve_closure_type_syntax() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @type {?} */
export const a = null;
/** @type {*} */
export const b = null;
/** @type {string?} */
export const c = null;
/** @type {string=} */
export const d = null;
/** @type {string!} */
export const e = null;
/** @type {function(string, number): object} */
export const f = null;
/** @type {function(new: object, string, number)} */
export const g = null;
/** @type {Object.<string, number>} */
export const h = null;
"#,
    );

    assert!(
        output.contains("export const a: unknown;"),
        "Expected bare Closure unknown @type to win over const null fallback: {output}"
    );
    assert!(output.contains("export const b: any;"));
    assert!(output.contains("export const c: string | null;"));
    assert!(output.contains("export const d: string | undefined;"));
    assert!(output.contains("export const e: string;"));
    assert!(output.contains("export const f: (arg0: string, arg1: number) => object;"));
    assert!(output.contains("export const g: new (arg1: string, arg2: number) => object;"));
    assert!(output.contains("export const h: {\n    [x: string]: number;\n};"));
}

#[test]
fn test_jsdoc_typedef_comment_before_namespace_object_is_not_duplicated() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @template T
 * @template {keyof T} K
 * @typedef {T[K]} Foo
 */
const x = { a: 1 };

/** @type {Foo<typeof x, "a">} */
const y = "a";
"#,
    );

    assert!(
        output.starts_with("declare namespace x {\n    let a: number;\n}"),
        "Expected namespace object emit without leaking implementation-only typedef JSDoc: {output}"
    );
    assert!(
        output.contains("type Foo<T, K extends keyof T> = T[K];"),
        "Expected typedef alias to still be emitted: {output}"
    );
    assert!(
        !output.contains("@typedef"),
        "Did not expect the source typedef comment to be duplicated in the DTS: {output}"
    );
}

#[test]
fn test_js_array_subclass_emits_array_any_and_constructors() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
class ElementsArray extends Array {
    static {
        this.isArray = (arg) => Array.isArray(arg);
    }
}
"#,
    );

    let expected = "declare class ElementsArray extends Array<any> {\n    constructor(arrayLength?: number);\n    constructor(arrayLength: number);\n    constructor(...items: any[]);\n}";
    assert!(
        output.contains(expected),
        "Expected bare JS Array subclasses to inherit Array constructor overloads: {output}"
    );
}

#[test]
fn test_js_commonjs_function_like_export_preserves_constructor_jsdoc_block() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {number} timeout
 */
function Timer(timeout) {
    this.timeout = timeout;
}
module.exports = Timer;
"#,
    );

    let expected = "declare class Timer {\n    /**\n     * @param {number} timeout\n     */\n    constructor(timeout: number);\n    timeout: number;\n}";
    assert!(
        output.contains(expected),
        "Expected synthetic function-like class constructor JSDoc to stay block-formatted: {output}"
    );
}

#[test]
fn test_js_commonjs_export_equals_function_jsdoc_follows_export_assignment() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {number} timeout
 */
function Timer(timeout) {
    this.timeout = timeout;
}
module.exports = Timer;
"#,
    );

    let export_pos = output
        .find("export = Timer;")
        .expect("Expected CommonJS export assignment");
    let jsdoc_pos = output
        .find("/**\n * @param {number} timeout\n */")
        .expect("Expected Timer JSDoc block");
    let function_pos = output
        .find("declare function Timer(timeout: number): void;")
        .expect("Expected Timer function declaration");
    assert!(
        export_pos < jsdoc_pos && jsdoc_pos < function_pos,
        "Expected export= before the function JSDoc and declaration: {output}"
    );
}

#[test]
fn test_js_commonjs_export_equals_plain_function_jsdoc_follows_export_assignment() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {number} value
 * @returns {string}
 */
function format(value) {
    return String(value);
}
module.exports = format;
"#,
    );

    let export_pos = output
        .find("export = format;")
        .expect("Expected CommonJS export assignment");
    let jsdoc_pos = output
        .find("/**\n * @param {number} value\n * @returns {string}\n */")
        .expect("Expected format JSDoc block");
    let function_pos = output
        .find("declare function format(value: number): string;")
        .expect("Expected format function declaration");
    assert!(
        export_pos < jsdoc_pos && jsdoc_pos < function_pos,
        "Expected export= before the plain function JSDoc and declaration: {output}"
    );
}

#[test]
fn test_js_exported_function_like_class_preserves_constructor_jsdoc_block() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {number} x
 * @param {number} y
 */
export function Point(x, y) {
    if (!(this instanceof Point)) {
        return new Point(x, y);
    }
    this.x = x;
    this.y = y;
}
"#,
    );

    let expected = "export class Point {\n    /**\n     * @param {number} x\n     * @param {number} y\n     */\n    constructor(x: number, y: number);";
    assert!(
        output.contains(expected),
        "Expected exported function-like class constructor JSDoc to stay block-formatted: {output}"
    );
}

#[test]
fn test_js_function_like_prototype_accessors_and_proto_surface() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {number} len
 */
export function Vec(len) {
    /**
     * @type {number[]}
     */
    this.storage = new Array(len);
}
Vec.prototype = {
    /**
     * @param {Vec} other
     */
    dot(other) {
        if (other.storage.length !== this.storage.length) {
            throw new Error("bad");
        }
        let sum = 0;
        for (let i = 0; i < this.storage.length; i++) {
            sum += this.storage[i] * other.storage[i];
        }
        return sum;
    }
};

/**
 * @param {number} x
 * @param {number} y
 */
export function Point2D(x, y) {
    if (!(this instanceof Point2D)) {
        return new Point2D(x, y);
    }
    Vec.call(this, 2);
    this.x = x;
    this.y = y;
}
Point2D.prototype = {
    __proto__: Vec,
    get x() {
        return this.storage[0];
    },
    /**
     * @param {number} x
     */
    set x(x) {
        this.storage[0] = x;
    }
};
"#,
    );

    assert!(
        output
            .contains("/**\n * @param {number} len\n */\nexport function Vec(len: number): void;"),
        "Expected hoisted function JSDoc to stay multiline: {output}"
    );
    assert!(
        output.contains("dot(other: Vec): number;"),
        "Expected local accumulator return type to recover as number: {output}"
    );
    assert!(
        !output.contains("x: number | undefined;"),
        "Expected prototype accessor to suppress constructor-inferred x property: {output}"
    );
    let set_pos = output
        .find("set x(x: number);")
        .expect("missing setter in output");
    let get_pos = output
        .find("get x(): number;")
        .expect("missing getter in output");
    let proto_pos = output
        .find("__proto__: typeof Vec;")
        .expect("missing __proto__ surface in output");
    assert!(
        set_pos < get_pos && get_pos < proto_pos,
        "Expected setter/getter before deferred __proto__ member: {output}"
    );
}

#[test]
fn test_namespace_exported_proto_var_suppresses_private_interface_merge() {
    let output = emit_dts_with_usage_analysis(
        r#"
namespace m1 {
    export var __proto__;
    interface __proto__ {}

    class C<T extends { __proto__: __proto__ }> { }
}
__proto__ = 0;
m1.__proto__ = 0;
"#,
    );

    assert!(
        output.contains("declare namespace m1 {\n    var __proto__: any;\n}"),
        "Expected exported __proto__ var to stay as the namespace surface: {output}"
    );
    assert!(
        !output.contains("interface __proto__"),
        "Private merged __proto__ interface should not leak into the namespace d.ts: {output}"
    );
    assert!(
        !output.contains("export {};"),
        "Skipping the private interface should also avoid a namespace scope marker: {output}"
    );
}

#[test]
fn test_namespace_exported_proto_interface_is_public_surface() {
    let output = emit_dts_with_usage_analysis(
        r#"
namespace m1 {
    export interface __proto__ {
        value: string;
    }
}
"#,
    );

    assert!(
        output.contains("interface __proto__"),
        "Expected exported __proto__ interface to stay in namespace d.ts: {output}"
    );
    assert!(
        output.contains("value: string;"),
        "Expected exported __proto__ interface members to stay in namespace d.ts: {output}"
    );
}

#[test]
fn test_js_class_getter_before_setter_preserves_both_accessors() {
    let output = emit_js_dts(
        r#"
class C {
    /** @returns {number} */
    get value() {
        return 1;
    }
    /** @param {number} next */
    set value(next) {
    }
}
"#,
    );

    let setter_pos = output
        .find("set value(next: number);")
        .expect("missing setter in output");
    let getter_pos = output
        .find("get value(): number;")
        .expect("missing getter in output");

    assert!(
        setter_pos < getter_pos,
        "Expected setter/getter pair to be emitted together even when getter appears first: {output}"
    );
}

#[test]
fn test_js_class_define_property_prototype_accessors_emit() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export class D {}
Object.defineProperty(D.prototype, "x", {
    get() {
        return 12;
    },
    /** @param {number} _arg */
    set(_arg) {}
});

/** @param {number} v */
const setter = (v) => {};
export class E {}
Object.defineProperty(E.prototype, "x", { set: setter });
"#,
    );

    assert!(
        output.contains("export class D {\n    set x(_arg: number);\n    get x(): number;\n}"),
        "Expected descriptor getter/setter to fold into class D: {output}"
    );
    assert!(
        output.contains("export class E {\n    set x(value: number);\n}"),
        "Expected descriptor setter alias to fold into class E: {output}"
    );
    assert!(
        !output.contains("Object.defineProperty"),
        "Descriptor statements should not leak to declaration output: {output}"
    );
}

#[test]
fn test_js_named_export_equals_class_expression_shadowing_preserves_root_name() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
class A {
    member = new Q();
}
class Q {
    x = 42;
}
module.exports = class Q {
    constructor() {
        this.x = new A();
    }
};
module.exports.Another = Q;
"#,
    );

    assert!(
        output.contains("export = Q;"),
        "Expected named CommonJS class export-equals roots to preserve their declared class name: {output}"
    );
    assert!(
        output.contains("declare namespace Q {"),
        "Expected named CommonJS class export-equals roots to own their namespace aliases: {output}"
    );
    assert!(
        output.contains("declare class A {\n    member: Q;\n}"),
        "Expected local classes referenced by the exported class expression surface to be retained: {output}"
    );
    assert!(
        output.contains("export { Q_1 as Another };"),
        "Expected shadowed local class aliases to be redirected through a unique declaration name: {output}"
    );
    assert!(
        output.contains("declare class Q_1 {"),
        "Expected the shadowed local class declaration to be emitted under a stable unique alias: {output}"
    );
    assert!(
        !output.contains("export = exports;"),
        "Did not expect named CommonJS class export-equals roots to fall back to the anonymous exports surface: {output}"
    );
}

#[test]
fn test_js_class_jsdoc_members_preserve_readonly_and_order() {
    let output = emit_js_dts(
        r#"
/**
 * @template T
 */
export class Box {
    /**
     * @type {T}
     */
    value;

    /**
     * @return {T}
     */
    get current() { return this.value; }

    /**
     * @type {string}
     * @readonly
     */
    static kind;
}
"#,
    );

    assert!(
        output.contains("export class Box<T> {"),
        "Expected JSDoc class templates to surface in declaration emit: {output}"
    );
    assert!(
        output.contains("static readonly kind: string;"),
        "Expected JSDoc readonly/type tags to control JS class static property emit: {output}"
    );
    assert!(
        output.contains("value: T;"),
        "Expected JSDoc property types to drive JS class field emit: {output}"
    );
    assert!(
        output.contains("get current(): T;"),
        "Expected JSDoc getter return types to drive JS accessor emit: {output}"
    );
}

#[test]
fn test_js_class_method_jsdoc_template_parameters_emit() {
    let output = emit_js_dts(
        r#"
export class Factory {
    /**
     * @template T
     * @param {T} value
     * @return {T}
     */
    static create(value) { return value; }
}
"#,
    );

    assert!(
        output.contains("static create<T>(value: T): T;"),
        "Expected JSDoc method templates on JS classes to surface in declaration emit: {output}"
    );
}

#[test]
fn test_js_class_jsdoc_template_parameters_drive_new_expression_return() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @template Item, Meta
 */
export class Store {
    /**
     * @param {Item} item
     * @param {Meta} meta
     */
    constructor(item, meta) {}

    /**
     * @template Value, Label
     * @param {Value} value
     * @param {Label} label
     */
    static make(value, label) { return new Store(value, label); }
}
"#,
    );

    assert!(
        output.contains("export class Store<Item, Meta> {"),
        "Expected class-level JSDoc templates to emit as type parameters: {output}"
    );
    assert!(
        output.contains(
            "static make<Value, Label>(value: Value, label: Label): Store<Value, Label>;"
        ),
        "Expected constructor parameter JSDoc to infer returned class type arguments: {output}"
    );
}

#[test]
fn test_js_class_jsdoc_extends_preserves_type_arguments() {
    let output = emit_js_dts(
        r#"
/**
 * @template Payload
 */
export class Base {
    /** @param {Payload} value */
    constructor(value) { this.value = value; }
}

/**
 * @template Entry
 * @extends {Base<Entry>}
 */
export class Derived extends Base {
    /** @param {Entry} value */
    constructor(value) { super(value); }
}
"#,
    );

    assert!(
        output.contains("export class Derived<Entry> extends Base<Entry> {"),
        "Expected JSDoc @extends type arguments to be preserved in class heritage: {output}"
    );
}

#[test]
fn test_js_local_class_named_exports_emit_at_export_surface() {
    let output = emit_js_dts(
        r#"
export class Before {}
class Plain {}
export { Plain };
class Hidden {}
export { Hidden as Public };
export class After {}
"#,
    );

    let before_pos = output
        .find("export class Before")
        .expect("Expected exported class before local export-list classes");
    let after_pos = output
        .find("export class After")
        .expect("Expected later exported class to stay in source order");
    let plain_pos = output
        .find("export class Plain")
        .expect("Expected plain local class export to emit at final export surface");
    let hidden_pos = output
        .find("declare class Hidden")
        .expect("Expected aliased local class export dependency");
    let alias_pos = output
        .find("export { Hidden as Public };")
        .expect("Expected aliased local class export line");

    assert!(
        before_pos < after_pos
            && after_pos < plain_pos
            && plain_pos < hidden_pos
            && hidden_pos < alias_pos,
        "Expected local class export-list declarations to be scheduled with final export surface: {output}"
    );
    assert!(
        !output.contains("export { Plain };"),
        "Expected plain local class export list to fold into class declaration: {output}"
    );
}

#[test]
fn test_js_class_extending_any_value_uses_synthetic_base_alias() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
var base = /** @type {*} */(null);
export class Derived extends base {}
"#,
    );

    assert!(
        output.contains("declare const Derived_base: any;"),
        "Expected any-valued JS base expression to get a synthetic class extends alias: {output}"
    );
    assert!(
        output.contains("export class Derived extends Derived_base {"),
        "Expected class to extend the synthetic base alias instead of raw value name: {output}"
    );
    assert!(
        output.contains("[x: string]: any;"),
        "Expected any base alias to contribute tsc's broad instance index signature: {output}"
    );
    assert!(
        !output.contains("extends base"),
        "Did not expect raw JS value base to leak into declaration heritage: {output}"
    );
    assert!(
        !output.contains("declare const base:"),
        "Expected the synthetic base alias to replace the private raw base dependency: {output}"
    );
}

#[test]
fn test_js_class_extending_lib_constructor_keeps_nameable_heritage() {
    let output = emit_js_dts_with_usage_analysis_and_lib(
        r#"
export class FancyError extends Error {
    constructor(status) {
        super(String(status));
    }
}
"#,
        r#"
interface Error {}
interface ErrorConstructor {
    new(message?: string): Error;
}
declare var Error: ErrorConstructor;
"#,
    );

    assert!(
        output.contains("export class FancyError extends Error {"),
        "Expected lib constructor heritage to stay nameable: {output}"
    );
    assert!(
        !output.contains("FancyError_base"),
        "Did not expect a synthetic base alias for lib constructors: {output}"
    );
}

#[test]
fn test_js_class_property_type_resolves_semicolon_typedef_alias() {
    let output = emit_js_dts(
        r#"
export class Box {
    /** @typedef {{ id: string }} Prop */
    ;
    /** @type {Prop} */
    value;
}
"#,
    );

    assert!(
        output.contains("value: { id: string };"),
        "Expected class property JSDoc @type alias to resolve from nearby semicolon-only typedef: {output}"
    );
    assert!(
        !output.contains("value: Prop;"),
        "Expected class property type to emit resolved typedef body, not unresolved alias name: {output}"
    );
}

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

