use super::*;

#[test]
fn jsdoc_type_after_trailing_line_comment_uses_template_default_alias() {
    let output = emit_js_dts(
        r#"
/**
 * @template {string | number} [T=string] - ok: defaults are permitted
 * @typedef {[T]} A
 */

/** @type {A} */ // default for T comes from A
const aDefault1 = [""];
/** @type {A<number>} */ // explicit type argument
const aNumber = [0];
"#,
    );

    assert!(
        output.contains("declare const aDefault1: A;"),
        "Expected trailing line comment after @type to preserve A annotation: {output}"
    );
    assert!(
        output.contains("/** @type {A} */ declare const aDefault1: A;"),
        "Expected single-line @type JSDoc to stay on the declaration line: {output}"
    );
    assert!(
        output.contains("declare const aNumber: A<number>;"),
        "Expected trailing line comment after generic @type to preserve A<number> annotation: {output}"
    );
    assert!(
        output.contains("/** @type {A<number>} */ declare const aNumber: A<number>;"),
        "Expected single-line generic @type JSDoc to stay on the declaration line: {output}"
    );
    assert!(
        output.contains("type A<T extends string | number = string> = [T];"),
        "Expected preceding typedef alias with constrained default to be emitted: {output}"
    );
}

#[test]
fn jsdoc_single_line_type_comment_stays_inline_for_renamed_alias() {
    let output = emit_js_dts(
        r#"
/**
 * @template [Value=number]
 * @typedef {[Value]} Wrapped
 */

/** @type {Wrapped} */
const defaultWrapped = [1];
/** @type {Wrapped<string>} */
const stringWrapped = [""];
"#,
    );

    assert!(
        output.contains("/** @type {Wrapped} */ declare const defaultWrapped: Wrapped;"),
        "Expected renamed defaulted @type alias comment to stay inline: {output}"
    );
    assert!(
        output.contains(
            "/** @type {Wrapped<string>} */ declare const stringWrapped: Wrapped<string>;"
        ),
        "Expected renamed generic @type alias comment to stay inline: {output}"
    );
    assert!(
        output.contains("type Wrapped<Value = number> = [Value];"),
        "Expected renamed typedef alias with default to be emitted: {output}"
    );
}

#[test]
fn jsdoc_multiline_type_comment_stays_block_before_declaration() {
    let output = emit_js_dts(
        r#"
/**
 * @typedef {[string]} Label
 */

/**
 * @type {Label}
 */
const label = [""];
"#,
    );

    assert!(
        output.contains("/**\n * @type {Label}\n */\ndeclare const label: Label;"),
        "Expected multiline @type JSDoc to remain a block before the declaration: {output}"
    );
}

#[test]
fn jsdoc_source_preserved_block_keeps_relative_star_indent() {
    let output = emit_js_dts(
        r#"
 /**
 * @template {string | number} [T=string]
 * @template U
 * @param {T} value
 * @param {U} other
 */
function pair(value, other) {}
"#,
    );

    assert!(
        output.contains(
            "/**\n* @template {string | number} [T=string]\n* @template U\n* @param {T} value\n* @param {U} other\n*/\ndeclare function pair"
        ),
        "Expected source-preserved JSDoc star indentation to match tsc: {output}"
    );
}
