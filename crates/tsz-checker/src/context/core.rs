//! Core implementation for `TypeCache` and `CheckerContext`.
//!
//! Contains the `impl` blocks and methods extracted from `mod.rs` to keep
//! the module entry point focused on type/struct definitions.

use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::sync::Arc;

use crate::control_flow::FlowGraph;
use crate::diagnostics::Diagnostic;
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
    pub const fn set_actual_lib_file_count(&mut self, count: usize) {
        self.actual_lib_file_count = count;
    }

    /// Set all arenas for cross-file resolution.
    pub fn set_all_arenas(&mut self, arenas: Arc<Vec<Arc<NodeArena>>>) {
        self.all_arenas = Some(arenas);
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
        let paths = self.resolved_module_paths.as_ref()?;
        for candidate in module_specifier_candidates(specifier) {
            if let Some(target_idx) = paths.get(&(source_file_idx, candidate)) {
                return Some(*target_idx);
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
        }
    }

    /// Add an error diagnostic (with deduplication).
    /// Diagnostics with the same (start, code) are only emitted once.
    pub fn error(&mut self, start: u32, length: u32, message: String, code: u32) {
        // Check if we've already emitted this diagnostic
        let key = (start, code);
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
    /// Exception: TS2318 (missing global type) at position 0 uses message hash
    /// to allow multiple distinct global type errors.
    pub fn push_diagnostic(&mut self, diag: Diagnostic) {
        // For TS2318 at position 0, include message hash in key to allow distinct errors
        // (e.g., "Cannot find global type 'Array'" vs "Cannot find global type 'Object'")
        let key = if diag.code == 2318 && diag.start == 0 {
            // Use a hash of the message to distinguish different TS2318 errors
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            diag.message_text.hash(&mut hasher);
            (hasher.finish() as u32, diag.code)
        } else {
            (diag.start, diag.code)
        };

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
    pub fn consume_fuel(&self) -> bool {
        let mut fuel = self.type_resolution_fuel.borrow_mut();
        if *fuel == 0 {
            return false;
        }
        *fuel -= 1;
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
