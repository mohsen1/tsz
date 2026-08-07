//! Type-member (interface / type-literal) modifier grammar parity with tsc.
//!
//! Two positions that `checkGrammarModifiers` rejects but tsz previously did
//! not, verified against `typescript@7.0.2`:
//!   * `export` / `in` / `out` as a modifier on a type member → TS1070
//!     (`'{0}' modifier cannot appear on a type member.`), anchored at the
//!     modifier. tsz previously mis-parsed `export` to TS1005 and dropped
//!     `in` / `out` silently.
//!   * `readonly` on a method or construct signature → TS1024
//!     (`'readonly' modifier can only appear on a property declaration or index
//!     signature.`), anchored at `readonly`. tsz previously stayed silent.
//!
//! `readonly` on a property / index signature stays legal, and `export` / `in`
//! / `out` used as a member *name* (`export: T`, `export(): void`) stay clean —
//! matching tsc. `import` / `const` / `default` are not type-member modifiers in
//! tsc (they take a parser-recovery path) and are intentionally not covered.

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

const TS1070: u32 = diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER;
const TS1024: u32 =
    diagnostic_codes::READONLY_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION_OR_INDEX_SIGNATURE;

// ---------------------------------------------------------------------------
// TS1070: export / in / out modifier on a type member
// ---------------------------------------------------------------------------

#[test]
fn export_modifier_on_interface_property_reports_ts1070() {
    // `export` starts at column 15 (`interface I { `).
    assert_eq!(
        fingerprints("interface I { export x: number; }"),
        vec![(
            TS1070,
            1,
            15,
            "'export' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn export_modifier_on_interface_method_reports_ts1070() {
    assert_eq!(
        fingerprints("interface I { export m(): void; }"),
        vec![(
            TS1070,
            1,
            15,
            "'export' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn export_modifier_on_type_literal_property_reports_ts1070() {
    // `type T = { ` → `export` at column 12.
    assert_eq!(
        fingerprints("type T = { export x: number; };"),
        vec![(
            TS1070,
            1,
            12,
            "'export' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn in_modifier_on_interface_property_reports_ts1070() {
    assert_eq!(
        fingerprints("interface Foo { in bar: number; }"),
        vec![(
            TS1070,
            1,
            17,
            "'in' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn out_modifier_on_interface_property_reports_ts1070() {
    assert_eq!(
        fingerprints("interface Foo { out bar: number; }"),
        vec![(
            TS1070,
            1,
            17,
            "'out' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn export_modifier_anchors_at_the_keyword_with_leading_whitespace() {
    let source = "interface I {\n    export foo(): void;\n}";
    assert_eq!(
        fingerprints(source),
        vec![(
            TS1070,
            2,
            5,
            "'export' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn export_before_legal_readonly_reports_only_ts1070() {
    // `readonly` after `export` is legal on the property, so only the `export`
    // is rejected — a single diagnostic, no TS1024.
    assert_eq!(
        codes("interface I { export readonly x: number; }"),
        vec![TS1070],
    );
}

// ---------------------------------------------------------------------------
// TS1024: readonly on a method / construct signature
// ---------------------------------------------------------------------------

#[test]
fn readonly_on_interface_method_reports_ts1024() {
    assert_eq!(
        fingerprints("interface I { readonly m(): void; }"),
        vec![(
            TS1024,
            1,
            15,
            "'readonly' modifier can only appear on a property declaration or index signature."
                .to_string()
        )],
    );
}

#[test]
fn readonly_on_interface_generic_method_reports_ts1024() {
    assert_eq!(
        fingerprints("interface I { readonly m<T>(): void; }"),
        vec![(
            TS1024,
            1,
            15,
            "'readonly' modifier can only appear on a property declaration or index signature."
                .to_string()
        )],
    );
}

#[test]
fn readonly_on_construct_signature_reports_ts1024() {
    assert_eq!(
        fingerprints("interface I { readonly new (): void; }"),
        vec![(
            TS1024,
            1,
            15,
            "'readonly' modifier can only appear on a property declaration or index signature."
                .to_string()
        )],
    );
}

#[test]
fn readonly_on_type_literal_method_reports_ts1024() {
    assert_eq!(
        fingerprints("type Shape = { readonly compute(): number; };"),
        vec![(
            TS1024,
            1,
            16,
            "'readonly' modifier can only appear on a property declaration or index signature."
                .to_string()
        )],
    );
}

// ---------------------------------------------------------------------------
// Negative controls: legal shapes stay clean, member-name uses stay clean
// ---------------------------------------------------------------------------

#[test]
fn readonly_property_and_index_signature_stay_clean() {
    assert!(codes("interface I { readonly x: number; }").is_empty());
    assert!(codes("interface I { readonly [k: string]: number; }").is_empty());
    assert!(codes("type T = { readonly y: string; };").is_empty());
}

#[test]
fn export_in_out_as_member_names_stay_clean() {
    // Followed by `:` → property name; followed by `(` → method name.
    assert!(codes("interface I { export: number; }").is_empty());
    assert!(codes("interface I { export(): void; }").is_empty());
    assert!(codes("interface I { in: number; }").is_empty());
    assert!(codes("interface I { out: number; }").is_empty());
    assert!(codes("interface I { in(): void; }").is_empty());
}

#[test]
fn method_named_readonly_stays_clean() {
    // `readonly` immediately followed by `(` is a method *named* `readonly`,
    // not a modifier — no TS1024.
    assert!(codes("interface I { readonly(): void; }").is_empty());
}

#[test]
fn plain_members_stay_clean() {
    assert!(codes("interface I { m(): void; x: number; }").is_empty());
}

#[test]
fn previously_covered_modifiers_still_report_ts1070() {
    // Guard against a regression in the shared modifier-error branch.
    for modifier in [
        "public",
        "private",
        "protected",
        "static",
        "abstract",
        "declare",
    ] {
        let source = format!("interface I {{ {modifier} x: number; }}");
        assert_eq!(codes(&source), vec![TS1070], "modifier `{modifier}`");
    }
}
