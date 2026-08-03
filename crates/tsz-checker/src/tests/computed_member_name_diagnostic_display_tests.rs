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
// #16229 — a *no-substitution* template literal computed name (`` [`abc`] ``)
//
// The last silent form in this family. `get_property_name` does resolve it to
// the key `abc` (`get_literal_property_name` accepts the kind), which is why
// #16225 recorded it as "already handled one step earlier" — but naming a
// member and resolving its key are different questions.
// `member_name_for_diagnostic` dispatches a `ComputedPropertyName` straight to
// the display renderer by node *kind*, so the key resolver never covers for a
// missing display arm, and `computed_name_expression_display_text` had no arm
// for `NoSubstitutionTemplateLiteral`: not a `StringLiteral`, not a
// `NumericLiteral`, not a `TemplateExpression`, and declined by
// `simple_computed_name_expr_text_in_arena`. The renderer returned `None` and
// every site gating on it dropped the diagnostic.
//
// tsc renders it verbatim through `declarationNameToString` → `getTextOfNode`,
// so the **backticks survive into the message** — `` '[`abc`]' ``, not
// `'["abc"]'` and not `'[abc]'`. Recorded from `typescript@7.0.2` under
// `--noEmit --strict --lib es2022 --target es2022`.
// ---------------------------------------------------------------------------

#[test]
fn ts7033_declare_class_no_substitution_template_name_keeps_backticks() {
    assert_names(
        "declare class Cq17 { get [`pq17`](); }",
        7033,
        "Property '[`pq17`]'",
    );
}

#[test]
fn ts7033_interface_no_substitution_template_name_keeps_backticks() {
    assert_names(
        "interface Iq18 { get [`pq18`](); }",
        7033,
        "Property '[`pq18`]'",
    );
}

#[test]
fn ts7033_type_literal_no_substitution_template_name_keeps_backticks() {
    assert_names(
        "type Tq19 = { get [`pq19`](); };",
        7033,
        "Property '[`pq19`]'",
    );
}

#[test]
fn ts7033_static_no_substitution_template_name_keeps_backticks() {
    assert_names(
        "declare class Cq20 { static get [`pq20`](); }",
        7033,
        "Property '[`pq20`]'",
    );
}

#[test]
fn ts7033_abstract_no_substitution_template_name_keeps_backticks() {
    assert_names(
        "abstract class Cq21 { abstract get [`pq21`](); }",
        7033,
        "Property '[`pq21`]'",
    );
}

#[test]
fn ts7032_setter_no_substitution_template_name_keeps_backticks() {
    assert_names(
        "declare class Cq22 { set [`pq22`](v); }",
        7032,
        "Property '[`pq22`]'",
    );
}

#[test]
fn ts7008_property_no_substitution_template_name_keeps_backticks() {
    assert_names(
        "declare class Cq23 { [`pq23`]; }",
        7008,
        "Member '[`pq23`]'",
    );
}

#[test]
fn ts7010_bodyless_method_no_substitution_template_name_is_not_downgraded_to_ts7011() {
    // Same code-level divergence as the computed-identifier row above: with no
    // name, the member is "unnamable" and falls through to TS7011.
    let source = "declare class Cq24 { [`pq24`](); }";
    let codes: Vec<u32> = check_source_strict_messages(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect();
    assert!(
        !codes.contains(&7011),
        "a nameable computed member must not fall through to TS7011; got {codes:?}"
    );
    assert_names(
        source,
        7010,
        "'[`pq24`]', which lacks return-type annotation",
    );
}

#[test]
fn no_substitution_template_name_is_not_rendered_as_a_string_literal() {
    // The renderer must not reach for the *key* (`pq25`) or re-spell it with
    // the string-literal quoting the key resolver would produce. This is the
    // row that separates "the diagnostic fires" from "the message is right":
    // a fix that routed through `get_property_name` would pass every
    // `has(code)` assertion above and fail here.
    let message = message_for("declare class Cq25 { get [`pq25`](); }", 7033)
        .expect("expected TS7033 for a no-substitution template getter");
    assert!(
        message.contains("Property '[`pq25`]'"),
        "the backticked source spelling must survive; got {message:?}"
    );
    assert!(
        !message.contains("[\"pq25\"]") && !message.contains("Property 'pq25'"),
        "must not be re-spelled as a string literal or a bare key; got {message:?}"
    );
}

#[test]
fn no_substitution_template_name_pairs_get_and_set_and_blames_the_setter() {
    // The inverse control of the substituted-template row in
    // `noimplicitany_ambient_member_surface_tests.rs`. A no-substitution
    // template *is* a constant key, so — unlike `` [`a${x}`] `` — the accessors
    // pair, and tsc blames only the setter (TS7032), leaving the getter clean.
    // Verified against the `typescript@7.0.2` oracle: TS7032 alone, with no
    // TS7033 and no TS7006 (the paired getter contextually types the
    // parameter).
    let codes: Vec<u32> =
        check_source_strict_messages("declare class Cq26 { get [`pq26`](); set [`pq26`](v); }")
            .into_iter()
            .map(|(code, _)| code)
            .collect();
    assert!(
        codes.contains(&7032),
        "the setter is the blame site: {codes:?}"
    );
    assert!(
        !codes.contains(&7033),
        "a paired setter takes the getter out of TS7033: {codes:?}"
    );
    assert!(
        !codes.contains(&7006),
        "a paired getter contextually types the setter parameter: {codes:?}"
    );
}

#[test]
fn no_substitution_template_name_pairs_with_its_string_literal_spelling() {
    // Two spellings of one key. The display fix must not disturb the *key*
    // resolution that makes them the same member: `` get [`pq27`] `` and
    // `set ["pq27"]` pair, so the setter is still the single blame site.
    let codes: Vec<u32> =
        check_source_strict_messages("declare class Cq27 { get [`pq27`](); set [\"pq27\"](v); }")
            .into_iter()
            .map(|(code, _)| code)
            .collect();
    assert!(
        codes.contains(&7032) && !codes.contains(&7033),
        "the two spellings name one member; the setter is the blame site: {codes:?}"
    );
}

#[test]
fn empty_no_substitution_template_name_is_still_named() {
    // An empty template is a well-formed literal, not a parse-error shape —
    // the malformed-name guard below must not catch it. tsc names it
    // ``'[``]'`` and reports TS7033.
    assert_names(
        "declare class Cq28 { get [``](); }",
        7033,
        "Property '[``]'",
    );
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
