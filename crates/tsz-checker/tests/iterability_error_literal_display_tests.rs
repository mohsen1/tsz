//! The not-iterable diagnostics for `for-of`, `for-await-of`, `yield*`, and ES5
//! iteration sources must display the operand's own *unwidened* type, matching
//! `tsc`.
//!
//! `tsc` reports these errors (TS2488 / TS2504 / TS2495 / TS2461) using the
//! operand's checked type, so a fresh primitive-literal operand shows its literal
//! type (`42`, `true`, `123n`, `-5`) rather than the widened base (`number`,
//! `boolean`, `bigint`). tsz previously widened single bare literals on the
//! `for-of`, `for-await-of`, `yield*`, and ES5 paths, while its ES2015+ spread and
//! array-destructuring paths already preserved them. This suite pins the unified
//! behavior. See `crates/tsz-checker/src/checkers/iterable_checker.rs`
//! (`iterand_display_type`) and `crates/tsz-checker/src/dispatch/yield_.rs`.

use std::sync::{Arc, OnceLock};
use tsz_binder::lib_loader::LibFile;
use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};
use tsz_common::common::ScriptTarget;

fn libs() -> &'static [Arc<LibFile>] {
    static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    LIBS.get_or_init(load_default_lib_files)
}

fn check(source: &str, target: ScriptTarget) -> Vec<(u32, String)> {
    check_source_with_libs_code_messages(
        source,
        "test.ts",
        CheckerOptions {
            target,
            ..CheckerOptions::default()
        },
        libs(),
    )
}

/// Iterability not-iterable diagnostic codes across the whole family.
const ITERABILITY_CODES: &[u32] = &[2488, 2495, 2461, 2504];

/// Assert some iterability diagnostic was emitted whose rendered message contains
/// `fragment`. Fails with the full diagnostic list when it is missing, so a
/// widened-vs-literal regression is easy to read.
fn assert_iterability_message(diags: &[(u32, String)], fragment: &str) {
    let hit = diags
        .iter()
        .any(|(code, msg)| ITERABILITY_CODES.contains(code) && msg.contains(fragment));
    assert!(
        hit,
        "expected an iterability diagnostic whose message contains {fragment:?}; got: {diags:?}",
    );
}

/// Assert no iterability diagnostic mentions `fragment` (used to prove a widened
/// base type is *not* surfaced when the literal should be shown).
fn assert_no_iterability_message(diags: &[(u32, String)], fragment: &str) {
    let hit = diags
        .iter()
        .any(|(code, msg)| ITERABILITY_CODES.contains(code) && msg.contains(fragment));
    assert!(
        !hit,
        "expected no iterability diagnostic to mention {fragment:?}; got: {diags:?}",
    );
}

// ---------------------------------------------------------------------------
// for-of (ES2015+): TS2488 preserves the literal operand type.
// ---------------------------------------------------------------------------

#[test]
fn for_of_numeric_literal_shows_literal_not_number() {
    let diags = check("for (const each of 42) {}", ScriptTarget::ESNext);
    assert_iterability_message(&diags, "Type '42'");
    assert_no_iterability_message(&diags, "Type 'number'");
}

#[test]
fn for_of_boolean_literal_shows_literal_not_boolean() {
    let diags = check("for (const flag of true) {}", ScriptTarget::ESNext);
    assert_iterability_message(&diags, "Type 'true'");
    assert_no_iterability_message(&diags, "Type 'boolean'");
}

#[test]
fn for_of_bigint_literal_shows_literal_not_bigint() {
    let diags = check("for (const big of 123n) {}", ScriptTarget::ESNext);
    assert_iterability_message(&diags, "Type '123n'");
    assert_no_iterability_message(&diags, "Type 'bigint'");
}

#[test]
fn for_of_negative_numeric_literal_shows_literal() {
    let diags = check("for (const item of -5) {}", ScriptTarget::ESNext);
    assert_iterability_message(&diags, "Type '-5'");
    assert_no_iterability_message(&diags, "Type 'number'");
}

#[test]
fn for_of_parenthesized_literal_shows_literal() {
    let diags = check("for (const value of (7)) {}", ScriptTarget::ESNext);
    assert_iterability_message(&diags, "Type '7'");
    assert_no_iterability_message(&diags, "Type 'number'");
}

#[test]
fn for_of_const_asserted_literal_shows_literal() {
    let diags = check("for (const entry of 9 as const) {}", ScriptTarget::ESNext);
    assert_iterability_message(&diags, "Type '9'");
    assert_no_iterability_message(&diags, "Type 'number'");
}

// ---------------------------------------------------------------------------
// yield* : same rule as for-of.
// ---------------------------------------------------------------------------

#[test]
fn yield_star_numeric_literal_shows_literal() {
    let diags = check("function* producer() { yield* 42; }", ScriptTarget::ESNext);
    assert_iterability_message(&diags, "Type '42'");
    assert_no_iterability_message(&diags, "Type 'number'");
}

#[test]
fn yield_star_negative_literal_shows_literal() {
    let diags = check("function* stream() { yield* -5; }", ScriptTarget::ESNext);
    assert_iterability_message(&diags, "Type '-5'");
    assert_no_iterability_message(&diags, "Type 'number'");
}

// ---------------------------------------------------------------------------
// for-await-of : TS2504 preserves the literal operand type.
// ---------------------------------------------------------------------------

#[test]
fn for_await_of_numeric_literal_shows_literal() {
    let diags = check(
        "async function consume() { for await (const chunk of 42) {} }",
        ScriptTarget::ESNext,
    );
    assert_iterability_message(&diags, "Type '42'");
    assert_no_iterability_message(&diags, "Type 'number'");
}

// ---------------------------------------------------------------------------
// ES5 iteration sources: TS2495 (for-of) / TS2461 (spread) preserve the literal.
// ---------------------------------------------------------------------------

#[test]
fn es5_for_of_numeric_literal_shows_literal() {
    let diags = check("for (const token of 42) {}", ScriptTarget::ES5);
    assert_iterability_message(&diags, "Type '42'");
    assert_no_iterability_message(&diags, "Type 'number'");
}

#[test]
fn es5_spread_numeric_literal_shows_literal() {
    let diags = check("const collected = [...99];", ScriptTarget::ES5);
    assert_iterability_message(&diags, "Type '99'");
    assert_no_iterability_message(&diags, "Type 'number'");
}

// ---------------------------------------------------------------------------
// Controls: non-literal operands must still show their (widened / union) type,
// exactly as tsc and the pre-existing behavior do.
// ---------------------------------------------------------------------------

#[test]
fn for_of_widened_variable_still_shows_base_type() {
    // A `number`-typed binding is genuinely `number`; tsc shows `number`, not a
    // literal. The binder name is arbitrary — the rule is structural.
    let diags = check(
        "declare const tally: number;\nfor (const digit of tally) {}",
        ScriptTarget::ESNext,
    );
    assert_iterability_message(&diags, "Type 'number'");
}

#[test]
fn for_of_literal_union_operand_is_preserved_not_widened() {
    // A conditional operand keeps its literal union in tsc; tsz already agreed,
    // and the literal-display fix must not collapse it to `number`.
    let diags = check(
        "declare const pick: boolean;\nfor (const slot of (pick ? 1 : 2)) {}",
        ScriptTarget::ESNext,
    );
    assert_iterability_message(&diags, "Type '1 | 2'");
    assert_no_iterability_message(&diags, "Type 'number'");
}
