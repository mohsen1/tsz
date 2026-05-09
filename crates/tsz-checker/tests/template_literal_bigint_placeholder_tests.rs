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
fn bigint_template_placeholder_rejects_plus_signed_decimal() {
    let codes = codes_for_template_assignments(
        r#"
let plusBigint: `${bigint}` = "+1";
let plusNumber: `${number}` = "+1";
let negativeBigint: `${bigint}` = "-1";
"#,
    );

    let ts2322_count = codes.iter().filter(|code| **code == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "expected only plus-signed bigint assignment to emit TS2322, got codes: {codes:?}"
    );
}
