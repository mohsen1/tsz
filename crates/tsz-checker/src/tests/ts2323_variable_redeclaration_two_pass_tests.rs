//! `tsc` reports variable redeclarations through two independent passes, so a
//! single declaration can carry TS2323 *and* TS2300 at once, and the two codes
//! can cover different subsets of the declaration list.
//!
//! Pass 1 is the binder's collision walk: each declaration is tested against
//! the *surviving* symbol, and a colliding declaration is reported (TS2451 when
//! the survivor is block-scoped, TS2300 otherwise) on the survivor's
//! declarations so far plus itself — then dropped onto a throwaway symbol, so
//! it never joins the survivor or widens its flags. Pass 2 is the
//! exported-variable check (TS2323), which reads the survivor alone.
//!
//! That is why `export var` / `export let` / `export var` reports TS2300 on
//! lines 1-2 but TS2323 on lines 1 and 3: the third `var` never collided with
//! the function-scoped survivor, so it joined it.
//!
//! Every expectation below is pinned against `tsc` 7.0.2 run with
//! `--noEmit --strict --pretty false --lib es2015 --target es2015`. Positions
//! matter, so these assert per-line code sets rather than whole-file code sets
//! — a code-set comparison cannot see co-emission at all.

use crate::test_utils::check_source_diagnostics;
use std::collections::BTreeMap;

/// Diagnostics grouped by 1-based source line, codes sorted and deduplicated.
fn codes_by_line(source: &str) -> Vec<(u32, Vec<u32>)> {
    let diags = check_source_diagnostics(source);
    let mut by_line: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for diag in &diags {
        let offset = (diag.start as usize).min(source.len());
        let line = source[..offset].matches('\n').count() as u32 + 1;
        by_line.entry(line).or_default().push(diag.code);
    }
    by_line
        .into_iter()
        .map(|(line, mut codes)| {
            codes.sort_unstable();
            codes.dedup();
            (line, codes)
        })
        .collect()
}

/// The headline row of #16165/#16166. The middle `let` collides with the
/// survivor (`var` on line 1) and earns TS2300 for both; the trailing `var`
/// does not collide, so it joins the survivor and the exported pass reports
/// TS2323 on lines 1 and 3. Neither code covers all three declarations.
#[test]
fn exported_var_let_var_co_emits_ts2300_and_ts2323_with_different_footprints() {
    assert_eq!(
        codes_by_line("export var alpha = 1;\nexport let alpha = 2;\nexport var alpha = 3;\n"),
        vec![(1, vec![2300, 2323]), (2, vec![2300]), (3, vec![2323])],
    );
}

/// Same shape under a different binder name — the rule is structural, not
/// keyed on any identifier.
#[test]
fn exported_var_let_var_is_name_independent() {
    assert_eq!(
        codes_by_line("export var zeta = 1;\nexport let zeta = 2;\nexport var zeta = 3;\n"),
        vec![(1, vec![2300, 2323]), (2, vec![2300]), (3, vec![2323])],
    );
}

/// Trailing block-scoped declaration: the survivor already holds both `var`s
/// when the `let` collides, so TS2300 covers all three and TS2323 covers the
/// first two.
#[test]
fn exported_var_var_let_co_emits_on_the_two_vars() {
    assert_eq!(
        codes_by_line("export var alpha = 1;\nexport var alpha = 2;\nexport let alpha = 3;\n"),
        vec![
            (1, vec![2300, 2323]),
            (2, vec![2300, 2323]),
            (3, vec![2300])
        ],
    );
}

/// `const` behaves exactly like `let` in the collision pass.
#[test]
fn exported_var_const_var_co_emits_like_the_let_row() {
    assert_eq!(
        codes_by_line("export var alpha = 1;\nexport const alpha = 2;\nexport var alpha = 3;\n"),
        vec![(1, vec![2300, 2323]), (2, vec![2300]), (3, vec![2323])],
    );
}

/// The row that rules out "TS2300 lands on every declaration": the rejoining
/// `var` on line 4 was never part of a collision, so it carries TS2323 only.
#[test]
fn exported_var_let_let_var_leaves_the_rejoining_var_without_ts2300() {
    assert_eq!(
        codes_by_line(
            "export var alpha = 1;\nexport let alpha = 2;\nexport let alpha = 3;\nexport var alpha = 4;\n"
        ),
        vec![
            (1, vec![2300, 2323]),
            (2, vec![2300]),
            (3, vec![2300]),
            (4, vec![2323]),
        ],
    );
}

/// Alternating four-declaration shape: both `var`s join the survivor, both
/// `let`s collide, and the two codes interleave.
#[test]
fn exported_var_let_var_let_interleaves_both_codes() {
    assert_eq!(
        codes_by_line(
            "export var alpha = 1;\nexport let alpha = 2;\nexport var alpha = 3;\nexport let alpha = 4;\n"
        ),
        vec![
            (1, vec![2300, 2323]),
            (2, vec![2300]),
            (3, vec![2300, 2323]),
            (4, vec![2300]),
        ],
    );
}

/// A lone surviving `var` is not a redeclaration of anything, so the exported
/// pass stays silent even though the file is full of `export var`.
#[test]
fn exported_var_let_let_reports_only_ts2300() {
    assert_eq!(
        codes_by_line("export var alpha = 1;\nexport let alpha = 2;\nexport let alpha = 3;\n"),
        vec![(1, vec![2300]), (2, vec![2300]), (3, vec![2300])],
    );
}

/// Block-scoped first: every later declaration collides with a block-scoped
/// survivor, so the whole group is TS2451 and the survivor never grows past
/// one declaration — no TS2323 anywhere, despite the exported `var`/`var` pair
/// on lines 2-3.
#[test]
fn exported_let_var_var_stays_ts2451_with_no_ts2323() {
    assert_eq!(
        codes_by_line("export let alpha = 1;\nexport var alpha = 2;\nexport var alpha = 3;\n"),
        vec![(1, vec![2451]), (2, vec![2451]), (3, vec![2451])],
    );
}

/// #16163's two-declaration rows must not move: one conflicting pair means
/// per-pass and per-symbol selection agree.
#[test]
fn two_declaration_rows_are_unchanged() {
    assert_eq!(
        codes_by_line("export var alpha = 1;\nexport var alpha = 2;\n"),
        vec![(1, vec![2323]), (2, vec![2323])],
    );
    assert_eq!(
        codes_by_line("export var alpha = 1;\nexport let alpha = 2;\n"),
        vec![(1, vec![2300]), (2, vec![2300])],
    );
    assert_eq!(
        codes_by_line("export let alpha = 1;\nexport var alpha = 2;\n"),
        vec![(1, vec![2451]), (2, vec![2451])],
    );
    assert_eq!(
        codes_by_line("export const alpha = 1;\nexport var alpha = 2;\n"),
        vec![(1, vec![2451]), (2, vec![2451])],
    );
}

/// Three exported `var`s never collide, so the binder pass is silent and only
/// the exported pass fires.
#[test]
fn three_exported_vars_are_ts2323_only() {
    assert_eq!(
        codes_by_line("export var alpha = 1;\nexport var alpha = 2;\nexport var alpha = 3;\n"),
        vec![(1, vec![2323]), (2, vec![2323]), (3, vec![2323])],
    );
}

/// Negative control, module scope: without `export` the exported pass cannot
/// fire, so the same var/let/var shape reports the binder pass only — and the
/// trailing `var` is clean.
#[test]
fn unexported_var_let_var_reports_the_binder_pass_only() {
    assert_eq!(
        codes_by_line("export {};\nvar alpha = 1;\nlet alpha = 2;\nvar alpha = 3;\n"),
        vec![(2, vec![2300]), (3, vec![2300])],
    );
}

/// Negative control: `var` is redeclarable, so a pure unexported `var`/`var`
/// group is legal and reports nothing at all.
#[test]
fn unexported_var_var_is_legal() {
    assert_eq!(
        codes_by_line("export {};\nvar alpha = 1;\nvar alpha = 2;\n"),
        Vec::<(u32, Vec<u32>)>::new(),
    );
}

/// Namespace-internal `export var` merges never reach the module's export
/// table, so the exported pass stays off (see #16158/#16161) while the binder
/// pass still reports the `let` collision.
#[test]
fn namespace_internal_var_let_var_reports_the_binder_pass_only() {
    assert_eq!(
        codes_by_line(
            "export {};\nnamespace Outer {\n  export var alpha = 1;\n  export let alpha = 2;\n  export var alpha = 3;\n}\n"
        ),
        vec![(3, vec![2300]), (4, vec![2300])],
    );
}

/// Function-scope control: the same collision walk runs inside a function
/// body, where no declaration can be exported.
#[test]
fn function_scope_var_let_var_reports_the_binder_pass_only() {
    assert_eq!(
        codes_by_line(
            "export {};\nfunction outer() {\n  var alpha = 1;\n  let alpha = 2;\n  var alpha = 3;\n}\n"
        ),
        vec![(3, vec![2300]), (4, vec![2300])],
    );
}

/// Fallback control: once a non-variable declaration joins the group the
/// family no longer applies and the general selection chain owns the report —
/// TS2300 across the group, with no TS2323 for the `var`/`var` pair.
#[test]
fn merged_function_declaration_leaves_the_variable_family() {
    assert_eq!(
        codes_by_line("export var alpha = 1;\nexport var alpha = 2;\nexport function alpha() {}\n"),
        vec![(1, vec![2300]), (2, vec![2300]), (3, vec![2300])],
    );
}
