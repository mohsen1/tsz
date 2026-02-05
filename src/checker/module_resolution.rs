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
use std::collections::HashSet;
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
        return Some(format!("./{}", without_ext));
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
                    return Some(format!("{}{}", prefix, without_ext));
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
/// - `resolved_module_paths`: Maps (source_file_idx, specifier) -> target_file_idx
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
) -> (FxHashMap<(usize, String), usize>, HashSet<String>) {
    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    let mut resolved_modules: HashSet<String> = HashSet::new();

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
            if tgt_stem.ends_with("/index") {
                let dir_path = &tgt_stem[..tgt_stem.len() - "/index".len()];
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

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // strip_ts_extension tests
    // =========================================================================

    #[test]
    fn test_strip_ts_extension() {
        assert_eq!(strip_ts_extension("foo.ts"), "foo");
        assert_eq!(strip_ts_extension("foo.tsx"), "foo");
        assert_eq!(strip_ts_extension("foo.js"), "foo");
        assert_eq!(strip_ts_extension("foo.jsx"), "foo");
        assert_eq!(strip_ts_extension("foo.d.ts"), "foo");
        assert_eq!(strip_ts_extension("foo.d.tsx"), "foo");
        assert_eq!(strip_ts_extension("foo.mts"), "foo");
        assert_eq!(strip_ts_extension("foo.cts"), "foo");
        assert_eq!(strip_ts_extension("foo.mjs"), "foo");
        assert_eq!(strip_ts_extension("foo.cjs"), "foo");
        assert_eq!(strip_ts_extension("foo.d.mts"), "foo");
        assert_eq!(strip_ts_extension("foo.d.cts"), "foo");
    }

    #[test]
    fn test_strip_ts_extension_with_path() {
        assert_eq!(strip_ts_extension("/tmp/test/foo.ts"), "/tmp/test/foo");
        assert_eq!(strip_ts_extension("lib/utils.d.ts"), "lib/utils");
        assert_eq!(strip_ts_extension("src/index.tsx"), "src/index");
    }

    #[test]
    fn test_strip_ts_extension_no_match() {
        assert_eq!(strip_ts_extension("foo"), "foo");
        assert_eq!(strip_ts_extension("foo.txt"), "foo.txt");
        assert_eq!(strip_ts_extension("foo.css"), "foo.css");
        assert_eq!(strip_ts_extension(""), "");
    }

    #[test]
    fn test_strip_dts_before_ts() {
        // .d.ts must be stripped as a whole, not just .ts leaving ".d"
        assert_eq!(strip_ts_extension("types.d.ts"), "types");
        assert_eq!(strip_ts_extension("globals.d.tsx"), "globals");
    }

    // =========================================================================
    // relative_specifier tests
    // =========================================================================

    #[test]
    fn test_relative_specifier_same_dir() {
        let from = Path::new("/tmp/test");
        let to = Path::new("/tmp/test/types.ts");
        assert_eq!(relative_specifier(from, to), Some("./types".to_string()));
    }

    #[test]
    fn test_relative_specifier_nested() {
        let from = Path::new("/tmp/test");
        let to = Path::new("/tmp/test/lib/utils.ts");
        assert_eq!(
            relative_specifier(from, to),
            Some("./lib/utils".to_string())
        );
    }

    #[test]
    fn test_relative_specifier_parent_dir() {
        let from = Path::new("/tmp/test/src");
        let to = Path::new("/tmp/test/lib/utils.ts");
        assert_eq!(
            relative_specifier(from, to),
            Some("../lib/utils".to_string())
        );
    }

    #[test]
    fn test_relative_specifier_two_levels_up() {
        let from = Path::new("/tmp/test/src/deep");
        let to = Path::new("/tmp/test/lib/utils.ts");
        assert_eq!(
            relative_specifier(from, to),
            Some("../../lib/utils".to_string())
        );
    }

    #[test]
    fn test_relative_specifier_dts_extension() {
        let from = Path::new("/tmp/test");
        let to = Path::new("/tmp/test/types.d.ts");
        assert_eq!(relative_specifier(from, to), Some("./types".to_string()));
    }

    // =========================================================================
    // build_module_resolution_maps - basic relative imports
    // =========================================================================

    #[test]
    fn test_simple_relative_import() {
        let files = vec![
            "/tmp/test/main.ts".to_string(),
            "/tmp/test/types.ts".to_string(),
        ];

        let (paths, modules) = build_module_resolution_maps(&files);

        assert_eq!(paths.get(&(0, "./types".to_string())), Some(&1));
        assert!(modules.contains("./types"));
        // Also available without ./ prefix
        assert_eq!(paths.get(&(0, "types".to_string())), Some(&1));
    }

    #[test]
    fn test_bidirectional_resolution() {
        let files = vec!["/tmp/test/a.ts".to_string(), "/tmp/test/b.ts".to_string()];

        let (paths, _) = build_module_resolution_maps(&files);

        // a can import b
        assert_eq!(paths.get(&(0, "./b".to_string())), Some(&1));
        // b can import a
        assert_eq!(paths.get(&(1, "./a".to_string())), Some(&0));
    }

    #[test]
    fn test_nested_import() {
        let files = vec![
            "/tmp/test/main.ts".to_string(),
            "/tmp/test/lib/utils.ts".to_string(),
        ];

        let (paths, modules) = build_module_resolution_maps(&files);

        assert_eq!(paths.get(&(0, "./lib/utils".to_string())), Some(&1));
        assert!(modules.contains("./lib/utils"));
    }

    #[test]
    fn test_deeply_nested_import() {
        let files = vec![
            "/tmp/test/main.ts".to_string(),
            "/tmp/test/src/lib/deep/module.ts".to_string(),
        ];

        let (paths, modules) = build_module_resolution_maps(&files);

        assert_eq!(
            paths.get(&(0, "./src/lib/deep/module".to_string())),
            Some(&1)
        );
        assert!(modules.contains("./src/lib/deep/module"));
    }

    #[test]
    fn test_parent_directory_import() {
        let files = vec![
            "/tmp/test/src/app.ts".to_string(),
            "/tmp/test/lib/utils.ts".to_string(),
        ];

        let (paths, modules) = build_module_resolution_maps(&files);

        // From src/app.ts, ../lib/utils should resolve to lib/utils.ts
        assert_eq!(paths.get(&(0, "../lib/utils".to_string())), Some(&1));
        assert!(modules.contains("../lib/utils"));
    }

    #[test]
    fn test_sibling_directory_import() {
        let files = vec![
            "/tmp/test/src/components/Button.tsx".to_string(),
            "/tmp/test/src/utils/helpers.ts".to_string(),
        ];

        let (paths, _) = build_module_resolution_maps(&files);

        assert_eq!(paths.get(&(0, "../utils/helpers".to_string())), Some(&1));
    }

    // =========================================================================
    // build_module_resolution_maps - extension handling
    // =========================================================================

    #[test]
    fn test_tsx_extension() {
        let files = vec![
            "/tmp/test/app.ts".to_string(),
            "/tmp/test/Button.tsx".to_string(),
        ];

        let (paths, _) = build_module_resolution_maps(&files);

        assert_eq!(paths.get(&(0, "./Button".to_string())), Some(&1));
    }

    #[test]
    fn test_dts_extension() {
        let files = vec![
            "/tmp/test/app.ts".to_string(),
            "/tmp/test/types.d.ts".to_string(),
        ];

        let (paths, _) = build_module_resolution_maps(&files);

        assert_eq!(paths.get(&(0, "./types".to_string())), Some(&1));
    }

    #[test]
    fn test_mts_extension() {
        let files = vec![
            "/tmp/test/app.mts".to_string(),
            "/tmp/test/utils.mts".to_string(),
        ];

        let (paths, _) = build_module_resolution_maps(&files);

        assert_eq!(paths.get(&(0, "./utils".to_string())), Some(&1));
    }

    #[test]
    fn test_js_extension() {
        let files = vec![
            "/tmp/test/app.ts".to_string(),
            "/tmp/test/legacy.js".to_string(),
        ];

        let (paths, _) = build_module_resolution_maps(&files);

        assert_eq!(paths.get(&(0, "./legacy".to_string())), Some(&1));
    }

    #[test]
    fn test_cjs_extension() {
        let files = vec![
            "/tmp/test/app.ts".to_string(),
            "/tmp/test/config.cjs".to_string(),
        ];

        let (paths, _) = build_module_resolution_maps(&files);

        assert_eq!(paths.get(&(0, "./config".to_string())), Some(&1));
    }

    #[test]
    fn test_declaration_mts() {
        let files = vec![
            "/tmp/test/app.ts".to_string(),
            "/tmp/test/types.d.mts".to_string(),
        ];

        let (paths, _) = build_module_resolution_maps(&files);

        assert_eq!(paths.get(&(0, "./types".to_string())), Some(&1));
    }

    // =========================================================================
    // build_module_resolution_maps - index file resolution
    // =========================================================================

    #[test]
    fn test_index_file_resolution() {
        let files = vec![
            "/tmp/test/main.ts".to_string(),
            "/tmp/test/lib/index.ts".to_string(),
        ];

        let (paths, modules) = build_module_resolution_maps(&files);

        // ./lib should resolve to lib/index.ts
        assert_eq!(paths.get(&(0, "./lib".to_string())), Some(&1));
        assert!(modules.contains("./lib"));
        // ./lib/index should also work
        assert_eq!(paths.get(&(0, "./lib/index".to_string())), Some(&1));
    }

    #[test]
    fn test_index_tsx_resolution() {
        let files = vec![
            "/tmp/test/main.ts".to_string(),
            "/tmp/test/components/index.tsx".to_string(),
        ];

        let (paths, _) = build_module_resolution_maps(&files);

        assert_eq!(paths.get(&(0, "./components".to_string())), Some(&1));
    }

    #[test]
    fn test_index_dts_resolution() {
        let files = vec![
            "/tmp/test/main.ts".to_string(),
            "/tmp/test/types/index.d.ts".to_string(),
        ];

        let (paths, _) = build_module_resolution_maps(&files);

        assert_eq!(paths.get(&(0, "./types".to_string())), Some(&1));
    }

    #[test]
    fn test_nested_index_resolution() {
        let files = vec![
            "/tmp/test/main.ts".to_string(),
            "/tmp/test/src/lib/index.ts".to_string(),
        ];

        let (paths, _) = build_module_resolution_maps(&files);

        assert_eq!(paths.get(&(0, "./src/lib".to_string())), Some(&1));
    }

    // =========================================================================
    // build_module_resolution_maps - multiple files
    // =========================================================================

    #[test]
    fn test_multiple_targets() {
        let files = vec![
            "/tmp/test/main.ts".to_string(),
            "/tmp/test/utils.ts".to_string(),
            "/tmp/test/types.ts".to_string(),
            "/tmp/test/config.ts".to_string(),
        ];

        let (paths, _) = build_module_resolution_maps(&files);

        assert_eq!(paths.get(&(0, "./utils".to_string())), Some(&1));
        assert_eq!(paths.get(&(0, "./types".to_string())), Some(&2));
        assert_eq!(paths.get(&(0, "./config".to_string())), Some(&3));
    }

    #[test]
    fn test_cross_imports_between_nested() {
        let files = vec![
            "/tmp/test/src/a.ts".to_string(),
            "/tmp/test/src/b.ts".to_string(),
            "/tmp/test/lib/c.ts".to_string(),
        ];

        let (paths, _) = build_module_resolution_maps(&files);

        // a -> b (same directory)
        assert_eq!(paths.get(&(0, "./b".to_string())), Some(&1));
        // a -> c (different directory)
        assert_eq!(paths.get(&(0, "../lib/c".to_string())), Some(&2));
        // c -> a
        assert_eq!(paths.get(&(2, "../src/a".to_string())), Some(&0));
    }

    #[test]
    fn test_self_import_excluded() {
        let files = vec!["/tmp/test/main.ts".to_string()];

        let (paths, _) = build_module_resolution_maps(&files);

        // A file should not resolve to itself
        assert!(paths.is_empty());
    }

    #[test]
    fn test_empty_file_list() {
        let files: Vec<String> = vec![];

        let (paths, modules) = build_module_resolution_maps(&files);

        assert!(paths.is_empty());
        assert!(modules.is_empty());
    }

    // =========================================================================
    // build_module_resolution_maps - real-world project layout
    // =========================================================================

    #[test]
    fn test_typical_project_layout() {
        let files = vec![
            "/project/src/index.ts".to_string(),
            "/project/src/app.ts".to_string(),
            "/project/src/components/Button.tsx".to_string(),
            "/project/src/components/index.ts".to_string(),
            "/project/src/utils/helpers.ts".to_string(),
            "/project/src/types/api.d.ts".to_string(),
        ];

        let (paths, _) = build_module_resolution_maps(&files);

        // index.ts -> app
        assert_eq!(paths.get(&(0, "./app".to_string())), Some(&1));
        // index.ts -> components (via index.ts)
        assert_eq!(paths.get(&(0, "./components".to_string())), Some(&3));
        // index.ts -> components/Button
        assert_eq!(paths.get(&(0, "./components/Button".to_string())), Some(&2));
        // index.ts -> utils/helpers
        assert_eq!(paths.get(&(0, "./utils/helpers".to_string())), Some(&4));
        // index.ts -> types/api
        assert_eq!(paths.get(&(0, "./types/api".to_string())), Some(&5));
        // Button -> ../utils/helpers
        assert_eq!(paths.get(&(2, "../utils/helpers".to_string())), Some(&4));
        // Button -> ../types/api
        assert_eq!(paths.get(&(2, "../types/api".to_string())), Some(&5));
    }
}
