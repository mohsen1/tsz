//! Pre-built global index of all declared/ambient module names across all binders.

use rustc_hash::FxHashSet;

/// Pre-built global index of all declared/ambient module names across all binders.
///
/// Separates exact module names (O(1) `HashSet` lookup) from wildcard patterns
/// (single `GlobSet::is_match` call). Built once in `set_all_binders` and
/// shared via `Arc`.
#[derive(Debug, Default)]
pub struct GlobalDeclaredModules {
    /// Exact module names from `declared_modules`, `shorthand_ambient_modules`,
    /// and `module_exports` keys (normalized: quotes stripped).
    pub exact: FxHashSet<String>,
    /// Wildcard patterns (e.g., `*.css`, `*/theme`) that require glob matching.
    pub patterns: Vec<String>,
    /// Pre-compiled matcher over `patterns`. Empty when no wildcards exist.
    /// Lazily filled by `finalize` after the patterns vector is populated.
    pub pattern_set: Option<globset::GlobSet>,
}

impl GlobalDeclaredModules {
    /// Build from pre-computed skeleton sets.
    ///
    /// `skeleton_exact` and `skeleton_patterns` come from
    /// `SkeletonIndex::build_declared_module_sets()`. The patterns must already
    /// be sorted and deduplicated (the skeleton builder guarantees this).
    #[must_use]
    pub fn from_skeleton(exact: FxHashSet<String>, patterns: Vec<String>) -> Self {
        let mut me = Self {
            exact,
            patterns,
            pattern_set: None,
        };
        me.finalize();
        me
    }

    /// Build from raw module specifier names.
    ///
    /// Names may be quoted and may include wildcard patterns. This uses the
    /// same normalization and finalization path as incremental insertion.
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

    /// Sort/deduplicate wildcard patterns and compile the matcher.
    pub fn finish(&mut self) {
        self.patterns.sort();
        self.patterns.dedup();
        self.finalize();
    }

    /// Compile `patterns` into a `GlobSet` for O(patterns) -> O(1)-amortized
    /// match calls. Call once after `patterns` is populated and sorted.
    pub fn finalize(&mut self) {
        if self.patterns.is_empty() {
            self.pattern_set = None;
            return;
        }
        let mut builder = globset::GlobSetBuilder::new();
        for pattern in &self.patterns {
            let trimmed = pattern.trim().trim_matches('"').trim_matches('\'');
            if let Ok(glob) = globset::GlobBuilder::new(trimmed)
                .literal_separator(false)
                .build()
            {
                builder.add(glob);
            }
        }
        self.pattern_set = builder.build().ok();
    }

    /// Returns true if any wildcard pattern matches `module_name`. Uses the
    /// pre-compiled `pattern_set` when available; otherwise falls back to
    /// per-pattern compilation (only hit before `finalize` runs, e.g. tests).
    #[must_use]
    pub fn matches_wildcard(&self, module_name: &str) -> bool {
        let normalized = module_name.trim().trim_matches('"').trim_matches('\'');
        if let Some(set) = &self.pattern_set {
            return set.is_match(normalized);
        }
        for pattern in &self.patterns {
            let trimmed = pattern.trim().trim_matches('"').trim_matches('\'');
            if !trimmed.contains('*') {
                if trimmed == normalized {
                    return true;
                }
                continue;
            }
            if wildcard_glob_match(trimmed, normalized) {
                return true;
            }
        }
        false
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
pub fn best_wildcard_match<'a, I>(patterns: I, module_name: &str) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let normalized = module_name.trim().trim_matches('"').trim_matches('\'');
    let mut best: Option<&str> = None;
    let mut best_prefix_len = 0usize;
    for pattern in patterns {
        let trimmed = pattern.trim().trim_matches('"').trim_matches('\'');
        if !trimmed.contains('*') || !wildcard_glob_match(trimmed, normalized) {
            continue;
        }
        let prefix_len = wildcard_prefix_len(trimmed);
        if best.is_none() || prefix_len > best_prefix_len {
            best = Some(trimmed);
            best_prefix_len = prefix_len;
        }
    }
    best
}

/// Length of the literal prefix of a wildcard pattern — the text before the
/// first `*`. Used to rank competing pattern matches (longest prefix wins).
#[must_use]
pub fn wildcard_prefix_len(pattern: &str) -> usize {
    pattern.split('*').next().map_or(0, str::len)
}

/// Glob-match a single ambient-module wildcard `pattern` against a concrete
/// module specifier. Uses `literal_separator(false)` so `*` spans `/`, matching
/// tsc's pattern-ambient-module semantics (`*.svg` matches `./assets/logo.svg`).
#[must_use]
pub fn wildcard_glob_match(pattern: &str, module_name: &str) -> bool {
    let pattern = pattern.trim().trim_matches('"').trim_matches('\'');
    let module_name = module_name.trim().trim_matches('"').trim_matches('\'');
    if !pattern.contains('*') {
        return pattern == module_name;
    }
    globset::GlobBuilder::new(pattern)
        .literal_separator(false)
        .build()
        .is_ok_and(|glob| glob.compile_matcher().is_match(module_name))
}
