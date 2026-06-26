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
