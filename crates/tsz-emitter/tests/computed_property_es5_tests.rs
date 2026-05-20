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

fn assert_comment_forced_comma_is_outdented(output: &str, assignment_fragment: &str) {
    let lines: Vec<&str> = output.lines().collect();
    let assignment_line = lines
        .iter()
        .position(|line| line.contains(assignment_fragment))
        .expect("lowered computed assignment line should be present");
    let comma_line = lines
        .get(assignment_line + 1)
        .expect("separator comma should follow the trailing comment line");
    let temp_line = lines
        .get(assignment_line + 2)
        .expect("temp return should follow the separator comma line");

    let leading_spaces = |line: &str| line.bytes().take_while(|&byte| byte == b' ').count();
    let assignment_indent = leading_spaces(lines[assignment_line]);
    let comma_indent = leading_spaces(comma_line);

    assert_eq!(
        comma_line.trim(),
        ",",
        "Trailing line comments on lowered computed members should put the separator comma on its own line.\nOutput:\n{output}"
    );
    assert_eq!(
        comma_indent + 4,
        assignment_indent,
        "Separator comma should be one indent level shallower than the computed assignment after a trailing line comment.\nOutput:\n{output}"
    );
    assert_eq!(
        leading_spaces(temp_line),
        assignment_indent,
        "Temp return should keep the computed-assignment continuation indent.\nOutput:\n{output}"
    );
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

#[test]
fn computed_method_super_call_in_arrow_uses_lexical_this() {
    let source = r#"
class Base {
    bar() { return 0; }
}
class C extends Base {
    foo() {
        () => {
            var obj = { [super.bar()]() { } };
        };
        return 0;
    }
}
"#;
    let output = emit_es5(source);

    assert!(
        output.contains("var _this = this;"),
        "Arrow with a super call in a computed key should capture lexical this.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_super.prototype.bar.call(_this)"),
        "Super method calls inside lowered arrow computed keys should bind the lexical this alias.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_super.prototype.bar.call(this)"),
        "Super method calls inside the arrow must not bind the nested function's this.\nOutput:\n{output}"
    );
}

#[test]
fn computed_method_trailing_comment_aligns_comma_expression_separator() {
    let source = r#"
class Base {
    bar() { return 0; }
}
class C extends Base {
    foo() {
        () => {
            var obj = {
                [super.bar()]() { } // needs capture
            };
        }
        return 0;
    }
}
"#;
    let output = emit_es5(source);

    assert_comment_forced_comma_is_outdented(
        &output,
        "_a[_super.prototype.bar.call(_this)] = function () { } // needs capture",
    );
}

#[test]
fn computed_method_super_element_call_in_arrow_uses_lexical_this() {
    let source = r#"
class Base {
    bar() { return 0; }
}
class C extends Base {
    foo(key) {
        () => {
            var obj = { [super[key]()]() { } };
        };
    }
}
"#;
    let output = emit_es5(source);

    assert!(
        output.contains("_super.prototype[key].call(_this)"),
        "Super element calls inside lowered arrow computed keys should bind the lexical this alias.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_super.prototype[key].call(this)"),
        "Super element calls inside the arrow must not bind the nested function's this.\nOutput:\n{output}"
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

/// When the first spread in an ES5 object literal is itself an object literal,
/// tsc uses it directly as the `__assign` target instead of allocating `{}`.
///
/// Rule: `{ ...{x} }` → `__assign({x})` (single arg, literal is the target)
///       `{ ...v }` → `__assign({}, v)` (variable: fresh empty target)
#[test]
fn object_literal_spread_uses_literal_as_assign_target() {
    // Single spread that is an object literal: __assign({x: 0}) not __assign({}, {x: 0})
    let output = emit_es5("const a = { ...{ x: 0 } };");
    assert!(
        output.contains("__assign({ x: 0 })"),
        "Object literal spread should be used as direct __assign target.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__assign({}, { x: 0 })"),
        "Should NOT create empty {{}} for object literal spread.\nOutput:\n{output}"
    );

    // Object literal spread first, then own props: __assign({x:0}, {y:1})
    let output2 = emit_es5("const b = { ...{ x: 0 }, y: 1 };");
    assert!(
        output2.contains("__assign({ x: 0 }, { y: 1 })"),
        "Object literal spread + own props: literal as target, own as source.\nOutput:\n{output2}"
    );
    assert!(
        !output2.contains("__assign(__assign"),
        "Should NOT double-wrap for object literal spread + own props.\nOutput:\n{output2}"
    );

    // Variable spread: keeps standard __assign({}, v) form
    let output3 = emit_es5("declare const v: any; const c = { ...v };");
    assert!(
        output3.contains("__assign({}, v)"),
        "Variable spread must still use fresh {{}} as target.\nOutput:\n{output3}"
    );

    // Variable spread + own props: __assign(__assign({}, v), {y:1})
    let output4 = emit_es5("declare const v: any; const d = { ...v, y: 1 };");
    assert!(
        output4.contains("__assign(__assign({}, v), { y: 1 })"),
        "Variable spread + own props must use nested __assign.\nOutput:\n{output4}"
    );
}

/// Object-literal spreads are only safe as direct `__assign` targets when the
/// inner object literal is simple. If the inner literal has its own spread, tsc
/// lowers that inner literal to a separate assign chain and copies it through
/// `{}` at the outer spread boundary.
#[test]
fn nested_object_literal_spread_keeps_empty_outer_assign_target() {
    let output = emit_es5("declare const b: any; const a = { ...{ a: 3, ...b } };");
    assert!(
        output.contains("__assign({}, __assign({ a: 3 }, b))"),
        "Nested object-literal spread should copy the inner assign result through {{}}.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__assign(__assign({ a: 3 }, b))"),
        "Nested object-literal spread should not use the inner assign result as the outer target.\nOutput:\n{output}"
    );

    let output2 = emit_es5("declare const b: any; const a = { ...{ a: 3, ...b }, c: 1 };");
    assert!(
        output2.contains("__assign(__assign({}, __assign({ a: 3 }, b)), { c: 1 })"),
        "Nested object-literal spread with trailing props should keep the empty outer target.\nOutput:\n{output2}"
    );
    assert!(
        !output2.contains("__assign(__assign({ a: 3 }, b), { c: 1 })"),
        "Trailing props should not mutate the inner object-spread assign result.\nOutput:\n{output2}"
    );
}
