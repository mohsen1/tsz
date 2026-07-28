//! Parameter diagnostics anchor at the PARAMETER, not at its name.
//!
//! Structural rule: tsc's `getErrorSpanForNode` has no `SyntaxKind.Parameter`
//! case, so a parameter keeps its own span. For a plain parameter that span
//! starts at the name and the distinction is invisible; for a **parameter
//! property** it starts at the accessibility modifier, and tsc anchors there.
//!
//! Pins `conformance/classes/constructorDeclarations/constructorParameters/
//! constructorImplementationWithDefaultValues2.ts` (TS2322 at `public`) and
//! `compiler/varBlock.ts` (TS2371 at `public`).

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

#[test]
fn parameter_property_default_mismatch_anchors_at_the_modifier() {
    let source = "class C {\n    constructor(public x: string = 1) {}\n}\n";
    assert_eq!(
        starts_for(source, 2322),
        vec![offset_of(source, "public")],
        "TS2322 on a parameter property must anchor at `public`, not at the name"
    );
}

/// The modifier-less sibling: the parameter node starts at the name, so the
/// answer is unchanged. This is the case the old unconditional narrowing was
/// written for, and it must keep working.
#[test]
fn plain_parameter_default_mismatch_still_anchors_at_the_name() {
    let source = "class C {\n    constructor(x: string = 1) {}\n}\n";
    assert_eq!(
        starts_for(source, 2322),
        vec![offset_of(source, "x: string")],
        "TS2322 on a plain parameter must still anchor at the name"
    );
}

/// Renamed binders and a different modifier keyword, so the fix cannot be a
/// match on `public` or on a fixed column.
#[test]
fn parameter_property_anchor_survives_renamed_binders_and_other_modifiers() {
    for modifier in ["private", "protected", "readonly"] {
        let source = format!(
            "class Holder {{\n    constructor({modifier} someValue: string = 1) {{}}\n}}\n"
        );
        assert_eq!(
            starts_for(&source, 2322),
            vec![offset_of(&source, modifier)],
            "TS2322 must anchor at `{modifier}`"
        );
    }
}

/// TS2371 (`A parameter initializer is only allowed in a function or
/// constructor implementation`) takes the same rule.
#[test]
fn ambient_parameter_property_initializer_anchors_at_the_modifier() {
    let source = "declare class C {\n    constructor(public c = 10);\n}\n";
    assert_eq!(
        starts_for(source, 2371),
        vec![offset_of(source, "public")],
        "TS2371 on a parameter property must anchor at `public`"
    );
}

#[test]
fn ambient_plain_parameter_initializer_still_anchors_at_the_name() {
    let source = "declare function f(c = 10): void;\n";
    assert_eq!(
        starts_for(source, 2371),
        vec![offset_of(source, "c = 10")],
        "TS2371 on a plain parameter must still anchor at the name"
    );
}
