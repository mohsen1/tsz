// JSDoc optionality vs `undefined` in declaration emit.
//
// Oracle (typescript@7.0.2, `--allowJs --checkJs --declaration
// --emitDeclarationOnly`, identical under `--strict` on and off): the
// bracketed-name form (`@param {T} [a]`, `@property {T} [x]`) marks the
// entity optional with `?` and prints the annotated type unchanged; the
// optional-type marker form (`{T=}`) marks it optional AND serializes the
// type as `T | undefined`, because the marker is part of the written type.
// A written `undefined` branch (`{T|undefined}`) is always kept.

use super::*;

#[test]
fn jsdoc_bracket_optional_param_prints_type_without_undefined() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {number} [count]
 */
function walk(count) { return count; }
module.exports = walk;
"#,
    );

    assert!(
        output.contains("declare function walk(count?: number)"),
        "Bracket-optional JSDoc param must print `count?: number` with no `| undefined`: {output}"
    );
    assert!(
        !output.contains("count?: number | undefined"),
        "Bracket optionality alone must not add `| undefined`: {output}"
    );
}

#[test]
fn jsdoc_bracket_optional_param_with_default_prints_type_without_undefined() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {number} [step=5]
 */
function advance(step) { return step; }
module.exports = advance;
"#,
    );

    assert!(
        output.contains("declare function advance(step?: number)"),
        "Bracket-with-default JSDoc param must print `step?: number`: {output}"
    );
    assert!(
        !output.contains("| undefined"),
        "Bracket-with-default must not add `| undefined`: {output}"
    );
}

#[test]
fn jsdoc_optional_type_marker_param_prints_undefined_union() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {number=} depth
 */
function descend(depth) { return depth; }
module.exports = descend;
"#,
    );

    assert!(
        output.contains("declare function descend(depth?: number | undefined)"),
        "`{{T=}}` marker must print `depth?: number | undefined`: {output}"
    );
}

#[test]
fn jsdoc_optional_type_marker_with_bracket_prints_single_undefined() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {number=} [width]
 */
function resize(width) { return width; }
module.exports = resize;
"#,
    );

    assert!(
        output.contains("declare function resize(width?: number | undefined)"),
        "`{{T=}} [name]` combines to `width?: number | undefined`: {output}"
    );
    assert!(
        !output.contains("undefined | undefined"),
        "The undefined branch must not be doubled: {output}"
    );
}

#[test]
fn jsdoc_written_undefined_branch_is_kept_for_bracket_optional() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {number|undefined} [limit]
 */
function clamp(limit) { return limit; }
module.exports = clamp;
"#,
    );

    assert!(
        output.contains("declare function clamp(limit?: number | undefined)"),
        "A written `undefined` branch is kept: {output}"
    );
}

#[test]
fn jsdoc_typedef_property_optionality_forms() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @typedef {Object} Opts
 * @property {number} [area]
 * @property {string=} label
 */
/** @param {Opts} o */
function build(o) { return o; }
module.exports = build;
"#,
    );

    assert!(
        output.contains("area?: number;"),
        "Bracket-optional @property prints `area?: number;` with no undefined: {output}"
    );
    assert!(
        output.contains("label?: string | undefined;"),
        "`{{T=}}` @property prints `label?: string | undefined;`: {output}"
    );
    assert!(
        !output.contains("string=;") && !output.contains("label: string="),
        "The raw `=` marker must never leak into the emitted type: {output}"
    );
}

#[test]
fn jsdoc_nested_object_literal_member_keeps_undefined_for_corpus_parity() {
    // Pinned-corpus parity (jsDeclarationsOptionalTypeLiteralProps1/2): a
    // bracket-optional member of a synthesized JSDoc object type literal
    // keeps `| undefined`, unlike plain parameter positions.
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @param {object} o
 * @param {object} o.inner
 * @param {number} [o.inner.gap]
 */
function nest({ inner }) { return inner; }
module.exports = nest;
"#,
    );

    assert!(
        output.contains("gap?: number | undefined;"),
        "Nested bracket-optional object-literal member keeps `| undefined` per the pinned corpus: {output}"
    );
}

#[test]
fn jsdoc_setter_param_optionality_forms() {
    // Pinned-corpus parity (jsDeclarationsReusesExistingTypeAnnotations):
    // JSDoc-optional setter params emit without `?`; the `{T=}` marker still
    // owns the `| undefined` branch, and bracket optionality adds nothing.
    let output = emit_js_dts(
        r#"
class Widget {
    /** @param {number} [v] */
    set size(v) {}
    /** @param {string=} t */
    set title(t) {}
}
"#,
    );

    assert!(
        output.contains("set size(v: number);"),
        "Bracket-optional setter param prints `v: number` with no `?` and no undefined: {output}"
    );
    assert!(
        output.contains("set title(t: string | undefined);"),
        "`{{T=}}` setter param prints `t: string | undefined`: {output}"
    );
}
