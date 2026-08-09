//! The `in` variance modifier directly before a `get`/`set` accessor in an
//! interface or type literal, in the confined shape `[clean-modifier]* in
//! (get|set)` where `in` is the accessor's immediate predecessor.
//!
//! `in` is a reserved binary operator, unlike `out` (a contextual keyword —
//! see `type_member_out_variance_accessor_cascade_tests.rs`), so although
//! `tsc` abandons the type-member body after `in` exactly like it does for
//! `out`/`async`/`declare`/`abstract`/`override`, the abandoned tail's
//! statement re-parse differs: `in` is NOT consumed before the re-parse
//! begins, because `tsc`'s expression parser folds it (and the following
//! `get`/`set`) into a missing-LHS binary expression (`<missing> in get`)
//! rather than reporting `get` as an unexpected keyword. The observable
//! result is one TS1131 per modifier (including `in` itself), then TS1005
//! (`';'`/`','` expected) at the tail's next unexpected token — no TS1434 —
//! then TS1128 at the container's closing `}`. Oracle-matched against
//! `typescript@7.0.2`.
//!
//! Deliberately NOT covered here (idiosyncratic `tsc` recoveries left on
//! their pre-existing paths):
//! - a hard modifier immediately before `in` (`async in get x()`): only the
//!   hard modifier gets its own TS1131; `in` does not, and does not start its
//!   own abandoned-tail re-parse either — a further narrower shape.
//!
//! The `set(param)` cases assert the oracle in full. They briefly pinned tsz's
//! then-current output instead, which omitted one trailing TS1005 (`','`
//! expected) that `tsc` reports — a general statement-parser gap in retrying a
//! fresh statement after a missing-LHS `in`/`instanceof` recovery when the
//! retried statement is a call expression with a malformed parameter list.
//! That was #17062, closed by #17064 (which extended #17052's
//! `force_next_missing_semicolon_error_once` one-shot to
//! `error_comma_expected`). The prediction held exactly: this cascade's wiring
//! was decoupled, and the assertions gained the missing diagnostic with no
//! change to the cascade itself.

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
const TS1005: u32 = diagnostic_codes::EXPECTED;
const TS1128: u32 = diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED;
const TS1070: u32 = diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER;

// ---------------------------------------------------------------------------
// Single `in` before an accessor in an interface.
// ---------------------------------------------------------------------------

#[test]
fn in_before_get_accessor_cascades() {
    // Oracle (typescript@7.0.2):
    //   (1,15) TS1131 in | (1,22) TS1005 ';' | (1,25) TS1005 ';' | (1,35) TS1128 }
    assert_eq!(
        fingerprints("interface I { in get x(): number; }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (TS1005, 1, 22, "';' expected.".to_string()),
            (TS1005, 1, 25, "';' expected.".to_string()),
            (
                TS1128,
                1,
                35,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

#[test]
fn in_before_set_accessor_cascades() {
    // Oracle (typescript@7.0.2), all four:
    //   (1,15) TS1131 in | (1,22) TS1005 ';' | (1,25) TS1005 ',' | (1,36) TS1128 }
    //
    // This previously pinned tsz's then-current 3-diagnostic output, deliberately
    // and with the oracle's real answer recorded in the comment: the middle
    // `','`-expected diagnostic was dropped by the #17062 gap, which was still
    // open when this suite was written. #17064 closed that gap (extending
    // #17052's `force_next_missing_semicolon_error_once` one-shot to
    // `error_comma_expected`), so tsz now emits all four and this asserts the
    // oracle directly.
    assert_eq!(
        fingerprints("interface I { in set x(v: number); }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (TS1005, 1, 22, "';' expected.".to_string()),
            (TS1005, 1, 25, "',' expected.".to_string()),
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
fn in_before_get_accessor_no_return_type_cascades() {
    // Oracle: (1,15) TS1131 | (1,22) TS1005 ';' | (1,26) TS1128 } — no second
    // TS1005 when there is no `: number` tail before the brace.
    assert_eq!(
        fingerprints("interface I { in get x() }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (TS1005, 1, 22, "';' expected.".to_string()),
            (
                TS1128,
                1,
                26,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

// ---------------------------------------------------------------------------
// Type literals: same cascade.
// ---------------------------------------------------------------------------

#[test]
fn in_before_get_accessor_on_type_literal_cascades() {
    assert_eq!(
        fingerprints("type T = { in get x(): number; };"),
        vec![
            (TS1131, 1, 12, "Property or signature expected.".to_string()),
            (TS1005, 1, 19, "';' expected.".to_string()),
            (TS1005, 1, 22, "';' expected.".to_string()),
            (
                TS1128,
                1,
                32,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

// ---------------------------------------------------------------------------
// Clean modifiers may precede `in`; one TS1131 per modifier (including `in`
// itself), then the tail.
// ---------------------------------------------------------------------------

#[test]
fn static_in_before_accessor_reports_ts1131_per_modifier() {
    // Oracle: (1,15) (1,22) TS1131 | (1,29) TS1005 | (1,32) TS1005 | (1,42) TS1128.
    assert_eq!(
        fingerprints("interface I { static in get x(): number; }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (TS1131, 1, 22, "Property or signature expected.".to_string()),
            (TS1005, 1, 29, "';' expected.".to_string()),
            (TS1005, 1, 32, "';' expected.".to_string()),
            (
                TS1128,
                1,
                42,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

#[test]
fn readonly_in_before_accessor_reports_ts1131_per_modifier() {
    assert_eq!(
        fingerprints("interface I { readonly in get x(): number; }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (TS1131, 1, 24, "Property or signature expected.".to_string()),
            (TS1005, 1, 31, "';' expected.".to_string()),
            (TS1005, 1, 34, "';' expected.".to_string()),
            (
                TS1128,
                1,
                44,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

// ---------------------------------------------------------------------------
// Structural, not keyed to any binder spelling.
// ---------------------------------------------------------------------------

#[test]
fn in_accessor_cascade_is_not_keyed_to_a_binder_name() {
    assert_eq!(
        fingerprints("interface Alpha { in get beta(): number; }"),
        vec![
            (TS1131, 1, 19, "Property or signature expected.".to_string()),
            (TS1005, 1, 26, "';' expected.".to_string()),
            (TS1005, 1, 32, "';' expected.".to_string()),
            (
                TS1128,
                1,
                42,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

// ---------------------------------------------------------------------------
// Negative controls: the cascade must fire ONLY for `[clean]* in (get|set)`.
// ---------------------------------------------------------------------------

#[test]
fn in_on_a_property_stays_ts1070() {
    // `in` before a plain property is the pre-existing semantic TS1070.
    assert_eq!(codes("interface I { in x: number; }"), vec![TS1070]);
}

#[test]
fn in_named_get_method_stays_ts1070() {
    // `in get(): void` — `get` immediately followed by `(` is a method
    // *named* `get`, not an accessor. The cascade lookahead must not fire.
    assert_eq!(codes("interface I { in get(): void; }"), vec![TS1070]);
}

#[test]
fn in_as_a_property_name_stays_clean() {
    // `in` used as the member's own name (`in: number`) is an ordinary
    // property — no cascade, no diagnostic.
    assert!(codes("interface I { in: number; }").is_empty());
}

#[test]
fn plain_accessor_without_modifier_stays_clean() {
    assert!(codes("interface I { get x(): number; set x(v: number); }").is_empty());
}

#[test]
fn hard_modifier_then_in_before_accessor_is_not_the_in_cascade() {
    // `in` AFTER a hard modifier (`async in get x()`) is a narrower,
    // out-of-scope shape (per the module doc comment): only the hard
    // modifier gets its own TS1131 in tsc, and `in` does not start its own
    // abandoned-tail re-parse. tsz's pre-existing single-TS1070 recovery for
    // the leading hard modifier is unaffected by this cascade's lookahead,
    // which requires the run to *start* with `in` or a clean modifier.
    assert_eq!(
        codes("interface I { async in get x(): number; }"),
        vec![TS1070],
    );
}

#[test]
fn line_break_between_in_and_accessor_does_not_cascade() {
    // A line break between `in` and `get`/`set` takes tsc down a different
    // (out-of-scope) path; the same-line-only lookahead must not fire.
    let with_break = "interface I {\n  in\n  get x(): number\n}";
    let without_break = "interface I {\n  in get x(): number\n}";
    assert_ne!(codes(with_break), codes(without_break));
}
