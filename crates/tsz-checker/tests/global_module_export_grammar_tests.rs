//! TS1314 / TS1315 / TS1316 — the `export as namespace N;` (global module
//! export) grammar family.
//!
//! tsc's `checkNamespaceExportDeclaration` is a three-step early-return chain,
//! and the *order* is the rule: the declaration's position is decided before
//! anything about the containing file is consulted.
//!
//! 1. parent is not the source file  -> TS1316, return
//! 2. file is not an external module -> TS1314, return
//! 3. file is not a declaration file -> TS1315, return
//!
//! Before this suite tsz wired only step 3, and ran it for every top-level
//! occurrence in a non-`.d.ts` file without consulting steps 1 or 2. Two
//! consequences, both pinned below: a non-module `.ts` file reported TS1315
//! where tsc reports TS1314 (a *wrong* code, not merely a missing one), and
//! every nested occurrence was silently accepted.
//!
//! Every expectation here is pinned against the vendored `typescript@7.0.2`
//! oracle run as
//! `tsc --noEmit --strict --target es2022 --module esnext --lib esnext`.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{check_source, check_source_codes_named};

/// Only this family's codes. The surrounding fixtures deliberately carry
/// unrelated diagnostics in a few rows (an unresolved import specifier, for
/// one), and pinning those would make the suite a hostage to unrelated work.
fn global_module_export_codes(source: &str, file_name: &str) -> Vec<u32> {
    let mut codes: Vec<u32> = check_source_codes_named(source, file_name)
        .into_iter()
        .filter(|c| matches!(c, 1314..=1316))
        .collect();
    codes.sort_unstable();
    codes
}

// ---------------------------------------------------------------------------
// Step 2 — TS1314: the file is not an external module.
//
// These are the rows that regressed *away* from TS1315. A
// `NamespaceExportDeclaration` is deliberately not an external-module
// indicator in either compiler, so step 2 never sees the very declaration it
// is judging: `export as namespace Foo;` alone leaves the file a script.
// ---------------------------------------------------------------------------

#[test]
fn lone_global_module_export_in_a_script_ts_file_is_ts1314_not_ts1315() {
    assert_eq!(
        global_module_export_codes("export as namespace Foo;\n", "a.ts"),
        vec![1314],
        "tsc reports TS1314 here; tsz reported TS1315 before this fix"
    );
}

#[test]
fn lone_global_module_export_in_a_script_dts_file_is_ts1314() {
    assert_eq!(
        global_module_export_codes("export as namespace Foo;\n", "a.d.ts"),
        vec![1314],
        "a .d.ts is not automatically a module; tsz reported nothing before this fix"
    );
}

#[test]
fn ts1314_does_not_depend_on_the_exported_name() {
    // Anti-hardcoding: the binder name is arbitrary and drives no decision.
    assert_eq!(
        global_module_export_codes("export as namespace Bar;\n", "a.ts"),
        vec![1314],
    );
    assert_eq!(
        global_module_export_codes("export as namespace qux$_0;\n", "a.ts"),
        vec![1314],
    );
}

#[test]
fn ts1314_reports_once_per_declaration() {
    assert_eq!(
        global_module_export_codes(
            "export as namespace Foo;\nexport as namespace Bar;\n",
            "a.d.ts"
        ),
        vec![1314, 1314],
        "tsc's grammarErrorOnNode fires per declaration, not once per file"
    );
}

#[test]
fn an_ambient_module_block_is_not_an_external_module_indicator() {
    assert_eq!(
        global_module_export_codes(
            "declare module \"m\" { }\nexport as namespace Foo;\n",
            "a.d.ts"
        ),
        vec![1314],
    );
}

// ---------------------------------------------------------------------------
// Step 3 — TS1315: module file, but not a declaration file.
//
// The pre-existing behaviour, which must survive the re-ordering.
// ---------------------------------------------------------------------------

#[test]
fn global_module_export_in_a_module_ts_file_is_still_ts1315() {
    assert_eq!(
        global_module_export_codes("export {};\nexport as namespace Foo;\n", "a.ts"),
        vec![1315],
    );
}

#[test]
fn an_import_declaration_is_enough_to_reach_step_3() {
    // `export {}` is not the only indicator — the step-2 predicate must accept
    // any of tsc's indicator nodes, not just the one the other tests use.
    assert_eq!(
        global_module_export_codes("import \"./other\";\nexport as namespace Foo;\n", "a.ts"),
        vec![1315],
    );
}

#[test]
fn an_exported_declaration_is_enough_to_reach_step_3() {
    assert_eq!(
        global_module_export_codes(
            "export const value = 1;\nexport as namespace Foo;\n",
            "a.ts"
        ),
        vec![1315],
    );
}

// ---------------------------------------------------------------------------
// The clean row — module file AND declaration file. The whole chain falls
// through. This is the shape real UMD `.d.ts` entrypoints have.
// ---------------------------------------------------------------------------

#[test]
fn global_module_export_in_a_module_dts_file_is_clean() {
    assert_eq!(
        global_module_export_codes("export {};\nexport as namespace Foo;\n", "a.d.ts"),
        Vec::<u32>::new(),
    );
}

#[test]
fn export_equals_also_reaches_the_clean_row() {
    assert_eq!(
        global_module_export_codes(
            "declare const x: number;\nexport = x;\nexport as namespace Foo;\n",
            "a.d.ts"
        ),
        Vec::<u32>::new(),
    );
}

// ---------------------------------------------------------------------------
// Step 1 — TS1316: not at top level. Decided BEFORE module-ness and
// declaration-file-ness, so it holds regardless of what the file looks like.
// ---------------------------------------------------------------------------

#[test]
fn nested_in_a_namespace_is_ts1316() {
    assert_eq!(
        global_module_export_codes(
            "export {};\nnamespace N { export as namespace Foo; }\n",
            "a.ts"
        ),
        vec![1316],
    );
}

#[test]
fn nested_in_a_declared_namespace_in_a_dts_file_is_ts1316() {
    assert_eq!(
        global_module_export_codes(
            "export {};\ndeclare namespace N { export as namespace Foo; }\n",
            "a.d.ts"
        ),
        vec![1316],
    );
}

#[test]
fn nested_in_an_ambient_module_block_is_ts1316() {
    assert_eq!(
        global_module_export_codes(
            "export {};\ndeclare module \"m\" { export as namespace Foo; }\n",
            "a.d.ts"
        ),
        vec![1316],
    );
}

#[test]
fn deeply_nested_is_ts1316() {
    assert_eq!(
        global_module_export_codes(
            "export {};\nnamespace A { namespace B { export as namespace Foo; } }\n",
            "a.ts"
        ),
        vec![1316],
    );
}

#[test]
fn ts1316_reports_once_per_declaration() {
    assert_eq!(
        global_module_export_codes(
            "export {};\nnamespace N { export as namespace Foo; export as namespace Bar; }\n",
            "a.ts"
        ),
        vec![1316, 1316],
    );
}

#[test]
fn position_wins_over_module_ness_in_a_script_file() {
    // The load-bearing ordering case. This file is NOT an external module, so
    // step 2 would also hold — but tsc returns at step 1, so TS1316 is the
    // only diagnostic and TS1314 must not accompany it.
    assert_eq!(
        global_module_export_codes("namespace N { export as namespace Foo; }\n", "a.ts"),
        vec![1316],
    );
}

#[test]
fn position_wins_over_declaration_file_ness() {
    // Same ordering rule from the other side: a module `.ts` file nested
    // occurrence would satisfy step 3, but step 1 returns first.
    assert_eq!(
        global_module_export_codes(
            "export {};\nnamespace N { export as namespace Foo; }\n",
            "a.ts"
        ),
        vec![1316],
    );
}

// ---------------------------------------------------------------------------
// Negative controls — the family must not fire where there is no
// `export as namespace` at all, and the module-ness change must not leak into
// files that never mention one.
// ---------------------------------------------------------------------------

#[test]
fn a_file_with_no_global_module_export_is_clean() {
    assert_eq!(
        global_module_export_codes("export {};\nfunction f() { }\n", "a.ts"),
        Vec::<u32>::new(),
    );
}

#[test]
fn a_plain_script_file_is_clean() {
    assert_eq!(
        global_module_export_codes("function f() { }\n", "a.ts"),
        Vec::<u32>::new(),
    );
}

#[test]
fn a_namespace_export_inside_a_namespace_is_not_confused_with_a_global_module_export() {
    // `export namespace` / `export const` inside a namespace body are ordinary
    // exported declarations, not `NamespaceExportDeclaration` nodes.
    assert_eq!(
        global_module_export_codes(
            "export {};\nnamespace N { export const x = 1; export namespace M { } }\n",
            "a.ts"
        ),
        Vec::<u32>::new(),
    );
}

// ---------------------------------------------------------------------------
// #16403 residual: TS1314's own COLUMN when a stray modifier precedes
// `export as namespace`. This is a checker diagnostic, so it reads whatever
// span the parser gave the `NamespaceExportDeclaration` node — tsc anchors it
// at the modifier (column 1), not at `export`, matching TS1184 alongside it.
// A code-set comparison cannot see this: TS1314 fires either way, only the
// column differs. Oracle-pinned (`typescript@7.0.2`) for every modifier in
// this family; `accessor`/`async` already anchored correctly before this fix.
// ---------------------------------------------------------------------------

fn ts1314_start(source: &str, file_name: &str) -> u32 {
    let diags = check_source(source, file_name, CheckerOptions::default());
    diags
        .iter()
        .find(|d| d.code == 1314)
        .unwrap_or_else(|| panic!("expected TS1314 for {source:?}, got {diags:?}"))
        .start
}

#[test]
fn modifier_before_global_module_export_anchors_ts1314_at_the_modifier() {
    for modifier in ["static", "public", "protected", "private", "readonly"] {
        let source = format!("{modifier} export as namespace Foo;\n");
        assert_eq!(
            ts1314_start(&source, "a.ts"),
            0,
            "tsc anchors TS1314 at the modifier (column 1), not at `export`, for {source:?}"
        );
    }
}

#[test]
fn modifier_before_global_module_export_ts1314_column_is_unaffected_by_leading_content() {
    // Same rule, with the declaration not at the very start of the file, so a
    // fix that merely special-cased offset 0 would not be caught by the test
    // above.
    let source = "export {};\nprivate export as namespace Foo;\n";
    let diags = check_source(source, "a.ts", CheckerOptions::default());
    // `export {};\n` makes the file an external module, so TS1314 (step 2:
    // "not an external module") no longer applies — this reaches TS1315
    // instead (step 3: module file, not a declaration file). Confirms the
    // node span fix generalizes to a differently-positioned declaration by
    // checking the sibling code in the same family anchors correctly too.
    let ts1315 = diags
        .iter()
        .find(|d| d.code == 1315)
        .unwrap_or_else(|| panic!("expected TS1315 for {source:?}, got {diags:?}"));
    assert_eq!(
        ts1315.start,
        "export {};\n".len() as u32,
        "TS1315 must anchor at `private`, not `export`, for {source:?}"
    );
}
