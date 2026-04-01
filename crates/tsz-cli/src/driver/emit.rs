use anyhow::{Context, Result};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};

use super::resolution::{
    canonicalize_or_owned, canonicalize_with_missing_tail, implied_resolution_mode_for_file,
    is_declaration_file,
};
use crate::config::{JsxEmit, ResolvedCompilerOptions};
use tsz::declaration_emitter::DeclarationEmitter;
use tsz::emitter::{NewLineKind, Printer};
use tsz::parallel::MergedProgram;
use tsz_common::common::ModuleKind;
use tsz_common::diagnostics::Diagnostic;

#[derive(Debug, Clone)]
pub(crate) struct OutputFile {
    pub(crate) path: PathBuf,
    pub(crate) contents: String,
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

    // When --outFile is set and there are multiple source files, collect JS
    // chunks and concatenate at the end instead of emitting individual files.
    // Single-file tests emit normally (bundle would just wrap the same content).
    let has_multiple_files = context.program.files.len() > 1;
    let js_bundle_path: Option<PathBuf> = if has_multiple_files {
        context.options.out_file.clone()
    } else {
        None
    };
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

        if let Some(js_path) = js_output_path(
            context.base_dir,
            context.root_dir,
            context.out_dir,
            context.options.jsx,
            &input_path,
        ) {
            // Get type_only_nodes from the type cache (if available)
            let type_only_nodes = context.type_caches.get(&input_path).map_or_else(
                || std::sync::Arc::new(rustc_hash::FxHashSet::default()),
                |cache| std::sync::Arc::new(cache.type_only_nodes.clone()),
            );

            // Clone and update printer options with type_only_nodes
            let mut printer_options = context.options.printer.clone();
            printer_options.type_only_nodes = type_only_nodes;

            // Wire JSX options from resolved compiler options to printer
            if let Some(jsx) = context.options.jsx {
                printer_options.jsx = config_jsx_to_emitter_jsx(jsx);
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

            // tsc's isFileForcedToBeModuleByFormat: .cjs/.cts/.mjs/.mts files are
            // always modules in "auto" mode. This applies even when the config-level
            // module_detection_force is false (e.g., explicit moduleDetection=auto).
            if !printer_options.module_detection_force && (is_cts_or_cjs || is_mts_or_mjs) {
                printer_options.module_detection_force = true;
            }

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

            let map_info = if context.options.source_map {
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
                append_source_mapping_url(&mut contents, &map_name, new_line);
                map_output = Some(OutputFile {
                    path: map_path,
                    contents: map_json,
                });
            }

            // When --outFile is set, collect content for bundling.
            if js_bundle_path.is_some() {
                js_bundle_chunks.push(contents);
            } else {
                outputs.push(OutputFile {
                    path: js_path,
                    contents,
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
                        def_types: FxHashMap::default(),
                        def_type_params: FxHashMap::default(),
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
                    emitter.set_remove_comments(context.options.printer.remove_comments);
                    emitter.set_strip_internal(context.options.strip_internal);
                    emitter.set_strict_null_checks(context.options.checker.strict_null_checks);
                    emitter.set_files_with_augmentations(files_with_augmentations.clone());
                    emitter
                } else {
                    let mut emitter = DeclarationEmitter::new(&file.arena);
                    // Still set binder even without cache for consistency
                    emitter.set_binder(Some(&binder));
                    emitter.set_arena_to_path(arena_to_path.clone());
                    emitter.set_remove_comments(context.options.printer.remove_comments);
                    emitter.set_strip_internal(context.options.strip_internal);
                    emitter.set_strict_null_checks(context.options.checker.strict_null_checks);
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

                if let Some((_, _, output_name)) = map_info.as_ref() {
                    if let Some(source_text) = file
                        .arena
                        .get(file.source_file)
                        .and_then(|node| file.arena.get_source_file(node))
                        .map(|source| source.text.as_ref())
                    {
                        emitter.set_source_map_text(source_text);
                    }
                    emitter.enable_source_map(output_name, &file.file_name);
                }

                // Run usage analysis and calculate required imports if we have type cache
                if let Some(ref cache) = type_cache {
                    use rustc_hash::FxHashMap;
                    use tsz::declaration_emitter::usage_analyzer::UsageAnalyzer;
                    use tsz_emitter::type_cache_view::TypeCacheView;

                    // Empty import_name_map for this usage (not needed for auto-import calculation)
                    let import_name_map = FxHashMap::default();
                    let cache_view = TypeCacheView {
                        node_types: cache.node_types.to_hash_map(),
                        symbol_types: cache.symbol_types.to_hash_map(),
                        def_to_symbol: cache.def_to_symbol.clone(),
                        def_types: FxHashMap::default(),
                        def_type_params: FxHashMap::default(),
                    };

                    let mut analyzer = UsageAnalyzer::new(
                        &file.arena,
                        &binder,
                        &cache_view,
                        &context.program.type_interner,
                        std::sync::Arc::clone(&file.arena),
                        &import_name_map,
                    );

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
                    .any(|diagnostic| diagnostic.code == 7056);
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
                    });
                }

                if declaration_bundle_path.is_some() {
                    declaration_bundle_chunks.push(bundle_declaration_output(
                        &contents,
                        context.options.printer.module,
                    ));
                } else {
                    outputs.push(OutputFile {
                        path: dts_path,
                        contents,
                    });
                    if let Some(map_output) = map_output {
                        outputs.push(map_output);
                    }
                }
            }
        }
    }

    // Emit bundled JS output when --outFile is set
    if let Some(bundle_path) = js_bundle_path
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
        outputs.push(OutputFile {
            path: bundle_path,
            contents: bundled,
        });
    }

    if let Some(bundle_path) = declaration_bundle_path
        && !declaration_bundle_chunks.is_empty()
        && !declaration_bundle_blocked
    {
        outputs.push(OutputFile {
            path: bundle_path,
            contents: join_declaration_bundle_chunks(&declaration_bundle_chunks, new_line),
        });
    }

    Ok((outputs, emit_diagnostics))
}

fn normalize_ts2883_diagnostics(diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut canonical_sites = FxHashSet::default();
    let mut exact_seen = FxHashSet::default();
    let mut unique = Vec::new();

    for diagnostic in diagnostics {
        let exact_key = (
            diagnostic.code,
            diagnostic.file.clone(),
            diagnostic.start,
            diagnostic.length,
            diagnostic.message_text.clone(),
        );
        if !exact_seen.insert(exact_key) {
            continue;
        }

        if diagnostic.code == 2883
            && let Some((first, second)) =
                parse_ts2883_named_reference_message(&diagnostic.message_text)
            && !looks_like_module_path(&first)
            && looks_like_module_path(&second)
        {
            canonical_sites.insert((diagnostic.file.clone(), diagnostic.start, diagnostic.length));
        }

        unique.push(diagnostic);
    }

    unique
        .into_iter()
        .filter(|diagnostic| {
            if diagnostic.code != 2883 {
                return true;
            }

            let Some((first, second)) =
                parse_ts2883_named_reference_message(&diagnostic.message_text)
            else {
                return true;
            };

            if !looks_like_module_path(&first) || looks_like_module_path(&second) {
                return true;
            }

            !canonical_sites.contains(&(
                diagnostic.file.clone(),
                diagnostic.start,
                diagnostic.length,
            ))
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

fn append_source_mapping_url(contents: &mut String, map_name: &str, new_line: &str) {
    if !contents.is_empty() && !contents.ends_with(new_line) {
        contents.push_str(new_line);
    }
    contents.push_str("//# sourceMappingURL=");
    contents.push_str(map_name);
}

const fn new_line_str(kind: NewLineKind) -> &'static str {
    match kind {
        NewLineKind::LineFeed => "\n",
        NewLineKind::CarriageReturnLineFeed => "\r\n",
    }
}

pub(crate) fn write_outputs(outputs: &[OutputFile]) -> Result<Vec<PathBuf>> {
    outputs.par_iter().try_for_each(|output| -> Result<()> {
        if let Some(parent) = output.path.parent() {
            std::fs::create_dir_all::<&Path>(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
        std::fs::write(&output.path, &output.contents)
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
    let relative = output_relative_path(base_dir, root_dir, input_path);
    let mut output = match out_dir {
        Some(out_dir) => out_dir.join(relative),
        None => input_path.to_path_buf(),
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

    let mut output = match out_dir {
        Some(out_dir) => out_dir.join(relative),
        None => input_path.to_path_buf(),
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

fn join_declaration_bundle_chunks(chunks: &[String], new_line: &str) -> String {
    let mut bundled = String::new();
    for chunk in chunks {
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

fn bundle_declaration_output(contents: &str, module_kind: ModuleKind) -> String {
    if !matches!(module_kind, ModuleKind::AMD) {
        return contents.to_string();
    }

    wrap_amd_declaration_output(contents).unwrap_or_else(|| contents.to_string())
}

fn wrap_amd_declaration_output(contents: &str) -> Option<String> {
    let mut directive_lines = Vec::new();
    let mut body_lines = Vec::new();
    let mut in_header = true;
    let mut amd_module_name = None;

    for line in contents.lines() {
        if in_header && line.trim_start().starts_with("///") {
            directive_lines.push(line.to_string());
            if amd_module_name.is_none() {
                amd_module_name = extract_amd_module_name(line);
            }
            continue;
        }
        in_header = false;
        body_lines.push(line.to_string());
    }

    let amd_module_name = amd_module_name?;
    let mut wrapped = String::new();
    for directive in directive_lines {
        wrapped.push_str(&directive);
        wrapped.push('\n');
    }
    wrapped.push_str("declare module \"");
    wrapped.push_str(&amd_module_name);
    wrapped.push_str("\" {\n");

    for line in body_lines {
        let rewritten = rewrite_ambient_module_member_line(&line);
        if rewritten.is_empty() {
            wrapped.push('\n');
            continue;
        }
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

fn rewrite_ambient_module_member_line(line: &str) -> String {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];

    if let Some(rest) = trimmed.strip_prefix("export declare ") {
        return format!("{indent}export {rest}");
    }
    if let Some(rest) = trimmed.strip_prefix("declare ") {
        return format!("{indent}{rest}");
    }

    line.to_string()
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
