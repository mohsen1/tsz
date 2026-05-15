//! Tests for TS2352 against `as X` type assertions where `X` is both a
//! type alias and a same-named const in scope (#6014).
//!
//! The two declarations share a single symbol with merged
//! `TYPE_ALIAS | BLOCK_SCOPED_VARIABLE` flags. Type-position uses of `X`
//! must resolve to the alias body, not the const's value type. Previously
//! the alias body wasn't registered to the symbol's DefId before the
//! `as X` overlap check ran, so the `Lazy(DefId)` returned by lowering
//! stayed unresolved and TS2352 fired as a false positive.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};

fn diags(source: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn as_assertion_resolves_to_type_alias_when_const_shares_name() {
    let source = r#"
type Point = { x: number; y: number };
const Point = { origin: { x: 0, y: 0 } as Point };
"#;
    let ds = diags(source);
    let ts2352: Vec<_> = ds.iter().filter(|d| d.0 == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "Expected no TS2352; the `as Point` should resolve to the type alias: {ts2352:?}",
    );
}

#[test]
fn baseline_no_name_collision_works() {
    // Sanity check: when names don't collide, no TS2352.
    let source = r#"
type PointType = { x: number; y: number };
const PointValue = { origin: { x: 0, y: 0 } as PointType };
"#;
    let ds = diags(source);
    let ts2352: Vec<_> = ds.iter().filter(|d| d.0 == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "Baseline (no collision) must not emit TS2352: {ts2352:?}",
    );
}

#[test]
fn merged_alias_with_alternate_name() {
    // Anti-hardcoding (.claude/CLAUDE.md §25): rule must not depend on the
    // identifier spelling.
    let source = r#"
type Shape = { side: number };
const Shape = { square: { side: 4 } as Shape };
"#;
    let ds = diags(source);
    let ts2352: Vec<_> = ds.iter().filter(|d| d.0 == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "Expected no TS2352 for alternate name: {ts2352:?}",
    );
}

#[test]
fn merged_alias_with_non_overlapping_assertion_still_flags() {
    // Regression guard: when the asserted shape does NOT overlap with the
    // type alias body, TS2352 must still fire.
    let source = r#"
type Point = { x: number; y: number };
const Point = { wrong: ("hello" as Point) };
"#;
    let ds = diags(source);
    let ts2352: Vec<_> = ds.iter().filter(|d| d.0 == 2352).collect();
    assert_eq!(
        ts2352.len(),
        1,
        "Expected one TS2352 for genuinely non-overlapping assertion: {ts2352:?}",
    );
}

/// #5990: the same merged-symbol pattern surfaced via a return-type
/// annotation rather than an `as` assertion. The arrow function returns
/// `{ type: "foo" }` against a `: Foo` return-type annotation where `Foo`
/// is both a type alias and a same-named const. The annotation must
/// resolve to the type alias body, not the const value type.
#[test]
fn return_type_annotation_resolves_to_type_alias_when_const_shares_name() {
    let source = r#"
type Foo = { type: "foo" };

const Foo = {
  make: (): Foo => {
    return { type: "foo" };
  }
};
"#;
    let ds = diags(source);
    let ts2322: Vec<_> = ds.iter().filter(|d| d.0 == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322; return-type `Foo` must bind to the alias body: {ts2322:?}",
    );
}
