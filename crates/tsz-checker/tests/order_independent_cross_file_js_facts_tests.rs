//! Order-independence regression test for the UMD-global cross-file JS fact
//! that is decided by scanning the *current file's* source AST.
//!
//! Root cause (refs <https://github.com/tsz-org/tsz/issues/17410>):
//! `current_file_is_module_for_umd_global_access` read the current file's
//! single-source-file arena with
//! `self.ctx.arena.source_files.get(self.ctx.current_file_idx)`. The per-file
//! arena holds exactly one source file at local position 0, so indexing it by
//! the *program-global* `current_file_idx` only succeeds when that file happens
//! to sit at program index 0. For any host file at a non-zero index the lookup
//! returned `None` and the scan silently bailed, suppressing `TS2686` for a bare
//! UMD-global reference in a `checkJs` file — the first predicate of the TS2686
//! guard.
//!
//! This was unmasked — not caused — by the tsc-accurate `discover_ts_files`
//! root ordering (`.ts` family before `.js` family), which moves a `.js` host
//! file off program index 0. The fix reads `source_files.first()` (the file's
//! own single source, like the 98 sibling sites), so the decision depends on the
//! declaring arena, not on file processing order.
//!
//! Structural rule under test:
//!
//! > When a per-file AST fact (is-this-file-a-module) is decided by scanning
//! > the current file's source, tsc's answer is a pure function of that file's
//! > AST and is identical regardless of where the file lands in program order;
//! > tsz now matches by selecting the arena's own single source file.
//!
//! Each test checks the same logical program under every file permutation and
//! asserts byte-identical diagnostics. The rule is name-agnostic: the UMD
//! namespace, module, member, and file names are varied across the two cases so
//! neither is keyed on any spelling.
//!
//! A sibling defect in `augment_object_type_with_define_properties` (same
//! `get(current_file_idx)` pattern, `Object.defineProperty` expando members)
//! looked like the same bug but is NOT fixed here: switching it to `.first()`
//! passed this file's own permutation tests but introduced a real false-positive
//! regression on `ensureNoCrashExportAssignmentDefineProperrtyPotentialMerge.ts`
//! (conformance `compare-to-parent` gate) — the whole-file defineProperty scan
//! doesn't respect statement order relative to the property access being
//! checked, so a later `Object.defineProperty` call's type leaks backward into
//! an earlier assignment's assignability check. That needs a position-aware
//! fix, not a same-shape `.first()` swap; left as an unclaimed follow-up on
//! #17410.

use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{check_multi_file_with_libs_stamped, load_default_lib_files};
use tsz_common::ModuleKind;

fn commonjs_checkjs_opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        allow_js: true,
        check_js: true,
        ..Default::default()
    }
}

/// Render diagnostics into a stable, order-independent signature: a sorted set
/// of `code@start+len` strings. File processing order must not change this set.
fn diagnostic_signature(diags: &[Diagnostic]) -> Vec<String> {
    let mut sig: Vec<String> = diags
        .iter()
        .map(|d| format!("TS{}@{}+{}", d.code, d.start, d.length))
        .collect();
    sig.sort();
    sig
}

/// Generate every permutation of `items` (small n only).
fn permutations<'a>(items: &[(&'a str, &'a str)]) -> Vec<Vec<(&'a str, &'a str)>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let head = rest.remove(i);
        for mut perm in permutations(&rest) {
            perm.insert(0, head);
            out.push(perm);
        }
    }
    out
}

/// Assert that checking `entry` is order-independent across every permutation of
/// the project files, and return the canonical diagnostic signature.
fn assert_order_independent(
    files: &[(&str, &str)],
    entry: &str,
    opts: CheckerOptions,
) -> Vec<String> {
    let libs = load_default_lib_files();
    let perms = permutations(files);
    let baseline = diagnostic_signature(&check_multi_file_with_libs_stamped(
        &perms[0],
        entry,
        opts.clone(),
        &libs,
    ));
    for (perm_idx, perm) in perms.iter().enumerate() {
        let sig = diagnostic_signature(&check_multi_file_with_libs_stamped(
            perm,
            entry,
            opts.clone(),
            &libs,
        ));
        assert_eq!(
            sig,
            baseline,
            "diagnostics diverged for file order #{perm_idx} = {:?}\n  baseline = {baseline:?}\n  got      = {sig:?}",
            perm.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        );
    }
    baseline
}

/// UMD witness: a bare reference to a `export as namespace` UMD global inside a
/// `checkJs` CommonJS module must report `TS2686` in every file order. The host
/// `.js` file (`app.js`) reaches the UMD guard only when
/// `current_file_is_module_for_umd_global_access` correctly recognizes it as a
/// module regardless of the file's program index.
#[test]
fn umd_global_reference_reports_ts2686_in_every_order() {
    let files = [
        (
            "widgets.d.ts",
            "export as namespace Widgets;\n\
             export interface Keyboard { key: string }\n\
             export function connect(name: string): void;\n",
        ),
        ("dep.d.ts", "declare function f(): string;\nexport = f;\n"),
        (
            "app.js",
            "const dep = require('./dep');\n\
             /** @type {Widgets.Keyboard} */\n\
             var kb;\n\
             Widgets.connect;\n",
        ),
    ];

    let sig = assert_order_independent(&files, "app.js", commonjs_checkjs_opts());
    assert!(
        sig.iter().any(|s| s.starts_with("TS2686@")),
        "expected TS2686 (UMD global used as a value in a module) in every order; got {sig:?}",
    );
}

/// Name-varied UMD witness: different UMD namespace / file / member spellings,
/// proving the rule is not keyed on `Widgets`/`app.js`. Still `TS2686` in every
/// order.
#[test]
fn umd_global_reference_renamed_reports_ts2686_in_every_order() {
    let files = [
        (
            "orm.d.ts",
            "export as namespace Sequelize;\n\
             export interface Model { id: number }\n\
             export function define(name: string): void;\n",
        ),
        (
            "runtime.d.ts",
            "declare function g(): number;\nexport = g;\n",
        ),
        (
            "service.js",
            "const runtime = require('./runtime');\n\
             /** @type {Sequelize.Model} */\n\
             var model;\n\
             Sequelize.define;\n",
        ),
    ];

    let sig = assert_order_independent(&files, "service.js", commonjs_checkjs_opts());
    assert!(
        sig.iter().any(|s| s.starts_with("TS2686@")),
        "expected TS2686 for the renamed UMD global in every order; got {sig:?}",
    );
}
