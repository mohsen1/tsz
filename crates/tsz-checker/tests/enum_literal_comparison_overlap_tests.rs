//! Enum-vs-literal comparison overlap parity (TS2367 / TS2678).
//!
//! Owner layer: checker comparability/overlap (`enum_utils::types_have_no_overlap`
//! for `===`/`!==` TS2367, and `type_comparability::is_type_comparable_to` for
//! `switch`/`case` TS2678). Both reach the enum's member-value union through the
//! shared `enum_comparison_member_union` helper.
//!
//! Structural rule (matches `tsc` 6.0.2): an enum operand overlaps a **non-enum**
//! literal/primitive/union operand exactly when one of the enum's member *values*
//! does — a string enum overlaps `"red"` iff `"red"` is a member value, a numeric
//! enum overlaps `5` iff `5` is a member value. Enum-vs-enum comparisons stay
//! nominal: two different enums never overlap even with equal member values, and
//! two members of the same enum compare by their (distinct) values.
//!
//! Before the fix, a string enum reached the assignability fall-through where a
//! string literal is never assignable to the nominal enum, producing a false
//! TS2367/TS2678 for the *valid* member-value cases (the
//! `enum_utils.rs` comparability gap noted as out of scope in #14712).
//!
//! §25 anti-hardcoding: binder names (enum, member, and variable identifiers)
//! are varied across cases so the rule is name-independent. Both positive
//! (overlap → clean) and negative (no overlap → diagnostic) cases are covered.

use tsz_checker::test_utils::check_source_codes;

fn codes(src: &str) -> Vec<u32> {
    let mut c = check_source_codes(src);
    c.sort_unstable();
    c
}

// ---------------------------------------------------------------------------
// TS2367: `===` / `!==` / `==` / `!=`
// ---------------------------------------------------------------------------

#[test]
fn string_enum_equals_matching_member_value_is_clean() {
    // `Palette === "crimson"` overlaps because "crimson" is a member value.
    let src = r#"
enum Palette { Crimson = "crimson", Azure = "azure" }
declare const shade: Palette;
const ok = shade === "crimson";
"#;
    assert!(
        !codes(src).contains(&2367),
        "string enum vs matching member value must overlap (no TS2367): {:?}",
        codes(src)
    );
}

#[test]
fn string_enum_equals_non_member_value_reports_ts2367() {
    let src = r#"
enum Palette { Crimson = "crimson", Azure = "azure" }
declare const shade: Palette;
const bad = shade === "viridian";
"#;
    assert!(
        codes(src).contains(&2367),
        "string enum vs non-member literal must report TS2367: {:?}",
        codes(src)
    );
}

#[test]
fn string_enum_inequality_operators_follow_overlap() {
    // `==` matching → clean; `!=` non-member → TS2367 (overlap rule is operator
    // independent for the equality family).
    let matching = r#"
enum Direction { North = "N", South = "S" }
declare const heading: Direction;
const a = heading == "N";
const b = heading != "S";
"#;
    assert!(
        !codes(matching).contains(&2367),
        "matching member values under == / != must be clean: {:?}",
        codes(matching)
    );
    let non_member = r#"
enum Direction { North = "N", South = "S" }
declare const heading: Direction;
const a = heading != "E";
"#;
    assert!(
        codes(non_member).contains(&2367),
        "non-member literal under != must report TS2367: {:?}",
        codes(non_member)
    );
}

#[test]
fn string_enum_vs_literal_union_overlaps_when_any_member_matches() {
    let overlapping = r#"
enum Fruit { Apple = "apple", Pear = "pear" }
declare const pick: Fruit;
declare const choice: "apple" | "kiwi";
const ok = pick === choice;
"#;
    assert!(
        !codes(overlapping).contains(&2367),
        "enum vs union with a matching member must overlap: {:?}",
        codes(overlapping)
    );
    let disjoint = r#"
enum Fruit { Apple = "apple", Pear = "pear" }
declare const pick: Fruit;
declare const choice: "kiwi" | "mango";
const bad = pick === choice;
"#;
    assert!(
        codes(disjoint).contains(&2367),
        "enum vs fully-disjoint union must report TS2367: {:?}",
        codes(disjoint)
    );
}

#[test]
fn numeric_enum_equality_unchanged_by_member_union_path() {
    // Numeric enums already matched `tsc`; the member-union path must keep that.
    let src = r#"
enum Level { Low = 1, High = 2 }
declare const lvl: Level;
const ok = lvl === 1;
const bad = lvl === 3;
"#;
    let c = codes(src);
    assert_eq!(
        c.iter().filter(|&&x| x == 2367).count(),
        1,
        "numeric enum: only the non-member comparison reports TS2367: {c:?}"
    );
}

#[test]
fn mixed_enum_overlaps_each_member_value_kind() {
    let src = r#"
enum Token { Word = "w", Count = 1 }
declare const tok: Token;
const okStr = tok === "w";
const okNum = tok === 1;
const badStr = tok === "z";
const badNum = tok === 9;
"#;
    let c = codes(src);
    assert_eq!(
        c.iter().filter(|&&x| x == 2367).count(),
        2,
        "mixed enum: only the two non-member comparisons report TS2367: {c:?}"
    );
}

#[test]
fn enum_member_vs_own_value_is_clean_but_other_member_value_reports() {
    let own = r#"
enum Suit { Spade = "spade", Heart = "heart" }
const ok = Suit.Spade === "spade";
"#;
    assert!(
        !codes(own).contains(&2367),
        "enum member vs its own value must overlap: {:?}",
        codes(own)
    );
    let other = r#"
enum Suit { Spade = "spade", Heart = "heart" }
const bad = Suit.Spade === "heart";
"#;
    assert!(
        codes(other).contains(&2367),
        "enum member vs another member's value must report TS2367: {:?}",
        codes(other)
    );
}

#[test]
fn distinct_enums_with_equal_values_stay_nominal() {
    // Two different enums whose members share the value "x" do NOT overlap
    // (nominal). This guards against the member-union path leaking into the
    // enum-vs-enum case.
    let src = r#"
enum Left { Mark = "x", Other = "y" }
enum Right { Mark = "x", Other = "z" }
declare const lhs: Left;
declare const rhs: Right;
const bad = lhs === rhs;
"#;
    assert!(
        codes(src).contains(&2367),
        "distinct enums with equal member values must stay nominal (TS2367): {:?}",
        codes(src)
    );
}

#[test]
fn same_enum_distinct_members_report_ts2367() {
    let src = r#"
enum Phase { Start = "start", Stop = "stop" }
const bad = Phase.Start === Phase.Stop;
"#;
    assert!(
        codes(src).contains(&2367),
        "two distinct members of the same enum must report TS2367: {:?}",
        codes(src)
    );
}

#[test]
fn string_enum_vs_string_primitive_overlaps() {
    let src = r#"
enum Mode { Dev = "dev", Prod = "prod" }
declare const m: Mode;
declare const s: string;
const ok = m === s;
"#;
    assert!(
        !codes(src).contains(&2367),
        "string enum vs `string` must overlap: {:?}",
        codes(src)
    );
}

// ---------------------------------------------------------------------------
// TS2678: `switch` / `case`
// ---------------------------------------------------------------------------

#[test]
fn switch_case_matching_member_value_is_clean() {
    let src = r#"
enum Signal { Red = "red", Green = "green" }
declare const sig: Signal;
switch (sig) {
  case "red": break;
  case "green": break;
}
"#;
    assert!(
        !codes(src).contains(&2678),
        "switch cases with member values must be clean (no TS2678): {:?}",
        codes(src)
    );
}

#[test]
fn switch_case_non_member_value_reports_ts2678() {
    let src = r#"
enum Signal { Red = "red", Green = "green" }
declare const sig: Signal;
switch (sig) {
  case "red": break;
  case "amber": break;
}
"#;
    let c = codes(src);
    assert_eq!(
        c.iter().filter(|&&x| x == 2678).count(),
        1,
        "only the non-member case reports TS2678: {c:?}"
    );
}

#[test]
fn switch_mixed_enum_case_value_kinds() {
    let src = r#"
enum Kind { Name = "name", Id = 1 }
declare const k: Kind;
switch (k) {
  case "name": break;
  case 1: break;
  case "bogus": break;
}
"#;
    let c = codes(src);
    assert_eq!(
        c.iter().filter(|&&x| x == 2678).count(),
        1,
        "only the non-member case reports TS2678 for a mixed enum: {c:?}"
    );
}

// ---------------------------------------------------------------------------
// TS2367 operand display: enum-base widening + union subtype reduction.
//
// tsc renders each no-overlap operand through `getBaseTypeOfLiteralType`
// (enum member -> parent enum, literal -> base primitive) and then reduces the
// resulting union via `getUnionType`. A numeric enum is a subtype of `number`
// and a string enum of `string`, so when an enum-member operand sits in a
// union beside a literal of that base primitive the enum collapses into the
// primitive: `E.A | 1` displays as `number`, `S.X | "lit"` as `string`. A
// disjoint pairing (string enum beside a numeric literal) is preserved.
//
// Owner layer: solver union subtype-reduction (`is_subtype_shallow` now defers
// an enum to its structural member union). Binder names are varied so the rule
// is name-independent.
// ---------------------------------------------------------------------------

use tsz_checker::test_utils::check_source_strict_messages;

fn ts2367_message(src: &str) -> String {
    check_source_strict_messages(src)
        .into_iter()
        .find(|(c, _)| *c == 2367)
        .map(|(_, m)| m)
        .unwrap_or_else(|| panic!("expected a TS2367 diagnostic for: {src}"))
}

#[test]
fn numeric_enum_member_union_with_literal_displays_as_number() {
    // `Hue.Red | 1` widens to `Color | number` then reduces to `number`.
    let msg = ts2367_message(
        r#"
enum Hue { Red, Green }
declare const sample: Hue.Red | 1;
const cmp = sample === "needle";
"#,
    );
    assert!(
        msg.contains("'number' and 'string'"),
        "numeric enum member unioned with a number literal must display as 'number': {msg}"
    );
}

#[test]
fn string_enum_member_union_with_literal_displays_as_string() {
    // `Tag.Open | "extra"` widens to `Marker | string` then reduces to `string`.
    let msg = ts2367_message(
        r#"
enum Tag { Open = "open", Shut = "shut" }
declare const sample: Tag.Open | "extra";
const cmp = sample === 42;
"#,
    );
    assert!(
        msg.contains("'string' and 'number'"),
        "string enum member unioned with a string literal must display as 'string': {msg}"
    );
}

#[test]
fn numeric_enum_member_union_with_distinct_value_still_displays_as_number() {
    // The literal need not be a member value; the base primitive still absorbs.
    let msg = ts2367_message(
        r#"
enum Level { Low = 5, High = 6 }
declare const sample: Level.Low | 9;
const cmp = sample === "needle";
"#,
    );
    assert!(
        msg.contains("'number' and 'string'"),
        "numeric enum member unioned with any number literal displays as 'number': {msg}"
    );
}

#[test]
fn string_enum_union_with_numeric_literal_is_preserved() {
    // A string enum is disjoint from `number`, so the union is kept (no collapse).
    let msg = ts2367_message(
        r#"
enum Mode { On = "on", Off = "off" }
declare const sample: Mode.On | 1;
const cmp = sample === true;
"#,
    );
    assert!(
        msg.contains("'number | Mode' and 'boolean'"),
        "string enum disjoint from number must keep the union 'number | Mode': {msg}"
    );
}

#[test]
fn bare_enum_member_comparison_keeps_enum_name() {
    // A bare enum-member operand (no union) still widens to the parent enum and
    // is displayed by name, not collapsed to a primitive.
    let msg = ts2367_message(
        r#"
enum Alpha { A, B }
enum Beta { X = "x" }
const cmp = Alpha.A === Beta.X;
"#,
    );
    assert!(
        msg.contains("'Alpha' and 'Beta'"),
        "bare enum member comparison must show parent enum names: {msg}"
    );
}
