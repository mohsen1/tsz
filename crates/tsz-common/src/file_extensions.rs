//! Centralized file-extension constants and helpers.
//!
//! Many crates need to recognize, strip, or compare TypeScript/JavaScript
//! file extensions. This module is the single source of truth for those
//! lists so that adding a new family member (or changing a stripping policy)
//! is a one-line change.
//!
//! Two extension families are tracked:
//!
//! - **TS family**: `.ts`, `.tsx`, `.mts`, `.cts`, `.d.ts`, `.d.mts`, `.d.cts`
//!   (and `.d.tsx` for completeness, though it's rare).
//! - **JS family**: `.js`, `.jsx`, `.mjs`, `.cjs`.
//!
//! tsc-display behaviour:
//! - `typeof import("X.ts")` → `typeof import("X")` (strip TS family).
//! - `typeof import("X.js")` → `typeof import("X.js")` (preserve JS family).
//!
//! All arrays list **longest extensions first** so that a `strip_suffix`
//! loop matches `.d.ts` before `.ts`.

/// TypeScript declaration extensions. Always stripped from display.
pub const TS_DECLARATION_EXTENSIONS: &[&str] = &[".d.ts", ".d.tsx", ".d.mts", ".d.cts"];

/// TypeScript source extensions. Always stripped from display.
pub const TS_SOURCE_EXTENSIONS: &[&str] = &[".ts", ".tsx", ".mts", ".cts"];

/// All TS-family extensions (declaration + source). Longest first so a
/// `strip_suffix` loop matches `.d.ts` before `.ts`.
pub const TS_FAMILY_EXTENSIONS: &[&str] = &[
    ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts",
];

/// JS-family extensions. tsc preserves these in `typeof import("X.js")`
/// display when the imported module is itself a JS file.
pub const JS_FAMILY_EXTENSIONS: &[&str] = &[".js", ".jsx", ".mjs", ".cjs"];

/// All TS+JS-family extensions plus `.json`. Used by module resolution to
/// recognize any file extension that the resolver can produce.
pub const KNOWN_MODULE_EXTENSIONS: &[&str] = &[
    ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs",
    ".cjs", ".json",
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

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn strip_known_extension_drops_both_families() {
        assert_eq!(strip_known_extension("foo.ts"), "foo");
        assert_eq!(strip_known_extension("foo.js"), "foo");
        assert_eq!(strip_known_extension("foo.d.ts"), "foo");
        assert_eq!(strip_known_extension("foo"), "foo");
        assert_eq!(strip_known_extension("foo.json"), "foo.json");
    }
}
