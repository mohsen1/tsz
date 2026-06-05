//! TS4105 ("Private or protected member '{0}' cannot be accessed on a type
//! parameter.") for indexed-access types over the polymorphic `this` type.
//!
//! tsc treats `this` as a type parameter whose constraint is the enclosing
//! class/interface, so `this["<nonpublic>"]` reports TS4105 just like
//! `T["<nonpublic>"]` for an explicit `T extends Base`. tsz previously only
//! recognized explicit type parameters here and silently accepted the `this`
//! form. These tests pin the `this`-type behavior in the positions the checker
//! visits (interfaces and declaration-merged interfaces), the explicit
//! type-parameter guard, and the negative cases (public members and concrete
//! class indices, which must NOT error). Binder names are varied so the
//! behavior cannot be satisfied by any name-specific shortcut.

use tsz_checker::context::CheckerOptions;

const TS4105: u32 = 4105;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_with_options(source, CheckerOptions::default())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn count(diags: &[(u32, String)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

/// `this["protected"]` on a declaration-merged interface reports TS4105.
#[test]
fn this_indexed_protected_member_reports_ts4105() {
    let diags = diagnostics(
        r#"
class Widget { protected value!: string; }
interface Widget { mirror(): this["value"]; }
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 for this[\"value\"] over a protected member; got {diags:?}"
    );
}

/// `this["private"]` reports TS4105 as well (private is also non-public).
#[test]
fn this_indexed_private_member_reports_ts4105() {
    let diags = diagnostics(
        r#"
class Gadget { private secret!: number; }
interface Gadget { peek(): this["secret"]; }
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 for this[\"secret\"] over a private member; got {diags:?}"
    );
}

/// Renamed binders must behave identically (no name-specific shortcut).
#[test]
fn this_indexed_nonpublic_member_renamed_binders_reports_ts4105() {
    let diags = diagnostics(
        r#"
class Receiver { protected hidden!: boolean; }
interface Receiver { reflect(): this["hidden"]; }
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 with renamed binders; got {diags:?}"
    );
}

/// A public member indexed through `this` must NOT report TS4105.
#[test]
fn this_indexed_public_member_no_ts4105() {
    let diags = diagnostics(
        r#"
class Surface { open!: string; }
interface Surface { echo(): this["open"]; }
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        0,
        "public member must not trigger TS4105; got {diags:?}"
    );
}

/// A concrete class index (`Base["protected"]`, not through a type parameter or
/// `this`) must NOT report TS4105 — tsc only reports it for type parameters.
#[test]
fn concrete_class_indexed_nonpublic_member_no_ts4105() {
    let diags = diagnostics(
        r#"
class Concrete { protected value!: string; }
type Probe = Concrete["value"];
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        0,
        "concrete class index must not trigger TS4105; got {diags:?}"
    );
}

/// Guard: the pre-existing explicit type-parameter path still reports TS4105.
#[test]
fn explicit_type_parameter_indexed_nonpublic_member_reports_ts4105() {
    let diags = diagnostics(
        r#"
class Holder { protected value!: string; }
type Pick<T extends Holder> = T["value"];
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "explicit type-parameter index must still report TS4105; got {diags:?}"
    );
}

/// A union of `this` and an explicit type parameter reports TS4105 once when a
/// non-public member is named on either constrained portion.
#[test]
fn union_this_and_type_parameter_indexed_nonpublic_member_reports_ts4105() {
    let diags = diagnostics(
        r#"
class Node2 { protected payload!: string; }
interface Node2 { combine<T extends Node2>(): (this | T)["payload"]; }
"#,
    );
    assert!(
        count(&diags, TS4105) >= 1,
        "expected TS4105 for (this | T)[\"payload\"]; got {diags:?}"
    );
}
