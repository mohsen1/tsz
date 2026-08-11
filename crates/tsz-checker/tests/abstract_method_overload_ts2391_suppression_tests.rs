//! Regression tests: TS2391 ("Function implementation is missing or not
//! immediately following the declaration.") must be suppressed when the LAST
//! bodyless signature in a method overload group carries the `abstract`
//! modifier — mirroring the constructor-overload rule already covered by
//! `constructor_implementation_missing_ts2390_tests.rs`. An abstract member
//! needs no implementation, so a group that *ends* abstract reports only
//! TS2512 ("Overload signatures must all be abstract or non-abstract."), not
//! TS2391 as well.
//!
//! tsz previously included abstract siblings in the same-name group scan but
//! never checked whether the group's last member was abstract before
//! reporting TS2391, so `m(x: number): void; abstract m(x: string): void;`
//! in an abstract class produced a spurious TS2391 alongside the correct
//! TS2512.
//!
//! Verified against the pinned `typescript@7.0.2` oracle
//! (`--noEmit --pretty false --strict`). Binder names are varied so no fix
//! can key on a specific identifier.

use tsz_checker::test_utils::check_source_code_messages as get_diagnostics;

const TS2391: u32 = 2391;
const TS2512: u32 = 2512;

fn count(source: &str, code: u32) -> usize {
    get_diagnostics(source)
        .iter()
        .filter(|d| d.0 == code)
        .count()
}

/// Non-abstract overload followed by an abstract last signature: tsc reports
/// only TS2512, TS2391 is suppressed.
#[test]
fn non_abstract_then_abstract_last_suppresses_ts2391() {
    let source =
        "abstract class Widget {\n  run(x: number): void;\n  abstract run(x: string): void;\n}\n";
    assert_eq!(
        count(source, TS2391),
        0,
        "abstract last overload suppresses TS2391"
    );
    assert_eq!(count(source, TS2512), 1);
}

/// Same shape with three signatures: two non-abstract, then abstract last —
/// still exactly one TS2512, zero TS2391.
#[test]
fn two_non_abstract_then_abstract_last_suppresses_ts2391() {
    let source = "abstract class Sprocket {\n  build(x: number): void;\n  build(x: string): void;\n  abstract build(x: boolean): void;\n}\n";
    assert_eq!(count(source, TS2391), 0);
    assert_eq!(count(source, TS2512), 1);
}

/// Abstract-first, non-abstract-last is the mirror shape: TS2391 still fires
/// (the group's LAST signature decides, not the first) alongside TS2512.
#[test]
fn abstract_first_non_abstract_last_still_reports_ts2391() {
    let source = "abstract class Gadget {\n  abstract render(x: number): void;\n  render(x: string): void;\n}\n";
    assert_eq!(
        count(source, TS2391),
        1,
        "non-abstract last overload still needs an implementation"
    );
    assert_eq!(count(source, TS2512), 1);
}

/// A group with no abstract member at all is unaffected: TS2391 still fires
/// normally when no implementation follows.
#[test]
fn no_abstract_member_still_reports_ts2391() {
    let source =
        "abstract class Plain {\n  compute(x: number): void;\n  compute(x: string): void;\n}\n";
    assert_eq!(count(source, TS2391), 1);
    assert_eq!(count(source, TS2512), 0);
}

/// A single standalone abstract method (no overload group) is unaffected —
/// no TS2391, no TS2512.
#[test]
fn single_abstract_method_reports_neither_code() {
    let source = "abstract class Solo {\n  abstract compute(x: number): void;\n}\n";
    assert_eq!(count(source, TS2391), 0);
    assert_eq!(count(source, TS2512), 0);
}

/// An abstract-last group whose overloads are eventually implemented in a
/// *subclass* still reports nothing in the declaring abstract class — the
/// suppression is purely about the last signature's own modifier, not about
/// whether any implementation exists anywhere in the program.
#[test]
fn abstract_last_group_with_renamed_binder_and_multiple_params_suppresses_ts2391() {
    let source = "abstract class Zeta {\n  handle(x: number, y: number): void;\n  handle(x: string, y: string): void;\n  abstract handle(x: boolean, y: boolean): void;\n}\n";
    assert_eq!(count(source, TS2391), 0);
    assert_eq!(count(source, TS2512), 1);
}
