// Regression coverage for Issue #12328: two DTS conformance tests
// (`dynamicNamesErrors`, `indexSignatures1`) regressed in PR #12321 because
// the METHOD_DECLARATION fallback that was added there made non-Symbol computed
// method keys (like `[someVar]()`) skip the index-signature path even when the
// key is a bare identifier. The fix restores source-text return for
// identifier/property-access computed keys and adds a `this`-keyword guard.

#[test]
fn computed_method_key_bare_identifier_emits_as_named_member() {
    // A method with a computed bare-identifier key (`[someVar]()`) must appear
    // in the inferred object return type as a named member using the source text
    // `[someVar]`, not be silently dropped or routed to an index signature.
    // (Regression from PR #12321 METHOD_DECLARATION fallback removal.)
    let output = emit_dts_with_usage_analysis(
        r#"
declare const someVar: unique symbol;
export function make() {
    return { [someVar]() { return 1; } };
}
"#,
    );

    assert!(
        output.contains("[someVar]"),
        "Expected computed bare-identifier method key [someVar] to appear as a named member: {output}"
    );
}

#[test]
fn computed_this_property_key_routes_to_index_signature() {
    // A method whose computed key is `[this.prop]` cannot be reproduced as a
    // stable property name in a .d.ts without type info. The emitter must
    // route it through the synthesized index-signature path, not emit it as a
    // named member. (Regression guard: `this` is a keyword, not an identifier.)
    let output = emit_dts_with_usage_analysis(
        r#"
export class Container {
    key = "x";
    build() {
        return { [this.key]() { return 1; } };
    }
}
"#,
    );

    // The inferred return type must not produce a named member `[this.key]`.
    assert!(
        !output.contains("[this.key]"),
        "Did not expect [this.key] to appear as a named member (should go to index sig): {output}"
    );
}

#[test]
fn computed_symbol_access_key_emits_as_named_member() {
    // `[Symbol.iterator]()` is a known unique-symbol access and must always
    // appear as a named member regardless of the source-text path.
    let output = emit_dts_with_usage_analysis(
        r#"
export function make() {
    return { [Symbol.iterator]() { return 1; } };
}
"#,
    );

    assert!(
        output.contains("[Symbol.iterator]"),
        "Expected [Symbol.iterator] computed method key to appear as a named member: {output}"
    );
}

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
        output.contains("    }): {\n        b: any;\n    };"),
        "Expected inferred method object return type to nest one level deeper than the method: {output}"
    );
    // The bug emitted members at the base indent (4 spaces) with a column-0
    // closing brace; ensure that broken shape is gone.
    assert!(
        !output.contains("    }): {\n    b: any;\n};"),
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
        output.contains("        }): {\n            size: any;\n        };"),
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
        output.contains("        meta: {\n            tag: any;\n        };"),
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
        output.contains("): {\n    b: any;\n};"),
        "Expected a top-level function inferred object return type to keep base indentation: {output}"
    );
    assert!(
        !output.contains("): {\n        b: any;\n    };"),
        "Did not expect a top-level function return type to be indented as a class member: {output}"
    );
}
