//! A JS "expando container" variable — a `var`/`let`/`const` initialized with
//! a function/arrow/class expression whose name later picks up `x.prop = ...`
//! expando assignments — owns its own expando writes even when it merges,
//! across files, with a same-named declaration of another (non-callable) type.
//!
//! Structural rule (oracle-verified against the pinned `typescript@7.0.2`):
//! when file A declares `x` as an expando container and a sibling file B
//! declares its own `var x` of another type, the two merge into one
//! script-global symbol whose single canonical `value_declaration` is the
//! first-*bound* file's. `tsc` nonetheless keeps A's own declaration
//! authoritative for A's uses: `x.prop = ...` inside A is a valid expando
//! declaration (no `TS2339`), regardless of which file is bound first. Only a
//! genuinely-absent member still reports `TS2339`.
//!
//! Companion to `js_expando_container_var_ts2403_exemption_tests` (the
//! `TS2403` half of the same fixture,
//! `TypeScript/tests/cases/conformance/salsa/jsContainerMergeTsDeclaration.ts`);
//! this file pins the `TS2339` half. Regression for the false positive
//! "Property 'a' does not exist on type 'number'" on `a.js`'s own
//! `x.a = ...`.

use tsz_checker::context::CheckerOptions;

/// Check `a.js` against `b.ts`, always reporting `a.js`'s own diagnostics.
/// `a_bound_first` controls binding order — the merged symbol's canonical
/// `value_declaration` is the first-bound file's, so binding `b.ts` first is
/// what exposes the file-order-sensitive contamination (`x` seen as the
/// foreign `number` at `a.js`'s own `x.a = ...`).
fn diags_checking_a_js(a_src: &str, b_src: &str, a_bound_first: bool) -> Vec<(u32, String)> {
    let files: Vec<(&str, &str)> = if a_bound_first {
        vec![("a.js", a_src), ("b.ts", b_src)]
    } else {
        vec![("b.ts", b_src), ("a.js", a_src)]
    };
    tsz_checker::test_utils::check_multi_file(
        &files,
        "a.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|d| d.code != 2318) // ignore lib-not-loaded noise
    .map(|d| (d.code, d.message_text))
    .collect()
}

/// Check a single `a.js` in isolation (no sibling).
fn diags_a_js_alone(a_src: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_multi_file(
        &[("a.js", a_src)],
        "a.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|d| d.code != 2318)
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn count_code(diags: &[(u32, String)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

/// The exact salsa fixture: `a.js`'s own `x.a = ...` expando write must not
/// report `TS2339`, in either binding order. The `.ts` sibling's `number`
/// value must not contaminate `a.js`'s own container type.
#[test]
fn expando_write_no_ts2339_in_either_binding_order() {
    for a_first in [true, false] {
        let diags = diags_checking_a_js(
            "var x = function foo() {}\nx.a = function bar() {}",
            "var x = function () { return 1; }();",
            a_first,
        );
        assert_eq!(
            count_code(&diags, 2339),
            0,
            "expando write must not report TS2339 (a_bound_first={a_first}); got: {diags:?}"
        );
        // The `TS2403` half must stay fixed too (belt and suspenders).
        assert_eq!(
            count_code(&diags, 2403),
            0,
            "expando container must not conflict by TS2403 (a_bound_first={a_first}); got: {diags:?}"
        );
    }
}

/// Anti-hardcoding: the rule is structural over names — repeat with
/// different identifiers and expando property names, in both binding orders.
#[test]
fn expando_write_no_ts2339_independent_of_identifier_choices() {
    for a_first in [true, false] {
        for var_name in ["widget", "thing"] {
            for expando in ["extra", "hook"] {
                let a_src = format!(
                    "var {var_name} = function foo() {{}}\n{var_name}.{expando} = function bar() {{}}"
                );
                let b_src = format!("var {var_name} = function () {{ return 1; }}();");
                let diags = diags_checking_a_js(&a_src, &b_src, a_first);
                assert_eq!(
                    count_code(&diags, 2339),
                    0,
                    "TS2339 must not fire for var '{var_name}' + expando '{expando}' \
                     (a_bound_first={a_first}); got: {diags:?}"
                );
            }
        }
    }
}

/// The container's initializer shape (function expression, arrow, or class
/// expression) and its declaration keyword (`var`/`let`/`const`) do not
/// change the exemption — it keys off the expando write, not the syntax.
#[test]
fn expando_write_no_ts2339_across_initializer_and_keyword_shapes() {
    let initializers = ["function foo() {}", "() => {}", "class {}"];
    for keyword in ["var", "let", "const"] {
        for init in initializers {
            let a_src = format!("{keyword} x = {init}\nx.a = function bar() {{}}");
            // Same-file only, so the block-scoped `let`/`const` redeclaration
            // dynamics of a cross-file `var x` do not confound the expando
            // signal we are pinning here.
            let diags = diags_a_js_alone(&a_src);
            assert_eq!(
                count_code(&diags, 2339),
                0,
                "TS2339 must not fire for '{keyword} x = {init}' expando; got: {diags:?}"
            );
        }
    }
}

/// The exemption is scoped to expando members that are actually written: a
/// genuinely-absent member on the same container must still report `TS2339`,
/// both alone and when merged with the `.ts` sibling in either binding order.
#[test]
fn genuinely_absent_member_still_reports_ts2339() {
    // Alone.
    let diags = diags_a_js_alone("var x = function foo() {}\nx.a = 1\nx.b");
    assert_eq!(
        count_code(&diags, 2339),
        1,
        "reading an unassigned member must still report TS2339 (alone); got: {diags:?}"
    );

    // Merged with a cross-file `.ts` `var x`: the assigned `x.a` write is
    // exempt, the unassigned `x.b` read is not — exactly one TS2339, in
    // either binding order.
    for a_first in [true, false] {
        let diags = diags_checking_a_js(
            "var x = function foo() {}\nx.a = 1\nx.b",
            "var x = function () { return 1; }();",
            a_first,
        );
        assert_eq!(
            count_code(&diags, 2339),
            1,
            "only the unassigned member must report TS2339 (a_bound_first={a_first}); got: {diags:?}"
        );
    }
}

/// A plain (non-expando) JS `var` with no useful local type must still defer
/// to a sibling `.ts`/`.d.ts` global — the exemption must not disable that
/// legitimate cross-file preference for non-container variables.
#[test]
fn plain_non_expando_js_var_still_defers_to_ts_global() {
    for a_first in [true, false] {
        // `y` is a plain number global from `b.ts`; `y.toFixed(2)` is a valid
        // method on it and must not report TS2339.
        let diags = diags_checking_a_js("y.toFixed(2)", "var y = 0;", a_first);
        assert_eq!(
            count_code(&diags, 2339),
            0,
            "plain non-expando JS var must resolve its .ts global's members \
             (a_bound_first={a_first}); got: {diags:?}"
        );
    }
}
