//! Regression tests for TS2390 ("Constructor implementation is missing.") on
//! constructor overload sets.
//!
//! tsc reports the missing-implementation diagnostic on the *last* declaration
//! of a constructor overload set, exactly once, and suppresses it entirely when
//! that last declaration carries the `abstract` modifier (an abstract member
//! needs no implementation — even though `abstract` on a constructor is itself a
//! grammar error, TS1242). tsz previously reported TS2390 once per bodyless
//! constructor signature (a duplicate on any 2+-overload set) and did not honour
//! `abstract`, producing a spurious TS2390 on `abstract constructor()`.
//!
//! Verified against `tsc@6.0.2 --noEmit --strict --target es2022 --module
//! esnext`. The class binders are varied so no fix can key on a specific name.

use tsz_checker::test_utils::check_source_code_messages as get_diagnostics;

const TS2390: u32 = 2390;

fn count(source: &str, code: u32) -> usize {
    get_diagnostics(source)
        .iter()
        .filter(|d| d.0 == code)
        .count()
}

fn has(source: &str, code: u32) -> bool {
    count(source, code) > 0
}

/// A lone bodyless constructor signature with no implementation reports exactly
/// one TS2390.
#[test]
fn single_bodyless_constructor_reports_one_ts2390() {
    let source = "class Widget { constructor(); }\n";
    assert_eq!(count(source, TS2390), 1);
}

/// Two constructor overload signatures with no implementation report TS2390
/// exactly once — on the last signature — not once per signature.
#[test]
fn two_constructor_overloads_report_one_ts2390() {
    let source = "class Gadget { constructor(x: number); constructor(x: string); }\n";
    assert_eq!(
        count(source, TS2390),
        1,
        "TS2390 belongs to the last overload only, reported once"
    );
}

/// Three overload signatures still collapse to a single TS2390.
#[test]
fn three_constructor_overloads_report_one_ts2390() {
    let source = "class Sprocket {\n  constructor(x: number);\n  constructor(x: string);\n  constructor(x: boolean);\n}\n";
    assert_eq!(count(source, TS2390), 1);
}

/// Overload signatures followed by an implementation report no TS2390.
#[test]
fn constructor_overloads_with_implementation_no_ts2390() {
    let source =
        "class Cog { constructor(x: number); constructor(x: string); constructor(x: any) {} }\n";
    assert!(!has(source, TS2390));
}

/// A single constructor with a body reports no TS2390.
#[test]
fn single_constructor_with_body_no_ts2390() {
    let source = "class Lever { constructor(x: number) {} }\n";
    assert!(!has(source, TS2390));
}

/// An `abstract` constructor signature (a grammar error, TS1242) needs no
/// implementation: tsc reports only TS1242, never TS2390. This was the original
/// false positive.
#[test]
fn abstract_constructor_suppresses_ts2390() {
    let source = "class Pulley { abstract constructor(); }\n";
    assert!(
        !has(source, TS2390),
        "an abstract constructor needs no implementation"
    );
}

/// Two abstract constructor signatures: still no TS2390 (the last one, on which
/// the diagnostic would anchor, is abstract).
#[test]
fn two_abstract_constructors_suppress_ts2390() {
    let source =
        "class Winch { abstract constructor(x: number); abstract constructor(x: string); }\n";
    assert!(!has(source, TS2390));
}

/// When the *last* overload is non-abstract the diagnostic is not suppressed —
/// an earlier abstract signature does not exempt a trailing concrete one.
#[test]
fn abstract_then_nonabstract_constructor_reports_one_ts2390() {
    let source = "class Ratchet { abstract constructor(); constructor(x: number); }\n";
    assert_eq!(count(source, TS2390), 1);
}

/// When the last overload is abstract, TS2390 is suppressed even if an earlier
/// signature was concrete.
#[test]
fn nonabstract_then_abstract_constructor_suppresses_ts2390() {
    let source = "class Flywheel { constructor(); abstract constructor(x: number); }\n";
    assert!(!has(source, TS2390));
}

/// An `abstract class` with two non-abstract constructor overload signatures
/// still reports a single TS2390: the class being abstract does not make its
/// constructors abstract.
#[test]
fn abstract_class_nonabstract_constructor_overloads_report_one_ts2390() {
    let source = "abstract class Turbine { constructor(x: number); constructor(x: string); }\n";
    assert_eq!(count(source, TS2390), 1);
}

/// Parameter properties on the overload signatures (their own TS2369) do not
/// change the single-TS2390 outcome.
#[test]
fn constructor_overloads_with_parameter_properties_report_one_ts2390() {
    let source = "class Motor { constructor(public a: number); constructor(public b: string); }\n";
    assert_eq!(count(source, TS2390), 1);
}

/// A non-constructor member between two constructor signatures breaks the
/// consecutive overload set into two independent sets, each of which reports its
/// own TS2390 — matching tsc (constructors of a class are one symbol, but a
/// bodyless-then-interrupted signature is diagnosed per contiguous run).
#[test]
fn non_consecutive_constructor_signatures_report_ts2390_per_run() {
    let source = "class Piston { constructor(x: number); static s = 1; constructor(x: string); }\n";
    assert_eq!(count(source, TS2390), 2);
}

/// A bodyless constructor followed by an unrelated method still reports its lone
/// TS2390 (the method does not count as the constructor's implementation).
#[test]
fn constructor_signature_then_method_reports_one_ts2390() {
    let source = "class Camshaft { constructor(x: number); rev() {} }\n";
    assert_eq!(count(source, TS2390), 1);
}
