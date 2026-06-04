fn contains_glob_meta(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[') || pattern.contains(']')
}

fn should_add_recursive_exclude_variant(pattern: &str) -> bool {
    if pattern.ends_with("/**") || pattern.ends_with("/**/*") {
        return false;
    }

    let base = pattern.trim_end_matches('/');
    let last_segment = base.rsplit('/').next().unwrap_or(base);

    !last_segment.is_empty() && !contains_glob_meta(last_segment) && !last_segment.contains('.')
}

fn expand_auto_import_exclude_pattern(pattern: &str) -> Vec<String> {
    let base = pattern.trim_end_matches('/').to_string();
    if base.is_empty() {
        return Vec::new();
    }

    let mut expanded = vec![base.clone()];
    if should_add_recursive_exclude_variant(&base) {
        expanded.push(format!("{base}/**"));
    }
    expanded
}

fn parse_regex_literal_pattern(input: &str) -> Option<(&str, &str)> {
    if !input.starts_with('/') {
        return None;
    }

    let mut closing = None;
    let mut escaped = false;
    for (idx, ch) in input.char_indices().skip(1) {
        if ch == '/' && !escaped {
            closing = Some(idx);
        }
        escaped = ch == '\\' && !escaped;
    }

    let closing = closing?;

    let body = &input[1..closing];
    if body.is_empty() {
        return None;
    }

    let mut body_escaped = false;
    for ch in body.chars() {
        if ch == '/' && !body_escaped {
            return None;
        }
        body_escaped = ch == '\\' && !body_escaped;
    }

    Some((body, &input[closing + 1..]))
}

fn compile_auto_import_specifier_exclude_pattern(pattern: &str) -> Option<Regex> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return None;
    }

    if let Some((body, flags)) = parse_regex_literal_pattern(pattern) {
        let mut builder = RegexBuilder::new(body);
        for flag in flags.chars() {
            match flag {
                'i' => {
                    builder.case_insensitive(true);
                }
                'm' => {
                    builder.multi_line(true);
                }
                's' => {
                    builder.dot_matches_new_line(true);
                }
                'x' => {
                    builder.ignore_whitespace(true);
                }
                // JavaScript flags that don't affect `is_match` behavior here.
                'g' | 'y' | 'u' | 'd' => {}
                _ => return None,
            }
        }
        return builder.build().ok();
    }

    Regex::new(pattern).ok()
}

/// Auto-import specifier preference matching the LSP `importModuleSpecifierPreference` values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImportSpecifierPreference {
    NonRelative,
    Relative,
    ProjectRelative,
}

impl ImportSpecifierPreference {
    fn from_lsp_value(s: &str) -> Option<Self> {
        match s {
            "non-relative" => Some(Self::NonRelative),
            "relative" => Some(Self::Relative),
            "project-relative" => Some(Self::ProjectRelative),
            _ => None,
        }
    }
}

/// Multi-file container for LSP operations.
pub struct Project {
    pub(crate) files: FxHashMap<String, ProjectFile>,
    pub(crate) dependency_graph: DependencyGraph,
    pub(crate) symbol_index: SymbolIndex,
    pub(crate) performance: ProjectPerformance,
    pub(crate) strict: bool,
    pub(crate) allow_importing_ts_extensions: bool,
    pub(crate) import_module_specifier_ending: Option<String>,
    pub(crate) import_module_specifier_preference: Option<ImportSpecifierPreference>,
    pub(crate) auto_import_file_exclude_matchers: Vec<globset::GlobMatcher>,
    pub(crate) auto_import_specifier_exclude_matchers: Vec<Regex>,
    pub(crate) auto_imports_allowed_without_tsconfig: bool,
    /// Workspace root directories (from workspace folders or tsconfig locations).
    pub(crate) workspace_roots: Vec<String>,
    /// Parsed tsconfig.json settings per workspace root.
    pub(crate) tsconfig_settings: FxHashMap<String, TsConfigSettings>,
    /// Shared type interner for cross-file type identity.
    ///
    /// All `ProjectFile` instances in this project share the same `TypeInterner`,
    /// ensuring that `TypeId`s are globally unique across files. This is a
    /// prerequisite for wiring shared `DefinitionStore` into per-file checkers,
    /// since `DefId -> TypeId` resolution requires a single type universe.
    pub(crate) type_interner: Arc<TypeInterner>,
    /// Shared definition store for cross-file `DefId` consistency.
    ///
    /// All `CheckerState` instances created for files in this project share
    /// the same `DefinitionStore`, ensuring that `DefId`s are globally unique
    /// and cross-file type references resolve correctly.
    ///
    /// Wired into per-file checkers via `ProjectFile::definition_store` field.
    /// When a `ProjectFile` has a shared `DefinitionStore`, its `get_diagnostics()`
    /// method uses `CheckerState::with_cache_and_shared_def_store` (or
    /// `new_with_shared_def_store`) to propagate it into the checker context.
    pub(crate) definition_store: Arc<DefinitionStore>,
    /// Stable file ID allocator for per-file `DefinitionStore` invalidation.
    ///
    /// Assigns a unique `u32` to each file name, ensuring that definitions
    /// registered in the `DefinitionStore` carry stable file provenance.
    /// When a file is removed or replaced, `invalidate_file(file_idx)` cleans
    /// up all stale definitions.
    pub(crate) file_id_allocator: FileIdAllocator,
    /// Centralized export signature fingerprint cache.
    ///
    /// Tracks the most recent export signature (as a `u64` fingerprint) for
    /// every file in the project, keyed by `file_idx`. Updated on every
    /// `set_file`/`update_file`/`remove_file` call.
    ///
    /// Enables batch change detection via
    /// [`tsz_solver::def::incremental::diff_fingerprints`] — snapshot before
    /// and after a batch of edits, diff the snapshots, and apply invalidation
    /// in one pass.
    pub(crate) fingerprint_cache: SkeletonFingerprintCache,
    /// Files currently open in the editor (tracked via `didOpen`/`didClose`).
    ///
    /// Open files are never evicted under memory pressure. The eviction module
    /// uses this set to skip actively-edited files.
    pub(crate) open_files: FxHashSet<String>,
    /// File the editor most recently focused (opened, edited, or asked a
    /// position-bearing request for). Workspace-symbol fuzzy ranking uses
    /// this to tie-break by file proximity. `None` when the editor has not
    /// yet announced any focus.
    pub(crate) focused_file: Option<String>,
}

/// Assigns stable `u32` file indices to file names.
///
/// Each file name gets a unique, monotonically increasing ID. IDs are never
/// reused (even after file removal) to avoid ABA problems where a new file
/// might accidentally inherit stale definitions from an old file with the
/// same ID.
///
/// The allocator is O(1) for both allocation and lookup.
#[derive(Debug, Clone, Default)]
pub(crate) struct FileIdAllocator {
    /// Maps file name to its assigned `u32` file index.
    name_to_id: FxHashMap<String, u32>,
    /// Reverse mapping: file index -> file name.
    /// Indexed by the `u32` file index. Entries are set to empty string on removal
    /// (IDs are never recycled, so the slot stays allocated).
    id_to_name: Vec<String>,
    /// Next ID to allocate.
    next_id: u32,
}

impl FileIdAllocator {
    /// Create a new allocator.
    pub fn new() -> Self {
        Self {
            name_to_id: FxHashMap::default(),
            id_to_name: Vec::new(),
            // Start at 0; u32::MAX is reserved as "unassigned".
            next_id: 0,
        }
    }

    /// Get or allocate a stable file index for the given file name.
    ///
    /// If the file already has an ID, returns it. Otherwise, allocates a new
    /// one. IDs are never reused.
    pub fn get_or_allocate(&mut self, file_name: &str) -> u32 {
        if let Some(&id) = self.name_to_id.get(file_name) {
            return id;
        }
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).expect("file ID overflow");
        self.name_to_id.insert(file_name.to_string(), id);
        self.id_to_name.push(file_name.to_string());
        debug_assert_eq!(self.id_to_name.len(), id as usize + 1);
        id
    }

    /// Look up the file index for a file name without allocating.
    ///
    /// Returns `None` if the file was never registered.
    pub fn lookup(&self, file_name: &str) -> Option<u32> {
        self.name_to_id.get(file_name).copied()
    }

    /// Remove a file name from the allocator.
    ///
    /// The ID is NOT recycled — future allocations continue from `next_id`.
    /// This prevents stale definition collisions.
    pub fn remove(&mut self, file_name: &str) -> Option<u32> {
        let id = self.name_to_id.remove(file_name)?;
        // Clear the reverse entry. The slot stays allocated (IDs are never recycled).
        if let Some(entry) = self.id_to_name.get_mut(id as usize) {
            entry.clear();
        }
        Some(id)
    }

    /// Look up the file name for a given file index.
    ///
    /// Returns `None` if the index was never allocated or the file was removed.
    pub fn name_for_id(&self, file_idx: u32) -> Option<&str> {
        let name = self.id_to_name.get(file_idx as usize)?;
        if name.is_empty() {
            None
        } else {
            Some(name.as_str())
        }
    }
}

/// Centralized cache of per-file export signature fingerprints.
///
/// Maintains a `file_idx -> fingerprint` mapping that tracks the most recent
/// export signature for every file in the project. This enables:
///
/// 1. **O(1) change detection**: compare old and new fingerprints to determine
///    whether a file's public API changed.
/// 2. **Batch diffing**: snapshot the cache as `(file_idx, fingerprint)` pairs
///    and feed them to [`tsz_solver::def::incremental::diff_fingerprints`] for
///    coordinated multi-file invalidation.
/// 3. **Separation of concerns**: the `Project` stores fingerprints in one
///    central location rather than scattering them across `ProjectFile` fields.
///
/// The cache is updated on every `set_file`, `update_file`, and `remove_file`
/// call. It is read-only during diagnostic computation.
#[derive(Debug, Clone, Default)]
pub(crate) struct SkeletonFingerprintCache {
    /// Maps `file_idx` to the file's current export signature fingerprint.
    entries: FxHashMap<u32, u64>,
}

impl SkeletonFingerprintCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self {
            entries: FxHashMap::default(),
        }
    }

    /// Record or update the fingerprint for a file.
    ///
    /// Returns the previous fingerprint if the file was already tracked,
    /// or `None` if this is a new entry.
    pub fn update(&mut self, file_idx: u32, fingerprint: u64) -> Option<u64> {
        self.entries.insert(file_idx, fingerprint)
    }

    /// Remove a file's fingerprint from the cache.
    ///
    /// Returns the removed fingerprint, or `None` if the file was not tracked.
    pub fn remove(&mut self, file_idx: u32) -> Option<u64> {
        self.entries.remove(&file_idx)
    }

    /// Look up the current fingerprint for a file.
    pub fn get(&self, file_idx: u32) -> Option<u64> {
        self.entries.get(&file_idx).copied()
    }

    /// Snapshot all entries as `(file_idx, fingerprint)` pairs.
    ///
    /// The output is suitable for [`tsz_solver::def::incremental::diff_fingerprints`].
    pub fn snapshot(&self) -> Vec<(u32, u64)> {
        self.entries.iter().map(|(&k, &v)| (k, v)).collect()
    }
}

/// Parsed settings from tsconfig.json relevant to LSP operation.
#[derive(Debug, Clone, Default)]
pub struct TsConfigSettings {
    /// The root directory containing the tsconfig.json.
    pub root_dir: String,
    /// Whether strict mode is enabled.
    pub strict: Option<bool>,
    /// Target ES version (affects lib files).
    pub target: Option<String>,
    /// Module resolution strategy.
    pub module_resolution: Option<String>,
    /// Base URL for module resolution.
    pub base_url: Option<String>,
    /// Path mappings for module resolution.
    pub paths: FxHashMap<String, Vec<String>>,
    /// Files to include.
    pub include: Vec<String>,
    /// Files to exclude.
    pub exclude: Vec<String>,
    /// Root directory for source files.
    pub root_dir_setting: Option<String>,
    /// Output directory.
    pub out_dir: Option<String>,
    /// Whether to allow importing .ts extensions.
    pub allow_importing_ts_extensions: Option<bool>,
    /// JSX setting.
    pub jsx: Option<String>,
}

impl Project {
    /// Create a new empty project.
    pub fn new() -> Self {
        Self {
            files: FxHashMap::default(),
            dependency_graph: DependencyGraph::new(),
            symbol_index: SymbolIndex::new(),
            performance: ProjectPerformance::default(),
            strict: false,
            allow_importing_ts_extensions: false,
            import_module_specifier_ending: None,
            import_module_specifier_preference: None,
            auto_import_file_exclude_matchers: Vec::new(),
            auto_import_specifier_exclude_matchers: Vec::new(),
            auto_imports_allowed_without_tsconfig: true,
            workspace_roots: Vec::new(),
            tsconfig_settings: FxHashMap::default(),
            type_interner: Arc::new(TypeInterner::new()),
            definition_store: Arc::new(DefinitionStore::new()),
            file_id_allocator: FileIdAllocator::new(),
            fingerprint_cache: SkeletonFingerprintCache::new(),
            open_files: FxHashSet::default(),
            focused_file: None,
        }
    }

    /// Creates an empty project using default values.
    fn empty() -> Self {
        Self {
            files: FxHashMap::default(),
            dependency_graph: DependencyGraph::new(),
            symbol_index: SymbolIndex::new(),
            performance: ProjectPerformance::default(),
            strict: false,
            allow_importing_ts_extensions: false,
            import_module_specifier_ending: None,
            import_module_specifier_preference: None,
            auto_import_file_exclude_matchers: Vec::new(),
            auto_import_specifier_exclude_matchers: Vec::new(),
            auto_imports_allowed_without_tsconfig: true,
            workspace_roots: Vec::new(),
            tsconfig_settings: FxHashMap::default(),
            type_interner: Arc::new(TypeInterner::new()),
            definition_store: Arc::new(DefinitionStore::new()),
            file_id_allocator: FileIdAllocator::new(),
            fingerprint_cache: SkeletonFingerprintCache::new(),
            open_files: FxHashSet::default(),
            focused_file: None,
        }
    }

    /// Add a workspace root directory.
    pub fn add_workspace_root(&mut self, root: String) {
        if !self.workspace_roots.contains(&root) {
            self.workspace_roots.push(root);
        }
    }

    /// Remove a workspace root directory.
    pub fn remove_workspace_root(&mut self, root: &str) {
        self.workspace_roots.retain(|r| r != root);
        self.tsconfig_settings.remove(root);
    }

    /// Get the workspace roots.
    pub fn workspace_roots(&self) -> &[String] {
        &self.workspace_roots
    }

    /// Get tsconfig settings for a workspace root.
    pub fn tsconfig_for_root(&self, root: &str) -> Option<&TsConfigSettings> {
        self.tsconfig_settings.get(root)
    }

    /// Get the shared type interner for this project.
    ///
    /// Returns a clone of the `Arc`, allowing callers to share the interner
    /// with checker instances or other components that need cross-file
    /// type identity.
    pub fn type_interner(&self) -> Arc<TypeInterner> {
        Arc::clone(&self.type_interner)
    }

    /// Get the shared definition store for this project.
    ///
    /// Returns a clone of the `Arc`, allowing callers to share the store
    /// with checker instances or other components that need cross-file
    /// `DefId` consistency.
    pub fn definition_store(&self) -> Arc<DefinitionStore> {
        Arc::clone(&self.definition_store)
    }

    /// Snapshot the current export signature fingerprints for all files.
    ///
    /// Returns `(file_idx, fingerprint)` pairs suitable for feeding to
    /// [`tsz_solver::def::incremental::diff_fingerprints`]. Take a snapshot
    /// before a batch of edits, apply the edits, take another snapshot, and
    /// diff them to determine which files' public APIs changed.
    pub fn fingerprint_snapshot(&self) -> Vec<(u32, u64)> {
        self.fingerprint_cache.snapshot()
    }

    /// Look up the current export signature fingerprint for a file.
    ///
    /// Returns `None` if the file is not in the project or has no assigned
    /// file index.
    pub fn fingerprint_for_file(&self, file_name: &str) -> Option<u64> {
        let file_idx = self.file_id_allocator.lookup(file_name)?;
        self.fingerprint_cache.get(file_idx)
    }

    /// Load and parse a tsconfig.json file, storing settings for the workspace root.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_tsconfig(&mut self, root: &str) {
        let tsconfig_path = Path::new(root).join("tsconfig.json");
        if !tsconfig_path.exists() {
            // Try jsconfig.json as fallback
            let jsconfig_path = Path::new(root).join("jsconfig.json");
            if jsconfig_path.exists()
                && let Some(settings) = parse_tsconfig_file(&jsconfig_path)
            {
                self.apply_tsconfig_settings(root, settings);
            }
            return;
        }

        if let Some(settings) = parse_tsconfig_file(&tsconfig_path) {
            self.apply_tsconfig_settings(root, settings);
        }
    }

    /// Apply parsed tsconfig settings to the project.
    #[cfg(not(target_arch = "wasm32"))]
    fn apply_tsconfig_settings(&mut self, root: &str, settings: TsConfigSettings) {
        // Apply strict mode
        if let Some(strict) = settings.strict {
            self.set_strict(strict);
        }

        // Apply allowImportingTsExtensions
        if let Some(allow) = settings.allow_importing_ts_extensions {
            self.set_allow_importing_ts_extensions(allow);
        }

        self.tsconfig_settings.insert(root.to_string(), settings);
    }

    /// Discover and load TypeScript/JavaScript files from workspace roots.
    ///
    /// Walks each workspace root directory and loads files matching common
    /// TypeScript/JavaScript extensions (.ts, .tsx, .js, .jsx, .mts, .cts).
    /// Respects tsconfig include/exclude patterns when available.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn discover_files(&mut self, roots: &[String]) -> Vec<String> {
        let mut discovered = Vec::new();

        for root in roots {
            let root_path = Path::new(root);
            if !root_path.is_dir() {
                continue;
            }

            // Get include/exclude patterns from tsconfig if available
            let (includes, excludes) = self
                .tsconfig_settings
                .get(root)
                .map(|ts| (ts.include.clone(), ts.exclude.clone()))
                .unwrap_or_else(|| {
                    (
                        Vec::new(),
                        vec![
                            "node_modules".to_string(),
                            "dist".to_string(),
                            "build".to_string(),
                            ".git".to_string(),
                        ],
                    )
                });

            // Build exclude matchers
            let exclude_matchers: Vec<globset::GlobMatcher> = excludes
                .iter()
                .filter_map(|pattern| {
                    Glob::new(&format!("**/{pattern}/**"))
                        .ok()
                        .map(|g| g.compile_matcher())
                })
                .collect();
            let include_matchers = compile_tsconfig_include_matchers(&includes);

            // Walk directory
            let walker = walkdir::WalkDir::new(root_path)
                .follow_links(false)
                .max_depth(20);

            for entry in walker.into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();

                // Skip directories that match exclude patterns
                if entry.file_type().is_dir() {
                    continue;
                }

                let path_str = path.to_string_lossy().to_string();
                let relative_path =
                    path_to_slash_string(path.strip_prefix(root_path).unwrap_or(path));

                // Check exclude patterns
                if exclude_matchers.iter().any(|m| m.is_match(&path_str)) {
                    continue;
                }

                // Check if it's a TS/JS file
                if !is_ts_js_file(&path_str) {
                    continue;
                }

                // Only load files within include patterns if specified
                if !include_matchers.is_empty()
                    && !include_matchers.iter().any(|m| m.is_match(&relative_path))
                {
                    continue;
                }

                // Load the file
                if let Ok(content) = std::fs::read_to_string(path) {
                    self.set_file(path_str.clone(), content);
                    discovered.push(path_str);
                }
            }
        }

        discovered
    }

    /// Get the strict mode setting for type checking.
    pub const fn strict(&self) -> bool {
        self.strict
    }

    /// Set the strict mode directly.
    pub fn set_strict(&mut self, strict: bool) {
        self.strict = strict;
        // Update strict mode on all existing files
        for file in self.files.values_mut() {
            file.set_strict(strict);
        }
    }

    pub const fn set_allow_importing_ts_extensions(&mut self, allow: bool) {
        self.allow_importing_ts_extensions = allow;
    }

    /// Set completion module-specifier ending preference (e.g. "js").
    pub fn set_import_module_specifier_ending(&mut self, ending: Option<String>) {
        self.import_module_specifier_ending = ending;
    }

    /// Set preference for module specifier generation from the LSP
    /// `importModuleSpecifierPreference` string. Unknown or `"shortest"` values
    /// are silently treated as the default (shortest-first) ordering.
    pub fn set_import_module_specifier_preference(&mut self, pref: Option<String>) {
        self.import_module_specifier_preference = pref
            .as_deref()
            .and_then(ImportSpecifierPreference::from_lsp_value);
    }

    /// Set auto-import exclusion patterns used by completions and import fixes.
    pub fn set_auto_import_file_exclude_patterns(&mut self, patterns: Vec<String>) {
        self.auto_import_file_exclude_matchers.clear();
        for pattern in patterns {
            let Some(normalized) = normalize_auto_import_exclude_pattern(&pattern) else {
                continue;
            };
            for expanded in expand_auto_import_exclude_pattern(&normalized) {
                let Ok(glob) = Glob::new(&expanded) else {
                    continue;
                };
                self.auto_import_file_exclude_matchers
                    .push(glob.compile_matcher());
            }
        }
    }

    /// Set module-specifier exclusion regexes used by completions and import fixes.
    pub fn set_auto_import_specifier_exclude_regexes(&mut self, patterns: Vec<String>) {
        self.auto_import_specifier_exclude_matchers.clear();
        for pattern in patterns {
            if let Some(regex) = compile_auto_import_specifier_exclude_pattern(&pattern) {
                self.auto_import_specifier_exclude_matchers.push(regex);
            }
        }
    }

    /// Set inferred-project fallback for whether module-export auto-imports are legal.
    pub const fn set_auto_imports_allowed_without_tsconfig(&mut self, allow: bool) {
        self.auto_imports_allowed_without_tsconfig = allow;
    }

    /// Total number of files tracked by the project.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Iterate over all file names in the project.
    pub fn file_names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(|s| s.as_str())
    }

    /// Look up the file name for a given `file_idx` (as stamped on binder symbols).
    ///
    /// Returns `None` if the index was never allocated or the file was removed.
    /// This enables resolving a symbol's owning file from its `decl_file_idx`.
    pub fn file_name_for_idx(&self, file_idx: u32) -> Option<&str> {
        self.file_id_allocator.name_for_id(file_idx)
    }

    /// Get the set of files that directly import the given file.
    pub fn get_file_dependents(&self, file: &str) -> Vec<String> {
        self.dependency_graph
            .get_dependents(file)
            .map(|deps| deps.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Snapshot of per-request timing data.
    pub const fn performance(&self) -> &ProjectPerformance {
        &self.performance
    }

    /// Compute aggregate memory residency statistics for the project.
    ///
    /// Iterates all files and sums their `estimated_size_bytes()`.
    /// The result is a snapshot — it does not cache or persist.
    #[must_use]
    pub fn residency_stats(&self) -> ProjectResidencyStats {
        let mut total: usize = 0;
        let mut largest: Option<(&str, usize)> = None;
        let mut smallest: Option<(&str, usize)> = None;

        for (name, file) in &self.files {
            let est = file.estimated_size_bytes();
            total = total.saturating_add(est);
            if largest.is_none_or(|(_, s)| est > s) {
                largest = Some((name.as_str(), est));
            }
            if smallest.is_none_or(|(_, s)| est < s) {
                smallest = Some((name.as_str(), est));
            }
        }

        ProjectResidencyStats {
            file_count: self.files.len(),
            total_estimated_bytes: total,
            largest_file: largest.map(|(n, s)| (n.to_string(), s)),
            smallest_file: smallest.map(|(n, s)| (n.to_string(), s)),
            type_interner_estimated_bytes: self.type_interner.estimated_size_bytes(),
            definition_store_estimated_bytes: self.definition_store.estimated_size_bytes(),
        }
    }

    /// Estimated memory footprint of a single file, or `None` if not tracked.
    #[must_use]
    pub fn file_estimated_size(&self, file_name: &str) -> Option<usize> {
        self.files.get(file_name).map(|f| f.estimated_size_bytes())
    }

    /// Return per-file residency info sorted for eviction (best candidates first).
    ///
    /// Files are ranked by a composite score: idle duration (seconds) multiplied
    /// by estimated size (bytes). This prefers evicting files that are both large
    /// and cold. Declaration files (`*.d.ts`) are deprioritized since they are
    /// typically shared dependencies.
    ///
    /// The optional `min_idle` parameter filters out files that have been accessed
    /// more recently than the threshold — active files are never eviction candidates.
    #[must_use]
    pub fn eviction_candidates(&self, min_idle: Option<Duration>) -> Vec<FileResidencyInfo> {
        let now = Instant::now();
        let mut candidates: Vec<FileResidencyInfo> = self
            .files
            .iter()
            .filter_map(|(name, file)| {
                let idle = now.duration_since(file.last_accessed);
                if let Some(threshold) = min_idle
                    && idle < threshold
                {
                    return None;
                }
                Some(FileResidencyInfo {
                    file_name: name.clone(),
                    estimated_bytes: file.estimated_size_bytes(),
                    idle_duration: idle,
                })
            })
            .collect();

        // Sort by composite eviction score: idle_seconds * size_bytes (descending).
        // Declaration files get a 4x penalty (lower effective score) to keep them
        // resident longer since they're shared across many importers.
        candidates.sort_by(|a, b| {
            let score = |info: &FileResidencyInfo| -> u64 {
                let idle_secs = info.idle_duration.as_secs().max(1);
                let size = info.estimated_bytes as u64;
                let raw = idle_secs.saturating_mul(size);
                if info.file_name.ends_with(".d.ts") {
                    raw / 4
                } else {
                    raw
                }
            };
            score(b).cmp(&score(a))
        });

        candidates
    }

    /// Mark a file as recently accessed.
    ///
    /// Call this when the file is used for any LSP operation (diagnostics,
    /// hover, completions, go-to-definition, references, etc.) so that
    /// eviction heuristics can distinguish hot files from cold ones.
    pub fn touch_file(&mut self, file_name: &str) {
        if let Some(file) = self.files.get_mut(file_name) {
            file.touch();
        }
    }

    /// Add or replace a file, re-parsing and re-binding its contents.
    ///
    /// If the file already exists with identical content (same content hash),
    /// the re-parse and re-bind are skipped entirely. This avoids redundant work
    /// when the LSP receives `didOpen` for an already-loaded file, or `didSave`
    /// without content changes.
    pub fn set_file(&mut self, file_name: String, source_text: String) {
        // Fast path: skip re-parse if file exists with identical content.
        let new_hash = hash_source_content(&source_text);
        if let Some(existing) = self.files.get(&file_name)
            && existing.content_hash == new_hash
        {
            return;
        }

        // Allocate a stable file index. If the file already has one, reuse it
        // (the allocator returns the existing ID). This ensures that
        // invalidate_file + re-register uses the same ID.
        let file_idx = self.file_id_allocator.get_or_allocate(&file_name);

        // Invalidate old definitions in the DefinitionStore before re-binding.
        // This cleans up stale DefIds from the previous version of this file.
        if self.files.contains_key(&file_name) {
            self.definition_store.invalidate_file(file_idx);
        }

        let file = ProjectFile::with_full_project_context(
            file_name.clone(),
            source_text,
            self.strict,
            Arc::clone(&self.type_interner),
            Arc::clone(&self.definition_store),
            file_idx,
        );

        // Update symbol index with the new file's binder data and AST identifiers
        // We need to get the arena before moving the file into self.files
        let arena = file.parser.get_arena();
        let source = file.source_text();
        self.symbol_index
            .index_file(&file_name, &file.binder, arena, source);

        // Record the new export signature in the fingerprint cache.
        let new_fp = file.export_signature.0;
        self.fingerprint_cache.update(file_idx, new_fp);

        self.files.insert(file_name.clone(), file);

        // Log per-file memory estimate for telemetry / pressure tracking.
        if let Some(f) = self.files.get(&file_name) {
            let est = f.estimated_size_bytes();
            tracing::debug!(
                file = %file_name,
                estimated_bytes = est,
                file_count = self.files.len(),
                "project: file added/replaced"
            );
        }

        // Update dependency graph with imports from this file
        self.update_dependencies(&file_name);
    }

    /// Update an existing file by applying incremental text edits.
    ///
    /// Uses export signature comparison to avoid unnecessary cache invalidation:
    /// if the file's public API (exports, re-exports, augmentations) didn't change,
    /// dependent files keep their cached diagnostics.
    ///
    /// Returns an `InvalidationSummary` describing what changed and how many
    /// dependents were invalidated. Useful for perf analysis.
    pub fn update_file(
        &mut self,
        file_name: &str,
        edits: &[TextEdit],
    ) -> Option<InvalidationSummary> {
        if edits.is_empty() {
            let sig = self.files.get(file_name)?.export_signature.0;
            return Some(InvalidationSummary::unchanged(file_name.to_string(), sig));
        }

        let (updated_source, unchanged) = {
            let file = self.files.get(file_name)?;
            let source = file.source_text();
            let updated = apply_text_edits(source, file.line_map(), edits)?;
            let unchanged = updated == source;
            (updated, unchanged)
        };

        if unchanged {
            let sig = self.files.get(file_name)?.export_signature.0;
            return Some(InvalidationSummary::unchanged(file_name.to_string(), sig));
        }

        // Capture old export signature before updating
        let old_signature = self.files.get(file_name)?.export_signature;

        // Invalidate old definitions before re-binding. The re-bind will
        // create new definitions with the same file_idx.
        if let Some(file_idx) = self.file_id_allocator.lookup(file_name) {
            self.definition_store.invalidate_file(file_idx);
        }

        let file = self.files.get_mut(file_name)?;
        file.update_source_with_edits(updated_source, edits);

        // Re-index the file in the symbol index with updated binder and arena
        let arena = file.parser.get_arena();
        let source = file.source_text();
        self.symbol_index
            .update_file(file_name, &file.binder, arena, source);

        // Update the fingerprint cache with the new export signature.
        let new_signature = file.export_signature;
        if let Some(file_idx) = self.file_id_allocator.lookup(file_name) {
            self.fingerprint_cache.update(file_idx, new_signature.0);
        }

        // Smart cache invalidation: only invalidate dependents if the public API changed.
        // Body-only edits, comment changes, and private symbol changes won't trigger
        // dependent re-checking — this is the key optimization.
        if old_signature != new_signature {
            let affected_files = self.dependency_graph.get_affected_files(file_name);
            let mut invalidated_count = 0;
            for affected_file in affected_files {
                if let Some(dep_file) = self.files.get_mut(&affected_file) {
                    dep_file.invalidate_caches();
                    invalidated_count += 1;
                }
            }
            Some(InvalidationSummary::changed(
                file_name.to_string(),
                Some(old_signature.0),
                new_signature.0,
                invalidated_count,
            ))
        } else {
            Some(InvalidationSummary::unchanged(
                file_name.to_string(),
                new_signature.0,
            ))
        }
    }

    /// Remove a file from the project.
    ///
    /// Cleans up:
    /// - Stale definitions in the shared `DefinitionStore`
    /// - Symbol index entries for the file
    /// - Dependency graph edges (both imports and dependents)
    /// - Cached diagnostics/types in files that depended on the removed file
    /// - File ID allocation (ID is retired, not recycled)
    pub fn remove_file(&mut self, file_name: &str) -> Option<ProjectFile> {
        // Invalidate definitions in the DefinitionStore for this file.
        // This must happen before removing from the files map so the file_idx
        // is still available.
        if let Some(file_idx) = self.file_id_allocator.lookup(file_name) {
            self.definition_store.invalidate_file(file_idx);
            // Remove from fingerprint cache.
            self.fingerprint_cache.remove(file_idx);
        }
        // Remove the file ID (retired, not recycled).
        self.file_id_allocator.remove(file_name);

        // Remove from symbol index
        self.symbol_index.remove_file(file_name);

        // Invalidate caches in files that depend on the removed file,
        // since the removed file's exports are no longer available.
        let affected_files = self.dependency_graph.get_affected_files(file_name);
        for affected_file in affected_files {
            if let Some(dep_file) = self.files.get_mut(&affected_file) {
                dep_file.invalidate_caches();
            }
        }

        // Remove from dependency graph (cleans up both outgoing and incoming edges)
        self.dependency_graph.remove_file(file_name);

        // Log memory freed by removal.
        let freed_bytes = self
            .files
            .get(file_name)
            .map(|f| f.estimated_size_bytes())
            .unwrap_or(0);

        let removed = self.files.remove(file_name);

        if removed.is_some() {
            tracing::debug!(
                file = %file_name,
                freed_bytes,
                remaining_files = self.files.len(),
                "project: file removed"
            );
        }

        removed
    }

    /// Update the dependency graph for a file using binder-collected import sources.
    ///
    /// Uses `BinderState::file_import_sources` which the binder populates during
    /// binding from static import/export declarations. This avoids a redundant
    /// full-AST walk that the previous `extract_imports` method performed.
    ///
    /// Note: `file_import_sources` captures static imports only (import/export
    /// declarations, `import = require()`). Dynamic `import()` and `require()`
    /// calls are not included, which is the correct behavior for the dependency
    /// graph — dynamic imports are lazy and should not trigger eager invalidation.
    fn update_dependencies(&mut self, file_name: &str) {
        let imports = match self.files.get(file_name) {
            Some(file) => file.binder.file_import_sources.clone(),
            None => return,
        };
        self.dependency_graph.update_file(file_name, &imports);
    }

    /// Handle file rename requests from the LSP client.
    ///
    /// When files are renamed or moved, this calculates the `TextEdits` needed
    /// to update import statements in all dependent files.
    ///
    /// # Arguments
    /// * `renames` - List of file renames (old path -> new path)
    ///
    /// # Returns
    /// A `WorkspaceEdit` containing all the `TextEdits` needed to update imports
    ///
    /// # Example
    /// ```ignore
    /// // When utils.ts moves to src/utils.ts
    /// let renames = vec![FileRename {
    ///     old_uri: "/project/utils.ts".to_string(),
    ///     new_uri: "/project/src/utils.ts".to_string(),
    /// }];
    /// let edits = project.handle_will_rename_files(&renames);
    /// // Returns edits for all files that import utils.ts
    /// ```
    #[cfg(not(target_arch = "wasm32"))]
    pub fn handle_will_rename_files(&mut self, renames: &[FileRename]) -> WorkspaceEdit {
        use std::path::Path;

        let mut result = WorkspaceEdit::new();

        for rename in renames {
            let old_path = Path::new(&rename.old_uri);
            let new_path = Path::new(&rename.new_uri);

            // Check if this is a directory rename
            if self.is_directory(old_path) {
                // Directory rename: expand to individual file renames
                let files_in_dir = self.find_files_in_directory(old_path);

                for old_file_path in files_in_dir {
                    // Compute the new path for this file
                    // Relative path within the directory
                    let relative = old_file_path
                        .strip_prefix(&rename.old_uri)
                        .unwrap_or(&old_file_path);
                    let new_file_path = new_path.join(relative);
                    let new_file_path_str = new_file_path.to_string_lossy().to_string();

                    // Process this file rename with the actual file paths (not directory)
                    self.process_file_rename(
                        Path::new(&old_file_path),
                        Path::new(&new_file_path_str),
                        &mut result,
                    );
                }
            } else {
                // Single file rename
                self.process_file_rename(old_path, new_path, &mut result);
            }
        }

        result
    }

    /// Process a single file rename (internal helper).
    ///
    /// Updates imports in all dependent files that reference the renamed file.
    /// Handles both relative specifiers (e.g. `./foo`, `../utils/bar`) and
    /// `paths`-aliased specifiers configured in the importer's nearest
    /// `tsconfig.json` (e.g. `@app/foo`).
    #[cfg(not(target_arch = "wasm32"))]
    fn process_file_rename(
        &mut self,
        old_path: &Path,
        new_path: &Path,
        result: &mut WorkspaceEdit,
    ) {
        use crate::rename::file_rename::FileRenameProvider;

        // Iterate through all files to find those that import the renamed file
        // We can't use dependency_graph.get_dependents() directly because it stores
        // raw import specifiers (e.g., "./utils/math") not resolved file paths
        for (dependent_path, dep_file) in &self.files {
            // Create a provider to find import nodes
            let provider = FileRenameProvider::new(
                dep_file.arena(),
                dep_file.line_map(),
                dep_file.source_text(),
            );

            // Find all import/export specifiers in this file
            let import_locations = provider.find_import_specifier_nodes(dep_file.root());

            // For each import, check if it needs updating
            for import_loc in import_locations {
                let Some(new_specifier) = self.compute_renamed_specifier(
                    dependent_path,
                    &import_loc.current_specifier,
                    old_path,
                    new_path,
                ) else {
                    continue;
                };
                // `import_loc.range` spans the surrounding quotes; use the
                // helper so the rewrite replaces only the inner content
                // and the original quote style is preserved.
                result.add_edit(
                    dependent_path.clone(),
                    import_loc.specifier_text_edit(new_specifier),
                );
            }
        }

        // Update the dependency graph to reflect the rename
        // Note: The dependency graph uses raw import specifiers, not resolved paths
        // So we can't directly update it here. The graph will be rebuilt when
        // files are re-parsed/re-checked in the normal workflow.
    }

    /// Compute the new module specifier when a file is renamed, preserving the
    /// importer's specifier style (relative path, `paths` alias, ...).
    ///
    /// Returns `None` when the import does not point to `old_path`, or when no
    /// supported rewrite applies (e.g. a bare npm package import unrelated to
    /// the renamed file).
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn compute_renamed_specifier(
        &self,
        dependent_path: &str,
        current_specifier: &str,
        old_path: &Path,
        new_path: &Path,
    ) -> Option<String> {
        use crate::utils::calculate_new_relative_path;

        let dependent_path_obj = Path::new(dependent_path);

        if is_relative_specifier(current_specifier) {
            if !self.is_import_pointing_to_file(dependent_path_obj, current_specifier, old_path) {
                return None;
            }
            return calculate_new_relative_path(
                dependent_path_obj,
                old_path,
                new_path,
                current_specifier,
            );
        }

        // Non-relative specifier: try tsconfig `paths` aliases. Only specifiers
        // that previously resolved to `old_path` through an alias are rewritten;
        // bare npm package specifiers are left untouched.
        self.rename_path_alias_specifier(
            dependent_path,
            current_specifier,
            &path_to_slash_string(old_path),
            &path_to_slash_string(new_path),
        )
    }

    /// Fetch a file by name.
    pub fn file(&self, file_name: &str) -> Option<&ProjectFile> {
        self.files.get(file_name)
    }

    /// Check if an import specifier points to a specific target file path.
    ///
    /// This is a simplified check that handles basic relative path resolution.
    /// It verifies if the specifier, when joined with the importer's directory,
    /// resolves to the target file path.
    ///
    /// # Arguments
    /// * `importer` - Path of the file containing the import
    /// * `specifier` - The import specifier (e.g., "./utils" or "../types")
    /// * `target` - The target file path we're checking against
    #[cfg(not(target_arch = "wasm32"))]
    fn is_import_pointing_to_file(&self, importer: &Path, specifier: &str, target: &Path) -> bool {
        let importer_dir = match importer.parent() {
            Some(p) => p,
            None => return false,
        };

        // Simple resolution: join dir + specifier
        let resolved = importer_dir.join(specifier);

        // Normalize the path by resolving .. and . components
        let normalized = self.normalize_path(&resolved);

        // Check exact match
        let target_str = target.to_string_lossy();
        if normalized == target_str {
            return true;
        }

        // Check with extensions (TypeScript resolution logic simplified)
        // The specifier might not have an extension, so we check stems
        let normalized_path = Path::new(&normalized);
        if let Some(target_stem) = target.file_stem()
            && let Some(resolved_stem) = normalized_path.file_stem()
            && target_stem == resolved_stem
        {
            // Normalize target as well for comparison
            let normalized_target = self.normalize_path(target);
            let normalized_target_path = Path::new(&normalized_target);
            // Check if parent dirs match
            if normalized_path.parent() == normalized_target_path.parent() {
                return true;
            }
        }

        false
    }

    /// Simple path normalization that resolves . and .. components without filesystem access.
    #[cfg(not(target_arch = "wasm32"))]
    fn normalize_path(&self, path: &Path) -> String {
        let path_str = path.to_string_lossy();

        // Split by / and process components
        let components: Vec<&str> = path_str.split('/').collect();
        let mut result = Vec::new();

        for component in components {
            if component == "." {
                // Skip current directory component
                continue;
            } else if component == ".." {
                // Pop from result if possible
                if !result.is_empty() && result.last() != Some(&"") {
                    result.pop();
                }
            } else {
                result.push(component);
            }
        }

        result.join("/")
    }

    /// Check if a path represents a directory (vs a file).
    ///
    /// This is a heuristic check for LSP file rename operations.
    /// In a real LSP server, you would use file system metadata, but here
    /// we check if the path exists in our project as a prefix to other files.
    #[cfg(not(target_arch = "wasm32"))]
    fn is_directory(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        let path_str_ref = path_str.as_ref();

        // Check if any file in the project has this path as a prefix
        for file_path in self.files.keys() {
            if file_path.starts_with(path_str_ref) {
                // Ensure it's a proper directory separator
                let Some(rest) = file_path.strip_prefix(path_str_ref) else {
                    continue;
                };
                if rest.starts_with('/') || rest.starts_with('\\') {
                    return true;
                }
            }
        }

        false
    }

    /// Recursively find all TypeScript files within a directory path.
    ///
    /// Returns all .ts and .tsx files that have the given directory as a prefix.
    #[cfg(not(target_arch = "wasm32"))]
    fn find_files_in_directory(&self, directory: &Path) -> Vec<String> {
        let dir_str = directory.to_string_lossy();
        let dir_str_ref = dir_str.as_ref();
        let mut result = Vec::new();

        for file_path in self.files.keys() {
            if file_path.starts_with(dir_str_ref) {
                // Check if it's a .ts or .tsx file (not a directory)
                if file_path.ends_with(".ts") || file_path.ends_with(".tsx") {
                    result.push(file_path.clone());
                }
            }
        }

        result
    }

    /// Get candidate files that might contain references to a symbol.
    ///
    /// This uses the `SymbolIndex` for O(1) lookup, turning cross-file searches
    /// from O(N) where N = all files to O(M) where M = files containing the symbol.
    ///
    /// # Arguments
    /// * `symbol_name` - The symbol name to search for
    ///
    /// # Returns
    /// A list of file paths that contain references to the symbol.
    /// Falls back to all files if the index is empty (e.g., for wildcard re-exports).
    pub(crate) fn get_candidate_files_for_symbol(&self, symbol_name: &str) -> Vec<String> {
        let candidate_files = self.symbol_index.get_files_with_symbol(symbol_name);
        if candidate_files.is_empty() {
            // Fallback to all files if index is empty
            // This handles wildcard re-exports (export * from './mod')
            self.files.keys().cloned().collect()
        } else {
            candidate_files.into_iter().collect()
        }
    }
}

impl Default for Project {
    fn default() -> Self {
        Self::empty()
    }
}

/// Check whether a file path has a TypeScript/JavaScript extension.
#[cfg(not(target_arch = "wasm32"))]
fn is_ts_js_file(path: &str) -> bool {
    let extensions = [".ts", ".tsx", ".js", ".jsx", ".mts", ".cts", ".mjs", ".cjs"];
    extensions.iter().any(|ext| path.ends_with(ext))
}

#[cfg(not(target_arch = "wasm32"))]
fn is_relative_specifier(spec: &str) -> bool {
    spec.starts_with("./") || spec.starts_with("../") || spec == "." || spec == ".."
}

#[cfg(not(target_arch = "wasm32"))]
fn path_to_slash_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(not(target_arch = "wasm32"))]
fn compile_tsconfig_include_matchers(patterns: &[String]) -> Vec<globset::GlobMatcher> {
    patterns
        .iter()
        .flat_map(|pattern| expand_tsconfig_include_pattern(pattern))
        .filter_map(|pattern| Glob::new(&pattern).ok().map(|glob| glob.compile_matcher()))
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn expand_tsconfig_include_pattern(pattern: &str) -> Vec<String> {
    let mut normalized = pattern.trim().replace('\\', "/");
    while let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped.to_string();
    }

    if normalized.is_empty() {
        return Vec::new();
    }

    if !contains_glob_meta(&normalized)
        && Path::new(&normalized)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_none()
    {
        normalized.push_str("/**/*");
    }

    let mut expanded = vec![normalized.clone()];
    let direct_child_variant = normalized.replace("/**/", "/");
    if direct_child_variant != normalized {
        expanded.push(direct_child_variant);
    }
    expanded
}

/// Parse a tsconfig.json or jsconfig.json file into `TsConfigSettings`.
#[cfg(not(target_arch = "wasm32"))]
fn parse_tsconfig_file(path: &std::path::Path) -> Option<TsConfigSettings> {
    let content = std::fs::read_to_string(path).ok()?;

    // Use json5 parser to handle comments and trailing commas
    let value: serde_json::Value = json5::from_str(&content).ok()?;
    let obj = value.as_object()?;

    let root_dir = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut settings = TsConfigSettings {
        root_dir,
        ..Default::default()
    };

    // Parse compilerOptions
    if let Some(compiler_options) = obj.get("compilerOptions").and_then(|v| v.as_object()) {
        settings.strict = compiler_options.get("strict").and_then(|v| v.as_bool());

        settings.target = compiler_options
            .get("target")
            .and_then(|v| v.as_str())
            .map(String::from);

        settings.module_resolution = compiler_options
            .get("moduleResolution")
            .and_then(|v| v.as_str())
            .map(String::from);

        settings.base_url = compiler_options
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .map(String::from);

        settings.root_dir_setting = compiler_options
            .get("rootDir")
            .and_then(|v| v.as_str())
            .map(String::from);

        settings.out_dir = compiler_options
            .get("outDir")
            .and_then(|v| v.as_str())
            .map(String::from);

        settings.allow_importing_ts_extensions = compiler_options
            .get("allowImportingTsExtensions")
            .and_then(|v| v.as_bool());

        settings.jsx = compiler_options
            .get("jsx")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Parse paths
        if let Some(paths) = compiler_options.get("paths").and_then(|v| v.as_object()) {
            for (key, val) in paths {
                if let Some(arr) = val.as_array() {
                    let mapped: Vec<String> = arr
                        .iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                    settings.paths.insert(key.clone(), mapped);
                }
            }
        }
    }

    // Parse include
    if let Some(include) = obj.get("include").and_then(|v| v.as_array()) {
        settings.include = include
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
    }

    // Parse exclude
    if let Some(exclude) = obj.get("exclude").and_then(|v| v.as_array()) {
        settings.exclude = exclude
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
    }

    Some(settings)
}
