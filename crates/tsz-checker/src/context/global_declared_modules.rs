//! Pre-built global index of all declared/ambient module names across all binders.

use rustc_hash::FxHashSet;

/// Strip surrounding whitespace and quote characters from a module specifier or
/// pattern spelling, yielding the bare name used for matching and map lookups.
fn trim_module(name: &str) -> &str {
    name.trim().trim_matches('"').trim_matches('\'')
}

/// If the ambient-module `pattern` matches `candidate`, return the literal-prefix
/// length used to rank competing matches; otherwise `None`. Both arguments must
/// already be quote-stripped ([`trim_module`]).
///
/// This mirrors tsc's `tryParsePattern`/`isPatternMatch`/`findBestPatternMatch`
/// exactly and allocates nothing:
/// - A name is a *pattern* only with exactly one `*`; the literal text before and
///   after the star is matched via `startsWith`/`endsWith` (the `*` spans `/`).
/// - Zero or more than one `*` is not a valid pattern — tsc keeps such a name as
///   a plain module name — so it matches only an identical specifier.
/// - The rank is the literal-prefix length (tsc keeps the longest-prefix match);
///   an exact literal ranks by its full length.
///
/// Using plain string operations here avoids compiling a `globset` regex DFA on
/// every lookup — the source of the per-query hot-path cost — while keeping the
/// matching semantics byte-for-byte aligned with tsc.
fn pattern_match_prefix_len(pattern: &str, candidate: &str) -> Option<usize> {
    match pattern.find('*') {
        // Exactly one `*` (no second star after the first): a wildcard pattern.
        Some(star) if !pattern[star + 1..].contains('*') => {
            let prefix = &pattern[..star];
            let suffix = &pattern[star + 1..];
            (candidate.len() >= prefix.len() + suffix.len()
                && candidate.starts_with(prefix)
                && candidate.ends_with(suffix))
            .then_some(prefix.len())
        }
        // Zero or multi-star: tsc treats the name as an exact literal.
        _ => (pattern == candidate).then_some(pattern.len()),
    }
}

/// Match a single ambient-module `pattern` against a concrete module specifier
/// using tsc's pattern semantics (see [`pattern_match_prefix_len`]). Shared by
/// every ambient-module wildcard check so they match identically and never
/// compile a glob regex per call.
#[must_use]
pub fn ambient_pattern_matches(pattern: &str, module_name: &str) -> bool {
    pattern_match_prefix_len(trim_module(pattern), trim_module(module_name)).is_some()
}

/// Pre-built global index of all declared/ambient module names across all binders.
///
/// Separates exact module names (O(1) `HashSet` lookup) from wildcard patterns
/// (a small linear `startsWith`/`endsWith` scan). Built once in `set_all_binders`
/// and shared via `Arc`.
#[derive(Debug, Default)]
pub struct GlobalDeclaredModules {
    /// Exact module names from `declared_modules`, `shorthand_ambient_modules`,
    /// and `module_exports` keys (normalized: quotes stripped).
    pub exact: FxHashSet<String>,
    /// Wildcard patterns (e.g., `*.css`, `*/theme`). Kept as their stored
    /// spelling so a matched entry can be used directly as a `module_exports`
    /// key. Matched with tsc-faithful prefix/suffix semantics, no glob regex.
    pub patterns: Vec<String>,
}

impl GlobalDeclaredModules {
    /// Build from pre-computed skeleton sets.
    ///
    /// `skeleton_exact` and `skeleton_patterns` come from
    /// `SkeletonIndex::build_declared_module_sets()`. The patterns must already
    /// be sorted and deduplicated (the skeleton builder guarantees this).
    #[must_use]
    pub const fn from_skeleton(exact: FxHashSet<String>, patterns: Vec<String>) -> Self {
        Self { exact, patterns }
    }

    /// Build from raw module specifier names.
    ///
    /// Names may be quoted and may include wildcard patterns. This uses the
    /// same normalization and `finish` path as incremental insertion.
    #[must_use]
    pub fn from_module_names<I, S>(module_names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut me = Self::default();
        for module_name in module_names {
            me.insert_module_name(module_name.as_ref());
        }
        me.finish();
        me
    }

    /// Add a module name or wildcard pattern from binder state.
    ///
    /// Binder maps may carry quoted module specifiers; the global lookup index
    /// stores the quote-stripped spelling and separates wildcard patterns from
    /// exact names.
    pub fn insert_module_name(&mut self, module_name: &str) {
        let normalized = module_name.trim_matches('"').trim_matches('\'');
        if normalized.contains('*') {
            self.patterns.push(normalized.to_string());
        } else {
            self.exact.insert(normalized.to_string());
        }
    }

    /// Sort and deduplicate the wildcard patterns after incremental insertion.
    pub fn finish(&mut self) {
        self.patterns.sort();
        self.patterns.dedup();
    }

    /// Returns true if any wildcard pattern matches `module_name`.
    #[must_use]
    pub fn matches_wildcard(&self, module_name: &str) -> bool {
        let normalized = trim_module(module_name);
        self.patterns
            .iter()
            .any(|pattern| pattern_match_prefix_len(trim_module(pattern), normalized).is_some())
    }

    /// Return the declared wildcard pattern that best matches `module_name`,
    /// or `None` when no pattern matches.
    ///
    /// "Best" mirrors tsc's `findBestPatternMatch`: among all matching patterns,
    /// the one with the longest literal prefix (the text before the `*`) wins,
    /// so `prefix/*` is preferred over a broad `*` when both match. Ties keep the
    /// first pattern in declaration order (patterns are sorted/deduplicated).
    ///
    /// The returned string is the pattern's stored spelling (quote-stripped), so
    /// it can be used directly as a `module_exports` key.
    #[must_use]
    pub fn best_matching_pattern(&self, module_name: &str) -> Option<&str> {
        best_wildcard_match(self.patterns.iter().map(String::as_str), module_name)
    }
}

/// Pick the wildcard `pattern` from `patterns` that best matches `module_name`,
/// following tsc's longest-literal-prefix preference (see
/// [`GlobalDeclaredModules::best_matching_pattern`]). Shared by the skeleton
/// index and the standalone `module_exports`-key scan so both rank candidates
/// identically. The returned slice is quote-stripped, ready for a map lookup.
///
/// Only entries containing a `*` participate; exact keys are resolved by the
/// caller's exact-key lookup before this fallback runs.
pub fn best_wildcard_match<'a, I>(patterns: I, module_name: &str) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let normalized = trim_module(module_name);
    let mut best: Option<&str> = None;
    let mut best_prefix_len = 0usize;
    for pattern in patterns {
        let trimmed = trim_module(pattern);
        if !trimmed.contains('*') {
            continue;
        }
        let Some(prefix_len) = pattern_match_prefix_len(trimmed, normalized) else {
            continue;
        };
        if best.is_none() || prefix_len > best_prefix_len {
            best = Some(trimmed);
            best_prefix_len = prefix_len;
        }
    }
    best
}
