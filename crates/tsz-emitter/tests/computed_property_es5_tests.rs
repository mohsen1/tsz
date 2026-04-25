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
