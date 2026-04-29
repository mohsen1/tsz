//! Binding-element default-value assignability messages: source displays as
//! the default value's type and target as the *non-undefined* shape of the
//! declared property type.
//!
//! For `function f({ bar = null }: { bar?: number } = {}) {}`, tsc reports
//! `Type 'null' is not assignable to type 'number'.` — the binding default
//! fills the undefined slot, so the check is between the default value and
//! the property type with `| undefined` stripped. tsz used to render the
//! message as the local-binding type vs the unnarrowed annotation, producing
//! a wrong `Type 'number' is not assignable to type 'number | undefined'.`
//! diagnostic.

use tsz_checker::context::CheckerOptions;

fn diagnostics_for(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn destructuring_default_null_against_optional_number_displays_null_vs_number() {
    let diagnostics = diagnostics_for(
        r#"
interface Foo {
    readonly bar?: number;
}

function performFoo2({ bar = null }: Foo = {}) {
    return bar;
}
"#,
    );

    let ts2322: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one TS2322 for the destructuring default mismatch, got: {diagnostics:#?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'null'") && msg.contains("'number'") && !msg.contains("'number | undefined'"),
        "Expected `Type 'null' is not assignable to type 'number'.` (target stripped of `| undefined`), got: {msg}"
    );
}
