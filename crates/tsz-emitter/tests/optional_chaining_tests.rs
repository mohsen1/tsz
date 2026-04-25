//! Integration tests for optional chaining downlevel emit

use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::{parse_and_lower_print, parse_and_print_with_opts};

fn emit_es2016(source: &str) -> String {
    let opts = PrintOptions {
        target: ScriptTarget::ES2016,
        ..Default::default()
    };
    parse_and_lower_print(source, opts)
}

/// super.method?.() should capture `super.method` as a unit and use `.call(this)`.
/// See GH#34952.
#[test]
fn super_property_access_optional_call() {
    let source = r#"class Base { method() {} }
class Derived extends Base {
    method1() { return super.method?.(); }
}"#;
    let output = emit_es2016(source);
    // Must capture `super.method` as a unit, not `(_a = super).method`
    assert!(
        output.contains("(_a = super.method) === null || _a === void 0 ? void 0 : _a.call(this)"),
        "super.method?.() should capture super.method as a unit.\nOutput:\n{output}"
    );
    // Must NOT contain (_a = super)
    assert!(
        !output.contains("(_a = super)") && !output.contains("= super)"),
        "super must not be captured in a temp variable.\nOutput:\n{output}"
    );
}

/// super["method"]?.() should capture `super["method"]` as a unit.
#[test]
fn super_element_access_optional_call() {
    let source = r#"class Base { method() {} }
class Derived extends Base {
    method2() { return super["method"]?.(); }
}"#;
    let output = emit_es2016(source);
    assert!(
        output.contains(
            "(_a = super[\"method\"]) === null || _a === void 0 ? void 0 : _a.call(this)"
        ),
        "super[\"method\"]?.() should capture super[\"method\"] as a unit.\nOutput:\n{output}"
    );
}

/// Hoisted var declarations should appear inline in single-line function bodies.
#[test]
fn hoisted_var_in_single_line_body() {
    let source = "class Base { method() {} }
class Derived extends Base {
    method1() { return super.method?.(); }
}";
    let opts = PrintOptions {
        target: ScriptTarget::ES2016,
        ..Default::default()
    };
    // parse_and_print_with_opts calls set_source_text (needed for single-line detection)
    let output = parse_and_print_with_opts(source, opts);
    assert!(
        output.contains("{ var _a; return"),
        "Single-line body should include inline var declaration.\nOutput:\n{output}"
    );
}
