use std::path::{Path, PathBuf};

use crate::config::ResolvedCompilerOptions;
use tsz::module_resolver::PackageType;
use tsz_common::module_resolution::types_versions;

#[allow(unused_imports)]
use super::*;

// Shared `typesVersions`/semver primitives live in `tsz_common`. Re-export the
// names still used across the CLI resolver (versioned export conditions, the
// compiler-version helpers in `package_resolution`) so call sites stay stable
// while there is a single implementation of the algorithm.
pub(crate) use tsz_common::module_resolution::types_versions::{
    SemVer, parse_semver, range_matches as types_versions_range_matches,
};

/// Resolve a `typesVersions` paths object against a subpath, then probe the
/// resulting candidate targets on disk via `resolve_package_entry`.
///
/// The version-range selection and exact/longest-prefix pattern matching are
/// owned by the shared `tsz_common::module_resolution::types_versions` module
/// so the CLI driver and the checker redirect cannot drift from each other or
/// from tsc.
pub(crate) fn resolve_types_versions(
    package_root: &Path,
    subpath: &str,
    types_versions: &serde_json::Value,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
    resolution_cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    let compiler_version = types_versions_compiler_version(options);
    let paths = types_versions::select_paths(types_versions, compiler_version)?;
    for target in types_versions::candidate_targets(paths, subpath) {
        if let Some(resolved) = resolve_package_entry(
            package_root,
            &target,
            options,
            package_type,
            resolution_cache,
        ) {
            return Some(resolved);
        }
    }
    None
}

pub(crate) fn resolve_exports_subpath(
    exports: &serde_json::Value,
    subpath_key: &str,
    conditions: &[&str],
    compiler_version: SemVer,
) -> Option<String> {
    match exports {
        serde_json::Value::String(value) => (subpath_key == ".").then(|| value.clone()),
        serde_json::Value::Array(list) => {
            for entry in list {
                if let Some(resolved) =
                    resolve_exports_subpath(entry, subpath_key, conditions, compiler_version)
                {
                    return Some(resolved);
                }
            }
            None
        }
        serde_json::Value::Object(map) => {
            let has_subpath_keys = map.keys().any(|key| key.starts_with('.'));
            if has_subpath_keys {
                if let Some(value) = map.get(subpath_key)
                    && let Some(target) =
                        resolve_exports_target(value, conditions, compiler_version)
                {
                    return Some(target);
                }

                if let Some((wildcard, value)) =
                    find_best_subpath_pattern(map, |key| match_exports_subpath(key, subpath_key))
                    && let Some(target) =
                        resolve_exports_target(value, conditions, compiler_version)
                {
                    return Some(apply_exports_subpath(&target, &wildcard));
                }

                None
            } else if subpath_key == "." {
                resolve_exports_target(exports, conditions, compiler_version)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) fn resolve_exports_target(
    target: &serde_json::Value,
    conditions: &[&str],
    compiler_version: SemVer,
) -> Option<String> {
    match target {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Array(list) => {
            for entry in list {
                if let Some(resolved) = resolve_exports_target(entry, conditions, compiler_version)
                {
                    return Some(resolved);
                }
            }
            None
        }
        serde_json::Value::Object(map) => {
            // Process keys in insertion order (Node.js spec). For each key:
            // 1. Check if it's a plain condition match
            // 2. Check if it's a versioned condition like "types@>=1"
            for (key, value) in map {
                // Check for versioned condition (e.g., "types@>=1")
                if let Some(at_pos) = key.find('@') {
                    let base_condition = &key[..at_pos];
                    let version_range = &key[at_pos + 1..];
                    if conditions.contains(&base_condition)
                        && types_versions_range_matches(version_range, compiler_version)
                        && let Some(resolved) =
                            resolve_exports_target(value, conditions, compiler_version)
                    {
                        return Some(resolved);
                    }
                } else if conditions.contains(&key.as_str())
                    && let Some(resolved) =
                        resolve_exports_target(value, conditions, compiler_version)
                {
                    return Some(resolved);
                }
            }
            None
        }
        _ => None,
    }
}

pub(crate) fn resolve_exports_target_candidates(
    target: &serde_json::Value,
    conditions: &[&str],
    compiler_version: SemVer,
) -> Vec<String> {
    match target {
        serde_json::Value::String(value) => vec![value.clone()],
        serde_json::Value::Array(list) => {
            let mut candidates = Vec::new();
            for entry in list {
                candidates.extend(resolve_exports_target_candidates(
                    entry,
                    conditions,
                    compiler_version,
                ));
            }
            candidates
        }
        serde_json::Value::Object(map) => {
            let mut candidates = Vec::new();
            for (key, value) in map {
                if let Some(at_pos) = key.find('@') {
                    let base_condition = &key[..at_pos];
                    let version_range = &key[at_pos + 1..];
                    if conditions.contains(&base_condition)
                        && types_versions_range_matches(version_range, compiler_version)
                    {
                        if value.is_null() {
                            return Vec::new();
                        }
                        candidates.extend(resolve_exports_target_candidates(
                            value,
                            conditions,
                            compiler_version,
                        ));
                    }
                } else if conditions.contains(&key.as_str()) {
                    if value.is_null() {
                        return Vec::new();
                    }
                    candidates.extend(resolve_exports_target_candidates(
                        value,
                        conditions,
                        compiler_version,
                    ));
                }
            }
            candidates
        }
        _ => Vec::new(),
    }
}

pub(crate) fn resolve_imports_subpath_candidates(
    imports: &serde_json::Value,
    subpath_key: &str,
    conditions: &[&str],
    compiler_version: SemVer,
) -> Vec<String> {
    let serde_json::Value::Object(map) = imports else {
        return Vec::new();
    };

    let has_subpath_keys = map.keys().any(|key| key.starts_with('#'));
    if !has_subpath_keys {
        return Vec::new();
    }

    if let Some(value) = map.get(subpath_key) {
        return resolve_exports_target_candidates(value, conditions, compiler_version);
    }

    if let Some((wildcard, value)) =
        find_best_subpath_pattern(map, |key| match_imports_subpath(key, subpath_key))
    {
        return resolve_exports_target_candidates(value, conditions, compiler_version)
            .into_iter()
            .map(|target| apply_exports_subpath(&target, &wildcard))
            .collect();
    }

    Vec::new()
}

/// `(prefix_len, suffix_len)` specificity for a package.json `exports` /
/// `imports` subpath pattern, per Node.js `PACKAGE_IMPORTS_EXPORTS_RESOLVE`.
/// Longer prefix beats shorter prefix; longer suffix only breaks
/// equal-prefix ties. Mirrors `tsz-core`'s `export_pattern_specificity`.
fn exports_subpath_specificity(pattern: &str) -> (usize, usize) {
    if let Some(star_pos) = pattern.find('*') {
        (star_pos, pattern.len() - star_pos - 1)
    } else {
        (pattern.len(), 0)
    }
}

/// Pick the most-specific pattern entry from `map`. `match_fn` accepts the
/// captured wildcard portion for a matching key (or returns `None`). Updates
/// only on strict improvement, so true ties resolve to the first matching
/// pattern in JSON insertion order (`serde_json` is built with
/// `preserve_order`).
fn find_best_subpath_pattern<'a>(
    map: &'a serde_json::Map<String, serde_json::Value>,
    match_fn: impl Fn(&str) -> Option<String>,
) -> Option<(String, &'a serde_json::Value)> {
    let mut best: Option<(String, &'a serde_json::Value)> = None;
    let mut best_score: Option<(usize, usize)> = None;
    for (key, value) in map {
        let Some(wildcard) = match_fn(key) else {
            continue;
        };
        let specificity = exports_subpath_specificity(key);
        if best_score.is_none_or(|s| specificity > s) {
            best_score = Some(specificity);
            best = Some((wildcard, value));
        }
    }
    best
}

pub(crate) fn match_exports_subpath(pattern: &str, subpath_key: &str) -> Option<String> {
    let pattern_inner = pattern.strip_prefix("./")?;
    let subpath = subpath_key.strip_prefix("./")?;

    // A bare "./" exports entry only exposes explicit file-like subpaths such as
    // "./index.js". It should not manufacture extensionless package subpaths like
    // "inner/other" that tsc still rejects with TS2307.
    if pattern == "./" {
        let has_explicit_extension = Path::new(subpath)
            .extension()
            .is_some_and(|ext| !ext.is_empty());
        return has_explicit_extension.then(|| subpath.to_string());
    }

    // Handle deprecated trailing-slash directory patterns like "./dir/".
    if !pattern_inner.is_empty() && pattern_inner.ends_with('/') && !pattern.contains('*') {
        if let Some(rest) = subpath.strip_prefix(pattern_inner) {
            return Some(rest.to_string());
        }
        return None;
    }

    if !pattern.contains('*') {
        return None;
    }

    let star = pattern_inner.find('*')?;
    let (prefix, suffix) = pattern_inner.split_at(star);
    let suffix = &suffix[1..];

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

pub(crate) fn match_imports_subpath(pattern: &str, subpath_key: &str) -> Option<String> {
    if !pattern.contains('*') {
        return None;
    }
    let pattern = pattern.strip_prefix('#')?;
    let subpath = subpath_key.strip_prefix('#')?;

    let star = pattern.find('*')?;
    let (prefix, suffix) = pattern.split_at(star);
    let suffix = &suffix[1..];

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

pub(crate) fn apply_exports_subpath(target: &str, wildcard: &str) -> String {
    if target.contains('*') {
        target.replacen('*', wildcard, 1)
    } else if target.ends_with('/') {
        // Trailing-slash directory pattern: append the matched portion
        format!("{target}{wildcard}")
    } else {
        target.to_string()
    }
}

/// Apply tsc's `type: "json"` import-attribute escape hatch to a TS2732 outcome.
///
/// When a module lookup fails with TS2732 (`resolveJsonModule` not enabled for
/// this specifier) but the import carries a `with { type: "json" }` attribute,
/// the attribute itself enables the JSON module under Node18+/NodeNext import
/// conditions. In that case tsc re-resolves the specifier and, if it lands on a
/// `.json` file, treats the import as resolved instead of erroring.
///
/// `has_type_json_import_attribute` is computed by the caller (the two call
/// sites obtain it differently — one precomputes, one reads it from the
/// specifier node) so this helper only owns the shared TS2732 override that was
/// previously duplicated verbatim across the source-discovery and
/// source-resolution-setup paths.
pub(crate) fn apply_json_type_import_attribute_override(
    outcome: &mut tsz::module_resolver::ModuleLookupOutcome,
    has_type_json_import_attribute: bool,
    containing_file: &Path,
    specifier: &str,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    resolution_cache: &mut ModuleResolutionCache,
    known_files: &rustc_hash::FxHashSet<PathBuf>,
) {
    if outcome
        .error
        .as_ref()
        .is_some_and(|error| error.code == 2732)
        && has_type_json_import_attribute
        && json_type_attribute_enables_json_module(
            options,
            containing_file,
            base_dir,
            resolution_cache,
        )
        && let Some(resolved_path) = resolve_module_specifier(
            containing_file,
            specifier,
            options,
            base_dir,
            resolution_cache,
            known_files,
        )
        && resolved_path.extension().is_some_and(|ext| ext == "json")
    {
        outcome.resolved_path = Some(resolved_path);
        outcome.is_resolved = true;
        outcome.error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_VERSION: SemVer = SemVer {
        major: 5,
        minor: 4,
        patch: 0,
    };

    #[test]
    fn exports_subpath_specificity_uses_prefix_then_suffix_tuple() {
        // Covers each branch of the algorithm plus the regression cases:
        //   * exact/non-wildcard keys score `(pattern.len(), 0)`,
        //   * patterns with `*` score `(prefix_len, suffix_len)` and so the
        //     equal-total-length / unequal-prefix pair `./abc/*` vs `./*/abc`
        //     resolves strictly (the previous `key.len()` heuristic tied them),
        //   * identical-prefix patterns are broken by suffix length.
        assert_eq!(exports_subpath_specificity("./exact.js"), (10, 0));
        assert_eq!(exports_subpath_specificity("./"), (2, 0));
        assert_eq!(exports_subpath_specificity("./*"), (2, 0));
        assert_eq!(exports_subpath_specificity("./prefix*"), (8, 0));
        assert_eq!(exports_subpath_specificity("./*.ts"), (2, 3));
        assert_eq!(exports_subpath_specificity("./abc/*"), (6, 0));
        assert_eq!(exports_subpath_specificity("./*/abc"), (2, 4));
        assert!(exports_subpath_specificity("./abc/*") > exports_subpath_specificity("./*/abc"));
        assert!(
            exports_subpath_specificity("./lib/*.d.ts") > exports_subpath_specificity("./lib/*")
        );
    }

    #[test]
    fn resolve_exports_subpath_uses_prefix_specificity_not_total_length() {
        // Regression: with the old `key.len()` rule, `./abc/*` (length 7) and
        // `./*/abc` (length 7) tied, so the JSON ordering decided who won.
        // With the `(prefix_len, suffix_len)` tuple, `./abc/*` (prefix 6) is
        // always the strict winner for `./abc/abc` regardless of JSON order.
        for exports in [
            serde_json::json!({
                "./*/abc": "./by-star-abc.js",
                "./abc/*": "./by-abc-star.js"
            }),
            serde_json::json!({
                "./abc/*": "./by-abc-star.js",
                "./*/abc": "./by-star-abc.js"
            }),
        ] {
            assert_eq!(
                resolve_exports_subpath(&exports, "./abc/abc", &["default"], TEST_VERSION)
                    .as_deref(),
                Some("./by-abc-star.js"),
            );
        }
    }

    #[test]
    fn resolve_exports_subpath_true_ties_resolve_to_first_in_json_order() {
        // When the `(prefix_len, suffix_len)` tuples are truly equal, the spec
        // says first-in-source-order wins. `serde_json` is built with
        // `preserve_order`, so iteration follows the JSON authoring order.
        let exports = serde_json::json!({
            "./a/*": "./first.js",
            "./b/*": "./second.js"
        });
        assert_eq!(
            resolve_exports_subpath(&exports, "./a/x", &["default"], TEST_VERSION).as_deref(),
            Some("./first.js"),
        );

        let exports_reversed = serde_json::json!({
            "./b/*": "./second.js",
            "./a/*": "./first.js"
        });
        assert_eq!(
            resolve_exports_subpath(&exports_reversed, "./b/x", &["default"], TEST_VERSION)
                .as_deref(),
            Some("./second.js"),
        );
    }

    // NOTE: `select_paths`/`parse_pattern`/version-range selection are owned and
    // unit-tested by `tsz_common::module_resolution::types_versions`. The CLI
    // keeps the end-to-end `resolve_types_versions` coverage in the driver
    // integration tests; only the re-exported `types_versions_range_matches`
    // (used by versioned export conditions) is smoke-tested here.
    #[test]
    fn types_versions_range_matches_handles_star_empty_and_disjunctions() {
        assert!(types_versions_range_matches("*", TEST_VERSION));
        assert!(types_versions_range_matches("", TEST_VERSION));
        assert!(types_versions_range_matches(">=4 <6", TEST_VERSION));
        assert!(!types_versions_range_matches(">=6", TEST_VERSION));
        assert!(types_versions_range_matches(">=6 || <=5.4", TEST_VERSION));
    }

    #[test]
    fn resolve_imports_subpath_uses_prefix_specificity_not_total_length() {
        // Same regression as exports, on the `#`-prefixed imports field.
        for imports in [
            serde_json::json!({
                "#*/abc": "./by-star-abc.js",
                "#abc/*": "./by-abc-star.js"
            }),
            serde_json::json!({
                "#abc/*": "./by-abc-star.js",
                "#*/abc": "./by-star-abc.js"
            }),
        ] {
            assert_eq!(
                resolve_imports_subpath_candidates(
                    &imports,
                    "#abc/abc",
                    &["default"],
                    TEST_VERSION,
                ),
                vec!["./by-abc-star.js".to_string()],
            );
        }
    }
}
