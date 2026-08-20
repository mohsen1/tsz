//! Companion coverage to `union_display_alias_repaint_parity_tests.rs` for the
//! longhand-primitive-union repaint fix (issue #16610).
//!
//! The parity file pins the source-display matrix (a longhand primitive union
//! renders structurally; a written-through alias keeps its name). These rows pin
//! the three angles that file does not cover, all of which the narrow fix must
//! honor:
//!   * a parenthesized longhand primitive union still renders structurally;
//!   * a union mixing a *named* reference is not a longhand primitive union, so
//!     the reference member keeps its name;
//!   * a union with an object-literal member whose index value is a named alias
//!     keeps that nested alias name through the drill-in — the regression that
//!     rules out broadening the shared anonymous-composite predicate (which is
//!     depth-blind and would erase the nested `B`).

use crate::test_utils::{
    check_source_with_libs_code_messages, load_default_lib_files, strict_checker_options,
};

/// The single TS2322 message emitted for `source`, or a panic listing what was
/// actually produced. The default lib is loaded so `PropertyKey` (a lib alias)
/// resolves — its absence is exactly what makes the repaint observable.
fn only_ts2322(source: &str) -> String {
    let libs = load_default_lib_files();
    let diags =
        check_source_with_libs_code_messages(source, "test.ts", strict_checker_options(), &libs);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322; got: {diags:?}"
    );
    ts2322[0].1.clone()
}

/// A parenthesized longhand primitive union is still a longhand primitive union.
#[test]
fn parenthesized_longhand_primitive_union_renders_structurally() {
    let msg = only_ts2322(
        r#"
type Zed = string | number | symbol;
declare const v: (string | number | symbol);
const probe: boolean = v;
"#,
    );
    assert_eq!(
        msg, "Type 'string | number | symbol' is not assignable to type 'boolean'.",
        "a parenthesized longhand primitive union must render structurally; got: {msg:?}"
    );
}

/// A union mixing a named reference is not a longhand primitive union — the
/// reference member keeps its name, so the narrow predicate must not fire.
#[test]
fn mixed_union_with_named_reference_keeps_reference_name() {
    let msg = only_ts2322(
        r#"
interface Foo { a: number }
declare const v: Foo | string;
const probe: boolean = v;
"#,
    );
    assert!(
        msg.contains("Foo"),
        "a union containing a named reference must keep that reference's name; got: {msg:?}"
    );
}

/// Regression guard for the rejected broad approach: a union whose members are
/// an object-literal (with a named-referenced index value) plus a primitive must
/// keep the nested alias name `B` through the drill-in. A broad "anonymous
/// composite" classification would route this through the depth-blind structural
/// formatter and erase `B`.
#[test]
fn union_with_indexed_object_member_keeps_nested_alias_name() {
    let msg = only_ts2322(
        r#"
type B = { z: number };
const control: number | { [k: string]: B } = { z: null };
"#,
    );
    assert_eq!(
        msg, "Type 'null' is not assignable to type 'B'.",
        "the nested index-signature value alias `B` must survive the drill-in; got: {msg:?}"
    );
}
