//! Tests for `| undefined` display preservation on optional-property targets
//! in TS2322 messages at object-literal call-site arguments.
//!
//! Rule under test:
//!
//! > Under `--strict` (strictNullChecks), when an object-literal property
//! > assignment fails its assignability check against an *optional* property
//! > of the contextual target type, the displayed target type must include
//! > the synthesized `| undefined` arm. tsc renders this as
//! > `Type 'z' is not assignable to type '"x" | "y" | undefined'`. Without
//! > this preservation tsz strips `| undefined` because the source value is
//! > non-nullable, mismatching tsc's TS2322 fingerprint.
//!
//! These tests pin the rule with two distinct property/literal name
//! choices so the invariant is structural, not name-hardcoded.

use crate::context::CheckerOptions;
use crate::test_utils::check_with_options;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    check_with_options(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

/// `f({ method: 'z' })` against `f(x: { method?: 'x' | 'y' })`:
/// the TS2322 message must display the optional-aware target including
/// `| undefined`, mirroring tsc's `getTypeOfPropertyOfContextualType` for
/// optional properties under strictNullChecks.
#[test]
fn optional_property_call_site_target_display_includes_undefined() {
    let source = r#"
declare function f(x: { method?: "x" | "y" }): void;
f({ method: "z" });
"#;
    let diags = check_strict(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "expected a TS2322 for optional-property mismatch, got: {diags:#?}"
    );
    let msg = ts2322[0].1.as_str();
    assert!(
        msg.contains("\"x\" | \"y\" | undefined"),
        "TS2322 target display must include `| undefined` for optional property, got: {msg}"
    );
}

/// Same invariant with different property name (`flag`) and literal values
/// (`'on'/'off'/'maybe'`) — verifies the fix is not name-hardcoded.
#[test]
fn optional_property_call_site_target_display_includes_undefined_alt_names() {
    let source = r#"
declare function g(y: { flag?: "on" | "off" }): void;
g({ flag: "maybe" });
"#;
    let diags = check_strict(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "expected a TS2322 for optional-property mismatch, got: {diags:#?}"
    );
    let msg = ts2322[0].1.as_str();
    assert!(
        msg.contains("\"on\" | \"off\" | undefined"),
        "TS2322 target display must include `| undefined` for optional property (alt names), got: {msg}"
    );
}

/// Optional plain-primitive property: `name?: string` should NOT show
/// `| undefined` in TS2322. tsc's policy preserves `| undefined` only when
/// the underlying union has multiple non-undefined members; for a single
/// primitive it strips. Pinning this invariant prevents over-application
/// of the optional-property preservation rule.
#[test]
fn optional_plain_primitive_property_call_site_target_display_omits_undefined() {
    let source = r#"
declare function k(w: { name?: string }): void;
k({ name: false });
"#;
    let diags = check_strict(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "expected a TS2322 for optional-primitive mismatch, got: {diags:#?}"
    );
    for (_, msg) in &ts2322 {
        assert!(
            !msg.contains("| undefined"),
            "TS2322 target display must NOT include `| undefined` for optional plain-primitive property (single-primitive stripped form), got: {msg}"
        );
    }
}

/// Sanity: a *required* property (non-optional) does NOT show `| undefined`.
/// This pins the invariant in the opposite direction — the fix must not
/// over-apply and append `| undefined` to required-property targets.
#[test]
fn required_property_call_site_target_display_omits_undefined() {
    let source = r#"
declare function h(z: { method: "x" | "y" }): void;
h({ method: "z" });
"#;
    let diags = check_strict(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "expected a TS2322 for required-property mismatch, got: {diags:#?}"
    );
    let msg = ts2322[0].1.as_str();
    assert!(
        !msg.contains("undefined"),
        "TS2322 target display must NOT include `undefined` for required property, got: {msg}"
    );
}
