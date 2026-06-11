//! Shared reverse lookup helpers for `package.json#exports`.
//!
//! Declaration emit and checker nameability both need to answer the same
//! question: given a package-local runtime target such as `./dist/index.js`,
//! which public export specifier, if any, exposes it? Keeping this logic here
//! prevents checker diagnostics and DTS output from drifting on condition,
//! wildcard, array, and folder-entry handling.

use serde_json::Value;
use std::path::Path;

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
}
