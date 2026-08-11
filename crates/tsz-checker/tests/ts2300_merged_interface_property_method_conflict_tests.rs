//! TS2300 for merged interface declarations whose same-named member disagrees
//! on signature *kind* (a property signature in one declaration, a method
//! signature in another).
//!
//! Structural rule: when two or more top-level `interface` declarations with
//! the same name merge into one symbol, `tsc`'s binder cannot merge a
//! property-signature symbol (`SymbolFlags::Property`) with a method-signature
//! symbol (`SymbolFlags::Method`) under the same name — that flag combination
//! is not mergeable, so it reports TS2300 ("Duplicate identifier") on every
//! conflicting declaration, regardless of which kind was declared first. This
//! is distinct from a same-kind property/property mismatch, which is a type
//! comparison (TS2717 "Subsequent property declarations must have the same
//! type"). `check_merged_interface_declaration_diagnostics`
//! (`duplicate_identifiers_followup.rs`) previously only recognized the
//! method-after-property direction as TS2300 and mis-routed the reverse
//! (property-after-method) through the TS2717 type-comparison branch — the two
//! branches must be symmetric. Oracle-verified against `typescript@7.0.2`
//! (`compiler/methodSignatureHandledDeclarationKindForSymbol.ts`).

use tsz_checker::test_utils::check_source_diagnostics;

fn ts2300_count(source: &str) -> usize {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2300)
        .count()
}

fn ts2717_count(source: &str) -> usize {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2717)
        .count()
}

/// The exact conformance witness: method signature declared first, property
/// signature second.
#[test]
fn merged_interface_method_then_property_emits_ts2300() {
    let source =
        "interface Foo {\n    bold(): string;\n}\n\ninterface Foo {\n    bold: string;\n}\n";
    let diags = check_source_diagnostics(source);
    let hits: Vec<_> = diags.iter().filter(|d| d.code == 2300).collect();
    assert_eq!(
        hits.len(),
        2,
        "expected TS2300 on both merged declarations, got: {diags:?}"
    );
    assert!(
        hits.iter().all(|d| d.message_text.contains("'bold'")),
        "TS2300 should name the disagreeing member 'bold'; got {hits:?}"
    );
    assert_eq!(
        ts2717_count(source),
        0,
        "a kind mismatch must not also emit TS2717: {diags:?}"
    );
}

/// The reverse direction: property signature declared first, method second.
/// This is the case the pre-fix code mis-routed to TS2717.
#[test]
fn merged_interface_property_then_method_emits_ts2300() {
    let source =
        "interface Bar {\n    bold: string;\n}\n\ninterface Bar {\n    bold(): string;\n}\n";
    assert_eq!(ts2300_count(source), 2);
    assert_eq!(ts2717_count(source), 0);
}

/// Renamed binder: the check must not key off any specific identifier.
#[test]
fn merged_interface_property_then_method_renamed_binder() {
    let source = "interface Zzyzx {\n    w: string;\n}\n\ninterface Zzyzx {\n    w(): string;\n}\n";
    assert_eq!(ts2300_count(source), 2);
}

/// Negative control: two method signatures with the same name across a merge
/// are valid overloads, not a duplicate identifier.
#[test]
fn merged_interface_method_overloads_stay_clean() {
    let source = "interface F {\n    m(): void;\n}\ninterface F {\n    m(x: number): void;\n}\n";
    assert_eq!(ts2300_count(source), 0);
}

/// Negative control: two property signatures with identical types across a
/// merge are valid, not a duplicate identifier.
#[test]
fn merged_interface_matching_properties_stay_clean() {
    let source = "interface N {\n    a: string;\n}\ninterface N {\n    a: string;\n}\n";
    assert_eq!(ts2300_count(source), 0);
    assert_eq!(ts2717_count(source), 0);
}

/// Same-kind property/property mismatch is unaffected by this fix — it stays
/// on the TS2717 type-comparison path, not TS2300.
#[test]
fn merged_interface_property_type_mismatch_stays_ts2717() {
    let source = "interface P {\n    a: string;\n}\ninterface P {\n    a: number;\n}\n";
    assert_eq!(ts2717_count(source), 1);
    assert_eq!(ts2300_count(source), 0);
}
