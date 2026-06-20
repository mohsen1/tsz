//! Grammar diagnostics for top-level `await` expressions (TS1375 / TS1378).
//!
//! `tsc`'s `checkAwaitExpression` emits two *independent* grammar diagnostics
//! for a top-level `await`:
//!
//! - **TS1375** — the file is not a module (it has no imports/exports), so a
//!   top-level `await` is not allowed.
//! - **TS1378** — the `module`/`target` combination does not support top-level
//!   `await` (it requires `module` in `{es2022, esnext, system, node16,
//!   node18, node20, nodenext, preserve}` *and* `target >= es2017`).
//!
//! These are **not** mutually exclusive: a bare script under an unsupported
//! module produces *both*. The sibling `await using` path already emits its
//! TS2853/TS2854 pair this way; these tests lock the same behaviour for the
//! `await` expression path, which previously short-circuited (an `else if`)
//! and dropped one of the two diagnostics.
//!
//! The structural rule keys only on module-ness and the module/target pair, so
//! the tests do not depend on any identifier, alias, or file-name spelling.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;
use tsz_common::common::{ModuleKind, ScriptTarget};

/// Diagnostic codes produced for `source` under the given module/target.
fn codes(source: &str, module: ModuleKind, target: ScriptTarget) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            module,
            target,
            ..CheckerOptions::default()
        },
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

#[test]
fn script_with_unsupported_module_emits_both_ts1375_and_ts1378() {
    // Non-module file + `module: ES2020` (not in the supported set): both the
    // "file is not a module" (TS1375) and the "module/target unsupported"
    // (TS1378) diagnostics must fire — they are independent.
    let got = codes("await 1;", ModuleKind::ES2020, ScriptTarget::ES2020);
    assert!(
        got.contains(&1375),
        "expected TS1375 (script file) alongside TS1378, got: {got:?}"
    );
    assert!(
        got.contains(&1378),
        "expected TS1378 (unsupported module/target) alongside TS1375, got: {got:?}"
    );
}

#[test]
fn script_with_commonjs_module_emits_both_ts1375_and_ts1378() {
    // The same independence holds for CommonJS, the most common unsupported
    // module. (`target: ES2020` keeps TS1378's target half satisfied so the
    // module half is what drives it.)
    let got = codes("await 1;", ModuleKind::CommonJS, ScriptTarget::ES2020);
    assert!(
        got.contains(&1375),
        "expected TS1375 (script file) alongside TS1378, got: {got:?}"
    );
    assert!(
        got.contains(&1378),
        "expected TS1378 (unsupported module) alongside TS1375, got: {got:?}"
    );
}

#[test]
fn module_with_unsupported_module_emits_only_ts1378() {
    // When the file IS a module (it has an `export`), the TS1375 "not a module"
    // half no longer applies; only TS1378 fires.
    let got = codes(
        "export {};\nawait 1;",
        ModuleKind::ES2020,
        ScriptTarget::ES2020,
    );
    assert!(
        got.contains(&1378),
        "expected TS1378 for an unsupported module in a module file, got: {got:?}"
    );
    assert!(
        !got.contains(&1375),
        "expected NO TS1375 in an external module, got: {got:?}"
    );
}

#[test]
fn script_with_supported_module_emits_only_ts1375() {
    // The converse: a supported module/target (ES2022 + ES2017) in a
    // non-module file yields only the "not a module" diagnostic TS1375, never
    // TS1378.
    let got = codes("await 1;", ModuleKind::ES2022, ScriptTarget::ES2017);
    assert!(
        got.contains(&1375),
        "expected TS1375 for a script file, got: {got:?}"
    );
    assert!(
        !got.contains(&1378),
        "expected NO TS1378 with a supported module/target, got: {got:?}"
    );
}

#[test]
fn module_with_supported_module_emits_neither() {
    // Fully valid top-level await: a module file under a supported
    // module/target emits neither grammar diagnostic.
    let got = codes(
        "export {};\nawait 1;",
        ModuleKind::ES2022,
        ScriptTarget::ES2017,
    );
    assert!(
        !got.contains(&1375) && !got.contains(&1378),
        "expected neither TS1375 nor TS1378 for valid top-level await, got: {got:?}"
    );
}

#[test]
fn unsupported_target_alone_emits_ts1378_in_module() {
    // TS1378 also fires when only the target half fails: a supported module
    // (ES2022) but `target: ES2015` (< ES2017). In a module file, only TS1378.
    let got = codes(
        "export {};\nawait 1;",
        ModuleKind::ES2022,
        ScriptTarget::ES2015,
    );
    assert!(
        got.contains(&1378),
        "expected TS1378 for target < ES2017, got: {got:?}"
    );
    assert!(
        !got.contains(&1375),
        "expected NO TS1375 in an external module, got: {got:?}"
    );
}
