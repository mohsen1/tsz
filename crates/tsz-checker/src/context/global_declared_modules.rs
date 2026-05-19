//! Pre-built global index of all declared/ambient module names across all binders.

use rustc_hash::FxHashSet;

/// Pre-built global index of all declared/ambient module names across all binders.
///
/// Separates exact module names (O(1) `HashSet` lookup) from wildcard patterns.
/// The common `*suffix` ambient-module shape is matched with literal suffix
/// checks; only complex wildcard patterns are compiled into `GlobSet`.
#[derive(Debug, Default)]
pub struct GlobalDeclaredModules {
    /// Exact module names from `declared_modules`, `shorthand_ambient_modules`,
    /// and `module_exports` keys (normalized: quotes stripped).
    pub exact: FxHashSet<String>,
    /// Wildcard patterns (e.g., `*.css`, `*/theme`) kept for diagnostics and
    /// validation parity with skeleton-built sets.
    pub patterns: Vec<String>,
    /// Literal suffixes derived from simple `*suffix` patterns.
    pub suffix_patterns: Vec<String>,
    /// Pre-compiled matcher over complex wildcard patterns. Empty when all
    /// wildcards are simple suffix patterns.
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
            suffix_patterns: Vec::new(),
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

    /// Sort/deduplicate wildcard patterns and compile complex matchers.
    pub fn finish(&mut self) {
        self.patterns.sort();
        self.patterns.dedup();
        self.finalize();
    }

    /// Compile complex wildcard patterns into a `GlobSet`.
    ///
    /// Vite-style ambient declarations are overwhelmingly `*suffix` patterns
    /// like `*.svg` and `*?worker`. TypeScript treats only `*` as the wildcard
    /// in ambient module patterns, so the suffix is literal.
    pub fn finalize(&mut self) {
        self.suffix_patterns.clear();
        if self.patterns.is_empty() {
            self.pattern_set = None;
            return;
        }

        let mut complex_patterns = Vec::new();
        for pattern in &self.patterns {
            let trimmed = pattern.trim().trim_matches('"').trim_matches('\'');
            if let Some(suffix) = simple_suffix_pattern(trimmed) {
                self.suffix_patterns.push(suffix.to_string());
            } else {
                complex_patterns.push(trimmed.to_string());
            }
        }
        self.suffix_patterns.sort();
        self.suffix_patterns.dedup();

        if complex_patterns.is_empty() {
            self.pattern_set = None;
            return;
        }

        let mut builder = globset::GlobSetBuilder::new();
        for trimmed in &complex_patterns {
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
        if self
            .suffix_patterns
            .iter()
            .any(|suffix| normalized.ends_with(suffix))
        {
            return true;
        }
        if let Some(set) = &self.pattern_set {
            return set.is_match(normalized);
        }
        for pattern in &self.patterns {
            let trimmed = pattern.trim().trim_matches('"').trim_matches('\'');
            if let Some(suffix) = simple_suffix_pattern(trimmed) {
                if normalized.ends_with(suffix) {
                    return true;
                }
                continue;
            }
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

fn simple_suffix_pattern(pattern: &str) -> Option<&str> {
    let suffix = pattern.strip_prefix('*')?;
    (!suffix.contains('*')).then_some(suffix)
}
