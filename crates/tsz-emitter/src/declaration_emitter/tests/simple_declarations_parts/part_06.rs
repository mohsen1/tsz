// Inferred object-literal return types must be indented relative to the
// emitter's current `indent_level`. A class method (or a namespaced function)
// nests its synthesized return shape one level deeper than a top-level
// function, matching tsc's declaration indentation. Regression coverage for the
// fix that bases `infer_fallback_type_text_at` on `self.indent_level` instead
// of a fixed depth of 0 (see declarationMapsMultifile emit parity).

#[test]
fn inferred_class_method_object_return_uses_member_relative_indent() {
    let output = emit_dts_with_usage_analysis(
        r#"
export class Foo {
    doThing(x: { a: number }) {
        return { b: x.a };
    }
}
"#,
    );

    // Method body lives at class-member indent (one level): the inferred object
    // return type's members sit at two levels (8 spaces) and the closing brace
    // at one level (4 spaces).
    assert!(
        output.contains("    }): {\n        b: number;\n    };"),
        "Expected inferred method object return type to nest one level deeper than the method: {output}"
    );
    // The bug emitted members at the base indent (4 spaces) with a column-0
    // closing brace; ensure that broken shape is gone.
    assert!(
        !output.contains("    }): {\n    b: number;\n};"),
        "Did not expect the inferred method object return type to ignore the member indent level: {output}"
    );
}

#[test]
fn inferred_namespaced_method_object_return_uses_deeper_indent() {
    // Rename every bound surface (namespace/class/method/parameter/property) to
    // prove the rule keys on structural nesting depth, not on identifier names.
    let output = emit_dts_with_usage_analysis(
        r#"
export namespace Outer {
    export class Widget {
        build(input: { width: number }) {
            return { size: input.width };
        }
    }
}
"#,
    );

    // namespace (1) -> class (2) -> method members (3 -> 12 spaces), closing (2 -> 8 spaces).
    assert!(
        output.contains("        }): {\n            size: number;\n        };"),
        "Expected namespaced method object return type to track the namespace+class indent depth: {output}"
    );
}

#[test]
fn inferred_method_nested_object_return_scales_recursively() {
    let output = emit_dts_with_usage_analysis(
        r#"
export class Box {
    pack(p: { weight: number }) {
        return { value: p.weight, meta: { tag: p.weight } };
    }
}
"#,
    );

    // The nested object literal inside the method return must indent one level
    // deeper again. Method return members sit at indent level 2 (8 spaces), so
    // the nested object's own members sit at level 3 (12 spaces) with its
    // closing brace back at level 2 (8 spaces).
    assert!(
        output.contains("        meta: {\n            tag: number;\n        };"),
        "Expected nested inferred object members to indent recursively relative to the method: {output}"
    );
}

#[test]
fn inferred_top_level_function_object_return_keeps_base_indent() {
    // Negative/control: a top-level function emits at indent level 0, so its
    // inferred object return type keeps the base (4-space members, column-0
    // closing brace). This proves the fix is no-op at the base level.
    let output = emit_dts_with_usage_analysis(
        r#"
export function make(x: { a: number }) {
    return { b: x.a };
}
"#,
    );

    assert!(
        output.contains("): {\n    b: number;\n};"),
        "Expected a top-level function inferred object return type to keep base indentation: {output}"
    );
    assert!(
        !output.contains("): {\n        b: number;\n    };"),
        "Did not expect a top-level function return type to be indented as a class member: {output}"
    );
}

#[test]
fn js_function_prototype_and_static_expando_share_one_namespace() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export function CombinedSurface() {}
CombinedSurface.prototype = {
    instanceMethod() { return 1; }
};
CombinedSurface.staticValue = 1;
"#,
    );

    assert_eq!(
        output
            .matches("export declare namespace CombinedSurface {")
            .count(),
        1,
        "Expected prototype and static declarations to share one namespace: {output}"
    );
    assert!(
        output.contains("var prototype: {")
            && output.contains("instanceMethod(): number;")
            && output.contains("var staticValue: number;"),
        "Expected both declaration surfaces inside the combined namespace: {output}"
    );
    let prototype_pos = output
        .find("var prototype: {")
        .expect("prototype member should be present");
    let static_pos = output
        .find("var staticValue: number;")
        .expect("static member should be present");
    assert!(
        prototype_pos < static_pos,
        "Expected namespace members to preserve assignment order: {output}"
    );
}

#[test]
fn js_function_static_expando_before_prototype_preserves_namespace_order() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export function PrefixSurface() {}
PrefixSurface.staticFirst = 1;
PrefixSurface.prototype = {
    instanceMethod() { return true; }
};
"#,
    );

    assert_eq!(
        output
            .matches("export declare namespace PrefixSurface {")
            .count(),
        1,
        "Expected one merged namespace: {output}"
    );
    let static_pos = output
        .find("var staticFirst: number;")
        .expect("static member should be present");
    let prototype_pos = output
        .find("var prototype: {")
        .expect("prototype member should be present");
    assert!(
        static_pos < prototype_pos,
        "Expected namespace members to preserve assignment order: {output}"
    );
}

#[test]
fn js_named_default_function_with_prototype_uses_local_namespace_merge() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export default function NamedDefault() {}
NamedDefault.prototype = {
    method() { return true; }
};
"#,
    );

    assert!(
        output.contains("declare function NamedDefault(): void;")
            && output.contains(
                "declare namespace NamedDefault {\n    var prototype: {\n        method(): boolean;\n    };\n}"
            )
            && output.contains("export default NamedDefault;"),
        "Expected a named default function to merge locally before its default export: {output}"
    );
    assert!(
        !output.contains("export declare namespace NamedDefault")
            && !output.contains("export default function NamedDefault"),
        "Did not expect either half of the local default merge to carry the default export: {output}"
    );
    let function_pos = output
        .find("declare function NamedDefault(): void;")
        .expect("default function declaration should be present");
    let export_pos = output
        .find("export default NamedDefault;")
        .expect("default export should be present");
    let namespace_pos = output
        .find("declare namespace NamedDefault {")
        .expect("merged namespace should be present");
    assert!(
        function_pos < export_pos && export_pos < namespace_pos,
        "Expected TypeScript's function, default export, namespace order: {output}"
    );
}

#[test]
fn js_function_prototype_namespace_preserves_alias_export_cutover() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
const local = 1;
export function Surface() {}
Surface.first = 1;
Surface.prototype = { value: 1 };
Surface.renamed = local;
Surface.last = 2;
"#,
    );

    assert_eq!(
        output
            .matches("export declare namespace Surface {")
            .count(),
        1,
        "Expected one source-ordered namespace: {output}"
    );
    let first_pos = output
        .find("var first: number;")
        .expect("leading static should be present");
    let prototype_pos = output
        .find("var prototype: {")
        .expect("prototype should be present");
    let alias_pos = output
        .find("export { local as renamed };")
        .expect("alias should remain an export event");
    let last_pos = output
        .find("export var last: number;")
        .expect("static after the alias should be exported");
    assert!(
        first_pos < prototype_pos && prototype_pos < alias_pos && alias_pos < last_pos,
        "Expected TypeScript's source-order export cutover: {output}"
    );
    assert!(
        !output.contains("export var first: number;")
            && !output.contains("export var prototype:"),
        "Members before the alias should remain unexported: {output}"
    );
}

#[test]
fn js_function_prototype_namespace_exports_direct_members_after_leading_alias() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
const local = 1;
export function Surface() {}
Surface.renamed = local;
Surface.prototype = { value: 1 };
Surface.last = 2;
"#,
    );

    let alias_pos = output
        .find("export { local as renamed };")
        .expect("alias should remain an export event");
    let prototype_pos = output
        .find("export var prototype: {")
        .expect("prototype after the alias should be exported");
    let last_pos = output
        .find("export var last: number;")
        .expect("static after the alias should be exported");
    assert!(
        alias_pos < prototype_pos && prototype_pos < last_pos,
        "Expected source-ordered direct exports after the alias: {output}"
    );
}

#[test]
fn js_function_prototype_namespace_repeats_precomputed_static_union() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export function Surface() {}
Surface.prototype = {};
Surface.kind = 1;
Surface.kind = "text";
"#,
    );

    assert_eq!(
        output.matches("var kind: string | number;").count(),
        2,
        "Expected each assignment event to reuse the merged scalar type: {output}"
    );
}

#[test]
fn js_function_prototype_namespace_includes_alias_rhs_in_static_union() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
const local = 1;
export function Surface() {}
Surface.prototype = {};
Surface.kind = local;
Surface.kind = "text";
"#,
    );

    assert!(
        output.contains("export { local as kind };")
            && output.contains("export var kind: string | number;"),
        "Expected alias and direct assignment events to share one inferred union: {output}"
    );
    assert_eq!(
        output
            .matches("export declare namespace Surface {")
            .count(),
        1,
        "Expected one combined namespace: {output}"
    );
}

#[test]
fn js_function_prototype_namespace_owns_nonliteral_static_assignments() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
function make() { return 1; }
export function Surface() {}
Surface.prototype = {};
Surface.called = make();
Surface.conditional = true ? 1 : "text";
"#,
    );

    assert_eq!(
        output
            .matches("export declare namespace Surface {")
            .count(),
        1,
        "Expected nonliteral statics and the prototype in one namespace: {output}"
    );
    let prototype_pos = output
        .find("var prototype: {};")
        .expect("prototype should be present");
    let call_pos = output
        .find("var called: number;")
        .expect("call-valued static should be present");
    let conditional_pos = output
        .find("var conditional: string | number;")
        .expect("conditional static should be present");
    assert!(
        prototype_pos < call_pos && call_pos < conditional_pos,
        "Expected nonliteral static events in source order: {output}"
    );
}

#[test]
fn js_named_default_alias_only_uses_local_function_namespace_merge() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
const local = 1;
export default function Surface() {}
Surface.renamed = local;
"#,
    );

    let function_pos = output
        .find("declare function Surface(): void;")
        .expect("local function should be present");
    let default_pos = output
        .find("export default Surface;")
        .expect("default export should be present");
    let namespace_pos = output
        .find("declare namespace Surface {\n    export { local as renamed };\n}")
        .expect("local namespace alias should be present");
    assert!(
        function_pos < default_pos && default_pos < namespace_pos,
        "Expected function, default export, then namespace: {output}"
    );
}

#[test]
fn js_named_default_prototype_and_class_expando_share_one_namespace() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export default function Surface() {}
Surface.prototype = {};
Surface.Widget = class {};
"#,
    );

    assert_eq!(
        output.matches("declare namespace Surface {").count(),
        1,
        "Expected one local namespace: {output}"
    );
    assert!(
        output.contains(
            "declare namespace Surface {\n    var prototype: {};\n    var Widget: {\n        new (): {};\n    };\n}"
        ) && !output.contains("declare class Widget"),
        "Expected the class expando to remain an inline namespace value: {output}"
    );
}

#[test]
fn js_commonjs_local_function_prototype_keeps_function_namespace_surface() {
    let export_equals_output = emit_js_dts_with_usage_analysis(
        r#"
function ExportedRoot() {}
ExportedRoot.prototype = {
    method() { return 1; }
};
module.exports = ExportedRoot;
"#,
    );
    assert!(
        export_equals_output.contains("export = ExportedRoot;")
            && export_equals_output.contains("declare function ExportedRoot(): void;")
            && export_equals_output.contains(
                "declare namespace ExportedRoot {\n    var prototype: {\n        method(): number;\n    };\n}"
            ),
        "Expected export-equals function plus prototype namespace: {export_equals_output}"
    );
    assert!(
        !export_equals_output.contains("class ExportedRoot"),
        "Did not expect a CommonJS function declaration to become a class: {export_equals_output}"
    );

    let named_output = emit_js_dts_with_usage_analysis(
        r#"
function NamedRoot() {}
NamedRoot.prototype = {
    renamed() { return true; }
};
exports.NamedRoot = NamedRoot;
"#,
    );
    assert!(
        named_output.contains("export { NamedRoot };")
            && named_output.contains("declare function NamedRoot(): void;")
            && named_output.contains(
                "declare namespace NamedRoot {\n    var prototype: {\n        renamed(): boolean;\n    };\n}"
            ),
        "Expected named CommonJS alias to retain the local function namespace: {named_output}"
    );
}

#[test]
fn js_export_equals_static_and_prototype_members_share_source_order() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
function OrderedRoot() {}
OrderedRoot.first = 1;
OrderedRoot.prototype = {
    current() { return true; }
};
OrderedRoot.second = "two";
module.exports = OrderedRoot;
"#,
    );

    assert_eq!(
        output.matches("declare namespace OrderedRoot {").count(),
        1,
        "Expected a single CommonJS function namespace: {output}"
    );
    let first_pos = output
        .find("var first: number;")
        .expect("first static member should be present");
    let prototype_pos = output
        .find("var prototype: {")
        .expect("prototype member should be present");
    let second_pos = output
        .find("var second: string;")
        .expect("second static member should be present");
    assert!(
        first_pos < prototype_pos && prototype_pos < second_pos,
        "Expected CommonJS namespace members in assignment order: {output}"
    );
    assert!(
        !output.contains("declare var first:")
            && !output.contains("declare var second:")
            && !output.contains("export { first")
            && !output.contains("export { second"),
        "Did not expect coalesced static members to leak top-level declarations: {output}"
    );
}

#[test]
fn js_whole_prototype_replacement_omits_prior_per_member_write() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
function ReplacementRoot() {}
ReplacementRoot.prototype.oldMethod = function () { return 1; };
ReplacementRoot.prototype = {
    currentMethod() { return "current"; }
};
module.exports = ReplacementRoot;
"#,
    );

    assert!(
        output.contains("currentMethod(): string;") && !output.contains("oldMethod"),
        "Expected the whole-object replacement to own the prototype declaration shape: {output}"
    );
}
