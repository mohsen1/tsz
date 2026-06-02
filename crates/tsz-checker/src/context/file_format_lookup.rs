//! Per-file ESM/CJS format lookup from `file_is_esm_map`.
//!
//! The driver builds a project-wide map from file name to ESM/CJS flag for
//! Node16/NodeNext/Bundler resolution. Checker code asks this map "is the
//! file at `<path>` an ESM file?" for decisions such as whether to emit
//! TS1192 for a missing default export, whether `export =` is allowed, and
//! which resolution mode to pick for extensionless imports.
//!
//! # Key contract
//!
//! Map keys are normalized to forward slashes at insertion time by the driver
//! and test helpers (see `normalize_path_key`). The lookup normalizes the
//! query to the same form, so every lookup is a single `map.get` call with
//! no fallback scan.
//!
//! Prior to this normalization the map was built with raw `file.file_name`
//! strings (which use backslashes on Windows) while query strings came from
//! `source_file.file_name` (which stores forward slashes). A multi-probe
//! suffix-match fallback compensated for that asymmetry; it is no longer
//! needed now that both sides agree on one canonical form.

use rustc_hash::FxHashMap;
use std::borrow::Cow;

/// Normalize a file-path key for insertion into `file_is_esm_map` or
/// `is_external_module_by_file`. Converts backslashes to forward slashes so
/// all map keys are in the same canonical form that `source_file.file_name`
/// uses at lookup time.
pub(crate) fn normalize_path_key(path: &str) -> String {
    if path.contains('\\') {
        path.replace('\\', "/")
    } else {
        path.to_owned()
    }
}

/// Look up whether `file_name` is an ESM file in `file_is_esm_map`.
///
/// Keys in the map are normalized to forward slashes at insertion; this
/// function normalizes the query to the same form and returns the result of
/// a single hash-map probe. Returns `None` when no key matches; callers
/// should fall back to compiler-option or extension-based heuristics.
pub(crate) fn lookup_file_is_esm_in_map(
    map: &FxHashMap<String, bool>,
    file_name: &str,
) -> Option<bool> {
    if map.is_empty() {
        return None;
    }
    let normalized = normalize_slashes(file_name);
    map.get(normalized.as_ref()).copied()
}

/// Convert backslashes to forward slashes, borrowing the input when no
/// conversion is needed.
fn normalize_slashes(path: &str) -> Cow<'_, str> {
    if path.contains('\\') {
        Cow::Owned(path.replace('\\', "/"))
    } else {
        Cow::Borrowed(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_of(entries: &[(&str, bool)]) -> FxHashMap<String, bool> {
        entries
            .iter()
            .map(|(k, v)| ((*k).to_string(), *v))
            .collect()
    }

    #[test]
    fn empty_map_returns_none() {
        let map = FxHashMap::default();
        assert_eq!(lookup_file_is_esm_in_map(&map, "/proj/foo.ts"), None);
    }

    #[test]
    fn direct_hit_returns_value() {
        let map = map_of(&[("/proj/foo.ts", true), ("/proj/bar.ts", false)]);
        assert_eq!(lookup_file_is_esm_in_map(&map, "/proj/foo.ts"), Some(true));
        assert_eq!(lookup_file_is_esm_in_map(&map, "/proj/bar.ts"), Some(false));
    }

    /// Query with backslashes is normalized to forward slashes before lookup.
    /// Map keys are always forward slashes (normalized at insertion by the driver).
    #[test]
    fn backslash_query_hits_normalized_key() {
        let map = map_of(&[("/proj/foo.ts", true)]);
        assert_eq!(
            lookup_file_is_esm_in_map(&map, "\\proj\\foo.ts"),
            Some(true)
        );
    }

    #[test]
    fn no_match_returns_none() {
        let map = map_of(&[("/proj/foo.ts", true)]);
        assert_eq!(lookup_file_is_esm_in_map(&map, "/proj/bar.ts"), None);
    }

    /// Test helpers in `conformance_issues/modules/context.rs` key the map
    /// with bare basenames (`mod.cts`, `b.mts`); the checker queries with the
    /// same names. Direct lookup must still work.
    #[test]
    fn basename_keys_match_basename_queries() {
        let map = map_of(&[("mod.cts", false), ("b.mts", true)]);
        assert_eq!(lookup_file_is_esm_in_map(&map, "mod.cts"), Some(false));
        assert_eq!(lookup_file_is_esm_in_map(&map, "b.mts"), Some(true));
    }

    #[test]
    fn normalize_path_key_converts_backslashes() {
        assert_eq!(normalize_path_key("C:\\proj\\foo.ts"), "C:/proj/foo.ts");
        assert_eq!(normalize_path_key("/proj/foo.ts"), "/proj/foo.ts");
        assert_eq!(normalize_path_key("mod.cts"), "mod.cts");
    }
}
