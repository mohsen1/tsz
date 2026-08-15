//! Lib-backed single-file test helpers.
//!
//! Split out of `test_utils` to keep that module under the file-size cap
//! (§19). These wire `lib.d.ts` contexts into the parse→bind→check pipeline
//! and expose the two harness fidelities: the default plain
//! [`CheckerState::new`] path and the production-faithful shared-
//! [`DefinitionStore`](tsz_solver::def::DefinitionStore) path
//! ([`check_source_with_libs_shared_def_store`], issue #16125).
//!
//! As a child of `test_utils`, this module can call the parent's private
//! pipeline helpers (`with_checked_source`, `diagnostic_code_messages`)
//! directly via `super::`.

use crate::context::{CheckerOptions, LibContext};
use crate::diagnostics::Diagnostic;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_parser::parser::ParserState;

/// Parse, bind, and type-check `source` with the given `lib_files` wired
/// into the binder and checker.
///
/// Mirrors [`super::check_source`] but routes through
/// [`tsz_binder::BinderState::bind_source_file_with_libs`] and
/// `Context::set_lib_contexts` / `set_actual_lib_file_count`. Use this
/// when tests rely on built-in types (`Promise`, `Array`, `Symbol`,
/// DOM, …); for tests that don't need libs, prefer [`super::check_source`]
/// which is faster.
///
/// Like [`super::check_source`], calls `enable_source_file_test_pragmas()` so
/// `// @ts-expect-error`-style pragmas are honored.
pub fn check_source_with_libs(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    lib_files: &[Arc<LibFile>],
) -> Vec<Diagnostic> {
    diagnostics_with_libs(source, file_name, options, lib_files, false)
}

/// Like [`check_source_with_libs`], but constructs the checker with a shared
/// [`DefinitionStore`](tsz_solver::def::DefinitionStore), exactly as the
/// production driver does (`crates/tsz-core/src/parallel/core/checking.rs`):
/// the store is pre-populated from the binder's semantic defs and attached to
/// the `QueryCache`.
///
/// Reach for this in a unit test whose fixture depends on the solver being
/// able to unify a **lib generic's** base declaration across the user arena
/// and the lib arena — the `DefId`-keyed cross-arena identity of issue #14344
/// (`TSZ_XARENA_BASE_DECL`). Without the store (the plain
/// [`check_source_with_libs`] path) that unification is unavailable, so a lib
/// generic's variance cannot be measured and a same-base relation can fall
/// back to a lossy structural walk — e.g. a self-similar
/// `AsyncGenerator<AsyncGenerator<string>>` vs
/// `AsyncGenerator<AsyncGenerator<number>>` mismatch is silently accepted in
/// the plain harness while the real CLI reports it (issue #16125).
///
/// This is opt-in rather than the default because attaching a shared store to
/// **every** lib-backed unit check is a broad behavior change across the
/// hundreds of existing callers (both correctness — latent false negatives the
/// plain path currently hides would begin to fire — and wall time) that is not
/// validated here; promoting it to the default is tracked as #16125 follow-up.
/// Use this where the cross-arena fidelity is the property under test.
pub fn check_source_with_libs_shared_def_store(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    lib_files: &[Arc<LibFile>],
) -> Vec<Diagnostic> {
    diagnostics_with_libs(source, file_name, options, lib_files, true)
}

/// Shared body for the two diagnostics-only entry points: routes the no-libs
/// fast path to [`super::with_checked_source`] and otherwise runs the
/// lib-wired pipeline, selecting the plain or shared-`DefinitionStore` checker
/// construction via `use_shared_def_store`.
fn diagnostics_with_libs(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    lib_files: &[Arc<LibFile>],
    use_shared_def_store: bool,
) -> Vec<Diagnostic> {
    if lib_files.is_empty() {
        return super::with_checked_source(source, file_name, options, None, |checker| {
            checker.ctx.diagnostics.clone()
        });
    }
    with_checked_source_with_libs(
        source,
        file_name,
        options,
        lib_files,
        use_shared_def_store,
        |checker, _types| checker.ctx.diagnostics.clone(),
    )
}

/// Run the canonical parse → bind → check pipeline **with `lib_files` wired
/// in**, handing the post-check `CheckerState` and the live [`TypeInterner`]
/// to `extract`. Shared body for the libs-based public helpers so any change
/// to lib-context setup applies to all of them, and so callers that need the
/// interner (e.g. type-count probes) don't have to copy the pipeline.
///
/// `use_shared_def_store` selects the checker construction: `false` uses a
/// plain [`CheckerState::new`] backed by a bare [`TypeInterner`]; `true`
/// mirrors the production driver by pre-populating a shared
/// [`DefinitionStore`](tsz_solver::def::DefinitionStore) from the binder's
/// semantic defs and attaching it to both the `QueryCache` and the checker.
/// See [`check_source_with_libs_shared_def_store`] and issue #16125 for why
/// that matters.
fn with_checked_source_with_libs<R>(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    lib_files: &[Arc<LibFile>],
    use_shared_def_store: bool,
    extract: impl FnOnce(&CheckerState<'_>, &TypeInterner) -> R,
) -> R {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let source_file = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), source_file, lib_files);

    let types = TypeInterner::new();
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();

    // Deferred-init so the shared branch's `definition_store`/`query_cache`
    // outlive the checker that borrows them, while the plain branch leaves
    // them unassigned (and unread).
    let definition_store;
    let query_cache;
    let mut checker = if use_shared_def_store {
        definition_store = Arc::new(tsz_solver::def::DefinitionStore::from_semantic_defs(
            &binder.semantic_defs,
            |s| types.intern_string(s),
        ));
        query_cache = tsz_solver::construction::QueryCache::new(&types)
            .with_definition_store(&definition_store);
        CheckerState::new_with_shared_def_store(
            parser.get_arena(),
            &binder,
            &query_cache,
            file_name.to_string(),
            options,
            Arc::clone(&definition_store),
        )
    } else {
        CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            file_name.to_string(),
            options,
        )
    };

    checker.enable_source_file_test_pragmas();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker.check_source_file(source_file);
    extract(&checker, &types)
}

/// Parse, bind, and type-check `source` with `lib_files`, returning the
/// diagnostics alongside the number of types interned during the check
/// ([`TypeInterner::len`]).
///
/// This is the in-process analogue of `tsz --extendedDiagnostics`'s
/// "Types" counter: it exposes how much of the lib-type graph a check
/// materialized. Lazy lib-interface heritage/member work (#12101, #13933,
/// #13935, #13936) is measured exactly by this count — a regression that
/// re-eagerly materializes a receiver's transitive `extends` closure shows
/// up here as a multi-thousand-type jump even when diagnostics stay
/// byte-identical. The absolute value depends on the bundled stripped lib
/// assets (so it differs from the `dist` binary's full-lib numbers), but it
/// is deterministic for a fixed lib set, which is what a regression guard
/// needs.
pub fn check_source_with_libs_type_count(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    lib_files: &[Arc<LibFile>],
) -> (Vec<Diagnostic>, usize) {
    with_checked_source_with_libs(
        source,
        file_name,
        options,
        lib_files,
        false,
        |checker, types| (checker.ctx.diagnostics.clone(), types.len()),
    )
}

/// `(code, message_text)` projection of [`check_source_with_libs`].
pub fn check_source_with_libs_code_messages(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    lib_files: &[Arc<LibFile>],
) -> Vec<(u32, String)> {
    super::diagnostic_code_messages(check_source_with_libs(
        source, file_name, options, lib_files,
    ))
}
