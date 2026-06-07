//! Regression coverage for the display of `keyof` type aliases whose operand is
//! a *grouped/composite* type, e.g. `type K = keyof ({ a: 1 } & { b: 2 })`.
//!
//! `keyof` of an anonymous composite has no writable `keyof Name` form, so `tsc`
//! renders the evaluated key set (`"a" | "b"`). The textual reconstruction of the
//! operand from source text previously stopped at the first `{` inside the
//! parentheses and emitted the malformed string `keyof (` in the assignment
//! diagnostic. The display path now renders the reduced literal key union, and a
//! *named* operand still keeps its `keyof Name` spelling.

use crate::test_utils::check_source_diagnostics;

fn target_displays(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text)
        .collect()
}

#[test]
fn keyof_alias_over_anonymous_object_intersection_renders_key_union() {
    let msgs = target_displays(
        r#"
type Keys = keyof ({ a: 1 } & { b: 2 });
declare let k: Keys;
k = "c";
"#,
    );
    assert_eq!(msgs.len(), 1, "expected exactly one TS2322; got: {msgs:?}");
    let msg = &msgs[0];
    assert!(
        !msg.contains("keyof ("),
        "must not emit the malformed `keyof (`; got: {msg}"
    );
    assert!(
        msg.contains("\"a\" | \"b\""),
        "keyof of an anonymous intersection must render its reduced key union; got: {msg}"
    );
}

#[test]
fn keyof_alias_over_anonymous_object_union_renders_key_set() {
    // `keyof (A | B)` reduces to the *common* keys; with disjoint anonymous
    // members that is `never`. The display must not leak a malformed `keyof (`.
    let msgs = target_displays(
        r#"
type Keys = keyof ({ a: 1 } | { b: 2 });
declare let k: Keys;
k = "z";
"#,
    );
    assert_eq!(msgs.len(), 1, "expected exactly one TS2322; got: {msgs:?}");
    assert!(
        !msgs[0].contains("keyof ("),
        "must not emit the malformed `keyof (`; got: {}",
        msgs[0]
    );
}

#[test]
fn keyof_alias_over_single_anonymous_object_renders_key_union() {
    let msgs = target_displays(
        r#"
type Keys = keyof { x: 1; y: 2 };
declare let k: Keys;
k = "z";
"#,
    );
    assert_eq!(msgs.len(), 1, "expected exactly one TS2322; got: {msgs:?}");
    assert!(
        msgs[0].contains("\"x\" | \"y\""),
        "keyof of an anonymous object alias renders its key union; got: {}",
        msgs[0]
    );
}

#[test]
fn keyof_alias_over_named_interface_keeps_keyof_name() {
    // Negative control: a *named* operand keeps `keyof Name` (tsc does not reduce
    // it in this position). Renamed binder to prove the rule follows structure,
    // not a specific identifier.
    let msgs = target_displays(
        r#"
interface Shape { a: 1; b: 2 }
type Keys = keyof Shape;
declare let k: Keys;
k = "c";
"#,
    );
    assert_eq!(msgs.len(), 1, "expected exactly one TS2322; got: {msgs:?}");
    assert!(
        msgs[0].contains("keyof Shape"),
        "a named-operand keyof keeps its `keyof Name` spelling; got: {}",
        msgs[0]
    );
}

#[test]
fn keyof_alias_composite_display_is_binder_name_independent() {
    // The reduction must depend on operand structure, not on the alias or
    // property identifiers chosen.
    let msgs = target_displays(
        r#"
type Selected = keyof ({ first: 1 } & { second: 2 });
declare let s: Selected;
s = "third";
"#,
    );
    assert_eq!(msgs.len(), 1, "expected exactly one TS2322; got: {msgs:?}");
    let msg = &msgs[0];
    assert!(
        !msg.contains("keyof (") && msg.contains("\"first\"") && msg.contains("\"second\""),
        "renamed binders must still reduce to the key union; got: {msg}"
    );
}
