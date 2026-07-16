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

/// Negative: a genuine JS/checkJs expando read-before-assign on an empty object
/// literal (no spread declares the member) must STILL report TS2565.
#[test]
fn js_expando_read_before_assign_still_reports_ts2565() {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_source;
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let codes: Vec<u32> = check_source(
        "function make() {\n  const box = {};\n  const x = box.count;\n  box.count = 1;\n  return x;\n}\n",
        "test.js",
        options,
    )
    .into_iter()
    .map(|d| d.code)
    .collect();
    assert!(
        codes.contains(&2565),
        "an empty-object expando read-before-assign in checkJs must still report TS2565. Got: {codes:?}"
    );
}
