//! A "hard" type-member modifier (`async`/`declare`/`abstract`/`override`)
//! immediately followed by the `out` variance modifier, immediately followed
//! by a `get`/`set` accessor in an interface or type literal.
//!
//! `tsc` stops parsing modifiers at the hard modifier: one TS1131 is reported
//! per modifier up to and including the hard one (clean modifiers, then the
//! hard modifier — `out` itself never gets its own TS1131), and the
//! type-member body is then abandoned. `out` and the accessor keyword both
//! fall into the re-parsed statement tail and each independently report their
//! own TS1434 (`out` is a contextual keyword, exactly like `get`/`set`, so the
//! general statement parser treats it the same way), followed by TS1005 at
//! the tail's next unexpected `:`/`,` and TS1128 at the container's closing
//! `}`. tsz previously reported the uniform semantic TS1070 for this shape
//! (it is a distinct case from both `type_member_hard_modifier_accessor_cascade_tests.rs`,
//! which has no `out`, and `type_member_out_variance_accessor_cascade_tests.rs`,
//! whose confined shape requires `out` to be the run's own last modifier).
//!
//! Deliberately NOT covered here (idiosyncratic recoveries left on their
//! pre-existing paths):
//! - `out` *before* a hard modifier (`out async get`) — `tsc` stops parsing
//!   modifiers at `out` itself, a different (already-excluded) shape;
//! - two hard modifiers, or a hard modifier repeated;
//! - the `in` variance modifier in this position (reserved-operator re-parse,
//!   a separate, deeper gap — see the NOTE on issue #16291).

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

const CASCADE_CODES: [u32; 5] = [TS1131, TS1434, TS1434, TS1005, TS1128];

// ---------------------------------------------------------------------------
// One hard modifier before `out` before an accessor, in an interface.
// ---------------------------------------------------------------------------

#[test]
fn async_out_before_get_accessor_cascades() {
    // Oracle (typescript@7.0.2):
    //   (1,15) TS1131 async | (1,21) TS1434 out | (1,25) TS1434 get |
    //   (1,32) TS1005 ';' | (1,42) TS1128 }
    assert_eq!(
        fingerprints("interface I { async out get x(): number; }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                21,
                "Unexpected keyword or identifier.".to_string()
            ),
            (
                TS1434,
                1,
                25,
                "Unexpected keyword or identifier.".to_string()
            ),
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
fn async_out_before_set_accessor_cascades_with_comma_expected() {
    assert_eq!(
        fingerprints("interface I { async out set x(v: number); }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                21,
                "Unexpected keyword or identifier.".to_string()
            ),
            (
                TS1434,
                1,
                25,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 1, 32, "',' expected.".to_string()),
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
fn async_out_before_get_accessor_no_return_type_cascades() {
    // No TS1005 when there is no `: number` tail before the brace.
    assert_eq!(
        codes("interface I { async out get x() }"),
        vec![TS1131, TS1434, TS1434, TS1128],
    );
}

// ---------------------------------------------------------------------------
// Every hard modifier, and type literals.
// ---------------------------------------------------------------------------

#[test]
fn declare_out_before_get_accessor_cascades() {
    assert_eq!(
        codes("interface I { declare out get x(): number; }"),
        CASCADE_CODES
    );
}

#[test]
fn abstract_out_before_get_accessor_cascades() {
    assert_eq!(
        codes("interface I { abstract out get x(): number; }"),
        CASCADE_CODES
    );
}

#[test]
fn override_out_before_get_accessor_cascades() {
    assert_eq!(
        codes("interface I { override out get x(): number; }"),
        CASCADE_CODES
    );
}

#[test]
fn out_before_get_accessor_on_type_literal_cascades() {
    assert_eq!(
        codes("type T = { async out get x(): number; };"),
        CASCADE_CODES
    );
}

// ---------------------------------------------------------------------------
// A clean modifier may precede the hard modifier; each gets its own TS1131.
// ---------------------------------------------------------------------------

#[test]
fn static_async_out_before_accessor_reports_ts1131_per_modifier() {
    // Oracle: (1,15) static, (1,22) async -> TS1131 each; `out` gets none.
    assert_eq!(
        codes("interface I { static async out get x(): number; }"),
        vec![TS1131, TS1131, TS1434, TS1434, TS1005, TS1128],
    );
}

// ---------------------------------------------------------------------------
// Structural, not keyed to any binder spelling.
// ---------------------------------------------------------------------------

#[test]
fn hard_then_out_cascade_is_not_keyed_to_a_binder_name() {
    assert_eq!(
        codes("interface Alpha { async out get beta(): number; }"),
        CASCADE_CODES.to_vec(),
    );
    assert_eq!(
        codes("type Gamma = { declare out set delta(v: number); };"),
        CASCADE_CODES.to_vec(),
    );
}

// ---------------------------------------------------------------------------
// Negative controls.
// ---------------------------------------------------------------------------

#[test]
fn async_out_on_a_property_stays_ts1070() {
    // Not an accessor shape — the pre-existing semantic TS1070 (for the
    // first illegal modifier, `async`) is unaffected.
    assert_eq!(codes("interface I { async out x: number; }"), vec![TS1070]);
}

#[test]
fn out_before_hard_modifier_is_not_the_hard_then_out_cascade() {
    // `out` BEFORE a hard modifier (`out async get`) is the mirror shape,
    // excluded by both this cascade and the plain `out` cascade — `tsc`
    // stops parsing modifiers at `out` itself. Must not be routed here.
    assert_ne!(
        codes("interface I { out async get x(): number; }"),
        CASCADE_CODES.to_vec(),
    );
}

#[test]
fn plain_hard_modifier_without_out_is_unaffected() {
    // The existing hard-modifier-only cascade (no `out` involved) must keep
    // producing its own (4-diagnostic) fingerprint, not this file's 5.
    assert_eq!(
        codes("interface I { async get x(): number; }"),
        vec![TS1131, TS1434, TS1005, TS1128],
    );
}

#[test]
fn plain_out_without_a_hard_modifier_is_unaffected() {
    // The existing `[clean]* out (get|set)` cascade (no hard modifier
    // involved) must keep producing its own fingerprint via its own
    // look-ahead, not this one.
    assert_eq!(
        codes("interface I { out get x(): number; }"),
        vec![TS1131, TS1434, TS1005, TS1128],
    );
}

#[test]
fn out_named_get_method_after_hard_modifier_stays_ts1070() {
    // `async out get(): void` — `get` immediately followed by `(` is a
    // method *named* `get`, not an accessor. The cascade look-ahead must not
    // fire.
    assert_eq!(
        codes("interface I { async out get(): void; }"),
        vec![TS1070]
    );
}
