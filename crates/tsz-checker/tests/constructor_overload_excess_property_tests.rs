//! Regression tests for overloaded-constructor excess-property false positives.
//!
//! Structural rule: when a `new` expression's target has more than one construct
//! signature (an overload set), a fresh object-literal argument that fits a *later*
//! overload must not be reported as an excess-property error (TS2353) against an
//! *earlier* overload's parameter type. Excess-property checking is deferred to
//! overload resolution, exactly as the call-expression path already does for
//! overloaded function calls. Only when *no* overload accepts the argument should a
//! diagnostic surface (TS2769 "No overload matches this call").
//!
//! See issue #10678 (kysely: false TS2353 excess property `executor` on
//! `KyselyConfig` — constructor-arg type resolved to the wrong overload).

use crate::test_utils::check_source_codes as get_error_codes;

/// Class constructor overloads where the literal fits the *second* overload by
/// supplying a property the first overload lacks. tsc accepts this.
#[test]
fn class_ctor_overload_literal_fits_later_overload_no_ts2353() {
    let source = r#"
interface ConfigA { a: number }
interface PropsB { a: number; executor: string }
declare class K {
    constructor(args: ConfigA);
    constructor(args: PropsB);
}
const k = new K({ a: 1, executor: "x" });
"#;
    let errors = get_error_codes(source);
    assert!(
        !errors.contains(&2353),
        "overloaded ctor: literal fitting a later overload must not emit TS2353, got: {errors:?}"
    );
    assert!(
        !errors.contains(&2769),
        "a matching overload exists; TS2769 must not fire, got: {errors:?}"
    );
}

/// Same rule, different property/type names — proves the fix is structural, not
/// keyed to any particular identifier spelling.
#[test]
fn class_ctor_overload_renamed_members_no_ts2353() {
    let source = r#"
interface Plain { id: number }
interface Extended { id: number; label: string }
declare class Widget {
    constructor(opts: Plain);
    constructor(opts: Extended);
}
const w = new Widget({ id: 7, label: "hi" });
"#;
    let errors = get_error_codes(source);
    assert!(
        !errors.contains(&2353) && !errors.contains(&2769),
        "renamed overloaded ctor must accept later-overload literal, got: {errors:?}"
    );
}

/// Overload order should not matter: the wider overload listed first still works.
#[test]
fn class_ctor_overload_wider_first_no_error() {
    let source = r#"
interface ConfigA { a: number }
interface PropsB { a: number; executor: string }
declare class K {
    constructor(args: PropsB);
    constructor(args: ConfigA);
}
const k = new K({ a: 1, executor: "x" });
"#;
    let errors = get_error_codes(source);
    assert!(
        !errors.contains(&2353) && !errors.contains(&2769),
        "overload order must not change acceptance, got: {errors:?}"
    );
}

/// Interface construct signatures (not a class) exercise the same `new` path.
#[test]
fn interface_construct_signature_overload_no_ts2353() {
    let source = r#"
interface Ctor {
    new (args: { a: number }): unknown;
    new (args: { a: number; executor: string }): unknown;
}
declare const C: Ctor;
const k = new C({ a: 1, executor: "x" });
"#;
    let errors = get_error_codes(source);
    assert!(
        !errors.contains(&2353) && !errors.contains(&2769),
        "interface construct-signature overload must accept later-overload literal, got: {errors:?}"
    );
}

/// Alias wrapping the constructor type must behave identically.
#[test]
fn aliased_constructor_overload_no_ts2353() {
    let source = r#"
interface ConfigA { a: number }
interface PropsB { a: number; executor: string }
interface KCtor {
    new (args: ConfigA): unknown;
    new (args: PropsB): unknown;
}
type KAlias = KCtor;
declare const K: KAlias;
const k = new K({ a: 1, executor: "x" });
"#;
    let errors = get_error_codes(source);
    assert!(
        !errors.contains(&2353) && !errors.contains(&2769),
        "aliased overloaded ctor must accept later-overload literal, got: {errors:?}"
    );
}

/// `super(...)` to an overloaded base constructor follows the same `new` resolution.
#[test]
fn super_call_to_overloaded_base_ctor_no_ts2353() {
    let source = r#"
class Base {
    constructor(a: { x: number });
    constructor(a: { x: number; y: number });
    constructor(a: any) { void a; }
}
class Sub extends Base {
    constructor() { super({ x: 1, y: 2 }); }
}
"#;
    let errors = get_error_codes(source);
    assert!(
        !errors.contains(&2353),
        "super() to overloaded base ctor must not emit TS2353, got: {errors:?}"
    );
}

/// Negative: a single (non-overloaded) constructor must STILL report excess
/// properties. The deferral only applies to genuine overload sets.
#[test]
fn single_constructor_still_reports_excess_ts2353() {
    let source = r#"
class K {
    constructor(a: { x: number }) { void a; }
}
const k = new K({ x: 1, y: 2 });
"#;
    let errors = get_error_codes(source);
    assert!(
        errors.contains(&2353),
        "single ctor with a fresh excess property must still emit TS2353, got: {errors:?}"
    );
}

/// Negative: when *no* overload accepts the literal (every candidate rejects an
/// excess property), tsc reports TS2769 — and tsz must not additionally emit a
/// spurious TS2353 against the first overload.
#[test]
fn no_matching_overload_reports_ts2769_only() {
    let source = r#"
declare class K {
    constructor(a: { x: number });
    constructor(a: { y: number });
}
const k = new K({ x: 1, y: 2, z: 3 });
"#;
    let errors = get_error_codes(source);
    assert!(
        errors.contains(&2769),
        "no overload accepts the literal; TS2769 must fire, got: {errors:?}"
    );
    assert!(
        !errors.contains(&2353),
        "the no-overload case must not also emit a spurious TS2353, got: {errors:?}"
    );
}

/// Negative: an overload mismatch driven by an incompatible property *type*
/// (not excess) still reports TS2769 when nothing matches.
#[test]
fn overload_type_mismatch_reports_ts2769() {
    let source = r#"
declare class K {
    constructor(a: { mode: "x" });
    constructor(a: { mode: "y" });
}
const k = new K({ mode: "z" });
"#;
    let errors = get_error_codes(source);
    assert!(
        errors.contains(&2769),
        "incompatible literal must report TS2769, got: {errors:?}"
    );
}
