//! Centralized file-extension constants and helpers.
//!
//! Many crates need to recognize, strip, or compare TypeScript/JavaScript
//! file extensions. This module is the single source of truth for those
//! lists so that adding a new family member (or changing a stripping policy)
//! is a one-line change.
//!
//! Two extension families are tracked:
//!
//! - **TS family**: `.ts`, `.tsx`, `.mts`, `.cts`, `.d.ts`, `.d.mts`, `.d.cts`.
//!   `.d.tsx` is treated as a `.tsx` source path, matching TypeScript.
//! - **JS family**: `.js`, `.jsx`, `.mjs`, `.cjs`.
//!
//! tsc-display behaviour:
//! - `typeof import("X.ts")` → `typeof import("X")` (strip TS family).
//! - `typeof import("X.js")` → `typeof import("X.js")` (preserve JS family).
//!
//! All arrays list **longest extensions first** so that a `strip_suffix`
//! loop matches `.d.ts` before `.ts`.

use std::path::{Path, PathBuf};

/// TypeScript declaration extensions. Always stripped from display.
pub const TS_DECLARATION_EXTENSIONS: &[&str] = &[".d.ts", ".d.mts", ".d.cts"];

/// TypeScript source extensions. Always stripped from display.
pub const TS_SOURCE_EXTENSIONS: &[&str] = &[".ts", ".tsx", ".mts", ".cts"];

/// All TS-family extensions (declaration + source). Longest first so a
/// `strip_suffix` loop matches `.d.ts` before `.ts`.
pub const TS_FAMILY_EXTENSIONS: &[&str] =
    &[".d.ts", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts"];

/// JS-family extensions. tsc preserves these in `typeof import("X.js")`
/// display when the imported module is itself a JS file.
pub const JS_FAMILY_EXTENSIONS: &[&str] = &[".js", ".jsx", ".mjs", ".cjs"];

/// JSON extension. Kept separate because JSON is a module-resolution/discovery
/// input only when the caller enables the relevant compiler option.
pub const JSON_EXTENSION: &str = ".json";

/// All TS+JS-family extensions plus `.json`. Used by module resolution to
/// recognize any file extension that the resolver can produce.
pub const KNOWN_MODULE_EXTENSIONS: &[&str] = &[
    ".d.ts", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs",
    ".json",
];

/// Strip a TS-family extension from a module-specifier display string.
/// Matches tsc's `typeof import("X")` behaviour: TS extensions are dropped,
/// JS extensions (and unknown suffixes) are preserved.
///
/// Returns the input unchanged if no TS-family extension matches.
#[must_use]
pub fn strip_ts_extension(specifier: &str) -> &str {
    for ext in TS_FAMILY_EXTENSIONS {
        if let Some(stripped) = specifier.strip_suffix(ext) {
            return stripped;
        }
    }
    specifier
}

/// Strip any known TS or JS extension. Use this in resolution paths where
/// we want a normalized "module identity" without extension. For display
/// strings, prefer [`strip_ts_extension`].
#[must_use]
pub fn strip_known_extension(path: &str) -> &str {
    for ext in TS_FAMILY_EXTENSIONS.iter().chain(JS_FAMILY_EXTENSIONS) {
        if let Some(stripped) = path.strip_suffix(ext) {
            return stripped;
        }
    }
    path
}

/// Return true when `path` has a TypeScript declaration extension.
#[must_use]
pub fn is_ts_declaration_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            TS_DECLARATION_EXTENSIONS
                .iter()
                .any(|ext| name.ends_with(ext))
        })
}

/// Return true when `path` has a TypeScript source extension, excluding
/// declaration files that share the final `.ts`/`.mts`/`.cts` suffix.
#[must_use]
pub fn is_ts_source_file(path: &Path) -> bool {
    if is_ts_declaration_file(path) {
        return false;
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| TS_SOURCE_EXTENSIONS.iter().any(|ext| name.ends_with(ext)))
}

/// Return true when `path` is in the TypeScript family, including declarations.
#[must_use]
pub fn is_ts_file(path: &Path) -> bool {
    is_ts_declaration_file(path) || is_ts_source_file(path)
}

/// Return true when `path` is in the JavaScript family.
#[must_use]
pub fn is_js_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            JS_FAMILY_EXTENSIONS
                .iter()
                .any(|candidate| ext == candidate.trim_start_matches('.'))
        })
}

/// Return true when `path` is a JSON file.
#[must_use]
pub fn is_json_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == JSON_EXTENSION.trim_start_matches('.'))
}

/// Check if a path is a valid module file for module resolution purposes.
/// This includes TypeScript files and JSON files, but intentionally excludes
/// JavaScript files for export-map resolution paths.
#[must_use]
pub fn is_valid_module_file(path: &Path) -> bool {
    is_ts_file(path) || is_json_file(path)
}

/// Like [`is_valid_module_file`], but also accepts JavaScript files for
/// non-export resolution paths such as package `imports`, `main`, or direct
/// file resolution.
#[must_use]
pub fn is_valid_module_or_js_file(path: &Path) -> bool {
    is_ts_file(path) || is_js_file(path) || is_json_file(path)
}

/// Build tsc-compatible default include globs for source discovery. tsc
/// displays this as `["**/*"]`, but discovery filters through these concrete
/// extension families.
#[must_use]
pub fn default_discovery_include_patterns(
    allow_js: bool,
    resolve_json_module: bool,
) -> Vec<String> {
    let mut patterns = glob_patterns_for_extensions(TS_SOURCE_EXTENSIONS);
    if allow_js {
        patterns.extend(glob_patterns_for_extensions(JS_FAMILY_EXTENSIONS));
    }
    if resolve_json_module {
        patterns.extend(glob_patterns_for_extensions(&[JSON_EXTENSION]));
    }
    patterns
}

/// Return true when an include pattern already targets a supported source
/// discovery extension. Directory patterns should be expanded by the caller.
#[must_use]
pub fn include_pattern_has_supported_extension(pattern: &str) -> bool {
    TS_SOURCE_EXTENSIONS
        .iter()
        .chain(JS_FAMILY_EXTENSIONS)
        .chain([JSON_EXTENSION].iter())
        .any(|ext| pattern.ends_with(ext))
}

/// Strip a TypeScript source extension from a path and return the parent-joined
/// stem. Returns `None` for declaration files and non-source extensions.
#[must_use]
pub fn strip_ts_source_extension_from_path(path: &Path) -> Option<PathBuf> {
    if is_ts_declaration_file(path) {
        return None;
    }
    strip_path_extension(path, TS_SOURCE_EXTENSIONS)
}

/// Strip a TypeScript declaration extension from a path and return the
/// parent-joined stem.
#[must_use]
pub fn strip_ts_declaration_extension_from_path(path: &Path) -> Option<PathBuf> {
    strip_path_extension(path, TS_DECLARATION_EXTENSIONS)
}

fn glob_patterns_for_extensions(extensions: &[&str]) -> Vec<String> {
    let mut patterns = Vec::with_capacity(extensions.len() * 2);
    for ext in extensions {
        patterns.push(format!("*{ext}"));
    }
    for ext in extensions {
        patterns.push(format!("**/*{ext}"));
    }
    patterns
}

fn strip_path_extension(path: &Path, extensions: &[&str]) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    for ext in extensions {
        if let Some(stem) = name.strip_suffix(ext) {
            return Some(path.with_file_name(stem));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn strip_ts_extension_drops_ts_family_only() {
        assert_eq!(strip_ts_extension("foo.ts"), "foo");
        assert_eq!(strip_ts_extension("foo.tsx"), "foo");
        assert_eq!(strip_ts_extension("foo.d.ts"), "foo");
        assert_eq!(strip_ts_extension("foo.d.mts"), "foo");
        assert_eq!(strip_ts_extension("foo.cts"), "foo");
        // JS family preserved (regression: lateBoundAssignmentDeclarationSupport2.js)
        assert_eq!(strip_ts_extension("foo.js"), "foo.js");
        assert_eq!(strip_ts_extension("foo.jsx"), "foo.jsx");
        assert_eq!(strip_ts_extension("foo.mjs"), "foo.mjs");
        assert_eq!(strip_ts_extension("foo.cjs"), "foo.cjs");
        // Unknown / no-extension preserved
        assert_eq!(strip_ts_extension("foo"), "foo");
        assert_eq!(strip_ts_extension("foo.json"), "foo.json");
    }

    #[test]
    fn strip_ts_extension_prefers_d_ts_over_ts() {
        assert_eq!(strip_ts_extension("foo.d.ts"), "foo");
        assert_eq!(strip_ts_extension("foo.d.mts"), "foo");
        assert_eq!(strip_ts_extension("foo.d.cts"), "foo");
        assert_eq!(strip_ts_extension("foo.d.tsx"), "foo.d");
    }

    #[test]
    fn strip_known_extension_drops_both_families() {
        assert_eq!(strip_known_extension("foo.ts"), "foo");
        assert_eq!(strip_known_extension("foo.js"), "foo");
        assert_eq!(strip_known_extension("foo.d.ts"), "foo");
        assert_eq!(strip_known_extension("foo"), "foo");
        assert_eq!(strip_known_extension("foo.json"), "foo.json");
    }

    #[test]
    fn path_predicates_classify_extension_families() {
        assert!(is_ts_file(Path::new("index.ts")));
        assert!(is_ts_file(Path::new("index.d.ts")));
        assert!(is_ts_file(Path::new("index.d.mts")));
        assert!(is_ts_source_file(Path::new("index.mts")));
        assert!(is_ts_source_file(Path::new("index.d.tsx")));
        assert!(!is_ts_source_file(Path::new("index.d.mts")));
        assert!(is_ts_declaration_file(Path::new("index.d.cts")));
        assert!(!is_ts_declaration_file(Path::new("index.d.tsx")));
        assert!(is_js_file(Path::new("index.cjs")));
        assert!(is_json_file(Path::new("package.json")));
        assert!(!is_valid_module_file(Path::new("index.js")));
        assert!(is_valid_module_or_js_file(Path::new("index.js")));
    }

    #[test]
    fn discovery_include_patterns_follow_extension_families() {
        assert_eq!(
            default_discovery_include_patterns(false, false),
            vec![
                "*.ts", "*.tsx", "*.mts", "*.cts", "**/*.ts", "**/*.tsx", "**/*.mts", "**/*.cts"
            ]
        );
        assert!(default_discovery_include_patterns(true, true).contains(&"**/*.json".to_string()));
        assert!(include_pattern_has_supported_extension("src/index.mjs"));
        assert!(include_pattern_has_supported_extension("src/*.json"));
        assert!(!include_pattern_has_supported_extension("src"));
    }

    #[test]
    fn path_extension_stripping_preserves_source_vs_declaration_boundary() {
        assert_eq!(
            strip_ts_source_extension_from_path(Path::new("src/index.ts")),
            Some(PathBuf::from("src/index"))
        );
        assert_eq!(
            strip_ts_source_extension_from_path(Path::new("src/index.d.ts")),
            None
        );
        assert_eq!(
            strip_ts_declaration_extension_from_path(Path::new("src/index.d.mts")),
            Some(PathBuf::from("src/index"))
        );
        assert_eq!(
            strip_ts_source_extension_from_path(Path::new("src/index.d.tsx")),
            Some(PathBuf::from("src/index.d"))
        );
        assert_eq!(
            strip_ts_declaration_extension_from_path(Path::new("src/index.d.tsx")),
            None
        );
    }
}
