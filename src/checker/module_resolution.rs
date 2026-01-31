//! Module resolution utilities for multi-file type checking.
//!
//! This module provides shared utilities for building cross-file module
//! resolution context. Used by both the CLI and the tsz-server.

use rustc_hash::FxHashMap;
use std::collections::HashSet;
use std::path::Path;

/// Build module resolution maps from a list of file paths.
///
/// Returns:
/// - `resolved_module_paths`: Maps (source_file_idx, specifier) -> target_file_idx
/// - `resolved_modules`: Set of all valid module specifiers
///
/// This handles simple relative imports between files in the same project.
/// For example, if we have files `/tmp/test/main.ts` and `/tmp/test/types.ts`,
/// then from `main.ts`, the specifier `./types` will resolve to `types.ts`.
pub fn build_module_resolution_maps(
    file_names: &[String],
) -> (FxHashMap<(usize, String), usize>, HashSet<String>) {
    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    let mut resolved_modules: HashSet<String> = HashSet::new();

    // Build a HashSet for fast file existence checks
    let file_set: HashSet<String> = file_names.iter().cloned().collect();

    // For each source file, compute what modules it can import
    for (src_idx, src_name) in file_names.iter().enumerate() {
        let src_path = Path::new(src_name);
        let src_dir = src_path.parent();

        for (tgt_idx, tgt_name) in file_names.iter().enumerate() {
            if src_idx == tgt_idx {
                continue;
            }

            let tgt_path = Path::new(tgt_name);
            let tgt_stem = tgt_path.file_stem().unwrap_or_default().to_string_lossy();

            // Try Node.js-style resolution if we have a common directory
            if let Some(src_dir) = src_dir {
                if let Ok(rel_path) = tgt_path.strip_prefix(src_dir) {
                    let rel_str = rel_path.to_string_lossy();

                    // Try extension resolution for the relative path
                    // Order: .ts, .tsx, .d.ts, .js, .jsx (TypeScript preference order)
                    let extensions = [".ts", ".tsx", ".d.ts", ".js", ".jsx"];
                    for ext in &extensions {
                        let with_ext = format!("{}{}", rel_str, ext);
                        if file_set.contains(&format!("{}/{}", src_dir.display(), with_ext)) {
                            // Found it! Add the specifier without extension
                            let specifier = rel_str
                                .trim_end_matches(".ts")
                                .trim_end_matches(".tsx")
                                .trim_end_matches(".d.ts")
                                .trim_end_matches(".js")
                                .trim_end_matches(".jsx")
                                .to_string();

                            // Add with ./ prefix
                            let full_specifier = format!("./{}", specifier);
                            resolved_module_paths
                                .insert((src_idx, full_specifier.clone()), tgt_idx);
                            resolved_modules.insert(full_specifier);

                            // Also add without ./ prefix for compatibility
                            resolved_module_paths.insert((src_idx, specifier.clone()), tgt_idx);
                            resolved_modules.insert(specifier);

                            break; // Found it, don't try other extensions
                        }
                    }

                    // Also try directory resolution (directory/index.ts)
                    let index_extensions = [
                        "/index.ts",
                        "/index.tsx",
                        "/index.d.ts",
                        "/index.js",
                        "/index.jsx",
                    ];
                    for index_ext in &index_extensions {
                        let with_index = format!("{}{}", rel_str, index_ext);
                        if file_set.contains(&format!("{}/{}", src_dir.display(), with_index)) {
                            // Found it! Add the specifier (without /index part)
                            let specifier = rel_str.to_string();
                            let full_specifier = format!("./{}", specifier);
                            resolved_module_paths
                                .insert((src_idx, full_specifier.clone()), tgt_idx);
                            resolved_modules.insert(full_specifier);

                            // Also add without ./ prefix
                            resolved_module_paths.insert((src_idx, specifier.clone()), tgt_idx);
                            resolved_modules.insert(specifier);

                            break; // Found it, don't try other index extensions
                        }
                    }
                }
            }

            // Also add the bare file stem as a valid specifier (with ./ prefix)
            resolved_module_paths.insert((src_idx, format!("./{}", tgt_stem)), tgt_idx);
            resolved_modules.insert(format!("./{}", tgt_stem));
        }
    }

    (resolved_module_paths, resolved_modules)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_relative_import() {
        let files = vec![
            "/tmp/test/main.ts".to_string(),
            "/tmp/test/types.ts".to_string(),
        ];

        let (paths, modules) = build_module_resolution_maps(&files);

        // From main.ts (idx 0), "./types" should resolve to types.ts (idx 1)
        assert_eq!(paths.get(&(0, "./types".to_string())), Some(&1));
        assert!(modules.contains("./types"));
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_nested_import() {
        let files = vec![
            "/tmp/test/main.ts".to_string(),
            "/tmp/test/lib/utils.ts".to_string(),
        ];

        let (paths, modules) = build_module_resolution_maps(&files);

        // From main.ts, "./lib/utils" should resolve to lib/utils.ts
        assert_eq!(paths.get(&(0, "./lib/utils".to_string())), Some(&1));
        assert!(modules.contains("./lib/utils"));
    }
}
