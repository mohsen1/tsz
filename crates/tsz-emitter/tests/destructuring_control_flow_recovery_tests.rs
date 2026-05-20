//! Regression coverage for malformed destructuring initializer recovery.

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print;
use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::PrintOptions;

fn emit_es2015(source: &str) -> String {
    parse_and_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    )
}

#[test]
fn parenthesized_type_property_tail_stays_in_destructuring_initializer() {
    let output = emit_es2015(
        r#"
const {
  date,
} = (inspectedElement: any).props;
"#,
    );

    assert!(
        output.contains("const { date, } = (inspectedElement) => , props;"),
        "Recovered property tail should stay on the malformed arrow initializer.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\nprops;"),
        "Recovered property tail must not emit again as a separate statement.\nOutput:\n{output}"
    );
}

#[test]
fn parenthesized_type_property_tail_uses_actual_member_name() {
    let output = emit_es2015(
        r#"
const {
  value,
} = (target: Example).fieldName;
"#,
    );

    assert!(
        output.contains("const { value, } = (target) => , fieldName;"),
        "Recovery should use the source member name, not a fixed fixture spelling.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\nfieldName;"),
        "Recovered member must not emit again as a standalone expression statement.\nOutput:\n{output}"
    );
}
