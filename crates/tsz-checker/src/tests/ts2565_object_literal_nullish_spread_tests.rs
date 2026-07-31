//! Regression tests for TS2565 ('used before being assigned') false positives on
//! object literals built from a spread of a nullish union.
//!
//! `const m = { ...maybe }` where `maybe: T | undefined` (or `| null`) has type
//! `{ ...T }` — the nullish constituents spread to `{}` and contribute nothing,
//! while `T`'s members are contributed (as optional). A later `m.prop = ...` is a
//! re-assignment of an existing (optional) member, NOT a JS-style expando
//! declaration, so an earlier read of `m.prop` must not report TS2565.
//!
//! Root cause was in the object-literal-declares-property classifier
//! (`spread_source_type_declares_property`): a spread source of `T | undefined`
//! failed the whole-union `type_has_property` check (the `undefined` constituent
//! lacks the member), so the property was misclassified as expando-capable. The
//! fix strips nullish constituents (getSpreadType semantics) and accepts the
//! property when any surviving constituent declares it. tanstack-query row FP.

use crate::test_utils::check_source_strict;

fn codes(source: &str) -> Vec<u32> {
    check_source_strict(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn spread_of_undefined_union_does_not_report_used_before_assigned() {
    let codes = codes(
        r#"
interface Knobs { keyHash?: string; mode?: string }
declare function getDefaults(): Knobs | undefined
export function normalize(): void {
  const merged = { ...getDefaults(), _done: true }
  if (!merged.keyHash) { merged.keyHash = String(merged.mode) }
}
"#,
    );
    assert!(
        !codes.contains(&2565),
        "spread of `Knobs | undefined` declares keyHash; a later write is re-assignment, not expando. Got: {codes:?}"
    );
}

#[test]
fn spread_of_null_union_does_not_report_used_before_assigned() {
    let codes = codes(
        r#"
interface Knobs { keyHash?: string; mode?: string }
declare function getDefaults(): Knobs | null
export function normalize(): void {
  const merged = { ...getDefaults(), _done: true }
  if (!merged.keyHash) { merged.keyHash = 'x' }
}
"#,
    );
    assert!(
        !codes.contains(&2565),
        "spread of `Knobs | null` declares keyHash. Got: {codes:?}"
    );
}

#[test]
fn spread_of_undefined_and_null_union_does_not_report_used_before_assigned() {
    let codes = codes(
        r#"
interface Knobs { keyHash?: string; mode?: string }
declare function getDefaults(): Knobs | undefined | null
export function normalize(): void {
  const merged = { ...getDefaults(), _done: true }
  if (!merged.keyHash) { merged.keyHash = 'x' }
}
"#,
    );
    assert!(
        !codes.contains(&2565),
        "spread of `Knobs | undefined | null` declares keyHash. Got: {codes:?}"
    );
}

#[test]
fn multiple_all_nullish_spreads_do_not_report_used_before_assigned() {
    let codes = codes(
        r#"
interface Knobs { keyHash?: string; mode?: string }
declare function getDefaults(): Knobs | undefined
declare function getMore(): Knobs | undefined
export function normalize(): void {
  const merged = { ...getDefaults(), ...getMore(), _done: true }
  if (!merged.keyHash) { merged.keyHash = 'x' }
}
"#,
    );
    assert!(
        !codes.contains(&2565),
        "every spread being a nullish union still declares keyHash. Got: {codes:?}"
    );
}

/// A required member (not optional) on the spread source is equally declared —
/// the trigger was the nullish constituent, not optionality.
#[test]
fn spread_of_undefined_union_with_required_member_does_not_report_ts2565() {
    let codes = codes(
        r#"
interface Knobs { keyHash: string; mode?: string }
declare function getDefaults(): Knobs | undefined
export function normalize(): void {
  const merged = { ...getDefaults(), _done: true }
  if (!merged.keyHash) { merged.keyHash = 'x' }
}
"#,
    );
    assert!(
        !codes.contains(&2565),
        "required member on the spread source is still declared. Got: {codes:?}"
    );
}

/// Anti-hardcoding cover: the rule is structural (nullish-union spread source),
/// not identifier-name based — renamed binders behave identically.
#[test]
fn nullish_union_spread_suppression_is_binder_name_invariant() {
    let codes = codes(
        r#"
interface Widget { tag?: string; slot?: string }
declare function pull(): Widget | undefined
export function shape(): void {
  const acc = { ...pull(), sealed: true }
  if (!acc.tag) { acc.tag = String(acc.slot) }
}
"#,
    );
    assert!(
        !codes.contains(&2565),
        "suppression must be structural, independent of identifier names. Got: {codes:?}"
    );
}

fn check_js_strict(source: &str) -> Vec<u32> {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_source;
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// Negative control, corrected: a plain object literal's expando properties are
/// NOT order-sensitive in tsc (`getSpreadType`/expando semantics type the object
/// from every assignment in the program regardless of position), so a read
/// textually preceding an assignment reports nothing. Verified against a real
/// `tsc@7.0.2` oracle: `function make() { const box = {}; const x = box.count;
/// box.count = 1; return x; }` under `--allowJs --checkJs --strict` emits zero
/// diagnostics. The previous version of this test asserted TS2565 here, which
/// tsc never emits for this shape — `expando_root_has_ordered_declarations`
/// (`access_semantics.rs`) already documents this exact rule and correctly
/// returns `false` for a plain-object-literal-initialized variable; the test
/// was asserting behavior contrary to both tsc and the code's own contract.
#[test]
fn plain_object_expando_read_before_assign_does_not_report_ts2565() {
    let codes = check_js_strict(
        "function make() {\n  const box = {};\n  const x = box.count;\n  box.count = 1;\n  return x;\n}\n",
    );
    assert!(
        !codes.contains(&2565),
        "a plain object literal's expando properties are unordered in tsc; a read preceding the write must not report TS2565. Got: {codes:?}"
    );
}

/// Positive control for the *ordered* expando case this file's suppression must
/// not blunt: `function C() {}` is a constructor-shaped expando root, and tsc
/// treats late-attached properties on it as declarations — a read that
/// textually precedes the assignment genuinely is "used before assigned".
/// Verified against a real `tsc@7.0.2` oracle: `function C() {} const x = C.f;
/// C.f = 1;` under `--allowJs --checkJs --strict` emits exactly TS2565 on the
/// `C.f` read. This is the real negative control for the nullish-spread
/// suppression above: it exercises `expando_root_has_ordered_declarations`'s
/// `true` branch, which no other test in this crate covered before.
#[test]
fn function_expando_read_before_assign_still_reports_ts2565() {
    let codes = check_js_strict("function C() {}\nconst x = C.f;\nC.f = 1;\n");
    assert!(
        codes.contains(&2565),
        "a function-as-constructor expando is ordered in tsc; a read preceding the write must report TS2565. Got: {codes:?}"
    );
}
