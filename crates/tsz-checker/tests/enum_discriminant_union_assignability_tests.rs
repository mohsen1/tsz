//! An enum-typed discriminant property must narrow per enum member against a
//! union of member-typed arms (#17643 finding 3).
//!
//! tsc models an enum type as the union of its member types, so
//! `{ m: Mode; v: string }` is assignable to
//! `{ m: Mode.A; v: string } | { m: Mode.B; v: string }`: the
//! discriminated-union rule narrows `m` per member and each narrowed source
//! matches an arm. tsz stored the source property as a semantic `Lazy`
//! reference that the discriminant-value extraction never resolved, so the
//! property surfaced as one opaque value that matched no arm and the union
//! wrongly rejected (TS2322). The same model makes a single-member enum type
//! identical to its member type (`One` relates to `One.Only`).
//!
//! Every expectation in this file is pinned against `tsc` (7.0.2 / 6.0.2
//! agree on all cases).

use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_code_message_refs};

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn assert_clean(source: &str, context: &str) {
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "{context}: expected no diagnostics, got {:#?}",
        diagnostic_code_message_refs(&diagnostics),
    );
}

#[test]
fn numeric_enum_source_accepted_against_member_typed_arms() {
    assert_clean(
        r#"
enum Mode { A, B }
declare const src: { m: Mode; v: string };
const x: { m: Mode.A; v: string } | { m: Mode.B; v: string } = src;
"#,
        "full member coverage narrows per member (tsc clean)",
    );
}

#[test]
fn renamed_binders_still_accepted() {
    assert_clean(
        r#"
enum Signal { Go, Halt }
declare const reading: { phase: Signal; label: string };
const routed: { phase: Signal.Go; label: string } | { phase: Signal.Halt; label: string } = reading;
"#,
        "renamed enum/property/variable binders (tsc clean)",
    );
}

#[test]
fn string_enum_source_accepted_against_member_typed_arms() {
    assert_clean(
        r#"
enum Chan { Email = "email", Sms = "sms" }
declare const msg: { kind: Chan; payload: number };
const routed: { kind: Chan.Email; payload: number } | { kind: Chan.Sms; payload: number } = msg;
"#,
        "string enum discriminant (tsc clean)",
    );
}

#[test]
fn const_enum_source_accepted_against_member_typed_arms() {
    assert_clean(
        r#"
const enum Level { Low, High }
declare const gauge: { lvl: Level; data: boolean };
const seen: { lvl: Level.Low; data: boolean } | { lvl: Level.High; data: boolean } = gauge;
"#,
        "const enum discriminant (tsc clean)",
    );
}

#[test]
fn heterogeneous_enum_source_accepted_against_member_typed_arms() {
    assert_clean(
        r#"
enum Het { N = 0, S = "s" }
declare const mixed: { t: Het; v: string };
const seen: { t: Het.N; v: string } | { t: Het.S; v: string } = mixed;
"#,
        "heterogeneous enum discriminant (tsc clean)",
    );
}

#[test]
fn enum_source_accepted_against_literal_typed_arms() {
    assert_clean(
        r#"
enum Mode { A, B }
declare const src: { m: Mode; v: string };
const x: { m: 0; v: string } | { m: 1; v: string } = src;
"#,
        "numeric-literal arms accept the enum's member values (tsc clean)",
    );
}

#[test]
fn enum_or_undefined_source_accepted_when_undefined_arm_present() {
    assert_clean(
        r#"
enum Mode { A, B }
declare const src: { m: Mode | undefined; v: string };
const x: { m: Mode.A; v: string } | { m: Mode.B; v: string } | { m: undefined; v: string } = src;
"#,
        "union constituents expand through the enum (tsc clean)",
    );
}

#[test]
fn two_discriminants_enum_and_literal_union_accepted() {
    assert_clean(
        r#"
enum Mode { A, B }
declare const src: { m: Mode; k: "x" | "y"; v: string };
const x:
  | { m: Mode.A; k: "x"; v: string }
  | { m: Mode.A; k: "y"; v: string }
  | { m: Mode.B; k: "x"; v: string }
  | { m: Mode.B; k: "y"; v: string } = src;
"#,
        "every discriminant combination lands in an arm (tsc clean)",
    );
}

#[test]
fn member_union_source_stays_accepted() {
    assert_clean(
        r#"
enum Mode { A, B }
declare const src: { m: Mode.A | Mode.B; v: string };
const x: { m: Mode.A; v: string } | { m: Mode.B; v: string } = src;
"#,
        "already-narrow member-union source (tsc clean)",
    );
}

#[test]
fn partial_member_coverage_still_rejected() {
    let source = r#"
enum Mode { A, B }
declare const src: { m: Mode; v: string };
const x: { m: Mode.A; v: string } | { m: Mode.A; v: number } = src;
"#;
    assert_eq!(
        codes(source),
        vec![2322],
        "Mode.B has no arm; tsc reports TS2322"
    );
}

#[test]
fn different_enum_source_still_rejected() {
    let source = r#"
enum Mode { A, B }
enum Other { A, B }
declare const src: { m: Other; v: string };
const x: { m: Mode.A; v: string } | { m: Mode.B; v: string } = src;
"#;
    assert_eq!(
        codes(source),
        vec![2322],
        "a different enum's members stay nominally incompatible; tsc reports TS2322"
    );
}

#[test]
fn single_member_enum_type_relates_to_its_member_type() {
    assert_clean(
        r#"
enum One { Only }
declare const o: One;
const q: One.Only = o;
"#,
        "a single-member enum type IS its member type (tsc clean)",
    );
}

#[test]
fn single_member_enum_object_accepted_against_member_typed_arms() {
    assert_clean(
        r#"
enum One { Only }
declare const src: { m: One; v: string };
const x: { m: One.Only; v: string } | { m: One.Only; w: number; v: string } = src;
"#,
        "single-member enum narrows to its member and matches the first arm (tsc clean)",
    );
}

#[test]
fn multi_member_enum_type_still_rejected_against_one_member() {
    let source = r#"
enum Mode { A, B }
declare const m: Mode;
const a: Mode.A = m;
"#;
    assert_eq!(
        codes(source),
        vec![2322],
        "a two-member enum type is not any single member; tsc reports TS2322"
    );
}
