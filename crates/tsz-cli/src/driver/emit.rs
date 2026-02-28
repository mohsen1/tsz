use anyhow::{Context, Result};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};

use super::resolution::{
    canonicalize_or_owned, implied_resolution_mode_for_file, is_declaration_file,
};
use crate::config::{JsxEmit, ResolvedCompilerOptions};
use tsz::declaration_emitter::DeclarationEmitter;
use tsz::emitter::{NewLineKind, Printer};
use tsz::parallel::MergedProgram;
use tsz_common::common::ModuleKind;

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

pub(crate) fn emit_outputs(context: EmitOutputsContext<'_>) -> Result<Vec<OutputFile>> {
    let mut outputs = Vec::new();
    let new_line = new_line_str(context.options.printer.new_line);

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

    for (file_idx, file) in context.program.files.iter().enumerate() {
        let input_path = PathBuf::from(&file.file_name);
        if let Some(dirty_paths) = context.dirty_paths
            && !dirty_paths.contains(&input_path)
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

            // For Node16/NodeNext, resolve the per-file module format based on
            // file extension and nearest package.json "type" field.
            // .mts/.mjs -> ESM, .cts/.cjs -> CJS, .ts/.js -> depends on package.json
            if printer_options.module.is_node_module() {
                let mode = implied_resolution_mode_for_file(&input_path, context.base_dir);
                printer_options.module = if mode == "import" {
                    ModuleKind::ESNext
                } else {
                    ModuleKind::CommonJS
                };
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

            outputs.push(OutputFile {
                path: js_path,
                contents,
            });
            if let Some(map_output) = map_output {
                outputs.push(map_output);
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
                        node_types: cache.node_types.clone(),
                        symbol_types: cache.symbol_types.clone(),
                        def_to_symbol: cache.def_to_symbol.clone(),
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
                    emitter
                } else {
                    let mut emitter = DeclarationEmitter::new(&file.arena);
                    // Still set binder even without cache for consistency
                    emitter.set_binder(Some(&binder));
                    emitter.set_arena_to_path(arena_to_path.clone());
                    emitter.set_remove_comments(context.options.printer.remove_comments);
                    emitter
                };
                let map_info = if context.options.declaration_map {
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
                        node_types: cache.node_types.clone(),
                        symbol_types: cache.symbol_types.clone(),
                        def_to_symbol: cache.def_to_symbol.clone(),
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

    Ok(outputs)
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
    if file_name.ends_with(".cts") {
        return Some(file_name.trim_end_matches(".cts").to_string() + ".d.cts");
    }
    if file_name.ends_with(".tsx") {
        return Some(file_name.trim_end_matches(".tsx").to_string() + ".d.ts");
    }
    if file_name.ends_with(".ts") {
        return Some(file_name.trim_end_matches(".ts").to_string() + ".d.ts");
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
            dir
        } else {
            base_dir.join(dir)
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

#[cfg(test)]
#[path = "emit_tests.rs"]
mod emit_tests;
