// Multiple JSDoc tags on one physical line in declaration emit.
//
// Oracle (typescript@7.0.2, `--allowJs --checkJs --declaration
// --emitDeclarationOnly`): tsc's JSDoc scanner ends a tag's comment at an
// `@` that follows whitespace and starts a tag name, so one physical line
// can carry several tags. An `@` inside a braced group (`{@link ...}`) or a
// backtick code span, or glued to preceding text (`user@host`), stays plain
// comment text.

use super::*;

#[test]
fn two_param_tags_on_one_line_both_parse() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @param {number} [a] @param {number} b */
function f(a, b) { return b; }
module.exports = f;
"#,
    );

    assert!(
        output.contains("declare function f(a?: number, b: number)"),
        "Both `@param` tags on one line must parse (`a?: number, b: number`): {output}"
    );
}

#[test]
fn optional_type_marker_then_bracket_optional_on_one_line() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @param {number=} start @param {string} [label] */
function tick(start, label) { return label; }
module.exports = tick;
"#,
    );

    assert!(
        output.contains("declare function tick(start?: number | undefined, label?: string)"),
        "`{{T=}}` then `[name]` on one line keep their own optionality forms: {output}"
    );
}

#[test]
fn param_tag_followed_by_returns_tag_on_one_line_still_parses() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @param {number} amount @returns {string} */
function fmt(amount) { return String(amount); }
module.exports = fmt;
"#,
    );

    assert!(
        output.contains("declare function fmt(amount: number)"),
        "A `@param` ending at a following `@returns` keeps its type: {output}"
    );
}

#[test]
fn inline_link_in_description_does_not_split_or_leak() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @param {number} first - see {@link other} @param {string} second */
function pair(first, second) { return second; }
module.exports = pair;
"#,
    );

    assert!(
        output.contains("declare function pair(first: number, second: string)"),
        "`{{@link ...}}` stays in the description; the `@param` after it still parses: {output}"
    );
}

#[test]
fn backtick_code_span_protects_tag_text() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @param {number} real - use `@param {string} fake` in docs */
function solo(real) { return real; }
module.exports = solo;
"#,
    );

    assert!(
        output.contains("declare function solo(real: number)"),
        "A backtick-quoted `@param` is comment text, not a tag: {output}"
    );
    assert!(
        !output.contains("fake: string") && !output.contains("fake?: string"),
        "The backtick-quoted fake param must not materialize in the signature: {output}"
    );
}

#[test]
fn glued_at_sign_does_not_start_a_tag() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @param {string} addr mail user@host.example @param {string} subject */
function send(addr, subject) { return subject; }
module.exports = send;
"#,
    );

    assert!(
        output.contains("declare function send(addr: string, subject: string)"),
        "A glued `@` (email) is not a tag boundary; the whitespace-preceded `@param` is: {output}"
    );
}

#[test]
fn leading_description_then_param_tag_on_same_line() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** Computes a thing @param {number} seed */
function compute(seed) { return seed; }
module.exports = compute;
"#,
    );

    assert!(
        output.contains("declare function compute(seed: number)"),
        "A `@param` after leading description text on the same line parses: {output}"
    );
}
