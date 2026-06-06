//! Module resolution helper functions and types.
//!
//! Pure helper functions for package.json parsing, path manipulation,
//! semver comparison, and pattern matching used by the module resolver.

use indexmap::IndexMap;
use rustc_hash::FxHashMap;
use std::borrow::Cow;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

// Per-thread path-existence caches for module-resolution hot loops.
//
// `try_file_with_suffixes_and_extension`, `try_directory`, and the
// node_modules walk probe the same candidate files and directories many
// times per build: every import in every source file re-walks overlapping
// ancestor directories and re-tests the same extension candidates. Within a
// single `tsz` invocation the filesystem state is stable, so caching the
// `path.is_file()` / `path.is_dir()` results collapses those repeated
// `stat()` syscalls to one per distinct path. This mirrors tsc's
// `ModuleResolutionHost`, which memoizes both `fileExists` and
// `directoryExists` for exactly the same reason.
//
// The caches are thread-local so rayon workers each have their own (no
// locking on the hot path); unbounded growth is acceptable for a one-shot
// CLI compile and tests reset between invocations because each test runs in
// a fresh thread / a fresh process.
thread_local! {
    static FILE_EXISTS: RefCell<FxHashMap<PathBuf, bool>> =
        RefCell::new(FxHashMap::default());
    static DIR_EXISTS: RefCell<FxHashMap<PathBuf, bool>> =
        RefCell::new(FxHashMap::default());
}

#[inline]
pub(crate) fn cached_is_file(path: &Path) -> bool {
    FILE_EXISTS.with(|cache| {
        if let Some(&exists) = cache.borrow().get(path) {
            return exists;
        }
        let exists = path.is_file();
        cache.borrow_mut().insert(path.to_path_buf(), exists);
        exists
    })
}

/// Cached counterpart to `Path::is_dir`, sharing the staleness contract of
/// [`cached_is_file`]: the filesystem is assumed stable for the lifetime of a
/// compilation, and the result is reset together with the file cache via
/// [`clear_path_existence_caches`].
#[inline]
pub(crate) fn cached_is_dir(path: &Path) -> bool {
    DIR_EXISTS.with(|cache| {
        if let Some(&exists) = cache.borrow().get(path) {
            return exists;
        }
        let exists = path.is_dir();
        cache.borrow_mut().insert(path.to_path_buf(), exists);
        exists
    })
}

/// Reset the caller thread's path-existence caches (files and directories).
///
/// Long-lived hosts should use `ModuleResolver::clear_cache` between
/// compilation cycles. That public reset path calls this helper for the
/// current thread; rayon worker threads keep their own cache entries.
pub(crate) fn clear_path_existence_caches() {
    FILE_EXISTS.with(|cache| cache.borrow_mut().clear());
    DIR_EXISTS.with(|cache| cache.borrow_mut().clear());
}

/// Collapse `.` and `..` segments in a path without touching the filesystem.
///
/// Path identity in the resolver and downstream file graph is textual: two
/// `PathBuf`s with different segment shapes are treated as distinct files even
/// when they refer to the same physical location. tsconfig `paths` targets,
/// `baseUrl`-joined specifiers, package.json `main`/`exports` targets, and
/// container-relative specifiers can all introduce stray `./` or `../` segments
/// that survive `Path::join` (which preserves the literal components).
///
/// The collapse rules (clamp `..` at the filesystem root / drive prefix,
/// preserve leading `..` on relative paths) are owned by
/// [`tsz_common::module_resolution::path_identity::normalize_segments`] so the
/// resolver's textual identity and the CLI driver's canonical file identity
/// cannot drift. A `..` that escapes the root previously survived here as a
/// `/../foo` spelling while the driver clamped it to `/foo`, minting two
/// distinct module identities for one file.
///
/// Returns `Cow::Borrowed` when the input is already canonical (the common
/// case on the hot probe path), avoiding the per-call `PathBuf` allocation.
pub(crate) fn normalize_path_segments(path: &Path) -> Cow<'_, Path> {
    if tsz_common::module_resolution::path_identity::is_already_normalized(path) {
        return Cow::Borrowed(path);
    }
    Cow::Owned(tsz_common::module_resolution::path_identity::normalize_segments(path))
}

pub(crate) fn parse_package_specifier(specifier: &str) -> (String, Option<String>) {
    // Handle scoped packages (@scope/pkg)
    if let Some(without_at) = specifier.strip_prefix('@') {
        if let Some(scope_sep) = without_at.find('/') {
            let scope = &without_at[..scope_sep];
            let rest = &without_at[scope_sep + 1..];

            if let Some(sub_sep) = rest.find('/') {
                return (
                    format!("@{}/{}", scope, &rest[..sub_sep]),
                    Some(rest[sub_sep + 1..].to_string()),
                );
            }
            return (specifier.to_string(), None);
        }
        return (specifier.to_string(), None);
    }

    // Handle regular packages
    if let Some(slash_idx) = specifier.find('/') {
        (
            specifier[..slash_idx].to_string(),
            Some(specifier[slash_idx + 1..].to_string()),
        )
    } else {
        (specifier.to_string(), None)
    }
}

/// Convert a package name to its @types equivalent.
/// For scoped packages like `@see/saw`, this produces `@types/see__saw`.
/// For regular packages like `foo`, this produces `@types/foo`.
pub(crate) fn types_package_name(package_name: &str) -> String {
    let stripped = package_name.strip_prefix('@').unwrap_or(package_name);
    format!("@types/{}", stripped.replace('/', "__"))
}

/// Match an export pattern against a subpath
pub(crate) fn match_export_pattern(pattern: &str, subpath: &str) -> Option<String> {
    if !pattern.contains('*') {
        // Exact match for non-wildcard patterns.
        if pattern == subpath {
            return Some(String::new());
        }
        // Directory export: a pattern ending with `/` (e.g. `"./"`) acts as a
        // prefix and matches any subpath starting with that prefix (e.g.
        // `./other`, `./index.js`). The matched portion is the subpath after
        // the prefix.
        if pattern.ends_with('/') && subpath.starts_with(pattern) {
            return Some(subpath[pattern.len()..].to_string());
        }
        return None;
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() != 2 {
        return None;
    }

    let prefix = parts[0];
    let suffix = parts[1];

    if !subpath.starts_with(prefix) || !subpath.ends_with(suffix) {
        return None;
    }

    let start = prefix.len();
    let end = subpath.len().saturating_sub(suffix.len());

    if end < start {
        return None;
    }

    Some(subpath[start..end].to_string())
}

/// Specificity ranking key for an `exports`/`imports` subpath key.
///
/// `(base_length, is_pattern, total_length)`, compared lexicographically with
/// "larger wins". This mirrors Node.js `PATTERN_KEY_COMPARE` (the comparator
/// behind `PACKAGE_IMPORTS_EXPORTS_RESOLVE`, which tsc reimplements as
/// `comparePatternKeys`) so the most specific key is chosen independently of
/// JSON declaration order:
///
/// 1. **`base_length`** — the anchored prefix length. For a single-`*` key this
///    is `indexOf('*') + 1`; for a non-wildcard key (exact or `/`-suffixed
///    directory) it is the full key length. A longer base is more specific and
///    wins first. This is what lets a long directory key (`"./lib/"`, base 6)
///    correctly outrank a short wildcard (`"./*"`, base 3), and a wildcard
///    (`"./lib/*"`, base 7) outrank that directory key.
/// 2. **`is_pattern`** — `1` for keys containing `*`, else `0`. At equal base
///    length a wildcard key beats a directory/exact key, matching Node rules
///    7–8 (`if keyA does not contain "*" return 1`). Without this term `"./"`
///    and `"./*"` tie on base length `2`, and the winner flips with JSON key
///    order — the "different physical files between rows" divergence.
/// 3. **`total_length`** — longer keys win last (Node rules 9–10). For two
///    wildcards with equal base this orders by suffix length, e.g. `"./*.js"`
///    beats `"./*"`.
///
/// True ties (identical ranking) resolve to the first key in iteration order, so
/// callers must iterate an insertion-order map (`IndexMap`) and update only on
/// strict improvement (`>`).
pub(crate) fn export_pattern_specificity(pattern: &str) -> (usize, usize, usize) {
    let len = pattern.len();
    if let Some(star_pos) = pattern.find('*') {
        (star_pos + 1, 1, len)
    } else {
        (len, 0, len)
    }
}

/// Find the most-specific pattern entry that matches `target`.
///
/// Iterates `patterns` in order and returns the entry whose
/// [`export_pattern_specificity`] ranking is largest. Equal-ranking ties resolve
/// to the first entry in iteration order — callers must use an insertion-order
/// map (`IndexMap`) to get deterministic JSON-source-order tie-breaking per the
/// Node.js/TypeScript spec.
type BestExportPatternEntry<'a> = ((usize, usize, usize), &'a str, String, &'a PackageExports);

pub(crate) fn find_best_export_pattern<'a>(
    patterns: impl Iterator<Item = (&'a String, &'a PackageExports)>,
    match_fn: impl Fn(&str) -> Option<String>,
) -> Option<(&'a str, String, &'a PackageExports)> {
    let mut best: Option<BestExportPatternEntry<'a>> = None;
    for (pattern, value) in patterns {
        if let Some(matched) = match_fn(pattern) {
            let specificity = export_pattern_specificity(pattern);
            if best.as_ref().is_none_or(|(s, _, _, _)| specificity > *s) {
                best = Some((specificity, pattern.as_str(), matched, value));
            }
        }
    }
    best.map(|(_, p, m, v)| (p, m, v))
}

/// Match an imports pattern against a specifier (#-prefixed)
pub(crate) fn match_imports_pattern(pattern: &str, specifier: &str) -> Option<String> {
    if !pattern.contains('*') {
        return (pattern == specifier).then(String::new);
    }

    // Strip # prefix for matching
    let pattern = pattern.strip_prefix('#').unwrap_or(pattern);
    let specifier = specifier.strip_prefix('#').unwrap_or(specifier);

    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() != 2 {
        return None;
    }

    let prefix = parts[0];
    let suffix = parts[1];

    if !specifier.starts_with(prefix) || !specifier.ends_with(suffix) {
        return None;
    }

    let start = prefix.len();
    let end = specifier.len().saturating_sub(suffix.len());

    if end < start {
        return None;
    }

    Some(specifier[start..end].to_string())
}

// The `typesVersions` / semver algorithm is owned by
// `tsz_common::module_resolution::types_versions`. Re-export the primitives so
// the tsz-core resolver and its callers keep their existing names while there
// is a single implementation shared with the CLI driver and the checker
// redirect.
pub(crate) use tsz_common::module_resolution::types_versions::{
    SemVer, parse_semver, range_matches as types_versions_range_matches,
    select_paths as select_types_versions_paths,
};

pub(crate) fn types_versions_compiler_version(value: Option<&str>) -> SemVer {
    value
        .and_then(parse_semver)
        .unwrap_or_else(default_types_versions_compiler_version)
}

pub(crate) const fn default_types_versions_compiler_version() -> SemVer {
    TYPES_VERSIONS_COMPILER_VERSION_FALLBACK
}

// NOTE: Keep this in sync with the TypeScript version this compiler targets.
pub(crate) const TYPES_VERSIONS_COMPILER_VERSION_FALLBACK: SemVer =
    tsz_common::module_resolution::types_versions::DEFAULT_COMPILER_VERSION;

/// Apply wildcard substitution to a target path.
///
/// `is_directory_match` distinguishes the two pattern-key shapes:
/// - `false` for `*`-pattern keys (e.g. `"./*"` matched against `"./foo"`):
///   only replace `*` in the target. If the target has no `*`, leave it
///   unchanged — Node.js does NOT append the wildcard to a `/`-ending
///   target when the key was a `*` pattern.
/// - `true` for `/`-suffix directory keys (e.g. `"./lib/"`): also append
///   the wildcard to a target ending in `/`, matching Node's directory
///   prefix resolution.
pub(crate) fn apply_wildcard_substitution(
    target: &str,
    wildcard: &str,
    is_directory_match: bool,
) -> String {
    if target.contains('*') {
        target.replacen('*', wildcard, 1)
    } else if is_directory_match && target.ends_with('/') {
        format!("{target}{wildcard}")
    } else {
        target.to_string()
    }
}

/// Apply wildcard substitution recursively to all String variants in a `PackageExports` value.
/// Used for pattern exports where `*` in the target must be replaced with the matched
/// portion before path resolution (per Node.js `PACKAGE_TARGET_RESOLVE` spec).
///
/// See [`apply_wildcard_substitution`] for the `is_directory_match` semantics.
pub(crate) fn substitute_wildcard_in_exports(
    value: &PackageExports,
    wildcard: &str,
    is_directory_match: bool,
) -> PackageExports {
    match value {
        PackageExports::String(s) => {
            if s.contains('*') {
                PackageExports::String(s.replacen('*', wildcard, 1))
            } else if is_directory_match && s.ends_with('/') {
                PackageExports::String(format!("{s}{wildcard}"))
            } else {
                PackageExports::String(s.clone())
            }
        }
        PackageExports::Conditional(entries) => PackageExports::Conditional(
            entries
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        substitute_wildcard_in_exports(v, wildcard, is_directory_match),
                    )
                })
                .collect(),
        ),
        PackageExports::Map(map) => PackageExports::Map(
            map.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        substitute_wildcard_in_exports(v, wildcard, is_directory_match),
                    )
                })
                .collect::<IndexMap<_, _>>(),
        ),
        PackageExports::Array(elements) => PackageExports::Array(
            elements
                .iter()
                .map(|v| substitute_wildcard_in_exports(v, wildcard, is_directory_match))
                .collect(),
        ),
        PackageExports::Null => PackageExports::Null,
    }
}

pub(crate) fn split_path_extension(path: &Path) -> Option<(PathBuf, &'static str)> {
    let path_str = path.to_string_lossy();
    for ext in KNOWN_EXTENSIONS {
        if path_str.ends_with(ext) {
            let base = &path_str[..path_str.len().saturating_sub(ext.len())];
            if base.is_empty() {
                return None;
            }
            return Some((PathBuf::from(base), ext.trim_start_matches('.')));
        }
    }
    None
}

pub(crate) fn try_file_with_suffixes(path: &Path, suffixes: &[String]) -> Option<PathBuf> {
    let (base, extension) = split_path_extension(path)?;
    try_file_with_suffixes_and_extension(&base, extension, suffixes)
}

pub(crate) fn try_file_with_suffixes_and_extension(
    base: &Path,
    extension: &str,
    suffixes: &[String],
) -> Option<PathBuf> {
    for suffix in suffixes {
        let Some(candidate) = path_with_suffix_and_extension(base, suffix, extension) else {
            continue;
        };
        if cached_is_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}

pub(crate) fn path_with_suffix_and_extension(
    base: &Path,
    suffix: &str,
    extension: &str,
) -> Option<PathBuf> {
    let file_name = base.file_name()?.to_string_lossy();
    let mut candidate = base.to_path_buf();
    let mut new_name = String::with_capacity(file_name.len() + suffix.len() + extension.len() + 1);
    new_name.push_str(&file_name);
    new_name.push_str(suffix);
    new_name.push('.');
    new_name.push_str(extension);
    candidate.set_file_name(new_name);
    Some(candidate)
}

pub(crate) fn try_arbitrary_extension_declaration(path: &Path, extension: &str) -> Option<PathBuf> {
    let declaration = path.with_extension(format!("d.{extension}.ts"));
    if cached_is_file(&declaration) {
        return Some(declaration);
    }
    None
}

pub(crate) fn resolve_explicit_unknown_extension(path: &Path) -> Option<PathBuf> {
    path.extension()?;
    if split_path_extension(path).is_some() {
        return None;
    }
    if cached_is_file(path) {
        return Some(path.to_path_buf());
    }
    None
}

pub(crate) const KNOWN_EXTENSIONS: [&str; 12] = [
    ".d.mts", ".d.cts", ".d.ts", ".mts", ".cts", ".tsx", ".ts", ".mjs", ".cjs", ".jsx", ".js",
    ".json",
];
/// TS-only candidate priority for path-mapping, baseUrl, classic, and bundler
/// probes (mirrors tsc's `supportedTSExtensions` — `[Ts, Tsx, Dts]` then
/// `[Cts, Dcts]` then `[Mts, Dmts]`).
pub(crate) const TS_EXTENSION_CANDIDATES: &[&str] =
    tsz_common::file_extensions::TSC_TS_RESOLUTION_EXTENSIONS_BARE;
pub(crate) const NODE16_MODULE_EXTENSION_CANDIDATES: [&str; 7] =
    ["mts", "d.mts", "ts", "tsx", "d.ts", "cts", "d.cts"];
pub(crate) const NODE16_MODULE_ALLOWJS_EXTENSION_CANDIDATES: [&str; 11] = [
    "mts", "d.mts", "ts", "tsx", "d.ts", "cts", "d.cts", "mjs", "js", "jsx", "cjs",
];
pub(crate) const NODE16_COMMONJS_EXTENSION_CANDIDATES: [&str; 7] =
    ["cts", "d.cts", "ts", "tsx", "d.ts", "mts", "d.mts"];
pub(crate) const NODE16_COMMONJS_ALLOWJS_EXTENSION_CANDIDATES: [&str; 11] = [
    "cts", "d.cts", "ts", "tsx", "d.ts", "mts", "d.mts", "cjs", "js", "jsx", "mjs",
];
pub(crate) const CLASSIC_EXTENSION_CANDIDATES: &[&str] = TS_EXTENSION_CANDIDATES;

/// TS+JS candidate priority for path-mapping, baseUrl, classic, and bundler
/// probes when `allowJs` is enabled (mirrors tsc's `allSupportedExtensions` —
/// `[Ts, Tsx, Dts, Js, Jsx]` then `[Cts, Dcts, Cjs]` then `[Mts, Dmts, Mjs]`).
pub(crate) const TS_JS_EXTENSION_CANDIDATES: &[&str] =
    tsz_common::file_extensions::TSC_TS_JS_RESOLUTION_EXTENSIONS_BARE;

pub(crate) fn node16_extension_substitution(path: &Path, extension: &str) -> Option<Vec<PathBuf>> {
    let replacements: &[&str] = match extension {
        "js" => &["ts", "tsx", "d.ts"],
        "jsx" => &["tsx", "d.ts"],
        "mjs" => &["mts", "d.mts"],
        "cjs" => &["cts", "d.cts"],
        "d.ts" => &["ts", "tsx"],
        "d.mts" => &["mts"],
        "d.cts" => &["cts"],
        _ => return None,
    };

    let base = split_path_extension(path)
        .map(|(base, _)| base)
        .unwrap_or_else(|| path.to_path_buf());

    Some(
        replacements
            .iter()
            .map(|ext| base.with_extension(ext))
            .collect(),
    )
}

pub(crate) fn declaration_substitution_for_main(path: &Path) -> Option<PathBuf> {
    let extension = path.extension().and_then(|ext| ext.to_str())?;
    match extension {
        "js" | "jsx" => Some(path.with_extension("d.ts")),
        "mjs" => Some(path.with_extension("d.mts")),
        "cjs" => Some(path.with_extension("d.cts")),
        _ => None,
    }
}

/// Simplified package.json structure for resolution
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageJson {
    #[serde(default, deserialize_with = "deserialize_optional_string_field")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string_field")]
    pub main: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string_field")]
    pub types: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string_field")]
    pub typings: Option<String>,
    #[serde(rename = "type")]
    #[serde(default, deserialize_with = "deserialize_optional_string_field")]
    pub package_type: Option<String>,
    pub exports: Option<PackageExports>,
    pub imports: Option<IndexMap<String, PackageExports>>,
    /// TypeScript typesVersions field for version-specific type definitions
    #[serde(rename = "typesVersions")]
    pub types_versions: Option<serde_json::Value>,
}

fn deserialize_optional_string_field<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <Option<serde_json::Value> as serde::Deserialize>::deserialize(deserializer)?;
    Ok(value.and_then(|value: serde_json::Value| value.as_str().map(ToOwned::to_owned)))
}

/// Package exports field can be a string, map, or conditional
///
/// Map variant: keys start with "." (subpath patterns like ".", "./foo")
///   Uses `IndexMap` to preserve JSON key order — required for deterministic
///   pattern-matching tie-breaking when two wildcard patterns have equal specificity.
/// Conditional variant: keys don't start with "." (condition names like "import", "default")
///   Uses Vec to preserve JSON key order (required for correct condition matching)
#[derive(Debug, Clone)]
pub(crate) enum PackageExports {
    String(String),
    Map(IndexMap<String, Self>),
    Conditional(Vec<(String, Self)>),
    /// Array of fallback targets — Node.js tries each element in order until one resolves
    Array(Vec<Self>),
    /// null in JSON — indicates an explicitly blocked export
    Null,
}

impl<'de> serde::Deserialize<'de> for PackageExports {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de;

        struct PackageExportsVisitor;

        impl<'de> de::Visitor<'de> for PackageExportsVisitor {
            type Value = PackageExports;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string, object, or null")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PackageExports::String(v.to_string()))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PackageExports::Null)
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PackageExports::Null)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let mut elements = Vec::new();
                while let Some(element) = seq.next_element::<PackageExports>()? {
                    elements.push(element);
                }
                Ok(PackageExports::Array(elements))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                // IndexMap preserves JSON insertion order for the subpath Map variant.
                // This is required for deterministic pattern-matching tie-breaking:
                // when two wildcard patterns have equal specificity, the first one
                // in JSON source order must win (per Node.js/TypeScript spec).
                let mut map_entries = IndexMap::default();
                let mut cond_entries = Vec::new();
                let mut is_subpath_map = None;

                while let Some((key, value)) = map.next_entry::<String, PackageExports>()? {
                    if is_subpath_map.is_none() {
                        is_subpath_map = Some(key.starts_with('.'));
                    }
                    if is_subpath_map == Some(true) {
                        map_entries.insert(key, value);
                    } else {
                        cond_entries.push((key, value));
                    }
                }

                if is_subpath_map.unwrap_or(false) {
                    Ok(PackageExports::Map(map_entries))
                } else {
                    Ok(PackageExports::Conditional(cond_entries))
                }
            }
        }

        deserializer.deserialize_any(PackageExportsVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    /// Pick the winning key from `keys` for `subpath`, returning the key text.
    /// Mirrors how the resolver calls `find_best_export_pattern` over a subpath
    /// `IndexMap`, but keeps the test free of filesystem probing.
    fn best_key<'a>(keys: &'a [&'a str], subpath: &str) -> Option<&'a str> {
        // A stable dummy value per key — only the selected *key* matters here.
        let entries: IndexMap<String, PackageExports> = keys
            .iter()
            .map(|k| (k.to_string(), PackageExports::String(String::new())))
            .collect();
        find_best_export_pattern(entries.iter(), |p| match_export_pattern(p, subpath))
            .map(|(pattern, _, _)| keys.iter().copied().find(|k| *k == pattern).unwrap())
    }

    #[test]
    fn export_pattern_specificity_mirrors_pattern_key_compare() {
        // Wildcard key: base = indexOf('*') + 1, flagged as a pattern.
        assert_eq!(export_pattern_specificity("./*"), (3, 1, 3));
        assert_eq!(export_pattern_specificity("./lib/*"), (7, 1, 7));
        assert_eq!(export_pattern_specificity("./*.js"), (3, 1, 6));
        // Directory / exact key: base = full length, not a pattern.
        assert_eq!(export_pattern_specificity("./"), (2, 0, 2));
        assert_eq!(export_pattern_specificity("./lib/"), (6, 0, 6));
        assert_eq!(export_pattern_specificity("./foo"), (5, 0, 5));
    }

    #[test]
    fn find_best_export_pattern_prefers_wildcard_over_equal_base_directory_either_order() {
        // `"./*"` (base 3) must beat `"./"` (base 2) for `./foo`, no matter the
        // JSON declaration order. Before the PATTERN_KEY_COMPARE fix these tied
        // on `(prefix_len, suffix_len)` and the winner flipped with key order,
        // resolving the same specifier to different physical files between rows.
        assert_eq!(best_key(&["./", "./*"], "./foo"), Some("./*"));
        assert_eq!(best_key(&["./*", "./"], "./foo"), Some("./*"));
    }

    #[test]
    fn find_best_export_pattern_longer_directory_base_beats_short_wildcard() {
        // A longer anchored directory prefix is more specific than a short
        // wildcard (Node orders by base length first): `"./lib/"` (base 6) wins
        // over `"./*"` (base 3) for `./lib/x`, independent of order.
        assert_eq!(best_key(&["./*", "./lib/"], "./lib/x"), Some("./lib/"));
        assert_eq!(best_key(&["./lib/", "./*"], "./lib/x"), Some("./lib/"));
        // …but a wildcard with an even longer base reclaims the win.
        assert_eq!(best_key(&["./lib/", "./lib/*"], "./lib/x"), Some("./lib/*"));
        assert_eq!(best_key(&["./lib/*", "./lib/"], "./lib/x"), Some("./lib/*"));
    }

    #[test]
    fn find_best_export_pattern_orders_wildcards_by_base_then_total_length() {
        // Longer prefix before `*` wins.
        assert_eq!(
            best_key(&["./*", "./feature/*"], "./feature/btn"),
            Some("./feature/*")
        );
        assert_eq!(
            best_key(&["./feature/*", "./*"], "./feature/btn"),
            Some("./feature/*")
        );
        // Equal base length → longer total (longer suffix) wins.
        assert_eq!(best_key(&["./*", "./*.js"], "./a.js"), Some("./*.js"));
        assert_eq!(best_key(&["./*.js", "./*"], "./a.js"), Some("./*.js"));
    }

    #[test]
    fn types_versions_compiler_version_uses_trimmed_value_and_fallback() {
        assert_eq!(
            types_versions_compiler_version(Some(" 5.4 ")),
            SemVer {
                major: 5,
                minor: 4,
                patch: 0,
            }
        );
        assert_eq!(
            types_versions_compiler_version(Some("not-a-version")),
            default_types_versions_compiler_version()
        );
        assert_eq!(
            types_versions_compiler_version(None),
            SemVer {
                major: 6,
                minor: 0,
                patch: 3,
            }
        );
    }

    #[test]
    fn parse_semver_ignores_prerelease_and_build_metadata() {
        assert_eq!(
            parse_semver("3.1.0-0"),
            Some(SemVer {
                major: 3,
                minor: 1,
                patch: 0,
            })
        );
        assert_eq!(
            parse_semver("5.4.1+dev"),
            Some(SemVer {
                major: 5,
                minor: 4,
                patch: 1,
            })
        );
    }

    #[test]
    fn select_types_versions_paths_returns_first_matching_key_in_declaration_order() {
        // tsc's `getPackageJsonTypesVersionsPaths` is a `for...in` loop that
        // returns the first key whose range satisfies the compiler version.
        // With `"*"` declared first, every later key is unreachable — even a
        // tighter `">=5.4"` range. This pins parity with that behavior.
        let types_versions = json!({
            "*": { "*": ["fallback/index.d.ts"] },
            ">=5.4": { "*": ["modern/index.d.ts"] },
            ">=5.2 <5.4": { "*": ["mid/index.d.ts"] }
        });

        let selected = select_types_versions_paths(
            &types_versions,
            SemVer {
                major: 5,
                minor: 4,
                patch: 1,
            },
        )
        .expect("expected a matching typesVersions entry");

        assert_eq!(selected.get("*"), Some(&json!(["fallback/index.d.ts"])));

        // The natural ordering — fallback last — picks the tighter range.
        let types_versions_natural = json!({
            ">=5.4": { "*": ["modern/index.d.ts"] },
            ">=5.2 <5.4": { "*": ["mid/index.d.ts"] },
            "*": { "*": ["fallback/index.d.ts"] }
        });

        let selected_natural = select_types_versions_paths(
            &types_versions_natural,
            SemVer {
                major: 5,
                minor: 4,
                patch: 1,
            },
        )
        .expect("expected a matching typesVersions entry");

        assert_eq!(
            selected_natural.get("*"),
            Some(&json!(["modern/index.d.ts"]))
        );
    }

    #[test]
    fn select_types_versions_paths_ties_resolve_to_first_in_declaration_order() {
        // Two equally-matching keys: tsc picks whichever was declared first,
        // regardless of lex order or constraint count.
        let first_wins = json!({
            "<=6.0": { "*": ["first/index.d.ts"] },
            "<=5.0": { "*": ["second/index.d.ts"] }
        });

        let selected = select_types_versions_paths(
            &first_wins,
            SemVer {
                major: 4,
                minor: 9,
                patch: 0,
            },
        )
        .expect("expected a matching typesVersions entry");

        assert_eq!(selected.get("*"), Some(&json!(["first/index.d.ts"])));

        // Same content, reversed declaration order — the (now-first) `<=5.0`
        // key wins instead.
        let reversed = json!({
            "<=5.0": { "*": ["second/index.d.ts"] },
            "<=6.0": { "*": ["first/index.d.ts"] }
        });

        let selected_reversed = select_types_versions_paths(
            &reversed,
            SemVer {
                major: 4,
                minor: 9,
                patch: 0,
            },
        )
        .expect("expected a matching typesVersions entry");

        assert_eq!(
            selected_reversed.get("*"),
            Some(&json!(["second/index.d.ts"]))
        );
    }

    #[test]
    fn select_types_versions_paths_skips_unparseable_keys() {
        // An invalid range key parses as `None` and is skipped; iteration
        // continues to the next valid key.
        let types_versions = json!({
            "not-a-range": { "*": ["skipped/index.d.ts"] },
            ">=5.4": { "*": ["modern/index.d.ts"] }
        });

        let selected = select_types_versions_paths(
            &types_versions,
            SemVer {
                major: 5,
                minor: 4,
                patch: 1,
            },
        )
        .expect("expected a matching typesVersions entry");

        assert_eq!(selected.get("*"), Some(&json!(["modern/index.d.ts"])));
    }

    #[test]
    fn types_versions_range_matches_bare_star_and_empty() {
        let v = SemVer {
            major: 6,
            minor: 0,
            patch: 0,
        };
        assert!(types_versions_range_matches("*", v));
        assert!(types_versions_range_matches("", v));
        assert!(types_versions_range_matches(">=4 <7", v));
        assert!(!types_versions_range_matches(">=7", v));
        // Disjunction: any segment may match.
        assert!(types_versions_range_matches(">=7 || <=6", v));
        // Invalid token in one segment fails just that segment.
        assert!(types_versions_range_matches(">=garbage || >=4", v));
    }

    // `parse_pattern` (multi-`*` rejection), `parse_range_token`,
    // `compare_range`, and `RangeOp` are owned and unit-tested by
    // `tsz_common::module_resolution::types_versions`; the re-exported
    // `select_types_versions_paths` / `types_versions_range_matches` above
    // exercise them end-to-end here.

    #[test]
    fn split_path_extension_prefers_longest_known_declaration_extension() {
        let (base, extension) =
            split_path_extension(Path::new("pkg/index.d.mts")).expect("expected known extension");
        assert_eq!(base, PathBuf::from("pkg/index"));
        assert_eq!(extension, "d.mts");

        let (base, extension) =
            split_path_extension(Path::new("pkg/index.d.ts")).expect("expected known extension");
        assert_eq!(base, PathBuf::from("pkg/index"));
        assert_eq!(extension, "d.ts");
    }

    #[test]
    fn declaration_extension_substitution_probes_sibling_implementations() {
        let dts = node16_extension_substitution(Path::new("pkg/a.d.ts"), "d.ts")
            .expect("expected declaration extension substitution");
        assert_eq!(
            dts,
            vec![PathBuf::from("pkg/a.ts"), PathBuf::from("pkg/a.tsx")]
        );

        let dmts = node16_extension_substitution(Path::new("pkg/a.d.mts"), "d.mts")
            .expect("expected declaration module substitution");
        assert_eq!(dmts, vec![PathBuf::from("pkg/a.mts")]);

        let dcts = node16_extension_substitution(Path::new("pkg/a.d.cts"), "d.cts")
            .expect("expected declaration commonjs substitution");
        assert_eq!(dcts, vec![PathBuf::from("pkg/a.cts")]);
    }

    #[test]
    fn try_file_with_suffixes_and_extension_returns_first_existing_candidate() {
        let dir = tempdir().expect("create temp dir");
        let base = dir.path().join("component");
        let preferred = dir.path().join("component.native.ts");
        let fallback = dir.path().join("component.web.ts");

        std::fs::write(&preferred, "").expect("write preferred candidate");
        std::fs::write(&fallback, "").expect("write fallback candidate");

        let resolved = try_file_with_suffixes_and_extension(
            &base,
            "ts",
            &[".native".to_string(), ".web".to_string()],
        )
        .expect("expected one suffix candidate to resolve");

        assert_eq!(resolved, preferred);
    }

    #[test]
    fn resolve_explicit_unknown_extension_accepts_existing_nonstandard_files_only() {
        let dir = tempdir().expect("create temp dir");
        let custom = dir.path().join("entry.custom");
        let known = dir.path().join("entry.ts");
        let no_extension = dir.path().join("entry");

        std::fs::write(&custom, "").expect("write custom extension file");
        std::fs::write(&known, "").expect("write known extension file");
        std::fs::write(&no_extension, "").expect("write extensionless file");

        assert_eq!(
            resolve_explicit_unknown_extension(&custom),
            Some(custom.clone())
        );
        assert_eq!(resolve_explicit_unknown_extension(&known), None);
        assert_eq!(resolve_explicit_unknown_extension(&no_extension), None);
    }

    #[test]
    fn node16_and_main_declaration_substitutions_cover_js_family_extensions() {
        assert_eq!(
            node16_extension_substitution(Path::new("pkg/index.js"), "js"),
            Some(vec![
                PathBuf::from("pkg/index.ts"),
                PathBuf::from("pkg/index.tsx"),
                PathBuf::from("pkg/index.d.ts"),
            ])
        );
        assert_eq!(
            node16_extension_substitution(Path::new("pkg/index.mjs"), "mjs"),
            Some(vec![
                PathBuf::from("pkg/index.mts"),
                PathBuf::from("pkg/index.d.mts"),
            ])
        );
        assert_eq!(
            declaration_substitution_for_main(Path::new("pkg/index.cjs")),
            Some(PathBuf::from("pkg/index.d.cts"))
        );
        assert_eq!(
            declaration_substitution_for_main(Path::new("pkg/index.jsx")),
            Some(PathBuf::from("pkg/index.d.ts"))
        );
        assert_eq!(
            declaration_substitution_for_main(Path::new("pkg/index.ts")),
            None
        );
    }

    #[test]
    fn path_existence_caches_are_stable_until_reset_for_files_and_directories() {
        clear_path_existence_caches();
        let root = tempdir().expect("create temp dir");
        let file = root.path().join("index.ts");
        let dir = root.path().join("nested");
        std::fs::write(&file, "").expect("write probed file");
        std::fs::create_dir(&dir).expect("create probed directory");

        // First probes record the file and directory as present.
        assert!(cached_is_file(&file));
        assert!(cached_is_dir(&dir));

        // Remove both underneath the caches. Within a single compilation the
        // filesystem is assumed stable, so the cached answers are reused even
        // though the paths are now gone. This is what collapses the repeated
        // `stat()` syscalls the resolver would otherwise issue for the same
        // files and ancestor directories across every import.
        std::fs::remove_file(&file).expect("remove probed file");
        std::fs::remove_dir(&dir).expect("remove probed directory");
        assert!(cached_is_file(&file));
        assert!(cached_is_dir(&dir));

        // The unified reset clears both caches (not just the file cache), so
        // the next compilation cycle re-reads the real filesystem state.
        clear_path_existence_caches();
        assert!(!cached_is_file(&file));
        assert!(!cached_is_dir(&dir));
    }
}
