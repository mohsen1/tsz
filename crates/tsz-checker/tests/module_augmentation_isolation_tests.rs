//! Regression tests for issue #6164: module self-augmentation incorrectly
//! merged with local declarations.
//!
//! Structural rule: `declare module "X" { interface Foo { ... } }` declares a
//! symbol that is separate from any file-scope `interface Foo { ... }` in the
//! augmenting file. Augmentation declarations never leak into the type used by
//! the file's own local declarations.

use tsz_checker::context::CheckerOptions;

fn check_single(source: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_multi_file(
        &[("test.ts", source)],
        "test.ts",
        CheckerOptions::default(),
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

/// The primary repro from issue #6164:
/// `const m: Merged = { a: 1, b: "test" }` must not require `augmented`.
#[test]
fn local_interface_after_augmentation_no_ts2741() {
    let codes = check_single(
        r#"
interface Merged {
  a: number;
}
interface Merged {
  b: string;
}
// Should NOT require 'augmented' — local Merged has only a and b.
const m: Merged = { a: 1, b: "test" };

declare module "./test" {
  interface Merged {
    augmented: boolean;
  }
}
export {};
"#,
    );
    assert!(
        !codes.contains(&2741),
        "TS2741 must not fire: augmentation must not affect file-scope Merged. Codes: {codes:?}"
    );
    assert!(codes.is_empty(), "expected no errors; got: {codes:?}");
}

/// Variant: different interface name — proves the fix is structural, not name-specific.
/// Uses self-augmentation (augmenting "./test" from test.ts) to avoid TS2664.
#[test]
fn local_interface_after_augmentation_different_name_no_errors() {
    let codes = check_single(
        r#"
interface Config {
  host: string;
}
const c: Config = { host: "localhost" };

declare module "./test" {
  interface Config {
    port: number;
  }
}
export {};
"#,
    );
    assert!(
        !codes.contains(&2741),
        "TS2741 must not fire: augmentation must not affect file-scope Config. Codes: {codes:?}"
    );
    assert!(codes.is_empty(), "expected no errors; got: {codes:?}");
}

/// Augmentation before the local declaration — source order should not matter.
#[test]
fn augmentation_before_local_interface_no_ts2741() {
    let codes = check_single(
        r#"
declare module "./test" {
  interface Widget {
    extraProp: boolean;
  }
}
interface Widget {
  id: number;
}
const w: Widget = { id: 42 };
export {};
"#,
    );
    // tsc allows this — the augmentation is separate; local Widget only has id.
    assert!(
        !codes.contains(&2741),
        "TS2741 must not fire when augmentation precedes local interface. Codes: {codes:?}"
    );
}
