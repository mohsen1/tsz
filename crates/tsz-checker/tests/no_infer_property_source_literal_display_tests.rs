//! `NoInfer<T>` is a transparent wrapper for contextual literal typing, so a
//! fresh literal object-literal property assigned against a `NoInfer<C>` target
//! must be judged as if the target were `C` (tsc's `isLiteralOfContextualType`
//! erases `NoInfer<>` for primitives).
//!
//! Regression cover for #17491: #17488 widens a fresh object-literal property
//! literal to its primitive when the target rejects the literal's *domain*. The
//! domain decision runs through `contextual_type_allows_literal`, which did not
//! see through a `NoInfer<C>` target — so `{ x: 'bar' }` against
//! `{ x: NoInfer<T> }` (with `T` fixed to `"foo"`) reported the source widened
//! to `Type 'string'` instead of tsc's `Type '"bar"'`, because the same-domain
//! sibling literal `"foo"` was hidden behind the wrapper.
//!
//! The widening itself is preserved where tsc widens (a genuinely cross-domain
//! target), and the non-`NoInfer` #17488 cases are unchanged. Every expectation
//! below was verified against `typescript@6.0.2` / `7.0.2`. Binder names (the
//! function, the type parameter, the property, the argument value) are varied
//! across the matrix so the rule is proven structural, not keyed on a spelling.

use tsz_checker::test_utils::check_source_strict_messages;

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_strict_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.replace('\n', " | "))
        .collect()
}

#[track_caller]
fn assert_contains(source: &str, expected_substring: &str) {
    let messages = ts2322_messages(source);
    assert!(
        messages.iter().any(|m| m.contains(expected_substring)),
        "expected a TS2322 containing {expected_substring:?}, got: {messages:#?}",
    );
}

#[track_caller]
fn assert_not_contains(source: &str, forbidden_substring: &str) {
    let messages = ts2322_messages(source);
    assert!(
        !messages.iter().any(|m| m.contains(forbidden_substring)),
        "expected no TS2322 containing {forbidden_substring:?}, got: {messages:#?}",
    );
}

// --- same-domain source literal is preserved through the NoInfer wrapper -----

#[test]
fn string_literal_property_against_no_infer_string_keeps_literal() {
    // `T` fixes to `"foo"`, the `x` target is `NoInfer<"foo">`; tsc keeps `"bar"`.
    let source = r#"declare function foo4<T extends string>(a: T, b: { x: NoInfer<T> }): void
        foo4('foo', { x: 'bar' });"#;
    assert_contains(source, "Type '\"bar\"' is not assignable to type '\"foo\"'");
    assert_not_contains(source, "Type 'string'");
}

#[test]
fn number_literal_property_against_no_infer_number_keeps_literal() {
    // `T` fixes to `1`, the `p` target is `NoInfer<1>`; tsc keeps `3`.
    let source = r#"declare function tier<N extends number>(seed: N, cfg: { p: NoInfer<N> }): void
        tier(1, { p: 3 });"#;
    assert_contains(source, "Type '3' is not assignable to type '1'");
    assert_not_contains(source, "Type 'number'");
}

// --- cross-domain still widens (the NoInfer unwrap must not over-preserve) ---

#[test]
fn cross_domain_string_source_against_no_infer_numeric_literal_still_widens() {
    // `T` fixes to `1`, the `q` target is `NoInfer<1>`; a string source has no
    // number-literal sibling, so tsc widens to `string`.
    let source = r#"declare function ranked<T extends 1 | 2>(a: T, b: { q: NoInfer<T> }): void
        ranked(1, { q: 'bar' });"#;
    assert_contains(source, "Type 'string' is not assignable to type '1'");
    assert_not_contains(source, "Type '\"bar\"'");
}

// --- #17488 non-NoInfer widening is unchanged (guard) ------------------------

#[test]
fn string_literal_against_boolean_property_still_widens_to_string() {
    // #17488: no NoInfer here, so the source still widens (target rejects the
    // string domain). Assert only the widened source; the target rendering of
    // an optional property is orthogonal to this issue.
    let source = r#"interface Descriptor { configurable?: boolean }
        const flags: Descriptor = { configurable: "yes" };"#;
    assert_contains(source, "Type 'string'");
    assert_not_contains(source, "Type '\"yes\"'");
}

#[test]
fn string_literal_against_numeric_literal_union_property_still_widens_to_string() {
    let source = r#"interface Tier { rank?: 1 | 2 }
        const t: Tier = { rank: "hi" };"#;
    assert_contains(source, "Type 'string'");
    assert_not_contains(source, "Type '\"hi\"'");
}
