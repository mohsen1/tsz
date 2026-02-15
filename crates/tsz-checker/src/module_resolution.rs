//! Module resolution utilities for multi-file type checking.
//!
//! This module provides shared utilities for building cross-file module
//! resolution context. Used by both the CLI and the tsz-server.
//!
//! Supports the following import forms:
//! - ES imports: `import { x } from "./module"`
//! - Require: `const x = require("./module")`
//! - Import equals: `import x = require("./module")`
//! - Dynamic import: `const x = await import("./module")`
//! - Re-exports: `export { x } from "./module"`
//!
//! All of these ultimately resolve against the same specifier-to-file mapping
//! built by `build_module_resolution_maps`.

use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::path::Path;

/// TypeScript file extensions in resolution priority order.
/// `.d.ts` must be checked before `.ts` to avoid `.d` being left as a stem artifact.
const TS_EXTENSIONS: &[&str] = &[
    ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs",
    ".cjs",
];

/// Strip a known TypeScript/JavaScript extension from a file path string.
/// Returns the path without the extension, or the original string if no known extension matched.
fn strip_ts_extension(path: &str) -> &str {
    for ext in TS_EXTENSIONS {
        if let Some(stripped) = path.strip_suffix(ext) {
            return stripped;
        }
    }
    path
}

/// Compute a relative path from `from_dir` to `to_path`, returning a string
/// suitable for use as a module specifier (with `./` or `../` prefix).
///
/// Returns `None` if a relative path cannot be computed (e.g., different drive roots on Windows).
fn relative_specifier(from_dir: &Path, to_path: &Path) -> Option<String> {
    // Try the simple case: target is inside from_dir
    if let Ok(rel) = to_path.strip_prefix(from_dir) {
        let rel_str = rel.to_string_lossy();
        let without_ext = strip_ts_extension(&rel_str);
        return Some(format!("./{without_ext}"));
    }

    // Walk up from from_dir to find a common ancestor
    let mut up_count = 0;
    let mut ancestor = from_dir;
    loop {
        match ancestor.parent() {
            Some(parent) => {
                up_count += 1;
                if let Ok(rel) = to_path.strip_prefix(parent) {
                    let rel_str = rel.to_string_lossy();
                    let without_ext = strip_ts_extension(&rel_str);
                    let prefix = "../".repeat(up_count);
                    return Some(format!("{prefix}{without_ext}"));
                }
                ancestor = parent;
            }
            None => return None,
        }
    }
}

/// Build module resolution maps from a list of file paths.
///
/// Returns:
/// - `resolved_module_paths`: Maps (`source_file_idx`, specifier) -> `target_file_idx`
/// - `resolved_modules`: Set of all valid module specifiers
///
/// This handles relative imports between files in the same project.
/// For example, if we have files `/tmp/test/main.ts` and `/tmp/test/types.ts`,
/// then from `main.ts`, the specifier `./types` will resolve to `types.ts`.
///
/// Also handles:
/// - Nested paths: `./lib/utils` resolving to `lib/utils.ts`
/// - Parent directory: `../sibling` resolving to `../sibling.ts`
/// - Index files: `./dir` resolving to `dir/index.ts`
/// - Declaration files: `./types` resolving to `types.d.ts`
/// - All TS/JS extensions: `.ts`, `.tsx`, `.d.ts`, `.js`, `.jsx`, `.mts`, `.cts`, etc.
pub fn build_module_resolution_maps(
    file_names: &[String],
) -> (FxHashMap<(usize, String), usize>, FxHashSet<String>) {
    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();

    // Build a map from extensionless path -> file index for index file resolution
    let mut stem_to_idx: FxHashMap<String, usize> = FxHashMap::default();
    for (idx, name) in file_names.iter().enumerate() {
        let without_ext = strip_ts_extension(name);
        stem_to_idx.insert(without_ext.to_string(), idx);
    }

    for (src_idx, src_name) in file_names.iter().enumerate() {
        let src_path = Path::new(src_name);
        let Some(src_dir) = src_path.parent() else {
            continue;
        };

        for (tgt_idx, tgt_name) in file_names.iter().enumerate() {
            if src_idx == tgt_idx {
                continue;
            }

            let tgt_path = Path::new(tgt_name);

            // Compute the relative specifier from source directory to target file
            if let Some(specifier) = relative_specifier(src_dir, tgt_path) {
                // Register the specifier with ./ or ../ prefix
                resolved_module_paths.insert((src_idx, specifier.clone()), tgt_idx);
                resolved_modules.insert(specifier.clone());

                // Also register without ./ prefix for bare relative specifiers
                // (e.g., "types" in addition to "./types")
                if let Some(bare) = specifier.strip_prefix("./") {
                    resolved_module_paths.insert((src_idx, bare.to_string()), tgt_idx);
                    resolved_modules.insert(bare.to_string());
                }
            }

            // Index file resolution: if target is `dir/index.ts`, also register `./dir`
            let tgt_stem = strip_ts_extension(tgt_name);
            if let Some(dir_path) = tgt_stem.strip_suffix("/index") {
                let dir_as_path = Path::new(dir_path);
                if let Some(dir_specifier) = relative_specifier(src_dir, dir_as_path) {
                    resolved_module_paths.insert((src_idx, dir_specifier.clone()), tgt_idx);
                    resolved_modules.insert(dir_specifier.clone());

                    if let Some(bare) = dir_specifier.strip_prefix("./") {
                        resolved_module_paths.insert((src_idx, bare.to_string()), tgt_idx);
                        resolved_modules.insert(bare.to_string());
                    }
                }
            }
        }
    }

    (resolved_module_paths, resolved_modules)
}

/// Build canonical lookup keys for a module specifier.
///
/// Invariant: every cross-module map lookup in checker code should go through
/// this function to avoid divergent quoting/slash normalization behavior.
pub fn module_specifier_candidates(specifier: &str) -> Vec<String> {
    let mut candidates = Vec::with_capacity(5);
    let mut push_unique = |value: String| {
        if !candidates.contains(&value) {
            candidates.push(value);
        }
    };

    push_unique(specifier.to_string());

    let trimmed = specifier.trim().trim_matches('"').trim_matches('\'');
    if trimmed != specifier {
        push_unique(trimmed.to_string());
    }
    if !trimmed.is_empty() {
        push_unique(format!("\"{trimmed}\""));
        push_unique(format!("'{trimmed}'"));
        if trimmed.contains('\\') {
            push_unique(trimmed.replace('\\', "/"));
        }
    }

    candidates
}

#[cfg(test)]
#[path = "../tests/module_resolution.rs"]
mod tests;
