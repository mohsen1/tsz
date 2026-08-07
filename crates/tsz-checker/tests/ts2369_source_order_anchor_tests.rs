//! TS2369 ("A parameter property is only allowed in a constructor
//! implementation") anchors at whichever parameter-property modifier
//! (`public`/`private`/`protected`/`readonly`/`override`) appears **first in
//! source order**, not at a fixed kind priority.
//!
//! Structural rule: tsc's `checkGrammarModifiers` walks a parameter's
//! modifier list left to right and reports at the first modifier it visits.
//! `find_first_parameter_property_modifier`
//! (`crates/tsz-checker/src/checkers/parameter_checker.rs`) previously chose
//! by a fixed `public > private > protected > readonly > override` priority
//! via chained `.or_else()` lookups, so `readonly private x` anchored at
//! `private` (matching the priority order) instead of `readonly` (the modifier
//! that actually comes first in the source). Oracle-verified against
//! `typescript@7.0.2`.

use tsz_checker::test_utils::check_source_diagnostics;

fn starts_for(source: &str, code: u32) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == code)
        .map(|d| d.start)
        .collect()
}

fn offset_of(source: &str, needle: &str) -> u32 {
    u32::try_from(source.find(needle).expect("needle present")).expect("fits u32")
}

/// The modifier that comes first in source order wins, even when a
/// higher-priority-by-kind modifier (`private`) follows it.
#[test]
fn readonly_before_private_anchors_at_readonly() {
    let source = "function f(readonly private x: number) {}\n";
    assert_eq!(
        starts_for(source, 2369),
        vec![offset_of(source, "readonly")],
        "TS2369 must anchor at `readonly`, the first modifier in source order"
    );
}

/// The mirror ordering: when `private` genuinely comes first, it is still
/// the anchor — this is not a "readonly always wins" special case.
#[test]
fn private_before_readonly_anchors_at_private() {
    let source = "function f(private readonly x: number) {}\n";
    assert_eq!(
        starts_for(source, 2369),
        vec![offset_of(source, "private")],
        "TS2369 must anchor at `private`, the first modifier in source order"
    );
}

/// `override` sorts last in the old fixed-priority chain; confirm it wins
/// the anchor when it is written first.
#[test]
fn override_before_protected_anchors_at_override() {
    let source = "function f(override protected x: number) {}\n";
    assert_eq!(
        starts_for(source, 2369),
        vec![offset_of(source, "override")],
        "TS2369 must anchor at `override`, the first modifier in source order"
    );
}

#[test]
fn protected_before_override_anchors_at_protected() {
    let source = "function f(protected override x: number) {}\n";
    assert_eq!(
        starts_for(source, 2369),
        vec![offset_of(source, "protected")],
        "TS2369 must anchor at `protected`, the first modifier in source order"
    );
}

/// Renamed binder and a non-function-declaration host (object-literal
/// method) — the rule is structural, not keyed to `function f` or a
/// particular container.
#[test]
fn object_method_readonly_before_public_anchors_at_readonly() {
    let source = "const obj = {\n    m(readonly public someValue: number) {}\n};\n";
    assert_eq!(
        starts_for(source, 2369),
        vec![offset_of(source, "readonly")],
        "TS2369 on an object-literal method must anchor at `readonly`"
    );
}

/// Single-modifier case is unaffected by the ordering fix — a regression
/// guard for the common shape.
#[test]
fn single_modifier_still_anchors_at_itself() {
    let source = "function f(public x: number) {}\n";
    assert_eq!(starts_for(source, 2369), vec![offset_of(source, "public")],);
}

/// Inside an actual constructor implementation, parameter properties are
/// legal and TS2369 must not fire at all, regardless of modifier order.
#[test]
fn constructor_implementation_with_reordered_modifiers_is_clean() {
    let source = "class C {\n    constructor(readonly private x: number) {}\n}\n";
    assert!(
        starts_for(source, 2369).is_empty(),
        "TS2369 must not fire inside a constructor implementation"
    );
}
