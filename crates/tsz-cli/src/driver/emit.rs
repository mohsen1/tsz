use anyhow::{Context, Result};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};

use super::resolution::{
    canonicalize_or_owned, canonicalize_with_missing_tail, implied_resolution_mode_for_file,
    is_declaration_file, normalize_path,
};
use crate::config::{JsxEmit, ResolvedCompilerOptions};
use tsz::declaration_emitter::DeclarationEmitter;
use tsz::emitter::{NewLineKind, Printer};
use tsz::enums::evaluator::{EnumEvaluator, EnumValue};
use tsz::parallel::{BoundFile, MergedProgram};
use tsz_common::common::ModuleKind;
use tsz_common::diagnostics::{Diagnostic, DiagnosticCategory};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

#[derive(Debug, Clone)]
pub(crate) struct OutputFile {
    pub(crate) path: PathBuf,
    pub(crate) contents: String,
    pub(crate) source_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct DeclarationBundleChunk {
    path_key: String,
    referenced_path_keys: Vec<String>,
    contents: String,
}

pub(crate) struct EmitOutputsContext<'a> {
    pub(crate) program: &'a MergedProgram,
    pub(crate) options: &'a ResolvedCompilerOptions,
    pub(crate) base_dir: &'a Path,
    pub(crate) root_dir: Option<&'a Path>,
    pub(crate) out_dir: Option<&'a Path>,
    pub(crate) declaration_dir: Option<&'a Path>,
    pub(crate) dirty_paths: Option<&'a FxHashSet<PathBuf>>,
    pub(crate) type_caches: &'a FxHashMap<std::path::PathBuf, tsz::checker::TypeCache>,
}

pub(crate) fn emit_outputs(
    context: EmitOutputsContext<'_>,
) -> Result<(Vec<OutputFile>, Vec<tsz_common::diagnostics::Diagnostic>)> {
    let mut outputs = Vec::new();
    let mut emit_diagnostics: Vec<tsz_common::diagnostics::Diagnostic> = Vec::new();
    let new_line = new_line_str(context.options.printer.new_line);
    let declaration_bundle_path = context.options.out_file.as_ref().and_then(|out_file| {
        declaration_bundle_output_path(
            context.base_dir,
            context.declaration_dir.or(context.out_dir),
            out_file,
        )
    });
    let mut declaration_bundle_chunks = Vec::new();
    let mut declaration_bundle_blocked = false;

    // When --outFile is set, collect JS chunks and concatenate at the end
    // instead of emitting individual files. TypeScript honors outFile even for
    // a single source file.
    let js_bundle_path = context.options.out_file.as_ref().map(|out_file| {
        if out_file.is_absolute() {
            out_file.to_path_buf()
        } else {
            context.base_dir.join(out_file)
        }
    });
    let mut js_bundle_chunks: Vec<String> = Vec::new();

    // Build mapping from arena address to file path for module resolution
    let arena_to_path: rustc_hash::FxHashMap<usize, String> = context
        .program
        .files
        .iter()
        .map(|file| {
            let arena_addr = std::sync::Arc::as_ptr(&file.arena) as usize;
            (arena_addr, file.file_name.clone())
        })
        .collect();

    // Build mapping from file index to file path for decl_file_idx-based
    // symbol source resolution (fallback when symbol_arenas is incomplete)
    let file_idx_to_path: rustc_hash::FxHashMap<u32, String> = context
        .program
        .files
        .iter()
        .enumerate()
        .map(|(idx, file)| (idx as u32, file.file_name.clone()))
        .collect();
    let file_lookup = build_program_file_lookup(context.program);

    // Use the MergedProgram's global symbol-to-arena mapping.
    // This enables the declaration emitter's portability check to resolve
    // cross-file symbols (e.g., imported types from node_modules) to their
    // source file paths, which is required for TS2883 diagnostics.
    let global_symbol_arenas = (*context.program.symbol_arenas).clone();

    // Collect file paths that contain module augmentations.
    // The declaration emitter uses this to preserve side-effect imports for
    // files whose named bindings were all elided but whose augmentations must
    // still take effect.
    let files_with_augmentations: rustc_hash::FxHashSet<String> = context
        .program
        .files
        .iter()
        .filter(|file| !file.module_augmentations.is_empty())
        .map(|file| file.file_name.clone())
        .collect();
    let declaration_const_enum_exports = build_declaration_const_enum_exports(context.program);
    let ambient_global_type_only_names = build_ambient_global_type_only_names(
        context.program,
        context.options.printer.preserve_const_enums,
    );
    let type_only_export_equals_modules = build_type_only_export_equals_modules(
        context.program,
        context.options.printer.preserve_const_enums,
    );
    let bundled_duplicate_var_names = if declaration_bundle_path.is_some() {
        collect_bundled_duplicate_var_names(context.program)
    } else {
        Default::default()
    };
    let bundled_prior_duplicate_var_types = if declaration_bundle_path.is_some() {
        build_bundled_prior_duplicate_var_types_by_file(
            context.program,
            &bundled_duplicate_var_names,
        )
    } else {
        Default::default()
    };

    // Build the set of JS output paths produced by TypeScript source files
    // (.ts/.tsx/.mts/.cts). When --allowJs is set and a JS input file (e.g.
    // a.js) would produce the same output path as a TS file (e.g. a.ts -> a.js),
    // tsc blocks the JS file's emit. We replicate that by collecting TS output
    // paths first and skipping JS inputs that collide.
    let ts_output_paths: FxHashSet<PathBuf> = context
        .program
        .files
        .iter()
        .filter_map(|file| {
            let input_path = PathBuf::from(&file.file_name);
            let ext = input_path.extension().and_then(|e| e.to_str())?;
            if matches!(ext, "ts" | "tsx" | "mts" | "cts") && !is_declaration_file(&input_path) {
                js_output_path(
                    context.base_dir,
                    context.root_dir,
                    context.out_dir,
                    context.options.jsx,
                    &input_path,
                )
            } else {
                None
            }
        })
        .collect();

    for (file_idx, file) in context.program.files.iter().enumerate() {
        let input_path = PathBuf::from(&file.file_name);
        if let Some(dirty_paths) = context.dirty_paths
            && !dirty_paths.contains(&input_path)
        {
            continue;
        }

        // Skip JS input files whose output path would collide with a TS file's
        // output (tsc's "emit blocked" behavior for --allowJs).
        let is_js_input = input_path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| matches!(ext, "js" | "jsx" | "mjs" | "cjs"));
        if is_js_input
            && let Some(js_path) = js_output_path(
                context.base_dir,
                context.root_dir,
                context.out_dir,
                context.options.jsx,
                &input_path,
            )
            && ts_output_paths.contains(&js_path)
        {
            continue;
        }

        if !context.options.emit_declaration_only
            && let Some(js_path) = js_output_path(
                context.base_dir,
                context.root_dir,
                context.out_dir,
                context.options.jsx,
                &input_path,
            )
        {
            if is_js_input
                && js_input_skipped_by_node_modules_depth(
                    &input_path,
                    context.options.max_node_module_js_depth,
                )
            {
                let contents = std::fs::read_to_string(&input_path).with_context(|| {
                    format!("failed to read skipped JS source {}", input_path.display())
                })?;
                if js_bundle_path.is_some() {
                    js_bundle_chunks.push(contents);
                } else {
                    outputs.push(OutputFile {
                        path: js_path,
                        contents,
                        source_path: Some(input_path.clone()),
                    });
                }
                continue;
            }

            let mut printer_options = context.options.printer.clone();
            let mut type_only_nodes = context
                .type_caches
                .get(&input_path)
                .map_or_else(rustc_hash::FxHashSet::default, |cache| {
                    cache.type_only_nodes.clone()
                });
            mark_ambient_global_type_only_export_specifiers(
                &file.arena,
                file.source_file,
                &ambient_global_type_only_names,
                &mut type_only_nodes,
            );
            printer_options.type_only_nodes = std::sync::Arc::new(type_only_nodes);
            printer_options.type_only_export_equals_modules =
                type_only_export_equals_modules.clone();

            printer_options.no_lib = context.options.checker.no_lib;
            printer_options.isolated_modules = context.options.checker.isolated_modules;
            // Wire JSX options from resolved compiler options to printer
            if let Some(jsx) = context.options.jsx {
                printer_options.jsx = config_jsx_to_emitter_jsx(jsx);
                if matches!(jsx, JsxEmit::Preserve) {
                    printer_options.jsx_preserve_explicit = true;
                }
            }
            if !context.options.checker.jsx_factory.is_empty() {
                printer_options.jsx_factory = Some(context.options.checker.jsx_factory.clone());
            }
            if !context.options.checker.jsx_fragment_factory.is_empty() {
                printer_options.jsx_fragment_factory =
                    Some(context.options.checker.jsx_fragment_factory.clone());
            }
            if !context.options.checker.jsx_import_source.is_empty() {
                printer_options.jsx_import_source =
                    Some(context.options.checker.jsx_import_source.clone());
            }

            // Per-file module kind resolution.
            //
            // tsc's module emit behavior depends on:
            // 1. The --module setting (global)
            // 2. The file extension (.cts/.cjs -> CJS, .mts/.mjs -> ESM)
            // 3. For node modules, the nearest package.json "type" field
            // 4. The effective moduleDetection mode (determines if all files are modules)
            //
            // The config-level module_detection_force is already set correctly:
            // - true when moduleDetection=force (explicit or tsc default for node modules)
            // - false when moduleDetection=auto or legacy
            //
            // Here we handle per-file overrides.
            let file_name_lower = input_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let is_cts_or_cjs =
                file_name_lower.ends_with(".cts") || file_name_lower.ends_with(".cjs");
            let is_mts_or_mjs =
                file_name_lower.ends_with(".mts") || file_name_lower.ends_with(".mjs");

            if printer_options.module.is_node_module() {
                // For Node16/NodeNext, resolve the per-file module format based on
                // file extension and nearest package.json "type" field.
                let mode = implied_resolution_mode_for_file(&input_path, context.base_dir);
                if mode == "import" {
                    printer_options.module = ModuleKind::ESNext;
                    printer_options.resolved_node_module_to_esm = true;
                } else {
                    printer_options.module = ModuleKind::CommonJS;
                    printer_options.resolved_node_module_to_cjs = true;
                }
                // module_detection_force is already set at config level
                // (true by default for node modules when moduleDetection not explicit)
            } else if is_cts_or_cjs {
                // .cts/.cjs files always emit as CJS regardless of --module setting.
                // This handles cases like module=esnext with .cts files.
                let is_cjs_only = file_name_lower.ends_with(".cjs");
                if !printer_options.module.is_commonjs() {
                    // For .cjs (JavaScript) files under ESM/preserve module settings,
                    // tsc emits them as plain CJS passthrough without adding "use strict".
                    // .cts (TypeScript) files still get "use strict" since they are
                    // compiled output, not passthrough.
                    if matches!(printer_options.module, ModuleKind::Preserve) || is_cjs_only {
                        printer_options.suppress_use_strict = true;
                    }
                    printer_options.module = ModuleKind::CommonJS;
                }
            }

            if is_mts_or_mjs && !printer_options.module.is_es_module() {
                // .mts/.mjs files always emit as ESM regardless of --module setting.
                printer_options.module = ModuleKind::ESNext;
            }

            if js_bundle_path.is_some()
                && matches!(printer_options.module, ModuleKind::AMD | ModuleKind::System)
            {
                printer_options.bundled_module_name =
                    bundled_module_name(context.base_dir, context.root_dir, &input_path);
            }

            // tsc's isFileForcedToBeModuleByFormat: .cjs/.cts/.mjs/.mts files are
            // always modules in "auto" mode. This applies even when the config-level
            // module_detection_force is false (e.g., explicit moduleDetection=auto).
            if !printer_options.module_detection_force && (is_cts_or_cjs || is_mts_or_mjs) {
                printer_options.module_detection_force = true;
            }
            apply_external_const_enum_values(
                &mut printer_options,
                context.program,
                file,
                &declaration_const_enum_exports,
            );

            // Run the lowering pass to generate transform directives
            let mut ctx = tsz::context::emit::EmitContext::with_options(printer_options.clone());
            // Enable auto-detect module: when module is None and file has imports/exports,
            // the emitter should switch to CommonJS (matching tsc behavior)
            ctx.auto_detect_module = true;
            let transforms =
                tsz::lowering::LoweringPass::new(&file.arena, &ctx).run(file.source_file);

            let mut printer =
                Printer::with_transforms_and_options(&file.arena, transforms, printer_options);
            printer.set_auto_detect_module(true);
            // Always set source text for comment preservation and single-line detection
            if let Some(source_text) = file
                .arena
                .get(file.source_file)
                .and_then(|node| file.arena.get_source_file(node))
                .map(|source| source.text.as_ref())
            {
                printer.set_source_text(source_text);
            }

            let map_info = if context.options.source_map || context.options.inline_source_map {
                map_output_info(&js_path)
            } else {
                None
            };

            // Always set source text for formatting decisions (single-line vs multi-line)
            // This is needed even when source maps are disabled
            if let Some(source_text) = file
                .arena
                .get(file.source_file)
                .and_then(|node| file.arena.get_source_file(node))
                .map(|source| source.text.as_ref())
            {
                printer.set_source_map_text(source_text);
            }

            if let Some((_, _, output_name)) = map_info.as_ref() {
                printer.enable_source_map(output_name, &file.file_name);
            }

            printer.emit(file.source_file);
            let map_json = map_info
                .as_ref()
                .and_then(|_| printer.generate_source_map_json());
            let mut contents = printer.take_output();
            let mut map_output = None;

            if let Some((map_path, map_name, _)) = map_info
                && let Some(map_json) = map_json
            {
                if context.options.inline_source_map {
                    append_inline_source_mapping_url(&mut contents, &map_json, new_line);
                } else {
                    append_source_mapping_url(&mut contents, &map_name, new_line);
                    map_output = Some(OutputFile {
                        path: map_path,
                        contents: map_json,
                        source_path: Some(input_path.clone()),
                    });
                }
            }

            // When --outFile is set, collect content for bundling.
            if js_bundle_path.is_some() {
                js_bundle_chunks.push(contents);
            } else {
                outputs.push(OutputFile {
                    path: js_path,
                    contents,
                    source_path: Some(input_path.clone()),
                });
                if let Some(map_output) = map_output {
                    outputs.push(map_output);
                }
            }
        }

        if context.options.emit_declarations {
            let decl_base = context.declaration_dir.or(context.out_dir);
            if let Some(dts_path) =
                declaration_output_path(context.base_dir, context.root_dir, decl_base, &input_path)
            {
                // Get type cache for this file if available
                let file_path = PathBuf::from(&file.file_name);
                let type_cache = context.type_caches.get(&file_path).cloned();

                // Reconstruct BinderState for this file to enable usage analysis
                let binder =
                    tsz::parallel::create_binder_from_bound_file(file, context.program, file_idx);

                // Create emitter with type information and binder
                let mut emitter = if let Some(ref cache) = type_cache {
                    use tsz_emitter::type_cache_view::TypeCacheView;
                    let cache_view = TypeCacheView {
                        node_types: cache.node_types.to_hash_map(),
                        symbol_types: cache.symbol_types.to_hash_map(),
                        def_to_symbol: cache.def_to_symbol.clone(),
                        def_types: cache.def_types.clone(),
                        def_type_params: cache.def_type_params.clone(),
                        def_to_name: cache.def_to_name.clone(),
                    };
                    let mut emitter = DeclarationEmitter::with_type_info(
                        &file.arena,
                        cache_view,
                        &context.program.type_interner,
                        &binder,
                    );
                    // Set current arena and file path for foreign symbol tracking
                    emitter.set_current_arena(
                        std::sync::Arc::clone(&file.arena),
                        file.file_name.clone(),
                    );
                    // Set arena to path mapping for module resolution
                    emitter.set_arena_to_path(arena_to_path.clone());
                    emitter.set_file_idx_to_path(file_idx_to_path.clone());
                    emitter.set_global_symbol_arenas(global_symbol_arenas.clone());
                    emitter.set_bundled_duplicate_var_context(
                        bundled_duplicate_var_names.clone(),
                        bundled_prior_duplicate_var_types
                            .get(file_idx)
                            .cloned()
                            .unwrap_or_default(),
                    );
                    emitter.set_remove_comments(context.options.printer.remove_comments);
                    emitter.set_strip_internal(context.options.strip_internal);
                    emitter.set_strict_null_checks(context.options.checker.strict_null_checks);
                    emitter
                        .set_isolated_declarations(context.options.checker.isolated_declarations);
                    emitter.set_files_with_augmentations(files_with_augmentations.clone());
                    emitter
                } else {
                    let mut emitter = DeclarationEmitter::new(&file.arena);
                    // Still set binder even without cache for consistency
                    emitter.set_binder(Some(&binder));
                    emitter.set_arena_to_path(arena_to_path.clone());
                    emitter.set_file_idx_to_path(file_idx_to_path.clone());
                    emitter.set_global_symbol_arenas(global_symbol_arenas.clone());
                    emitter.set_bundled_duplicate_var_context(
                        bundled_duplicate_var_names.clone(),
                        bundled_prior_duplicate_var_types
                            .get(file_idx)
                            .cloned()
                            .unwrap_or_default(),
                    );
                    emitter.set_remove_comments(context.options.printer.remove_comments);
                    emitter.set_strip_internal(context.options.strip_internal);
                    emitter.set_strict_null_checks(context.options.checker.strict_null_checks);
                    emitter
                        .set_isolated_declarations(context.options.checker.isolated_declarations);
                    emitter.set_files_with_augmentations(files_with_augmentations.clone());
                    emitter
                };

                // NOTE: tsc still emits TS2883 for non-portable inferred type
                // references even in node16/nodenext mode. The exports map blocks
                // direct imports (TS2307) but doesn't suppress the portability
                // check on inferred types in declaration emit.

                // Precompute the export surface summary for this file.
                // This seeds the overload pre-scan so the emitter doesn't
                // need to discover overloads incrementally during the walk.
                let surface = tsz_binder::ExportSurface::from_binder(
                    &binder,
                    &file.arena,
                    &file.file_name,
                    file.source_file,
                );
                emitter.set_export_surface(surface);
                let map_info =
                    if declaration_bundle_path.is_none() && context.options.declaration_map {
                        map_output_info(&dts_path)
                    } else {
                        None
                    };

                if let Some((map_path, _, output_name)) = map_info.as_ref() {
                    if let Some(source_text) = file
                        .arena
                        .get(file.source_file)
                        .and_then(|node| file.arena.get_source_file(node))
                        .map(|source| source.text.as_ref())
                    {
                        emitter.set_source_map_text(source_text);
                    }
                    let source_name = declaration_map_source_name(map_path, &input_path);
                    emitter.enable_source_map_without_sources_content(output_name, &source_name);
                }

                // Run usage analysis and calculate required imports if we have type cache
                if let Some(ref cache) = type_cache {
                    use rustc_hash::FxHashMap;
                    use tsz::declaration_emitter::usage_analyzer::{
                        UsageAnalyzer, UsageAnalyzerConfig,
                    };
                    use tsz_emitter::type_cache_view::TypeCacheView;

                    // Empty import_name_map for this usage (not needed for auto-import calculation)
                    let import_name_map = FxHashMap::default();
                    let cache_view = TypeCacheView {
                        node_types: cache.node_types.to_hash_map(),
                        symbol_types: cache.symbol_types.to_hash_map(),
                        def_to_symbol: cache.def_to_symbol.clone(),
                        def_types: cache.def_types.clone(),
                        def_type_params: cache.def_type_params.clone(),
                        def_to_name: cache.def_to_name.clone(),
                    };

                    let mut analyzer = UsageAnalyzer::new(UsageAnalyzerConfig {
                        arena: &file.arena,
                        binder: &binder,
                        type_cache: &cache_view,
                        type_interner: &context.program.type_interner,
                        current_arena: std::sync::Arc::clone(&file.arena),
                        current_file_path: Some(file.file_name.clone()),
                        import_name_map: &import_name_map,
                        source_is_js_file: is_js_input,
                        source_is_declaration_file: file
                            .arena
                            .get(file.source_file)
                            .and_then(|node| file.arena.get_source_file(node))
                            .is_some_and(|source_file| source_file.is_declaration_file),
                    });

                    // Clone used_symbols before calling another method on analyzer
                    let used_symbols = analyzer.analyze(file.source_file).clone();
                    let foreign_symbols = analyzer.get_foreign_symbols().clone();

                    // Set used symbols and foreign symbols on emitter
                    emitter.set_used_symbols(used_symbols);
                    emitter.set_foreign_symbols(foreign_symbols);
                }

                let mut contents = emitter.emit(file.source_file);
                let emitter_diagnostics = normalize_ts2883_diagnostics(emitter.take_diagnostics());
                let declaration_emit_blocked = emitter_diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.category == DiagnosticCategory::Error);
                emit_diagnostics.extend(emitter_diagnostics);
                if declaration_emit_blocked {
                    declaration_bundle_blocked = true;
                    continue;
                }
                let map_json = map_info
                    .as_ref()
                    .and_then(|_| emitter.generate_source_map_json());
                let mut map_output = None;

                if let Some((map_path, map_name, _)) = map_info
                    && let Some(map_json) = map_json
                {
                    append_source_mapping_url(&mut contents, &map_name, new_line);
                    map_output = Some(OutputFile {
                        path: map_path,
                        contents: map_json,
                        source_path: Some(input_path.clone()),
                    });
                }

                if declaration_bundle_path.is_some() {
                    let declaration_module_name = file
                        .is_external_module
                        .then(|| {
                            bundled_module_name(context.base_dir, context.root_dir, &input_path)
                        })
                        .flatten();
                    declaration_bundle_chunks.push(DeclarationBundleChunk {
                        path_key: normalized_file_key(&file.file_name),
                        referenced_path_keys: declaration_bundle_reference_path_keys(
                            &file.file_name,
                            &file.arena,
                            file.source_file,
                            &file_lookup,
                        ),
                        contents: bundle_declaration_output(
                            &contents,
                            context.options.printer.module,
                            declaration_module_name.as_deref(),
                        ),
                    });
                } else {
                    outputs.push(OutputFile {
                        path: dts_path,
                        contents,
                        source_path: Some(input_path.clone()),
                    });
                    if let Some(map_output) = map_output {
                        outputs.push(map_output);
                    }
                }
            }
        }
    }

    // Emit bundled JS output when --outFile is set
    if !context.options.emit_declaration_only
        && let Some(bundle_path) = js_bundle_path
        && !js_bundle_chunks.is_empty()
    {
        let mut bundled = String::new();
        for (i, chunk) in js_bundle_chunks.iter().enumerate() {
            let mut trimmed = chunk.trim_end_matches(['\r', '\n']);
            // Strip duplicate "use strict" directives from non-first files.
            // In bundled output, only the first file's prologue is kept.
            if i > 0 {
                if let Some(rest) = trimmed.strip_prefix("\"use strict\";\n") {
                    trimmed = rest;
                } else if let Some(rest) = trimmed.strip_prefix("\"use strict\";\r\n") {
                    trimmed = rest;
                }
            }
            if trimmed.is_empty() {
                continue;
            }
            if !bundled.is_empty() && !bundled.ends_with(new_line) {
                bundled.push_str(new_line);
            }
            bundled.push_str(trimmed);
            bundled.push_str(new_line);
        }
        // Remove trailing newline to match tsc behavior
        if bundled.ends_with(new_line) {
            bundled.truncate(bundled.len() - new_line.len());
        }
        if matches!(context.options.printer.module, ModuleKind::AMD)
            && context.options.printer.always_strict
        {
            // Only prepend a top-level `"use strict";` when the bundle
            // contains at least one script (a non-module file). For an
            // all-modules bundle, every chunk is wrapped in `define(...)`
            // and emits its own `"use strict";` inside the callback —
            // tsc does not add a second one at the top of the bundle.
            let any_script_chunk = js_bundle_chunks.iter().any(|chunk| {
                let trimmed = chunk.trim_start();
                !(trimmed.starts_with("define(") || trimmed.starts_with("System.register("))
            });
            if any_script_chunk {
                prepend_use_strict_to_bundle(&mut bundled, new_line);
            }
        }
        outputs.push(OutputFile {
            path: bundle_path,
            contents: bundled,
            source_path: None,
        });
    }

    if let Some(bundle_path) = declaration_bundle_path
        && !declaration_bundle_chunks.is_empty()
        && !declaration_bundle_blocked
    {
        outputs.push(OutputFile {
            path: bundle_path,
            contents: join_declaration_bundle_chunks(&declaration_bundle_chunks, new_line),
            source_path: None,
        });
    }

    Ok((outputs, emit_diagnostics))
}

fn prepend_use_strict_to_bundle(contents: &mut String, new_line: &str) {
    let directive = format!("\"use strict\";{new_line}");
    if contents.starts_with(&directive) {
        return;
    }

    if contents.starts_with("#!") {
        if let Some(line_end) = contents.find('\n') {
            let insert_at = line_end + 1;
            if !contents[insert_at..].starts_with(&directive) {
                contents.insert_str(insert_at, &directive);
            }
        }
        return;
    }

    contents.insert_str(0, &directive);
}

type ConstEnumValues = FxHashMap<String, EnumValue>;

#[derive(Clone, Default, PartialEq)]
struct DeclarationConstEnumExports {
    named: FxHashMap<String, ConstEnumValues>,
    default: Option<ConstEnumValues>,
}

fn build_declaration_const_enum_exports(
    program: &MergedProgram,
) -> FxHashMap<String, DeclarationConstEnumExports> {
    let file_lookup = build_program_file_lookup(program);
    let mut exports_by_file: FxHashMap<String, DeclarationConstEnumExports> = FxHashMap::default();

    for file in &program.files {
        let path = normalized_file_key(&file.file_name);
        if !is_declaration_file(Path::new(&file.file_name)) {
            continue;
        }
        let mut exports = DeclarationConstEnumExports::default();
        collect_direct_const_enum_exports(file, &mut exports);
        exports_by_file.insert(path, exports);
    }

    for _ in 0..4 {
        let previous = exports_by_file.clone();
        let mut changed = false;
        for file in &program.files {
            if !is_declaration_file(Path::new(&file.file_name)) {
                continue;
            }
            let path = normalized_file_key(&file.file_name);
            let mut exports = previous.get(&path).cloned().unwrap_or_default();
            collect_aliased_const_enum_exports(file, &file_lookup, &previous, &mut exports);
            if previous.get(&path) != Some(&exports) {
                exports_by_file.insert(path, exports);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    exports_by_file
}

fn apply_external_const_enum_values(
    options: &mut tsz::emitter::PrinterOptions,
    program: &MergedProgram,
    file: &BoundFile,
    exports_by_file: &FxHashMap<String, DeclarationConstEnumExports>,
) {
    if options.no_const_enum_inlining || exports_by_file.is_empty() {
        return;
    }

    let file_lookup = build_program_file_lookup(program);
    let Some(statements) = source_statements(file) else {
        return;
    };
    for &stmt_idx in &statements.nodes {
        let Some(stmt_node) = file.arena.get(stmt_idx) else {
            continue;
        };
        if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
            continue;
        }
        let Some(import_data) = file.arena.get_import_decl(stmt_node) else {
            continue;
        };
        let Some(module_spec) = literal_text(&file.arena, import_data.module_specifier) else {
            continue;
        };
        let Some(target_path) =
            resolve_relative_module_file(&file.file_name, &module_spec, &file_lookup)
        else {
            continue;
        };
        let Some(exports) = exports_by_file.get(&target_path) else {
            continue;
        };
        let Some(clause_node) = file.arena.get(import_data.import_clause) else {
            continue;
        };
        let Some(clause) = file.arena.get_import_clause(clause_node) else {
            continue;
        };

        if clause.name.is_some()
            && let Some(values) = &exports.default
        {
            let local_name = identifier_text(&file.arena, clause.name);
            if !local_name.is_empty() {
                options
                    .external_const_enum_values
                    .insert(local_name.clone(), values.clone());
                options.external_const_enum_bindings.insert(local_name);
            }
        }

        if clause.named_bindings.is_some()
            && let Some(bindings_node) = file.arena.get(clause.named_bindings)
            && let Some(named_imports) = file.arena.get_named_imports(bindings_node)
            && named_imports.name.is_none()
        {
            for &spec_idx in &named_imports.elements.nodes {
                let Some(spec_node) = file.arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = file.arena.get_specifier(spec_node) else {
                    continue;
                };
                if spec.is_type_only {
                    continue;
                }
                let imported_name = if spec.property_name.is_some() {
                    identifier_text(&file.arena, spec.property_name)
                } else {
                    identifier_text(&file.arena, spec.name)
                };
                let local_name = identifier_text(&file.arena, spec.name);
                if imported_name.is_empty() || local_name.is_empty() {
                    continue;
                }
                if let Some(values) = exports.named.get(&imported_name) {
                    options
                        .external_const_enum_values
                        .insert(local_name.clone(), values.clone());
                    options.external_const_enum_bindings.insert(local_name);
                }
            }
        }
    }
}

fn collect_direct_const_enum_exports(file: &BoundFile, exports: &mut DeclarationConstEnumExports) {
    let Some(statements) = source_statements(file) else {
        return;
    };
    let mut evaluator = EnumEvaluator::new(&file.arena);
    for &stmt_idx in &statements.nodes {
        collect_direct_const_enum_export_from_node(
            &file.arena,
            &mut evaluator,
            stmt_idx,
            false,
            exports,
        );
    }
}

fn collect_direct_const_enum_export_from_node(
    arena: &NodeArena,
    evaluator: &mut EnumEvaluator<'_>,
    node_idx: NodeIndex,
    force_exported: bool,
    exports: &mut DeclarationConstEnumExports,
) {
    let Some(node) = arena.get(node_idx) else {
        return;
    };
    if node.kind == syntax_kind_ext::EXPORT_DECLARATION
        && let Some(export_data) = arena.get_export_decl(node)
        && export_data.module_specifier.is_none()
        && let Some(clause_node) = arena.get(export_data.export_clause)
    {
        collect_direct_const_enum_export_from_node(
            arena,
            evaluator,
            export_data.export_clause,
            true,
            exports,
        );
        if clause_node.kind != syntax_kind_ext::ENUM_DECLARATION {
            return;
        }
    }

    if node.kind != syntax_kind_ext::ENUM_DECLARATION {
        return;
    }
    let Some(enum_data) = arena.get_enum(node) else {
        return;
    };
    if !arena.has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword)
        || (!force_exported && !arena.has_modifier(&enum_data.modifiers, SyntaxKind::ExportKeyword))
    {
        return;
    }
    let name = identifier_text(arena, enum_data.name);
    if name.is_empty() {
        return;
    }
    let values = evaluator.evaluate_enum(node_idx);
    if !values.is_empty() {
        exports.named.insert(name, values);
    }
}

fn collect_aliased_const_enum_exports(
    file: &BoundFile,
    file_lookup: &FxHashMap<String, String>,
    exports_by_file: &FxHashMap<String, DeclarationConstEnumExports>,
    exports: &mut DeclarationConstEnumExports,
) {
    let Some(statements) = source_statements(file) else {
        return;
    };
    let local_aliases = collect_const_enum_import_aliases(file, file_lookup, exports_by_file);
    for &stmt_idx in &statements.nodes {
        let Some(stmt_node) = file.arena.get(stmt_idx) else {
            continue;
        };
        if stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
            && let Some(export_assignment) = file.arena.get_export_assignment(stmt_node)
            && !export_assignment.is_export_equals
        {
            let name = identifier_text(&file.arena, export_assignment.expression);
            if let Some(values) = local_aliases
                .get(&name)
                .or_else(|| exports.named.get(&name))
                .cloned()
            {
                exports.default = Some(values);
            }
            continue;
        }

        if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
            continue;
        }
        let Some(export_data) = file.arena.get_export_decl(stmt_node) else {
            continue;
        };
        if export_data.is_default_export {
            let name = identifier_text(&file.arena, export_data.export_clause);
            if let Some(values) = local_aliases
                .get(&name)
                .or_else(|| exports.named.get(&name))
                .cloned()
            {
                exports.default = Some(values);
            }
            continue;
        }
        let Some(clause_node) = file.arena.get(export_data.export_clause) else {
            continue;
        };
        if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
            continue;
        }
        let Some(named_exports) = file.arena.get_named_imports(clause_node) else {
            continue;
        };
        let source_exports = if export_data.module_specifier.is_some() {
            literal_text(&file.arena, export_data.module_specifier)
                .and_then(|module_spec| {
                    resolve_relative_module_file(&file.file_name, &module_spec, file_lookup)
                })
                .and_then(|path| exports_by_file.get(&path))
        } else {
            None
        };

        for &spec_idx in &named_exports.elements.nodes {
            let Some(spec_node) = file.arena.get(spec_idx) else {
                continue;
            };
            let Some(spec) = file.arena.get_specifier(spec_node) else {
                continue;
            };
            if spec.is_type_only {
                continue;
            }
            let exported_name = identifier_text(&file.arena, spec.name);
            let local_or_imported_name = if spec.property_name.is_some() {
                identifier_text(&file.arena, spec.property_name)
            } else {
                exported_name.clone()
            };
            let values = source_exports
                .and_then(|source| source.named.get(&local_or_imported_name))
                .cloned()
                .or_else(|| local_aliases.get(&local_or_imported_name).cloned())
                .or_else(|| exports.named.get(&local_or_imported_name).cloned());
            let Some(values) = values else {
                continue;
            };
            if exported_name == "default" {
                exports.default = Some(values);
            } else if !exported_name.is_empty() {
                exports.named.insert(exported_name, values);
            }
        }
    }
}

fn collect_const_enum_import_aliases(
    file: &BoundFile,
    file_lookup: &FxHashMap<String, String>,
    exports_by_file: &FxHashMap<String, DeclarationConstEnumExports>,
) -> FxHashMap<String, ConstEnumValues> {
    let mut aliases = FxHashMap::default();
    let Some(statements) = source_statements(file) else {
        return aliases;
    };
    for &stmt_idx in &statements.nodes {
        let Some(stmt_node) = file.arena.get(stmt_idx) else {
            continue;
        };
        if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
            continue;
        }
        let Some(import_data) = file.arena.get_import_decl(stmt_node) else {
            continue;
        };
        let Some(module_spec) = literal_text(&file.arena, import_data.module_specifier) else {
            continue;
        };
        let Some(target_path) =
            resolve_relative_module_file(&file.file_name, &module_spec, file_lookup)
        else {
            continue;
        };
        let Some(source_exports) = exports_by_file.get(&target_path) else {
            continue;
        };
        let Some(clause_node) = file.arena.get(import_data.import_clause) else {
            continue;
        };
        let Some(clause) = file.arena.get_import_clause(clause_node) else {
            continue;
        };
        if clause.name.is_some()
            && let Some(values) = &source_exports.default
        {
            let name = identifier_text(&file.arena, clause.name);
            if !name.is_empty() {
                aliases.insert(name, values.clone());
            }
        }
        if clause.named_bindings.is_some()
            && let Some(bindings_node) = file.arena.get(clause.named_bindings)
            && let Some(named_imports) = file.arena.get_named_imports(bindings_node)
            && named_imports.name.is_none()
        {
            for &spec_idx in &named_imports.elements.nodes {
                let Some(spec_node) = file.arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = file.arena.get_specifier(spec_node) else {
                    continue;
                };
                let imported_name = if spec.property_name.is_some() {
                    identifier_text(&file.arena, spec.property_name)
                } else {
                    identifier_text(&file.arena, spec.name)
                };
                let local_name = identifier_text(&file.arena, spec.name);
                if let Some(values) = source_exports.named.get(&imported_name)
                    && !local_name.is_empty()
                {
                    aliases.insert(local_name, values.clone());
                }
            }
        }
    }
    aliases
}

fn source_statements(file: &BoundFile) -> Option<&tsz_parser::parser::NodeList> {
    file.arena
        .get(file.source_file)
        .and_then(|node| file.arena.get_source_file(node))
        .map(|source| &source.statements)
}

fn identifier_text(arena: &NodeArena, idx: NodeIndex) -> String {
    arena.identifier_text_owned(idx).unwrap_or_default()
}

fn literal_text(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    arena
        .get(idx)
        .and_then(|node| arena.get_literal(node))
        .map(|lit| lit.text.clone())
}

fn build_program_file_lookup(program: &MergedProgram) -> FxHashMap<String, String> {
    program
        .files
        .iter()
        .map(|file| {
            let key = normalized_file_key(&file.file_name);
            (key.clone(), key)
        })
        .collect()
}

fn normalized_file_key(file_name: &str) -> String {
    normalize_path(Path::new(file_name))
        .to_string_lossy()
        .replace('\\', "/")
}

fn resolve_relative_module_file(
    containing_file: &str,
    module_spec: &str,
    file_lookup: &FxHashMap<String, String>,
) -> Option<String> {
    if !(module_spec.starts_with("./") || module_spec.starts_with("../")) {
        return None;
    }
    let containing = Path::new(containing_file);
    let base = containing
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(module_spec);
    for candidate in module_resolution_candidates(&base) {
        let key = normalize_path(&candidate)
            .to_string_lossy()
            .replace('\\', "/");
        if let Some(path) = file_lookup.get(&key) {
            return Some(path.clone());
        }
    }
    None
}

fn module_resolution_candidates(base: &Path) -> Vec<PathBuf> {
    if base.extension().is_some() {
        return vec![base.to_path_buf()];
    }
    vec![
        base.with_extension("ts"),
        base.with_extension("tsx"),
        base.with_extension("d.ts"),
        base.with_extension("js"),
        base.join("index.ts"),
        base.join("index.tsx"),
        base.join("index.d.ts"),
        base.join("index.js"),
    ]
}

fn declaration_bundle_reference_path_keys(
    containing_file: &str,
    arena: &NodeArena,
    source_file_idx: NodeIndex,
    file_lookup: &FxHashMap<String, String>,
) -> Vec<String> {
    let Some(source_text) = arena
        .get(source_file_idx)
        .and_then(|node| arena.get_source_file(node))
        .map(|source| source.text.as_ref())
    else {
        return Vec::new();
    };

    tsz::checker::triple_slash_validator::extract_reference_paths(source_text)
        .into_iter()
        .filter_map(|(reference_path, _, _)| {
            resolve_declaration_reference_path_file(containing_file, &reference_path, file_lookup)
        })
        .collect()
}

fn resolve_declaration_reference_path_file(
    containing_file: &str,
    reference_path: &str,
    file_lookup: &FxHashMap<String, String>,
) -> Option<String> {
    if reference_path.is_empty() {
        return None;
    }

    let containing = Path::new(containing_file);
    let base_dir = containing.parent().unwrap_or_else(|| Path::new(""));
    let direct_reference = base_dir.join(reference_path);
    let mut candidates = vec![direct_reference];
    if !reference_path.contains('.') {
        for ext in tsz::checker::triple_slash_validator::reference_path_probe_extensions(true) {
            candidates.push(base_dir.join(format!("{reference_path}{ext}")));
        }
    }

    candidates.into_iter().find_map(|candidate| {
        let key = normalize_path(&candidate)
            .to_string_lossy()
            .replace('\\', "/");
        file_lookup.get(&key).cloned()
    })
}

fn normalize_ts2883_diagnostics(diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut exact_seen: FxHashMap<(u32, String, u32, u32, String), usize> = FxHashMap::default();
    let mut unique: Vec<(Diagnostic, bool)> = Vec::new();

    for diagnostic in diagnostics {
        let mut diagnostic = diagnostic;
        let mut was_canonicalized = false;
        if diagnostic.code == 2883
            && let Some(message) =
                canonical_ts2883_named_reference_message(&diagnostic.message_text)
        {
            diagnostic.message_text = message;
            was_canonicalized = true;
        }
        let exact_key = (
            diagnostic.code,
            diagnostic.file.clone(),
            diagnostic.start,
            diagnostic.length,
            diagnostic.message_text.clone(),
        );
        if let Some(&existing_idx) = exact_seen.get(&exact_key) {
            if !was_canonicalized && unique[existing_idx].1 {
                unique[existing_idx] = (diagnostic, was_canonicalized);
            }
            continue;
        }

        exact_seen.insert(exact_key, unique.len());
        unique.push((diagnostic, was_canonicalized));
    }

    let surviving_canonical_sites: FxHashSet<_> = unique
        .iter()
        .filter_map(|(diagnostic, was_canonicalized)| {
            if diagnostic.code != 2883 || *was_canonicalized {
                return None;
            }
            let (first, second) = parse_ts2883_named_reference_message(&diagnostic.message_text)?;
            (!looks_like_module_path(&first) && looks_like_module_path(&second))
                .then(|| (diagnostic.file.clone(), diagnostic.start, diagnostic.length))
        })
        .collect();

    unique
        .into_iter()
        .filter_map(|(diagnostic, was_canonicalized)| {
            if diagnostic.code != 2883 {
                return Some(diagnostic);
            }

            let Some((first, second)) =
                parse_ts2883_named_reference_message(&diagnostic.message_text)
            else {
                return Some(diagnostic);
            };

            if !was_canonicalized
                || looks_like_module_path(&first)
                || !looks_like_module_path(&second)
            {
                return Some(diagnostic);
            }

            (!surviving_canonical_sites.contains(&(
                diagnostic.file.clone(),
                diagnostic.start,
                diagnostic.length,
            )))
            .then_some(diagnostic)
        })
        .collect()
}

fn parse_ts2883_named_reference_message(message: &str) -> Option<(String, String)> {
    let prefix = "cannot be named without a reference to '";
    let start = message.find(prefix)? + prefix.len();
    let rest = &message[start..];
    let (first, tail) = rest.split_once("' from '")?;
    let (second, _) = tail.split_once('\'')?;
    Some((first.to_string(), second.to_string()))
}

fn canonical_ts2883_named_reference_message(message: &str) -> Option<String> {
    let (first, second) = parse_ts2883_named_reference_message(message)?;
    if !looks_like_module_path(&first) || looks_like_module_path(&second) {
        return None;
    }

    Some(message.replace(
        &format!("reference to '{first}' from '{second}'"),
        &format!("reference to '{second}' from '{first}'"),
    ))
}

fn looks_like_module_path(text: &str) -> bool {
    text.starts_with('.')
        || text.starts_with('/')
        || text.contains('/')
        || text.contains('\\')
        || text.contains("node_modules")
}

fn map_output_info(output_path: &Path) -> Option<(PathBuf, String, String)> {
    let output_name = output_path.file_name()?.to_string_lossy().into_owned();
    let map_name = format!("{output_name}.map");
    let map_path = output_path.with_file_name(&map_name);
    Some((map_path, map_name, output_name))
}

fn declaration_map_source_name(map_path: &Path, source_path: &Path) -> String {
    let map_dir = map_path.parent().unwrap_or_else(|| Path::new(""));
    relative_path_from_dir(map_dir, source_path)
        .unwrap_or_else(|| source_path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

fn relative_path_from_dir(from_dir: &Path, to_path: &Path) -> Option<PathBuf> {
    if from_dir.is_absolute() != to_path.is_absolute() {
        return None;
    }

    let from_components = normalized_path_components(from_dir);
    let to_components = normalized_path_components(to_path);
    let mut common_len = 0;
    while common_len < from_components.len()
        && common_len < to_components.len()
        && from_components[common_len] == to_components[common_len]
    {
        common_len += 1;
    }

    let mut relative = PathBuf::new();
    for _ in common_len..from_components.len() {
        relative.push("..");
    }
    for component in &to_components[common_len..] {
        relative.push(component);
    }

    Some(relative)
}

fn normalized_path_components(path: &Path) -> Vec<std::ffi::OsString> {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::CurDir | std::path::Component::RootDir => None,
            std::path::Component::ParentDir => Some(std::ffi::OsString::from("..")),
            std::path::Component::Normal(part) => Some(part.to_os_string()),
            std::path::Component::Prefix(prefix) => Some(prefix.as_os_str().to_os_string()),
        })
        .collect()
}

fn append_source_mapping_url(contents: &mut String, map_name: &str, new_line: &str) {
    if !contents.is_empty() && !contents.ends_with(new_line) {
        contents.push_str(new_line);
    }
    contents.push_str("//# sourceMappingURL=");
    contents.push_str(map_name);
}

fn append_inline_source_mapping_url(contents: &mut String, map_json: &str, new_line: &str) {
    if !contents.is_empty() && !contents.ends_with(new_line) {
        contents.push_str(new_line);
    }
    contents.push_str("//# sourceMappingURL=data:application/json;base64,");
    contents.push_str(&base64_encode(map_json.as_bytes()));
}

fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        encoded.push(ALPHABET[(b0 >> 2) as usize] as char);
        encoded.push(ALPHABET[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(ALPHABET[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(ALPHABET[(b2 & 0b0011_1111) as usize] as char);
        } else {
            encoded.push('=');
        }
    }

    encoded
}

const fn new_line_str(kind: NewLineKind) -> &'static str {
    match kind {
        NewLineKind::LineFeed => "\n",
        NewLineKind::CarriageReturnLineFeed => "\r\n",
    }
}

pub(crate) fn write_outputs(outputs: &[OutputFile], emit_bom: bool) -> Result<Vec<PathBuf>> {
    outputs.par_iter().try_for_each(|output| -> Result<()> {
        if let Some(parent) = output.path.parent() {
            std::fs::create_dir_all::<&Path>(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
        let contents = if emit_bom && !output.contents.starts_with('\u{feff}') {
            format!("\u{feff}{}", output.contents)
        } else {
            output.contents.clone()
        };
        std::fs::write(&output.path, contents)
            .with_context(|| format!("failed to write {}", output.path.display()))?;
        Ok(())
    })?;

    Ok(outputs.iter().map(|output| output.path.clone()).collect())
}

fn js_output_path(
    base_dir: &Path,
    root_dir: Option<&Path>,
    out_dir: Option<&Path>,
    jsx: Option<JsxEmit>,
    input_path: &Path,
) -> Option<PathBuf> {
    if is_declaration_file(input_path) {
        return None;
    }

    let extension = js_extension_for(input_path, jsx)?;
    let mut output = if should_emit_next_to_source(root_dir, out_dir, input_path) {
        input_path.to_path_buf()
    } else {
        let relative = output_relative_path(base_dir, root_dir, input_path);
        match out_dir {
            Some(out_dir) => out_dir.join(relative),
            None => input_path.to_path_buf(),
        }
    };
    output.set_extension(extension);
    Some(output)
}

fn declaration_output_path(
    base_dir: &Path,
    root_dir: Option<&Path>,
    out_dir: Option<&Path>,
    input_path: &Path,
) -> Option<PathBuf> {
    if is_declaration_file(input_path) {
        return None;
    }

    let relative = output_relative_path(base_dir, root_dir, input_path);
    let file_name = relative.file_name()?.to_str()?;
    let new_name = declaration_file_name(file_name)?;

    let mut output = if should_emit_next_to_source(root_dir, out_dir, input_path) {
        input_path.to_path_buf()
    } else {
        match out_dir {
            Some(out_dir) => out_dir.join(relative),
            None => input_path.to_path_buf(),
        }
    };
    output.set_file_name(new_name);
    Some(output)
}

fn declaration_bundle_output_path(
    base_dir: &Path,
    out_dir: Option<&Path>,
    out_file: &Path,
) -> Option<PathBuf> {
    let relative = if out_file.is_absolute() {
        PathBuf::from(out_file.file_name()?)
    } else {
        out_file.to_path_buf()
    };

    let mut output = match out_dir {
        Some(out_dir) => out_dir.join(&relative),
        None if out_file.is_absolute() => out_file.to_path_buf(),
        None => base_dir.join(&relative),
    };
    let file_name = output.file_name()?.to_str()?;
    let new_name = declaration_file_name(file_name)?;
    output.set_file_name(new_name);
    Some(output)
}

fn join_declaration_bundle_chunks(chunks: &[DeclarationBundleChunk], new_line: &str) -> String {
    let ordered_indices = declaration_bundle_chunk_order(chunks);
    let mut bundled = String::new();
    for chunk in ordered_indices
        .into_iter()
        .filter_map(|idx| chunks.get(idx))
        .map(|chunk| chunk.contents.as_str())
    {
        if !bundled.is_empty() && !bundled.ends_with(new_line) {
            bundled.push_str(new_line);
        }
        bundled.push_str(chunk.trim_end_matches(['\r', '\n']));
        bundled.push_str(new_line);
    }
    if bundled.ends_with(new_line) {
        bundled.truncate(bundled.len() - new_line.len());
    }
    bundled
}

fn declaration_bundle_chunk_order(chunks: &[DeclarationBundleChunk]) -> Vec<usize> {
    let by_path: FxHashMap<&str, usize> = chunks
        .iter()
        .enumerate()
        .map(|(idx, chunk)| (chunk.path_key.as_str(), idx))
        .collect();
    let mut ordered = Vec::with_capacity(chunks.len());
    let mut emitted = FxHashSet::default();
    let mut visiting = FxHashSet::default();

    fn visit(
        idx: usize,
        chunks: &[DeclarationBundleChunk],
        by_path: &FxHashMap<&str, usize>,
        emitted: &mut FxHashSet<usize>,
        visiting: &mut FxHashSet<usize>,
        ordered: &mut Vec<usize>,
    ) {
        if emitted.contains(&idx) || !visiting.insert(idx) {
            return;
        }

        for referenced_path in &chunks[idx].referenced_path_keys {
            if let Some(&referenced_idx) = by_path.get(referenced_path.as_str()) {
                visit(referenced_idx, chunks, by_path, emitted, visiting, ordered);
            }
        }

        visiting.remove(&idx);
        if emitted.insert(idx) {
            ordered.push(idx);
        }
    }

    for idx in 0..chunks.len() {
        visit(
            idx,
            chunks,
            &by_path,
            &mut emitted,
            &mut visiting,
            &mut ordered,
        );
    }

    ordered
}

fn bundle_declaration_output(
    contents: &str,
    module_kind: ModuleKind,
    fallback_module_name: Option<&str>,
) -> String {
    if !matches!(module_kind, ModuleKind::AMD) {
        return contents.to_string();
    }

    wrap_amd_declaration_output(contents, fallback_module_name)
        .unwrap_or_else(|| contents.to_string())
}

fn wrap_amd_declaration_output(
    contents: &str,
    fallback_module_name: Option<&str>,
) -> Option<String> {
    let mut directive_lines = Vec::new();
    let mut body_lines = Vec::new();
    let mut in_header = true;
    let mut amd_module_name = None;

    for line in contents.lines() {
        if in_header && line.trim_start().starts_with("///") {
            directive_lines.push(line.to_string());
            if amd_module_name.is_none() && is_amd_module_directive(line) {
                amd_module_name = extract_amd_module_name(line);
            }
            continue;
        }
        in_header = false;
        body_lines.push(line.to_string());
    }

    let amd_module_name = amd_module_name.or_else(|| fallback_module_name.map(str::to_string))?;
    let mut wrapped = String::new();
    for directive in directive_lines {
        wrapped.push_str(&directive);
        wrapped.push('\n');
    }
    wrapped.push_str("declare module \"");
    wrapped.push_str(&amd_module_name);
    wrapped.push_str("\" {\n");

    for line in body_lines {
        let Some(rewritten) = rewrite_ambient_module_member_line(&line, &amd_module_name) else {
            continue;
        };
        wrapped.push_str("    ");
        wrapped.push_str(&rewritten);
        wrapped.push('\n');
    }

    wrapped.push('}');
    Some(wrapped)
}

fn extract_amd_module_name(line: &str) -> Option<String> {
    let needle = "name=";
    let pos = line.find(needle)?;
    let after = &line[pos + needle.len()..];
    let quote = after.as_bytes().first().copied()?;
    if !matches!(quote, b'\'' | b'"') {
        return None;
    }
    let quote = quote as char;
    let end = after[1..].find(quote)?;
    Some(after[1..1 + end].to_string())
}

fn is_amd_module_directive(line: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix("///") else {
        return false;
    };
    let Some(after_tag) = rest.trim_start().strip_prefix("<amd-module") else {
        return false;
    };
    match after_tag.chars().next() {
        None => true,
        Some(ch) => ch.is_ascii_whitespace() || matches!(ch, '/' | '>'),
    }
}

fn rewrite_ambient_module_member_line(line: &str, module_name: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];

    if trimmed == "export {};" {
        return None;
    }

    if let Some(rest) = trimmed.strip_prefix("export declare ") {
        return Some(rewrite_amd_relative_module_specifier_line(
            format!("{indent}export {rest}"),
            module_name,
        ));
    }
    if let Some(rest) = trimmed.strip_prefix("declare ") {
        return Some(rewrite_amd_relative_module_specifier_line(
            format!("{indent}{rest}"),
            module_name,
        ));
    }

    Some(rewrite_amd_relative_module_specifier_line(
        line.to_string(),
        module_name,
    ))
}

fn rewrite_amd_relative_module_specifier_line(line: String, module_name: &str) -> String {
    let trimmed = line.trim_start();
    let Some(after_keyword) = trimmed
        .strip_prefix("module \"")
        .or_else(|| trimmed.strip_prefix("import \""))
        .or_else(|| {
            trimmed
                .strip_prefix("import ")
                .and_then(|rest| rest.rsplit_once(" from \"").map(|(_, spec)| spec))
        })
    else {
        return line;
    };
    let Some(end_quote) = after_keyword.find('"') else {
        return line;
    };
    let specifier = &after_keyword[..end_quote];
    if !specifier.starts_with('.') {
        return line;
    }
    let Some(resolved) = resolve_amd_relative_module_specifier(module_name, specifier) else {
        return line;
    };

    let spec_start = line.len() - trimmed.len()
        + trimmed
            .find(specifier)
            .expect("specifier came from trimmed line");
    let spec_end = spec_start + specifier.len();
    let mut rewritten = String::with_capacity(line.len() + resolved.len());
    rewritten.push_str(&line[..spec_start]);
    rewritten.push_str(&resolved);
    rewritten.push_str(&line[spec_end..]);
    rewritten
}

fn resolve_amd_relative_module_specifier(module_name: &str, specifier: &str) -> Option<String> {
    let base_dir = module_name
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("");
    let mut parts: Vec<&str> = if base_dir.is_empty() {
        Vec::new()
    } else {
        base_dir.split('/').collect()
    };
    for part in specifier.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    if let Some(last) = parts.last_mut() {
        *last = last
            .strip_suffix(".ts")
            .or_else(|| last.strip_suffix(".tsx"))
            .or_else(|| last.strip_suffix(".js"))
            .or_else(|| last.strip_suffix(".jsx"))
            .unwrap_or(last);
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn output_relative_path(base_dir: &Path, root_dir: Option<&Path>, input_path: &Path) -> PathBuf {
    if let Some(root_dir) = root_dir
        && let Ok(relative) = input_path.strip_prefix(root_dir)
    {
        return relative.to_path_buf();
    }

    input_path
        .strip_prefix(base_dir)
        .unwrap_or(input_path)
        .to_path_buf()
}

fn should_emit_next_to_source(
    root_dir: Option<&Path>,
    out_dir: Option<&Path>,
    input_path: &Path,
) -> bool {
    root_dir.is_some_and(|root_dir| input_path.strip_prefix(root_dir).is_err()) && out_dir.is_some()
}

fn bundled_module_name(
    base_dir: &Path,
    root_dir: Option<&Path>,
    input_path: &Path,
) -> Option<String> {
    let mut relative = output_relative_path(base_dir, root_dir, input_path);
    relative.set_extension("");
    let module_name = relative.to_string_lossy().replace('\\', "/");
    (!module_name.is_empty()).then_some(module_name)
}

fn declaration_file_name(file_name: &str) -> Option<String> {
    if file_name.ends_with(".mts") {
        return Some(file_name.trim_end_matches(".mts").to_string() + ".d.mts");
    }
    if file_name.ends_with(".mjs") {
        return Some(file_name.trim_end_matches(".mjs").to_string() + ".d.mts");
    }
    if file_name.ends_with(".cts") {
        return Some(file_name.trim_end_matches(".cts").to_string() + ".d.cts");
    }
    if file_name.ends_with(".cjs") {
        return Some(file_name.trim_end_matches(".cjs").to_string() + ".d.cts");
    }
    if file_name.ends_with(".tsx") {
        return Some(file_name.trim_end_matches(".tsx").to_string() + ".d.ts");
    }
    if file_name.ends_with(".ts") || file_name.ends_with(".jsx") || file_name.ends_with(".js") {
        let suffix = if file_name.ends_with(".ts") {
            ".ts"
        } else if file_name.ends_with(".jsx") {
            ".jsx"
        } else {
            ".js"
        };
        return Some(file_name.trim_end_matches(suffix).to_string() + ".d.ts");
    }

    None
}

fn js_extension_for(path: &Path, jsx: Option<JsxEmit>) -> Option<&'static str> {
    let name = path.file_name().and_then(|name| name.to_str())?;
    if name.ends_with(".mts") {
        return Some("mjs");
    }
    if name.ends_with(".cts") {
        return Some("cjs");
    }

    match path.extension().and_then(|ext| ext.to_str()) {
        Some("tsx") => match jsx {
            Some(JsxEmit::Preserve) => Some("jsx"),
            Some(JsxEmit::React)
            | Some(JsxEmit::ReactJsx)
            | Some(JsxEmit::ReactJsxDev)
            | Some(JsxEmit::ReactNative)
            | None => Some("js"),
        },
        // .ts files emit as .js. JS input files (.js, .jsx, .mjs, .cjs) are valid
        // inputs that go through the emit pipeline (adding "use strict" for
        // alwaysStrict, module transforms, etc.) and produce output with the same
        // extension. This matches tsc behavior where `allowJs` files are emitted
        // alongside .ts files.
        Some("ts") | Some("js") => Some("js"),
        Some("jsx") => Some("jsx"),
        Some("mjs") => Some("mjs"),
        Some("cjs") => Some("cjs"),
        _ => None,
    }
}

fn js_input_skipped_by_node_modules_depth(path: &Path, max_depth: u32) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    if !matches!(ext, "js" | "jsx" | "mjs" | "cjs") {
        return false;
    }
    let depth = path
        .components()
        .filter(|component| component.as_os_str() == "node_modules")
        .count() as u32;
    depth > max_depth
}

fn build_ambient_global_type_only_names(
    program: &MergedProgram,
    preserve_const_enums: bool,
) -> FxHashSet<String> {
    let mut type_only_names = FxHashSet::default();
    let mut value_names = FxHashSet::default();

    for file in &program.files {
        let input_path = PathBuf::from(&file.file_name);
        if !is_declaration_file(&input_path) {
            continue;
        }

        let Some(source) = file
            .arena
            .get(file.source_file)
            .and_then(|node| file.arena.get_source_file(node))
        else {
            continue;
        };

        if source_file_has_top_level_module_syntax(&file.arena, &source.statements.nodes) {
            continue;
        }

        type_only_names.extend(
            tsz_emitter::transforms::module_commonjs::build_type_only_declaration_names(
                &file.arena,
                &source.statements.nodes,
                preserve_const_enums,
            ),
        );
        value_names.extend(
            tsz_emitter::transforms::module_commonjs::build_value_declaration_names(
                &file.arena,
                &source.statements.nodes,
                preserve_const_enums,
            ),
        );
    }

    type_only_names.retain(|name| !value_names.contains(name));
    type_only_names
}

fn build_type_only_export_equals_modules(
    program: &MergedProgram,
    preserve_const_enums: bool,
) -> FxHashSet<String> {
    let mut modules = FxHashSet::default();

    for file in &program.files {
        let input_path = PathBuf::from(&file.file_name);
        if !is_declaration_file(&input_path) {
            continue;
        }

        let Some(source) = file
            .arena
            .get(file.source_file)
            .and_then(|node| file.arena.get_source_file(node))
        else {
            continue;
        };

        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = file.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            let Some(module) = file.arena.get_module(stmt_node) else {
                continue;
            };
            if !file
                .arena
                .has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword)
            {
                continue;
            }
            let Some(module_name) = file
                .arena
                .get(module.name)
                .and_then(|node| file.arena.get_literal(node))
                .map(|lit| lit.text.clone())
            else {
                continue;
            };
            let Some(statements) = module_block_statements(&file.arena, module.body) else {
                continue;
            };
            let Some(export_name) = export_equals_identifier_name(&file.arena, statements) else {
                continue;
            };
            let value_names =
                tsz_emitter::transforms::module_commonjs::build_value_declaration_names(
                    &file.arena,
                    statements,
                    preserve_const_enums,
                );

            if !value_names.contains(&export_name) {
                modules.insert(module_name);
            }
        }
    }

    modules
}

fn module_block_statements(arena: &NodeArena, body_idx: NodeIndex) -> Option<&[NodeIndex]> {
    let body_node = arena.get(body_idx)?;
    let block = arena.get_module_block(body_node)?;
    block
        .statements
        .as_ref()
        .map(|statements| statements.nodes.as_slice())
}

fn export_equals_identifier_name(arena: &NodeArena, statements: &[NodeIndex]) -> Option<String> {
    statements.iter().find_map(|&stmt_idx| {
        let node = arena.get(stmt_idx)?;
        if node.kind != syntax_kind_ext::EXPORT_ASSIGNMENT {
            return None;
        }
        let export_assignment = arena.get_export_assignment(node)?;
        if !export_assignment.is_export_equals {
            return None;
        }
        arena.identifier_text_owned(export_assignment.expression)
    })
}

fn source_file_has_top_level_module_syntax(arena: &NodeArena, statements: &[NodeIndex]) -> bool {
    statements.iter().any(|&stmt_idx| {
        let Some(node) = arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::IMPORT_DECLARATION
                || k == syntax_kind_ext::EXPORT_DECLARATION
                || k == syntax_kind_ext::EXPORT_ASSIGNMENT =>
            {
                true
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                let Some(import_data) = arena.get_import_decl(node) else {
                    return false;
                };
                let Some(spec_node) = arena.get(import_data.module_specifier) else {
                    return false;
                };
                spec_node.kind == SyntaxKind::StringLiteral as u16
                    || spec_node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
            }
            _ => false,
        }
    })
}

fn mark_ambient_global_type_only_export_specifiers(
    arena: &NodeArena,
    source_file_idx: NodeIndex,
    ambient_global_type_only_names: &FxHashSet<String>,
    type_only_nodes: &mut FxHashSet<NodeIndex>,
) {
    if ambient_global_type_only_names.is_empty() {
        return;
    }

    let Some(source) = arena
        .get(source_file_idx)
        .and_then(|node| arena.get_source_file(node))
    else {
        return;
    };

    for &stmt_idx in &source.statements.nodes {
        let Some(node) = arena.get(stmt_idx) else {
            continue;
        };
        if node.kind != syntax_kind_ext::EXPORT_DECLARATION {
            continue;
        }

        let Some(export_decl) = arena.get_export_decl(node) else {
            continue;
        };
        if export_decl.is_type_only || export_decl.module_specifier.is_some() {
            continue;
        }

        let Some(clause_node) = arena.get(export_decl.export_clause) else {
            continue;
        };
        let Some(named_exports) = arena.get_named_imports(clause_node) else {
            continue;
        };

        for &spec_idx in &named_exports.elements.nodes {
            let Some(spec) = arena.get_specifier_at(spec_idx) else {
                continue;
            };
            if spec.is_type_only {
                continue;
            }

            let local_name_idx = if spec.property_name.is_some() {
                spec.property_name
            } else {
                spec.name
            };
            if let Some(local_name) = arena.identifier_text_owned(local_name_idx)
                && ambient_global_type_only_names.contains(&local_name)
            {
                type_only_nodes.insert(spec_idx);
            }
        }
    }
}

fn collect_bundled_duplicate_var_names(program: &MergedProgram) -> FxHashSet<String> {
    let mut counts: FxHashMap<String, usize> = FxHashMap::default();
    for file in &program.files {
        for (name, _) in collect_top_level_var_declaration_types(&file.arena, file.source_file) {
            *counts.entry(name).or_default() += 1;
        }
    }

    counts
        .into_iter()
        .filter_map(|(name, count)| (count > 1).then_some(name))
        .collect()
}

fn build_bundled_prior_duplicate_var_types_by_file(
    program: &MergedProgram,
    duplicate_names: &FxHashSet<String>,
) -> Vec<FxHashMap<String, String>> {
    let mut prior_types = FxHashMap::default();
    let mut by_file = Vec::with_capacity(program.files.len());

    for file in &program.files {
        by_file.push(prior_types.clone());
        for (name, type_text) in
            collect_top_level_var_declaration_types(&file.arena, file.source_file)
        {
            if duplicate_names.contains(&name) {
                prior_types.insert(name, type_text);
            }
        }
    }

    by_file
}

fn collect_top_level_var_declaration_types(
    arena: &NodeArena,
    source_file_idx: NodeIndex,
) -> Vec<(String, String)> {
    let Some(source) = arena
        .get(source_file_idx)
        .and_then(|node| arena.get_source_file(node))
    else {
        return Vec::new();
    };

    let mut declarations = Vec::new();
    for &stmt_idx in &source.statements.nodes {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            continue;
        };
        if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            continue;
        }
        let Some(var_stmt) = arena.get_variable(stmt_node) else {
            continue;
        };
        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = arena.get(decl_list_idx) else {
                continue;
            };
            if decl_list_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                continue;
            }
            let flags = decl_list_node.flags as u32;
            if flags
                & (tsz_parser::parser::node_flags::LET
                    | tsz_parser::parser::node_flags::CONST
                    | tsz_parser::parser::node_flags::USING)
                != 0
            {
                continue;
            }
            let Some(decl_list) = arena.get_variable(decl_list_node) else {
                continue;
            };
            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                let Some(name) = arena.identifier_text_owned(decl.name) else {
                    continue;
                };
                let type_text = primitive_variable_type_text(arena, decl.type_annotation)
                    .or_else(|| primitive_variable_type_text(arena, decl.initializer))
                    .unwrap_or_else(|| "any".to_string());
                declarations.push((name, type_text));
            }
        }
    }

    declarations
}

fn primitive_variable_type_text(arena: &NodeArena, node_idx: NodeIndex) -> Option<String> {
    let node = arena.get(node_idx)?;
    match node.kind {
        k if k == SyntaxKind::StringKeyword as u16 => Some("string".to_string()),
        k if k == SyntaxKind::NumberKeyword as u16 => Some("number".to_string()),
        k if k == SyntaxKind::BooleanKeyword as u16 => Some("boolean".to_string()),
        k if k == SyntaxKind::StringLiteral as u16
            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
        {
            Some("string".to_string())
        }
        k if k == SyntaxKind::NumericLiteral as u16 || k == SyntaxKind::BigIntLiteral as u16 => {
            Some("number".to_string())
        }
        k if k == SyntaxKind::TrueKeyword as u16 || k == SyntaxKind::FalseKeyword as u16 => {
            Some("boolean".to_string())
        }
        _ => None,
    }
}

pub(crate) fn normalize_base_url(base_dir: &Path, dir: Option<PathBuf>) -> Option<PathBuf> {
    dir.map(|dir| {
        let resolved = if dir.is_absolute() || is_windows_absolute_like(&dir) {
            dir
        } else {
            base_dir.join(dir)
        };
        canonicalize_or_owned(&resolved)
    })
}

fn is_windows_absolute_like(path: &Path) -> bool {
    let Some(path) = path.to_str() else {
        return false;
    };

    let bytes = path.as_bytes();
    if bytes.len() < 3 {
        return false;
    }

    (bytes[1] == b':' && (bytes[2] == b'/' || bytes[2] == b'\\')) || path.starts_with("\\\\")
}

pub(crate) fn normalize_output_dir(base_dir: &Path, dir: Option<PathBuf>) -> Option<PathBuf> {
    dir.map(|dir| {
        if dir.is_absolute() {
            canonicalize_with_missing_tail(&dir)
        } else {
            canonicalize_with_missing_tail(&base_dir.join(dir))
        }
    })
}

pub(crate) fn normalize_root_dir(base_dir: &Path, dir: Option<PathBuf>) -> Option<PathBuf> {
    dir.map(|dir| {
        let resolved = if dir.is_absolute() {
            dir
        } else {
            base_dir.join(dir)
        };
        canonicalize_or_owned(&resolved)
    })
}

pub(crate) fn normalize_root_dirs(base_dir: &Path, roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots
        .into_iter()
        .map(|root| {
            let resolved = if root.is_absolute() {
                root
            } else {
                base_dir.join(root)
            };
            canonicalize_with_missing_tail(&resolved)
        })
        .collect()
}

pub(crate) fn normalize_type_roots(
    base_dir: &Path,
    roots: Option<Vec<PathBuf>>,
) -> Option<Vec<PathBuf>> {
    let roots = roots?;
    let mut normalized = Vec::new();
    for root in roots {
        let resolved = if root.is_absolute() {
            root
        } else {
            base_dir.join(root)
        };
        // Match tsc: absolute typeRoots paths are used as-is.
        // If the path doesn't exist on disk, it's simply skipped (no fallback).
        let resolved = canonicalize_or_owned(&resolved);
        if resolved.is_dir() {
            normalized.push(resolved);
        }
    }
    Some(normalized)
}

/// Convert config `JsxEmit` to emitter `JsxEmit`.
const fn config_jsx_to_emitter_jsx(jsx: JsxEmit) -> tsz::emitter::JsxEmit {
    match jsx {
        JsxEmit::Preserve => tsz::emitter::JsxEmit::Preserve,
        JsxEmit::React => tsz::emitter::JsxEmit::React,
        JsxEmit::ReactJsx => tsz::emitter::JsxEmit::ReactJsx,
        JsxEmit::ReactJsxDev => tsz::emitter::JsxEmit::ReactJsxDev,
        JsxEmit::ReactNative => tsz::emitter::JsxEmit::ReactNative,
    }
}

#[cfg(test)]
#[path = "emit_tests.rs"]
mod emit_tests;
