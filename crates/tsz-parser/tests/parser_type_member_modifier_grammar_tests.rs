//! Grammar recovery for class-member modifiers written on a **type** member
//! (an interface member or a type-literal member).
//!
//! `tsc`'s `checkGrammarModifiers` rejects every class-member modifier on a
//! type member with a single diagnostic anchored on — and naming — the FIRST
//! modifier: `TS1070` (`'{0}' modifier cannot appear on a type member.`) for a
//! property/method member, `TS1071` (`... on an index signature.`) for an index
//! signature. `readonly` is the one modifier `tsc` accepts, so it never
//! triggers and is preserved when it leads the actual member.
//!
//! Two gaps this pins:
//!   * `export` was absent from the modifier set, so `export`-led type members
//!     mis-recovered (`TS1005` for an interface member, a `TS1128`/`TS1131`/
//!     `TS1434` cascade for a type-literal member) instead of `TS1070`/`TS1071`.
//!   * A run of more than one modifier (`public static x`) recovered only past
//!     the first modifier, so the rest mis-parsed into a `TS1005` cascade
//!     instead of `tsc`'s single report.
//!
//! Oracle for every row: `tsc@7.0.2`
//! (`--noEmit --strict --target es2022 --pretty false`).

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::position::LineMap;

/// `(code, line, column, message)` fingerprints, 1-based line/column, in the
/// order the parser reported them.
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

// ── `export` on a type member: the missing modifier ──────────────────────────

#[test]
fn export_on_interface_property_is_ts1070_anchored_on_export() {
    // `interface Shape { export width: number; }` — `export` at column 19.
    let src = "interface Shape { export width: number; }";
    assert_eq!(
        fingerprints(src),
        vec![(
            diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER,
            1,
            19,
            "'export' modifier cannot appear on a type member.".to_string(),
        )],
        "single TS1070 on the `export` modifier, member recovers cleanly",
    );
}

#[test]
fn export_on_interface_method_is_ts1070() {
    let src = "interface Store { export load(): void; }";
    assert_eq!(
        codes(src),
        vec![diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER],
    );
}

#[test]
fn export_on_type_literal_property_is_ts1070_no_cascade() {
    // Previously mis-recovered as a TS1128/TS1131/TS1434 cascade.
    let src = "type Row = { export id: number; };";
    assert_eq!(
        codes(src),
        vec![diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER],
    );
}

#[test]
fn export_on_index_signature_is_ts1071() {
    let src = "interface Bag { export [key: string]: number; }";
    assert_eq!(
        fingerprints(src),
        vec![(
            diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_AN_INDEX_SIGNATURE,
            1,
            17,
            "'export' modifier cannot appear on an index signature.".to_string(),
        )],
    );
}

#[test]
fn export_before_readonly_property_keeps_the_readonly_member() {
    // `export` is illegal (TS1070); the trailing `readonly` is a legal member
    // modifier and must not itself draw a diagnostic.
    let src = "interface Cfg { export readonly name: string; }";
    assert_eq!(
        codes(src),
        vec![diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER],
    );
}

// ── `export` used as a legal member *name* stays clean ────────────────────────

#[test]
fn export_as_property_name_is_clean() {
    // `export` followed by `:` is a property literally named `export`.
    assert!(codes("interface I { export: number; }").is_empty());
}

#[test]
fn export_as_method_name_is_clean() {
    assert!(codes("interface I { export(): void; }").is_empty());
}

// ── Multi-modifier runs report once, on the first modifier ───────────────────

#[test]
fn two_modifiers_report_once_on_the_first() {
    // `public static` — tsc names `public` (the first), once.
    let src = "interface I { public static value: number; }";
    assert_eq!(
        fingerprints(src),
        vec![(
            diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER,
            1,
            15,
            "'public' modifier cannot appear on a type member.".to_string(),
        )],
    );
}

#[test]
fn two_modifiers_reordered_names_the_first() {
    // `static public` — the first modifier is now `static`.
    let src = "interface I { static public value: number; }";
    assert_eq!(
        fingerprints(src),
        vec![(
            diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER,
            1,
            15,
            "'static' modifier cannot appear on a type member.".to_string(),
        )],
    );
}

#[test]
fn export_leading_a_modifier_run_names_export() {
    let src = "interface I { export static value: number; }";
    assert_eq!(
        codes(src),
        vec![diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER],
    );
    assert!(fingerprints(src)[0].3.starts_with("'export'"));
}

#[test]
fn three_modifiers_report_once_and_keep_the_readonly_member() {
    // `public static readonly x` — one TS1070 on `public`, and the trailing
    // `readonly x` still parses as a member (no cascade).
    let src = "interface I { public static readonly x: number; }";
    assert_eq!(
        codes(src),
        vec![diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER],
    );
}

#[test]
fn modifier_run_before_index_signature_is_a_single_ts1071() {
    let src = "interface I { public static [key: string]: number; }";
    assert_eq!(
        fingerprints(src),
        vec![(
            diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_AN_INDEX_SIGNATURE,
            1,
            15,
            "'public' modifier cannot appear on an index signature.".to_string(),
        )],
    );
}

#[test]
fn declare_export_type_literal_names_declare() {
    // Type-literal, two modifiers; tsc names the first (`declare`), once.
    let src = "type T = { declare export field: number; };";
    assert_eq!(
        codes(src),
        vec![diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER],
    );
    assert!(fingerprints(src)[0].3.starts_with("'declare'"));
}

#[test]
fn illegal_modifier_does_not_swallow_following_members() {
    // The recovered member and the next member both parse: exactly one
    // diagnostic, and `y` is not lost to a cascade.
    let src = "interface I { export x: number; y: string; }";
    assert_eq!(
        codes(src),
        vec![diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER],
    );
}

// ── Regressions: legal shapes stay clean ─────────────────────────────────────

#[test]
fn readonly_property_is_clean() {
    assert!(codes("interface I { readonly x: number; }").is_empty());
}

#[test]
fn readonly_index_signature_is_clean() {
    assert!(codes("interface I { readonly [key: string]: number; }").is_empty());
}

#[test]
fn plain_property_and_method_are_clean() {
    assert!(codes("interface Api { url: string; send(): void; }").is_empty());
}

// ── Renamed-binder variants (anti-hardcoding gate) ───────────────────────────

#[test]
fn export_gap_is_not_keyed_to_a_binder_name() {
    // Same defect under different interface/type-literal and member names: the
    // rule is structural, not tied to any identifier spelling.
    for src in [
        "interface Alpha { export beta: number; }",
        "interface ZZ { export qq(): void; }",
        "type Gamma = { export delta: string; };",
    ] {
        assert_eq!(
            codes(src),
            vec![diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER],
            "structural rule must fire regardless of binder names: {src}",
        );
    }
}
