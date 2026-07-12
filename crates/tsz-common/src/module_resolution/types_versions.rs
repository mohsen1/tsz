//! `typesVersions` resolution primitives, shared by the CLI driver resolver
//! and the checker's post-resolution redirect.
//!
//! This is the single source of truth for how `typesVersions` branch selection
//! mirrors TypeScript's `getPackageJsonTypesVersionsPaths` +
//! `matchPatternOrExact`/`findBestPatternMatch` chain. Keeping the algorithm
//! in one place avoids the historical drift where the checker copy ignored
//! version-range matching while the driver copy honored it.
//!
//! tsc semantics, in order:
//! 1. Pick the **first** `typesVersions` key, in package.json declaration
//!    order, whose semver range matches the active compiler version.
//! 2. Within that single entry's path map, an exact (no-`*`) key equal to the
//!    subpath wins outright.
//! 3. Otherwise, among single-`*` wildcard keys, the longest **prefix** wins
//!    (ties resolved by first occurrence in declaration order).
//! 4. Two-or-more-`*` keys are skipped entirely.

use serde_json::{Map, Value};

/// A `major.minor.patch` semantic version. Pre-release / build metadata is
/// dropped during parsing, matching tsc's `VersionRange` comparison core.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

/// The TypeScript version this compiler targets for `typesVersions` range
/// resolution.
///
/// NOTE: Keep this in sync with the TypeScript version this compiler targets
/// (`scripts/conformance/typescript-versions.json`).
pub const DEFAULT_COMPILER_VERSION: SemVer = SemVer {
    major: 7,
    minor: 0,
    patch: 2,
};

/// Parse a `major[.minor[.patch]]` version, ignoring any `-pre`/`+build`
/// suffix. Missing minor/patch default to `0`.
pub fn parse_semver(value: &str) -> Option<SemVer> {
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

/// A comparison operator parsed from a `typesVersions` range token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RangeOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

/// Parse a single range token such as `>=6.0`, `<7.0`, `=5`, or a bare `4.2`
/// (treated as an exact match, mirroring tsc's primitive comparator parsing).
fn parse_range_token(token: &str) -> Option<(RangeOp, SemVer)> {
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

/// Apply a single comparison operator.
fn compare_range(version: SemVer, op: RangeOp, bound: SemVer) -> bool {
    match op {
        RangeOp::Gt => version > bound,
        RangeOp::Gte => version >= bound,
        RangeOp::Lt => version < bound,
        RangeOp::Lte => version <= bound,
        RangeOp::Eq => version == bound,
    }
}

/// Returns `true` when `range` is a valid semver range that `compiler_version`
/// satisfies. Disjunctions (`||`) match if any segment matches; whitespace-
/// separated tokens within a segment are conjunctive. `*` / empty matches all.
pub fn range_matches(range: &str, compiler_version: SemVer) -> bool {
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return true;
    }
    range
        .split("||")
        .any(|segment| range_segment_matches(segment.trim(), compiler_version))
}

fn range_segment_matches(segment: &str, compiler_version: SemVer) -> bool {
    // An empty segment comes from a malformed disjunction like `">=4 || "` —
    // a vacuous empty-token loop would return `true`, so reject explicitly.
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

/// First-match-in-declaration-order version selection (tsc's
/// `getPackageJsonTypesVersionsPaths`). Returns the path map of the first
/// version-range key whose range matches `compiler_version`.
pub fn select_paths(
    types_versions: &Value,
    compiler_version: SemVer,
) -> Option<&Map<String, Value>> {
    let map = types_versions.as_object()?;
    for (key, value) in map {
        let Some(value_map) = value.as_object() else {
            continue;
        };
        if range_matches(key, compiler_version) {
            return Some(value_map);
        }
    }
    None
}

/// Split a `prefix*suffix` pattern, mirroring tsc's `tryParsePattern`. Returns
/// `None` for no-`*` patterns and for multi-`*` patterns.
fn parse_pattern(pattern: &str) -> Option<(&str, &str)> {
    let star_pos = pattern.find('*')?;
    let suffix_start = star_pos + 1;
    if pattern[suffix_start..].contains('*') {
        return None;
    }
    Some((&pattern[..star_pos], &pattern[suffix_start..]))
}

/// Substitute the first `*` in a target template with `wildcard`. No-`*`
/// targets are returned unchanged (exact-match entries).
fn substitute_target(target: &str, wildcard: &str) -> String {
    if target.contains('*') {
        target.replacen('*', wildcard, 1)
    } else {
        target.to_string()
    }
}

/// Collect a path-map entry's targets (string or array of strings), with the
/// captured `wildcard` substituted into each.
fn entry_targets(value: &Value, wildcard: &str) -> Vec<String> {
    match value {
        Value::String(target) => vec![substitute_target(target, wildcard)],
        Value::Array(list) => list
            .iter()
            .filter_map(Value::as_str)
            .map(|target| substitute_target(target, wildcard))
            .collect(),
        _ => Vec::new(),
    }
}

/// Given the path map of the selected version entry, return the ordered list of
/// candidate target strings for `subpath` (with `*` already substituted),
/// matching tsc's `matchPatternOrExact`:
/// - an exact (`*`-free) key equal to `subpath` wins outright;
/// - otherwise the longest-prefix single-`*` wildcard key wins (ties → first).
///
/// The caller probes the returned targets in order against the filesystem /
/// file index and uses the first that resolves.
pub fn candidate_targets(paths: &Map<String, Value>, subpath: &str) -> Vec<String> {
    // 1) Exact-match short-circuit (`matchableStringSet.has(candidate)`).
    for (key, value) in paths {
        if !key.contains('*') && key == subpath {
            return entry_targets(value, "");
        }
    }

    // 2) Wildcard candidates: longest prefix wins, ties → first in order.
    let mut best: Option<(usize, &Value, String)> = None;
    for (key, value) in paths {
        let Some((prefix, suffix)) = parse_pattern(key) else {
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

    match best {
        Some((_, value, wildcard)) => entry_targets(value, &wildcard),
        None => Vec::new(),
    }
}

/// Convenience: resolve `types_versions` directly to the ordered candidate
/// target strings for `subpath` at `compiler_version`, performing both the
/// version-range selection and the pattern match. Returns an empty vec when no
/// version range matches or no pattern matches.
pub fn resolve_candidate_targets(
    types_versions: &Value,
    subpath: &str,
    compiler_version: SemVer,
) -> Vec<String> {
    match select_paths(types_versions, compiler_version) {
        Some(paths) => candidate_targets(paths, subpath),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const V6: SemVer = SemVer {
        major: 6,
        minor: 0,
        patch: 3,
    };
    const V5: SemVer = SemVer {
        major: 5,
        minor: 5,
        patch: 0,
    };

    #[test]
    fn parses_partial_and_pre_release_versions() {
        assert_eq!(
            parse_semver("6"),
            Some(SemVer {
                major: 6,
                minor: 0,
                patch: 0
            })
        );
        assert_eq!(
            parse_semver("5.4"),
            Some(SemVer {
                major: 5,
                minor: 4,
                patch: 0
            })
        );
        assert_eq!(
            parse_semver("6.0.3-beta+abc"),
            Some(SemVer {
                major: 6,
                minor: 0,
                patch: 3
            })
        );
        assert_eq!(parse_semver(""), None);
        assert_eq!(parse_semver("x"), None);
    }

    #[test]
    fn range_matching_handles_star_disjunction_and_conjunction() {
        assert!(range_matches("*", V6));
        assert!(range_matches("", V6));
        assert!(range_matches(">=6.0", V6));
        assert!(!range_matches(">=6.0", V5));
        assert!(range_matches(">=5.0 <7.0", V6));
        assert!(!range_matches(">=5.0 <5.5", V6));
        assert!(range_matches(">=7.0 || >=5.0", V6));
        // Malformed disjunction segment must not vacuously match.
        assert!(!range_matches(">=7.0 || ", V6));
        // An unparseable range never matches.
        assert!(!range_matches("garbage", V6));
    }

    #[test]
    fn selects_first_matching_range_in_declaration_order() {
        let tv = json!({
            ">=6.0": { "feature/*": ["loose/feature/*"] },
            ">=5.0 <7.0": { "feature/*": ["ranged/feature/*"] },
            "*": { "feature/*": ["fallback/feature/*"] },
        });
        assert_eq!(
            resolve_candidate_targets(&tv, "feature/widget", V6),
            vec!["loose/feature/widget".to_string()]
        );
    }

    #[test]
    fn skips_non_matching_ranges_and_falls_through_to_next() {
        let tv = json!({
            ">=7.0": { "*": ["next/*"] },
            ">=5.0": { "*": ["current/*"] },
        });
        // 6.0.3 does not satisfy >=7.0, so the first *matching* range is >=5.0.
        assert_eq!(
            resolve_candidate_targets(&tv, "index", V6),
            vec!["current/index".to_string()]
        );
    }

    #[test]
    fn no_matching_range_yields_no_candidates() {
        let tv = json!({ ">=7.0": { "*": ["next/*"] } });
        assert!(resolve_candidate_targets(&tv, "index", V6).is_empty());
    }

    #[test]
    fn exact_match_beats_short_wildcard() {
        let tv = json!({
            "*": { "a": ["wild/a"], "*": ["wild/*"] },
        });
        // Exact key "a" must win over the wildcard even though it is shorter.
        assert_eq!(
            resolve_candidate_targets(&tv, "a", V6),
            vec!["wild/a".to_string()]
        );
    }

    #[test]
    fn longest_prefix_wildcard_wins_ties_keep_first() {
        let tv = json!({
            "*": {
                "*": ["short/*"],
                "feature/*": ["long/*"],
            },
        });
        assert_eq!(
            resolve_candidate_targets(&tv, "feature/x", V6),
            vec!["long/x".to_string()]
        );
    }

    #[test]
    fn declaration_order_independent_of_specificity_for_version_keys() {
        // Version selection must NOT prefer the tighter/more-specific range; it
        // takes the first matching one in declaration order.
        let tv = json!({
            ">=5.0 <7.0": { "*": ["ranged/*"] },
            ">=6.0": { "*": ["loose/*"] },
        });
        assert_eq!(
            resolve_candidate_targets(&tv, "index", V6),
            vec!["ranged/index".to_string()]
        );
    }

    #[test]
    fn multi_star_keys_are_ignored() {
        let tv = json!({ "*": { "a/*/*": ["bad/*"], "a/*": ["good/*"] } });
        assert_eq!(
            resolve_candidate_targets(&tv, "a/b", V6),
            vec!["good/b".to_string()]
        );
    }

    #[test]
    fn array_targets_preserve_order() {
        let tv = json!({ "*": { "*": ["first/*", "second/*"] } });
        assert_eq!(
            resolve_candidate_targets(&tv, "x", V6),
            vec!["first/x".to_string(), "second/x".to_string()]
        );
    }
}
