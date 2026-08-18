//! A wide application of a covariant generic base must be rejected against a
//! union of narrower same-base applications (#17643).
//!
//! `Crate<string | number>` is a strict supertype of both `Crate<string>` and
//! `Crate<number>`, so it is assignable to neither arm of
//! `Crate<string> | Crate<number>`; a union target requires the source to be
//! assignable to at least one member. tsz wrongly accepted the wide source:
//! `is_discriminant_for_union` credited a property whose type merely
//! *contained* a unit constituent (`held: Payload | undefined` instantiates to
//! the mixed union `string | number | undefined`), letting the
//! discriminated-union rule narrow the wide source per-constituent into
//! different arms. tsc (`isLiteralType`) only treats a property as
//! discriminant-capable when the WHOLE type is unit-like: `boolean`, a single
//! unit type, or a union of unit types.
//!
//! Every expectation in this file is pinned against `tsc` 7.0.2.

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

const CRATE_PRELUDE: &str = r#"
interface Crate<Payload> {
  readonly held: Payload | undefined;
  readonly sealed: true;
}
"#;

#[test]
fn wide_application_rejected_against_union_of_same_base_arms() {
    let source = format!(
        r#"{CRATE_PRELUDE}
declare const wideCrate: Crate<string | number>;
const assigned: Crate<string> | Crate<number> = wideCrate;
"#
    );
    assert_eq!(codes(&source), vec![2322], "tsc 7.0.2 reports TS2322 here");
}

#[test]
fn alias_wrapped_wide_application_rejected() {
    let source = format!(
        r#"{CRATE_PRELUDE}
type CrateAlias<P> = Crate<P>;
declare const wideAlias: CrateAlias<string | number>;
const assigned: CrateAlias<string> | CrateAlias<number> = wideAlias;
"#
    );
    assert_eq!(codes(&source), vec![2322], "tsc 7.0.2 reports TS2322 here");
}

#[test]
fn generic_body_wide_application_rejected() {
    let source = format!(
        r#"{CRATE_PRELUDE}
function reWrap<T>(x: Crate<T | string>): Crate<string> | Crate<T> {{
  return x;
}}
"#
    );
    assert_eq!(codes(&source), vec![2322], "tsc 7.0.2 reports TS2322 here");
}

#[test]
fn wide_application_rejected_against_nullish_arm_union() {
    let source = format!(
        r#"{CRATE_PRELUDE}
declare const wideCrate: Crate<string | number>;
const assigned: Crate<string> | null = wideCrate;
"#
    );
    assert_eq!(codes(&source), vec![2322], "tsc 7.0.2 reports TS2322 here");
}

#[test]
fn mixed_nonunit_union_property_is_not_a_discriminant() {
    // Concrete (non-generic) witness of the same defect: `tag` mixes unit and
    // non-unit constituents across the arms, so it must not act as a
    // discriminant, and the wide source must be rejected.
    let source = r#"
interface LeftTag { tag: string | undefined; n: number }
interface RightTag { tag: number | undefined; n: number }
declare const mixedTag: { tag: string | number | undefined; n: number };
const assigned: LeftTag | RightTag = mixedTag;
"#;
    assert_eq!(codes(source), vec![2322], "tsc 7.0.2 reports TS2322 here");
}

#[test]
fn one_arm_source_stays_accepted() {
    let source = format!(
        r#"{CRATE_PRELUDE}
declare const oneCrate: Crate<string>;
const assigned: Crate<string> | Crate<number> = oneCrate;
"#
    );
    assert_clean(&source, "source equal to one arm");
}

#[test]
fn one_arm_source_with_null_arm_stays_accepted() {
    let source = format!(
        r#"{CRATE_PRELUDE}
declare const oneCrate: Crate<string>;
const assigned: Crate<string> | null = oneCrate;
"#
    );
    assert_clean(&source, "source equal to the non-null arm");
}

#[test]
fn contravariant_parameter_occurrence_stays_accepted() {
    // With the parameter only in argument position the wide application IS
    // assignable to each arm (parameter contravariance), so the union accepts.
    let source = r#"
interface Sink<Input> {
  accept(value: Input): void;
}
declare const wideSink: Sink<string | number>;
const assigned: Sink<string> | Sink<number> = wideSink;
"#;
    assert_clean(source, "contravariant-occurrence wide source");
}

#[test]
fn literal_discriminated_union_narrowing_stays_accepted() {
    let source = r#"
interface AKind { kind: "a"; v: string }
interface BKind { kind: "b"; v: string }
declare const eitherKind: { kind: "a" | "b"; v: string };
const assigned: AKind | BKind = eitherKind;
"#;
    assert_clean(source, "classic literal discriminant");
}

#[test]
fn optional_discriminant_stays_accepted() {
    let source = r#"
interface AOpt { kind?: "a"; v: string }
interface BOpt { kind: "b"; v: string }
declare const optKind: { kind: "a" | undefined; v: string };
const assigned: AOpt | BOpt = optKind;
"#;
    assert_clean(source, "optional discriminant (`\"a\" | undefined` is all-unit)");
}

#[test]
fn boolean_union_property_remains_discriminant_capable() {
    // tsc models `boolean` as `true | false`, so `held: boolean | undefined`
    // is an all-unit union and the discriminated-union rule legitimately
    // accepts the per-constituent narrowing of `boolean | bigint | undefined`.
    let source = r#"
interface Held<Payload> {
  readonly held: Payload | undefined;
  readonly sealed: true;
}
declare const wideHeld: Held<boolean | bigint>;
const assigned: Held<boolean> | Held<bigint> = wideHeld;
"#;
    assert_clean(source, "boolean-union discriminant (tsc 7.0.2 accepts)");
}
