/// Counter-regression: when computed-named instance members appear in
/// source order *before* static members, the static members must still
/// hoist to the top of the d.ts class body — that's the actual rule
/// tsc follows for computed-name TS classes (see
/// `declarationEmitSimpleComputedNames1`).  Verifies the static-hoist
/// rule didn't regress when the constructor-handling fix landed.
#[test]
fn ts_class_with_computed_names_hoists_static_members_above_instance() {
    let output = emit_dts(
        r#"
declare const classFieldName: string;
declare const otherField: string;
declare const staticField: string;
export class Holder {
    [classFieldName]() { return "value"; }
    [otherField]() { return 42; }
    static [staticField]() { return { static: true as boolean }; }
    static [staticField]() { return { static: "sometimes" as string }; }
}
"#,
    );
    let trimmed = output.trim();
    let static_a = trimmed
        .find("static [staticField]")
        .expect("expected first static member");
    let instance_a = trimmed
        .find("[classFieldName]")
        .expect("expected first instance member");
    let instance_b = trimmed
        .find("[otherField]")
        .expect("expected second instance member");
    assert!(
        static_a < instance_a && static_a < instance_b,
        "static members should hoist above instance members for TS classes with computed names: {trimmed}"
    );
}

/// Direct regression test for the trim helper used by
/// `type_argument_list_source_text`.  Two-axis property: a bare
/// overshoot `Foo>` becomes `Foo`, and a nested balanced `<…>` like
/// `C.A<C.B>` is left intact (naive trimming would corrupt it into
/// `C.A<C.B`).  The parser's `token_full_start()` correctly anchors
/// `TypeReference` ends; only `LiteralType`/`UnionType`/
/// `IntersectionType` have the `token_end()` overshoot quirk this
/// helper fixes.
#[test]
fn strip_type_argument_overshoot_balances_nested_angle_brackets() {
    use crate::declaration_emitter::DeclarationEmitter;

    let mut overshoot = String::from("\"Hello\">");
    DeclarationEmitter::strip_type_argument_overshoot_for_test(&mut overshoot);
    assert_eq!(
        overshoot, "\"Hello\"",
        "literal-type overshoot must be trimmed"
    );

    let mut nested = String::from("C.A<C.B>");
    DeclarationEmitter::strip_type_argument_overshoot_for_test(&mut nested);
    assert_eq!(
        nested, "C.A<C.B>",
        "balanced nested `<…>` must not be trimmed"
    );

    let mut nested_with_overshoot = String::from("C.A<C.B>>");
    DeclarationEmitter::strip_type_argument_overshoot_for_test(&mut nested_with_overshoot);
    assert_eq!(
        nested_with_overshoot, "C.A<C.B>",
        "trailing overshoot `>` must be trimmed but inner `>` kept"
    );

    let mut trailing_comma = String::from("\"foo\", ");
    DeclarationEmitter::strip_type_argument_overshoot_for_test(&mut trailing_comma);
    assert_eq!(
        trailing_comma, "\"foo\"",
        "trailing `,`/whitespace must drop"
    );

    let mut quoted_gt = String::from("\"a>b\"");
    DeclarationEmitter::strip_type_argument_overshoot_for_test(&mut quoted_gt);
    assert_eq!(
        quoted_gt, "\"a>b\"",
        "`>` inside string literals must not affect the balance count"
    );
}

#[test]
fn test_js_exported_class_emits_documented_constructor_assignment_field() {
    let source = r#"
export class Aleph {
    /**
     * Impossible to construct.
     * @param {Aleph} a
     * @param {null} b
     */
	    constructor(a, b) {
	        /**
	         * Field is always null
	         */
	        this.field = b;
	        /**
	         * Explicitly typed count.
	         * @type {number}
	         */
	        this.count = 1;
	    }

    /**
     * Doesn't actually do anything
     * @returns {void}
     */
    doIt() {}
	}
	"#;
    let output = emit_js_dts(source);

    assert!(
        output.contains(
            "/**\n     * Field is always null\n     */\n    field: any;\n    /**\n     * Explicitly typed count.\n     * @type {number}\n     */\n    count: number;\n    /**\n     * Doesn't actually do anything"
        ),
        "Expected documented constructor assignment field before method declaration: {output}"
    );
}

#[test]
fn test_js_constructor_assignment_single_line_type_comment_stays_compact() {
    let source = r#"
/**
 * @typedef {string | number} Whatever
 */
class Conn {
    constructor() {}
    item = 3;
    method() {}
}

class Wrap {
    /**
     * @param {Conn} c
     */
    constructor(c) {
        this.connItem = c.item;
        /** @type {Whatever} */
        this.another = "";
    }
}

export { Wrap };
"#;
    let output = emit_js_dts(source);

    assert!(
        output.contains("    /** @type {Whatever} */\n    another: Whatever;"),
        "Expected single-line constructor assignment @type JSDoc to stay compact: {output}"
    );
    assert!(
        output.contains("export type Whatever = string | number;"),
        "Expected exported typedef alias used by compact @type comment to be emitted: {output}"
    );
}

#[test]
fn test_js_local_bare_require_alias_without_exports_is_elided() {
    let source = r#"
const u = require("untyped");
u.assignment.nested = true;
u.noError();
"#;
    let output = emit_js_dts(source);

    assert!(
        !output.contains("declare const u"),
        "Expected local bare require alias in a non-exporting JS module to be elided: {output}"
    );
    assert_eq!("export {};", output.trim());
}

#[test]
fn test_js_local_destructured_require_alias_without_exports_is_elided() {
    let source = r#"
const { apply } = require("./moduleExportAliasDuplicateAlias");
const result = apply.toFixed();
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert!(
        !output.contains("declare const apply"),
        "Expected local destructured require alias in a non-exporting JS module to be elided: {output}"
    );
    assert!(
        !output.contains("declare const result"),
        "Expected locals derived from the elided destructured require alias to be omitted: {output}"
    );
    assert_eq!("export {};", output.trim());
}

#[test]
fn test_js_local_dynamic_require_alias_without_exports_is_preserved() {
    let source = r#"
const moduleName = "untyped";
const u = require(moduleName);
u.noError();
"#;
    let output = emit_js_dts(source);

    assert!(
        output.contains("declare const u: any;"),
        "Expected dynamic require alias to be preserved: {output}"
    );
}

#[test]
fn test_js_returned_function_expression_uses_attached_jsdoc_signature() {
    let output = emit_js_dts(
        r#"
function f1() {
    /**
     * @param {number} a
     * @param {number} b
     * @returns {number}
     */
    return (a, b) => a + b;
}

function f2() {
    /** @type {(a: string, b: string) => string} */
    return function (a, b) {
        return a + b;
    };
}
"#,
    );

    assert!(
        output.contains("declare function f1(): (a: number, b: number) => number;"),
        "Expected returned arrow signature to use attached @param/@returns JSDoc: {output}"
    );
    assert!(
        output.contains("declare function f2(): (a: string, b: string) => string;"),
        "Expected returned function expression signature to use attached @type JSDoc: {output}"
    );
}

#[test]
fn test_js_export_equals_function_static_assignments_stay_top_level() {
    let output = emit_js_dts(
        r#"
module.exports = MyClass;

function MyClass() {}
MyClass.staticMethod = function() {}
MyClass.prototype.method = function() {}
MyClass.staticProperty = 123;
"#,
    );

    assert!(
        output.contains("export = MyClass;"),
        "Expected CommonJS export assignment: {output}"
    );
    assert!(
        output.contains(
            "declare namespace MyClass {\n    export { staticMethod, staticProperty };\n}"
        ),
        "Expected namespace to re-export top-level expando declarations: {output}"
    );
    assert!(
        output.contains("declare function staticMethod(): void;"),
        "Expected static function expando to remain a top-level declaration: {output}"
    );
    assert!(
        output.contains("declare var staticProperty: number;"),
        "Expected static value expando to remain a top-level declaration: {output}"
    );
    assert!(
        !output.contains("declare namespace MyClass {\n    function staticMethod(): void;"),
        "Did not expect static expandos to be folded into the namespace body: {output}"
    );
}

#[test]
fn test_js_function_static_properties_export_from_merged_namespace() {
    let output = emit_js_dts(
        r#"
function foo() {}
foo.x = 1;
foo.default = 2;
"#,
    );

    assert!(
        output.contains("declare namespace foo {\n    export let x: number;"),
        "Expected direct expando property to get export let when a reserved-word sibling requires aliasing: {output}"
    );
    assert!(
        output.contains("let _default: number;\n    export { _default as default };"),
        "Expected reserved expando property to use local alias plus export specifier: {output}"
    );
}

#[test]
fn test_js_function_expando_function_member_exported_when_alias_sibling_present() {
    let output = emit_js_dts(
        r#"
function bar() {}
bar.greet = function(name) { return name; };
bar.default = 42;
"#,
    );

    assert!(
        output.contains("export function greet"),
        "Expected function-valued expando member to get export when a reserved-word sibling requires aliasing: {output}"
    );
    assert!(
        output.contains("let _default: number;\n    export { _default as default };"),
        "Expected reserved-word alias emission for default: {output}"
    );
}
