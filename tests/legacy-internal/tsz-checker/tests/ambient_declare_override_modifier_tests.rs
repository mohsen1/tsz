//! `declare`/`override` interaction on a class member (issue #16291 follow-up
//! named on #16838's 2026-08-08T00:19:45Z comment): tsz over-reported
//! multiple diagnostics for `declare override`/`override declare` combos
//! instead of tsc's single one.
//!
//! tsc's `checkGrammarModifiers` walks a member's modifiers in SOURCE ORDER
//! and reports exactly one diagnostic for the pair, so which code wins
//! depends on order and member kind:
//! - `override` before `declare`: the ambient conflict (TS1040) is known as
//!   soon as `declare` is reached, regardless of member kind — even a method,
//!   accessor, or constructor reports TS1040 alone (not TS1031/TS1089).
//! - `declare` before `override`: `declare` is checked against the member
//!   kind immediately. On a method/accessor/constructor (which never allow
//!   `declare`) that is TS1031 and the walk stops there, before `override` is
//!   even reached. On a property (the one member kind `declare` is legal on),
//!   the walk continues and reports TS1243 ("'override' modifier cannot be
//!   used with 'declare' modifier") at `override` instead.
//!
//! Either way, once one of these grammar diagnostics fires for the pair, tsc
//! never reaches the semantic override-compatibility checks (TS4112/TS4113)
//! or the constructor-specific TS1089 for that member — this suite pins that
//! suppression too, both with and without a base class in scope.
//!
//! Every expectation is pinned against `typescript@7.0.2`
//! (`--noEmit --strict --target es2022 --lib es2022`). Binder names are varied
//! so the diagnostic is keyed on the modifier shape, not any identifier.

// TS1031/TS1040/TS1243 are parser-emitted; TS4112/TS4113/TS1089 are
// checker-emitted. Only `check_source_codes_with_parse_health` sees both
// sides — the plain `check_source_diagnostics`/`check_source_codes` helpers
// never wire parser diagnostics into the result at all.
use crate::test_utils::check_source_codes_with_parse_health;

const TS1031: u32 = 1031; // 'declare' modifier cannot appear on class elements of this kind.
const TS1040: u32 = 1040; // 'override' modifier cannot be used in an ambient context.
const TS1089: u32 = 1089; // 'override' modifier cannot appear on a constructor declaration.
const TS1243: u32 = 1243; // 'override' modifier cannot be used with 'declare' modifier.
const TS4112: u32 = 4112; // cannot have override — containing class does not extend another class.
const TS4113: u32 = 4113; // cannot have override — not declared in the base class.

/// Grammar/override codes this suite is about, filtered so assertions stay
/// immune to unrelated harness noise (the unit harness has no lib).
const RELEVANT_CODES: [u32; 6] = [TS1031, TS1040, TS1089, TS1243, TS4112, TS4113];

fn codes(source: &str) -> Vec<u32> {
    let mut v: Vec<u32> = check_source_codes_with_parse_health(source)
        .into_iter()
        .filter(|c| RELEVANT_CODES.contains(c))
        .collect();
    v.sort_unstable();
    v
}

// --- `override` before `declare`, any member kind: TS1040 alone ------------

#[test]
fn override_declare_property_reports_ts1040_alone() {
    for name in ["p", "value", "data", "field"] {
        let source = format!("class C {{ override declare {name}: number; }}");
        assert_eq!(codes(&source), vec![TS1040], "source: {source}");
    }
}

#[test]
fn override_declare_method_reports_ts1040_alone_not_ts1031() {
    assert_eq!(
        codes("class C { override declare m(): void; }"),
        vec![TS1040]
    );
}

#[test]
fn override_declare_accessor_reports_ts1040_alone_not_ts1031() {
    assert_eq!(
        codes("class C { override declare get x(): number; }"),
        vec![TS1040]
    );
    assert_eq!(
        codes("class C { override declare set x(v: number); }"),
        vec![TS1040]
    );
}

#[test]
fn override_declare_constructor_reports_ts1040_alone_not_ts1089() {
    assert_eq!(
        codes("class C { override declare constructor(); }"),
        vec![TS1040]
    );
}

#[test]
fn override_declare_reports_ts1040_independent_of_class_name() {
    for name in ["C", "Widget", "Repository", "Zzz"] {
        let source = format!("class {name} {{ override declare m(): void; }}");
        assert_eq!(codes(&source), vec![TS1040], "source: {source}");
    }
}

// --- `declare` before `override`, on a method/accessor/constructor: TS1031 alone

#[test]
fn declare_override_method_reports_ts1031_alone() {
    assert_eq!(
        codes("class C { declare override m(): void; }"),
        vec![TS1031]
    );
}

#[test]
fn declare_override_accessor_reports_ts1031_alone() {
    assert_eq!(
        codes("class C { declare override get x(): number; }"),
        vec![TS1031]
    );
    assert_eq!(
        codes("class C { declare override set x(v: number); }"),
        vec![TS1031]
    );
}

#[test]
fn declare_override_constructor_reports_ts1031_alone_not_ts1089() {
    assert_eq!(
        codes("class C { declare override constructor(); }"),
        vec![TS1031]
    );
}

#[test]
fn declare_override_method_reports_ts1031_independent_of_class_name() {
    for name in ["C", "Widget", "Repository", "Zzz"] {
        let source = format!("class {name} {{ declare override m(): void; }}");
        assert_eq!(codes(&source), vec![TS1031], "source: {source}");
    }
}

// --- `declare` before `override`, on a property: TS1243 alone --------------

#[test]
fn declare_override_property_reports_ts1243_not_ts1040() {
    for name in ["p", "value", "data", "field"] {
        let source = format!("class C {{ declare override {name}: number; }}");
        assert_eq!(codes(&source), vec![TS1243], "source: {source}");
    }
}

// --- suppression holds with a real base class in scope, both orders --------

#[test]
fn override_declare_method_suppresses_ts4112_with_extends() {
    let source =
        "class Base { m(): void {} } declare class D extends Base { override declare m(): void; }";
    assert_eq!(codes(source), vec![TS1040], "source: {source}");
}

#[test]
fn override_declare_method_suppresses_ts4113_when_base_lacks_member() {
    // `Base` has no `m` at all — would be TS4113 without the suppression.
    let source = "class Base { other(): void {} } declare class D extends Base { override declare m(): void; }";
    assert_eq!(codes(source), vec![TS1040], "source: {source}");
}

#[test]
fn declare_override_method_suppresses_ts4112_no_extends() {
    let source = "declare class C { declare override m(): void; }";
    assert_eq!(codes(source), vec![TS1031], "source: {source}");
}

// --- negative controls: unrelated grammar errors do not suppress TS4112 ----

#[test]
fn duplicate_accessibility_does_not_suppress_ts4112() {
    // A grammar error unrelated to declare/override (duplicate accessibility)
    // must not suppress the override-compatibility check.
    assert_eq!(
        codes("class C { public public override m(): void {} }"),
        vec![TS4112]
    );
}

// --- negative controls: plain `override`/`declare` alone are unaffected ----

#[test]
fn plain_override_no_extends_still_reports_ts4112() {
    assert_eq!(
        codes("declare class C { override m(): void; }"),
        vec![TS4112]
    );
}

#[test]
fn plain_declare_method_still_reports_ts1031() {
    assert_eq!(
        codes("declare class C { declare m(): void; }"),
        vec![TS1031]
    );
}

#[test]
fn plain_override_constructor_still_reports_ts1089() {
    assert_eq!(
        codes("class Base {} class C extends Base { override constructor() {} }"),
        vec![TS1089]
    );
}

#[test]
fn plain_override_with_matching_base_member_is_clean() {
    assert_eq!(
        codes("class Base { m(): void {} } class D extends Base { override m(): void {} }"),
        Vec::<u32>::new()
    );
}
