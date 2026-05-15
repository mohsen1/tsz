//! Tests for TS2871 ("This expression is always nullish.").
//!
//! Closes #5913. The structural rule:
//!
//! > When the left operand of `??` is syntactically a nullish-coalescing
//! > chain or a bare nullish literal and its evaluated type contains only
//! > nullish constituents (split into non-nullish vs. cause yields no
//! > non-nullish part), the checker emits TS2871 at the left operand's
//! > anchor.
//!
//! tsc reference: `This expression is always nullish.` Complementary to
//! TS2869 ("Right operand of ?? is unreachable...").

use tsz_checker::test_utils::check_source_codes;

// =========================================================================
// Always-nullish literal chains — TS2871 fires
// =========================================================================

/// Direct repro from #6895: bare `null` and `undefined` left operands are
/// always nullish, so the `??` fallback is always selected.
#[test]
fn direct_nullish_literals_emit_ts2871() {
    let diags = check_source_codes(
        "const a: number = null ?? 0;\n\
         const b: number = undefined ?? 0;\n",
    );
    let count = diags.iter().filter(|&&code| code == 2871).count();
    assert_eq!(
        count,
        2,
        "null ?? x and undefined ?? x must both emit TS2871; got: {:?}",
        diags.to_vec(),
    );
}

/// Direct repro from #5913. `(null ?? undefined)` is always nullish, so
/// `(null ?? undefined) ?? "fallback"` should emit TS2871.
#[test]
fn null_then_undefined_nullish_chain_emits_ts2871() {
    let diags = check_source_codes("const result = (null ?? undefined) ?? \"fallback\";\n");
    assert!(
        diags.contains(&2871),
        "(null ?? undefined) ?? x must emit TS2871; got: {:?}",
        diags.to_vec(),
    );
}

/// Reversed order: `(undefined ?? null)` is also always nullish. Per
/// .claude/CLAUDE.md §25 anti-hardcoding, the rule must work both ways.
#[test]
fn undefined_then_null_nullish_chain_emits_ts2871() {
    let diags = check_source_codes("const r = (undefined ?? null) ?? 42;\n");
    assert!(
        diags.contains(&2871),
        "(undefined ?? null) ?? x must emit TS2871; got: {:?}",
        diags.to_vec(),
    );
}

/// Deeper chain — `null ?? undefined ?? null` collapses to always-nullish.
#[test]
fn deep_nullish_chain_emits_ts2871_at_top() {
    let diags = check_source_codes("const r = (null ?? undefined ?? null) ?? 0;\n");
    assert!(
        diags.contains(&2871),
        "Deep chain of nullish literals must emit TS2871; got: {:?}",
        diags.to_vec(),
    );
}

// =========================================================================
// Regression guards — TS2871 must NOT fire on non-nullish-only expressions
// =========================================================================

/// Mixed type: left is `string | null` — not always nullish. Neither
/// TS2869 nor TS2871 should fire.
#[test]
fn mixed_nullable_left_no_ts2871() {
    let diags = check_source_codes(
        "declare const x: string | null;\n\
         const r = x ?? \"fallback\";\n",
    );
    assert!(
        !diags.contains(&2871),
        "string | null is NOT always nullish; TS2871 must not fire; got: {:?}",
        diags.to_vec(),
    );
}

/// Never-nullish left (`"hello"`) emits TS2869, NOT TS2871. The
/// complementary branch must not accidentally also emit TS2871.
#[test]
fn never_nullish_emits_ts2869_not_ts2871() {
    let diags = check_source_codes("const r = \"hello\" ?? \"fallback\";\n");
    assert!(
        !diags.contains(&2871),
        "\"hello\" ?? x is never-nullish (TS2869 territory); TS2871 must not fire; got: {:?}",
        diags.to_vec(),
    );
    assert!(
        diags.contains(&2869),
        "Existing TS2869 must still fire for never-nullish left; got: {:?}",
        diags.to_vec(),
    );
}

/// `any` and `unknown` on the left must NOT trigger TS2871 (matches tsc;
/// the `left_is_top_type` guard in the producer covers this).
#[test]
fn top_type_left_no_ts2871() {
    let diags = check_source_codes(
        "declare const a: any;\n\
         declare const u: unknown;\n\
         const r1 = a ?? 1;\n\
         const r2 = u ?? 1;\n",
    );
    assert!(
        !diags.contains(&2871),
        "any/unknown left must not trigger TS2871; got: {:?}",
        diags.to_vec(),
    );
}
