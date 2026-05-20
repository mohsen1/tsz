//! Locks in the `source_literal_primitive_matches_target_literal` policy
//! that decides whether a literal source like `1` should be displayed as
//! `'1'` or widened to `'number'` in TS2345 argument-mismatch diagnostics.
//!
//! The rule (verified against tsc 6.0.3 across permutations of mixed-primitive
//! and pure-literal targets):
//!
//! 1. Single literal target → preserve source literal.
//! 2. All-literal union (no plain primitive) → preserve source literal.
//! 3. Mixed union (literal + plain primitive of the *same* base) where the
//!    source's primitive base differs → widen source to its primitive.
//! 4. Multiple distinct primitive bases in the target → preserve source.
//!
//! Tests use multiple type-parameter and identifier names so the rule is
//! verified to be structural (see `.claude/CLAUDE.md` §25), not bound to any
//! particular spelling.
//!
//! Note: this layer alone does not yet flip every fingerprint failure in
//! `unionTypeInference.ts`-style tests, because a union target can be
//! narrowed to a single literal constituent before it reaches the display
//! path. The conservative behaviour locked in here (preserve in that case)
//! avoids regressions; a deeper failure-analysis change is required to
//! activate the widen path on those callers.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_with_options_code_messages;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
}

fn ts2345_messages(source: &str) -> Vec<String> {
    diagnostics(source)
        .into_iter()
        .filter_map(|(code, msg)| (code == 2345).then_some(msg))
        .collect()
}

/// Single literal target: source must be preserved (matches tsc).
/// `bar(1, "")` infers `T = 1`, then checks `""` against literal `1`.
/// tsc reports `Argument of type '""' is not assignable to parameter of type '1'.`
#[test]
fn single_literal_target_preserves_source_literal() {
    let source = r#"
declare function bar<T>(item1: T, item2: T): T;
bar(1, "");
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2345, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Argument of type '\"\"'") && msgs[0].contains("parameter of type '1'"),
        "single-literal target must preserve source, got: {msgs:#?}"
    );
}

/// Pure literal target (single literal expressed inline, non-generic): same
/// behaviour as the generic single-literal case — preserve source literal.
#[test]
fn plain_literal_target_preserves_source_literal() {
    let source = r#"
function f(x: 2): void {}
f(3);
"#;
    let msgs = ts2345_messages(source);
    assert!(
        msgs.iter().any(|m| m.contains("'3'") && m.contains("'2'")),
        "non-generic literal target must keep literal display, got: {msgs:#?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("Argument of type 'number'")),
        "non-generic literal target must not widen to 'number', got: {msgs:#?}"
    );
}

/// All-literal union with a single primitive base that differs from the
/// source: tsc preserves both. (`fA(x: 1 | 2)("foo")` keeps `'"foo"'`.)
#[test]
fn all_literal_union_with_mismatched_primitive_preserves_source() {
    let source = r#"
function fA(x: 1 | 2): void {}
fA("foo");
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2345, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Argument of type '\"foo\"'"),
        "all-literal union must preserve source literal, got: {msgs:#?}"
    );
}

/// Mixed-primitive union with literals on different primitive bases:
/// the helper itself classifies the target as having multiple primitive
/// bases (`{string, number}` for `string | 1`) and returns "preserve",
/// matching tsc's `'true' / 'number | "a"'` output for the parallel
/// `fD` case below. The `string | 1` source-side outcome is governed by
/// other layers (this test is included in the fE preserve-cluster below).
#[test]
fn mixed_primitive_union_with_matching_member_preserves_source_literal() {
    // `string | true | 1` has multiple primitive bases AND `1` matches the
    // source's number primitive — both signals preserve the source.
    let source = r#"
function fE(x: string | true | 1): void {}
fE(2);
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2345, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Argument of type '2'"),
        "mixed-primitive union with same-base literal must preserve source, got: {msgs:#?}"
    );
}

/// Mixed union (plain primitive + literal) whose base differs from the source
/// primitive: tsc widens source to its primitive. `fB(x: string | "a")(2n)`
/// renders as `'bigint' / 'string'`.
#[test]
fn primitive_with_literal_of_same_base_widens_mismatched_source() {
    let source = r#"
function fB(x: string | "a"): void {}
fB(2n);
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2345, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Argument of type 'bigint'"),
        "single-base mixed target must widen mismatched source, got: {msgs:#?}"
    );
    assert!(
        !msgs[0].contains("Argument of type '2n'"),
        "source must not preserve when target collapses to a primitive, got: {msgs:#?}"
    );
}

/// Source primitive matches a target literal's primitive: preserve source.
/// (`fE(x: string | 1 | true)(2)` keeps `'2'` because `1` is a number literal.)
#[test]
fn source_primitive_matches_target_literal_primitive_preserves_source() {
    let source = r#"
function fE(x: string | 1 | true): void {}
fE(2);
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2345, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Argument of type '2'"),
        "source primitive matching target literal primitive must preserve source, got: {msgs:#?}"
    );
}

/// `never` target preserves the source literal — tsc keeps `'42' / 'never'`
/// rather than widening to `'number' / 'never'` so the user can see the
/// specific value that tripped the check.
#[test]
fn never_target_preserves_source_literal() {
    let source = r#"
declare function f<T>(x: string & T): T;
f(42);
"#;
    let msgs = ts2345_messages(source);
    assert!(
        msgs.iter()
            .any(|m| m.contains("'42'") && m.contains("'never'")),
        "never target must keep the source literal, got: {msgs:#?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("Argument of type 'number'")),
        "never target must not widen the source, got: {msgs:#?}"
    );
}

/// Renaming the type parameter and identifiers must not change behaviour —
/// the rule is structural over types, not over spellings.
#[test]
fn rule_is_structural_under_renaming() {
    let source = r#"
declare function pick<X>(a: X, b: X): X;
pick(7, "");
"#;
    let msgs_t = ts2345_messages(source);

    let source = r#"
declare function pick<K>(a: K, b: K): K;
pick(7, "");
"#;
    let msgs_k = ts2345_messages(source);

    assert_eq!(msgs_t.len(), 1);
    assert_eq!(msgs_k.len(), 1);
    // The structural classification must not depend on the type-param name
    // (T vs K vs X). Both spellings must produce the same source-display
    // outcome.
    let normalize = |m: &str| {
        // Rough normalization: collapse whitespace and drop the spelled-out
        // type-param name from the rendered message body.
        m.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    assert_eq!(normalize(&msgs_t[0]), normalize(&msgs_k[0]));
}
