use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::check_source;

fn codes_for_template_assignments(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn numeric_template_placeholders_reject_empty_string() {
    let codes = codes_for_template_assignments(
        r#"
let numericText: `${number}` = "";
let bigintText: `${bigint}` = "";
let stringText: `${string}` = "";
"#,
    );

    let ts2322_count = codes.iter().filter(|code| **code == 2322).count();
    assert_eq!(
        ts2322_count, 2,
        "expected empty number and bigint placeholders to emit TS2322 while empty string placeholder remains valid, got codes: {codes:?}"
    );
}
