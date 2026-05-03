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

/// When a binding pattern destructures through an *optional* property
/// without supplying a parent default, accessing nested properties produces
/// TS2339/TS2532 because the parent value may be `undefined`. tsc reports
/// only the upstream property-access error and **suppresses cascading
/// default-value TS2322s** inside the nested pattern. Mirror that
/// suppression: the cascade is noise — the user fixes the optional-parent
/// access first, and the secondary default mismatch becomes meaningful
/// only after the parent is non-undefined.
#[test]
fn nested_pattern_through_optional_parent_suppresses_cascade_ts2322() {
    // `nested?` is optional → element_type for nested = `{ p: 'a'|'b' } |
    // undefined`. Without a parent default to strip the `| undefined`,
    // tsc emits ONLY TS2339 for the `.p` access; the inner `p = 'c'`
    // default vs `'a' | 'b'` mismatch is suppressed.
    let diagnostics = diagnostics_for(
        r#"
function test({
    method = "z",
    nested: { p = "c" }
}: {
    method?: "x" | "y",
    nested?: { p: "a" | "b" }
}) {
    method;
    p;
}
"#,
    );
    let ts2322_for_inner_default: Vec<_> = diagnostics
        .iter()
        .filter(|(c, m)| *c == 2322 && m.contains("'\"c\"'"))
        .collect();
    assert!(
        ts2322_for_inner_default.is_empty(),
        "Expected NO TS2322 for the inner `p = \"c\"` default (cascade suppression through optional parent), got: {diagnostics:#?}"
    );
    // Sanity: the outer-default mismatch (`method = 'z'` vs `'x' | 'y'`)
    // and the upstream `.p` access (TS2339) must still fire.
    let outer_ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(c, m)| *c == 2322 && m.contains("'\"z\"'"))
        .collect();
    assert!(
        !outer_ts2322.is_empty(),
        "Outer `method = \"z\"` default mismatch must still fire, got: {diagnostics:#?}"
    );
    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        !ts2339.is_empty(),
        "Upstream TS2339 for `.p` access on possibly-undefined parent must still fire, got: {diagnostics:#?}"
    );
}

/// Same suppression rule with alternative property/literal name choices —
/// the fix must trigger structurally for any `pattern: { inner = default }`
/// where `pattern` is optional, not just one specific spelling.
#[test]
fn nested_pattern_through_optional_parent_suppresses_cascade_ts2322_alt_names() {
    let diagnostics = diagnostics_for(
        r#"
function run({
    flag = "on",
    config: { mode = "fast" }
}: {
    flag?: "on" | "off",
    config?: { mode: "slow" | "lazy" }
}) {
    flag;
    mode;
}
"#,
    );
    let ts2322_for_inner_default: Vec<_> = diagnostics
        .iter()
        .filter(|(c, m)| *c == 2322 && m.contains("'\"fast\"'"))
        .collect();
    assert!(
        ts2322_for_inner_default.is_empty(),
        "Expected NO TS2322 for the inner `mode = \"fast\"` default (cascade suppression, alt names), got: {diagnostics:#?}"
    );
}

/// When the parent binding element supplies a default (`= {}`), the
/// `| undefined` is stripped before recursing — so the inner default
/// mismatch DOES fire normally. Pins the inverse direction of the
/// suppression rule.
#[test]
fn nested_pattern_with_parent_default_does_not_suppress_inner_cascade() {
    let diagnostics = diagnostics_for(
        r#"
function test({
    nested: { p = "c" } = { p: "a" }
}: {
    nested?: { p: "a" | "b" }
}) {
    p;
}
"#,
    );
    let ts2322_for_inner_default: Vec<_> = diagnostics
        .iter()
        .filter(|(c, m)| *c == 2322 && m.contains("'\"c\"'"))
        .collect();
    assert!(
        !ts2322_for_inner_default.is_empty(),
        "When parent has default `= {{ p: \"a\" }}`, the inner default `p = \"c\"` mismatch MUST fire (no cascade), got: {diagnostics:#?}"
    );
}
