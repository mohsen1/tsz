//! Regression tests for #16204 — a computed member name keeps its brackets in
//! the implicit-any diagnostics (TS7008 / TS7010 / TS7032 / TS7033).
//!
//! `tsc` names a member through `declarationNameToString`, which renders the
//! name node's *syntax*: `["a"]`, `[0x10]`, `[k]`, `[Symbol.iterator]`. tsz
//! used to pick the renderer by whichever helper succeeded first, so a computed
//! name whose expression is a literal went through the semantic key resolver
//! (`get_property_name`) and came out as a bare key — `foo`, `0`, `16` — while
//! the names that resolver *declines* (an identifier or `Symbol.x` expression)
//! kept their brackets. Two spellings of one message, decided by an accident of
//! call order.
//!
//! Every binder name below is distinct so nothing can key on an identifier
//! string, and the numeric rows deliberately use forms whose *key* differs from
//! their *source spelling* (`1.0` -> key `1`, `0x10` -> key `16`): a test that
//! only used `[0]` would pass against a renderer that still canonicalized.

use crate::test_utils::check_source_strict_messages;

fn message_for(source: &str, code: u32) -> Option<String> {
    check_source_strict_messages(source)
        .into_iter()
        .find(|(c, _)| *c == code)
        .map(|(_, message)| message)
}

fn assert_names(source: &str, code: u32, expected: &str) {
    let message = message_for(source, code)
        .unwrap_or_else(|| panic!("expected TS{code} for source:\n{source}\n"));
    assert!(
        message.contains(expected),
        "TS{code} should name the member {expected}; got {message:?}"
    );
}

// ---------------------------------------------------------------------------
// TS7033 — lone bodyless getter, every container a bodyless getter can appear in
// ---------------------------------------------------------------------------

#[test]
fn ts7033_declare_class_computed_string_name_keeps_brackets() {
    assert_names(
        "declare class Cq1 { get [\"pq1\"](); }",
        7033,
        "Property '[\"pq1\"]'",
    );
}

#[test]
fn ts7033_declare_class_computed_numeric_name_keeps_brackets_and_source_spelling() {
    // `1.0` canonicalizes to the key `1`. The message must show the source form.
    assert_names(
        "declare class Cq2 { get [1.0](); }",
        7033,
        "Property '[1.0]'",
    );
}

#[test]
fn ts7033_declare_class_computed_hex_name_is_not_canonicalized() {
    // `0x10` canonicalizes to the key `16`.
    assert_names(
        "declare class Cq3 { get [0x10](); }",
        7033,
        "Property '[0x10]'",
    );
}

#[test]
fn ts7033_interface_computed_string_name_keeps_brackets() {
    assert_names(
        "interface Iq4 { get [\"pq4\"](); }",
        7033,
        "Property '[\"pq4\"]'",
    );
}

#[test]
fn ts7033_type_literal_computed_string_name_keeps_brackets() {
    assert_names(
        "type Tq5 = { get [\"pq5\"](); };",
        7033,
        "Property '[\"pq5\"]'",
    );
}

#[test]
fn ts7033_static_computed_numeric_name_keeps_brackets() {
    assert_names(
        "declare class Cq6 { static get [1.0](); }",
        7033,
        "Property '[1.0]'",
    );
}

#[test]
fn ts7033_abstract_computed_numeric_name_keeps_brackets() {
    assert_names(
        "abstract class Cq7 { abstract get [0x10](); }",
        7033,
        "Property '[0x10]'",
    );
}

#[test]
fn ts7033_identifier_computed_name_still_keeps_brackets() {
    // The arm that was already correct: `get_property_name` declines an
    // identifier expression, so this row went through the display renderer
    // before the fix and must be unchanged by it.
    assert_names(
        "declare const kq8: unique symbol; declare class Cq8 { get [kq8](); }",
        7033,
        "Property '[kq8]'",
    );
}

// ---------------------------------------------------------------------------
// The sibling codes that share the renderer
// ---------------------------------------------------------------------------

#[test]
fn ts7032_setter_computed_numeric_name_keeps_brackets() {
    assert_names(
        "declare class Cq9 { set [1.0](v); }",
        7032,
        "Property '[1.0]'",
    );
}

#[test]
fn ts7032_interface_setter_computed_string_name_keeps_brackets() {
    assert_names(
        "interface Iq10 { set [\"pq10\"](v); }",
        7032,
        "Property '[\"pq10\"]'",
    );
}

#[test]
fn ts7008_property_computed_numeric_name_keeps_brackets() {
    assert_names("declare class Cq11 { [0x10]; }", 7008, "Member '[0x10]'");
}

#[test]
fn ts7010_bodyless_method_computed_string_name_keeps_brackets() {
    assert_names(
        "declare class Cq12 { [\"pq12\"](); }",
        7010,
        "'[\"pq12\"]', which lacks return-type annotation",
    );
}

#[test]
fn ts7010_bodyless_method_computed_identifier_name_is_not_downgraded_to_ts7011() {
    // An unnamable member fell through to TS7011 ("Function expression, which
    // lacks return-type annotation..."). A computed identifier name is
    // nameable, so tsc reports TS7010 with the bracketed name — this row is a
    // diagnostic *code* divergence, not only a message one.
    let source = "declare const kq13: unique symbol; declare class Cq13 { [kq13](); }";
    let codes: Vec<u32> = check_source_strict_messages(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect();
    assert!(
        !codes.contains(&7011),
        "a nameable computed member must not fall through to TS7011; got {codes:?}"
    );
    assert_names(source, 7010, "'[kq13]', which lacks return-type annotation");
}

// ---------------------------------------------------------------------------
// Controls — non-computed names are named by their key, unchanged
// ---------------------------------------------------------------------------

#[test]
fn ts7033_plain_identifier_name_has_no_brackets() {
    let message = message_for("declare class Cq14 { get pq14(); }", 7033)
        .expect("expected TS7033 for a plain identifier getter");
    assert!(
        message.contains("Property 'pq14'") && !message.contains('['),
        "a non-computed name must not gain brackets; got {message:?}"
    );
}

#[test]
fn ts7008_plain_identifier_name_has_no_brackets() {
    let message = message_for("declare class Cq15 { pq15; }", 7008)
        .expect("expected TS7008 for a plain identifier property");
    assert!(
        message.contains("Member 'pq15'") && !message.contains('['),
        "a non-computed name must not gain brackets; got {message:?}"
    );
}

#[test]
fn malformed_computed_name_reports_no_implicit_any_member_diagnostic() {
    // A computed name with no expression is a parse-error shape. tsc reports
    // only the syntax error, so the renderer must return `None` rather than an
    // empty `[]`, and the implicit-any diagnostics must stay off.
    let codes: Vec<u32> = check_source_strict_messages("declare class Cq16 { get [](); }")
        .into_iter()
        .map(|(code, _)| code)
        .collect();
    assert!(
        !codes.contains(&7033),
        "a malformed computed name must not draw TS7033; got {codes:?}"
    );
}
