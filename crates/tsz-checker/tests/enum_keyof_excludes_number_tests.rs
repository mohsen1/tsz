//! `keyof typeof E` for an enum with numeric members must not include `number`.
//!
//! Structural rule: a numeric (or mixed) enum object type carries an implicit
//! reverse-mapping index signature `[index: number]: string` so that
//! `E[someNumber]` element access resolves to `string`. tsc excludes that
//! implicit numeric index from `keyof typeof E`, so the key space is the union
//! of the member-name string literals only (`"A" | "B" | ...`), never `number`.
//! A numeric key (`0`) is therefore not assignable to `keyof typeof E`.
//!
//! The enum object shape is marked `ObjectFlags::ENUM_NAMESPACE`, which the
//! solver's `keyof` evaluation uses to drop the implicit numeric index — the
//! same mechanism the merged enum/namespace path already relied on.
//!
//! Binder names (enum name, member names, key-alias variable) are varied across
//! the cases so the rule is structural, not keyed on any identifier.

use tsz_checker::test_utils::{check_source_code_messages, check_source_codes};

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_code_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, msg)| msg)
        .collect()
}

fn ts2322_count(source: &str) -> usize {
    check_source_codes(source)
        .into_iter()
        .filter(|&c| c == 2322)
        .count()
}

/// Assert that assigning a numeric key to `keyof typeof E` produces exactly one
/// TS2322 whose target key union does not mention `number`, and return that
/// message for any case-specific follow-up assertions.
fn assert_numeric_key_rejected_without_number(source: &str) -> String {
    let messages = ts2322_messages(source);
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS2322 for the numeric key, got {messages:?}"
    );
    assert!(
        !messages[0].contains("number"),
        "keyof of an enum must not include `number`: {}",
        messages[0]
    );
    messages.into_iter().next().unwrap()
}

// ---------------------------------------------------------------------------
// Numeric enum: `number` is not a member key.
// ---------------------------------------------------------------------------

#[test]
fn numeric_enum_keyof_rejects_numeric_key() {
    // Assigning a numeric literal to `keyof typeof E` must error: tsc excludes
    // the implicit reverse-mapping numeric index from the key space.
    let source = r#"
        enum Color { Red = 1, Green, Blue }
        const k: keyof typeof Color = 0;
    "#;
    let msg = assert_numeric_key_rejected_without_number(source);
    for member in ["Red", "Green", "Blue"] {
        assert!(
            msg.contains(member),
            "key union should list member `{member}`: {msg}"
        );
    }
}

#[test]
fn numeric_enum_keyof_accepts_member_name() {
    let source = r#"
        enum Color { Red = 1, Green, Blue }
        const k: keyof typeof Color = "Green";
    "#;
    assert_eq!(
        ts2322_count(source),
        0,
        "a real member name is a valid key of `keyof typeof E`"
    );
}

#[test]
fn numeric_enum_keyof_rejects_unknown_name() {
    let source = r#"
        enum Color { Red = 1, Green, Blue }
        const k: keyof typeof Color = "Purple";
    "#;
    assert_eq!(
        ts2322_count(source),
        1,
        "a non-member name is not a key of `keyof typeof E`"
    );
}

// ---------------------------------------------------------------------------
// Mixed enum (has at least one numeric member) — same exclusion applies.
// ---------------------------------------------------------------------------

#[test]
fn mixed_enum_keyof_rejects_numeric_key() {
    let source = r#"
        enum Mix { First = 1, Second = "second", Third = 3 }
        const k: keyof typeof Mix = 0;
    "#;
    assert_numeric_key_rejected_without_number(source);
}

// ---------------------------------------------------------------------------
// `const enum` with numeric members — same exclusion applies.
// ---------------------------------------------------------------------------

#[test]
fn const_numeric_enum_keyof_rejects_numeric_key() {
    let source = r#"
        const enum Flag { On = 1, Off }
        const k: keyof typeof Flag = 0;
    "#;
    assert_numeric_key_rejected_without_number(source);
}

// ---------------------------------------------------------------------------
// String enum: control — never had a numeric index, still correct.
// ---------------------------------------------------------------------------

#[test]
fn string_enum_keyof_rejects_unknown_name_and_excludes_number() {
    let source = r#"
        enum Status { Active = "active", Done = "done" }
        const k: keyof typeof Status = 0;
    "#;
    assert_numeric_key_rejected_without_number(source);
}

// ---------------------------------------------------------------------------
// Reverse-mapping element access regression: excluding the numeric index from
// `keyof` must NOT remove numeric element access. `E[number]` still yields
// `string`.
// ---------------------------------------------------------------------------

#[test]
fn numeric_enum_reverse_mapping_still_resolves_to_string() {
    // `E[1]` and `E[numberVar]` must remain valid and produce `string`.
    let source = r#"
        enum Color { Red = 1, Green, Blue }
        declare const i: number;
        const a: string = Color[1];
        const b: string = Color[i];
    "#;
    assert_eq!(
        ts2322_count(source),
        0,
        "reverse-mapping element access must still resolve to `string`"
    );
}

#[test]
fn numeric_enum_reverse_mapping_is_not_number() {
    // The reverse-mapping value is `string`, not `number`.
    let source = r#"
        enum Color { Red = 1, Green, Blue }
        const bad: number = Color[1];
    "#;
    assert_eq!(
        ts2322_count(source),
        1,
        "reverse-mapping result is `string` and is not assignable to `number`"
    );
}
