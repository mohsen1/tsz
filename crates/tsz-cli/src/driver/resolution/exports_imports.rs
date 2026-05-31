use std::path::{Path, PathBuf};

use crate::config::ResolvedCompilerOptions;
use tsz::module_resolver::PackageType;

#[allow(unused_imports)]
use super::*;

/// Resolve a `typesVersions` paths object against a subpath, mirroring tsc's
/// `matchPatternOrExact` + `findBestPatternMatch` chain (see the matching
/// `tsz-core` resolver for the full rationale). The CLI driver has its own
/// resolver because it composes with `resolve_package_entry`, but the
/// algorithm must stay byte-for-byte aligned with tsc:
///
/// 1. A no-`*` key that equals `subpath` exactly wins outright.
/// 2. Otherwise, among single-`*` wildcard keys, the longest **prefix** wins
///    (ties resolved by first occurrence in declaration order).
/// 3. Two-or-more-`*` keys are skipped entirely.
pub(crate) fn resolve_types_versions(
    package_root: &Path,
    subpath: &str,
    types_versions: &serde_json::Value,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
    resolution_cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    let compiler_version = types_versions_compiler_version(options);
    let paths = select_types_versions_paths(types_versions, compiler_version)?;

    // 1) Exact-match short-circuit (`matchableStringSet.has(candidate)`).
    for (key, value) in paths {
        if !key.contains('*') && key == subpath {
            return apply_types_versions_targets(
                package_root,
                value,
                "",
                options,
                package_type,
                resolution_cache,
            );
        }
    }

    // 2) Wildcard candidates: longest prefix wins, ties → first in order.
    //    `best` stores `(prefix_len, value, captured wildcard)`; the
    //    prefix_len lives inside the tuple so we keep a single source of
    //    truth for "the current best prefix length".
    let mut best: Option<(usize, &serde_json::Value, String)> = None;
    for (key, value) in paths {
        let Some((prefix, suffix)) = parse_types_versions_pattern(key) else {
            continue;
        };
        if !subpath.starts_with(prefix) || !subpath.ends_with(suffix) {
            continue;
        }
        let start = prefix.len();
        let end = subpath.len() - suffix.len();
        if end < start {
            continue;
        }
        // Strict `>` so equal-prefix ties keep the earlier entry (tsc's
        // `findBestPatternMatch`).
        if best
            .as_ref()
            .is_some_and(|(best_len, ..)| prefix.len() <= *best_len)
        {
            continue;
        }
        best = Some((prefix.len(), value, subpath[start..end].to_string()));
    }

    let (_, value, wildcard) = best?;
    apply_types_versions_targets(
        package_root,
        value,
        &wildcard,
        options,
        package_type,
        resolution_cache,
    )
}

/// Iterate the `value` of a `typesVersions` paths entry, substitute `*` with
/// `wildcard`, and return the first target that resolves on disk.
fn apply_types_versions_targets(
    package_root: &Path,
    value: &serde_json::Value,
    wildcard: &str,
    options: &ResolvedCompilerOptions,
    package_type: Option<PackageType>,
    resolution_cache: &mut ModuleResolutionCache,
) -> Option<PathBuf> {
    let mut try_target = |target: &str| {
        let substituted = substitute_path_target(target, wildcard);
        resolve_package_entry(
            package_root,
            &substituted,
            options,
            package_type,
            resolution_cache,
        )
    };
    match value {
        serde_json::Value::String(target) => try_target(target.as_str()),
        serde_json::Value::Array(list) => list
            .iter()
            .filter_map(serde_json::Value::as_str)
            .find_map(try_target),
        _ => None,
    }
}

/// First-match-in-declaration-order version selection (tsc's
/// `getPackageJsonTypesVersionsPaths`).
pub(crate) fn select_types_versions_paths(
    types_versions: &serde_json::Value,
    compiler_version: SemVer,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    let map = types_versions.as_object()?;
    for (key, value) in map {
        let Some(value_map) = value.as_object() else {
            continue;
        };
        if types_versions_range_matches(key, compiler_version) {
            return Some(value_map);
        }
    }
    None
}

/// Split a `prefix*suffix` typesVersions pattern, mirroring tsc's
/// `tryParsePattern`. Returns `None` for no-`*` patterns and for multi-`*`
/// patterns.
pub(crate) fn parse_types_versions_pattern(pattern: &str) -> Option<(&str, &str)> {
    let star_pos = pattern.find('*')?;
    let suffix_start = star_pos + 1;
    if pattern[suffix_start..].contains('*') {
        return None;
    }
    Some((&pattern[..star_pos], &pattern[suffix_start..]))
}

/// Returns `true` when `range` is a valid semver range that the supplied
/// compiler version satisfies.
pub(crate) fn types_versions_range_matches(range: &str, compiler_version: SemVer) -> bool {
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return true;
    }
    for segment in range.split("||") {
        if types_versions_range_segment_matches(segment.trim(), compiler_version) {
            return true;
        }
    }
    false
}

fn types_versions_range_segment_matches(segment: &str, compiler_version: SemVer) -> bool {
    // An empty segment comes from a malformed disjunction like `">=4 || "` —
    // a vacuous empty-token loop would return `true`, so we reject explicitly.
    // The lone `"*"` token is handled by the `continue` below; no early
    // return needed.
    if segment.is_empty() {
        return false;
    }
    for token in segment.split_whitespace() {
        if token.is_empty() || token == "*" {
            continue;
        }
        let Some((op, version)) = parse_range_token(token) else {
            return false;
        };
        if !compare_range(compiler_version, op, version) {
            return false;
        }
    }
    true
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RangeOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

pub(crate) fn parse_range_token(token: &str) -> Option<(RangeOp, SemVer)> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }

    let (op, rest) = if let Some(rest) = token.strip_prefix(">=") {
        (RangeOp::Gte, rest)
    } else if let Some(rest) = token.strip_prefix("<=") {
        (RangeOp::Lte, rest)
    } else if let Some(rest) = token.strip_prefix('>') {
        (RangeOp::Gt, rest)
    } else if let Some(rest) = token.strip_prefix('<') {
        (RangeOp::Lt, rest)
    } else if let Some(rest) = token.strip_prefix('=') {
        (RangeOp::Eq, rest)
    } else {
        (RangeOp::Eq, token)
    };

    parse_semver(rest).map(|version| (op, version))
}

pub(crate) fn compare_range(version: SemVer, op: RangeOp, bound: SemVer) -> bool {
    match op {
        RangeOp::Gt => version > bound,
        RangeOp::Gte => version >= bound,
        RangeOp::Lt => version < bound,
        RangeOp::Lte => version <= bound,
        RangeOp::Eq => version == bound,
    }
}

pub(crate) fn parse_semver(value: &str) -> Option<SemVer> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let core = value.split(['-', '+']).next().unwrap_or(value);
    let mut parts = core.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next().unwrap_or("0").parse().ok()?;
    let patch: u32 = parts.next().unwrap_or("0").parse().ok()?;
    Some(SemVer {
        major,
        minor,
        patch,
    })
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

    #[test]
    fn select_types_versions_paths_returns_first_matching_key_in_declaration_order() {
        // Pinned against tsc's `getPackageJsonTypesVersionsPaths` (first-match
        // semantics). With `"*"` declared first, every later key is
        // unreachable.
        let types_versions = serde_json::json!({
            "*": { "*": ["fallback/index.d.ts"] },
            ">=5.4": { "*": ["modern/index.d.ts"] }
        });

        let selected = select_types_versions_paths(&types_versions, TEST_VERSION)
            .expect("expected a matching typesVersions entry");
        assert_eq!(
            selected.get("*"),
            Some(&serde_json::json!(["fallback/index.d.ts"]))
        );

        // Natural ordering — fallback last — picks the tighter range.
        let natural = serde_json::json!({
            ">=5.4": { "*": ["modern/index.d.ts"] },
            "*": { "*": ["fallback/index.d.ts"] }
        });
        let selected_natural = select_types_versions_paths(&natural, TEST_VERSION)
            .expect("expected a matching typesVersions entry");
        assert_eq!(
            selected_natural.get("*"),
            Some(&serde_json::json!(["modern/index.d.ts"])),
        );
    }

    #[test]
    fn parse_types_versions_pattern_rejects_multi_star_keys() {
        assert_eq!(parse_types_versions_pattern("lib/*"), Some(("lib/", "")));
        assert_eq!(parse_types_versions_pattern("*.d.ts"), Some(("", ".d.ts")));
        assert_eq!(parse_types_versions_pattern("a*b*c"), None);
        assert_eq!(parse_types_versions_pattern("exact"), None);
    }

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
