use std::path::{Path, PathBuf};

use crate::config::ResolvedCompilerOptions;
use tsz::module_resolver::PackageType;

#[allow(unused_imports)]
use super::*;

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
    let mut best_pattern: Option<&String> = None;
    let mut best_value: Option<&serde_json::Value> = None;
    let mut best_wildcard = String::new();
    let mut best_specificity = 0usize;
    let mut best_len = 0usize;

    for (pattern, value) in paths {
        let Some(wildcard) = match_types_versions_pattern(pattern, subpath) else {
            continue;
        };
        let specificity = types_versions_specificity(pattern);
        let pattern_len = pattern.len();
        let is_better = match best_pattern {
            None => true,
            Some(current) => {
                specificity > best_specificity
                    || (specificity == best_specificity && pattern_len > best_len)
                    || (specificity == best_specificity
                        && pattern_len == best_len
                        && pattern < current)
            }
        };

        if is_better {
            best_specificity = specificity;
            best_len = pattern_len;
            best_pattern = Some(pattern);
            best_value = Some(value);
            best_wildcard = wildcard;
        }
    }

    let value = best_value?;

    let mut targets = Vec::new();
    match value {
        serde_json::Value::String(value) => targets.push(value.as_str()),
        serde_json::Value::Array(list) => {
            for entry in list {
                if let Some(value) = entry.as_str() {
                    targets.push(value);
                }
            }
        }
        _ => {}
    }

    for target in targets {
        let substituted = substitute_path_target(target, &best_wildcard);
        if let Some(resolved) = resolve_package_entry(
            package_root,
            &substituted,
            options,
            package_type,
            resolution_cache,
        ) {
            return Some(resolved);
        }
    }

    None
}

pub(crate) fn select_types_versions_paths(
    types_versions: &serde_json::Value,
    compiler_version: SemVer,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    select_types_versions_paths_for_version(types_versions, compiler_version)
}

pub(crate) fn select_types_versions_paths_for_version(
    types_versions: &serde_json::Value,
    compiler_version: SemVer,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    let map = types_versions.as_object()?;
    let mut best_score: Option<RangeScore> = None;
    let mut best_key: Option<&str> = None;
    let mut best_value: Option<&serde_json::Map<String, serde_json::Value>> = None;

    for (key, value) in map {
        let Some(value_map) = value.as_object() else {
            continue;
        };
        let Some(score) = match_types_versions_range(key, compiler_version) else {
            continue;
        };
        let is_better = match best_score {
            None => true,
            Some(best) => {
                score > best
                    || (score == best && best_key.is_none_or(|best_key| key.as_str() < best_key))
            }
        };

        if is_better {
            best_score = Some(score);
            best_key = Some(key);
            best_value = Some(value_map);
        }
    }

    best_value
}

pub(crate) fn match_types_versions_pattern(pattern: &str, subpath: &str) -> Option<String> {
    if !pattern.contains('*') {
        return (pattern == subpath).then(String::new);
    }

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

pub(crate) fn types_versions_specificity(pattern: &str) -> usize {
    if let Some(star) = pattern.find('*') {
        star + (pattern.len() - star - 1)
    } else {
        pattern.len()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct RangeScore {
    pub(super) constraints: usize,
    pub(super) min_version: SemVer,
    pub(super) key_len: usize,
}

pub(crate) fn match_types_versions_range(
    range: &str,
    compiler_version: SemVer,
) -> Option<RangeScore> {
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return Some(RangeScore {
            constraints: 0,
            min_version: SemVer::ZERO,
            key_len: range.len(),
        });
    }

    let mut best: Option<RangeScore> = None;
    for segment in range.split("||") {
        let segment = segment.trim();
        let Some(score) =
            match_types_versions_range_segment(segment, compiler_version, range.len())
        else {
            continue;
        };
        if best.is_none_or(|current| score > current) {
            best = Some(score);
        }
    }

    best
}

pub(crate) fn match_types_versions_range_segment(
    segment: &str,
    compiler_version: SemVer,
    key_len: usize,
) -> Option<RangeScore> {
    if segment.is_empty() {
        return None;
    }
    if segment == "*" {
        return Some(RangeScore {
            constraints: 0,
            min_version: SemVer::ZERO,
            key_len,
        });
    }

    let mut min_version = SemVer::ZERO;
    let mut constraints = 0usize;

    for token in segment.split_whitespace() {
        if token.is_empty() || token == "*" {
            continue;
        }
        let (op, version) = parse_range_token(token)?;
        if !compare_range(compiler_version, op, version) {
            return None;
        }
        constraints += 1;
        if matches!(op, RangeOp::Gt | RangeOp::Gte | RangeOp::Eq) && version > min_version {
            min_version = version;
        }
    }

    Some(RangeScore {
        constraints,
        min_version,
        key_len,
    })
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
                        && match_types_versions_range(version_range, compiler_version).is_some()
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
                        && match_types_versions_range(version_range, compiler_version).is_some()
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
