//! `v`-mode class-set operator continuation — TS1005 `'&&'/'--' expected.`
//!
//! A `ClassIntersection` / `ClassSubtraction` (a `v`-mode class that has
//! committed to `&&` or `--`) admits, after each operand, only more of that
//! same operator or the closing `]`. Stray union content is a grammar error:
//! `tsc` reports `TS1005 '&&' expected.` (or `'--' expected.`) at each stray
//! operand and recovers one operand at a time until `]`, re-syncing to a
//! later valid operator when it meets one. Two shapes deviate from the plain
//! `TS1005` recovery:
//!
//! - a lone `&` in operator position is consumed as a malformed `&&` and
//!   draws `TS1508 Unexpected '&'.` instead (independent of which operator the
//!   class committed to, because `&` always starts the `&&` token);
//! - a bare `-` or the *other* operator is operator mixing and draws
//!   `TS1519` through the shared `note_class_set_kind` path.
//!
//! A stray operand is consumed the same way a real operand is (a nested
//! `[...]`, a `\`-escape, or a `\q{...}` disjunction is one operand; a bare
//! `ClassSetSyntaxCharacter` such as `(`/`|` is one code point) but its own
//! `TS1508`/`TS1522` report is suppressed so it does not stack on top of the
//! `TS1005`.
//!
//! Every row is pinned against `typescript@7.0.2`
//! (`--noEmit --strict --target es2024 --lib es2024`), the version
//! `scripts/conformance/typescript-versions.json` `current` resolves to.
//! Operand identifiers are varied across rows (`a b c`, `x y z`, class
//! escapes) so the behaviour is demonstrably structural, not keyed to any
//! particular character.
use crate::parser::test_fixture::parse_source_with_language_version;
use tsz_common::ScriptTarget;
use tsz_common::diagnostics::diagnostic_codes;

const TS1005: u32 = diagnostic_codes::EXPECTED;
const TS1508: u32 = diagnostic_codes::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH;
const TS1519: u32 =
    diagnostic_codes::OPERATORS_MUST_NOT_BE_MIXED_WITHIN_A_CHARACTER_CLASS_WRAP_IT_IN_A_NESTED_CLASS_I;

/// `(code, message, zero-based byte offset)` for each diagnostic, in emission
/// order. The message pins `'&&'` vs `'--'` (both `TS1005`) and the offset
/// pins the per-operand anchor.
fn diags(source: &str) -> Vec<(u32, String, u32)> {
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES2024);
    parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone(), d.start))
        .collect()
}

/// Codes only, for the "did/did-not report" rows.
fn codes(source: &str, target: ScriptTarget) -> Vec<u32> {
    let (parser, _root) = parse_source_with_language_version(source, target);
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

/// Byte offset of `needle` in `source` — the anchor a diagnostic points at.
fn at(source: &str, needle: &str) -> u32 {
    source.find(needle).expect("needle in source") as u32
}

fn expected(op: &str) -> String {
    format!("'{op}' expected.")
}

// ---------------------------------------------------------------------------
// Intersection (`&&`) stray union content

#[test]
fn intersection_stray_char_after_operand_reports_expected_per_operand() {
    let src = r"const a = /[a&&b|c]/v;";
    assert_eq!(
        diags(src),
        vec![
            (TS1005, expected("&&"), at(src, "|")),
            (TS1005, expected("&&"), at(src, "c]")),
        ]
    );
}

#[test]
fn intersection_stray_double_pipe_reports_three_expecteds() {
    // Each stray code point draws its own report until `]`.
    let src = r"const a = /[x&&y||z]/v;";
    assert_eq!(
        diags(src),
        vec![
            (TS1005, expected("&&"), at(src, "||z")),
            (TS1005, expected("&&"), at(src, "|z")),
            (TS1005, expected("&&"), at(src, "z]")),
        ]
    );
}

#[test]
fn intersection_single_trailing_operand_reports_once() {
    let src = r"const a = /[a&&bc]/v;";
    assert_eq!(diags(src), vec![(TS1005, expected("&&"), at(src, "c]"))]);
}

#[test]
fn intersection_stray_syntax_char_suppresses_its_own_ts1508() {
    // `(` and `)` are `ClassSetSyntaxCharacter`s; in operator-expected
    // position they draw only the `TS1005`, not the `TS1508` a real operand
    // scan would add.
    let src = r"const a = /[a&&b()]/v;";
    assert_eq!(
        diags(src),
        vec![
            (TS1005, expected("&&"), at(src, "()")),
            (TS1005, expected("&&"), at(src, ")]")),
        ]
    );
}

// ---------------------------------------------------------------------------
// Subtraction (`--`) reports its own operator name

#[test]
fn subtraction_stray_char_reports_dashes_expected() {
    let src = r"const a = /[a--b|c]/v;";
    assert_eq!(
        diags(src),
        vec![
            (TS1005, expected("--"), at(src, "|")),
            (TS1005, expected("--"), at(src, "c]")),
        ]
    );
}

// ---------------------------------------------------------------------------
// A stray operand is consumed as a whole operand, not one code point

#[test]
fn stray_nested_class_is_one_recovery_operand() {
    // The nested `[c]` is a single operand, so only its `[` is anchored.
    let src = r"const a = /[a&&b[c]d]/v;";
    assert_eq!(
        diags(src),
        vec![
            (TS1005, expected("&&"), at(src, "[c]")),
            (TS1005, expected("&&"), at(src, "d]")),
        ]
    );
}

#[test]
fn stray_class_escape_is_one_recovery_operand() {
    let src = r"const a = /[a&&b\dz]/v;";
    assert_eq!(
        diags(src),
        vec![
            (TS1005, expected("&&"), at(src, r"\dz")),
            (TS1005, expected("&&"), at(src, "z]")),
        ]
    );
}

#[test]
fn stray_string_disjunction_is_one_recovery_operand() {
    let src = r"const a = /[a&&b\q{xy}z]/v;";
    assert_eq!(
        diags(src),
        vec![
            (TS1005, expected("&&"), at(src, r"\q{xy}")),
            (TS1005, expected("&&"), at(src, "z]")),
        ]
    );
}

#[test]
fn stray_reserved_double_punctuator_is_one_recovery_operand() {
    // `~~` is a reserved double punctuator (`TS1522` in operand position); in
    // recovery it is consumed as one operand and its own report is suppressed,
    // so the pair draws a single `TS1005` at its first `~`.
    let src = r"const a = /[a&&b~~z]/v;";
    assert_eq!(
        diags(src),
        vec![
            (TS1005, expected("&&"), at(src, "~~z")),
            (TS1005, expected("&&"), at(src, "z]")),
        ]
    );
}

// ---------------------------------------------------------------------------
// A lone `&` is a malformed `&&`: TS1508, not TS1005

#[test]
fn lone_ampersand_in_intersection_reports_ts1508() {
    let src = r"const a = /[a&&b&c]/v;";
    // `&` errors, `c` is then a clean right operand — no trailing TS1005.
    assert_eq!(
        diags(src),
        vec![(
            TS1508,
            "Unexpected '&'. Did you mean to escape it with backslash?".to_string(),
            at(src, "&c"),
        )]
    );
}

#[test]
fn lone_ampersand_in_subtraction_also_reports_ts1508() {
    // `&` always starts the `&&` token, so it is a malformed operator even in
    // a class committed to `--`.
    let src = r"const a = /[a--b&c]/v;";
    assert_eq!(
        diags(src),
        vec![(
            TS1508,
            "Unexpected '&'. Did you mean to escape it with backslash?".to_string(),
            at(src, "&c"),
        )]
    );
}

// ---------------------------------------------------------------------------
// Re-sync: a later valid operator ends the recovery cleanly

#[test]
fn recovery_resyncs_to_a_later_valid_operator() {
    // The stray ` ` and `y` draw TS1005; the following `&&z` is a clean
    // intersection continuation and draws nothing.
    let src = r"const a = /[a&&x y&&z]/v;";
    assert_eq!(
        diags(src),
        vec![
            (TS1005, expected("&&"), at(src, " y")),
            (TS1005, expected("&&"), at(src, "y&&")),
        ]
    );
}

#[test]
fn recovery_then_the_other_operator_is_mixing_ts1519() {
    let src = r"const a = /[a&&x y--z]/v;";
    assert_eq!(
        diags(src),
        vec![
            (TS1005, expected("&&"), at(src, " y")),
            (TS1005, expected("&&"), at(src, "y--")),
            (TS1519, "Operators must not be mixed within a character class. Wrap it in a nested class instead.".to_string(), at(src, "--z")),
        ]
    );
}

// ---------------------------------------------------------------------------
// The right operand of a real operator is scanned normally (TS1508 applies)

#[test]
fn right_operand_of_operator_still_reports_syntax_char() {
    // `-` right after `&&` is a real operand position, so its `TS1508` fires;
    // the trailing `b` is then operator-expected and draws `TS1005`.
    let src = r"const a = /[a&&-b]/v;";
    assert_eq!(
        diags(src),
        vec![
            (
                TS1508,
                "Unexpected '-'. Did you mean to escape it with backslash?".to_string(),
                at(src, "-b"),
            ),
            (TS1005, expected("&&"), at(src, "b]")),
        ]
    );
}

// ---------------------------------------------------------------------------
// Well-formed set expressions and non-`v` classes are untouched

#[test]
fn well_formed_intersection_is_clean() {
    assert_eq!(
        codes(r"const a = /[a&&b&&c]/v;", ScriptTarget::ES2024),
        Vec::<u32>::new()
    );
}

#[test]
fn well_formed_subtraction_is_clean() {
    assert_eq!(
        codes(r"const a = /[a--b--c]/v;", ScriptTarget::ES2024),
        Vec::<u32>::new()
    );
}

#[test]
fn without_v_flag_the_continuation_rule_does_not_apply() {
    // Without `v`, `&&` is not an operator; `[a&&b~c]` is a plain union of
    // ordinary characters, so no `'&&' expected.` recovery runs.
    for target in [ScriptTarget::ES2024, ScriptTarget::ES2015] {
        assert!(
            !codes(r"const a = /[a&&b~c]/u;", target).contains(&TS1005),
            "u-mode class must not enter the &&-continuation recovery"
        );
        assert!(
            !codes(r"const a = /[a&&b~c]/;", target).contains(&TS1005),
            "flagless class must not enter the &&-continuation recovery"
        );
    }
}
