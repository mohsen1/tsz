//! TS2339 receiver display for JS locals initialized with an object literal.
//!
//! Structural rule (oracle-verified against typescript@7.0.2, #17622): when a
//! TS2339 fires on a property of a JS local whose declaration initializer is an
//! object literal, tsc displays the receiver's structural widened shape
//! (`{ a: number; }`), for reads and writes alike — never `typeof o`.
//! TypeScript 7 reserves `typeof X` receiver displays for class, enum, and
//! namespace value sides.
//!
//! Before the fix, `property_receiver_display_for_node` carried a JS-only
//! carve-out that rendered `typeof <ident>` for exactly this shape.

use tsz_checker::context::CheckerOptions;

fn diagnostics_for_js(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(
        source,
        "test.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn assert_single_ts2339(source: &str, expected_message: &str) {
    let diags = diagnostics_for_js(source);
    let ts2339: Vec<&(u32, String)> = diags.iter().filter(|(code, _)| *code == 2339).collect();
    assert!(
        ts2339.len() == 1 && ts2339[0].1 == expected_message,
        "expected exactly one TS2339 `{expected_message}`; got: {diags:?}"
    );
}

/// Read access on a `const` object-literal local displays the widened
/// structural shape, not `typeof store`.
#[test]
fn read_on_const_object_literal_local_displays_structural_shape() {
    assert_single_ts2339(
        "const store = { count: 2 };\nstore.total;\n",
        "Property 'total' does not exist on type '{ count: number; }'.",
    );
}

/// Write access (plain RHS) displays the structural shape, not `typeof box`.
#[test]
fn write_on_const_object_literal_local_displays_structural_shape() {
    assert_single_ts2339(
        "const box = { v: 1 };\nbox.n = 5;\n",
        "Property 'n' does not exist on type '{ v: number; }'.",
    );
}

/// Write access with a named function-expression RHS (the #17622 witness)
/// displays the structural shape, not `typeof o`.
#[test]
fn function_valued_write_on_object_literal_local_displays_structural_shape() {
    assert_single_ts2339(
        "const o = { a: 1 };\no.m = function C() {};\n",
        "Property 'm' does not exist on type '{ a: number; }'.",
    );
}

/// `let` binding with renamed binders behaves identically to `const`.
#[test]
fn read_on_let_object_literal_local_displays_structural_shape() {
    assert_single_ts2339(
        "let cfg = { on: true };\ncfg.off;\n",
        "Property 'off' does not exist on type '{ on: boolean; }'.",
    );
}

/// A nested object-literal member receiver also displays structurally.
#[test]
fn nested_object_literal_member_receiver_displays_structural_shape() {
    assert_single_ts2339(
        "const wrap = { inner: { a: 1 } };\nwrap.inner.b;\n",
        "Property 'b' does not exist on type '{ a: number; }'.",
    );
}

/// Multi-property shapes keep declaration order in the display.
#[test]
fn multi_property_object_literal_local_keeps_declaration_order() {
    assert_single_ts2339(
        "var mix = { n: 1, s: \"x\" };\nmix.q = function named() {};\n",
        "Property 'q' does not exist on type '{ n: number; s: string; }'.",
    );
}

/// Negative control: a JS class's static side still displays as `typeof K` —
/// the fix removes only the object-literal-local carve-out.
#[test]
fn js_class_static_receiver_still_displays_typeof() {
    assert_single_ts2339(
        "class Widget { }\nWidget.absent;\n",
        "Property 'absent' does not exist on type 'typeof Widget'.",
    );
}
