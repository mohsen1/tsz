//! Regression coverage for #13855: a `const X = Symbol()` / `Symbol.for(...)`
//! that is name-merged with a same-named `type X = typeof X` alias lost its
//! `unique symbol` value identity, so a computed-key object literal `{ [X]: v }`
//! degraded to a wide `[k: symbol]: V` index signature instead of the
//! symbol-keyed member an interface `interface I { [X](): T }` declares.
//!
//! Structural rule: TypeScript gives an unannotated `const` initialized with a
//! global `Symbol(...)`/`Symbol.for(...)` factory call the value identity
//! `typeof X` (a `unique symbol`), independent of any same-named type alias and
//! independent of which file reads it. tsz must agree on that identity across
//! every value-typing path (direct variable typing, the merged type-alias/value
//! path, and cross-arena value-declaration delegation), so `[X]` keys minted
//! anywhere intern to the same member.
//!
//! The fix centralizes the initializer-based unique-symbol upgrade
//! (`const_symbol_factory_unique_value_type`) the same way the annotation form
//! (`const X: unique symbol`) was already centralized. Binder names are varied
//! so nothing depends on the identifier text.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_source_with_libs, diagnostic_count, load_lib_files, strict_checker_options,
};
use tsz_common::diagnostics::Diagnostic;

fn symbol_libs() -> Vec<std::sync::Arc<tsz_binder::lib_loader::LibFile>> {
    load_lib_files(&[
        "es5.d.ts",
        "es2015.core.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
        "es2015.iterable.d.ts",
    ])
}

fn check_single(src: &str) -> Vec<Diagnostic> {
    check_source_with_libs(src, "test.ts", strict_checker_options(), &symbol_libs())
}

fn assert_no_symbol_member_fp(label: &str, diags: &[Diagnostic]) {
    let bad: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2322 | 2345 | 2353 | 2536 | 2741 | 2464))
        .map(|d| (d.code, d.message_text.clone()))
        .collect();
    assert!(
        bad.is_empty(),
        "{label}: expected no symbol-member false positives, got: {bad:?}"
    );
}

// ── primary repro: `const X = Symbol.for(...)` + `type X = typeof X` ──────────

#[test]
fn merged_symbol_for_const_keeps_unique_member_single_file() {
    let diags = check_single(
        r#"
const matcher = Symbol.for("@demo/matcher");
type matcher = typeof matcher;
interface Matcher { [matcher](): number; }
const lit = { [matcher]: () => 1 };
const m: Matcher = lit;
const made: Matcher = { [matcher]: () => 1 };
"#,
    );
    assert_no_symbol_member_fp("Symbol.for merged const", &diags);
}

#[test]
fn merged_symbol_const_keeps_unique_member_single_file() {
    // `Symbol()` (not `.for`) takes the same factory upgrade.
    let diags = check_single(
        r#"
const token = Symbol();
type token = typeof token;
interface Bag { [token](): string; }
const made: Bag = { [token]: () => "x" };
"#,
    );
    assert_no_symbol_member_fp("Symbol() merged const", &diags);
}

// ── renamed binders prove the fix is structural, not identifier-keyed ─────────

#[test]
fn merged_symbol_factory_is_name_invariant() {
    let diags = check_single(
        r#"
const tag = Symbol.for("k");
type tag = typeof tag;
interface Widget { [tag](): boolean; }
const w: Widget = { [tag]: () => true };
"#,
    );
    assert_no_symbol_member_fp("renamed binder", &diags);
}

// ── negative: a genuine value mismatch through the unique member still errors ──

#[test]
fn merged_symbol_factory_value_mismatch_still_reports_ts2322() {
    let diags = check_single(
        r#"
const matcher = Symbol.for("m");
type matcher = typeof matcher;
interface Matcher { [matcher](): number; }
const bad: Matcher = { [matcher]: () => "not a number" };
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2322),
        1,
        "a real value mismatch on the symbol-keyed member must still surface TS2322: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ── negative: a genuinely wide `symbol` key still yields a `[k: symbol]` index
//    signature (issue #9755), i.e. the upgrade is gated on the const + factory
//    shape, not applied to every symbol-typed key. ───────────────────────────

#[test]
fn wide_symbol_let_key_stays_index_signature() {
    let diags = check_single(
        r#"
let key: symbol = Symbol();
const lit = { [key]: () => 1 };
interface IndexSig { [k: symbol]: () => number }
const a: IndexSig = lit;
"#,
    );
    assert_no_symbol_member_fp("wide symbol index signature", &diags);
}

// ── control: the annotation form was already correct and must stay correct ────

#[test]
fn merged_declared_unique_symbol_stays_clean() {
    let diags = check_single(
        r#"
declare const marker: unique symbol;
type marker = typeof marker;
interface Marked { [marker](): number; }
const m: Marked = { [marker]: () => 1 };
"#,
    );
    assert_no_symbol_member_fp("declared unique symbol merged", &diags);
}

// NOTE: the cross-file witness from #13855 (a provider that declares the
// name-merged `Symbol.for` const, imported and keyed in a consumer) requires the
// driver's global symbol-file index to resolve the imported value identity. The
// `check_multi_file_with_libs` harness here does not install that index, so it
// cannot model the cross-arena value-declaration delegation path. That path is
// covered by the driver-level test
// `merged_symbol_factory_const_keeps_unique_member_across_files` in
// `crates/tsz-cli` (real multi-file compilation), and the fix site is
// `type_of_value_declaration_with_mode`.

#[test]
fn merged_symbol_factory_default_options_no_panic() {
    // Guard the non-strict path resolves the same identity (no crash, no FP).
    let diags = check_source_with_libs(
        r#"
const matcher = Symbol.for("d");
type matcher = typeof matcher;
interface Matcher { [matcher](): number; }
const m: Matcher = { [matcher]: () => 1 };
"#,
        "test.ts",
        CheckerOptions::default(),
        &symbol_libs(),
    );
    assert_no_symbol_member_fp("default options", &diags);
}
