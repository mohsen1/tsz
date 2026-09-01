//! TS2515/TS2654 for ambient (`declare`) classes with unimplemented inherited
//! abstract members.
//!
//! Structural rule: when a non-abstract class declaration inherits an
//! unimplemented abstract member, `tsc` reports TS2515 (or TS2654 for two or
//! more) regardless of whether the declaration is ambient. The only escape
//! hatch is the `abstract` modifier on the derived declaration itself, which
//! `check_abstract_member_implementations` applies before either reporting path
//! runs.
//!
//! Before the fix, both reporting sites carried an extra
//! `has_declare_modifier(class_data.modifiers)` gate, so `declare class B
//! extends A {}` was silently clean. The boundary was the `declare` **modifier**,
//! not ambient-ness: a class inside `declare module` / `declare namespace`, and
//! any class in a `.d.ts`, carries no `declare` modifier of its own and was
//! always reported correctly. That asymmetry is what the negative controls below
//! pin down.
//!
//! Every expectation here was taken from the vendored `tsc` 7.0.2 oracle
//! (`--noEmit --strict --pretty false --target es2020`), not from tsz's own
//! output.

use crate::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect()
}

fn assert_reports_2515(source: &str, label: &str) {
    let got = codes(source);
    assert!(
        got.contains(&2515),
        "{label}: expected TS2515, got codes {got:?}"
    );
}

fn assert_no_missing_impl_diagnostic(source: &str, label: &str) {
    let got = codes(source);
    assert!(
        !got.iter().any(|c| matches!(c, 2515 | 2653 | 2654 | 2656)),
        "{label}: expected no missing-implementation diagnostic, got codes {got:?}"
    );
}

// ---------------------------------------------------------------------------
// Positive cases: `declare` must not exempt the class.
// ---------------------------------------------------------------------------

/// The witness from the bug report: an ambient class leaving an inherited
/// abstract *method* unimplemented.
#[test]
fn declare_class_missing_abstract_method_reports_ts2515() {
    assert_reports_2515(
        r#"
abstract class A { abstract m(): string; }
declare class B extends A {}
"#,
        "declare class, missing method",
    );
}

/// Same rule for an abstract *property*. tsc does not distinguish member kinds
/// here, so neither may tsz.
#[test]
fn declare_class_missing_abstract_property_reports_ts2515() {
    assert_reports_2515(
        r#"
abstract class A { abstract p: string; }
declare class B extends A {}
"#,
        "declare class, missing property",
    );
}

/// Same rule for an abstract *accessor*.
#[test]
fn declare_class_missing_abstract_accessor_reports_ts2515() {
    assert_reports_2515(
        r#"
abstract class A { abstract get g(): string; }
declare class B extends A {}
"#,
        "declare class, missing accessor",
    );
}

/// Binder-name control: the rule is structural, so renaming every binder must
/// not change the outcome.
#[test]
fn declare_class_missing_abstract_member_renamed_binders_reports_ts2515() {
    assert_reports_2515(
        r#"
abstract class Shape { abstract area(): number; }
declare class Square extends Shape {}
"#,
        "declare class, renamed binders",
    );
}

/// Two or more missing members promote TS2515 to TS2654, and the `declare`
/// modifier must not suppress that variant either.
#[test]
fn declare_class_missing_two_abstract_members_reports_ts2654() {
    let got = codes(
        r#"
abstract class A { abstract m(): string; abstract n(): number; }
declare class B extends A {}
"#,
    );
    assert!(
        got.contains(&2654),
        "declare class with two missing members: expected TS2654, got codes {got:?}"
    );
}

/// A generic abstract base reached through an ambient derived class. Keeps the
/// rule honest for the instantiated-heritage path, not just the bare one.
#[test]
fn declare_class_extending_generic_abstract_base_reports_ts2515() {
    assert_reports_2515(
        r#"
abstract class Container<T> { abstract get(): T; }
declare class NumberBox extends Container<number> {}
"#,
        "declare class, generic abstract base",
    );
}

// ---------------------------------------------------------------------------
// Negative controls: what must stay clean.
// ---------------------------------------------------------------------------

/// `declare abstract class` stays exempt — because it is abstract, not because
/// it is ambient. This is the control that fails if the fix is written as
/// "always report" instead of "let the `abstract` gate decide".
#[test]
fn declare_abstract_class_stays_clean() {
    assert_no_missing_impl_diagnostic(
        r#"
abstract class A { abstract m(): string; }
declare abstract class B extends A {}
"#,
        "declare abstract class",
    );
}

/// An ambient class that does declare the member is complete and must stay
/// clean — the members of a `declare class` still count as implementations.
#[test]
fn declare_class_implementing_abstract_member_stays_clean() {
    assert_no_missing_impl_diagnostic(
        r#"
abstract class A { abstract m(): string; }
declare class B extends A { m(): string; }
"#,
        "declare class implementing the member",
    );
}

/// Inheriting through an intermediate ambient class that implements the member
/// must stay clean: the requirement is satisfied before it reaches `B`.
#[test]
fn declare_class_inheriting_through_implementing_intermediate_stays_clean() {
    assert_no_missing_impl_diagnostic(
        r#"
abstract class A { abstract m(): string; }
declare class Mid extends A { m(): string; }
declare class B extends Mid {}
"#,
        "declare class via implementing intermediate",
    );
}

// ---------------------------------------------------------------------------
// Boundary controls: the forms that were already correct must not move.
// ---------------------------------------------------------------------------

/// The non-ambient form always worked; pinned so a future change to the gate
/// cannot silently trade one direction for the other.
#[test]
fn plain_class_missing_abstract_method_still_reports_ts2515() {
    assert_reports_2515(
        r#"
abstract class A { abstract m(): string; }
class B extends A {}
"#,
        "plain class, missing method",
    );
}

/// Implicitly-ambient via `declare module`: the class node carries no `declare`
/// modifier, so this was reported correctly before the fix. It is the control
/// proving the old boundary was the modifier rather than ambient-ness.
#[test]
fn implicitly_ambient_class_in_declare_module_reports_ts2515() {
    assert_reports_2515(
        r#"
declare module "m" {
  abstract class A { abstract m(): string; }
  class B extends A {}
}
"#,
        "class inside declare module",
    );
}

/// Same for `declare namespace`.
#[test]
fn implicitly_ambient_class_in_declare_namespace_reports_ts2515() {
    assert_reports_2515(
        r#"
declare namespace N {
  abstract class A { abstract m(): string; }
  class B extends A {}
}
"#,
        "class inside declare namespace",
    );
}

/// A `declare class` with no abstract members to inherit must stay clean —
/// removing the gate must not make ambient classes noisy in general.
#[test]
fn declare_class_extending_concrete_base_stays_clean() {
    assert_no_missing_impl_diagnostic(
        r#"
class A { m(): string { return ""; } }
declare class B extends A {}
"#,
        "declare class over concrete base",
    );
}
