//! `in`-operator operand `checkNonNullType` parity.
//!
//! Under `strictNullChecks`, tsc runs `checkNonNullType` on **both** the key
//! (LHS) and object (RHS) operands of `in`, then runs the structural key/object
//! check on the **non-nullish remainder**. A nullable operand is reported as
//! TS18047/18048/18049 (named entity), TS2531/2532/2533 (unnamed expression),
//! or TS18050 (literal `null`/`undefined` keyword). An `unknown` operand is
//! reported as TS18046 (named) / TS2571 (unnamed) on either side. All codes and
//! messages below were verified against tsc 6.0.2.

use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::check_source_code_messages;

const TS2322: u32 = diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE;
const TS18046: u32 = diagnostic_codes::IS_OF_TYPE_UNKNOWN;
const TS2571: u32 = diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN;
const TS18047: u32 = diagnostic_codes::IS_POSSIBLY_NULL;
const TS18048: u32 = diagnostic_codes::IS_POSSIBLY_UNDEFINED;
const TS18049: u32 = diagnostic_codes::IS_POSSIBLY_NULL_OR_UNDEFINED;
const TS2532: u32 = diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED;
const TS18050: u32 = diagnostic_codes::THE_VALUE_CANNOT_BE_USED_HERE;

fn codes(source: &str) -> Vec<u32> {
    let mut v: Vec<u32> = check_source_code_messages(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect();
    v.sort_unstable();
    v
}

fn msgs(source: &str) -> Vec<(u32, String)> {
    check_source_code_messages(source)
}

const PRELUDE: &str = concat!(
    "declare const obj: object;\n",
    "declare const su: string | undefined;\n",
    "declare const sn: string | null;\n",
    "declare const snu: string | null | undefined;\n",
    "declare const ou: object | undefined;\n",
    "declare const on: object | null;\n",
    "declare const u: undefined;\n",
    "declare const n: null;\n",
    "declare function gkey(): string | undefined;\n",
    "declare function gobj(): object | undefined;\n",
);

fn src(expr: &str) -> String {
    format!("{PRELUDE}const r = {expr};\n")
}

// ---- LHS (key side): checkNonNullType, previously entirely unreported. ----

#[test]
fn lhs_named_nullish_reports_18047_48_49() {
    assert_eq!(codes(&src("su in obj")), vec![TS18048]);
    assert_eq!(codes(&src("sn in obj")), vec![TS18047]);
    assert_eq!(codes(&src("snu in obj")), vec![TS18049]);
    assert_eq!(codes(&src("u in obj")), vec![TS18048]);
    assert_eq!(codes(&src("n in obj")), vec![TS18047]);
}

#[test]
fn lhs_unnamed_nullish_reports_object_possibly_codes() {
    // A call result has no nameable text → TS2531/2532/2533, not TS18047-9.
    assert_eq!(codes(&src("gkey() in obj")), vec![TS2532]);
}

#[test]
fn lhs_literal_keyword_reports_18050() {
    assert_eq!(codes(&src("null in obj")), vec![TS18050]);
    assert_eq!(codes(&src("undefined in obj")), vec![TS18050]);
}

#[test]
fn lhs_non_key_remainder_coemits_with_nullish() {
    // `boolean | undefined`: the nullish part is reported AND the `boolean`
    // remainder still fails the key check.
    let got = codes(
        "declare const obj: object;\ndeclare const bu: boolean | undefined;\nconst r = bu in obj;\n",
    );
    assert_eq!(got, vec![TS2322, TS18048]);
}

#[test]
fn lhs_named_property_access_uses_dotted_name() {
    let m = msgs(
        "declare const obj: object;\ndeclare const p: { k: string | undefined };\nconst r = p.k in obj;\n",
    );
    assert!(
        m.iter()
            .any(|(c, s)| *c == TS18048 && s == "'p.k' is possibly 'undefined'."),
        "expected named TS18048 on `p.k`, got {m:#?}"
    );
}

// ---- RHS (object side): formerly TS2719/TS2322 spurious or wrong code. ----

#[test]
fn rhs_object_with_nullish_reports_only_nullish_not_self_mismatch() {
    // `object | undefined` previously surfaced a spurious TS2719; the stripped
    // `object` remainder is a valid RHS, so only the nullish diagnostic remains.
    assert_eq!(codes(&src("\"a\" in ou")), vec![TS18048]);
    assert_eq!(codes(&src("\"a\" in on")), vec![TS18047]);
}

#[test]
fn rhs_unnamed_nullish_reports_object_possibly_undefined() {
    assert_eq!(codes(&src("\"a\" in gobj()")), vec![TS2532]);
}

#[test]
fn rhs_string_remainder_coemits_structural_and_nullish() {
    // `string | null | undefined`: `string` is not a valid object RHS (TS2322)
    // and the operand is also possibly null|undefined (TS18049).
    assert_eq!(codes(&src("\"a\" in snu")), vec![TS2322, TS18049]);
}

#[test]
fn rhs_literal_keyword_reports_18050() {
    assert_eq!(codes(&src("\"a\" in null")), vec![TS18050]);
    assert_eq!(codes(&src("\"a\" in undefined")), vec![TS18050]);
}

#[test]
fn rhs_pure_nullish_named_reports_18047_48() {
    assert_eq!(codes(&src("\"a\" in u")), vec![TS18048]);
    assert_eq!(codes(&src("\"a\" in n")), vec![TS18047]);
}

// ---- unknown operand: TS18046 (named) / TS2571 (unnamed) on either side. ----

#[test]
fn unknown_operand_reports_of_type_unknown_on_both_sides() {
    let lhs =
        codes("declare const obj: object;\ndeclare const unk: unknown;\nconst r = unk in obj;\n");
    assert_eq!(lhs, vec![TS18046]);
    let rhs = codes("declare const unk: unknown;\nconst r = \"a\" in unk;\n");
    assert_eq!(rhs, vec![TS18046]);
    let rhs_unnamed = codes("declare function g(): unknown;\nconst r = \"a\" in g();\n");
    assert_eq!(rhs_unnamed, vec![TS2571]);
}

// ---- Anti-hardcoding: decision is structural, not name/text based. ----

#[test]
fn renamed_binders_still_report_nullish_operand() {
    // Same shape, different binder names — the diagnostic must still fire.
    let a = codes(
        "declare const wibble: object;\ndeclare const wobble: string | undefined;\nconst r = wobble in wibble;\n",
    );
    assert_eq!(a, vec![TS18048]);
}

// ---- Negative controls: valid non-nullish operands stay clean. ----

#[test]
fn non_nullish_operands_produce_no_in_diagnostics() {
    assert!(
        codes("declare const obj: object;\ndeclare const s: string;\nconst r = s in obj;\n")
            .is_empty()
    );
    assert!(codes("declare const o: { a: number };\nconst r = \"a\" in o;\n").is_empty());
}

#[test]
fn without_strict_null_checks_no_nullish_operand_diagnostics() {
    // checkNonNullType's TS18047-49 / TS2531-3 are strictNullChecks-only.
    let source = format!("{PRELUDE}const r = su in obj;\n");
    let got: Vec<u32> = tsz_checker::test_utils::check_with_options_code_messages(
        &source,
        tsz_checker::context::CheckerOptions {
            strict_null_checks: false,
            ..tsz_checker::context::CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|(c, _)| c)
    .collect();
    assert!(
        got.iter()
            .all(|c| *c != TS18048 && *c != TS18047 && *c != TS18049),
        "no nullish-operand diagnostics without strictNullChecks, got {got:#?}"
    );
}
