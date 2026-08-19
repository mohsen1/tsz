//! `number` is assignable to a numeric enum member type at every structural
//! depth (#17643 residual a, documented on PR #17661).
//!
//! tsc `isSimpleTypeRelatedTo`: under the assignable relation, `number`
//! relates to any enum member whose value is numeric
//! (`t & NumberLiteral && t & EnumLiteral`) and, through the union rule, to
//! any enum type with at least one numeric member. tsz applied a checker-side
//! override only at the immediate assignment target, so the rule vanished in
//! property positions, nested objects, contravariant parameter positions, and
//! conditional-type `extends` clauses.
//!
//! Every expectation in this file is pinned against `tsc` 6.0.2 (`--strict`).

use tsz_checker::test_utils::{
    check_source_codes, check_source_diagnostics, diagnostic_code_message_refs,
};

fn assert_clean(source: &str, context: &str) {
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "{context}: expected no diagnostics, got {:#?}",
        diagnostic_code_message_refs(&diagnostics),
    );
}

#[test]
fn top_level_number_to_numeric_enum_member() {
    assert_clean(
        r#"
enum Mode { A, B }
declare const n: number;
const a: Mode.A = n;
"#,
        "number -> Mode.A at the assignment target (tsc clean)",
    );
}

#[test]
fn property_position_number_to_numeric_enum_member() {
    assert_clean(
        r#"
enum Mode { A, B }
declare const src: { m: number };
const x: { m: Mode.A } = src;
"#,
        "number -> Mode.A one property deep (tsc clean)",
    );
}

#[test]
fn wide_number_property_against_member_typed_union_arms() {
    assert_clean(
        r#"
enum Mode { A, B }
declare const src: { m: number; v: string };
const x: { m: Mode.A; v: string } | { m: Mode.B; v: string } = src;
"#,
        "the #17643 witness: number property vs member-typed arms (tsc clean)",
    );
}

#[test]
fn renamed_binders_still_accepted() {
    assert_clean(
        r#"
enum Signal { Go, Halt }
declare const reading: { phase: number; label: string };
const routed: { phase: Signal.Go; label: string } = reading;
"#,
        "renamed enum/property/variable binders (tsc clean)",
    );
}

#[test]
fn nested_property_depth_two_accepted() {
    assert_clean(
        r#"
enum Mode { A, B }
declare const d: { a: { m: number } };
const x: { a: { m: Mode.A } } = d;
"#,
        "number -> Mode.A two properties deep (tsc clean)",
    );
}

#[test]
fn contravariant_parameter_position_accepted() {
    assert_clean(
        r#"
enum Mode { A, B }
declare const g: (x: { m: Mode.A }) => void;
const h: (x: { m: number }) => void = g;
"#,
        "number -> Mode.A inside a contravariant parameter (tsc clean)",
    );
}

#[test]
fn union_of_member_types_target_accepted() {
    assert_clean(
        r#"
enum Mode { A, B }
declare const src: { m: number };
const x: { m: Mode.A | Mode.B } = src;
"#,
        "number -> a plain union of member types (tsc clean)",
    );
}

#[test]
fn other_numeric_enum_member_accepted() {
    assert_clean(
        r#"
enum Mode { A, B }
enum Other { X = 10 }
declare const src: { m: number };
const x: { m: Other.X } = src;
"#,
        "the rule admits ANY numeric enum's member, not one enum (tsc clean)",
    );
}

#[test]
fn heterogeneous_enum_numeric_member_accepted() {
    assert_clean(
        r#"
enum Het { N = 0, S = "s" }
declare const n: number;
declare const src: { m: number };
const a: Het.N = n;
const x: { m: Het.N } = src;
"#,
        "numeric member of a heterogeneous enum admits number (tsc clean)",
    );
}

#[test]
fn heterogeneous_enum_string_member_rejected() {
    let source = r#"
enum Het { N = 0, S = "s" }
declare const src: { m: string };
const x: { m: Het.S } = src;
"#;
    assert_eq!(
        check_source_codes(source),
        vec![2322],
        "string member of a heterogeneous enum stays nominal; tsc reports TS2322"
    );
}

#[test]
fn whole_heterogeneous_enum_target_accepts_number() {
    assert_clean(
        r#"
enum Het { N = 0, S = "s" }
declare const n: number;
declare const src: { m: number };
const a: Het = n;
const x: { m: Het } = src;
"#,
        "number -> Het admits via the numeric member Het.N (tsc clean)",
    );
}

#[test]
fn computed_member_enum_accepts_number() {
    assert_clean(
        r#"
enum Flags { A = 1 << 4 }
declare const n: number;
declare const src: { m: number };
const a: Flags = n;
const b: Flags.A = n;
const x: { m: Flags.A } = src;
"#,
        "const-expression member enums admit number (tsc clean)",
    );
}

#[test]
fn string_enum_member_property_rejected() {
    let source = r#"
enum S { X = "x" }
declare const src: { m: string };
const x: { m: S.X } = src;
"#;
    assert_eq!(
        check_source_codes(source),
        vec![2322],
        "string -> string enum member stays rejected; tsc reports TS2322"
    );
}

#[test]
fn whole_string_enum_target_rejects_number() {
    let source = r#"
enum S { X = "x" }
declare const n: number;
const a: S = n;
"#;
    assert_eq!(
        check_source_codes(source),
        vec![2322],
        "number -> an all-string enum; tsc reports TS2322"
    );
}

#[test]
fn matching_numeric_literal_property_accepted_mismatch_rejected() {
    assert_clean(
        r#"
enum Mode { A, B }
const ok: { m: Mode.A } = { m: 0 };
"#,
        "a matching literal value converts to the member (tsc clean)",
    );

    let source = r#"
enum Mode { A, B }
const bad: { m: Mode.A } = { m: 1 };
"#;
    assert_eq!(
        check_source_codes(source),
        vec![2322],
        "a mismatched literal value stays rejected; tsc reports TS2322"
    );
}

#[test]
fn string_source_to_numeric_member_rejected() {
    let source = r#"
enum Mode { A, B }
declare const src: { m: string };
const x: { m: Mode.A } = src;
"#;
    assert_eq!(
        check_source_codes(source),
        vec![2322],
        "string -> numeric enum member; tsc reports TS2322"
    );
}

#[test]
fn conditional_extends_sees_the_same_relation() {
    // tsc: `number extends Mode.A` takes the TRUE branch (the assignable
    // relation, including the enum admission, drives conditional types), and
    // `string extends SE.X` takes the false branch.
    assert_clean(
        r#"
enum Mode { A, B }
enum SE { X = "x" }
type C1 = number extends Mode.A ? "yes" : "no";
type C3 = string extends SE.X ? "yes" : "no";
const x1: "yes" = null as unknown as C1;
const x3: "no" = null as unknown as C3;
"#,
        "conditional extends uses the same enum admission (tsc-pinned branches)",
    );
}
