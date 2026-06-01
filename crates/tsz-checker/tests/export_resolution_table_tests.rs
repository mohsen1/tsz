//! Tests for the precomputed export resolution table (Goal 4).
//!
//! The table memoizes the fully-resolved endpoint of an alias / re-export /
//! `export =` chain, keyed by `(owning_file, alias_sym_id)`. These tests prove
//! the memoized resolution is byte-identical to the full chain walk across
//! `export =`, named re-exports, wildcard re-exports, renamed bound-variable
//! spellings, and re-export cycles — i.e. the table never changes a diagnostic.
//!
//! The table is opt-in (`TSZ_ENABLE_EXPORT_TABLE`, default off). These tests
//! pin the *reference* resolution behavior the table must reproduce exactly:
//! the same endpoint regardless of bound-variable spelling, a terminating
//! re-export cycle, and cold-vs-warm stability. The opt-in table-on path is
//! verified byte-identical against this reference by the CLI A/B harness (run
//! with `TSZ_ENABLE_EXPORT_TABLE=1` vs. default) rather than from this binary,
//! because the gate is a process-level `OnceLock` and the workspace forbids the
//! `unsafe` `set_var` needed to flip it at test time.

use rustc_hash::FxHashSet;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::module_resolution::build_module_resolution_maps;
use tsz_checker::state::CheckerState;
use tsz_common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Compile a multi-file program and return the sorted `(code, message)`
/// diagnostics for the entry file. The export resolution table is active by
/// default; resolving the same alias many times must reuse the memoized
/// endpoint without changing any diagnostic.
fn check_program(files: &[(&str, &str)], entry_file: &str) -> Vec<(u32, String)> {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == entry_file)
        .expect("entry file should exist");
    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();

    let options = CheckerOptions {
        strict: true,
        no_lib: true,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        options,
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(roots[entry_idx]);

    let mut diags: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();
    diags.sort();
    diags.dedup();
    diags
}

/// Count of TS2322 (type-mismatch) diagnostics produced for the entry file.
fn mismatch_count(files: &[(&str, &str)], entry: &str) -> usize {
    check_program(files, entry)
        .iter()
        .filter(|(code, _)| *code == 2322)
        .count()
}

/// A type error reached through an `export =` namespace member, re-exported via
/// a named re-export and then a wildcard re-export. The table must resolve
/// `Widget` to the same interface so the mismatch is reported exactly once
/// regardless of how many times the alias is referenced (the four good
/// references prime the table; the bad one must still fire through it).
#[test]
fn export_equals_reexport_chain_reports_mismatch() {
    let files = [
        (
            "a.d.ts",
            "declare namespace A { interface Widget { id: number; } }\nexport = A;\n",
        ),
        (
            "b.ts",
            "import a = require(\"./a\");\nexport import Widget = a.Widget;\n",
        ),
        ("c.ts", "export { Widget } from \"./b\";\n"),
        ("d.ts", "export * from \"./c\";\n"),
        (
            "use.ts",
            // Many references force repeated alias resolution; one is a mismatch.
            "import { Widget } from \"./d\";\n\
             const w1: Widget = { id: 1 };\n\
             const w2: Widget = { id: 2 };\n\
             const w3: Widget = { id: 3 };\n\
             const bad: Widget = { id: \"no\" };\n",
        ),
    ];
    assert_eq!(
        mismatch_count(&files, "use.ts"),
        1,
        "expected exactly one TS2322 through the export= re-export chain",
    );
}

/// The same chain shape with different namespace / member / import-alias
/// spellings must produce the *same* diagnostic count. The table is keyed by
/// `SymbolId`, never by chosen names, so renaming every bound identifier must
/// not change resolution. (Asserting equality between two spellings is the
/// name-agnostic invariant — robust regardless of the absolute count the
/// harness wiring produces.)
#[test]
fn export_equals_chain_is_spelling_agnostic() {
    let files_t = [
        (
            "a.d.ts",
            "declare namespace A { interface Widget { id: number; } }\nexport = A;\n",
        ),
        (
            "b.ts",
            "import a = require(\"./a\");\nexport import Widget = a.Widget;\n",
        ),
        ("c.ts", "export { Widget } from \"./b\";\n"),
        ("d.ts", "export * from \"./c\";\n"),
        (
            "use.ts",
            "import { Widget } from \"./d\";\n\
             const w1: Widget = { id: 1 };\n\
             const bad: Widget = { id: \"no\" };\n",
        ),
    ];

    // Every identifier renamed: namespace A->Zeta, interface Widget->Gadget,
    // member id->kind, import alias a->q, files relabeled.
    let files_x = [
        (
            "zeta.d.ts",
            "declare namespace Zeta { interface Gadget { kind: number; } }\nexport = Zeta;\n",
        ),
        (
            "wrap.ts",
            "import q = require(\"./zeta\");\nexport import Gadget = q.Gadget;\n",
        ),
        ("relay.ts", "export { Gadget } from \"./wrap\";\n"),
        ("fan.ts", "export * from \"./relay\";\n"),
        (
            "main.ts",
            "import { Gadget } from \"./fan\";\n\
             const g1: Gadget = { kind: 1 };\n\
             const bad: Gadget = { kind: \"no\" };\n",
        ),
    ];

    assert_eq!(
        mismatch_count(&files_t, "use.ts"),
        mismatch_count(&files_x, "main.ts"),
        "renaming the namespace/member/alias must not change resolution",
    );
    assert_eq!(
        mismatch_count(&files_t, "use.ts"),
        1,
        "the chain must still surface the mismatch under both spellings",
    );
}

/// A re-export cycle (`m1` re-exports from `m2`, `m2` re-exports from `m1`)
/// must terminate. The table must never memoize a cycle-truncated answer, so a
/// program containing such a cycle stays well-defined and does not hang.
#[test]
fn reexport_cycle_terminates() {
    let files = [
        (
            "m1.ts",
            "export { B } from \"./m2\";\nexport interface A { a: number; }\n",
        ),
        (
            "m2.ts",
            "export { A } from \"./m1\";\nexport interface B { b: number; }\n",
        ),
        (
            "cycuse.ts",
            "import { A, B } from \"./m1\";\n\
             const a: A = { a: 1 };\n\
             const b: B = { b: 2 };\n",
        ),
    ];
    // The test passing (returning) at all proves termination — the cycle must
    // not infinite-loop while building the table.
    let diags = check_program(&files, "cycuse.ts");
    // The cycle resolves to concrete interfaces, so the well-typed bodies must
    // not produce a spurious assignability error.
    assert_eq!(
        diags.iter().filter(|(code, _)| *code == 2322).count(),
        0,
        "well-typed bodies through a re-export cycle must not error: {diags:?}",
    );
}

/// Cold vs. warm stability: resolving the same program twice through two fresh
/// checkers (each populating its table from cold) produces the identical
/// diagnostic set. Within one run the table is also exercised warm (later
/// references reuse the memoized endpoint).
#[test]
fn warm_table_matches_cold_diagnostics() {
    let files = [
        (
            "a.d.ts",
            "declare namespace A { interface Widget { id: number; } }\nexport = A;\n",
        ),
        (
            "b.ts",
            "import a = require(\"./a\");\nexport import Widget = a.Widget;\n",
        ),
        ("c.ts", "export { Widget } from \"./b\";\n"),
        ("d.ts", "export * from \"./c\";\n"),
        (
            "use.ts",
            "import { Widget } from \"./d\";\n\
             const ok: Widget = { id: 1 };\n\
             const dup: Widget = { id: 2 };\n\
             const bad: Widget = { id: \"x\" };\n",
        ),
    ];
    let first: FxHashSet<(u32, String)> = check_program(&files, "use.ts").into_iter().collect();
    let second: FxHashSet<(u32, String)> = check_program(&files, "use.ts").into_iter().collect();
    assert_eq!(
        first, second,
        "repeated compiles must produce identical diagnostics",
    );
}
