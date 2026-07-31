//! Regression tests for object-literal synthetic-`this` member display order.
//!
//! When an object-literal method captures the surrounding literal as `this`
//! (`noImplicitThis`), tsc renders that `this` type with its members in source
//! order (`{ prop1; prop2; prop3; test(): void; accept_foo(...): boolean; }`).
//! The synthetic `this` type is assembled from three sources — the
//! incrementally-built properties, the members spliced in from *after* the
//! current method, and the pre-scanned method signatures — which must all draw
//! their display-order slots from the same range so the assembled type sorts in
//! source order (see `object_literal_circularity` /
//! `LITERAL_DISPLAY_ORDER_BASE`).
//!
//! Also guards the property-table dedup for repeated direct keys
//! (`{ a: 1, b: 2, a: 3 }` renders `{ a: number; b: number; }`).

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::check_with_options_code_messages;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(
        source,
        CheckerOptions {
            no_implicit_this: true,
            ..CheckerOptions::default()
        },
    )
}

/// Return the first diagnostic message that mentions every `needle`, or panic
/// with the full diagnostic set for debugging.
fn message_mentioning(diags: &[(u32, String)], needles: &[&str]) -> String {
    diags
        .iter()
        .find(|(_, msg)| needles.iter().all(|n| msg.contains(n)))
        .map(|(_, msg)| msg.clone())
        .unwrap_or_else(|| panic!("no diagnostic mentioned {needles:?}; got: {diags:?}"))
}

/// Assert the `names` appear in the given order somewhere in `msg`.
fn assert_member_order(msg: &str, names: &[&str]) {
    let mut last = 0usize;
    for name in names {
        let at = msg[last..]
            .find(name)
            .map(|off| last + off)
            .unwrap_or_else(|| panic!("member {name:?} not found after index {last} in {msg:?}"));
        assert!(
            at >= last,
            "member {name:?} out of order (at {at}, expected >= {last}) in {msg:?}",
        );
        last = at + name.len();
    }
}

#[test]
fn this_capturing_method_keeps_source_member_order() {
    // The original objectLiteralThisWidenedOnUse.ts witness: data properties
    // declared before the methods must render before them in the synthetic
    // `this` type that flows into `this.accept_foo(this)`.
    let source = r#"
interface Foo { bar: boolean; }
var GlobalIns = {
  prop1: 1,
  prop2: 2,
  prop3: 3,
  test () {
    this.accept_foo(this);
  },
  accept_foo (foo: Foo): boolean {
    return !!foo && !!foo.bar;
  }
};
"#;
    let diags = get_diagnostics(source);
    let msg = message_mentioning(&diags, &["prop1", "accept_foo"]);
    assert_member_order(&msg, &["prop1", "prop2", "prop3", "test", "accept_foo"]);
}

#[test]
fn this_capturing_method_source_order_with_renamed_binders() {
    // Same structure, different identifiers: the ordering must follow source
    // position, never any name-based sort.
    let source = r#"
interface Shape { edge: boolean; }
var widget = {
  alpha: 1,
  beta: 2,
  gamma: 3,
  run () {
    this.take(this);
  },
  take (shape: Shape): boolean {
    return !!shape && !!shape.edge;
  }
};
"#;
    let diags = get_diagnostics(source);
    let msg = message_mentioning(&diags, &["alpha", "take"]);
    assert_member_order(&msg, &["alpha", "beta", "gamma", "run", "take"]);
}

#[test]
fn this_capturing_method_between_data_members_keeps_order() {
    // A data member declared *after* the this-capturing method is spliced into
    // the synthetic `this` type and must sort after the method, matching source.
    let source = r#"
interface Foo { bar: boolean; }
var obj = {
  head: 1,
  probe () {
    this.tail;
    this.accept(this);
  },
  tail: 2,
  accept (foo: Foo): boolean {
    return !!foo && !!foo.bar;
  }
};
"#;
    let diags = get_diagnostics(source);
    let msg = message_mentioning(&diags, &["head", "accept"]);
    assert_member_order(&msg, &["head", "probe", "tail", "accept"]);
}

#[test]
fn duplicate_direct_key_renders_once_first_slot() {
    // Property-table semantics: a repeated direct key keeps the first slot with
    // the last value, and renders exactly once. Distinctive identifiers avoid
    // colliding with ordinary diagnostic prose.
    let source = r#"
declare const n: number;
const e: string = { zprop: n, wprop: n, zprop: n };
"#;
    let diags = get_diagnostics(source);
    let msg = message_mentioning(&diags, &["zprop", "wprop"]);
    assert_eq!(
        msg.matches("zprop").count(),
        1,
        "duplicate direct key should render once, got: {msg:?}",
    );
    assert_member_order(&msg, &["zprop", "wprop"]);
}

#[test]
fn duplicate_direct_key_last_slot_dedup() {
    // Mirror case: the repeated key is `wprop`, declared first then last; it
    // keeps its first position and renders once.
    let source = r#"
declare const n: number;
const e: string = { wprop: n, zprop: n, wprop: n };
"#;
    let diags = get_diagnostics(source);
    let msg = message_mentioning(&diags, &["zprop", "wprop"]);
    assert_eq!(
        msg.matches("wprop").count(),
        1,
        "duplicate direct key should render once, got: {msg:?}",
    );
    assert_member_order(&msg, &["wprop", "zprop"]);
}

// =============================================================================
// Duplicate direct keys in the *annotated-initializer* source display.
//
// `object_literal_source_type_display` renders the source of a TS2322/TS2345
// from the object-literal syntax rather than from the finalized literal type,
// and used to emit one member per element — so a repeated key rendered twice.
// tsc's property table keeps the FIRST position and the LAST value.
//
// Every expectation below is pinned against `tsc` 7.0.2
// (`--noEmit --strict --pretty false --lib es2015`).
// =============================================================================

/// Exact-message control: a repeated key with a *different* value type each
/// time. tsc prints `{ zprop: boolean; wprop: string; }` — first position,
/// last value. This is the case that proves the rendering is not merely
/// deduped but takes the correct occurrence's type.
#[test]
fn duplicate_direct_key_keeps_first_position_and_last_value() {
    let diags = get_diagnostics(
        r#"
const e: string = { zprop: 1, wprop: "s", zprop: true };
"#,
    );
    let msg = message_mentioning(&diags, &["zprop", "wprop"]);
    assert!(
        msg.contains("{ zprop: boolean; wprop: string; }"),
        "expected tsc's first-position/last-value rendering, got: {msg:?}",
    );
}

/// The same key three times collapses to one member. Guards against a fix that
/// only dedupes an adjacent pair.
#[test]
fn triple_duplicate_direct_key_renders_once() {
    let diags = get_diagnostics(
        r"
declare const n: number;
const e: string = { zprop: n, wprop: n, zprop: n, zprop: n };
",
    );
    let msg = message_mentioning(&diags, &["zprop", "wprop"]);
    assert_eq!(
        msg.matches("zprop").count(),
        1,
        "a key repeated three times must still render once, got: {msg:?}",
    );
    assert_member_order(&msg, &["zprop", "wprop"]);
}

/// Shorthand members participate in the same property table. Binder names
/// differ from the property-assignment cases so nothing keys on identifier text.
#[test]
fn duplicate_shorthand_key_renders_once() {
    let diags = get_diagnostics(
        r"
const alpha = 1, beta = 2;
const e: string = { alpha, beta, alpha };
",
    );
    let msg = message_mentioning(&diags, &["alpha", "beta"]);
    assert_eq!(
        msg.matches("alpha").count(),
        1,
        "duplicate shorthand key must render once, got: {msg:?}",
    );
    assert_member_order(&msg, &["alpha", "beta"]);
}

/// A quoted key and a bare key naming the same property are one table entry.
#[test]
fn quoted_and_bare_same_key_render_once() {
    let diags = get_diagnostics(
        r#"
declare const n: number;
const e: string = { "kprop": n, kprop: n };
"#,
    );
    let msg = message_mentioning(&diags, &["kprop"]);
    assert_eq!(
        msg.matches("kprop").count(),
        1,
        "a quoted and a bare spelling of one key must render once, got: {msg:?}",
    );
}

/// The dedup applies to a nested literal reached through the recursive arm of
/// the display builder, not only to the outermost literal.
#[test]
fn duplicate_key_inside_nested_literal_renders_once() {
    let diags = get_diagnostics(
        r"
declare const n: number;
const e: string = { outer: { zprop: n, wprop: n, zprop: n } };
",
    );
    let msg = message_mentioning(&diags, &["outer", "zprop"]);
    assert_eq!(
        msg.matches("zprop").count(),
        1,
        "duplicate key in a nested literal must render once, got: {msg:?}",
    );
    assert_member_order(&msg, &["outer", "zprop", "wprop"]);
}

/// Negative control: a literal with no repeated key must be untouched by the
/// dedup — same member count, same source order, no collapsing of distinct
/// names that merely share a prefix.
#[test]
fn distinct_keys_including_shared_prefix_all_render() {
    let diags = get_diagnostics(
        r"
declare const n: number;
const e: string = { pre: n, prefix: n, prefixed: n };
",
    );
    let msg = message_mentioning(&diags, &["pre", "prefix"]);
    assert!(
        msg.contains("{ pre: number; prefix: number; prefixed: number; }"),
        "distinct keys must all render in source order, got: {msg:?}",
    );
}
