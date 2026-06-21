//! Regression: a `readonly` class property (incl. `static readonly`) in an
//! ambient context (`declare class` / `.d.ts`) behaves like an ambient `const`
//! — a string/numeric/negated-numeric literal initializer is accepted, a
//! non-literal initializer is `TS1254`, and a non-readonly property is `TS1039`.
//! tsz previously emitted `TS1039` unconditionally for any initialized ambient
//! class property.
//!
//! Owner: `crates/tsz-checker/src/state/state_checking_members/ambient_signature_checks.rs`
//! (mirrors the ambient *variable* path in `statement_checks.rs`).

use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn readonly_literal_initializers_accepted_in_ambient_class() {
    let codes = codes(
        r#"
declare class C {
  static readonly A = "x";
  readonly B = 1;
  readonly E = -1;
}
"#,
    );
    assert!(
        !codes.contains(&1039) && !codes.contains(&1254),
        "readonly literal initializers are valid in an ambient class; got {codes:?}"
    );
}

#[test]
fn non_readonly_initializer_still_ts1039() {
    // Negative control: a non-readonly property keeps TS1039.
    let codes = codes(
        r#"
declare class C {
  static prefix = 5;
}
"#,
    );
    assert!(
        codes.contains(&1039),
        "a non-readonly ambient property initializer is TS1039; got {codes:?}"
    );
}

#[test]
fn readonly_non_literal_initializer_is_ts1254() {
    // Negative control: readonly but a non-literal initializer is TS1254, not
    // accepted and not TS1039.
    let codes = codes(
        r#"
declare class C {
  readonly sum = 1 + 2;
}
"#,
    );
    assert!(
        codes.contains(&1254) && !codes.contains(&1039),
        "a readonly non-literal ambient initializer is TS1254; got {codes:?}"
    );
}

#[test]
fn readonly_literal_initializers_accepted_binder_variation() {
    // Binder-name variation to keep the check structural rather than name-driven.
    let codes = codes(
        r#"
declare class Widget {
  static readonly version = 2;
  readonly label = "w";
  protected readonly flag = -3;
}
"#,
    );
    assert!(
        !codes.contains(&1039) && !codes.contains(&1254),
        "readonly literal initializers are valid regardless of binder name; got {codes:?}"
    );
}
