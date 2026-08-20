#[test]
fn test_js_function_expando_contextual_keyword_member_exported_when_alias_sibling_present() {
    let output = emit_js_dts(
        r#"
function baz() {}
baz.get = 1;
baz.default = 2;
"#,
    );

    assert!(
        output.contains("export let get: number;"),
        "Expected contextual-keyword property to get export let when reserved-word sibling requires aliasing: {output}"
    );
    assert!(
        output.contains("let _default: number;\n    export { _default as default };"),
        "Expected reserved-word alias emission for default: {output}"
    );
}

#[test]
fn test_js_function_static_property_without_alias_omits_member_export() {
    let output = emit_js_dts(
        r#"
function foo() {}
foo.x = 1;
"#,
    );

    assert!(
        output.contains("declare namespace foo {\n    let x: number;\n}"),
        "Expected ordinary expando property without alias scheduling to omit member export: {output}"
    );
}

#[test]
fn test_js_exported_function_static_property_without_alias_omits_member_export() {
    let output = emit_js_dts(
        r#"
export function foo() {}
foo.x = 1;
"#,
    );

    assert!(
        output.contains("export namespace foo {\n    let x: number;\n}"),
        "Expected exported function expando namespace to omit member export when no alias is scheduled: {output}"
    );
}

#[test]
fn test_js_commonjs_factory_namespace_alias_declaration_emits_after_namespace() {
    let output = emit_js_dts(
        r#"
class Base {
    constructor() {}
}

const BaseFactory = () => {
    return new Base();
};

BaseFactory.Base = Base;
module.exports = BaseFactory;
"#,
    );

    let export_pos = output
        .find("export = BaseFactory;")
        .expect("Expected CommonJS export assignment");
    let factory_pos = output
        .find("declare function BaseFactory")
        .expect("Expected factory function declaration");
    let namespace_pos = output
        .find("declare namespace BaseFactory")
        .expect("Expected merged namespace declaration");
    let class_pos = output
        .find("declare class Base")
        .expect("Expected local class dependency declaration");

    assert!(
        export_pos < factory_pos && factory_pos < namespace_pos && namespace_pos < class_pos,
        "Expected namespace alias dependency declaration to follow the namespace schedule: {output}"
    );
    assert!(
        output.contains("export { Base };"),
        "Expected namespace to export the local class alias: {output}"
    );
}

#[test]
fn test_js_commonjs_namespace_alias_jsdoc_function_declaration_emits_once_after_namespace() {
    let output = emit_js_dts(
        r#"
function Root() {}

/**
 * @param {number} x
 * @returns {number}
 */
function Member(x) {
    return x;
}

Root.Member = Member;
module.exports = Root;
"#,
    );

    let namespace_pos = output
        .find("declare namespace Root")
        .expect("Expected merged namespace declaration");
    let member_pos = output
        .find("declare function Member")
        .expect("Expected local function dependency declaration");

    assert!(
        namespace_pos < member_pos,
        "Expected JSDoc alias dependency declaration to follow the namespace schedule: {output}"
    );
    assert_eq!(
        output.matches("declare function Member").count(),
        1,
        "Expected JSDoc alias dependency declaration to emit once: {output}"
    );
    assert!(
        output.contains("export { Member };"),
        "Expected namespace to export the local function alias: {output}"
    );
}

#[test]
fn test_js_commonjs_expando_does_not_defer_unrelated_same_named_jsdoc_function() {
    let output = emit_js_dts(
        r#"
function Root() {}

/**
 * @returns {string}
 */
function x() {
    return "";
}

Root.x = 1;
module.exports = Root;
"#,
    );

    let function_pos = output
        .find("declare function x")
        .expect("Expected unrelated same-named function declaration");
    let namespace_pos = output
        .find("declare namespace Root")
        .expect("Expected merged namespace declaration");

    assert!(
        function_pos < namespace_pos,
        "Expected same-named non-alias JSDoc function to avoid namespace-alias deferral: {output}"
    );
    assert!(
        output.contains("declare var x: number;"),
        "Expected non-alias expando property declaration to remain a value declaration: {output}"
    );
}

#[test]
fn test_jsdoc_bare_commonjs_import_preserves_import_when_static_surface_is_partial() {
    let module_source = r#"
function Root() {}
class Supported {}
const unsupported = 1;

Root.Supported = Supported;
Root.unsupported = unsupported;
module.exports = Root;
"#;
    let mut module_parser = ParserState::new(
        "/tmp/tsz-jsdoc-partial-surface/root.js".to_string(),
        module_source.to_string(),
    );
    module_parser.parse_source_file();
    let module_arena = Arc::new(module_parser.arena.clone());

    let consumer_source = r#"
/** @type {import("./root")} */
let value;
"#;
    let mut consumer_parser = ParserState::new(
        "/tmp/tsz-jsdoc-partial-surface/consumer.js".to_string(),
        consumer_source.to_string(),
    );
    let consumer_root = consumer_parser.parse_source_file();
    let consumer_arena = Arc::new(consumer_parser.arena.clone());

    let mut binder = BinderState::new();
    binder.bind_source_file(&consumer_parser.arena, consumer_root);

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let mut emitter =
        DeclarationEmitter::with_type_info(&consumer_parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(
        consumer_arena,
        "/tmp/tsz-jsdoc-partial-surface/consumer.js".to_string(),
    );

    let mut arena_to_path = FxHashMap::default();
    arena_to_path.insert(
        Arc::as_ptr(&module_arena) as usize,
        "/tmp/tsz-jsdoc-partial-surface/root.js".to_string(),
    );
    emitter.set_arena_to_path(arena_to_path);

    let mut global_symbol_arenas = FxHashMap::default();
    global_symbol_arenas.insert(tsz_binder::SymbolId(1), module_arena);
    emitter.set_global_symbol_arenas(global_symbol_arenas);

    let output = emitter.emit(consumer_root);

    assert!(
        output.contains(r#"declare let value: import("./root");"#),
        "Expected partial CommonJS static surface to keep original import type: {output}"
    );
    assert!(
        !output.contains("Supported: {"),
        "Did not expect a partial object surface that drops unsupported static members: {output}"
    );
}

#[test]
fn test_js_reordered_accessor_comments_keep_backing_field_comment() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export const key = Symbol("key");

export class C {
    /**
     * @protected
     * @type {null | string}
     */
    [key] = null;

    get value() {
        return this[key];
    }

    /**
     * @type {string}
     */
    set value(v) {
        this[key] = v;
    }
}
"#,
    );

    assert!(
        output.contains(" * @type {string}\n     */\n    set value(v: string | null);"),
        "Expected setter to keep its own JSDoc and backing nullability: {output}"
    );
    assert!(
        output.contains("get value(): string | null;"),
        "Expected getter to reuse the setter/backing-field type: {output}"
    );
    assert!(
        output.contains(
            " * @protected\n     * @type {null | string}\n     */\n    protected [key]: null | string;"
        ),
        "Expected backing field JSDoc to stay attached to the deferred field: {output}"
    );
}

#[test]
fn test_js_getter_uses_jsdoc_type_tag() {
    let output = emit_js_dts(
        r#"
class C {
    /** @type {string=} */
    get p1() {
        return undefined;
    }

    /** @type {?string} */
    get p2() {
        return null;
    }

    /** @type {string | null} */
    get p3() {
        return null;
    }
}
"#,
    );

    assert!(
        output.contains("get p1(): string | undefined;"),
        "Expected getter @type to override undefined body inference: {output}"
    );
    assert!(
        output.contains("get p2(): string | null;"),
        "Expected nullable getter @type to override null body inference: {output}"
    );
    assert!(
        output.contains("get p3(): string | null;"),
        "Expected explicit union getter @type to override null body inference: {output}"
    );
}

#[test]
fn test_js_accessor_pair_preserves_jsdoc_type_comments_and_optional_param_type() {
    let output = emit_js_dts(
        r#"
class C {
    /** @type {string=} */
    get value() {
        return undefined;
    }

    /** @param {string=} value */
    set value(value) {
        this.value = value;
    }
}
"#,
    );

    assert!(
        output.contains(
            "    /** @param {string=} value */\n    set value(value: string | undefined);"
        ),
        "Expected reordered setter comment to stay single-line and optional param to emit as a union: {output}"
    );
    assert!(
        output.contains("    /** @type {string=} */\n    get value(): string | undefined;"),
        "Expected reordered getter comment to stay single-line and @type to drive getter type: {output}"
    );
}

#[test]
fn test_js_accessor_pair_preserves_multiline_jsdoc_type_comments_when_reordered() {
    let output = emit_js_dts(
        r#"
class C {
    /**
     * @type {string=}
     */
    get value() {
        return undefined;
    }

    /**
     * @param {string=} value
     */
    set value(value) {
        this.value = value;
    }
}
"#,
    );

    assert!(
        output.contains("    /**\n     * @param {string=} value\n     */\n    set value(value: string | undefined);"),
        "Expected reordered setter comment to stay multiline: {output}"
    );
    assert!(
        output.contains(
            "    /**\n     * @type {string=}\n     */\n    get value(): string | undefined;"
        ),
        "Expected reordered getter comment to stay multiline: {output}"
    );
    assert!(
        !output.contains("/** @param {string=} value */\n    set value"),
        "Did not expect reordered setter comment to collapse to one line: {output}"
    );
    assert!(
        !output.contains("/** @type {string=} */\n    get value"),
        "Did not expect reordered getter comment to collapse to one line: {output}"
    );
}

#[test]
fn test_js_setter_does_not_lift_nested_nullish_from_array_element_union() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export const key = Symbol("key");

export class C {
    /**
     * @protected
     * @type {(null | string)[]}
     */
    [key] = [];

    /**
     * @type {string[]}
     */
    set value(v) {
        this[key] = v;
    }
}
"#,
    );

    assert!(
        output.contains("set value(v: string[]);"),
        "Expected nested `(null | string)[]` backing type not to inject top-level null into setter type: {output}"
    );
    assert!(
        !output.contains("set value(v: string[] | null);"),
        "Did not expect nested element union nullability to be appended at top level: {output}"
    );
}

#[test]
fn test_property_access_to_unannotated_getter_uses_paired_setter_type() {
    let output = emit_dts_with_usage_analysis(
        r#"
class C {
    value: number;
    method(input: number) {
        return this.value + input;
    }
    get prop() {
        return this.method(this.value);
    }
    set prop(value: number) {
        this.value = this.method(value);
    }
}
const c = new C();
const propValue = c.prop;
"#,
    );

    assert!(
        output.contains("declare const propValue: number;"),
        "Expected property access to recover the paired setter type: {output}"
    );
}

#[test]
fn test_jsdoc_redirected_builtin_lookup_names_normalize_in_js_declarations() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @type {String} */ const text = "";
/** @type {Number} */ const count = 0;
/** @type {Boolean} */ const flag = true;
/** @type {Void} */ const nothing = undefined;
/** @type {Undefined} */ const absent = undefined;
/** @type {Null} */ const empty = null;
/** @type {function} */ const callback = () => void 0;
/** @type {array} */ const values = [];
/** @type {promise} */ const ready = Promise.resolve(0);
"#,
    );

    for expected in [
        "declare const text: string;",
        "declare const count: number;",
        "declare const flag: boolean;",
        "declare const nothing: void;",
        "declare const absent: undefined;",
        "declare const empty: null;",
        "declare const callback: Function;",
        "declare const values: any[];",
        "declare const ready: Promise<any>;",
    ] {
        assert!(
            output.contains(expected),
            "Expected redirected JSDoc lookup `{expected}` in output: {output}"
        );
    }
}

#[test]
fn test_jsdoc_unrecognized_lookup_names_remain_unresolved_in_js_declarations() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @type {bool} */ const maybe = true;
/** @type {integer} */ const count = 1;
"#,
    );

    assert!(
        output.contains("declare const maybe: bool;"),
        "Expected unrecognized JSDoc lookup `bool` to remain unresolved: {output}"
    );
    assert!(
        output.contains("declare const count: integer;"),
        "Expected unrecognized JSDoc lookup `integer` to remain unresolved: {output}"
    );
}

#[test]
fn test_jsdoc_object_index_type_prevents_namespace_object_emit() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @type {Object<string, string>} */ const labels = {x: "x"};
"#,
    );

    assert!(
        output.contains("declare const labels: {\n    [x: string]: string;\n};"),
        "Expected Object<K,V> JSDoc to emit an index-signature const declaration: {output}"
    );
    assert!(
        !output.contains("declare namespace labels"),
        "Did not expect an explicit JSDoc object type to be emitted as a namespace object: {output}"
    );
}

#[test]
fn test_jsdoc_redirected_event_const_undefined_includes_undefined() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @type {event} */ const evt = undefined;
"#,
    );

    assert!(
        output.contains("declare const evt: Event | undefined;"),
        "Expected redirected `event` lookup with undefined initializer to preserve undefined: {output}"
    );
}

/// Object literal optional method `a?() {}` must emit as method-signature
/// style `a?(): void`, not property-function style `a?: () => void`.
///
/// Adjacent shape: different method name (`handle?()`) proves the rule is
/// not name-dependent.
#[test]
fn test_object_literal_optional_method_uses_method_signature_style() {
    // Uses emit_dts_with_binding (no solver cache) which goes through the
    // allowlisted_initializer_type_text path; the optional method must still
    // render with method-signature syntax.
    let output = emit_dts_with_binding(
        r#"
const bar = { a?() {} };
const baz = { handle?() {} };
"#,
    );

    assert!(
        output.contains("a?(): void"),
        "Expected optional method to emit as 'a?(): void' (method-signature style), not 'a?: () => void': {output}"
    );
    assert!(
        output.contains("handle?(): void"),
        "Expected optional method 'handle' to emit as 'handle?(): void': {output}"
    );
    assert!(
        !output.contains("a?: () =>"),
        "Expected no property-function style for optional method 'a': {output}"
    );
    assert!(
        !output.contains("handle?: () =>"),
        "Expected no property-function style for optional method 'handle': {output}"
    );
}

/// When `{ y? }` is written (grammar error TS1162), tsc still infers an optional
/// property for the object type. The `?` token position is recorded during parsing
/// so the DTS emitter can produce `y?: T` instead of `y: T`.
///
/// Structural rule: when a shorthand property assignment has `question_token_pos != 0`,
/// the emitted object type member uses `name?` as the prefix (optional property).
/// This applies regardless of the identifier name used (structural, not spelling-dependent).
#[test]
fn optional_shorthand_property_inferred_as_optional_in_dts() {
    // `{ y? }` is a grammar error but tsc still makes the property optional in DTS.
    // Usage analysis is required so the emitter can resolve `y` → `let y: number`.
    let output = emit_dts_with_usage_analysis(
        r#"
let y: number;
export const obj = { y? };
"#,
    );
    assert!(
        output.contains("y?: number"),
        "shorthand property with `?` should emit as optional (y?: number): {output}"
    );
    assert!(
        !output.contains("y: number"),
        "shorthand property with `?` must not emit as non-optional: {output}"
    );

    // Same rule with a different name — proves the rule is not spelling-dependent.
    let output2 = emit_dts_with_usage_analysis(
        r#"
let count: string;
export const o = { count? };
"#,
    );
    assert!(
        output2.contains("count?: string"),
        "renamed shorthand property with `?` should emit as optional (count?: string): {output2}"
    );
}
