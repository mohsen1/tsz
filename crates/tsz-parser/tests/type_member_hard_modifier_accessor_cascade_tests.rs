//! A "hard" type-member modifier (`async`/`declare`/`abstract`/`override`)
//! directly before a `get`/`set` accessor in an interface or type literal.
//!
//! Unlike the "clean" modifiers (`private`/`protected`/`public`/`static`/
//! `accessor`/`export`/`readonly`), which `tsc` rejects with a single TS1131
//! per modifier and then recovers as a bare accessor, a run containing a hard
//! modifier does not parse as any member at all in `typescript@7.0.2`. After
//! one TS1131 per modifier in the run, `tsc` abandons the type-member body and
//! re-parses the accessor's own tail as top-level statements — a cascade of
//! TS1434 (`Unexpected keyword or identifier.`) at the accessor keyword,
//! TS1005 (`';'`/`','` expected) at the tail's next unexpected `:`/`,`, and
//! TS1128 (`Declaration or statement expected.`) at the container's closing
//! `}`. tsz previously reported the uniform semantic TS1070 for these; this
//! suite pins the cascade parity.
//!
//! `in`/`out` are intentionally NOT covered: `in` is a reserved operator whose
//! statement re-parse differs, and both carry variance-position idiosyncrasies
//! — they keep the pre-existing semantic TS1070 (asserted in
//! `type_member_modifier_grammar_tests.rs`).

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

// ---------------------------------------------------------------------------
// Single hard modifier before a get accessor in an interface.
// ---------------------------------------------------------------------------

#[test]
fn async_before_get_accessor_cascades() {
    // Oracle (typescript@7.0.2):
    //   (1,15) TS1131 async | (1,21) TS1434 get | (1,28) TS1005 ';' | (1,38) TS1128 }
    assert_eq!(
        fingerprints("interface I { async get x(): number; }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                21,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 1, 28, "';' expected.".to_string()),
            (
                TS1128,
                1,
                38,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

#[test]
fn async_before_set_accessor_cascades_with_comma_expected() {
    // A setter's `(v: number)` param list makes the tail's next unexpected
    // token the `:` inside the parens → TS1005 `','` expected (not `';'`).
    assert_eq!(
        fingerprints("interface I { async set x(v: number); }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                21,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 1, 28, "',' expected.".to_string()),
            (
                TS1128,
                1,
                39,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

#[test]
fn declare_before_get_accessor_cascades() {
    assert_eq!(
        fingerprints("interface I { declare get x(): number; }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                23,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 1, 30, "';' expected.".to_string()),
            (
                TS1128,
                1,
                40,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

#[test]
fn abstract_before_get_accessor_cascades() {
    assert_eq!(
        fingerprints("interface I { abstract get x(): number; }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                24,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 1, 31, "';' expected.".to_string()),
            (
                TS1128,
                1,
                41,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

#[test]
fn override_before_get_accessor_cascades() {
    assert_eq!(
        fingerprints("interface I { override get x(): number; }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                24,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 1, 31, "';' expected.".to_string()),
            (
                TS1128,
                1,
                41,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

#[test]
fn async_before_get_accessor_no_return_type_cascades() {
    // Oracle: (1,15) TS1131 | (1,21) TS1434 | (1,29) TS1128 } — no TS1005 when
    // there is no `: number` tail before the brace.
    assert_eq!(
        fingerprints("interface I { async get x() }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                21,
                "Unexpected keyword or identifier.".to_string()
            ),
            (
                TS1128,
                1,
                29,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

// ---------------------------------------------------------------------------
// Type literals: same cascade.
// ---------------------------------------------------------------------------

#[test]
fn async_before_get_accessor_on_type_literal_cascades() {
    assert_eq!(
        fingerprints("type T = { async get x(): number; };"),
        vec![
            (TS1131, 1, 12, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                18,
                "Unexpected keyword or identifier.".to_string()
            ),
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
fn declare_before_set_accessor_on_type_literal_cascades() {
    assert_eq!(
        fingerprints("type T = { declare set x(v: number); };"),
        vec![
            (TS1131, 1, 12, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                20,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 1, 27, "',' expected.".to_string()),
            (
                TS1128,
                1,
                38,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

// ---------------------------------------------------------------------------
// Stacked runs: a clean modifier leading a hard one still cascades, with one
// TS1131 per modifier before the tail re-parse.
// ---------------------------------------------------------------------------

#[test]
fn clean_then_hard_modifier_before_accessor_reports_ts1131_per_modifier() {
    // `static declare get x()` — TS1131 at both `static` and `declare`, then
    // the cascade tail. Oracle: (1,15) (1,22) TS1131 | (1,30) TS1434 |
    // (1,37) TS1005 | (1,47) TS1128.
    assert_eq!(
        fingerprints("interface I { static declare get x(): number; }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (TS1131, 1, 22, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                30,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 1, 37, "';' expected.".to_string()),
            (
                TS1128,
                1,
                47,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

#[test]
fn public_then_abstract_before_accessor_cascades() {
    assert_eq!(
        fingerprints("interface I { public abstract get x(): number; }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (TS1131, 1, 22, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                31,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 1, 38, "';' expected.".to_string()),
            (
                TS1128,
                1,
                48,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

#[test]
fn readonly_then_async_before_accessor_cascades() {
    assert_eq!(
        fingerprints("interface I { readonly async get x(): number; }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string()),
            (TS1131, 1, 24, "Property or signature expected.".to_string()),
            (
                TS1434,
                1,
                30,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 1, 37, "';' expected.".to_string()),
            (
                TS1128,
                1,
                47,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

// ---------------------------------------------------------------------------
// Multi-line / ASI: once the body is abandoned, every following member is also
// re-parsed as statements.
// ---------------------------------------------------------------------------

#[test]
fn async_accessor_multiline_reparses_the_whole_tail() {
    // Oracle: (2,3) TS1131 | (2,9) TS1434 | (2,20) TS1005 ';' | (3,3) TS1434 |
    // (3,9) TS1434 | (3,20) TS1005 ',' | (4,1) TS1128.
    let source = "interface Widget {\n  async get value(): string\n  async set value(v: string)\n}";
    assert_eq!(
        fingerprints(source),
        vec![
            (TS1131, 2, 3, "Property or signature expected.".to_string()),
            (
                TS1434,
                2,
                9,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 2, 20, "';' expected.".to_string()),
            (
                TS1434,
                3,
                3,
                "Unexpected keyword or identifier.".to_string()
            ),
            (
                TS1434,
                3,
                9,
                "Unexpected keyword or identifier.".to_string()
            ),
            (TS1005, 3, 20, "',' expected.".to_string()),
            (
                TS1128,
                4,
                1,
                "Declaration or statement expected.".to_string()
            ),
        ],
    );
}

// ---------------------------------------------------------------------------
// Structural, not keyed to any binder spelling.
// ---------------------------------------------------------------------------

#[test]
fn hard_modifier_accessor_cascade_is_not_keyed_to_a_binder_name() {
    // Renamed container and accessor; the cascade's leading TS1131 is fixed.
    assert_eq!(
        codes("interface Alpha { abstract get beta(): number; }"),
        vec![TS1131, TS1434, TS1005, TS1128],
    );
    assert_eq!(
        codes("type Gamma = { declare get delta(): number; };"),
        vec![TS1131, TS1434, TS1005, TS1128],
    );
}

// ---------------------------------------------------------------------------
// Negative controls: the fix must not fire where a hard modifier is NOT
// immediately before an accessor keyword.
// ---------------------------------------------------------------------------

#[test]
fn hard_modifier_on_method_or_property_stays_ts1070() {
    // A hard modifier before a plain method/property is the pre-existing
    // semantic TS1070, untouched by the accessor cascade.
    const TS1070: u32 = diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER;
    for modifier in ["async", "declare", "abstract", "override"] {
        assert_eq!(
            codes(&format!("interface I {{ {modifier} m(): void; }}")),
            vec![TS1070],
            "method: {modifier}",
        );
        assert_eq!(
            codes(&format!("interface I {{ {modifier} p: number; }}")),
            vec![TS1070],
            "property: {modifier}",
        );
    }
}

#[test]
fn hard_modifier_named_get_method_stays_ts1070() {
    // `async get(): number` — `get` immediately followed by `(` is a method
    // *named* `get`, not an accessor. The cascade lookahead must not fire; the
    // pre-existing semantic TS1070 stands.
    const TS1070: u32 = diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER;
    assert_eq!(codes("interface I { async get(): number; }"), vec![TS1070]);
    assert_eq!(codes("interface I { declare get: number; }"), vec![TS1070]);
}

#[test]
fn hard_modifier_as_a_property_name_stays_clean() {
    // `async`/`declare` used as the member's own name (`async: number`) are
    // ordinary properties — no cascade, no diagnostic.
    assert!(codes("interface I { async: number; }").is_empty());
    assert!(codes("interface I { declare: number; }").is_empty());
}

#[test]
fn line_break_between_hard_modifier_and_accessor_does_not_cascade() {
    // A line break between the modifier and `get`/`set` takes tsc down a
    // different (out-of-scope) ASI path; the same-line-only lookahead must not
    // fire, so the two sources differ.
    let with_break = "interface I {\n  async\n  get x(): number\n}";
    let without_break = "interface I {\n  async get x(): number\n}";
    assert_ne!(codes(with_break), codes(without_break));
}

#[test]
fn plain_accessor_without_modifier_stays_clean() {
    assert!(codes("interface I { get x(): number; set x(v: number); }").is_empty());
}
