//! Unit tests documenting known conformance test failures
//!
//! These tests are marked `#[ignore]` and document specific issues found during
//! conformance test investigation (2026-02-08). They serve as:
//! - Documentation of expected vs actual behavior
//! - Easy verification when fixes are implemented
//! - Minimal reproduction cases for debugging
//!
//! See docs/conformance-*.md for full context.

pub(crate) use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
pub(crate) use std::sync::Arc;
pub(crate) use tsz_binder::BinderState;
pub(crate) use tsz_binder::lib_loader::LibFile;
pub(crate) use tsz_binder::state::LibContext as BinderLibContext;
pub(crate) use tsz_checker::context::LibContext as CheckerLibContext;
pub(crate) use tsz_checker::context::{CheckerOptions, ScriptTarget};
pub(crate) use tsz_checker::module_resolution::build_module_resolution_maps;
pub(crate) use tsz_checker::state::CheckerState;
pub(crate) use tsz_common::ModuleKind;
pub(crate) use tsz_common::checker_options::JsxMode;
pub(crate) use tsz_parser::parser::ParserState;
pub(crate) use tsz_solver::construction::TypeInterner;

/// Helper to compile TypeScript and get diagnostics
pub(crate) fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    compile_and_get_diagnostics_with_options(source, CheckerOptions::default())
}

pub(crate) fn compile_two_files_get_diagnostics(
    a_source: &str,
    b_source: &str,
    module_spec: &str,
) -> Vec<(u32, String)> {
    compile_two_files_get_diagnostics_with_options(
        a_source,
        b_source,
        module_spec,
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            no_lib: true,
            ..Default::default()
        },
    )
}

pub(crate) fn compile_two_files_get_diagnostics_with_options(
    a_source: &str,
    b_source: &str,
    module_spec: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser_a = ParserState::new("a.ts".to_string(), a_source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new("b.ts".to_string(), b_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let arena_a = Arc::new(parser_a.get_arena().clone());
    let arena_b = Arc::new(parser_b.get_arena().clone());

    let all_arenas = Arc::new(vec![Arc::clone(&arena_a), Arc::clone(&arena_b)]);

    // Merge module exports: copy a.ts exports into b.ts's binder for cross-file resolution
    let file_a_exports = binder_a.module_exports.get("a.ts").cloned();
    if let Some(exports) = &file_a_exports {
        std::sync::Arc::make_mut(&mut binder_b.module_exports)
            .insert(module_spec.to_string(), exports.clone());
    }

    // Record cross-file symbol targets: SymbolIds from binder_a need to resolve
    // in binder_a's arena, not binder_b's. Map them to file index 0 (a.ts).
    let mut cross_file_targets = FxHashMap::default();
    if let Some(exports) = &file_a_exports {
        for (_name, &sym_id) in exports.iter() {
            cross_file_targets.insert(sym_id, 0usize);
        }
    }

    let binder_a = Arc::new(binder_a);
    let binder_b = Arc::new(binder_b);
    let all_binders = Arc::new(vec![Arc::clone(&binder_a), Arc::clone(&binder_b)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_b.as_ref(),
        binder_b.as_ref(),
        &types,
        "b.ts".to_string(),
        options,
    );
    checker.enable_source_file_test_pragmas();

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);

    // Register cross-file symbol targets so the checker looks up SymbolIds
    // from a.ts in the correct binder (file index 0).
    for (sym_id, file_idx) in &cross_file_targets {
        checker.ctx.register_symbol_file_target(*sym_id, *file_idx);
    }

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, module_spec.to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert(module_spec.to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_b);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

pub(crate) fn compile_named_files_get_diagnostics_with_options(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    compile_named_files_get_diagnostics_with_options_and_import_reporting(
        files, entry_file, options, false,
    )
}

pub(crate) fn compile_named_files_get_diagnostics_with_options_and_import_reporting(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
    report_unresolved_imports: bool,
) -> Vec<(u32, String)> {
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
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        options,
    );
    checker.enable_source_file_test_pragmas();

    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.report_unresolved_imports = report_unresolved_imports;

    checker.check_source_file(roots[entry_idx]);

    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

pub(crate) fn compile_named_project_get_diagnostics_with_options(
    files: &[(&str, &str)],
    options: CheckerOptions,
) -> Vec<(u32, String)> {
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

    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
    let resolved_module_paths = Arc::new(resolved_module_paths);
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
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
        checker.enable_source_file_test_pragmas();
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(file_idx);
        checker.ctx.set_lib_contexts(Vec::new());
        checker
            .ctx
            .set_resolved_module_paths(Arc::clone(&resolved_module_paths));
        checker.ctx.set_resolved_modules(resolved_modules.clone());
        checker.check_source_file(roots[file_idx]);
        diagnostics.extend(
            checker
                .ctx
                .diagnostics
                .iter()
                .filter(|d| d.code != 2318)
                .map(|d| (d.code, d.message_text.clone())),
        );
    }

    diagnostics
}

pub(crate) fn compile_named_files_get_diagnostics_with_lib_and_options(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let lib_files = load_lib_files_for_test();
    compile_named_files_get_diagnostics_with_lib_files_and_options(
        files, entry_file, options, lib_files, false,
    )
}

/// Production-faithful variant of
/// [`compile_named_files_get_diagnostics_with_lib_and_options`]: each binder is
/// given its file index *before* binding (so `stamp_file_idx` records every
/// module-local symbol's `decl_file_idx`) and the `global_symbol_file_index` is
/// wired, exactly like the CLI driver's bind-result reducer.
///
/// The plain (unstamped) variant leaves `decl_file_idx == u32::MAX`, which makes
/// a module-local symbol that shadows a lib global (e.g. an imported value named
/// `Promise`) look like the lib symbol. Witnesses that turn on that distinction —
/// notably lib-global shadowing in call position (#14263) — must use this helper
/// so the harness matches production symbol provenance rather than reporting a
/// harness-only false positive.
pub(crate) fn compile_named_files_get_diagnostics_with_lib_and_options_stamped(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let lib_files = load_lib_files_for_test();
    compile_named_files_get_diagnostics_with_lib_files_and_options(
        files, entry_file, options, lib_files, true,
    )
}

pub(crate) fn compile_named_files_get_diagnostics_with_compiled_libs_and_options(
    files: &[(&str, &str)],
    entry_file: &str,
    lib_names: &[&str],
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let lib_files = tsz_checker::test_utils::load_compiled_lib_files(lib_names);
    compile_named_files_get_diagnostics_with_lib_files_and_options(
        files, entry_file, options, lib_files, false,
    )
}

fn compile_named_files_get_diagnostics_with_lib_files_and_options(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
    lib_files: Vec<Arc<LibFile>>,
    stamp: bool,
) -> Vec<(u32, String)> {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    let raw_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| BinderLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    let checker_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| CheckerLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();

    for (file_idx, (name, source)) in files.iter().enumerate() {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        if stamp {
            // Stamp the driver-assigned file index before binding so
            // `stamp_file_idx` (run at bind end) records every module-local
            // symbol's `decl_file_idx`, exactly as the production driver's
            // bind-result reducer does. Without this, `decl_file_idx == u32::MAX`
            // and a module-local symbol that shadows a lib global (e.g. an
            // imported value named `Promise`) is misclassified as the lib symbol,
            // producing a harness-only false positive (#14263).
            binder.set_file_idx(file_idx as u32);
        }
        if !raw_lib_contexts.is_empty() {
            binder.merge_lib_contexts_into_binder(&raw_lib_contexts);
        }
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
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        options,
    );
    checker.enable_source_file_test_pragmas();

    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    if stamp {
        // Build the immutable declaring-file index (`SymbolId -> file_idx`) like
        // the production driver's `build_global_symbol_file_index`, so cross-file
        // symbol provenance (and lib-global shadowing) matches the real pipeline.
        //
        // Only module-local symbols are mapped: `stamp_file_idx` (run at bind
        // end) leaves lib symbols at `decl_file_idx == u32::MAX`, so they are
        // excluded here. Mapping a merged lib symbol to a user file would
        // mis-provenance lib globals (e.g. make `console` / node-builtin lookups
        // look user-declared).
        let symbol_capacity: usize = all_binders.iter().map(|b| b.symbols.len()).sum();
        let mut symbol_file_index =
            FxHashMap::with_capacity_and_hasher(symbol_capacity, FxBuildHasher);
        for (file_idx, binder) in all_binders.iter().enumerate() {
            for symbol in binder.symbols.iter() {
                if symbol.decl_file_idx == u32::MAX {
                    continue;
                }
                symbol_file_index.entry(symbol.id).or_insert(file_idx);
            }
        }
        checker
            .ctx
            .set_global_symbol_file_index(Arc::new(symbol_file_index));
    }
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    if !checker_lib_contexts.is_empty() {
        checker.ctx.set_lib_contexts(checker_lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(roots[entry_idx]);

    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

pub(crate) fn compile_and_get_diagnostics_with_merged_lib_contexts_and_shared_cache_and_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let lib_files = load_lib_files_for_test();

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let checker_lib_contexts = if lib_files.is_empty() {
        Vec::new()
    } else {
        let raw_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&raw_contexts);
        vec![CheckerLibContext {
            arena: Arc::clone(&lib_files[0].arena),
            binder: Arc::new({
                let mut merged = BinderState::new();
                merged.merge_lib_contexts_into_binder(&raw_contexts);
                merged
            }),
        }]
    };
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.enable_source_file_test_pragmas();

    if !checker_lib_contexts.is_empty() {
        checker.ctx.set_lib_contexts(checker_lib_contexts);
    }
    checker.ctx.shared_lib_type_cache = Some(Arc::new(dashmap::DashMap::new()));

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

pub(crate) fn compile_imports_and_get_diagnostics(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.enable_source_file_test_pragmas();
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

pub(crate) fn compile_and_get_diagnostics_with_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    compile_and_get_diagnostics_named("test.ts", source, options)
}

pub(crate) fn compile_and_get_diagnostics_named(
    file_name: &str,
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    compile_and_get_raw_diagnostics_named(file_name, source, options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

pub(crate) fn compile_and_get_raw_diagnostics_named(
    file_name: &str,
    source: &str,
    options: CheckerOptions,
) -> Vec<tsz_common::diagnostics::Diagnostic> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    checker.enable_source_file_test_pragmas();
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(root);

    checker.ctx.diagnostics
}

/// Helper to check if specific error codes are present
pub(crate) fn has_error(diagnostics: &[(u32, String)], code: u32) -> bool {
    diagnostics.iter().any(|(c, _)| *c == code)
}

pub(crate) fn diagnostic_message(diagnostics: &[(u32, String)], code: u32) -> Option<&str> {
    diagnostics
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, message)| message.as_str())
}

pub(crate) fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    tsz_checker::test_utils::load_default_lib_files()
}

pub(crate) fn lib_files_available() -> bool {
    !load_lib_files_for_test().is_empty()
}

pub(crate) fn without_missing_global_type_errors(
    diagnostics: Vec<(u32, String)>,
) -> Vec<(u32, String)> {
    diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect()
}

pub(crate) fn compile_and_get_diagnostics_with_lib(source: &str) -> Vec<(u32, String)> {
    compile_and_get_diagnostics_with_lib_and_options(source, CheckerOptions::default())
}

pub(crate) fn compile_and_get_diagnostics_with_lib_and_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    compile_and_get_diagnostics_named_with_lib_and_options("test.ts", source, options)
}

pub(crate) fn compile_and_get_diagnostics_named_with_lib_and_options(
    file_name: &str,
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    compile_and_get_raw_diagnostics_named_with_lib_and_options(file_name, source, options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

pub(crate) fn compile_and_get_raw_diagnostics_named_with_lib_and_options(
    file_name: &str,
    source: &str,
    options: CheckerOptions,
) -> Vec<tsz_common::diagnostics::Diagnostic> {
    let lib_files = load_lib_files_for_test();

    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let checker_lib_contexts = if lib_files.is_empty() {
        Vec::new()
    } else {
        let raw_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&raw_contexts);
        lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect()
    };
    // Match the CLI/LSP convention: assign the user file a stable file_idx
    // (0) so checker-side `def_file_idx` lookups can distinguish user-defined
    // aliases from merged-in lib symbols, matching how the conformance and
    // single-file CLI runners stamp `DefinitionInfo.file_id`.
    binder.set_file_idx(0);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    checker.enable_source_file_test_pragmas();

    if !checker_lib_contexts.is_empty() {
        checker.ctx.set_lib_contexts(checker_lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics
}

pub(crate) fn compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let lib_files = load_lib_files_for_test();

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let checker_lib_contexts = if lib_files.is_empty() {
        Vec::new()
    } else {
        let raw_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&raw_contexts);
        lib_files
            .iter()
            .map(|lib| CheckerLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect()
    };
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.enable_source_file_test_pragmas();

    if !checker_lib_contexts.is_empty() {
        checker.ctx.set_lib_contexts(checker_lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[cfg(test)]
#[path = "helpers/regression_tests.rs"]
mod regression_tests;
