//! Import path completions for LSP.
//!
//! Provides completions for module specifiers inside import/export statements:
//! - `import { foo } from "./|"` → suggests files/directories
//! - `export { foo } from "./|"` → suggests files/directories
//! - `import "./|"` → suggests files/directories
//!
//! This is separate from string literal completions which handle
//! string arguments in function calls.

use super::{CompletionItem, CompletionItemKind, sort_priority};

/// Import path completion entry from the project's file list.
#[derive(Debug, Clone)]
pub struct ImportPathEntry {
    /// The relative module specifier (e.g., "./utils", "../lib/helpers").
    pub specifier: String,
    /// Whether this is a directory (suggests further nesting).
    pub is_directory: bool,
}

/// Generate import path completions given a partial specifier and the
/// list of known project files.
///
/// # Arguments
/// * `partial` - The partial module specifier typed so far (e.g., "./ut")
/// * `available_paths` - All available import paths from the project
///
/// # Returns
/// A list of completion items for matching paths.
pub fn get_import_path_completions(
    partial: &str,
    available_paths: &[ImportPathEntry],
) -> Vec<CompletionItem> {
    if partial.is_empty() {
        return Vec::new();
    }

    available_paths
        .iter()
        .filter(|entry| entry.specifier.starts_with(partial))
        .map(|entry| {
            let kind = CompletionItemKind::Module;

            CompletionItem::new(entry.specifier.clone(), kind)
                .with_detail(if entry.is_directory {
                    "directory".to_string()
                } else {
                    "module".to_string()
                })
                .with_sort_text(sort_priority::LOCATION_PRIORITY)
                .with_insert_text(entry.specifier.clone())
        })
        .collect()
}

/// Build import path entries from a list of project file paths relative
/// to a given source file.
///
/// # Arguments
/// * `current_file` - The file requesting completions
/// * `project_files` - All files in the project (absolute or relative paths)
///
/// # Returns
/// A list of `ImportPathEntry` values with relative specifiers.
pub fn build_import_paths(current_file: &str, project_files: &[String]) -> Vec<ImportPathEntry> {
    let current_dir = match current_file.rfind('/') {
        Some(pos) => &current_file[..pos],
        None => ".",
    };

    let mut entries = Vec::new();
    let mut seen_dirs = std::collections::HashSet::new();

    for file in project_files {
        if file == current_file {
            continue;
        }

        // Skip non-TS/JS files
        if !is_importable_file(file) {
            continue;
        }

        // Compute relative path
        let relative = compute_relative_path(current_dir, file);

        // Strip extension for module specifier
        let specifier = strip_ts_extension(&relative);

        entries.push(ImportPathEntry {
            specifier,
            is_directory: false,
        });

        // Also suggest parent directories
        if let Some(dir_end) = relative.rfind('/') {
            let dir = &relative[..dir_end];
            if seen_dirs.insert(dir.to_string()) {
                entries.push(ImportPathEntry {
                    specifier: dir.to_string(),
                    is_directory: true,
                });
            }
        }
    }

    entries
}

/// Check if a file is importable (TypeScript/JavaScript).
fn is_importable_file(path: &str) -> bool {
    path.ends_with(".ts")
        || path.ends_with(".tsx")
        || path.ends_with(".js")
        || path.ends_with(".jsx")
        || path.ends_with(".mts")
        || path.ends_with(".mjs")
        || path.ends_with(".cts")
        || path.ends_with(".cjs")
        || path.ends_with(".json")
}

/// Strip TypeScript/JavaScript extensions from a path for module specifiers.
fn strip_ts_extension(path: &str) -> String {
    for ext in &[".ts", ".tsx", ".js", ".jsx", ".mts", ".mjs", ".cts", ".cjs"] {
        if let Some(stripped) = path.strip_suffix(ext) {
            // Don't strip .d.ts → just .ts portion
            if let Some(base) = stripped.strip_suffix(".d") {
                return base.to_string();
            }
            // Don't strip index files to just directory path
            return stripped.to_string();
        }
    }
    path.to_string()
}

/// Compute a relative path from `from_dir` to `to_file`.
fn compute_relative_path(from_dir: &str, to_file: &str) -> String {
    let from_parts: Vec<&str> = from_dir.split('/').filter(|s| !s.is_empty()).collect();
    let to_parts: Vec<&str> = to_file.split('/').filter(|s| !s.is_empty()).collect();

    // Find common prefix
    let common_len = from_parts
        .iter()
        .zip(to_parts.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let ups = from_parts.len() - common_len;
    let mut result = String::new();

    if ups == 0 {
        result.push_str("./");
    } else {
        for _ in 0..ups {
            result.push_str("../");
        }
    }

    let remaining: Vec<&str> = to_parts[common_len..].to_vec();
    result.push_str(&remaining.join("/"));

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- get_import_path_completions -------------------------------------

    fn entry(specifier: &str, is_directory: bool) -> ImportPathEntry {
        ImportPathEntry {
            specifier: specifier.to_string(),
            is_directory,
        }
    }

    #[test]
    fn get_import_path_completions_returns_empty_for_empty_partial() {
        let entries = vec![entry("./utils", false), entry("./types", false)];
        assert!(get_import_path_completions("", &entries).is_empty());
    }

    #[test]
    fn get_import_path_completions_filters_by_starts_with() {
        let entries = vec![
            entry("./utils", false),
            entry("./util-helpers", false),
            entry("./types", false),
        ];
        let completions = get_import_path_completions("./util", &entries);
        assert_eq!(completions.len(), 2);
        // Both `./utils` and `./util-helpers` match the prefix.
    }

    #[test]
    fn get_import_path_completions_returns_empty_when_no_match() {
        let entries = vec![entry("./utils", false)];
        let completions = get_import_path_completions("./other", &entries);
        assert!(completions.is_empty());
    }

    // ---- build_import_paths ----------------------------------------------

    #[test]
    fn build_import_paths_skips_self_reference() {
        let current = "src/main.ts";
        let files = vec!["src/main.ts".to_string(), "src/utils.ts".to_string()];
        let entries = build_import_paths(current, &files);
        assert!(entries.iter().all(|e| !e.specifier.contains("main")));
    }

    #[test]
    fn build_import_paths_strips_ts_extension() {
        let current = "src/a.ts";
        let files = vec!["src/b.ts".to_string()];
        let entries = build_import_paths(current, &files);
        // Specifier should not contain `.ts`.
        let specifier_entry = entries.iter().find(|e| !e.is_directory).unwrap();
        assert!(!specifier_entry.specifier.ends_with(".ts"));
        assert!(specifier_entry.specifier.ends_with("/b"));
    }

    #[test]
    fn build_import_paths_filters_non_importable_extensions() {
        let current = "src/a.ts";
        let files = vec![
            "src/b.ts".to_string(),
            "src/img.png".to_string(),
            "src/data.csv".to_string(),
            "src/types.d.ts".to_string(),
        ];
        let entries = build_import_paths(current, &files);
        let specifiers: Vec<&str> = entries.iter().map(|e| e.specifier.as_str()).collect();
        assert!(!specifiers.iter().any(|s| s.contains("img")));
        assert!(!specifiers.iter().any(|s| s.contains("csv")));
    }

    #[test]
    fn build_import_paths_includes_parent_directories() {
        let current = "src/a.ts";
        let files = vec!["src/lib/b.ts".to_string(), "src/lib/c.ts".to_string()];
        let entries = build_import_paths(current, &files);
        // Should include both files AND the parent dir entry "lib"
        let dir_entries: Vec<_> = entries.iter().filter(|e| e.is_directory).collect();
        assert!(!dir_entries.is_empty());
        assert!(dir_entries.iter().any(|e| e.specifier.contains("lib")));
    }

    #[test]
    fn build_import_paths_no_parent_dir_added_twice() {
        // Two files in the same directory — only one directory entry.
        let current = "src/a.ts";
        let files = vec!["src/sub/x.ts".to_string(), "src/sub/y.ts".to_string()];
        let entries = build_import_paths(current, &files);
        let sub_dir_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.is_directory && e.specifier.contains("sub"))
            .collect();
        assert_eq!(sub_dir_entries.len(), 1, "duplicate parent dir entries");
    }

    // ---- is_importable_file ----------------------------------------------

    #[test]
    fn is_importable_file_accepts_ts_jsx_and_json() {
        for ext in &[
            "a.ts", "a.tsx", "a.js", "a.jsx", "a.mts", "a.mjs", "a.cts", "a.cjs", "a.json",
        ] {
            assert!(is_importable_file(ext), "expected importable: {ext}");
        }
    }

    #[test]
    fn is_importable_file_rejects_non_source_extensions() {
        for ext in &["a.png", "a.css", "a.md", "a.html", "a"] {
            assert!(!is_importable_file(ext), "expected non-importable: {ext}");
        }
    }

    // ---- strip_ts_extension ----------------------------------------------

    #[test]
    fn strip_ts_extension_removes_ts_jsx_variants() {
        assert_eq!(strip_ts_extension("a.ts"), "a");
        assert_eq!(strip_ts_extension("a.tsx"), "a");
        assert_eq!(strip_ts_extension("a.mjs"), "a");
        assert_eq!(strip_ts_extension("a.cts"), "a");
    }

    #[test]
    fn strip_ts_extension_strips_d_dot_ts_to_base_name() {
        // `.d.ts` strips both the `.ts` and the trailing `.d`.
        assert_eq!(strip_ts_extension("types.d.ts"), "types");
    }

    #[test]
    fn strip_ts_extension_leaves_unknown_extensions_untouched() {
        assert_eq!(strip_ts_extension("readme.md"), "readme.md");
        assert_eq!(strip_ts_extension("file"), "file");
    }

    // ---- compute_relative_path -------------------------------------------

    #[test]
    fn compute_relative_path_same_directory() {
        // src/ → src/b.ts means b is a sibling: `./b.ts`.
        assert_eq!(compute_relative_path("src", "src/b.ts"), "./b.ts");
    }

    #[test]
    fn compute_relative_path_subdirectory() {
        // src/ → src/lib/b.ts: `./lib/b.ts`.
        assert_eq!(compute_relative_path("src", "src/lib/b.ts"), "./lib/b.ts");
    }

    #[test]
    fn compute_relative_path_parent_directory() {
        // src/sub → src/b.ts: `../b.ts`.
        assert_eq!(compute_relative_path("src/sub", "src/b.ts"), "../b.ts");
    }

    #[test]
    fn compute_relative_path_disjoint_branches() {
        // src/a → other/b.ts: `../../other/b.ts`.
        assert_eq!(
            compute_relative_path("src/a", "other/b.ts"),
            "../../other/b.ts"
        );
    }

    #[test]
    fn compute_relative_path_handles_leading_slashes() {
        // Leading slashes treated as empty parts and skipped.
        assert_eq!(compute_relative_path("/src", "/src/a.ts"), "./a.ts");
    }
}
