//! `super.member` accesses that do not resolve on the receiver side must reach
//! the ordinary nonexistent-property diagnostics, converging the property-access
//! path with the already-correct element-access path.
//!
//! Structural rule (matches `tsc`'s `reportNonexistentProperty`):
//! - instance receiver, name exists on the class *static* side -> TS2576 with
//!   the "did you mean to access the static member 'Base.name'" suggestion.
//! - static receiver (`super` in a static context, `typeof Base`), name exists
//!   on the *instance* side -> plain TS2339 against `typeof Base`.
//! - genuinely absent name -> plain TS2339 (negative control).
//!
//! Issue #17370; split out of the TS2340/TS2341/TS2855 half fixed in #17369.

use tsz_checker::test_utils::check_source_codes;

#[test]
fn super_instance_receiver_static_side_member_reports_ts2576() {
    let source = r#"
class Base { static s: number = 1; }
class Derived extends Base {
    m() { return super.s; }
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2576),
        "super.s (instance receiver, static-side member) should report TS2576; got {codes:?}",
    );
}

#[test]
fn super_instance_receiver_static_side_member_renamed_binders_reports_ts2576() {
    // The interface/class names must not matter.
    let source = r#"
class Vehicle { static wheels: number = 4; }
class Car extends Vehicle {
    describe() { return super.wheels; }
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2576),
        "renamed binders should still report TS2576; got {codes:?}",
    );
}

#[test]
fn super_static_receiver_instance_side_member_reports_ts2339() {
    let source = r#"
class Base { x: number = 1; }
class Derived extends Base {
    static m() { return super.x; }
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2339),
        "super.x (static receiver, instance-side member) should report TS2339; got {codes:?}",
    );
    assert!(
        !codes.contains(&2576),
        "static receiver should not get the static-member suggestion; got {codes:?}",
    );
}

#[test]
fn super_absent_member_still_reports_ts2339() {
    // Negative control: a member absent on both sides still reports TS2339.
    let source = r#"
class Base {}
class Derived extends Base {
    m() { return super.nope; }
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2339),
        "super.nope (absent everywhere) should report TS2339; got {codes:?}",
    );
}

#[test]
fn super_existing_instance_member_stays_clean() {
    // Negative control: a genuinely inherited member must not report anything.
    let source = r#"
class Base { greet(): string { return "hi"; } }
class Derived extends Base {
    m() { return super.greet(); }
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2339) && !codes.contains(&2576),
        "super.greet (present instance member) must stay clean; got {codes:?}",
    );
}
