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

// ---------------------------------------------------------------------------
// Resolution-candidate priority lists (tsc parity)
//
// These mirror tsc's `supportedTSExtensions` and `allSupportedExtensions`
// from `src/compiler/utilities.ts` (TypeScript 5.5+). They are the single
// source of truth for extensionless-stem fan-out order across the crates:
//
//   tsz-common      — defines the order (this file)
//   tsz-core        — uses `BARE_*` lists (no leading dot) for filesystem probes
//   tsz-cli         — uses `BARE_*` lists for the CLI driver probes
//   tsz-lsp         — uses `BARE_*` lists for module-specifier inference
//   tsz-checker     — uses `DOTTED_*` lists for filename-index lookups
//
// Structural rule from tsc (`supportedTSExtensions`):
//
//   [[Ts, Tsx, Dts], [Cts, Dcts], [Mts, Dmts]]
//
// Grouped by module flavor: the universal TS group first, then the CJS-tagged
// pair, then the ESM-tagged pair. Source surfaces precede their declaration
// counterpart inside each group. `supportedJSExtensions` follows the same
// shape: `[[Js, Jsx], [Mjs], [Cjs]]` — note that for JS, `mjs` precedes `cjs`,
// the opposite of the TS grouping. `allSupportedExtensions` interleaves them
// by module flavor: `[[Ts, Tsx, Dts, Js, Jsx], [Cts, Dcts, Cjs], [Mts, Dmts, Mjs]]`.
// ---------------------------------------------------------------------------

/// TS-only resolution candidate priority, with leading dot. Mirrors tsc's
/// `supportedTSExtensions` flattened. Used by `tsz-checker` for
/// filename-index probing where each entry already carries the dot.
pub const TSC_TS_RESOLUTION_EXTENSIONS: &[&str] =
    &[".ts", ".tsx", ".d.ts", ".cts", ".d.cts", ".mts", ".d.mts"];

/// TS+JS resolution candidate priority, with leading dot. Mirrors tsc's
/// `allSupportedExtensions` flattened: TS+JS are interleaved by module
/// flavor (`[Ts, Tsx, Dts, Js, Jsx], [Cts, Dcts, Cjs], [Mts, Dmts, Mjs]`).
/// Used by `tsz-checker` for stem fan-out on projects that may contain
/// either TS or JS files.
pub const TSC_TS_JS_RESOLUTION_EXTENSIONS: &[&str] = &[
    ".ts", ".tsx", ".d.ts", ".js", ".jsx", ".cts", ".d.cts", ".cjs", ".mts", ".d.mts", ".mjs",
];

/// TS-only resolution candidate priority, without leading dot. Mirrors
/// `TSC_TS_RESOLUTION_EXTENSIONS`. Used by `tsz-core` / `tsz-cli` / `tsz-lsp`
/// where the probe API takes the bare extension stem (it appends the dot
/// internally via `Path::with_extension`).
pub const TSC_TS_RESOLUTION_EXTENSIONS_BARE: &[&str] =
    &["ts", "tsx", "d.ts", "cts", "d.cts", "mts", "d.mts"];

/// TS+JS resolution candidate priority, without leading dot. Mirrors
/// `TSC_TS_JS_RESOLUTION_EXTENSIONS`.
pub const TSC_TS_JS_RESOLUTION_EXTENSIONS_BARE: &[&str] = &[
    "ts", "tsx", "d.ts", "js", "jsx", "cts", "d.cts", "cjs", "mts", "d.mts", "mjs",
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
        .is_some_and(is_ts_declaration_file_name)
}

/// Return true when `name` is a TypeScript declaration file name.
#[must_use]
pub fn is_ts_declaration_file_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let name = name.rsplit('/').next().unwrap_or(&name);
    let name = name.rsplit('\\').next().unwrap_or(name);

    if TS_DECLARATION_EXTENSIONS
        .iter()
        .any(|ext| name.ends_with(ext))
    {
        return true;
    }

    // Arbitrary extension declaration files: .d.<ext>.ts (for example .d.css.ts).
    name.ends_with(".ts") && name.contains(".d.")
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

/// Return true when `path` is a TypeScript default library file.
///
/// Matches:
/// - Any file whose name starts with `lib.` and ends with `.d.ts`
///   (e.g. `lib.es5.d.ts`, `lib.esnext.full.d.ts`).
/// - Files inside an `@typescript/lib-*` `node_modules` package
///   (the split-per-lib distribution used by bundlers).
///
/// Case-sensitive. Use [`is_default_lib_file_name`] when you only have
/// a bare file name rather than a full path.
#[must_use]
pub fn is_default_lib_file(path: &Path) -> bool {
    is_default_lib_file_name(
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default(),
    ) || path
        .to_string_lossy()
        .contains("/node_modules/@typescript/lib-")
}

/// Return true when `name` is a TypeScript default library file name.
///
/// Matches names that start with `"lib."` and end with `".d.ts"`.
/// Does **not** check for `@typescript/lib-*` paths; use
/// [`is_default_lib_file`] for full-path checks.
#[must_use]
pub fn is_default_lib_file_name(name: &str) -> bool {
    name.starts_with("lib.") && name.ends_with(".d.ts")
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
    _resolve_json_module: bool,
) -> Vec<String> {
    let mut patterns = glob_patterns_for_extensions(TS_SOURCE_EXTENSIONS);
    if allow_js {
        patterns.extend(glob_patterns_for_extensions(JS_FAMILY_EXTENSIONS));
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
        assert!(is_ts_declaration_file(Path::new("style.d.css.ts")));
        assert!(is_ts_declaration_file(Path::new("INDEX.D.MTS")));
        assert!(!is_ts_declaration_file(Path::new("style.css.ts")));
        assert!(!is_ts_declaration_file_name("foo.d/bar.ts"));
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
        assert!(!default_discovery_include_patterns(true, true).contains(&"**/*.json".to_string()));
        assert!(include_pattern_has_supported_extension("src/index.mjs"));
        assert!(!include_pattern_has_supported_extension("src/*.json"));
        assert!(!include_pattern_has_supported_extension("src"));
    }

    #[test]
    fn resolution_priority_lists_match_tsc_supported_extensions() {
        // `supportedTSExtensions = [[Ts, Tsx, Dts], [Cts, Dcts], [Mts, Dmts]]`
        // — universal TS group, then CJS-tagged group, then ESM-tagged group.
        assert_eq!(
            TSC_TS_RESOLUTION_EXTENSIONS,
            &[".ts", ".tsx", ".d.ts", ".cts", ".d.cts", ".mts", ".d.mts"],
        );
        // `allSupportedExtensions = [[Ts, Tsx, Dts, Js, Jsx], [Cts, Dcts, Cjs], [Mts, Dmts, Mjs]]`
        // — JS surfaces sit in the universal first group; `.cjs` ships with
        // the CJS-tagged group; `.mjs` ships with the ESM-tagged group.
        assert_eq!(
            TSC_TS_JS_RESOLUTION_EXTENSIONS,
            &[
                ".ts", ".tsx", ".d.ts", ".js", ".jsx", ".cts", ".d.cts", ".cjs", ".mts", ".d.mts",
                ".mjs",
            ],
        );
    }

    #[test]
    fn bare_resolution_lists_mirror_dotted_lists_without_leading_dot() {
        // The bare lists are the same priority order, with the leading dot
        // stripped. `tsz-core` / `tsz-cli` / `tsz-lsp` append the dot via
        // `Path::with_extension`, so they consume the bare form.
        for (dotted, bare) in [
            (
                TSC_TS_RESOLUTION_EXTENSIONS,
                TSC_TS_RESOLUTION_EXTENSIONS_BARE,
            ),
            (
                TSC_TS_JS_RESOLUTION_EXTENSIONS,
                TSC_TS_JS_RESOLUTION_EXTENSIONS_BARE,
            ),
        ] {
            assert_eq!(dotted.len(), bare.len());
            for (d, b) in dotted.iter().zip(bare) {
                assert_eq!(d.strip_prefix('.'), Some(*b), "{d} → {b}");
            }
        }
    }

    #[test]
    fn is_default_lib_file_name_matches_lib_prefix_dts_suffix() {
        assert!(is_default_lib_file_name("lib.d.ts"));
        assert!(is_default_lib_file_name("lib.es5.d.ts"));
        assert!(is_default_lib_file_name("lib.esnext.full.d.ts"));
        assert!(is_default_lib_file_name("lib.dom.d.ts"));
        assert!(is_default_lib_file_name("lib.decorators.d.ts"));
    }

    #[test]
    fn is_default_lib_file_name_rejects_non_lib_files() {
        assert!(!is_default_lib_file_name("types.d.ts"));
        assert!(!is_default_lib_file_name("index.ts"));
        assert!(!is_default_lib_file_name("lib.custom.ts")); // not .d.ts
        assert!(!is_default_lib_file_name("mylib.d.ts")); // no "lib." prefix
    }

    #[test]
    fn is_default_lib_file_matches_at_typescript_lib_package() {
        use std::path::Path;
        // Absolute paths (as they appear in real project compilation)
        assert!(is_default_lib_file(Path::new(
            "/project/node_modules/@typescript/lib-es5/index.d.ts"
        )));
        assert!(is_default_lib_file(Path::new("lib.es5.d.ts")));
        assert!(!is_default_lib_file(Path::new(
            "/project/node_modules/some-pkg/index.d.ts"
        )));
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
