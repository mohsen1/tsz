//! TS1309 (`The current file is a CommonJS module and cannot use 'await' at
//! the top level.`) — the per-file arm of `tsc`'s
//! `checkGrammarAwaitOrAwaitUsing` module switch.
//!
//! Under a Node module kind (`node16`/`node18`/`node20`/`nodenext`) `tsc`
//! decides the top-level-`await` question per *file*, from the file's implied
//! format, not per program:
//!
//! * `CommonJS`-format file (a `.cts`/`.cjs` extension, or an ambiguous
//!   `.ts`/`.js` whose nearest `package.json` has no `"type": "module"`) —
//!   TS1309, at every target, and the module/target family (TS1378 for
//!   `await`, TS1432 for `for await`, TS2854 for `await using`) does not also
//!   fire.
//! * ESM-format file — no TS1309; the ordinary module + target requirement
//!   applies, so a target below ES2017 still answers TS1378.
//!
//! The "…but this file has no imports or exports" family (TS1375/TS1431/
//! TS2853) is unreachable under a Node module kind in either direction: an
//! ESM-format file is a module even with no imports or exports, and a
//! `CommonJS`-format one answers TS1309 instead.
//!
//! Every expectation below is pinned against a live `typescript@7.0.2` run
//! (`--noEmit --strict --lib esnext`, `--module node16 --moduleResolution
//! node16` unless the row says otherwise), not recalled. The fixtures used a
//! two-package layout — one `package.json` with no `"type"` field, one with
//! `"type": "module"` — to drive the ambiguous-extension rows.

use crate::test_utils::check_source_with_file_is_esm;
use tsz_common::checker_options::CheckerOptions;
use tsz_common::common::{ModuleKind, ScriptTarget};

/// Check `source` as `file_name` under `module`/`target`, with the driver's
/// per-file format classification set to `file_is_esm`, and return the
/// diagnostic codes.
fn codes(
    source: &str,
    file_name: &str,
    module: ModuleKind,
    target: ScriptTarget,
    file_is_esm: Option<bool>,
) -> Vec<u32> {
    let options = CheckerOptions {
        module,
        target,
        ..CheckerOptions::default()
    };
    check_source_with_file_is_esm(source, file_name, options, file_is_esm)
        .iter()
        .map(|d| d.code)
        .collect()
}

/// The common case: a Node module kind and a `CommonJS`-format file.
fn node16_cjs_codes(source: &str, file_name: &str) -> Vec<u32> {
    codes(
        source,
        file_name,
        ModuleKind::Node16,
        ScriptTarget::ES2022,
        Some(false),
    )
}

// --- `await` expression ---

/// oracle: `m/cjs/a.ts(2,1): error TS1309` under `--module node16`. An
/// ambiguous `.ts` extension in a package without `"type": "module"`.
#[test]
fn top_level_await_in_commonjs_ts_file_reports_ts1309() {
    let source = "export {};\nawait 1;";
    let diags = node16_cjs_codes(source, "a.ts");
    assert!(
        diags.contains(&1309),
        "a CommonJS-format file under node16 must report TS1309; got {diags:?}"
    );
    assert!(
        !diags.contains(&1378),
        "TS1309 replaces the module/target family, it does not accompany it; got {diags:?}"
    );
}

/// The unambiguous extension: `.cts` is `CommonJS` whatever the surrounding
/// `package.json` says, so the format lookup must not be consulted.
/// oracle: `nomod.cts(1,1): error TS1309`.
#[test]
fn top_level_await_in_cts_file_reports_ts1309_without_format_lookup() {
    let source = "await 1;";
    let diags = codes(
        source,
        "nomod.cts",
        ModuleKind::Node16,
        ScriptTarget::ES2022,
        None,
    );
    assert!(
        diags.contains(&1309),
        "a `.cts` file under node16 must report TS1309; got {diags:?}"
    );
}

/// The same `.cts` file has no imports or exports, and `tsc` still does not
/// add TS1375 — under a Node module kind the "no imports or exports" family
/// is unreachable. oracle: TS1309 alone.
#[test]
fn top_level_await_in_cts_file_does_not_also_report_ts1375() {
    let source = "await 1;";
    let diags = codes(
        source,
        "nomod.cts",
        ModuleKind::Node16,
        ScriptTarget::ES2022,
        None,
    );
    assert!(
        !diags.contains(&1375),
        "TS1375 must not accompany TS1309; got {diags:?}"
    );
}

/// The ambiguous-extension, no-imports-or-exports row: `plain.ts` in a
/// CommonJS package. oracle: TS1309 alone — again no TS1375.
#[test]
fn top_level_await_in_commonjs_script_reports_only_ts1309() {
    let source = "await 1;";
    let diags = node16_cjs_codes(source, "plain.ts");
    assert!(diags.contains(&1309), "expected TS1309; got {diags:?}");
    assert!(
        !diags.contains(&1375) && !diags.contains(&1378),
        "TS1309 is the whole answer for this row; got {diags:?}"
    );
}

/// Anti-hardcoding control: nothing about the decision may depend on the
/// awaited expression or on the binder names around it.
#[test]
fn top_level_await_ts1309_is_independent_of_awaited_expression_and_names() {
    let source = "export const someBinding = 1;\ndeclare const producer: { readonly job: number };\nawait producer.job;";
    let diags = node16_cjs_codes(source, "renamed-module.cts");
    assert!(
        diags.contains(&1309),
        "the TS1309 decision is per file format, not per expression; got {diags:?}"
    );
}

// --- the ESM side of the same switch ---

/// oracle: `esm.mts` under node16 is clean — an ESM-format file may host
/// top-level `await`.
#[test]
fn top_level_await_in_mts_file_is_clean() {
    let source = "export {};\nawait 1;";
    let diags = codes(
        source,
        "esm.mts",
        ModuleKind::Node16,
        ScriptTarget::ES2022,
        None,
    );
    assert!(
        !diags.contains(&1309) && !diags.contains(&1378) && !diags.contains(&1375),
        "an ESM-format file under node16 must be clean; got {diags:?}"
    );
}

/// The ambiguous extension resolved the other way: a `.ts` file in a package
/// with `"type": "module"`. oracle: `m/esmpkg/a.ts` is clean.
#[test]
fn top_level_await_in_esm_resolved_ts_file_is_clean() {
    let source = "export {};\nawait 1;";
    let diags = codes(
        source,
        "a.ts",
        ModuleKind::Node16,
        ScriptTarget::ES2022,
        Some(true),
    );
    assert!(
        !diags.contains(&1309),
        "an ESM-resolved file must not report TS1309; got {diags:?}"
    );
}

/// An ESM-format file with no imports or exports is still a module under a
/// Node module kind — `tsc` does not report TS1375 for it.
/// oracle: `plainesm.mts` under node16 is clean.
#[test]
fn top_level_await_in_esm_script_under_node16_is_clean() {
    let source = "await 1;";
    let diags = codes(
        source,
        "plainesm.mts",
        ModuleKind::Node16,
        ScriptTarget::ES2022,
        None,
    );
    assert!(
        !diags.contains(&1375) && !diags.contains(&1309),
        "an ESM-format file needs no import or export to be a module; got {diags:?}"
    );
}

/// The unresolved single-file case (`file_is_esm == None`, ambiguous
/// extension): no project resolved the format, so the `CommonJS` arm must not
/// be taken on a guess.
#[test]
fn top_level_await_with_unresolved_file_format_does_not_report_ts1309() {
    let source = "export {};\nawait 1;";
    let diags = codes(
        source,
        "a.ts",
        ModuleKind::Node16,
        ScriptTarget::ES2022,
        None,
    );
    assert!(
        !diags.contains(&1309),
        "an unresolved implied format must not answer TS1309; got {diags:?}"
    );
}

// --- the other Node module kinds ---

/// oracle: node18, node20 and nodenext all report TS1309 on the same file.
#[test]
fn top_level_await_reports_ts1309_under_every_node_module_kind() {
    for module in [ModuleKind::Node18, ModuleKind::Node20, ModuleKind::NodeNext] {
        let diags = codes(
            "export {};\nawait 1;",
            "a.ts",
            module,
            ScriptTarget::ES2022,
            Some(false),
        );
        assert!(
            diags.contains(&1309),
            "{module:?} must report TS1309; got {diags:?}"
        );
    }
}

/// The `CommonJS` arm short-circuits the target check: TS1309 fires at a
/// target below ES2017 too, and TS1378 still does not accompany it.
/// oracle: `--target es2016 --module node16` on the CommonJS file → TS1309.
#[test]
fn top_level_await_in_commonjs_file_reports_ts1309_below_es2017() {
    let diags = codes(
        "export {};\nawait 1;",
        "a.ts",
        ModuleKind::Node16,
        ScriptTarget::ES2016,
        Some(false),
    );
    assert!(
        diags.contains(&1309) && !diags.contains(&1378),
        "the CommonJS arm precedes the target check; got {diags:?}"
    );
}

/// The ESM-format sibling at the same low target keeps the ordinary
/// module/target answer. oracle: `esm.mts --target es2015 --module node16` →
/// TS1378.
#[test]
fn top_level_await_in_esm_file_below_es2017_reports_ts1378() {
    let diags = codes(
        "export {};\nawait 1;",
        "esm.mts",
        ModuleKind::Node16,
        ScriptTarget::ES2015,
        None,
    );
    assert!(
        diags.contains(&1378) && !diags.contains(&1309),
        "an ESM-format file below ES2017 still answers TS1378; got {diags:?}"
    );
}

// --- non-Node module kinds are untouched ---

/// oracle: `--module commonjs` on a module file → TS1378, never TS1309. The
/// per-file arm belongs to the Node module kinds alone.
#[test]
fn top_level_await_under_module_commonjs_still_reports_ts1378() {
    let diags = codes(
        "export {};\nawait 1;",
        "a.ts",
        ModuleKind::CommonJS,
        ScriptTarget::ES2022,
        Some(false),
    );
    assert!(
        diags.contains(&1378) && !diags.contains(&1309),
        "module=commonjs answers TS1378; got {diags:?}"
    );
}

/// oracle: `--module commonjs` on a script → the TS1375 + TS1378 pair, which
/// this change must leave intact.
#[test]
fn top_level_await_under_module_commonjs_script_still_reports_the_pair() {
    let diags = codes(
        "await 1;",
        "plain.ts",
        ModuleKind::CommonJS,
        ScriptTarget::ES2022,
        None,
    );
    assert!(
        diags.contains(&1375) && diags.contains(&1378),
        "the non-Node pair must survive; got {diags:?}"
    );
}

/// oracle: `--module esnext` on a `.cts` file is clean — outside the Node
/// module kinds the extension does not decide anything.
#[test]
fn cts_extension_under_module_esnext_is_clean() {
    let diags = codes(
        "await 1;",
        "nomod.cts",
        ModuleKind::ESNext,
        ScriptTarget::ES2022,
        None,
    );
    assert!(
        !diags.contains(&1309),
        "a `.cts` file under module=esnext must not report TS1309; got {diags:?}"
    );
}

// --- the two sibling constructs ---

/// `for await` asks the same question and gets the same answer.
/// oracle: `forawait.cts(3,5): error TS1309` — and no TS1432.
#[test]
fn top_level_for_await_in_commonjs_file_reports_ts1309() {
    let source =
        "export {};\ndeclare const it: AsyncIterable<number>;\nfor await (const value of it) { }";
    let diags = node16_cjs_codes(source, "forawait.cts");
    assert!(
        diags.contains(&1309),
        "top-level `for await` in a CommonJS file must report TS1309; got {diags:?}"
    );
    assert!(
        !diags.contains(&1432) && !diags.contains(&1431),
        "TS1309 replaces the `for await` module/target family; got {diags:?}"
    );
}

/// `await using` likewise. oracle: `awaitusing.cts(3,1): error TS1309` — and
/// no TS2853/TS2854.
#[test]
fn top_level_await_using_in_commonjs_file_reports_ts1309() {
    let source =
        "export {};\ndeclare const resource: AsyncDisposable;\nawait using held = resource;";
    let diags = node16_cjs_codes(source, "awaitusing.cts");
    assert!(
        diags.contains(&1309),
        "top-level `await using` in a CommonJS file must report TS1309; got {diags:?}"
    );
    assert!(
        !diags.contains(&2853) && !diags.contains(&2854),
        "TS1309 replaces the `await using` module/target family; got {diags:?}"
    );
}

/// The ESM sibling of both constructs stays clean.
/// oracle: `fa-nomod.mts` and `au-nomod.mts` under node16 are clean.
#[test]
fn sibling_constructs_in_esm_file_are_clean() {
    let for_await = "declare const it: AsyncIterable<number>;\nfor await (const value of it) { }";
    let diags = codes(
        for_await,
        "fa-nomod.mts",
        ModuleKind::Node16,
        ScriptTarget::ES2022,
        None,
    );
    assert!(
        !diags.contains(&1309) && !diags.contains(&1431) && !diags.contains(&1432),
        "`for await` in an ESM-format file is clean; got {diags:?}"
    );

    let await_using = "declare const resource: AsyncDisposable;\nawait using held = resource;";
    let diags = codes(
        await_using,
        "au-nomod.mts",
        ModuleKind::Node16,
        ScriptTarget::ES2022,
        None,
    );
    assert!(
        !diags.contains(&1309) && !diags.contains(&2853) && !diags.contains(&2854),
        "`await using` in an ESM-format file is clean; got {diags:?}"
    );
}

// --- negative control ---

/// The construct still has to be at the top level: an `await` inside an
/// `async` function in the very same CommonJS file is legal.
/// oracle: `inasync.cts` under node16 is clean.
#[test]
fn await_inside_async_function_in_commonjs_file_is_clean() {
    let source = "export {};\nasync function run() { await 1; }";
    let diags = node16_cjs_codes(source, "inasync.cts");
    assert!(
        !diags.contains(&1309),
        "a nested `await` is not a top-level `await`; got {diags:?}"
    );
}
