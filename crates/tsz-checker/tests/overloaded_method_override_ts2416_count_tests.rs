//! TS2416 anchor count for overloaded derived method overrides.
//!
//! Structural rule: when an overloaded derived method (multiple declarations of
//! one name) is not assignable to the base member, tsc reports TS2416 at EVERY
//! declaration of the name — each bodiless overload signature and the
//! implementation alike. tsz previously reported it once per name (the
//! combined-shape path deduped every repeat). The single combined `CallableShape`
//! comparison is correct on both sides; only the anchor count/positions differ.
//!
//! Issue #17655.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;
use tsz_common::diagnostics::Diagnostic;

fn ts2416(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags.iter().filter(|d| d.code == 2416).collect()
}

/// Distinct anchor positions, one per TS2416.
fn distinct_starts(diags: &[&Diagnostic]) -> usize {
    let mut starts: Vec<u32> = diags.iter().map(|d| d.start).collect();
    starts.sort_unstable();
    starts.dedup();
    starts.len()
}

/// Non-ambient, two bodiless overload signatures plus an implementation
/// (3 declarations). tsc emits THREE TS2416, each anchored at its own
/// declaration and each carrying the identical combined-shape elaboration.
#[test]
fn concrete_overload_impl_mismatch_emits_one_ts2416_per_declaration() {
    let source = r#"
class BaseF {
  probe(x: string): number { return 1; }
}
class ChildF extends BaseF {
  probe(x: string): string;
  probe(x: number): string;
  probe(x: any): string { return "s"; }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let errs = ts2416(&diags);
    assert_eq!(
        errs.len(),
        3,
        "Expected TS2416 at each of the 3 derived declarations, got: {diags:#?}"
    );
    assert_eq!(
        distinct_starts(&errs),
        3,
        "Each TS2416 must anchor at a distinct declaration, got: {diags:#?}"
    );
    // The elaboration must stay the combined overload shape, not a per-node
    // signature: every anchor carries the same primary message.
    let first = &errs[0].message_text;
    assert!(
        errs.iter().all(|d| &d.message_text == first),
        "All anchors must share the identical combined-shape message, got: {diags:#?}"
    );
}

/// Ambient form: two bodiless declarations, no implementation. tsc emits TWO
/// TS2416.
#[test]
fn ambient_overload_mismatch_emits_one_ts2416_per_declaration() {
    let source = r#"
declare class BaseA {
  probe(x: string): number;
}
declare class ChildA extends BaseA {
  probe(x: string): string;
  probe(x: number): string;
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let errs = ts2416(&diags);
    assert_eq!(
        errs.len(),
        2,
        "Expected TS2416 at each of the 2 ambient declarations, got: {diags:#?}"
    );
    assert_eq!(distinct_starts(&errs), 2, "Distinct anchors: {diags:#?}");
}

/// Abstract bodiless form: two abstract overload declarations that mismatch the
/// base. tsc emits TWO TS2416, one per declaration.
#[test]
fn abstract_bodiless_overload_mismatch_emits_one_ts2416_per_declaration() {
    let source = r#"
class BaseB {
  probe(x: string): number { return 1; }
}
abstract class ChildB extends BaseB {
  abstract probe(x: string): string;
  abstract probe(x: number): string;
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let errs = ts2416(&diags);
    assert_eq!(
        errs.len(),
        2,
        "Expected TS2416 at each of the 2 abstract declarations, got: {diags:#?}"
    );
    assert_eq!(distinct_starts(&errs), 2, "Distinct anchors: {diags:#?}");
}

/// Binder-name independence: renaming the class and method must not change the
/// per-declaration count. Guards against any name-scoped shortcut.
#[test]
fn per_declaration_count_is_binder_name_independent() {
    let source = r#"
class Widget {
  render(x: string): number { return 1; }
}
class Gadget extends Widget {
  render(x: string): string;
  render(x: number): string;
  render(x: any): string { return "s"; }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let errs = ts2416(&diags);
    assert_eq!(
        errs.len(),
        3,
        "Anchor count must not depend on binder names, got: {diags:#?}"
    );
}

/// Static overloaded methods use TS2417 anchored once at the class name, not
/// per declaration — the overloaded-method path must not start emitting a
/// per-declaration TS2416 for the static side.
#[test]
fn static_overload_mismatch_stays_single_ts2417() {
    let source = r#"
class BaseS {
  static probe(x: string): number;
  static probe(x: string): number { return 1; }
}
class ChildS extends BaseS {
  static probe(x: string): string;
  static probe(x: number): string;
  static probe(x: any): string { return "s"; }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let errs2416 = ts2416(&diags);
    let errs2417: Vec<_> = diags.iter().filter(|d| d.code == 2417).collect();
    assert!(
        errs2416.is_empty(),
        "Static side must not emit TS2416, got: {diags:#?}"
    );
    assert_eq!(
        errs2417.len(),
        1,
        "Static side stays a single TS2417 at the class name, got: {diags:#?}"
    );
}

/// A single derived declaration overriding an overloaded base still yields a
/// single TS2416 — the per-declaration rule scales with the number of *derived*
/// declarations, and here there is only one.
#[test]
fn single_derived_declaration_over_overloaded_base_emits_one_ts2416() {
    let source = r#"
class BaseD {
  probe(x: string): string;
  probe(x: number): number;
  probe(x: any): string | number { return x; }
}
class ChildD extends BaseD {
  probe(x: string): boolean { return true; }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let errs = ts2416(&diags);
    assert_eq!(
        errs.len(),
        1,
        "One derived declaration yields one TS2416, got: {diags:#?}"
    );
}

/// Matched overloads must remain clean — the per-declaration emission must not
/// fire when the combined shapes are compatible.
#[test]
fn matched_overloads_emit_no_ts2416() {
    let source = r#"
class BaseM {
  probe(x: string): string;
  probe(x: number): number;
  probe(x: any): string | number { return x; }
}
class ChildM extends BaseM {
  probe(x: string): string;
  probe(x: number): number;
  probe(x: any): string | number { return x; }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let errs = ts2416(&diags);
    assert!(
        errs.is_empty(),
        "Compatible overload sets must stay clean, got: {diags:#?}"
    );
}
