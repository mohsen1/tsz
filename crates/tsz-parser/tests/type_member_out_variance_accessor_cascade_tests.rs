//! The `out` variance modifier directly before a `get`/`set` accessor in an
//! interface or type literal, in the confined shape `[clean-modifier]* out
//! (get|set)` where `out` is the accessor's immediate predecessor.
//!
//! `tsc` treats `out` in this position exactly like a "hard" modifier
//! (`async`/`declare`/`abstract`/`override`): it parses `out` as a variance
//! modifier, reports one TS1131 per modifier in the run, then abandons the
//! type-member body and re-parses the accessor's own tail as top-level
//! statements — TS1434 (`Unexpected keyword or identifier.`) at the accessor
//! keyword, TS1005 (`';'`/`','` expected) at the tail's next unexpected
//! `:`/`,`, and TS1128 (`Declaration or statement expected.`) at the
//! container's closing `}`. tsz previously reported the uniform semantic TS1070
//! for these; this suite pins the cascade parity, oracle-matched against
//! `typescript@7.0.2`.
//!
//! Deliberately NOT covered here (idiosyncratic `tsc` recoveries left on their
//! pre-existing paths, guarded by the negative controls below):
//! - a modifier *after* `out` (`out readonly get`, `out async get`);
//! - `out` *after* a hard modifier (`async out get`).
//!
//! The `in` variance modifier's own cascade (a reserved operator, so its
//! statement re-parse differs from `out`'s) is covered separately in
//! `type_member_in_variance_accessor_cascade_tests.rs`.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::position::LineMap;

fn fingerprints(source: &str) -> Vec<(u32, u32, u32, String)> {
    let (parser, _root) = parse_source(source);
    let line_map = LineMap::build(source);
    parser
        .get_diagnostics()
        .iter()
        .map(|diag| {
            let pos = line_map.offset_to_position(diag.start, source);
            (
                diag.code,
                pos.line + 1,
                pos.character + 1,
                diag.message.clone(),
            )
        })
        .collect()
}

fn codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

const TS1131: u32 = diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED;
const TS1434: u32 = diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER;
const TS1005: u32 = diagnostic_codes::EXPECTED;
const TS1128: u32 = diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED;
const TS1070: u32 = diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER;

const CASCADE_CODES: [u32; 4] = [TS1131, TS1434, TS1005, TS1128];

// ---------------------------------------------------------------------------
// Single `out` before an accessor in an interface.
// ---------------------------------------------------------------------------

#[test]
fn out_before_get_accessor_cascades() {
    // Oracle (typescript@7.0.2):
    //   (1,15) TS1131 out | (1,19) TS1434 get | (1,26) TS1005 ';' | (1,36) TS1128 }
    assert_eq!(
        fingerprints("interface I { out get x(): number; }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                19,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 1, 26, "';' expected.".to_string()),
            (
                TS1128,
                1,
                36,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

#[test]
fn out_before_set_accessor_cascades_with_comma_expected() {
    // A setter's `(v: number)` param list makes the tail's next unexpected
    // token the `:` inside the parens → TS1005 `','` expected (not `';'`).
    assert_eq!(
        fingerprints("interface I { out set x(v: number); }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                19,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 1, 26, "',' expected.".to_string()),
            (
                TS1128,
                1,
                37,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

#[test]
fn out_before_get_accessor_no_return_type_cascades() {
    // Oracle: (1,15) TS1131 | (1,19) TS1434 | (1,27) TS1128 } — no TS1005 when
    // there is no `: number` tail before the brace.
    assert_eq!(
        fingerprints("interface I { out get x() }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                19,
                "Unexpected keyword or identifier.".to_string()
            ),
            (
                TS1128,
                1,
                27,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

// ---------------------------------------------------------------------------
// Type literals: same cascade.
// ---------------------------------------------------------------------------

#[test]
fn out_before_get_accessor_on_type_literal_cascades() {
    assert_eq!(
        fingerprints("type T = { out get x(): number; };"),
        vec![
            (TS1131, 1, 12, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                16,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 1, 23, "';' expected.".to_string()),
            (
                TS1128,
                1,
                33,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

// ---------------------------------------------------------------------------
// Clean modifiers may precede `out`; one TS1131 per modifier, then the tail.
// ---------------------------------------------------------------------------

#[test]
fn static_out_before_accessor_reports_ts1131_per_modifier() {
    // Oracle: (1,15) (1,22) TS1131 | (1,26) TS1434 | (1,33) TS1005 | (1,43) TS1128.
    assert_eq!(
        fingerprints("interface I { static out get x(): number; }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (TS1131, 1, 22, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                26,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 1, 33, "';' expected.".to_string()),
            (
                TS1128,
                1,
                43,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

#[test]
fn readonly_out_before_accessor_reports_ts1131_per_modifier() {
    assert_eq!(
        fingerprints("interface I { readonly out get x(): number; }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (TS1131, 1, 24, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                28,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 1, 35, "';' expected.".to_string()),
            (
                TS1128,
                1,
                45,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

#[test]
fn public_out_before_accessor_reports_ts1131_per_modifier() {
    assert_eq!(
        fingerprints("interface I { public out get x(): number; }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (TS1131, 1, 22, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                26,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 1, 33, "';' expected.".to_string()),
            (
                TS1128,
                1,
                43,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

#[test]
fn two_clean_modifiers_then_out_reports_ts1131_per_modifier() {
    // `protected static out get x()` — three TS1131 (protected, static, out),
    // then the cascade tail.
    assert_eq!(
        codes("interface I { protected static out get x(): number; }"),
        vec![TS1131, TS1131, TS1131, TS1434, TS1005, TS1128],
    );
}

// ---------------------------------------------------------------------------
// Structural, not keyed to any binder spelling.
// ---------------------------------------------------------------------------

#[test]
fn out_accessor_cascade_is_not_keyed_to_a_binder_name() {
    assert_eq!(
        codes("interface Alpha { out get beta(): number; }"),
        CASCADE_CODES.to_vec(),
    );
    assert_eq!(
        codes("type Gamma = { out set delta(v: number); };"),
        CASCADE_CODES.to_vec(),
    );
}

// ---------------------------------------------------------------------------
// Negative controls: the cascade must fire ONLY for `[clean]* out (get|set)`.
// ---------------------------------------------------------------------------

#[test]
fn out_on_a_property_stays_ts1070() {
    // `out` before a plain property is the pre-existing semantic TS1070.
    assert_eq!(codes("interface I { out x: number; }"), vec![TS1070]);
}

#[test]
fn out_named_get_method_stays_ts1070() {
    // `out get(): void` — `get` immediately followed by `(` is a method *named*
    // `get`, not an accessor. The cascade lookahead must not fire.
    assert_eq!(codes("interface I { out get(): void; }"), vec![TS1070]);
    assert_eq!(codes("interface I { out get: number; }"), vec![TS1070]);
}

#[test]
fn out_as_a_property_name_stays_clean() {
    // `out` used as the member's own name (`out: number`) is an ordinary
    // property — no cascade, no diagnostic.
    assert!(codes("interface I { out: number; }").is_empty());
}

#[test]
fn plain_accessor_without_modifier_stays_clean() {
    assert!(codes("interface I { get x(): number; set x(v: number); }").is_empty());
}

// ---------------------------------------------------------------------------
// Excluded shapes: a modifier after `out` and `out` after a hard modifier
// keep their pre-existing (non-cascade) recovery — the new lookahead must not
// route them into the clean `out` cascade. `in`'s own (different) cascade is
// covered in `type_member_in_variance_accessor_cascade_tests.rs`.
// ---------------------------------------------------------------------------

#[test]
fn out_then_modifier_before_accessor_is_not_the_clean_cascade() {
    // A modifier AFTER `out` (`out readonly get`, `out async get`) has an
    // idiosyncratic tsc recovery not reproduced here — it must not be routed
    // into the confined clean-`out` cascade.
    assert_ne!(
        codes("interface I { out readonly get x(): number; }"),
        CASCADE_CODES.to_vec(),
    );
    assert_ne!(
        codes("interface I { out async get x(): number; }"),
        CASCADE_CODES.to_vec(),
    );
}

#[test]
fn hard_modifier_then_out_before_accessor_is_not_the_clean_cascade() {
    // `out` AFTER a hard modifier (`async out get`) — tsc stops at the hard
    // modifier, so `out` falls into the statement re-parse; not covered here.
    assert_ne!(
        codes("interface I { async out get x(): number; }"),
        CASCADE_CODES.to_vec(),
    );
}

#[test]
fn line_break_between_out_and_accessor_does_not_cascade() {
    // A line break between `out` and `get`/`set` takes tsc down a different
    // (out-of-scope) ASI path; the same-line-only lookahead must not fire.
    let with_break = "interface I {\n  out\n  get x(): number\n}";
    let without_break = "interface I {\n  out get x(): number\n}";
    assert_ne!(codes(with_break), codes(without_break));
}
