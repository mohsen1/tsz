//! TS2687 for merged interface declarations that disagree on the `readonly`
//! or optional (`?`) property modifier.
//!
//! Structural rule: when two or more top-level `interface` declarations with
//! the same name merge into one symbol, `tsc`'s `checkVariableLikeDeclaration`
//! requires every property signature sharing a name across the merged group to
//! carry identical `readonly`/optional flags, or it reports TS2687 ("All
//! declarations of '{0}' must have identical modifiers.") on every disagreeing
//! declaration. `check_merged_interface_declaration_diagnostics`
//! (`duplicate_identifiers_followup.rs`) already checked TS2717 (differing
//! types) and TS2413 (index signatures) across the merged group but never
//! TS2687 — a same-typed property that only disagrees on `?` produced no
//! diagnostic at all. Oracle-verified against `typescript@7.0.2`
//! (`conformance/compiler/duplicateIdentifierDifferentModifiers.ts`).

use tsz_checker::test_utils::check_source_diagnostics;

fn ts2687_count(source: &str) -> usize {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2687)
        .count()
}

/// The exact conformance witness: two merged `interface B` declarations whose
/// `x` member disagrees only on the optional token.
#[test]
fn merged_interface_optional_disagreement_emits_ts2687() {
    let source = "interface B { x: any; }\ninterface B { x?: any; }\n";
    let diags = check_source_diagnostics(source);
    let hits: Vec<_> = diags.iter().filter(|d| d.code == 2687).collect();
    assert_eq!(
        hits.len(),
        2,
        "expected TS2687 on both merged declarations, got: {diags:?}"
    );
    assert!(
        hits.iter().all(|d| d.message_text.contains("'x'")),
        "TS2687 should name the disagreeing member 'x'; got {hits:?}"
    );
}

/// `readonly` disagreement across a merged interface pair is the same family.
#[test]
fn merged_interface_readonly_disagreement_emits_ts2687() {
    let source = "interface R { y: string; }\ninterface R { readonly y: string; }\n";
    assert_eq!(ts2687_count(source), 2);
}

/// Three-way merge: only the odd one out (and the reference) are flagged,
/// matching the single-interface-body convention already covered by
/// `report_property_modifier_disagreements`.
#[test]
fn merged_interface_three_way_only_disagreeing_pair_flagged() {
    let source =
        "interface M { z: number; }\ninterface M { z: number; }\ninterface M { z?: number; }\n";
    assert_eq!(ts2687_count(source), 2);
}

/// Renamed binder: the check must not key off any specific identifier.
#[test]
fn merged_interface_optional_disagreement_renamed_binder() {
    let source = "interface Zzyzx { w: any; }\ninterface Zzyzx { w?: any; }\n";
    assert_eq!(ts2687_count(source), 2);
}

/// Method signatures are exempt: `tsc` only runs this check from
/// `checkVariableLikeDeclaration`, which never visits method signatures — an
/// overload-shaped merge must stay silent on TS2687 regardless of any
/// (nonsensical, but syntactically legal) modifier-looking difference.
#[test]
fn merged_interface_method_signatures_are_not_flagged() {
    let source = "interface F { m(): void; }\ninterface F { m(x: number): void; }\n";
    assert_eq!(ts2687_count(source), 0);
}

/// Negative control: identical modifiers across a merged pair stay clean.
#[test]
fn merged_interface_matching_modifiers_stay_clean() {
    let source = "interface N { a: string; }\ninterface N { a: string; b: number; }\n";
    assert_eq!(ts2687_count(source), 0);
}

/// A single (non-merged) interface body already reports TS2687 through the
/// sibling `check_duplicate_interface_members` path; this locks in that the
/// new merged-declaration path does not double-report when everything lives
/// in one declaration.
#[test]
fn single_interface_body_disagreement_still_single_owner() {
    let source = "interface S { s: any; s?: any; }\n";
    // Duplicate-in-body is TS2300 territory (and its own TS2687 pairing) —
    // just confirm the merged-declaration path adds nothing extra beyond
    // what the single-body path already emits for a solitary declaration.
    let diags = check_source_diagnostics(source);
    let hits = diags.iter().filter(|d| d.code == 2687).count();
    assert_eq!(
        hits, 2,
        "single-body path should still emit its own pair: {diags:?}"
    );
}
