//! Shared `package.json#exports`/`imports` primitives.
//!
//! Two directions live here so the forward resolver, reverse lookup, checker
//! diagnostics, and DTS output cannot drift on condition, wildcard, array, and
//! folder-entry handling:
//!
//! - **Forward** ([`pattern_key_specificity`]): rank subpath pattern keys by
//!   Node.js `comparePatternKeys` so both the tsz-core resolver and the CLI
//!   driver pick the same most-specific key.
//! - **Reverse**: given a package-local runtime target such as
//!   `./dist/index.js`, which public export specifier, if any, exposes it?
//!   (Declaration emit and checker nameability need this.)

use serde_json::Value;
use std::path::Path;

/// Specificity ranking key for a `package.json` `exports`/`imports` subpath
/// pattern key, mirroring Node.js `comparePatternKeys` (which `tsc`
/// reimplements in `moduleNameResolver`). The returned tuple is compared
/// lexicographically with **larger wins**, reproducing the comparator's sort
/// order so the single most-specific matching key is chosen independently of
/// JSON declaration order:
///
/// 1. **`base_length`** — the anchored prefix length. For a single-`*` key this
///    is `indexOf('*') + 1`; for a non-wildcard key (exact or `/`-suffixed
///    directory) it is the full key length. A longer base is more specific. This
///    is what lets a long directory key (`"./lib/"`, base 6) outrank a short
///    wildcard (`"./*"`, base 3), and a wildcard (`"./lib/*"`, base 7) outrank
///    that directory key.
/// 2. **`is_pattern`** — `1` for keys containing `*`, else `0`. At equal base
///    length a wildcard key beats a directory/exact key, matching
///    `comparePatternKeys` (`aPatternIndex === -1 ? 1`). Without this term a
///    directory key (`"./"`, base 2) ties its wildcard (`"./*"`, base 2 with a
///    2-tuple comparator) and the winner flips with JSON key order.
/// 3. **`total_length`** — longer keys win last, reached only when both keys are
///    wildcards with equal base. For two wildcards with equal base this orders
///    by suffix length, e.g. `"./*.js"` beats `"./*"`.
///
/// True ties (identical ranking) resolve to the first key in iteration order, so
/// callers must iterate an insertion-order map and update only on strict
/// improvement (`>`).
pub fn pattern_key_specificity(key: &str) -> (usize, usize, usize) {
    let len = key.len();
    match key.find('*') {
        Some(star_index) => (star_index + 1, 1, len),
        None => (len, 0, len),
    }
}

/// Read `package.json` under `package_root` and reverse-match its `exports`
/// map for `runtime_relative_path`.
///
/// The returned string omits the leading package name and leading `./`.
/// The package root export (`"."`) is returned as an empty string so callers
/// can append it directly to the package name.
pub fn reverse_export_specifier_for_runtime_path(
    package_root: &Path,
    runtime_relative_path: &str,
) -> Option<String> {
    let package_json_path = package_root.join("package.json");
    let package_json = std::fs::read_to_string(package_json_path).ok()?;
    let package_json: Value = serde_json::from_str(&package_json).ok()?;
    let exports = package_json.get("exports")?;
    let runtime_relative_path = format!("./{}", runtime_relative_path.trim_start_matches("./"));
    reverse_match_exports_subpath(exports, &runtime_relative_path)
}

/// Reverse-match a package `exports` value for a package-local runtime target.
pub fn reverse_match_exports_subpath(exports: &Value, runtime_path: &str) -> Option<String> {
    match exports {
        Value::String(target) => match_export_target(".", target, runtime_path),
        Value::Array(entries) => entries
            .iter()
            .find_map(|entry| reverse_match_exports_subpath(entry, runtime_path)),
        Value::Object(map) => {
            for (key, value) in map {
                if key == "." || key.starts_with("./") {
                    if let Some(specifier) = reverse_match_export_entry(key, value, runtime_path) {
                        return Some(specifier);
                    }
                    continue;
                }

                if let Some(specifier) = reverse_match_exports_subpath(value, runtime_path) {
                    return Some(specifier);
                }
            }
            None
        }
        _ => None,
    }
}

/// Reverse-match one export-map entry for a package-local runtime target.
pub fn reverse_match_export_entry(
    subpath_key: &str,
    value: &Value,
    runtime_path: &str,
) -> Option<String> {
    match value {
        Value::String(target) => match_export_target(subpath_key, target, runtime_path),
        Value::Array(entries) => entries
            .iter()
            .find_map(|entry| reverse_match_export_entry(subpath_key, entry, runtime_path)),
        Value::Object(map) => map
            .values()
            .find_map(|entry| reverse_match_export_entry(subpath_key, entry, runtime_path)),
        _ => None,
    }
}

/// Match an `exports` target string and synthesize the public subpath.
pub fn match_export_target(subpath_key: &str, target: &str, runtime_path: &str) -> Option<String> {
    let target = target.trim();
    let runtime_path = runtime_path.trim();

    if target.contains('*') {
        let wildcard = match_exports_wildcard(target, runtime_path)?;
        return Some(apply_exports_wildcard(subpath_key, &wildcard));
    }

    if target.ends_with('/') && subpath_key.ends_with('/') {
        let remainder = runtime_path.strip_prefix(target)?;
        return Some(format!(
            "{}{}",
            subpath_key.trim_start_matches("./"),
            remainder
        ));
    }

    if target != runtime_path {
        return None;
    }

    if subpath_key == "." {
        return Some(String::new());
    }

    Some(subpath_key.trim_start_matches("./").to_string())
}

/// Return the wildcard substring captured by a single-star export pattern.
pub fn match_exports_wildcard(pattern: &str, value: &str) -> Option<String> {
    let star_idx = pattern.find('*')?;
    let prefix = &pattern[..star_idx];
    let suffix = &pattern[star_idx + 1..];
    let middle = value.strip_prefix(prefix)?.strip_suffix(suffix)?;
    Some(middle.to_string())
}

/// Apply a captured wildcard to a public export pattern.
pub fn apply_exports_wildcard(pattern: &str, wildcard: &str) -> String {
    pattern
        .replace('*', wildcard)
        .trim_start_matches("./")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reverse_match_root_export_returns_empty_subpath() {
        let exports = json!("./dist/index.js");
        assert_eq!(
            reverse_match_exports_subpath(&exports, "./dist/index.js"),
            Some(String::new())
        );
    }

    #[test]
    fn reverse_match_subpath_wildcard_applies_captured_middle() {
        let exports = json!({
            "./feature/*": "./dist/feature/*.js"
        });

        assert_eq!(
            reverse_match_exports_subpath(&exports, "./dist/feature/a/b.js"),
            Some("feature/a/b".to_string())
        );
    }

    #[test]
    fn reverse_match_preserves_condition_order() {
        let exports = json!({
            ".": {
                "types": "./dist/index.d.ts",
                "default": "./dist/index.js"
            }
        });

        assert_eq!(
            reverse_match_exports_subpath(&exports, "./dist/index.d.ts"),
            Some(String::new())
        );
        assert_eq!(
            reverse_match_exports_subpath(&exports, "./dist/index.js"),
            Some(String::new())
        );
    }

    #[test]
    fn match_export_target_supports_folder_entries() {
        assert_eq!(
            match_export_target("./feature/", "./dist/feature/", "./dist/feature/a.js"),
            Some("feature/a.js".to_string())
        );
    }

    #[test]
    fn pattern_key_specificity_mirrors_compare_pattern_keys() {
        // Non-wildcard (exact / `/`-directory) keys: base == total == full length.
        assert_eq!(pattern_key_specificity("./exact.js"), (10, 0, 10));
        assert_eq!(pattern_key_specificity("./"), (2, 0, 2));
        assert_eq!(pattern_key_specificity("./lib/"), (6, 0, 6));
        // Wildcard keys: base == indexOf('*') + 1, is_pattern == 1.
        assert_eq!(pattern_key_specificity("./*"), (3, 1, 3));
        assert_eq!(pattern_key_specificity("./lib/*"), (7, 1, 7));
        assert_eq!(pattern_key_specificity("./*.ts"), (3, 1, 6));

        // A wildcard strictly outranks the directory key it shares a base with —
        // the order-independence the 2-tuple comparator lost.
        assert!(pattern_key_specificity("./*") > pattern_key_specificity("./"));
        assert!(pattern_key_specificity("./lib/*") > pattern_key_specificity("./lib/"));
        // Longer base still wins first (directory beats shorter wildcard).
        assert!(pattern_key_specificity("./lib/") > pattern_key_specificity("./*"));
        // Equal base, both wildcards: longer total (suffix) wins.
        assert!(pattern_key_specificity("./*.js") > pattern_key_specificity("./*"));
    }
}
