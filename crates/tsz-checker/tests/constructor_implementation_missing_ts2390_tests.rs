use tsz_checker::test_utils::check_source_diagnostics;

fn ts2390_count(source: &str) -> usize {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|diag| diag.code == 2390)
        .count()
}

#[test]
fn single_bodyless_signature_reports_once() {
    let count = ts2390_count(
        r#"
class Widget {
  constructor(x: number);
}
"#,
    );
    assert_eq!(
        count, 1,
        "single bodyless constructor signature should report TS2390 once"
    );
}

#[test]
fn two_overloads_report_once_not_twice() {
    let count = ts2390_count(
        r#"
class Gadget {
  constructor(x: number);
  constructor(x: string);
}
"#,
    );
    assert_eq!(
        count, 1,
        "a contiguous run of 2 bodyless constructor signatures should report TS2390 exactly once, not once per signature"
    );
}

#[test]
fn three_overloads_report_once_not_thrice() {
    let count = ts2390_count(
        r#"
class Thingamajig {
  constructor(a: number);
  constructor(b: string);
  constructor(c: boolean);
}
"#,
    );
    assert_eq!(
        count, 1,
        "a contiguous run of 3 bodyless constructor signatures should report TS2390 exactly once"
    );
}

#[test]
fn overloads_with_implementation_report_none() {
    let count = ts2390_count(
        r#"
class Sprocket {
  constructor(x: number);
  constructor(x: string);
  constructor(x: number | string) {}
}
"#,
    );
    assert_eq!(
        count, 0,
        "an implementation following the overload run suppresses TS2390"
    );
}

#[test]
fn single_constructor_with_body_reports_none() {
    let count = ts2390_count(
        r#"
class Doohickey {
  constructor(x: number) {}
}
"#,
    );
    assert_eq!(
        count, 0,
        "a constructor with a body is not an overload signature"
    );
}

#[test]
fn abstract_last_signature_suppresses_ts2390() {
    let count = ts2390_count(
        r#"
class Contraption {
  constructor(x: number);
  abstract constructor(x: string);
}
"#,
    );
    assert_eq!(
        count, 0,
        "TS2390 is suppressed when the LAST signature in the run carries `abstract`"
    );
}

#[test]
fn abstract_first_signature_does_not_suppress_ts2390() {
    let count = ts2390_count(
        r#"
class Apparatus {
  abstract constructor(x: number);
  constructor(x: string);
}
"#,
    );
    assert_eq!(
        count, 1,
        "an `abstract` FIRST signature does not suppress TS2390 — only the LAST signature's modifier matters"
    );
}

#[test]
fn two_abstract_signatures_suppress_ts2390() {
    let count = ts2390_count(
        r#"
class Mechanism {
  abstract constructor(x: number);
  abstract constructor(x: string);
}
"#,
    );
    assert_eq!(
        count, 0,
        "when every signature (including the last) is abstract, TS2390 is suppressed"
    );
}

#[test]
fn single_abstract_constructor_reports_no_ts2390() {
    let count = ts2390_count(
        r#"
class Gizmo {
  abstract constructor();
}
"#,
    );
    assert_eq!(
        count, 0,
        "a lone abstract constructor signature needs no implementation"
    );
}

#[test]
fn abstract_class_with_concrete_constructor_overloads_still_reports() {
    let count = ts2390_count(
        r#"
abstract class Machine {
  constructor(x: number);
  constructor(x: string);
}
"#,
    );
    assert_eq!(
        count, 1,
        "the enclosing class being `abstract` does not make its constructor signatures abstract"
    );
}

#[test]
fn parameter_property_signatures_report_once() {
    let count = ts2390_count(
        r#"
class Instrument {
  constructor(public a: number);
  constructor(private b: string);
}
"#,
    );
    assert_eq!(
        count, 1,
        "parameter-property constructor signatures follow the same dedup rule"
    );
}

#[test]
fn non_constructor_member_between_two_runs_reports_once_per_run() {
    let count = ts2390_count(
        r#"
class Appliance {
  constructor(x: number);
  constructor(x: string);
  method(): void {}
  constructor(y: boolean);
  constructor(y: object);
}
"#,
    );
    assert_eq!(
        count, 2,
        "an intervening non-constructor member starts a new contiguous run, each reporting its own TS2390"
    );
}

#[test]
fn constructor_signature_then_unrelated_method_reports_once() {
    let count = ts2390_count(
        r#"
class Fixture {
  constructor(x: number);
  helper(): void {}
}
"#,
    );
    assert_eq!(
        count, 1,
        "a single bodyless constructor signature followed by an unrelated method still reports once"
    );
}
