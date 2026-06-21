//! Checker integration tests for TS2367 (`===`/`!==` no-overlap) when one
//! operand is a deferred conditional type.
//!
//! Structural rule: a deferred conditional operand (e.g. `Exclude<S, true>` /
//! `Extract<S, boolean>` over an instantiable check type) overlaps with the
//! other operand when its apparent type — the conditional's default constraint
//! (the union of its branch types) — is comparable to it. tsc relates through
//! that constraint; tsz used to treat the unresolved conditional as having no
//! overlap and wrongly emitted TS2367.
//!
//! Owner: `CheckerState::types_have_no_overlap_inner`
//! (`crates/tsz-checker/src/types/utilities/enum_utils.rs`) now routes a
//! deferred-conditional operand through `is_type_comparable_to` using
//! `conditional_default_constraint`. #14253 (type-plus).

use tsz_checker::test_utils::check_source_codes;

fn assert_no_errors(source: &str, label: &str) {
    let codes = check_source_codes(source);
    assert!(
        codes.is_empty(),
        "{label}: expected no diagnostics, got {codes:?}"
    );
}

fn assert_has_2367(source: &str, label: &str) {
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2367),
        "{label}: expected a TS2367, got {codes:?}"
    );
}

// =============================================================================
// Positive: a deferred conditional overlaps via its default constraint
// =============================================================================

#[test]
fn exclude_conditional_overlaps_excluded_literal() {
    // The reported repro (#14253).
    assert_no_errors(
        r#"
function noTrue<S>(subject: Exclude<S, true>): void {
  if (subject === true) throw new TypeError('subject is true');
}
export {};
"#,
        "Exclude<S, true> === true overlaps (no TS2367)",
    );
}

#[test]
fn extract_conditional_overlaps_literal() {
    assert_no_errors(
        r#"
function noFalse<S>(subject: Extract<S, boolean>): void {
  if (subject === false) throw new TypeError('subject is false');
}
export {};
"#,
        "Extract<S, boolean> === false overlaps (no TS2367)",
    );
}

#[test]
fn exclude_conditional_overlaps_union_member() {
    assert_no_errors(
        r#"
function f<S>(subject: Exclude<S, 1 | 2>): void {
  if (subject === 1) throw 0;
}
export {};
"#,
        "Exclude<S, 1 | 2> === 1 overlaps (no TS2367)",
    );
}

#[test]
fn deferred_conditional_overlap_is_binder_name_independent() {
    assert_no_errors(
        r#"
function pick<Elem>(value: Exclude<Elem, true>): void {
  if (value === true) throw 0;
}
export {};
"#,
        "renamed binder still overlaps",
    );
}

// =============================================================================
// Negative: genuinely-disjoint operands still report TS2367.
// =============================================================================

#[test]
fn disjoint_primitives_still_report_ts2367() {
    assert_has_2367(
        r#"
function bad(s: string): void {
  if (s === 5) throw 0;
}
export {};
"#,
        "string vs number literal still has no overlap (TS2367)",
    );
}
