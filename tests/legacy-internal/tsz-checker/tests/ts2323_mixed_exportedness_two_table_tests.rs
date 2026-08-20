//! Mixed-exportedness variable redeclarations (#16170): `tsc` runs the
//! collision walk over *two* independent symbol tables — `container.locals`
//! (every declaration, but an exported one contributes only `EXPORT_VALUE`)
//! and `container.symbol.exports` (only the exported declarations, real
//! flags) — plus two checker-side follow-ups that each read one of those
//! tables: TS2395 reads the locals-table survivor, TS2323 reads the
//! exports-table survivor. A single merged pass (`ts2323_variable_redeclaration_two_pass_tests.rs`'s
//! all-exported/all-local family) cannot reproduce this because the two
//! tables see different declaration sequences the moment exportedness mixes.
//!
//! Every expectation below is pinned against `tsc` 7.0.2 run with
//! `--noEmit --strict --pretty false --lib es2015 --target es2015`, from the
//! oracle matrix gathered for #16170. Positions matter, so these assert
//! per-line code sets.

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

/// `_var evar elet`: the locals-table survivor accumulates
/// `FUNCTION_SCOPED_VARIABLE` from line 2 (unexported, real flags), stays
/// unbothered by line 3's exported `var` (contributes `EXPORT_VALUE` only,
/// which does not collide with a var survivor), then collides with line 4's
/// exported `let` — TS2300 lands on all three. The locals-table survivor
/// mixes exported (line 3) and non-exported (line 2) members, so TS2395 joins
/// on those two only; line 4 lost the collision onto its own throwaway
/// symbol and is never part of that group. No declaration reaches the
/// exports-table survivor with more than one member (only line 3 is
/// exports-table eligible), so TS2323 never fires.
#[test]
fn unexported_var_then_exported_var_then_exported_let() {
    assert_eq!(
        codes_by_line("export {};\nvar alpha = 1;\nexport var alpha = 2;\nexport let alpha = 3;\n"),
        vec![
            (2, vec![2300, 2395]),
            (3, vec![2300, 2395]),
            (4, vec![2300])
        ],
    );
}

/// `elet _var elet`: the locals-table survivor holds line 2 (exported let,
/// contributes `EXPORT_VALUE` only) then line 3 (unexported var, real flags)
/// without colliding — `EXPORT_VALUE` never excludes a var. Line 4 (exported
/// let) collides with that survivor (whose accumulated flags now include the
/// real `FUNCTION_SCOPED_VARIABLE` bit from line 3): TS2300, not TS2451,
/// because the *survivor's* accumulated flags never carried a real
/// block-scoped bit. TS2395 covers the same locals-table survivor (lines 2-3,
/// mixed exportedness). Independently, the exports-table walk holds only
/// lines 2 and 4 (both exported lets, real flags) — those collide with each
/// other for TS2451, on both, regardless of what happened in locals.
#[test]
fn exported_let_then_unexported_var_then_exported_let() {
    assert_eq!(
        codes_by_line("export {};\nexport let alpha = 1;\nvar alpha = 2;\nexport let alpha = 3;\n"),
        vec![
            (2, vec![2300, 2395, 2451]),
            (3, vec![2300, 2395]),
            (4, vec![2300, 2451]),
        ],
    );
}

/// `evar _let evar`: the locals-table survivor holds line 2 (exported var,
/// `EXPORT_VALUE` only), then line 3 (unexported let, real
/// `BLOCK_SCOPED_VARIABLE` flag) joins without colliding (`EXPORT_VALUE`
/// alone never excludes anything). Line 4 (exported var) collides with that
/// survivor because the survivor's accumulated flags now carry the real
/// block-scoped bit from line 3 — TS2451, not TS2300, and it lands on all
/// three (lines 2-3 from the survivor, line 4 as the colliding declaration).
/// TS2395 covers the same survivor (lines 2-3). Independently, the
/// exports-table walk holds only lines 2 and 4 (both exported vars, which do
/// not exclude each other) — that group survives with two members, so TS2323
/// fires on both.
#[test]
fn exported_var_then_unexported_let_then_exported_var() {
    assert_eq!(
        codes_by_line("export {};\nexport var alpha = 1;\nlet alpha = 2;\nexport var alpha = 3;\n"),
        vec![
            (2, vec![2323, 2395, 2451]),
            (3, vec![2395, 2451]),
            (4, vec![2323, 2451]),
        ],
    );
}

/// `_var elet _var`: line 3 (exported let) collides with the locals-table
/// survivor seeded by line 2 (unexported var, real flags) — TS2300 on both.
/// Line 4 (unexported var) does *not* collide with the survivor (still `var`,
/// still just line 2's real flags — line 3 lost its collision onto a
/// throwaway symbol and never widened the survivor), so it rejoins the
/// survivor silently. The rejoined survivor (lines 2 and 4) is uniformly
/// non-exported, so TS2395 never fires, and nothing is exports-table
/// eligible except line 3 alone, so TS2323 never fires either. Line 4 ends
/// with zero diagnostics.
#[test]
fn unexported_var_then_exported_let_then_unexported_var_leaves_the_rejoiner_clean() {
    assert_eq!(
        codes_by_line("export {};\nvar alpha = 1;\nexport let alpha = 2;\nvar alpha = 3;\n"),
        vec![(2, vec![2300]), (3, vec![2300])],
    );
}

/// Renamed-binder control: the rule is structural, not keyed on any
/// identifier — same shape as the `evar _let evar` row above, different name.
#[test]
fn exported_var_then_unexported_let_then_exported_var_is_name_independent() {
    assert_eq!(
        codes_by_line("export {};\nexport var zeta = 1;\nlet zeta = 2;\nexport var zeta = 3;\n"),
        vec![
            (2, vec![2323, 2395, 2451]),
            (3, vec![2395, 2451]),
            (4, vec![2323, 2451]),
        ],
    );
}

/// Simplest two-declaration mixed-exportedness pair, reversed order from the
/// pre-existing `mixed_exportedness_var_pair_is_ts2395` regression control:
/// neither table ever collides (a var survivor is never excluded by another
/// var's `EXPORT_VALUE`-only contribution, and the exports table alone has
/// one member), so only TS2395 fires.
#[test]
fn unexported_var_then_exported_var_is_ts2395_only() {
    assert_eq!(
        codes_by_line("export {};\nvar alpha = 1;\nexport var alpha = 2;\n"),
        vec![(2, vec![2395]), (3, vec![2395])],
    );
}

/// Two-declaration mixed pair where the *unexported* declaration is the
/// block-scoped one: line 2's real `BLOCK_SCOPED_VARIABLE` flag collides with
/// line 3's exported `var` (the exclusion is keyed on the *incoming*
/// declaration's own real kind — `var`'s excludes do intersect a block-scoped
/// survivor regardless of export status) for TS2451 on both. Line 3 loses the
/// collision onto its own throwaway symbol, so the locals-table survivor
/// stays a lone, uniformly non-exported line 2 — too small to ever mix
/// exportedness, so TS2395 cannot fire here (that needs 3+ declarations; see
/// the `evar _let evar` / `elet _var elet` rows above). The exports table
/// holds line 3 alone, so TS2323 never fires either.
#[test]
fn unexported_let_then_exported_var_is_ts2451_only() {
    assert_eq!(
        codes_by_line("export {};\nlet alpha = 1;\nexport var alpha = 2;\n"),
        vec![(2, vec![2451]), (3, vec![2451])],
    );
}
