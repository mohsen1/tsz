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
        output.contains("declare const aNumber: A<number>;"),
        "Expected trailing line comment after generic @type to preserve A<number> annotation: {output}"
    );
    assert!(
        output.contains("type A<T extends string | number = string> = [T];"),
        "Expected preceding typedef alias with constrained default to be emitted: {output}"
    );
}
