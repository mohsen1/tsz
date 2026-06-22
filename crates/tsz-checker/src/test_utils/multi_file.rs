//! Multi-file project test helpers.
//!
//! Split out of `test_utils` to keep that module under the file-size cap
//! (§19). These build the parse→bind→check pipeline across several in-memory
//! files and exercise the cross-file resolution paths (module resolution,
//! `all_arenas`/`all_binders` overlays, lib contexts, and the production
//! `global_symbol_file_index`).

use crate::context::{CheckerOptions, LibContext};
use crate::diagnostics::Diagnostic;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_parser::parser::ParserState;

/// Parse, bind, and type-check a multi-file project, returning diagnostics for
/// the entry file.
///
/// Use this for cross-file regression tests that rely on import-resolution or
/// cross-file symbol delegation. For tests that only need a single file, prefer
/// [`crate::test_utils::check_source`] / [`crate::test_utils::check_with_options`].
///
/// Like `check_source`, `lib_contexts` is left empty so tests run without lib
/// definitions.
pub fn check_multi_file(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
) -> Vec<Diagnostic> {
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
        .unwrap_or_else(|| panic!("entry_file {entry_file:?} not found in files"));
    let (resolved_module_paths, resolved_modules) =
        crate::module_resolution::build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
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
    checker.ctx.diagnostics.clone()
}

/// Parse, bind, and type-check a multi-file project with the production
/// `global_symbol_file_index` wired up, exactly like the CLI driver.
///
/// [`check_multi_file`] leaves `global_symbol_file_index` empty, so cross-file
/// alias pinning falls back to the dynamic overlay. This variant builds the
/// immutable declaring-file index (`SymbolId -> file_idx`) the same way the
/// driver does (`build_global_symbol_file_index` from `symbol_arenas`), so
/// order-independent alias resolution is exercised on the real path. Used by
/// the order-independence regression tests (refs #7574, #12148).
pub fn check_multi_file_with_global_index(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
) -> Vec<Diagnostic> {
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
        .unwrap_or_else(|| panic!("entry_file {entry_file:?} not found in files"));
    let (resolved_module_paths, resolved_modules) =
        crate::module_resolution::build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);

    // Build the immutable declaring-file index (SymbolId -> file_idx), matching
    // the driver's `build_global_symbol_file_index`. First binder owning a raw
    // SymbolId wins, deterministic by file index.
    let mut symbol_file_index = rustc_hash::FxHashMap::default();
    for (file_idx, binder) in all_binders.iter().enumerate() {
        for symbol in binder.symbols.iter() {
            symbol_file_index.entry(symbol.id).or_insert(file_idx);
        }
    }

    let types = TypeInterner::new();
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
    checker
        .ctx
        .set_global_symbol_file_index(Arc::new(symbol_file_index));

    checker.check_source_file(roots[entry_idx]);
    checker.ctx.diagnostics.clone()
}

/// Parse, bind, and type-check every file in a multi-file project with the
/// production `global_symbol_file_index` wired up.
///
/// This mirrors project checks more closely than entry-only helpers when a
/// regression depends on earlier files populating shared project state.
pub fn check_all_multi_file_with_global_index(
    files: &[(&str, &str)],
    options: CheckerOptions,
) -> Vec<Diagnostic> {
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

    let (resolved_module_paths, resolved_modules) =
        crate::module_resolution::build_module_resolution_maps(&file_names);
    let resolved_module_paths = Arc::new(resolved_module_paths);
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);

    let mut symbol_file_index = rustc_hash::FxHashMap::default();
    for (file_idx, binder) in all_binders.iter().enumerate() {
        for symbol in binder.symbols.iter() {
            symbol_file_index.entry(symbol.id).or_insert(file_idx);
        }
    }
    let symbol_file_index = Arc::new(symbol_file_index);

    let types = TypeInterner::new();
    let mut diagnostics = Vec::new();
    for (file_idx, file_name) in file_names.iter().enumerate() {
        let mut checker = CheckerState::new(
            all_arenas[file_idx].as_ref(),
            all_binders[file_idx].as_ref(),
            &types,
            file_name.clone(),
            options.clone(),
        );
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(file_idx);
        checker.ctx.set_lib_contexts(Vec::new());
        checker
            .ctx
            .set_resolved_module_paths(Arc::clone(&resolved_module_paths));
        checker.ctx.set_resolved_modules(resolved_modules.clone());
        checker
            .ctx
            .set_global_symbol_file_index(Arc::clone(&symbol_file_index));

        checker.check_source_file(roots[file_idx]);
        diagnostics.extend(checker.ctx.diagnostics.clone());
    }

    diagnostics
}

/// Parse, bind, and type-check a multi-file project with lib contexts loaded.
///
/// This is the lib-aware counterpart to [`check_multi_file`]. Each project
/// file is bound through [`tsz_binder::BinderState::bind_source_file_with_libs`],
/// and the checker receives matching `lib_contexts`, so regressions involving
/// local/imported names that conflict with globals (`Boolean`, `String`, ...)
/// exercise the same lookup path as project compiles.
pub fn check_multi_file_with_libs(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
    lib_files: &[Arc<LibFile>],
) -> Vec<Diagnostic> {
    check_multi_file_with_libs_impl(files, entry_file, options, lib_files, false)
}

/// Like [`check_multi_file_with_libs`] but production-faithful with respect to
/// per-file symbol provenance: each binder is given its file index *before*
/// binding, so `BinderState::stamp_file_idx` records every module-local
/// symbol's `decl_file_idx` exactly as the driver's bind-result reducer does.
/// It also wires the `global_symbol_file_index`.
///
/// Tests that depend on `symbol_is_from_actual_or_cloned_lib` (which uses
/// `decl_file_idx` to distinguish module-local symbols from lib globals) must
/// use this helper: the plain [`check_multi_file_with_libs`] leaves
/// `decl_file_idx == u32::MAX`, which makes a module-local symbol that shadows
/// a lib global look like the lib symbol.
pub fn check_multi_file_with_libs_stamped(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
    lib_files: &[Arc<LibFile>],
) -> Vec<Diagnostic> {
    check_multi_file_with_libs_impl(files, entry_file, options, lib_files, true)
}

/// Shared body for [`check_multi_file_with_libs`] and
/// [`check_multi_file_with_libs_stamped`]. When `stamp` is set, each binder is
/// given its file index before binding (so `stamp_file_idx` records
/// `decl_file_idx`) and the `global_symbol_file_index` is wired — matching the
/// production driver. When unset, this is the lib-aware counterpart to
/// [`check_multi_file`] that leaves `decl_file_idx == u32::MAX`.
fn check_multi_file_with_libs_impl(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
    lib_files: &[Arc<LibFile>],
    stamp: bool,
) -> Vec<Diagnostic> {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (file_idx, (name, source)) in files.iter().enumerate() {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        if stamp {
            // Stamp the driver-assigned file index before binding so that
            // `stamp_file_idx` runs at bind end and records `decl_file_idx`.
            binder.set_file_idx(file_idx as u32);
        }
        if lib_files.is_empty() {
            binder.bind_source_file(parser.get_arena(), root);
        } else {
            binder.bind_source_file_with_libs(parser.get_arena(), root, lib_files);
        }
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == entry_file)
        .unwrap_or_else(|| panic!("entry_file {entry_file:?} not found in files"));
    let (resolved_module_paths, resolved_modules) =
        crate::module_resolution::build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
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
    if lib_files.is_empty() {
        checker.ctx.set_lib_contexts(Vec::new());
    } else {
        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    if stamp {
        // Build the immutable declaring-file index (SymbolId -> file_idx) like
        // the driver's `build_global_symbol_file_index`.
        let mut symbol_file_index = rustc_hash::FxHashMap::default();
        for (file_idx, binder) in all_binders.iter().enumerate() {
            for symbol in binder.symbols.iter() {
                symbol_file_index.entry(symbol.id).or_insert(file_idx);
            }
        }
        checker
            .ctx
            .set_global_symbol_file_index(Arc::new(symbol_file_index));
    }

    checker.check_source_file(roots[entry_idx]);
    checker.ctx.diagnostics.clone()
}

/// T2.2 test helper: parse, bind, type-check a multi-file project AND return
/// the populated `cross_file_type_params_cache` for assertion. The cache is
/// installed before the check runs and is the same `Arc<DashMap>` returned
/// to the caller, so assertions can inspect what the checker memoized
/// during the run.
///
/// Used by tests that need to prove the cross-file type-parameter
/// memoization (`PERFORMANCE_PLAN.md` §7) actually populated.
pub fn check_multi_file_with_type_params_cache(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
) -> (Vec<Diagnostic>, crate::context::CrossFileTypeParamsCache) {
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
        .unwrap_or_else(|| panic!("entry_file {entry_file:?} not found in files"));
    let (resolved_module_paths, resolved_modules) =
        crate::module_resolution::build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
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

    let cache = Arc::new(dashmap::DashMap::new());
    checker.ctx.cross_file_type_params_cache = Some(Arc::clone(&cache));

    checker.check_source_file(roots[entry_idx]);
    (checker.ctx.diagnostics.clone(), cache)
}
