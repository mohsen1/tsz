//! Core implementation for `TypeCache` and `CheckerContext`.
//!
//! Contains the `impl` blocks and methods extracted from `mod.rs` to keep
//! the module entry point focused on type/struct definitions.

use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::sync::Arc;

use crate::control_flow::FlowGraph;
use crate::diagnostics::{Diagnostic, diagnostic_codes};
use crate::module_resolution::module_specifier_candidates;
use tsz_binder::{BinderState, SymbolId};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_solver::TypeId;

use super::{CheckerContext, LibContext, ResolutionError, TypeCache};

impl TypeCache {
    /// Invalidate cached symbol types that depend on the provided roots.
    /// Returns the number of affected symbols.
    pub fn invalidate_symbols(&mut self, roots: &[SymbolId]) -> usize {
        if roots.is_empty() {
            return 0;
        }

        let mut reverse: FxHashMap<SymbolId, Vec<SymbolId>> = FxHashMap::default();
        for (symbol, deps) in &self.symbol_dependencies {
            for dep in deps {
                reverse.entry(*dep).or_default().push(*symbol);
            }
        }

        let mut affected: FxHashSet<SymbolId> = FxHashSet::default();
        let mut pending = VecDeque::new();
        for &root in roots {
            if affected.insert(root) {
                pending.push_back(root);
            }
        }

        while let Some(sym_id) = pending.pop_front() {
            if let Some(dependents) = reverse.get(&sym_id) {
                for &dependent in dependents {
                    if affected.insert(dependent) {
                        pending.push_back(dependent);
                    }
                }
            }
        }

        for sym_id in &affected {
            self.symbol_types.remove(sym_id);
            self.symbol_instance_types.remove(sym_id);
            self.symbol_dependencies.remove(sym_id);
        }
        self.node_types.clear();
        self.class_instance_type_cache.clear();
        self.class_constructor_type_cache.clear();
        self.class_instance_type_to_decl.clear();
        affected.len()
    }

    /// Merge another `TypeCache` into this one.
    /// Used to accumulate type information from multiple file checks for declaration emit.
    pub fn merge(&mut self, other: Self) {
        self.symbol_types.extend(other.symbol_types);
        self.symbol_instance_types
            .extend(other.symbol_instance_types);
        self.node_types.extend(other.node_types);
        self.class_instance_type_to_decl
            .extend(other.class_instance_type_to_decl);
        self.class_instance_type_cache
            .extend(other.class_instance_type_cache);
        self.class_constructor_type_cache
            .extend(other.class_constructor_type_cache);
        self.type_only_nodes.extend(other.type_only_nodes);
        self.namespace_module_names
            .extend(other.namespace_module_names);

        // Merge symbol dependencies sets
        for (sym, deps) in other.symbol_dependencies {
            self.symbol_dependencies
                .entry(sym)
                .or_default()
                .extend(deps);
        }

        // Merge def_to_symbol mapping
        self.def_to_symbol.extend(other.def_to_symbol);
    }
}

impl<'a> CheckerContext<'a> {
    /// Set lib contexts for global type resolution.
    /// Note: `lib_contexts` may include both actual lib files AND user files for cross-file
    /// resolution. Use `set_actual_lib_file_count()` to track how many are actual lib files.
    pub fn set_lib_contexts(&mut self, lib_contexts: Vec<LibContext>) {
        self.lib_contexts = lib_contexts;
    }

    /// Set the count of actual lib files loaded (not including user files).
    /// This is used by `has_lib_loaded()` to correctly determine if standard library is available.
    /// Also updates the capabilities matrix `has_lib` flag.
    pub fn set_actual_lib_file_count(&mut self, count: usize) {
        self.actual_lib_file_count = count;
        // Update the precomputed capabilities matrix
        let has_lib = !self.compiler_options.no_lib && count > 0;
        self.capabilities.has_lib = has_lib;
    }

    /// Record whether a project-local `@typescript/lib-dom` replacement package was loaded
    /// and which common globals it explicitly provides.
    pub const fn set_typescript_dom_replacement_globals(
        &mut self,
        loaded: bool,
        has_window: bool,
        has_self: bool,
    ) {
        self.typescript_dom_replacement_loaded = loaded;
        self.typescript_dom_replacement_has_window = has_window;
        self.typescript_dom_replacement_has_self = has_self;
    }

    /// Set all arenas for cross-file resolution.
    pub fn set_all_arenas(&mut self, arenas: Arc<Vec<Arc<NodeArena>>>) {
        // Build module specifiers map from arena file names.
        // Each file (other than the current file) gets its name stem as the module specifier.
        // This enables import-qualified type display like `import("a").F`.
        self.module_specifiers = Self::build_module_specifiers(&arenas);
        self.all_arenas = Some(arenas);
    }

    /// Build a mapping from `file_id` -> module specifier for import-qualified type display.
    /// Returns `file_idx -> stem` for each source file in the arenas.
    fn build_module_specifiers(arenas: &[Arc<NodeArena>]) -> FxHashMap<u32, String> {
        let mut map = FxHashMap::default();
        for (idx, arena) in arenas.iter().enumerate() {
            for sf in &arena.source_files {
                let file_name = &sf.file_name;
                // Strip .ts/.tsx/.d.ts/.js/.jsx extension to get the module specifier
                let specifier = Self::strip_ts_extension(file_name);
                // Use just the filename component (without directory path) to match tsc's
                // diagnostic display. tsc shows `import("a").F` not `import("/full/path/a").F`.
                let basename = specifier
                    .rsplit_once('/')
                    .map(|(_, name)| name)
                    .unwrap_or(specifier);
                map.insert(idx as u32, basename.to_string());
            }
        }
        map
    }

    /// Strip TypeScript/JavaScript extension from a file path to get the module specifier.
    fn strip_ts_extension(path: &str) -> &str {
        // Check extensions in order: longer extensions first to avoid partial matches
        for ext in &[
            ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx",
            ".mjs", ".cjs",
        ] {
            if let Some(stripped) = path.strip_suffix(ext) {
                return stripped;
            }
        }
        path
    }

    /// Set all binders for cross-file resolution.
    pub fn set_all_binders(&mut self, binders: Arc<Vec<Arc<BinderState>>>) {
        self.all_binders = Some(binders);
    }

    /// Set resolved module paths map for cross-file import resolution.
    pub fn set_resolved_module_paths(&mut self, paths: Arc<FxHashMap<(usize, String), usize>>) {
        self.resolved_module_paths = Some(paths);
    }

    /// Set resolved module specifiers (module names that exist in the project).
    /// Used to suppress TS2307 errors for known modules.
    pub fn set_resolved_modules(&mut self, modules: FxHashSet<String>) {
        self.resolved_modules = Some(modules);
    }

    /// Set resolved module errors map for cross-file import resolution.
    /// Populated by the driver when `ModuleResolver` returns specific errors (TS2834, TS2835, TS2792, etc.).
    pub fn set_resolved_module_errors(
        &mut self,
        errors: Arc<FxHashMap<(usize, String), ResolutionError>>,
    ) {
        self.resolved_module_errors = Some(errors);
    }

    /// Get the resolution error for a specifier, if any.
    /// Returns the specific error (TS2834, TS2835, TS2792, etc.) if the module resolution failed with a known error.
    pub fn get_resolution_error(&self, specifier: &str) -> Option<&ResolutionError> {
        let errors = self.resolved_module_errors.as_ref()?;

        for candidate in module_specifier_candidates(specifier) {
            if let Some(error) = errors.get(&(self.current_file_idx, candidate)) {
                return Some(error);
            }
        }
        None
    }

    /// Set the current file index.
    pub const fn set_current_file_idx(&mut self, idx: usize) {
        self.current_file_idx = idx;
    }

    /// Get the arena for a specific file index.
    /// Returns the current arena if `file_idx` is `u32::MAX` (single-file mode).
    pub fn get_arena_for_file(&self, file_idx: u32) -> &NodeArena {
        if file_idx == u32::MAX {
            return self.arena;
        }
        if let Some(arenas) = self.all_arenas.as_ref()
            && let Some(arena) = arenas.get(file_idx as usize)
        {
            return arena.as_ref();
        }
        self.arena
    }

    /// Get the binder for a specific file index.
    /// Returns None if `file_idx` is out of bounds or `all_binders` is not set.
    pub fn get_binder_for_file(&self, file_idx: usize) -> Option<&BinderState> {
        self.all_binders
            .as_ref()
            .and_then(|binders| binders.get(file_idx))
            .map(Arc::as_ref)
    }

    /// Get the binder that owns a specific arena.
    ///
    /// This is used when cross-file resolution discovers a declaration arena
    /// directly (via `symbol_arenas` / `declaration_arenas`) without already
    /// knowing the originating file index.
    pub fn get_binder_for_arena(&self, arena: &NodeArena) -> Option<&BinderState> {
        let arenas = self.all_arenas.as_ref()?;
        let binders = self.all_binders.as_ref()?;
        let arena_ptr = arena as *const NodeArena;

        arenas.iter().enumerate().find_map(|(idx, candidate)| {
            (Arc::as_ptr(candidate) == arena_ptr)
                .then(|| binders.get(idx).map(Arc::as_ref))
                .flatten()
        })
    }

    /// Get the file index that owns a specific arena.
    ///
    /// This keeps delegated child contexts aligned with the declaring file when
    /// cross-file resolution discovers an arena directly from declaration metadata.
    pub fn get_file_idx_for_arena(&self, arena: &NodeArena) -> Option<usize> {
        let arenas = self.all_arenas.as_ref()?;
        let arena_ptr = arena as *const NodeArena;

        arenas
            .iter()
            .enumerate()
            .find_map(|(idx, candidate)| (Arc::as_ptr(candidate) == arena_ptr).then_some(idx))
    }

    /// Resolve an import specifier to its target file index.
    /// Uses the `resolved_module_paths` map populated by the driver.
    /// Returns None if the import cannot be resolved (e.g., external module).
    pub fn resolve_import_target(&self, specifier: &str) -> Option<usize> {
        self.resolve_import_target_from_file(self.current_file_idx, specifier)
    }

    /// Resolve an import specifier from a specific file to its target file index.
    /// Like `resolve_import_target` but for any source file, not just the current one.
    pub fn resolve_import_target_from_file(
        &self,
        source_file_idx: usize,
        specifier: &str,
    ) -> Option<usize> {
        if let Some(paths) = self.resolved_module_paths.as_ref() {
            for candidate in module_specifier_candidates(specifier) {
                if let Some(target_idx) = paths.get(&(source_file_idx, candidate)) {
                    return Some(*target_idx);
                }
            }
        }

        let arenas = self.all_arenas.as_ref()?;
        let file_names: Vec<String> = arenas
            .iter()
            .filter_map(|arena| arena.source_files.first().map(|sf| sf.file_name.clone()))
            .collect();
        let (fallback_paths, _) =
            crate::module_resolution::build_module_resolution_maps(&file_names);
        for candidate in module_specifier_candidates(specifier) {
            if let Some(target_idx) = fallback_paths.get(&(source_file_idx, candidate)) {
                return Some(*target_idx);
            }
        }
        None
    }

    /// Resolve a member exported by the target module of an ALIAS symbol.
    ///
    /// When an ALIAS symbol's `import_module` holds a relative specifier
    /// (e.g., `"./Something"`), it must be resolved from the ALIAS's source
    /// file, not the current file.  This helper uses `cross_file_symbol_targets`
    /// to find the ALIAS's origin file, resolves the specifier from that file's
    /// perspective, then looks up the member in the target module's exports.
    pub fn resolve_alias_import_member(
        &self,
        alias_id: tsz_binder::SymbolId,
        module_specifier: &str,
        member_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let source_file_idx = self
            .cross_file_symbol_targets
            .borrow()
            .get(&alias_id)
            .copied()?;
        let target_idx = self.resolve_import_target_from_file(source_file_idx, module_specifier)?;
        let target_binder = self.get_binder_for_file(target_idx)?;
        let target_arena = self.get_arena_for_file(target_idx as u32);
        let file_name = &target_arena.source_files.first()?.file_name;
        // Use the target binder's own re-export resolution (handles
        // direct exports, named re-exports, and wildcard re-exports).
        target_binder
            .resolve_import_with_reexports_type_only(file_name, member_name)
            .map(|(sym_id, _)| {
                self.cross_file_symbol_targets
                    .borrow_mut()
                    .insert(sym_id, target_idx);
                sym_id
            })
    }

    /// Follow an import alias to its actual target symbol across file boundaries.
    ///
    /// For ALIAS symbols (created by `import {A} from "./file"`), resolves
    /// the module specifier from the alias's source file, then looks up the
    /// exported name in the target file's binder. Returns None if the symbol
    /// is not an alias or resolution fails.
    ///
    /// This is a pure lookup — it does NOT register the result in
    /// `cross_file_symbol_targets`. Callers that need cross-arena delegation
    /// (e.g., lazy type resolution) should call [`resolve_import_alias_and_register`]
    /// instead.
    pub fn resolve_import_alias(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<tsz_binder::SymbolId> {
        let symbol = self.binder.symbols.get(sym_id).or_else(|| {
            self.all_binders
                .as_ref()
                .and_then(|bs| bs.iter().find_map(|b| b.symbols.get(sym_id)))
        })?;

        if (symbol.flags & tsz_binder::symbol_flags::ALIAS) == 0 {
            return None;
        }
        let module_specifier = symbol.import_module.as_ref()?;
        let import_name = symbol.import_name.as_ref().unwrap_or(&symbol.escaped_name);

        let source_file_idx = symbol.decl_file_idx as usize;
        if let Some(target_idx) =
            self.resolve_import_target_from_file(source_file_idx, module_specifier)
        {
            let target_binder = self.get_binder_for_file(target_idx)?;
            return target_binder.file_locals.get(import_name);
        }

        // Fallback: check ambient module exports (declare module "X" { ... }).
        // These are keyed by the module specifier in binder.module_exports.
        self.resolve_import_from_ambient_module(module_specifier, import_name)
    }

    /// Like [`resolve_import_alias`], but also registers the resolved symbol in
    /// `cross_file_symbol_targets` so that `delegate_cross_arena_symbol_resolution`
    /// can create a child checker with the correct arena when computing its type.
    pub fn resolve_import_alias_and_register(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<tsz_binder::SymbolId> {
        let symbol = self.binder.symbols.get(sym_id).or_else(|| {
            self.all_binders
                .as_ref()
                .and_then(|bs| bs.iter().find_map(|b| b.symbols.get(sym_id)))
        })?;

        if (symbol.flags & tsz_binder::symbol_flags::ALIAS) == 0 {
            return None;
        }
        let module_specifier = symbol.import_module.as_ref()?;
        let import_name = symbol.import_name.as_ref().unwrap_or(&symbol.escaped_name);

        let source_file_idx = symbol.decl_file_idx as usize;
        if let Some(target_idx) =
            self.resolve_import_target_from_file(source_file_idx, module_specifier)
        {
            let target_binder = self.get_binder_for_file(target_idx)?;
            let result = target_binder.file_locals.get(import_name)?;
            self.cross_file_symbol_targets
                .borrow_mut()
                .insert(result, target_idx);
            return Some(result);
        }

        // Fallback: check ambient module exports (declare module "X" { ... }).
        // These are keyed by the module specifier in binder.module_exports.
        // For ambient modules, the symbol lives in the same binder that declared
        // the module, so we also register it in cross_file_symbol_targets with
        // the declaring file's index for proper cross-arena delegation.
        if let Some((result, file_idx)) =
            self.resolve_import_from_ambient_module_with_file_idx(module_specifier, import_name)
        {
            self.cross_file_symbol_targets
                .borrow_mut()
                .insert(result, file_idx);
            return Some(result);
        }
        None
    }

    /// Resolve an import name from ambient module exports (`declare module "X" { ... }`).
    ///
    /// When file-based module resolution fails (the module specifier doesn't correspond
    /// to any file), this fallback checks `module_exports` in the current binder and
    /// all cross-file binders. Ambient module declarations populate `module_exports`
    /// keyed by their string-literal module specifier (e.g., `"A"` for `declare module "A"`).
    fn resolve_import_from_ambient_module(
        &self,
        module_specifier: &str,
        import_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        if let Some(exports) = self.binder.module_exports.get(module_specifier)
            && let Some(sym_id) = exports.get(import_name)
        {
            return Some(sym_id);
        }
        if let Some(all_binders) = self.all_binders.as_ref() {
            for binder in all_binders.iter() {
                if let Some(exports) = binder.module_exports.get(module_specifier)
                    && let Some(sym_id) = exports.get(import_name)
                {
                    return Some(sym_id);
                }
            }
        }
        None
    }

    /// Like [`resolve_import_from_ambient_module`] but also returns the file index
    /// of the binder that owns the resolved symbol, for `cross_file_symbol_targets`
    /// registration.
    fn resolve_import_from_ambient_module_with_file_idx(
        &self,
        module_specifier: &str,
        import_name: &str,
    ) -> Option<(tsz_binder::SymbolId, usize)> {
        if let Some(exports) = self.binder.module_exports.get(module_specifier)
            && let Some(sym_id) = exports.get(import_name)
        {
            return Some((sym_id, self.current_file_idx));
        }
        if let Some(all_binders) = self.all_binders.as_ref() {
            for (idx, binder) in all_binders.iter().enumerate() {
                if let Some(exports) = binder.module_exports.get(module_specifier)
                    && let Some(sym_id) = exports.get(import_name)
                {
                    return Some((sym_id, idx));
                }
            }
        }
        None
    }

    /// Extract the persistent cache from this context.
    /// This allows saving type checking results for future queries.
    pub fn extract_cache(self) -> TypeCache {
        TypeCache {
            symbol_types: self.symbol_types,
            symbol_instance_types: self.symbol_instance_types,
            node_types: self.node_types,
            symbol_dependencies: self.symbol_dependencies,
            def_to_symbol: self.def_to_symbol.into_inner(),
            flow_analysis_cache: self.flow_analysis_cache.into_inner(),
            class_instance_type_to_decl: self.class_instance_type_to_decl,
            class_instance_type_cache: self.class_instance_type_cache,
            class_constructor_type_cache: self.class_constructor_type_cache,
            type_only_nodes: self.type_only_nodes,
            namespace_module_names: self.namespace_module_names,
        }
    }

    fn diagnostic_dedup_key_from_parts(&self, start: u32, code: u32, message: &str) -> (u32, u32) {
        if code == 2318 && start == 0 {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            message.hash(&mut hasher);
            (hasher.finish() as u32, code)
        } else if code == 2411 || code == 2430 || code == 2536 {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            message.hash(&mut hasher);
            (start ^ (hasher.finish() as u32), code)
        } else {
            (start, code)
        }
    }

    pub fn diagnostic_dedup_key(&self, diag: &Diagnostic) -> (u32, u32) {
        self.diagnostic_dedup_key_from_parts(diag.start, diag.code, &diag.message_text)
    }

    pub fn rebuild_emitted_diagnostics_from_current(&mut self) {
        self.emitted_diagnostics.clear();
        for diag in &self.diagnostics {
            let key = self.diagnostic_dedup_key(diag);
            self.emitted_diagnostics.insert(key);
        }
    }

    /// Add an error diagnostic (with deduplication).
    /// Diagnostics with the same (start, code) are only emitted once.
    /// Exceptions:
    /// - TS2411 uses (start ^ `message_hash`, code) to allow a single property to
    ///   fail against both string and number index signatures at the same span.
    /// - TS2430 uses (start ^ `message_hash`, code) to allow multiple
    ///   "incorrectly extends" errors at the same interface name when an interface
    ///   incompatibly extends several distinct bases.
    /// - TS2536 uses the same scheme so nested indexed-access failures can report
    ///   multiple distinct messages at the same indexed-access start.
    pub fn error(&mut self, start: u32, length: u32, message: String, code: u32) {
        // TS2304 ("Cannot find name") and TS2552 ("Cannot find name ... Did you mean?")
        // are suppressed when TS2301 already exists at the same position, since TS2301
        // ("Initializer of instance member cannot reference identifier declared in constructor")
        // already explains the problem.
        if (code == 2304 || code == 2552)
            && self
                .diagnostics
                .iter()
                .any(|diag| diag.start == start && diag.code == 2301)
        {
            return;
        }
        if code == 2301 {
            self.diagnostics
                .retain(|diag| !(diag.start == start && (diag.code == 2304 || diag.code == 2552)));
            self.emitted_diagnostics.remove(&(start, 2304));
            self.emitted_diagnostics.remove(&(start, 2552));
        }

        // TS2304 and TS2552 are mutually exclusive at the same position.
        // TS2552 (with spelling suggestion) takes priority over TS2304 (without).
        // Multiple code paths can emit these for the same unresolved name.
        if code == 2304 && self.emitted_diagnostics.contains(&(start, 2552)) {
            return;
        }
        if code == 2552 {
            self.diagnostics
                .retain(|diag| !(diag.start == start && diag.code == 2304));
            self.emitted_diagnostics.remove(&(start, 2304));
        }

        // Check if we've already emitted this diagnostic
        let key = self.diagnostic_dedup_key_from_parts(start, code, &message);
        if self.emitted_diagnostics.contains(&key) {
            return;
        }
        self.emitted_diagnostics.insert(key);
        tracing::debug!(
            code,
            start,
            length,
            file = %self.file_name,
            message = %message,
            "diagnostic"
        );
        self.diagnostics.push(Diagnostic::error(
            self.file_name.clone(),
            start,
            length,
            message,
            code,
        ));
    }

    /// Push a diagnostic with deduplication.
    /// Diagnostics with the same (start, code) are only emitted once.
    /// Exceptions:
    /// - TS2318 (missing global type) at position 0 uses message hash to allow multiple distinct
    ///   global type errors.
    /// - TS2411 uses (start ^ `message_hash`, code) to allow a single property to
    ///   report both string and number index incompatibilities.
    /// - TS2430 (incorrectly extends interface) uses (start ^ `message_hash`, code) to allow
    ///   multiple per-base diagnostics at the same interface name position.
    pub fn push_diagnostic(&mut self, diag: Diagnostic) {
        if diag.code == 2304
            && self
                .diagnostics
                .iter()
                .any(|existing| existing.start == diag.start && existing.code == 2301)
        {
            return;
        }
        if diag.code == 2301 {
            self.diagnostics
                .retain(|existing| !(existing.start == diag.start && existing.code == 2304));
            self.emitted_diagnostics.remove(&(diag.start, 2304));
        }
        // TS2304 and TS2552 are mutually exclusive at the same position.
        // TS2552 (with spelling suggestion) takes priority over TS2304 (without).
        if diag.code == 2304 && self.emitted_diagnostics.contains(&(diag.start, 2552)) {
            return;
        }
        if diag.code == 2552 {
            self.diagnostics
                .retain(|existing| !(existing.start == diag.start && existing.code == 2304));
            self.emitted_diagnostics.remove(&(diag.start, 2304));
        }
        if diag.code == 2322 {
            let diag_end = diag.start.saturating_add(diag.length);
            // TS2353/TS2561 on a property inside an object literal should suppress
            // a redundant enclosing TS2322 on the whole literal.
            if self.diagnostics.iter().any(|existing| {
                (existing.code
                    == diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
                    || existing.code
                        == diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID)
                    && existing.start >= diag.start
                    && existing.start < diag_end
            }) {
                return;
            }
        }

        let key = self.diagnostic_dedup_key(&diag);

        if self.emitted_diagnostics.contains(&key) {
            return;
        }
        self.emitted_diagnostics.insert(key);
        tracing::debug!(
            code = diag.code,
            start = diag.start,
            length = diag.length,
            file = %diag.file,
            message = %diag.message_text,
            "diagnostic"
        );
        self.diagnostics.push(diag);
    }

    /// Get node span (pos, end) from index.
    pub fn get_node_span(&self, idx: NodeIndex) -> Option<(u32, u32)> {
        let node = self.arena.get(idx)?;
        Some((node.pos, node.end))
    }

    /// Push an expected return type onto the stack.
    pub fn push_return_type(&mut self, return_type: TypeId) {
        self.return_type_stack.push(return_type);
    }

    /// Pop the expected return type from the stack.
    pub fn pop_return_type(&mut self) {
        self.return_type_stack.pop();
    }

    /// Get the current expected return type.
    pub fn current_return_type(&self) -> Option<TypeId> {
        self.return_type_stack.last().copied()
    }

    /// Push a contextual yield type for a generator function.
    pub fn push_yield_type(&mut self, yield_type: Option<TypeId>) {
        self.yield_type_stack.push(yield_type);
    }

    /// Pop the contextual yield type from the stack.
    pub fn pop_yield_type(&mut self) {
        self.yield_type_stack.pop();
    }

    /// Get the current contextual yield type for the enclosing generator.
    pub fn current_yield_type(&self) -> Option<TypeId> {
        self.yield_type_stack.last().copied().flatten()
    }

    pub fn push_generator_next_type(&mut self, next_type: Option<TypeId>) {
        self.generator_next_type_stack.push(next_type);
    }

    pub fn pop_generator_next_type(&mut self) {
        self.generator_next_type_stack.pop();
    }

    pub fn current_generator_next_type(&self) -> Option<TypeId> {
        self.generator_next_type_stack.last().copied().flatten()
    }

    /// Enter an async context (increment async depth).
    pub const fn enter_async_context(&mut self) {
        self.async_depth += 1;
    }

    /// Exit an async context (decrement async depth).
    pub const fn exit_async_context(&mut self) {
        if self.async_depth > 0 {
            self.async_depth -= 1;
        }
    }

    /// Check if we're currently inside an async function.
    pub const fn in_async_context(&self) -> bool {
        self.async_depth > 0
    }

    /// Consume one unit of type resolution fuel.
    /// Returns true if fuel is still available, false if exhausted.
    /// When exhausted, type resolution should return ERROR to prevent timeout.
    /// Also tracks a thread-local global fuel counter that is NOT reset when
    /// child contexts are created for cross-arena delegation, preventing
    /// unbounded total work across multiple contexts.
    pub fn consume_fuel(&self) -> bool {
        let fuel = self.type_resolution_fuel.get();
        if fuel == 0 {
            return false;
        }
        self.type_resolution_fuel.set(fuel - 1);
        // Thread-local global fuel prevents OOM when child contexts each get
        // fresh per-context fuel (cross-arena delegation). This is the only
        // fuel counter that survives context boundaries.
        if crate::state_domain::type_environment::lazy::global_resolution_fuel_exhausted() {
            return false;
        }
        crate::state_domain::type_environment::lazy::increment_global_resolution_fuel();
        true
    }

    /// Enter a recursive call. Returns true if recursion is allowed,
    /// false if the depth limit has been reached (caller should bail out).
    #[inline]
    pub fn enter_recursion(&self) -> bool {
        self.recursion_depth.borrow_mut().enter()
    }

    /// Leave a recursive call (decrement depth counter).
    #[inline]
    pub fn leave_recursion(&self) {
        self.recursion_depth.borrow_mut().leave();
    }

    // =========================================================================
    // Flow Graph Queries
    // =========================================================================

    /// Check flow usage at a specific AST node.
    ///
    /// This method queries the control flow graph to determine flow-sensitive
    /// information at a given node. Returns `None` if flow graph is not available.
    ///
    /// # Arguments
    /// * `node_idx` - The AST node to query flow information for
    ///
    /// # Returns
    /// * `Some(FlowNodeId)` - The flow node ID at this location
    /// * `None` - If flow graph is not available or node has no flow info
    pub fn check_flow_usage(&self, node_idx: NodeIndex) -> Option<tsz_binder::FlowNodeId> {
        if let Some(ref _graph) = self.flow_graph {
            // Look up the flow node for this AST node from the binder's node_flow mapping
            self.binder.node_flow.get(&node_idx.0).copied()
        } else {
            None
        }
    }

    /// Get a reference to the flow graph.
    pub const fn flow_graph(&self) -> Option<&FlowGraph<'a>> {
        self.flow_graph.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::TypeCache;
    use rustc_hash::{FxHashMap, FxHashSet};
    use tsz_binder::SymbolId;
    use tsz_parser::parser::NodeIndex;
    use tsz_solver::TypeId;

    fn empty_cache() -> TypeCache {
        TypeCache {
            symbol_types: FxHashMap::default(),
            symbol_instance_types: FxHashMap::default(),
            node_types: FxHashMap::default(),
            symbol_dependencies: FxHashMap::default(),
            def_to_symbol: FxHashMap::default(),
            flow_analysis_cache: FxHashMap::default(),
            class_instance_type_to_decl: FxHashMap::default(),
            class_instance_type_cache: FxHashMap::default(),
            class_constructor_type_cache: FxHashMap::default(),
            type_only_nodes: FxHashSet::default(),
            namespace_module_names: FxHashMap::default(),
        }
    }

    #[test]
    fn type_cache_merge_keeps_constructor_type_cache() {
        let mut lhs = empty_cache();
        let mut rhs = empty_cache();

        rhs.class_constructor_type_cache
            .insert(NodeIndex(42), TypeId::STRING);

        lhs.merge(rhs);

        assert_eq!(
            lhs.class_constructor_type_cache.get(&NodeIndex(42)),
            Some(&TypeId::STRING)
        );
    }

    #[test]
    fn type_cache_merge_keeps_error_class_type_cache_entries() {
        let mut lhs = empty_cache();
        let mut rhs = empty_cache();

        rhs.class_instance_type_cache
            .insert(NodeIndex(10), TypeId::ERROR);
        rhs.class_constructor_type_cache
            .insert(NodeIndex(11), TypeId::ERROR);

        lhs.merge(rhs);

        assert_eq!(
            lhs.class_instance_type_cache.get(&NodeIndex(10)),
            Some(&TypeId::ERROR)
        );
        assert_eq!(
            lhs.class_constructor_type_cache.get(&NodeIndex(11)),
            Some(&TypeId::ERROR)
        );
    }

    #[test]
    fn invalidate_symbols_clears_class_type_caches() {
        let mut cache = empty_cache();
        let sym = SymbolId(7);
        cache
            .symbol_dependencies
            .insert(sym, FxHashSet::<SymbolId>::default());
        cache
            .class_instance_type_cache
            .insert(NodeIndex(1), TypeId::NUMBER);
        cache
            .class_constructor_type_cache
            .insert(NodeIndex(2), TypeId::STRING);
        cache
            .class_instance_type_to_decl
            .insert(TypeId::BOOLEAN, NodeIndex(3));

        let affected = cache.invalidate_symbols(&[sym]);

        assert_eq!(affected, 1);
        assert!(cache.class_instance_type_cache.is_empty());
        assert!(cache.class_constructor_type_cache.is_empty());
        assert!(cache.class_instance_type_to_decl.is_empty());
    }
}
