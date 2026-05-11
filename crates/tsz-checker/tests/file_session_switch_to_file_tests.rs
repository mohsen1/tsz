//! T2.1.B foundation: lock the `CheckerContext::switch_to_file` API.
//!
//! These tests prove a single `CheckerContext` can be re-targeted at
//! successive files without state leaking from one file's check into the
//! next. The intended caller is a future sequential session-reuse path in
//! `crates/tsz-cli/src/driver/check.rs` (T2.1.B PR part 2), which will
//! pre-build all binders, construct one `CheckerState`, and call
//! `switch_to_file` between files instead of dropping and reconstructing
//! the checker per file.
//!
//! The `PERFORMANCE_PLAN.md` §6 staged-PR table item T2.1.B requires:
//!
//! > Full conformance with flag produces byte-identical diagnostics to
//! > default path.
//!
//! This module locks the byte-identical-diagnostics invariant **at the
//! `CheckerContext` API level**, independently of the driver wire-up.
//! Concretely: for any two-file project, the diagnostics produced by
//! checking each file through one `CheckerContext` (with `switch_to_file`
//! between them) must match the diagnostics produced by checking each
//! file through its own fresh `CheckerContext`.

use crate::context::{CheckerContext, CheckerOptions};
use crate::diagnostics::Diagnostic;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::{NodeIndex, ParserState, node::NodeArena};

struct ParsedFile {
    arena: Arc<NodeArena>,
    binder: Arc<BinderState>,
    root: NodeIndex,
}

fn parse_and_bind(name: &str, source: &str) -> ParsedFile {
    let mut parser = ParserState::new(name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    ParsedFile {
        arena: Arc::new(parser.get_arena().clone()),
        binder: Arc::new(binder),
        root,
    }
}

/// Build a `CheckerState` whose `arena`/`binder` borrow from `parsed`. The
/// returned checker also has the cross-file `all_arenas`/`all_binders`
/// vectors set up so cross-file lookups work; the caller decides which
/// file the checker is currently focused on via `current_file_idx`.
fn fresh_checker<'a>(
    parsed: &'a ParsedFile,
    types: &'a TypeInterner,
    file_name: String,
    all_arenas: Arc<Vec<Arc<NodeArena>>>,
    all_binders: Arc<Vec<Arc<BinderState>>>,
    file_idx: usize,
    options: CheckerOptions,
) -> CheckerState<'a> {
    let mut checker = CheckerState::new(
        parsed.arena.as_ref(),
        parsed.binder.as_ref(),
        types,
        file_name,
        options,
    );
    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(file_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
}

fn diagnostics_signature(diags: &[Diagnostic]) -> Vec<(u32, u32, String)> {
    diags
        .iter()
        .map(|d| (d.code, d.start, d.message_text.clone()))
        .collect()
}

#[test]
fn switch_to_file_yields_byte_identical_intra_file_diagnostics() {
    // Two completely independent files, each with intra-file type errors
    // that don't depend on cross-file resolution. The reused-checker path
    // must produce the same diagnostics as the fresh-checker path for
    // the simple case of `const x: T = wrong_T;` style mismatches.
    //
    // **Scope note**: this test uses independent binders (each file has
    // its own `BinderState` built from a fresh `parse_and_bind` call).
    // Production checking uses a shared binder + shared `DefinitionStore`
    // for cross-file SymbolId/DefId stability; that production setup is
    // exercised by the driver-side T2.1.B part 2 PR's integration tests,
    // not this API-level lock. Therefore this test deliberately limits
    // the source to errors that do not pivot on cross-file symbol
    // resolution: each `const` is a self-contained primitive-vs-literal
    // mismatch.
    let file_a = parse_and_bind(
        "a.ts",
        r#"
        const a1: number = "not a number";
        const a2: number = true;
        "#,
    );
    let file_b = parse_and_bind(
        "b.ts",
        r#"
        const b1: boolean = 123;
        const b2: boolean = "no";
        "#,
    );

    let all_arenas: Arc<Vec<Arc<NodeArena>>> =
        Arc::new(vec![Arc::clone(&file_a.arena), Arc::clone(&file_b.arena)]);
    let all_binders: Arc<Vec<Arc<BinderState>>> =
        Arc::new(vec![Arc::clone(&file_a.binder), Arc::clone(&file_b.binder)]);

    let opts = CheckerOptions::default();

    // Path 1 — fresh CheckerState per file (the current driver behavior).
    let (fresh_diags_a, fresh_diags_b) = {
        let types = TypeInterner::new();
        let mut checker_a = fresh_checker(
            &file_a,
            &types,
            "a.ts".to_string(),
            Arc::clone(&all_arenas),
            Arc::clone(&all_binders),
            0,
            opts.clone(),
        );
        checker_a.check_source_file(file_a.root);
        let diags_a = checker_a.ctx.diagnostics.clone();
        drop(checker_a);

        let mut checker_b = fresh_checker(
            &file_b,
            &types,
            "b.ts".to_string(),
            Arc::clone(&all_arenas),
            Arc::clone(&all_binders),
            1,
            opts.clone(),
        );
        checker_b.check_source_file(file_b.root);
        let diags_b = checker_b.ctx.diagnostics.clone();
        (diags_a, diags_b)
    };

    // Path 2 — one CheckerState, `switch_to_file` between files. Must produce
    // the same diagnostics as Path 1.
    let (reused_diags_a, reused_diags_b) = {
        let types = TypeInterner::new();
        let mut checker = fresh_checker(
            &file_a,
            &types,
            "a.ts".to_string(),
            Arc::clone(&all_arenas),
            Arc::clone(&all_binders),
            0,
            opts,
        );
        checker.check_source_file(file_a.root);
        let diags_a = checker.ctx.diagnostics.clone();

        checker.ctx.switch_to_file(
            file_b.arena.as_ref(),
            file_b.binder.as_ref(),
            "b.ts".to_string(),
            1,
        );
        checker.check_source_file(file_b.root);
        let diags_b = checker.ctx.diagnostics.clone();
        (diags_a, diags_b)
    };

    // Both files must have produced at least one diagnostic — otherwise the
    // test isn't actually exercising the path it claims to.
    assert!(
        !fresh_diags_a.is_empty(),
        "fresh-path a.ts must emit at least one diagnostic for this test to be load-bearing",
    );
    assert!(
        !fresh_diags_b.is_empty(),
        "fresh-path b.ts must emit at least one diagnostic for this test to be load-bearing",
    );

    assert_eq!(
        diagnostics_signature(&fresh_diags_a),
        diagnostics_signature(&reused_diags_a),
        "file A diagnostics differ between fresh-per-file and switch_to_file paths",
    );
    assert_eq!(
        diagnostics_signature(&fresh_diags_b),
        diagnostics_signature(&reused_diags_b),
        "file B diagnostics differ between fresh-per-file and switch_to_file paths",
    );
}

#[test]
fn switch_to_file_clears_previous_file_diagnostics() {
    // After `switch_to_file`, the previous file's diagnostics must be gone
    // — otherwise the reused checker would accumulate cross-file
    // diagnostics on `ctx.diagnostics` and the driver wire-up (T2.1.B
    // part 2) couldn't simply read `ctx.diagnostics` per file like the
    // construction-per-file path does.
    let file_a = parse_and_bind("a.ts", r#"const a: number = "not a number";"#);
    let file_b = parse_and_bind("b.ts", r#"const b: number = 42;"#);

    let all_arenas: Arc<Vec<Arc<NodeArena>>> =
        Arc::new(vec![Arc::clone(&file_a.arena), Arc::clone(&file_b.arena)]);
    let all_binders: Arc<Vec<Arc<BinderState>>> =
        Arc::new(vec![Arc::clone(&file_a.binder), Arc::clone(&file_b.binder)]);

    let types = TypeInterner::new();
    let mut checker = fresh_checker(
        &file_a,
        &types,
        "a.ts".to_string(),
        Arc::clone(&all_arenas),
        Arc::clone(&all_binders),
        0,
        CheckerOptions::default(),
    );
    checker.check_source_file(file_a.root);
    assert!(
        !checker.ctx.diagnostics.is_empty(),
        "a.ts must emit at least one diagnostic for this test to be load-bearing",
    );

    checker.ctx.switch_to_file(
        file_b.arena.as_ref(),
        file_b.binder.as_ref(),
        "b.ts".to_string(),
        1,
    );
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "switch_to_file must drain the previous file's diagnostics",
    );
    assert_eq!(checker.ctx.file_name, "b.ts");
    assert_eq!(checker.ctx.current_file_idx, 1);
    assert!(
        std::ptr::eq(checker.ctx.arena, file_b.arena.as_ref()),
        "switch_to_file must rebind the arena reference",
    );
    assert!(
        std::ptr::eq(checker.ctx.binder, file_b.binder.as_ref()),
        "switch_to_file must rebind the binder reference",
    );
}

#[test]
fn switch_to_file_then_check_emits_only_the_new_file_diagnostics() {
    // Belt-and-suspenders against the worst regression this API could
    // hide: after `switch_to_file`, checking the new file must emit only
    // diagnostics that belong to the new file. A bad implementation that
    // forgot to clear `request_node_types` (file-local NodeIndex-keyed
    // cache) could return wrong results without emitting wrong-file
    // diagnostics. Diagnostics-equality alone doesn't catch that — but
    // re-running with two CheckerStates and comparing IS the strongest
    // available test, which `switch_to_file_yields_byte_identical_diagnostics_to_fresh_per_file`
    // covers. This test adds the simpler explicit check: the second
    // file's `file_name` appears in every emitted diagnostic.
    let file_a = parse_and_bind("a.ts", r#"const a: number = "wrong";"#);
    let file_b = parse_and_bind("b.ts", r#"const b: boolean = 7;"#);

    let all_arenas: Arc<Vec<Arc<NodeArena>>> =
        Arc::new(vec![Arc::clone(&file_a.arena), Arc::clone(&file_b.arena)]);
    let all_binders: Arc<Vec<Arc<BinderState>>> =
        Arc::new(vec![Arc::clone(&file_a.binder), Arc::clone(&file_b.binder)]);

    let types = TypeInterner::new();
    let mut checker = fresh_checker(
        &file_a,
        &types,
        "a.ts".to_string(),
        Arc::clone(&all_arenas),
        Arc::clone(&all_binders),
        0,
        CheckerOptions::default(),
    );
    checker.check_source_file(file_a.root);
    let _ = std::mem::take(&mut checker.ctx.diagnostics);

    checker.ctx.switch_to_file(
        file_b.arena.as_ref(),
        file_b.binder.as_ref(),
        "b.ts".to_string(),
        1,
    );
    checker.check_source_file(file_b.root);

    assert!(
        !checker.ctx.diagnostics.is_empty(),
        "b.ts must emit at least one diagnostic post-switch",
    );
    for d in &checker.ctx.diagnostics {
        assert_eq!(
            d.file, "b.ts",
            "post-switch diagnostic anchors at wrong file: {d:?}"
        );
    }
}

// Compile-time check: the method signature requires both `arena` and
// `binder` to share the `CheckerContext<'a>` lifetime `'a`. This catches a
// future refactor that loosens the constraint and lets the caller pass
// stack-allocated arena/binder that don't outlive the context.
const _: fn(&mut CheckerContext<'static>, &'static NodeArena, &'static BinderState, String, usize) =
    CheckerContext::switch_to_file;
