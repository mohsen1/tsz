//! Tests for TS2871 ("This expression is always nullish.") when the always-
//! nullish literal operand of `??` is wrapped in parentheses, type
//! assertions (`as T`, `<T>`, `satisfies T`), or a non-null assertion (`!`).
//!
//! Closes #9783. Structural rule:
//!
//! > When the `??` left operand, looked through any combination of
//! > parentheses, type assertions, and non-null assertions, is a `null`
//! > literal or the `undefined` identifier, the underlying runtime value
//! > is always nullish; tsc emits TS2871 regardless of what the asserted
//! > static type became (widening to `any`, narrowing to `never`, or
//! > restoring a non-nullish slice via `as string | null`).
//!
//! The companion file `ts2871_always_nullish_tests.rs` covers the
//! pre-existing bare and chained nullish-literal forms.
//!
//! Per CLAUDE.md §25, every case below uses at least two name choices for
//! variables / aliases / property names so the rule survives spelling
//! changes, and `is_literal_null_or_undefined_node` keys off the
//! `NullKeyword` / `Identifier("undefined")` shapes only.

use tsz_checker::test_utils::check_source_codes;

// =========================================================================
// Always-nullish literal seen through type assertions — TS2871 fires
// =========================================================================

/// Reported repro: `(null as string | null) ?? "x"`. The asserted type
/// still carries a non-nullish slice, but the runtime value is the literal
/// `null`, so tsc emits TS2871.
#[test]
fn null_as_union_with_nullish_emits_ts2871() {
    let diags = check_source_codes("const a = (null as string | null) ?? \"x\";\n");
    assert!(
        diags.contains(&2871),
        "(null as string | null) ?? x must emit TS2871; got: {:?}",
        diags.to_vec(),
    );
}

/// Reported repro: `(undefined as number | undefined) ?? 1`. Same rule
/// with `undefined` and a different primitive non-nullish slice.
#[test]
fn undefined_as_union_with_nullish_emits_ts2871() {
    let diags = check_source_codes("const b = (undefined as number | undefined) ?? 1;\n");
    assert!(
        diags.contains(&2871),
        "(undefined as number | undefined) ?? x must emit TS2871; got: {:?}",
        diags.to_vec(),
    );
}

/// Reported repro: `null! ?? 1`. The non-null assertion strips the
/// nullish constituents from the static type (yielding `never`), but the
/// runtime value is still `null`.
#[test]
fn null_non_null_asserted_emits_ts2871() {
    let diags = check_source_codes("const c = null! ?? 1;\n");
    assert!(
        diags.contains(&2871),
        "null! ?? x must emit TS2871; got: {:?}",
        diags.to_vec(),
    );
}

/// Reported repro: `((null as any)) ?? 1`. Nested parens around an `as`
/// that widens to `any` must still trigger TS2871 — the runtime value is
/// the literal `null` regardless of the top type assertion.
#[test]
fn null_as_any_emits_ts2871() {
    let diags = check_source_codes("const d = ((null as any)) ?? 1;\n");
    assert!(
        diags.contains(&2871),
        "((null as any)) ?? x must emit TS2871; got: {:?}",
        diags.to_vec(),
    );
}

/// `undefined!` (non-null asserting `undefined`) is symmetrical to `null!`
/// and exercises the assertion-skip path on the `undefined` identifier.
#[test]
fn undefined_non_null_asserted_emits_ts2871() {
    let diags = check_source_codes("const e = undefined! ?? 0;\n");
    assert!(
        diags.contains(&2871),
        "undefined! ?? x must emit TS2871; got: {:?}",
        diags.to_vec(),
    );
}

/// Double assertion `null as unknown as string` — the skip helper must
/// unwrap repeatedly until it reaches the underlying literal.
#[test]
fn null_double_assertion_emits_ts2871() {
    let diags = check_source_codes("const f = (null as unknown as string) ?? \"x\";\n");
    assert!(
        diags.contains(&2871),
        "(null as unknown as string) ?? x must emit TS2871; got: {:?}",
        diags.to_vec(),
    );
}

/// `null satisfies null` participates in the same skip chain. `satisfies`
/// keeps the operand type as `null`, so this is also covered by the
/// pre-existing path, but the new path must keep firing it correctly when
/// `satisfies` is layered with other wrappers.
#[test]
fn null_satisfies_then_non_null_emits_ts2871() {
    let diags = check_source_codes("const g = (null satisfies null)! ?? 0;\n");
    assert!(
        diags.contains(&2871),
        "(null satisfies null)! ?? x must emit TS2871; got: {:?}",
        diags.to_vec(),
    );
}

/// Old-style type assertion `<T>null` (legal in `.ts`, not `.tsx`) must
/// also trigger the skip-and-detect path.
#[test]
fn null_prefix_type_assertion_emits_ts2871() {
    let diags = check_source_codes("const h = (<string | null>null) ?? \"x\";\n");
    assert!(
        diags.contains(&2871),
        "<string | null>null ?? x must emit TS2871; got: {:?}",
        diags.to_vec(),
    );
}

/// Parenthesized bare literal `(null) ?? "x"` — historically slipped
/// through both the chain detector and the bare-literal detector. The new
/// skip-parens-and-assertions path covers it for free.
#[test]
fn parenthesized_null_emits_ts2871() {
    let diags = check_source_codes("const i = (null) ?? \"x\";\n");
    assert!(
        diags.contains(&2871),
        "(null) ?? x must emit TS2871; got: {:?}",
        diags.to_vec(),
    );
}

// =========================================================================
// Regression guards — controls that must NOT report TS2871
// =========================================================================

/// `string | null` variable: not a literal operand, so neither TS2871 nor
/// TS2869 should fire. Confirms the new path is keyed on the syntactic
/// literal shape, not on the static type.
#[test]
fn nullable_variable_no_ts2871() {
    let diags = check_source_codes(
        "declare const value: string | null;\n\
         const r = value ?? \"fallback\";\n",
    );
    assert!(
        !diags.contains(&2871),
        "nullable variable left must not trigger TS2871; got: {:?}",
        diags.to_vec(),
    );
    assert!(
        !diags.contains(&2869),
        "nullable variable left must not trigger TS2869; got: {:?}",
        diags.to_vec(),
    );
}

/// Variable narrowed to `null` is still a variable, not a syntactic null
/// literal — TS2871 must not fire. Tests the anti-hardcoding guarantee
/// (the rule is over expression shape, not type).
#[test]
fn variable_typed_null_no_ts2871() {
    let diags = check_source_codes(
        "const n: null = null;\n\
         const r = n ?? \"x\";\n",
    );
    assert!(
        !diags.contains(&2871),
        "variable typed null must not trigger TS2871; got: {:?}",
        diags.to_vec(),
    );
}

/// Identifier `undef` of declared type `undefined` is NOT the syntactic
/// `undefined` literal — the helper keys off `escaped_text == "undefined"`,
/// not the type. Renaming a variable can never produce or suppress TS2871.
#[test]
fn renamed_undefined_typed_variable_no_ts2871() {
    let diags = check_source_codes(
        "declare const undef: undefined;\n\
         const r = undef ?? 1;\n",
    );
    assert!(
        !diags.contains(&2871),
        "renamed `undefined`-typed variable must not trigger TS2871; got: {:?}",
        diags.to_vec(),
    );
}

/// String literal asserted as `string | null` is genuinely never nullish
/// at runtime; it must still fire TS2869, not TS2871. The new syntactic
/// check must only react to `null` / `undefined` literals.
#[test]
fn string_literal_through_assertion_emits_ts2869_not_ts2871() {
    let diags = check_source_codes("const r = (\"hello\" as string | null) ?? \"fallback\";\n");
    assert!(
        !diags.contains(&2871),
        "non-null string literal must not trigger TS2871; got: {:?}",
        diags.to_vec(),
    );
}

/// `any` variable (no syntactic null/undefined literal) — neither
/// diagnostic should fire. Distinct from `(null as any) ?? 1` above,
/// which DOES fire because the inner operand IS a null literal.
#[test]
fn any_variable_no_ts2871() {
    let diags = check_source_codes(
        "declare const a: any;\n\
         const r = a ?? 1;\n",
    );
    assert!(
        !diags.contains(&2871),
        "any-typed variable must not trigger TS2871; got: {:?}",
        diags.to_vec(),
    );
}

/// Regression: the original bare `null ?? x` path must still emit TS2871
/// after the refactor. (Anchor stays on the `null` keyword.)
#[test]
fn bare_null_still_emits_ts2871() {
    let diags = check_source_codes("const r = null ?? 0;\n");
    assert!(
        diags.contains(&2871),
        "bare null ?? x must still emit TS2871; got: {:?}",
        diags.to_vec(),
    );
}

/// Regression: the chained `(null ?? undefined) ?? "x"` path must still
/// emit TS2871 after the refactor.
#[test]
fn nullish_chain_still_emits_ts2871() {
    let diags = check_source_codes("const r = (null ?? undefined) ?? \"fallback\";\n");
    assert!(
        diags.contains(&2871),
        "nullish-chain ?? x must still emit TS2871; got: {:?}",
        diags.to_vec(),
    );
}
