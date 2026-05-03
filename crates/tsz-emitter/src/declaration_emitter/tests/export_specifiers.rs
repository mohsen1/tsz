use super::*;

#[test]
fn type_modifier_ambiguous_export_specifiers_keep_local_values() {
    let output = emit_dts_with_usage_analysis(
        r#"
const type = 0;
const as = 0;
const something = 0;
export { type };
export { type as };
export { type something };
export { type type as foo };
export { type as as bar };
export type { type something as whatever };
"#,
    );

    assert!(
        output.contains("declare const type = 0;"),
        "Expected local `type` declaration to be preserved: {output}"
    );
    assert!(
        output.contains("declare const as = 0;"),
        "Expected ambiguous `type as` export to preserve local `as`: {output}"
    );
    assert!(
        output.contains("declare const something = 0;"),
        "Expected ambiguous `type something` export to preserve local `something`: {output}"
    );
    assert!(
        output.contains("export { type as };"),
        "Expected ambiguous type-only export specifier to be emitted: {output}"
    );
    assert!(
        output.contains("export { type something };"),
        "Expected ambiguous type-only export specifier to be emitted: {output}"
    );
}
