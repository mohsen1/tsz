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
