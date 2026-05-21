use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::check_source;

fn codes(source: &str) -> Vec<u32> {
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
    .map(|d| d.code)
    .collect()
}

fn has_ts2322(source: &str) -> bool {
    codes(source).contains(&2322)
}

fn no_errors(source: &str) -> bool {
    codes(source).is_empty()
}

// ── ${any} placeholder ────────────────────────────────────────────────────────

#[test]
fn string_not_assignable_to_template_any() {
    // tsc: TS2322 — `string` is NOT assignable to `` `${any}` ``
    assert!(
        has_ts2322("declare const s: string; const a: `${any}` = s;"),
        "`string` must not be assignable to `${{any}}`"
    );
}

#[test]
fn string_literal_assignable_to_template_any() {
    // tsc: ok — a string literal IS assignable to `` `${any}` ``
    assert!(
        no_errors(r#"const a: `${any}` = "hello";"#),
        "string literal must be assignable to `${{any}}`"
    );
}

#[test]
fn any_assignable_to_template_any() {
    // tsc: ok — `any` itself is assignable to any type
    assert!(
        no_errors("declare const x: any; const a: `${any}` = x;"),
        "`any` must be assignable to `${{any}}`"
    );
}

#[test]
fn template_any_is_subtype_of_string() {
    // tsc: ok — all template literal types are subtypes of `string`
    assert!(
        no_errors("declare const a: `${any}`; const s: string = a;"),
        "`${{any}}` must be assignable to `string`"
    );
}

#[test]
fn prefixed_any_rejects_string() {
    // `prefix-${any}` also rejects a bare `string`
    assert!(
        has_ts2322("declare const s: string; const a: `prefix-${any}` = s;"),
        "`string` must not be assignable to `prefix-${{any}}`"
    );
}

#[test]
fn prefixed_any_accepts_matching_literal() {
    // `prefix-${any}` accepts a literal starting with "prefix-"
    assert!(
        no_errors(r#"const a: `prefix-${any}` = "prefix-hello";"#),
        "matching literal must be assignable to `prefix-${{any}}`"
    );
}

// ── ${unknown} placeholder ────────────────────────────────────────────────────

#[test]
fn string_not_assignable_to_template_unknown() {
    // tsc: TS2322 — `string` is NOT assignable to `` `${unknown}` ``
    assert!(
        has_ts2322("declare const s: string; const a: `${unknown}` = s;"),
        "`string` must not be assignable to `${{unknown}}`"
    );
}

#[test]
fn string_literal_assignable_to_template_unknown() {
    // tsc: ok — a string literal IS assignable to `` `${unknown}` ``
    assert!(
        no_errors(r#"const a: `${unknown}` = "hello";"#),
        "string literal must be assignable to `${{unknown}}`"
    );
}

#[test]
fn template_unknown_is_subtype_of_string() {
    // tsc: ok — all template literal types are subtypes of `string`
    assert!(
        no_errors("declare const a: `${unknown}`; const s: string = a;"),
        "`${{unknown}}` must be assignable to `string`"
    );
}

#[test]
fn multi_span_unknown_rejects_string() {
    // `a${unknown}b` also rejects a bare `string`
    assert!(
        has_ts2322("declare const s: string; const a: `a${unknown}b` = s;"),
        "`string` must not be assignable to `a${{unknown}}b`"
    );
}

#[test]
fn multi_span_unknown_accepts_matching_literal() {
    assert!(
        no_errors(r#"const a: `a${unknown}b` = "axyzb";"#),
        "matching literal must be assignable to `a${{unknown}}b`"
    );
}
