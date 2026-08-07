//! Override-compatibility tests for accessor pairs in derived classes.
//!
//! Structural rule (matches tsc): when a class declares both a getter and a
//! setter for the same name + static-ness, the accessor pair has ONE property
//! type (the getter return type). TS2416/TS2417 override-compat runs once
//! against that canonical type rather than relating the setter parameter type
//! independently.
//!
//! Issue #9679: tsz was emitting false TS2416 when a derived setter parameter
//! type differed from the base property type even though the derived getter
//! return type matched the base. The setter's standalone compat check is
//! wrong: tsc's accessor-pair property type is the getter return type, and
//! that is what override-compat must relate against.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn ts2416_count(source: &str) -> usize {
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    diags.iter().filter(|d| d.code == 2416).count()
}

fn diags(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

// ---------------------------------------------------------------------------
// 1. Reported repro and adjacent shapes — false TS2416 must be silenced.
// ---------------------------------------------------------------------------

/// The reported repro from issue #9679. Getter matches base; setter differs.
/// Property type = getter return type = `number`, which matches base. tsc OK.
#[test]
fn accessor_pair_setter_differs_getter_matches_base() {
    let source = r#"
class Base { get prop(): number { return 1; } set prop(v: number) {} }
class Sub extends Base { get prop(): number { return 2; } set prop(v: string) {} }
"#;
    assert_eq!(
        ts2416_count(source),
        0,
        "false TS2416 — derived setter type should not be related independently against the base property type. Got: {:#?}",
        diags(source)
    );
}

/// Same rule, different member name. Property type = getter return = `number`,
/// matches base. The fix must apply structurally, not to a specific name.
#[test]
fn accessor_pair_renamed_members_no_ts2416() {
    let source = r#"
class A { get x(): number { return 1; } set x(v: number) {} }
class B extends A { get x(): number { return 2; } set x(v: boolean) {} }
"#;
    assert_eq!(ts2416_count(source), 0, "{:#?}", diags(source));
}

/// Source-order independence: setter declared before getter in derived. The
/// accessor pair's property type is still the getter return type.
#[test]
fn accessor_pair_setter_before_getter_no_ts2416() {
    let source = r#"
class A { get x(): number { return 1; } set x(v: number) {} }
class B extends A { set x(v: string) {} get x(): number { return 2; } }
"#;
    assert_eq!(ts2416_count(source), 0, "{:#?}", diags(source));
}

/// Source-order independence on the BASE side: even if the base declares its
/// setter first, the accessor pair's canonical property type is the getter
/// return type. Without canonicalization on the base side, `or_insert` in the
/// chain summary would store the setter parameter type as the base's property
/// type and produce a false TS2416 against the derived getter.
#[test]
fn accessor_pair_base_setter_before_getter_no_ts2416() {
    let source = r#"
class A { set x(v: number) {} get x(): number { return 1; } }
class B extends A { get x(): number { return 2; } set x(v: string) {} }
"#;
    assert_eq!(ts2416_count(source), 0, "{:#?}", diags(source));
}

/// Static accessor pairs follow the same rule (TS2417 path).
#[test]
fn static_accessor_pair_setter_differs_no_ts2416_or_ts2417() {
    let source = r#"
class A { static get x(): number { return 1; } static set x(v: number) {} }
class B extends A { static get x(): number { return 2; } static set x(v: string) {} }
"#;
    let codes: Vec<u32> = diags(source).iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2416) && !codes.contains(&2417),
        "unexpected override-compat diagnostic on static accessor pair: {:#?}",
        diags(source)
    );
}

/// Computed accessor names that resolve to a string literal late-bind to the
/// same name. The accessor-pair canonicalization must apply to the resolved
/// name, not the syntactic spelling.
#[test]
fn computed_accessor_pair_setter_differs_no_ts2416() {
    let source = r#"
const k = "key";
class A { get [k](): number { return 1; } set [k](v: number) {} }
class B extends A { get [k](): number { return 2; } set [k](v: string) {} }
"#;
    assert_eq!(ts2416_count(source), 0, "{:#?}", diags(source));
}

// ---------------------------------------------------------------------------
// 2. Negative controls — genuine type mismatches must still fire.
// ---------------------------------------------------------------------------

/// When the derived getter return type is genuinely incompatible with the
/// base property type, TS2416 must still fire. Exactly ONE diagnostic — the
/// setter's independent check used to add a duplicate.
#[test]
fn accessor_pair_getter_incompatible_emits_single_ts2416() {
    let source = r#"
class A { get x(): number { return 1; } set x(v: number) {} }
class B extends A { get x(): string { return ""; } set x(v: string) {} }
"#;
    assert_eq!(
        ts2416_count(source),
        1,
        "expected exactly one TS2416 (at the getter); got: {:#?}",
        diags(source)
    );
}

/// Setter-only derived override against a base accessor pair. With no derived
/// getter, the accessor's property type IS the setter parameter type, and a
/// genuine mismatch must fire TS2416.
#[test]
fn setter_only_derived_with_mismatched_type_emits_ts2416() {
    let source = r#"
class A { get x(): number { return 1; } set x(v: number) {} }
class B extends A { set x(v: string) {} }
"#;
    assert_eq!(ts2416_count(source), 1, "{:#?}", diags(source));
}

/// Both base and derived declare setter-only. No accessor pair on either
/// side; property type = setter param type. A mismatch must fire TS2416.
#[test]
fn setter_only_both_sides_with_mismatched_type_emits_ts2416() {
    let source = r#"
class A { set x(v: number) {} }
class B extends A { set x(v: string) {} }
"#;
    assert_eq!(ts2416_count(source), 1, "{:#?}", diags(source));
}

/// Derived getter-only that DOES match base property type. No diagnostic.
#[test]
fn getter_only_derived_matching_type_no_ts2416() {
    let source = r#"
class A { get x(): number { return 1; } set x(v: number) {} }
class B extends A { get x(): number { return 2; } }
"#;
    assert_eq!(ts2416_count(source), 0, "{:#?}", diags(source));
}

/// Accessor pair on both sides, all types compatible — no diagnostic.
#[test]
fn accessor_pair_all_types_compatible_no_ts2416() {
    let source = r#"
class A { get x(): number { return 1; } set x(v: number) {} }
class B extends A { get x(): number { return 2; } set x(v: number) {} }
"#;
    assert_eq!(ts2416_count(source), 0, "{:#?}", diags(source));
}

// ---------------------------------------------------------------------------
// 3. Property-vs-accessor override (TS2611) — still fires once for the pair.
// ---------------------------------------------------------------------------

/// Base has a plain property; derived overrides with an accessor pair. The
/// canonical accessor property type matches the base, but TS2611 still applies
/// (defined as property, overridden as accessor). Should fire ONCE for the
/// pair, not once per accessor node.
#[test]
fn accessor_pair_overrides_base_property_emits_ts2611_once() {
    let source = r#"
class A { x: number = 0; }
class B extends A { get x(): number { return 1; } set x(v: number) {} }
"#;
    let ts2611_count = diags(source).iter().filter(|d| d.code == 2611).count();
    assert_eq!(
        ts2611_count,
        1,
        "expected single TS2611; got: {:#?}",
        diags(source)
    );
}

// ---------------------------------------------------------------------------
// 4. Unannotated bodyless (abstract / ambient) getter has implicit type `any`,
//    not `void` — a derived override must not spuriously fail TS2416.
//    Conformance: compiler/noImplicitAnyMissingSetAccessor.ts. Every type is
//    assignable to an `any` base member, so the override is always compatible.
//    Binder names vary across cases so the fix cannot be pinned to a spelling.
// ---------------------------------------------------------------------------

/// Reported repro: abstract getter with no return annotation, derived getter
/// returns `string`. Base member type is implicit `any`, so no TS2416.
#[test]
fn abstract_unannotated_getter_is_any_derived_getter_no_ts2416() {
    let source = r#"
abstract class Parent { public abstract get message(); }
class Child extends Parent { public get message() { return ""; } }
"#;
    assert_eq!(
        ts2416_count(source),
        0,
        "an unannotated abstract getter is implicit `any`, not `void`; got: {:#?}",
        diags(source)
    );
}

/// Same rule when the derived member is a plain property rather than a getter.
#[test]
fn abstract_unannotated_getter_is_any_derived_property_no_ts2416() {
    let source = r#"
abstract class Shape { abstract get area(); }
class Square extends Shape { area = 4; }
"#;
    assert_eq!(
        ts2416_count(source),
        0,
        "derived property over an `any` base getter must not report TS2416; got: {:#?}",
        diags(source)
    );
}

/// The base member really is `any`: a derived getter returning an object type
/// is just as compatible as one returning a primitive.
#[test]
fn abstract_unannotated_getter_is_any_derived_object_no_ts2416() {
    let source = r#"
abstract class Repo { abstract get handle(); }
class FileRepo extends Repo { get handle() { return { fd: 1 }; } }
"#;
    assert_eq!(
        ts2416_count(source),
        0,
        "every type is assignable to an `any` base member; got: {:#?}",
        diags(source)
    );
}

/// An explicitly annotated abstract getter still drives a real TS2416 when the
/// override genuinely disagrees — the fix must not blanket-suppress the check.
#[test]
fn abstract_annotated_getter_mismatch_still_reports_ts2416() {
    let source = r#"
abstract class Provider { abstract get token(): string; }
class NumProvider extends Provider { get token() { return 1; } }
"#;
    assert_eq!(
        ts2416_count(source),
        1,
        "a real getter-type mismatch against an annotated base must still report TS2416; got: {:#?}",
        diags(source)
    );
}
