//! Diagnostic display for the static side of a declaration-merged
//! class + namespace symbol.
//!
//! When a class merges with a same-named namespace, the merged static value
//! carries the namespace exports as static properties. The merge rebuilt the
//! callable shape without its symbol, so the type printer expanded the
//! structural shape (`{ new (): C; prototype: C; staticFn: () => void; }`)
//! instead of rendering `typeof C`. tsc prints `typeof C` for both the merged
//! and unmerged cases.
//!
//! Structural rule: the rebuilt static callable keeps the class symbol, so the
//! printer's existing class-constructor branch (construct signatures + a
//! `CLASS`-flagged symbol) renders it as `typeof C` — independent of the class
//! identifier's spelling.

use tsz_checker::context::CheckerOptions;

fn ts2339_messages(source: &str) -> Vec<String> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .filter(|d| d.code == 2339)
        .map(|d| d.message_text)
        .collect()
}

fn assert_single_display(source: &str, expected_fragment: &str) {
    let msgs = ts2339_messages(source);
    assert_eq!(msgs.len(), 1, "expected exactly one TS2339, got: {msgs:?}");
    assert!(
        msgs[0].contains(expected_fragment),
        "expected message to contain `{expected_fragment}`, got: {}",
        msgs[0]
    );
}

#[test]
fn class_merged_with_namespace_displays_typeof() {
    // Reported repro: bogus static access on a class merged with a namespace.
    assert_single_display(
        r#"
class C { method() {} }
namespace C { export function staticFn() {} }
C.bogus();
"#,
        "typeof C",
    );
}

#[test]
fn class_merged_with_namespace_does_not_expand_static_shape() {
    let msgs = ts2339_messages(
        r#"
class C { method() {} }
namespace C { export function staticFn() {} }
C.bogus();
"#,
    );
    assert_eq!(msgs.len(), 1, "got: {msgs:?}");
    assert!(
        !msgs[0].contains("new ()") && !msgs[0].contains("staticFn"),
        "static shape must not be expanded, got: {}",
        msgs[0]
    );
}

#[test]
fn renamed_class_merged_with_namespace_displays_typeof() {
    // Structural, not keyed on the identifier `C`.
    assert_single_display(
        r#"
class Widget { render() {} }
namespace Widget { export const version = 1; }
Widget.missing();
"#,
        "typeof Widget",
    );
}

#[test]
fn generic_class_merged_with_namespace_displays_typeof() {
    assert_single_display(
        r#"
class Box<T> { value!: T; }
namespace Box { export const tag = "box"; }
Box.bogus();
"#,
        "typeof Box",
    );
}

#[test]
fn plain_class_static_access_still_displays_typeof() {
    // Control: an unmerged class already displays `typeof D`.
    assert_single_display(
        r#"
class D { method() {} }
D.foo();
"#,
        "typeof D",
    );
}

#[test]
fn plain_function_property_access_keeps_signature_display() {
    // Negative control: an unmerged function is NOT `typeof G`; tsc shows the
    // call signature. The namespace-merge display rule must not leak here.
    let msgs = ts2339_messages(
        r#"
function G() {}
G.bogus();
"#,
    );
    assert_eq!(msgs.len(), 1, "got: {msgs:?}");
    assert!(
        msgs[0].contains("() => void") && !msgs[0].contains("typeof"),
        "plain function should display as `() => void`, got: {}",
        msgs[0]
    );
}

#[test]
fn abstract_class_merged_with_namespace_still_reports_ts2511() {
    // Negative guard: preserving the merged symbol must not mask the
    // abstract-construction diagnostic. `new C()` on an abstract class merged
    // with a namespace still reports TS2511, matching tsc.
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    let codes: Vec<u32> = tsz_checker::test_utils::check_source(
        r#"
abstract class C { abstract m(): void; }
namespace C { export const x = 1; }
new C();
"#,
        "test.ts",
        options,
    )
    .into_iter()
    .map(|d| d.code)
    .collect();
    assert!(
        codes.contains(&2511),
        "expected TS2511 for `new C()` on abstract class merged with namespace, got: {codes:?}"
    );
}

#[test]
fn merged_namespace_static_member_access_succeeds() {
    // Using a real merged static member must not error; only the bogus access
    // on the instance should.
    let msgs = ts2339_messages(
        r#"
class C { method() {} }
namespace C { export const x = 1; }
C.x;
const c = new C();
c.bogus();
"#,
    );
    assert_eq!(
        msgs.len(),
        1,
        "expected only the instance error, got: {msgs:?}"
    );
    // The instance type is `C`, not `typeof C`.
    assert!(
        msgs[0].contains("type 'C'") && !msgs[0].contains("typeof"),
        "instance access should display as `C`, got: {}",
        msgs[0]
    );
}
