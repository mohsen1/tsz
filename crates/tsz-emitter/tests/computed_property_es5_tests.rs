//! Integration tests for ES5 computed property object literal formatting.
//!
//! tsc always formats the lowered comma expression as multi-line:
//!   (_a = {},
//!       _a[key] = value,
//!       _a);

use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print;

fn emit_es5(source: &str) -> String {
    let opts = PrintOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    parse_and_lower_print(source, opts)
}

/// Multiple computed properties should be emitted multi-line, one per line.
#[test]
fn computed_property_multiline_multiple_keys() {
    let source = r#"var v = { [p1]: 0, [p2]: 1, [p3]: 2 };"#;
    let output = emit_es5(source);
    // Each assignment should be on its own line (multi-line format)
    assert!(
        output.contains("_a[p1] = 0,\n"),
        "Each computed property assignment should be on its own line.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a[p2] = 1,\n"),
        "Each computed property assignment should be on its own line.\nOutput:\n{output}"
    );
}

/// Single computed property should also be emitted multi-line.
#[test]
fn computed_property_multiline_single_key() {
    let source = r#"var o = { [+"foo"]: "", [+"bar"]: 0 };"#;
    let output = emit_es5(source);
    // Should have comma + newline between assignments
    assert!(
        output.contains(",\n"),
        "Computed property comma expression should use multi-line format.\nOutput:\n{output}"
    );
    // Should NOT be all on one line
    assert!(
        !output.contains("_a = {}, _a"),
        "Computed property assignments should NOT be on a single line.\nOutput:\n{output}"
    );
}

/// The comma expression should start with (_a = {} on the first line.
#[test]
fn computed_property_format_structure() {
    let source = r#"var v = { [f("")]: 0, [f(0)]: 0, [f(true)]: 0 };"#;
    let output = emit_es5(source);
    // Opening: (_a = {},
    assert!(
        output.contains("(_a = {},"),
        "Comma expression should start with (_a = {{}},\nOutput:\n{output}"
    );
    // Closing: should end with _a)
    assert!(
        output.contains("_a)"),
        "Comma expression should end with _a).\nOutput:\n{output}"
    );
}

#[test]
fn computed_property_return_uses_function_scoped_temp_without_parens() {
    let source = r#"var a = "a";
var b = "b";
var result = (function () {
    return { [a]: 1, [b]: 1 };
})();"#;
    let output = emit_es5(source);

    assert!(
        output.contains(
            "function () {\n    var _a;\n    return _a = {}, _a[a] = 1, _a[b] = 1, _a;\n}"
        ),
        "Computed object return should declare the temp in the function and own the comma expression.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return (_a ="),
        "Return statements should not wrap ES5 computed-object lowering in a parenthesized multiline expression.\nOutput:\n{output}"
    );
}

/// Plain object-literal methods need only method-shorthand lowering, not the
/// computed-property temp/comma-expression transform.
#[test]
fn ordinary_method_lowers_inline_without_computed_temp() {
    let source = r#"var o = { x: 1, method() { return 2; } };"#;
    let output = emit_es5(source);

    assert!(
        output.contains("method: function () { return 2; }"),
        "Ordinary method shorthand should lower inline to a function value.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_a.method = function"),
        "Ordinary methods should not start the computed-property temp path.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(_a = { x: 1 },"),
        "Object literal should remain a normal literal when no computed property exists.\nOutput:\n{output}"
    );
}

#[test]
fn computed_method_still_uses_computed_temp() {
    let source = r#"var k = "method"; var o = { [k]() { return 2; } };"#;
    let output = emit_es5(source);

    assert!(
        output.contains("_a[k] = function () { return 2; }"),
        "Computed method must still use the ES5 computed-property temp path.\nOutput:\n{output}"
    );
}

/// Issue #3968: An object literal spread whose leading element segment
/// contains a computed property must lower the elements via the
/// `(_a = {}, _a[k] = 1, _a)` pattern, not as an ES2015 `{ [k]: 1 }`
/// literal. The emitted code must be valid ES5 with balanced parens.
#[test]
fn object_spread_leading_computed_property_lowers_to_assign_pattern() {
    let source = r#"const k = "x";
const b = { y: 2 };
const obj = { [k]: 1, ...b };"#;
    let output = emit_es5(source);

    // Must NOT keep the ES2015 computed-property literal form.
    assert!(
        !output.contains("[k]: 1"),
        "Computed property must be lowered, not emitted as ES2015 syntax.\nOutput:\n{output}"
    );

    // Must use the comma-expression lowering pattern.
    assert!(
        output.contains("_a[k] = 1"),
        "Computed property should be lowered to `_a[k] = 1`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__assign("),
        "Spread should be lowered with __assign helper.\nOutput:\n{output}"
    );

    // Parens must balance — Node would reject mismatched output.
    let opens = output.matches('(').count();
    let closes = output.matches(')').count();
    assert_eq!(
        opens, closes,
        "Emitted JS must have balanced parens.\nOutput:\n{output}"
    );
}
