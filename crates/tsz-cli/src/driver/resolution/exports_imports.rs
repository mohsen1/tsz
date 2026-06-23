use std::path::{Path, PathBuf};

use crate::config::ResolvedCompilerOptions;
use tsz::module_resolver::PackageType;
use tsz_common::module_resolution::TargetMatch;
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
) -> TargetMatch<String> {
    match exports {
        serde_json::Value::String(value) => {
            if subpath_key == "." {
                TargetMatch::Resolved(value.clone())
            } else {
                TargetMatch::NotApplicable
            }
        }
        // A bare `"exports": null` blocks the package entirely.
        serde_json::Value::Null => TargetMatch::Blocked,
        serde_json::Value::Array(list) => {
            for entry in list {
                match resolve_exports_subpath(entry, subpath_key, conditions, compiler_version) {
                    TargetMatch::NotApplicable => {}
                    stop => return stop,
                }
            }
            TargetMatch::NotApplicable
        }
        serde_json::Value::Object(map) => {
            let has_subpath_keys = map.keys().any(|key| key.starts_with('.'));
            if has_subpath_keys {
                // An exact subpath key is authoritative — its result (resolved,
                // blocked, or miss) is returned without pattern fallthrough.
                if let Some(value) = map.get(subpath_key) {
                    return resolve_exports_target(value, conditions, compiler_version);
                }

                if let Some((wildcard, value)) =
                    find_best_subpath_pattern(map, |key| match_exports_subpath(key, subpath_key))
                {
                    return resolve_exports_target(value, conditions, compiler_version)
                        .map(|target| apply_exports_subpath(&target, &wildcard));
                }

                TargetMatch::NotApplicable
            } else if subpath_key == "." {
                resolve_exports_target(exports, conditions, compiler_version)
            } else {
                TargetMatch::NotApplicable
            }
        }
        _ => TargetMatch::NotApplicable,
    }
}

pub(crate) fn resolve_exports_target(
    target: &serde_json::Value,
    conditions: &[&str],
    compiler_version: SemVer,
) -> TargetMatch<String> {
    match target {
        serde_json::Value::String(value) => TargetMatch::Resolved(value.clone()),
        // An explicit JSON `null` reached through a matching condition or array
        // element blocks the whole resolution (Node `PACKAGE_TARGET_RESOLVE`):
        // it must not fall through to a sibling condition or an outer fallback.
        serde_json::Value::Null => TargetMatch::Blocked,
        serde_json::Value::Array(list) => {
            for entry in list {
                match resolve_exports_target(entry, conditions, compiler_version) {
                    TargetMatch::NotApplicable => {}
                    stop => return stop,
                }
            }
            TargetMatch::NotApplicable
        }
        serde_json::Value::Object(map) => {
            // Process keys in insertion order (Node.js spec). For each key:
            // 1. Check if it's a plain condition match
            // 2. Check if it's a versioned condition like "types@>=1"
            for (key, value) in map {
                let matched = if let Some(at_pos) = key.find('@') {
                    let base_condition = &key[..at_pos];
                    let version_range = &key[at_pos + 1..];
                    conditions.contains(&base_condition)
                        && types_versions_range_matches(version_range, compiler_version)
                } else {
                    conditions.contains(&key.as_str())
                };
                if matched {
                    // A matching condition (including one mapping to `null`)
                    // stops the search: Resolved and Blocked both short-circuit;
                    // only NotApplicable continues to the next condition.
                    match resolve_exports_target(value, conditions, compiler_version) {
                        TargetMatch::NotApplicable => {}
                        stop => return stop,
                    }
                }
            }
            TargetMatch::NotApplicable
        }
        _ => TargetMatch::NotApplicable,
    }
}

/// Resolve a package.json `imports` subpath to ordered `(target, is_types)`
/// candidates. `is_types` reports whether the matched value passed through a
/// types-flavored condition (`types`/`types@<range>`); the imports caller uses
/// it to keep declaration-aware probing of an extensionless target, mirroring
/// the spec'd `PACKAGE_TARGET_RESOLVE` algorithm shared with the `exports` field.
pub(crate) fn resolve_imports_subpath_candidates_with_flavor(
    imports: &serde_json::Value,
    subpath_key: &str,
    conditions: &[&str],
    compiler_version: SemVer,
) -> Vec<(String, bool)> {
    let serde_json::Value::Object(map) = imports else {
        return Vec::new();
    };

    let has_subpath_keys = map.keys().any(|key| key.starts_with('#'));
    if !has_subpath_keys {
        return Vec::new();
    }

    // An exact key is authoritative (no pattern fallthrough). A `null` block and
    // an empty miss both collapse to "no candidates" here — `#imports`
    // resolution has no further fallback, so both terminate as NotFound.
    if let Some(value) = map.get(subpath_key) {
        return resolve_target_candidates_with_flavor(value, conditions, compiler_version, false)
            .into_option()
            .unwrap_or_default();
    }

    if let Some((wildcard, value)) =
        find_best_subpath_pattern(map, |key| match_imports_subpath(key, subpath_key))
    {
        return resolve_target_candidates_with_flavor(value, conditions, compiler_version, false)
            .into_option()
            .unwrap_or_default()
            .into_iter()
            .map(|(target, is_types)| (apply_exports_subpath(&target, &wildcard), is_types))
            .collect();
    }

    Vec::new()
}

/// Resolve a value side of an `exports`/`imports` entry to ordered
/// `(target, is_types)` candidates. Condition keys (including versioned
/// `<base>@<range>`) are matched against `conditions`; `is_types` is set when
/// the value was reached through a `types`/`types@<range>` condition, so the
/// caller can keep declaration-aware probing of an extensionless target.
fn resolve_target_candidates_with_flavor(
    target: &serde_json::Value,
    conditions: &[&str],
    compiler_version: SemVer,
    is_types_condition: bool,
) -> TargetMatch<Vec<(String, bool)>> {
    match target {
        serde_json::Value::String(value) => {
            TargetMatch::Resolved(vec![(value.clone(), is_types_condition)])
        }
        // An explicit JSON `null` reached through a matching condition or array
        // element blocks the whole `#imports` resolution: it must not fall
        // through to a sibling condition or an outer fallback. The earlier
        // `return Vec::new()` only blocked at the exact nesting level, so a
        // *nested* null still let the enclosing conditional pick a later
        // sibling — the bug this `Blocked` propagation fixes.
        serde_json::Value::Null => TargetMatch::Blocked,
        serde_json::Value::Array(list) => {
            // Disk probing is deferred to the caller, so matching targets are
            // accumulated in order (the caller probes them in order). A matching
            // `null` short-circuits the whole collection.
            let mut candidates = Vec::new();
            for entry in list {
                match resolve_target_candidates_with_flavor(
                    entry,
                    conditions,
                    compiler_version,
                    is_types_condition,
                ) {
                    TargetMatch::Resolved(found) => candidates.extend(found),
                    TargetMatch::Blocked => return TargetMatch::Blocked,
                    TargetMatch::NotApplicable => {}
                }
            }
            TargetMatch::from_candidates(candidates)
        }
        serde_json::Value::Object(map) => {
            let mut candidates = Vec::new();
            for (key, value) in map {
                let (matched, nested_is_types) = if let Some(at_pos) = key.find('@') {
                    let base_condition = &key[..at_pos];
                    let version_range = &key[at_pos + 1..];
                    let matched = conditions.contains(&base_condition)
                        && types_versions_range_matches(version_range, compiler_version);
                    (matched, is_types_condition || base_condition == "types")
                } else {
                    (
                        conditions.contains(&key.as_str()),
                        is_types_condition || key == "types",
                    )
                };
                if matched {
                    match resolve_target_candidates_with_flavor(
                        value,
                        conditions,
                        compiler_version,
                        nested_is_types,
                    ) {
                        TargetMatch::Resolved(found) => candidates.extend(found),
                        TargetMatch::Blocked => return TargetMatch::Blocked,
                        TargetMatch::NotApplicable => {}
                    }
                }
            }
            TargetMatch::from_candidates(candidates)
        }
        _ => TargetMatch::NotApplicable,
    }
}

/// Pick the most-specific pattern entry from `map`. `match_fn` accepts the
/// captured wildcard portion for a matching key (or returns `None`). Updates
/// only on strict improvement, so true ties resolve to the first matching
/// pattern in JSON insertion order (`serde_json` is built with
/// `preserve_order`).
///
/// Specificity is ranked by the shared
/// [`pattern_key_specificity`](tsz_common::module_resolution::package_exports::pattern_key_specificity)
/// comparator (Node.js `comparePatternKeys`), so this driver cannot drift from
/// the tsz-core resolver or from `tsc`. The earlier `(prefix_len, suffix_len)`
/// 2-tuple dropped the `+1` on a wildcard's base length and the wildcard-beats-
/// directory tiebreak, so a directory key (`"./"`, `"./lib/"`) tied its
/// corresponding wildcard (`"./*"`, `"./lib/*"`) and the chosen file flipped
/// with JSON key order.
fn find_best_subpath_pattern<'a>(
    map: &'a serde_json::Map<String, serde_json::Value>,
    match_fn: impl Fn(&str) -> Option<String>,
) -> Option<(String, &'a serde_json::Value)> {
    use tsz_common::module_resolution::package_exports::pattern_key_specificity;

    let mut best: Option<(String, &'a serde_json::Value)> = None;
    let mut best_score: Option<(usize, usize, usize)> = None;
    for (key, value) in map {
        let Some(wildcard) = match_fn(key) else {
            continue;
        };
        let specificity = pattern_key_specificity(key);
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
        // Node `PACKAGE_TARGET_RESOLVE` substitutes the captured subpath into
        // EVERY `*` in the target (tsc's `resolvedTarget.replace(/\*/g, subpath)`),
        // not just the first. A target with two or more `*` (e.g.
        // `"./*": "./dist/*/*.js"`) must not strand a literal `*`, which would
        // never resolve on disk and produce a spurious TS2307. This mirrors the
        // tsz-core resolver's `apply_wildcard_substitution`; the two exports/
        // imports substitution chokepoints must stay in agreement. (tsconfig
        // `paths`/`typesVersions` substitution is a separate Node-spec concern
        // that replaces only the first `*` and is handled elsewhere.)
        target.replace('*', wildcard)
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

    // --- JSON-`null` target blocking (Node `PACKAGE_TARGET_RESOLVE`) ---------
    //
    // A `null` reached through a *matching* condition / array element / exact
    // subpath key blocks the whole resolution; it must not fall through to a
    // sibling. A `null` on an UNMATCHED condition is never reached. Verified
    // against bundled `tsc` 6.0.2. CommonJS conditions used here:
    // `["require", "types", "node", "default"]`.
    const CJS_CONDS: &[&str] = &["require", "types", "node", "default"];

    #[test]
    fn exports_target_nested_matching_null_blocks_outer_default() {
        // node -> require:null blocks; neither the inner `default` nor the outer
        // `default` is reached.
        let exports = serde_json::json!({
            "node": { "require": null, "default": "./inner.js" },
            "default": "./outer.js"
        });
        assert_eq!(
            resolve_exports_target(&exports, CJS_CONDS, TEST_VERSION),
            TargetMatch::Blocked,
        );
    }

    #[test]
    fn exports_target_top_level_matching_null_blocks_sibling() {
        let exports = serde_json::json!({ "node": null, "default": "./fallback.js" });
        assert_eq!(
            resolve_exports_target(&exports, CJS_CONDS, TEST_VERSION),
            TargetMatch::Blocked,
        );
    }

    #[test]
    fn exports_target_null_on_unmatched_condition_resolves_default() {
        // `import` is not a CommonJS condition, so its null is never reached.
        let exports = serde_json::json!({ "import": null, "default": "./present.js" });
        assert_eq!(
            resolve_exports_target(&exports, CJS_CONDS, TEST_VERSION),
            TargetMatch::Resolved("./present.js".to_string()),
        );
    }

    #[test]
    fn exports_target_null_array_element_blocks_remaining() {
        let exports = serde_json::json!([null, "./real.js"]);
        assert_eq!(
            resolve_exports_target(&exports, CJS_CONDS, TEST_VERSION),
            TargetMatch::Blocked,
        );
    }

    #[test]
    fn exports_subpath_exact_null_key_blocks_without_pattern_fallthrough() {
        // `"./blocked": null` blocks even though `"./*"` would otherwise match;
        // a different subpath still resolves through the wildcard.
        let exports = serde_json::json!({ "./blocked": null, "./*": "./impl/*.js" });
        assert_eq!(
            resolve_exports_subpath(&exports, "./blocked", &["default"], TEST_VERSION),
            TargetMatch::Blocked,
        );
        assert_eq!(
            resolve_exports_subpath(&exports, "./allowed", &["default"], TEST_VERSION)
                .into_option()
                .as_deref(),
            Some("./impl/allowed.js"),
        );
    }

    #[test]
    fn imports_candidates_nested_matching_null_blocks_and_yields_no_candidates() {
        // The `#imports` twin: a nested matching null blocks the collection, so
        // the outer `default` is never accumulated and the import is unresolved.
        let imports = serde_json::json!({
            "#feature": {
                "node": { "require": null, "default": "./inner.js" },
                "default": "./outer.js"
            }
        });
        assert!(
            resolve_imports_subpath_candidates_with_flavor(
                &imports,
                "#feature",
                CJS_CONDS,
                TEST_VERSION,
            )
            .is_empty(),
            "nested matching null must yield no #imports candidates"
        );
    }

    #[test]
    fn imports_candidates_null_on_unmatched_condition_resolves_default() {
        let imports = serde_json::json!({
            "#feature": { "import": null, "default": "./present.js" }
        });
        assert_eq!(
            resolve_imports_subpath_candidates_with_flavor(
                &imports,
                "#feature",
                CJS_CONDS,
                TEST_VERSION,
            ),
            vec![("./present.js".to_string(), false)],
        );
    }

    // The `pattern_key_specificity` comparator itself is unit-tested in
    // `tsz_common::module_resolution::package_exports`; the driver tests below
    // pin that the resolver actually routes its key selection through it
    // (end-to-end, in either JSON authoring order).

    #[test]
    fn resolve_exports_subpath_wildcard_beats_directory_key_in_either_order() {
        // Regression: a `*` key is strictly more specific than the trailing-slash
        // directory key it shares a base with, so `tsc` always picks the wildcard
        // target regardless of JSON authoring order. The prior 2-tuple comparator
        // tied them and let JSON order pick the (wrong) directory target.
        for exports in [
            serde_json::json!({ "./lib/": "./d/", "./lib/*": "./s/*" }),
            serde_json::json!({ "./lib/*": "./s/*", "./lib/": "./d/" }),
        ] {
            assert_eq!(
                resolve_exports_subpath(&exports, "./lib/foo.js", &["default"], TEST_VERSION)
                    .into_option()
                    .as_deref(),
                Some("./s/foo.js"),
            );
        }

        // The bare-`"./"` directory sugar vs `"./*"` shows the same fix.
        for exports in [
            serde_json::json!({ "./": "./pub/", "./*": "./src/*" }),
            serde_json::json!({ "./*": "./src/*", "./": "./pub/" }),
        ] {
            assert_eq!(
                resolve_exports_subpath(&exports, "./foo.js", &["default"], TEST_VERSION)
                    .into_option()
                    .as_deref(),
                Some("./src/foo.js"),
            );
        }
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
                    .into_option()
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
            resolve_exports_subpath(&exports, "./a/x", &["default"], TEST_VERSION)
                .into_option()
                .as_deref(),
            Some("./first.js"),
        );

        let exports_reversed = serde_json::json!({
            "./b/*": "./second.js",
            "./a/*": "./first.js"
        });
        assert_eq!(
            resolve_exports_subpath(&exports_reversed, "./b/x", &["default"], TEST_VERSION)
                .into_option()
                .as_deref(),
            Some("./second.js"),
        );
    }

    #[test]
    fn apply_exports_subpath_replaces_every_star_in_target() {
        // Node `PACKAGE_TARGET_RESOLVE` / tsc `replace(/\*/g, subpath)`: every
        // `*` in the target is substituted with the captured subpath, matching
        // the tsz-core resolver's `apply_wildcard_substitution`. The prior
        // first-`*`-only substitution stranded a literal `*` (→ spurious TS2307).
        assert_eq!(
            apply_exports_subpath("./dist/*/*.js", "button"),
            "./dist/button/button.js"
        );
        assert_eq!(apply_exports_subpath("./*/*/*.d.ts", "a"), "./a/a/a.d.ts");
        // Single-star, no-star, and trailing-slash directory targets are
        // unaffected by the fix.
        assert_eq!(
            apply_exports_subpath("./dist/*.js", "index"),
            "./dist/index.js"
        );
        assert_eq!(
            apply_exports_subpath("./dist/index.js", "x"),
            "./dist/index.js"
        );
        assert_eq!(apply_exports_subpath("./lib/", "sub/mod"), "./lib/sub/mod");
    }

    #[test]
    fn resolve_exports_subpath_substitutes_every_star_end_to_end() {
        // The replace-all rule must hold through the driver's exports resolver,
        // not just the leaf helper: a single-`*` KEY mapping to a multi-`*`
        // TARGET resolves with no literal `*` left behind.
        let exports = serde_json::json!({ "./*": "./dist/*/*.js" });
        assert_eq!(
            resolve_exports_subpath(&exports, "./button", &["default"], TEST_VERSION)
                .into_option()
                .as_deref(),
            Some("./dist/button/button.js"),
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
                resolve_imports_subpath_candidates_with_flavor(
                    &imports,
                    "#abc/abc",
                    &["default"],
                    TEST_VERSION,
                ),
                vec![("./by-abc-star.js".to_string(), false)],
            );
        }
    }
}
