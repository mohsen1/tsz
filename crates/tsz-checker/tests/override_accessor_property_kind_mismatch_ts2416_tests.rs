//! Issue #9720: a property<->accessor kind mismatch on an override
//! (TS2610/TS2611) must not suppress the override type-compatibility gate
//! (TS2416). When the overriding member's type is not assignable to the base
//! member's type, tsc emits BOTH the kind-mismatch diagnostic and a TS2416;
//! tsz previously emitted only the kind-mismatch error.
//!
//! Structural rule: when a derived class member overrides a base member with a
//! different kind (instance property vs. accessor) AND the overriding type is
//! not assignable to the base member type, emit TS2416 in addition to
//! TS2610/TS2611. The two checks are independent — neither suppresses the
//! other. The rule is structural and does not depend on member or class names.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn codes(source: &str) -> Vec<u32> {
    check_source(source, "test.ts", CheckerOptions::default())
        .iter()
        .map(|d| d.code)
        .collect()
}

/// Repro A: base accessor overridden by an incompatible derived property.
/// tsc: TS2610 (kind mismatch) + TS2416 (string not assignable to number).
#[test]
fn accessor_base_property_derived_incompatible_emits_both_2610_and_2416() {
    let source = r#"
class A { get x(): number { return 1; } set x(v: number) {} }
class B extends A { x: string = "s"; }
"#;
    let got = codes(source);
    assert!(
        got.contains(&2610),
        "Expected TS2610 kind-mismatch, got: {got:?}"
    );
    assert!(
        got.contains(&2416),
        "Expected TS2416 type-incompatibility alongside TS2610, got: {got:?}"
    );
}

/// Repro B: base property overridden by an incompatible derived accessor.
/// tsc: TS2611 (kind mismatch) + TS2416 (string not assignable to number),
/// reported at both the getter and setter positions.
#[test]
fn property_base_accessor_derived_incompatible_emits_both_2611_and_2416() {
    let source = r#"
class A { x: number = 1; }
class B extends A { get x(): string { return "s"; } set x(v: string) {} }
"#;
    let got = codes(source);
    assert!(
        got.contains(&2611),
        "Expected TS2611 kind-mismatch, got: {got:?}"
    );
    assert!(
        got.contains(&2416),
        "Expected TS2416 type-incompatibility alongside TS2611, got: {got:?}"
    );
}

/// Negative control: kind change with COMPATIBLE types must emit only the
/// kind-mismatch diagnostic, never TS2416. Confirms TS2416 stays gated on
/// genuine type incompatibility rather than firing on every kind change.
#[test]
fn accessor_property_kind_change_with_compatible_types_no_2416() {
    let source = r#"
class A { get x(): number { return 1; } set x(v: number) {} }
class B extends A { x: number = 2; }
"#;
    let got = codes(source);
    assert!(
        got.contains(&2610),
        "Expected TS2610 kind-mismatch, got: {got:?}"
    );
    assert!(
        !got.contains(&2416),
        "Expected NO TS2416 when override types are compatible, got: {got:?}"
    );
}

/// Mirror negative control for the property->accessor direction with
/// compatible types: only TS2611, no TS2416.
#[test]
fn property_accessor_kind_change_with_compatible_types_no_2416() {
    let source = r#"
class A { x: number = 1; }
class B extends A { get x(): number { return 2; } set x(v: number) {} }
"#;
    let got = codes(source);
    assert!(
        got.contains(&2611),
        "Expected TS2611 kind-mismatch, got: {got:?}"
    );
    assert!(
        !got.contains(&2416),
        "Expected NO TS2416 when override types are compatible, got: {got:?}"
    );
}

/// Anti-hardcoding: renamed members, classes, and unrelated types must behave
/// identically. The fix is structural, not keyed to the repro's spellings.
#[test]
fn renamed_members_and_types_still_emit_both_diagnostics() {
    let source = r#"
class Widget { get payload(): boolean { return true; } set payload(v: boolean) {} }
class Gadget extends Widget { payload: { kind: "x" } = { kind: "x" }; }
"#;
    let got = codes(source);
    assert!(
        got.contains(&2610),
        "Expected TS2610 under renamed members, got: {got:?}"
    );
    assert!(
        got.contains(&2416),
        "Expected TS2416 under renamed members, got: {got:?}"
    );
}

/// Getter-only derived accessor (no setter) overriding an incompatible base
/// property must still emit both TS2611 and TS2416.
#[test]
fn getter_only_accessor_over_incompatible_property_emits_both() {
    let source = r#"
class A { value: number = 0; }
class B extends A { get value(): string { return "s"; } }
"#;
    let got = codes(source);
    assert!(
        got.contains(&2611),
        "Expected TS2611 for getter-only accessor over property, got: {got:?}"
    );
    assert!(
        got.contains(&2416),
        "Expected TS2416 for getter-only accessor over incompatible property, got: {got:?}"
    );
}
