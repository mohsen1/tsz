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
            if let Ok(glob) = globset::GlobBuilder::new(trimmed)
                .literal_separator(false)
                .build()
                && glob.compile_matcher().is_match(normalized)
            {
                return true;
            }
        }
        false
    }
}
