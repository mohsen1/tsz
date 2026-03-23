//! DefId migration helpers for `CheckerContext`.
//!
//! Handles bidirectional mapping between `SymbolId` and `DefId`, lazy type
//! references, type parameter registration, and resolved-type registration
//! in the `TypeEnvironment`.

use tracing::trace;
use tsz_binder::SymbolId;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;

use crate::context::CheckerContext;

impl<'a> CheckerContext<'a> {
    /// Get or create a `DefId` for a symbol.
    ///
    /// If the symbol already has a `DefId`, return it.
    /// Otherwise, create a new `DefId` and store the mapping.
    ///
    /// This is used during the migration from `SymbolRef` to `DefId`.
    /// Eventually, all type references will use `DefId` directly.
    ///
    /// ## Lookup strategy
    ///
    /// 1. **Local cache** (`symbol_to_def`): O(1) `FxHashMap` lookup, no locking.
    /// 2. **Authoritative index** (`DefinitionStore::symbol_def_index`): O(1)
    ///    `DashMap` lookup keyed by `(symbol_id, file_idx)`. This naturally
    ///    disambiguates the same raw `SymbolId(u32)` across different binders
    ///    and eliminates the expensive multi-binder name-validation that was
    ///    previously done on every cache hit.
    /// 3. **Create**: look up the symbol, build `DefinitionInfo`, register in
    ///    both the store and the index.
    pub fn get_or_create_def_id(&self, sym_id: SymbolId) -> DefId {
        use tsz_solver::def::DefinitionInfo;

        // ---- Step 1: local cache fast path ----
        if let Some(def_id) = self.symbol_to_def.borrow().get(&sym_id).copied() {
            return def_id;
        }

        // ---- Step 2: authoritative symbol-only index (O(1)) ----
        // Check the DefinitionStore's symbol_only_index before doing any binder
        // lookups. This avoids O(N) lib_contexts/all_binders scans for symbols
        // that already have DefIds from pre-population or previous contexts.
        if let Some(def_id) = self.definition_store.find_def_by_symbol(sym_id.0) {
            // Populate local caches for future fast-path hits.
            self.symbol_to_def.borrow_mut().insert(sym_id, def_id);
            self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
            return def_id;
        }

        // ---- Step 3: look up the symbol to get its file_idx ----
        // We need the symbol to determine which binder it came from.
        // This O(N) scan only runs for truly new DefIds (not yet in DefinitionStore).
        let symbol = self
            .binder
            .symbols
            .get(sym_id)
            .or_else(|| {
                self.lib_contexts
                    .iter()
                    .find_map(|lib_ctx| lib_ctx.binder.symbols.get(sym_id))
            })
            .or_else(|| {
                self.all_binders.as_ref().and_then(|binders| {
                    binders.iter().find_map(|binder| binder.symbols.get(sym_id))
                })
            });

        let symbol = match symbol {
            Some(s) => s,
            None => return DefId::INVALID,
        };

        let file_idx = symbol.decl_file_idx;

        // ---- Step 3b: composite key lookup ----
        // The composite key (symbol_id, file_idx) uniquely identifies a symbol
        // across all binders, so no name-validation is needed.
        if let Some(def_id) = self.definition_store.lookup_by_symbol(sym_id.0, file_idx) {
            // Populate local caches for future fast-path hits.
            self.symbol_to_def.borrow_mut().insert(sym_id, def_id);
            self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
            return def_id;
        }

        // ---- Step 4: create new DefId ----
        let name = self.types.intern_string(&symbol.escaped_name);

        // Determine DefKind from symbol flags.
        // CLASS is checked before INTERFACE because declaration merging can give
        // a symbol both flags (e.g., `class Component<P,S>` + interface augmentation).
        // A class-with-interface-merge is semantically still a class.
        let kind = if (symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0 {
            tsz_solver::def::DefKind::TypeAlias
        } else if (symbol.flags & tsz_binder::symbol_flags::CLASS) != 0 {
            tsz_solver::def::DefKind::Class
        } else if (symbol.flags & tsz_binder::symbol_flags::INTERFACE) != 0 {
            tsz_solver::def::DefKind::Interface
        } else if (symbol.flags & tsz_binder::symbol_flags::ENUM) != 0 {
            tsz_solver::def::DefKind::Enum
        } else if (symbol.flags
            & (tsz_binder::symbol_flags::NAMESPACE_MODULE | tsz_binder::symbol_flags::VALUE_MODULE))
            != 0
        {
            tsz_solver::def::DefKind::Namespace
        } else if (symbol.flags & tsz_binder::symbol_flags::FUNCTION) != 0 {
            tsz_solver::def::DefKind::Function
        } else if (symbol.flags
            & (tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE
                | tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE))
            != 0
        {
            tsz_solver::def::DefKind::Variable
        } else {
            // Default to TypeAlias for remaining symbols (type parameters, etc.)
            tsz_solver::def::DefKind::TypeAlias
        };

        // Create a placeholder DefinitionInfo - body will be set lazily
        // Get span from the first declaration if available
        let span = symbol.declarations.first().map(|n| (n.0, n.0));

        let info = DefinitionInfo {
            kind,
            name,
            type_params: Vec::new(), // Will be populated when type is resolved
            body: None,              // Lazy: computed on first access
            instance_shape: None,
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members: Vec::new(),
            exports: Vec::new(), // Will be populated for namespaces/modules
            file_id: Some(file_idx),
            span,
            symbol_id: Some(sym_id.0),
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
        };

        let def_id = self.definition_store.register(info);
        trace!(
            symbol_name = %symbol.escaped_name,
            symbol_id = %sym_id.0,
            def_id = %def_id.0,
            kind = ?kind,
            "DefId fallback: created new DefId on demand (not pre-populated)"
        );

        // Track fallback firings for observability. If this counter grows
        // unexpectedly, it indicates binder semantic_defs coverage gaps.
        self.def_fallback_count
            .set(self.def_fallback_count.get() + 1);

        // Register in the authoritative index (shared across contexts).
        self.definition_store
            .register_symbol_mapping(sym_id.0, file_idx, def_id);

        // Populate local caches.
        self.symbol_to_def.borrow_mut().insert(sym_id, def_id);
        self.def_to_symbol.borrow_mut().insert(def_id, sym_id);

        // Propagate DefKind to both TypeEnvironments so both the evaluator
        // and flow-analyzer can query it.
        self.register_def_kind_in_envs(def_id, kind);

        def_id
    }

    /// Get or create a `DefId` for a lib symbol.
    ///
    /// Lib symbols *should* already have `DefIds` from pre-population
    /// (`pre_populate_def_ids_from_lib_binders`). This method first checks
    /// the pre-populated index and only falls back to `get_or_create_def_id`
    /// as a safety net, logging a trace when the fallback fires.
    ///
    /// Use this instead of the manual `get_existing_def_id().unwrap_or_else(||
    /// get_or_create_def_id())` pattern in lib resolution paths.
    pub fn get_lib_def_id(&self, sym_id: SymbolId) -> DefId {
        if let Some(def_id) = self.get_existing_def_id(sym_id) {
            return def_id;
        }
        // Pre-population missed this symbol — create on demand but log it.
        // If this fires frequently for a specific symbol kind, the binder's
        // `record_semantic_def` coverage should be extended.
        trace!(
            symbol_id = %sym_id.0,
            "lib symbol not pre-populated, creating DefId on demand"
        );
        self.get_or_create_def_id(sym_id)
    }

    /// Return the canonical `SymbolId` for a lib symbol name.
    ///
    /// Prefers the main (merged) binder's `file_locals` entry because that
    /// identity is what `DefId`s are keyed to after `merge_lib_contexts_into_binder`.
    /// Falls back to `per_lib_sym_id` (from an individual lib context binder)
    /// only when the main binder doesn't carry the symbol yet — a scenario that
    /// can happen with lazily-loaded or target-gated lib files.
    ///
    /// Callers should use this instead of the inline `main_sym_id.unwrap_or(sym_id)`
    /// recovery pattern.
    pub fn canonical_lib_sym_id(&self, name: &str, per_lib_sym_id: SymbolId) -> SymbolId {
        self.binder.file_locals.get(name).unwrap_or(per_lib_sym_id)
    }

    /// Return the `DefId` for a lib symbol, canonicalizing the `SymbolId` first.
    ///
    /// Combines [`canonical_lib_sym_id`] and [`get_lib_def_id`] into a single
    /// call. Use this in per-lib-context lowering paths (e.g.,
    /// `resolve_lib_type_with_params`) where the `SymbolId` comes from an
    /// individual lib binder and must be mapped to the merged-binder identity
    /// before creating/looking up the `DefId`.
    pub fn get_canonical_lib_def_id(&self, name: &str, per_lib_sym_id: SymbolId) -> DefId {
        let canonical_sym = self.canonical_lib_sym_id(name, per_lib_sym_id);
        self.get_lib_def_id(canonical_sym)
    }

    /// Cache type parameters for a canonical lib symbol (without body registration).
    ///
    /// Combines [`get_canonical_lib_def_id`] + [`insert_def_type_params`] into a
    /// single call.  Used in `resolve_lib_type_with_params` where the type body
    /// is still being accumulated across multiple lib contexts and should not be
    /// registered in the type environments yet.
    ///
    /// Returns the `DefId` for subsequent use.
    pub fn cache_canonical_lib_type_params(
        &self,
        name: &str,
        per_lib_sym_id: SymbolId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) -> DefId {
        let def_id = self.get_canonical_lib_def_id(name, per_lib_sym_id);
        self.insert_def_type_params(def_id, params);
        def_id
    }

    /// Register a lib type's DefId, type parameters, and body in one step.
    ///
    /// Combines `get_lib_def_id` + `insert_def_type_params` +
    /// `register_def_auto_params_in_envs` into a single call, eliminating the
    /// repeated three-step pattern in `resolve_lib_type_by_name` (interface and
    /// type-alias branches) and `resolve_lib_type_with_params`.
    ///
    /// Returns the `DefId` for subsequent use (e.g., creating `Lazy(DefId)`).
    pub fn register_lib_def_resolved(
        &self,
        sym_id: SymbolId,
        body: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) -> DefId {
        let def_id = self.get_lib_def_id(sym_id);
        self.insert_def_type_params(def_id, params.clone());
        self.register_def_auto_params_in_envs(def_id, body, params);
        def_id
    }

    /// Ensure the `TypeEnvironment` has a reference to the shared `DefinitionStore`.
    ///
    /// This enables `TypeEnvironment::get_def_kind` to fall back to the
    /// `DefinitionStore` when the local `def_kinds` map is missing entries
    /// (which happens when `insert_def_kind` fails due to `RefCell` borrow conflicts).
    pub fn ensure_type_env_has_definition_store(&self) {
        if let Ok(mut env) = self.type_env.try_borrow_mut() {
            env.set_definition_store(std::sync::Arc::clone(&self.definition_store));
        }
    }

    // ---- Dual-environment registration helpers ----
    //
    // `type_env` (primary evaluator env) and `type_environment` (flow-analyzer
    // snapshot) are separate `TypeEnvironment` instances.  When a definition or
    // class-instance type is registered, both must be updated so that narrowing
    // contexts and the evaluator see the same data.
    //
    // These helpers eliminate the duplicated `try_borrow_mut` blocks that were
    // scattered across lib resolution, symbol-type resolution, and augmentation
    // merge paths.

    /// Register a non-generic definition body in **both** type environments.
    pub fn register_def_in_envs(&self, def_id: DefId, body: TypeId) {
        if let Ok(mut env) = self.type_env.try_borrow_mut() {
            env.insert_def(def_id, body);
        }
        if let Ok(mut env) = self.type_environment.try_borrow_mut() {
            env.insert_def(def_id, body);
        }
    }

    /// Register a generic definition body (with type parameters) in **both**
    /// type environments.
    pub fn register_def_with_params_in_envs(
        &self,
        def_id: DefId,
        body: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        if let Ok(mut env) = self.type_env.try_borrow_mut() {
            env.insert_def_with_params(def_id, body, params.clone());
        }
        if let Ok(mut env) = self.type_environment.try_borrow_mut() {
            env.insert_def_with_params(def_id, body, params);
        }
    }

    /// Register a definition body in **both** type environments, choosing
    /// `insert_def` or `insert_def_with_params` based on whether `params` is
    /// empty.
    pub fn register_def_auto_params_in_envs(
        &self,
        def_id: DefId,
        body: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        if params.is_empty() {
            self.register_def_in_envs(def_id, body);
        } else {
            self.register_def_with_params_in_envs(def_id, body, params);
        }
    }

    /// Register a class instance type in **both** type environments.
    pub fn register_class_instance_in_envs(&self, def_id: DefId, instance_type: TypeId) {
        if let Ok(mut env) = self.type_env.try_borrow_mut() {
            env.insert_class_instance_type(def_id, instance_type);
        }
        if let Ok(mut env) = self.type_environment.try_borrow_mut() {
            env.insert_class_instance_type(def_id, instance_type);
        }
    }

    /// Register an augmented definition body in **both** type environments.
    ///
    /// If the definition is a class (or already has a class-instance entry),
    /// updates the class-instance type. Otherwise, preserves existing type
    /// parameters (if any) when re-inserting the definition body.
    pub fn register_augmented_def_in_envs(&self, def_id: DefId, augmented: TypeId, is_class: bool) {
        use tsz_solver::TypeEnvironment;

        // Helper that applies the augmentation logic to a single env.
        fn apply(env: &mut TypeEnvironment, def_id: DefId, augmented: TypeId, is_class: bool) {
            if is_class || env.get_class_instance_type(def_id).is_some() {
                env.insert_class_instance_type(def_id, augmented);
            } else {
                let params: Option<Vec<tsz_solver::TypeParamInfo>> =
                    env.get_def_params(def_id).map(|s| s.to_vec());
                if let Some(params) = params {
                    env.insert_def_with_params(def_id, augmented, params);
                } else {
                    env.insert_def(def_id, augmented);
                }
            }
        }

        if let Ok(mut env) = self.type_env.try_borrow_mut() {
            apply(&mut env, def_id, augmented, is_class);
        }
        if let Ok(mut env) = self.type_environment.try_borrow_mut() {
            apply(&mut env, def_id, augmented, is_class);
        }
    }

    /// Register a `DefKind` for a `DefId` in **both** type environments.
    ///
    /// This ensures the evaluator (`type_env`) and flow-analyzer (`type_environment`)
    /// both see the `DefKind`, which is needed for `Lazy(DefId)` resolution and
    /// semantic queries (e.g., distinguishing class vs interface callables).
    ///
    /// Prior to this helper, pre-population and fallback paths only propagated
    /// `DefKind` to `type_env`, leaving `type_environment` without the mapping
    /// until the full checker walk populated it incidentally.
    fn register_def_kind_in_envs(&self, def_id: DefId, kind: tsz_solver::def::DefKind) {
        if let Ok(mut env) = self.type_env.try_borrow_mut() {
            env.insert_def_kind(def_id, kind);
        }
        if let Ok(mut env) = self.type_environment.try_borrow_mut() {
            env.insert_def_kind(def_id, kind);
        }
    }

    /// Create a Lazy type reference from a symbol.
    ///
    /// This returns `TypeData::Lazy(DefId)` for use in the new `DefId` system.
    /// During migration, this is called alongside or instead of creating
    /// `TypeData::Ref(SymbolRef)`.
    pub fn create_lazy_type_ref(&mut self, sym_id: SymbolId) -> TypeId {
        let def_id = self.get_or_create_def_id(sym_id);
        self.types.lazy(def_id)
    }

    /// Look up the `SymbolId` for a `DefId` (reverse mapping).
    pub fn def_to_symbol_id(&self, def_id: DefId) -> Option<SymbolId> {
        self.def_to_symbol.borrow().get(&def_id).copied()
    }

    /// Look up the `SymbolId` for a `DefId`, with fallback to the shared
    /// `DefinitionStore` for cross-context `DefIds`.
    ///
    /// Use this when the DefId may have been created in a different checker
    /// context (e.g., cross-file type references where `get_or_create_def_id`
    /// invalidated the per-context mapping but the Lazy(DefId) type is still
    /// alive in the type graph).
    pub fn def_to_symbol_id_with_fallback(&self, def_id: DefId) -> Option<SymbolId> {
        self.def_to_symbol_id(def_id).or_else(|| {
            let info = self.definition_store.get(def_id)?;
            info.symbol_id.map(SymbolId)
        })
    }

    /// Get or create a `DefId` for a symbol and register its type parameters in one step.
    ///
    /// Consolidates the common two-step pattern of `get_or_create_def_id` +
    /// `insert_def_type_params` into a single call. Empty params are a no-op
    /// (just returns the DefId).
    pub fn get_or_create_def_id_with_params(
        &self,
        sym_id: SymbolId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) -> DefId {
        let def_id = self.get_or_create_def_id(sym_id);
        self.insert_def_type_params(def_id, params);
        def_id
    }

    /// Insert type parameters for a `DefId` (Phase 4.2.1: generic type alias support).
    ///
    /// This enables the Solver to expand Application(Lazy(DefId), Args) by providing
    /// the type parameters needed for generic substitution.
    ///
    /// # Example
    /// ```text
    /// // For type List<T> = { value: T; next: List<T> | null }
    /// let def_id = ctx.get_or_create_def_id(list_sym_id);
    /// let params = vec![TypeParamInfo { name: "T", ... }];
    /// ctx.insert_def_type_params(def_id, params);
    /// ```
    pub fn insert_def_type_params(&self, def_id: DefId, params: Vec<tsz_solver::TypeParamInfo>) {
        if !params.is_empty() {
            // Sync type params into the DefinitionStore so the TypeFormatter
            // can display generic types with their type parameter names
            // (e.g., `MyClass<T>` instead of just `MyClass`).
            self.definition_store
                .set_type_params(def_id, params.clone());
            self.def_type_params.borrow_mut().insert(def_id, params);
        }
    }

    /// Get type parameters for a `DefId`.
    ///
    /// Returns None if the `DefId` has no type parameters or hasn't been registered yet.
    /// Falls back to the shared `DefinitionStore` when the same interface has multiple
    /// `DefIds` (e.g., lib types like `PromiseLike` that get different `DefIds` in
    /// different contexts).
    pub fn get_def_type_params(&self, def_id: DefId) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        // ---- Step 1: local cache fast path ----
        let params = self.def_type_params.borrow();
        if let Some(result) = params.get(&def_id) {
            return Some(result.clone());
        }
        drop(params);

        // ---- Step 2: DefinitionStore direct lookup (O(1)) ----
        // The store has type params for this exact DefId if they were set via
        // insert_def_type_params (which calls definition_store.set_type_params).
        if let Some(store_params) = self.definition_store.get_type_params(def_id)
            && !store_params.is_empty()
        {
            self.def_type_params
                .borrow_mut()
                .insert(def_id, store_params.clone());
            return Some(store_params);
        }

        // ---- Step 3: cross-DefId fallback via SymbolId (O(1)) ----
        // Multiple DefIds can map to the same symbol when lib interfaces are
        // referenced from different checker contexts. Use the symbol_only_index
        // to find the canonical DefId and retrieve its type params.
        let sym_id = self.def_to_symbol.borrow().get(&def_id).copied()?;
        let canonical_def_id = self.definition_store.find_def_by_symbol(sym_id.0)?;
        if canonical_def_id != def_id
            && let Some(canonical_params) = self.definition_store.get_type_params(canonical_def_id)
            && !canonical_params.is_empty()
        {
            // Cache for future lookups under the requesting DefId.
            self.def_type_params
                .borrow_mut()
                .insert(def_id, canonical_params.clone());
            return Some(canonical_params);
        }

        None
    }

    /// Resolve a `TypeId` to its underlying `SymbolId` if it is a reference type.
    ///
    /// This helper bridges the DefId-based Solver and SymbolId-based Binder.
    /// It handles the indirection automatically: `TypeId` → `DefId` → `SymbolId`.
    ///
    /// # Example
    /// ```text
    /// // Old (broken):
    /// if let Some(sym_ref) = get_ref_symbol(self.ctx.types, type_id) {
    ///     let sym_id = SymbolId(sym_ref.0); // BROKEN CAST
    /// }
    ///
    /// // New (correct):
    /// if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id) {
    ///     // use sym_id
    /// }
    /// ```
    pub fn resolve_type_to_symbol_id(&self, type_id: TypeId) -> Option<SymbolId> {
        use tsz_solver::type_queries;

        // 0. Direct TypeQuery(typeof X) resolves to X's value symbol.
        if let tsz_solver::type_queries::TypeQueryKind::TypeQuery(sym_ref) =
            tsz_solver::type_queries::classify_type_query(self.types, type_id)
        {
            return Some(SymbolId(sym_ref.0));
        }

        // 1. Try to get DefId from Lazy type - Phase 4.2+
        // Use with_fallback because get_or_create_def_id can invalidate per-context
        // DefId→SymbolId mappings when the same symbol gets a new DefId (e.g., lib
        // types like Promise referenced multiple times). The DefinitionStore retains
        // the symbol_id even after the per-context map entry is removed.
        if let Some(def_id) = type_queries::get_lazy_def_id(self.types, type_id) {
            return self.def_to_symbol_id_with_fallback(def_id);
        }

        // 2. Try to get DefId from Enum type
        if let Some(def_id) = type_queries::get_enum_def_id(self.types, type_id) {
            return self.def_to_symbol_id(def_id);
        }

        // 3. Try to get SymbolId from ObjectShape (Object or ObjectWithIndex)
        if let Some(sym_id) = tsz_solver::type_queries::data::get_object_symbol(self.types, type_id)
        {
            return Some(sym_id);
        }

        None
    }

    /// Look up an existing `DefId` for a symbol without creating a new one.
    ///
    /// Returns None if the symbol doesn't have a `DefId` yet.
    /// This is used by the `DefId` resolver in `TypeLowering` to prefer
    /// `DefId` when available but fall back to `SymbolRef` otherwise.
    ///
    /// ## Lookup strategy
    ///
    /// 1. **Local cache** (`symbol_to_def`): O(1) `FxHashMap` lookup, no locking.
    /// 2. **Authoritative index** (`DefinitionStore::symbol_only_index`): O(1)
    ///    `DashMap` lookup. This catches `DefIds` created in other checker contexts
    ///    (e.g., cross-file references, lib types) that aren't yet in the local cache.
    ///    On a hit, the local caches are populated for future fast-path access.
    pub fn get_existing_def_id(&self, sym_id: SymbolId) -> Option<DefId> {
        // Fast path: local cache
        if let Some(def_id) = self.symbol_to_def.borrow().get(&sym_id).copied() {
            return Some(def_id);
        }

        // Fallback: authoritative index (catches cross-context DefIds)
        if let Some(def_id) = self.definition_store.find_def_by_symbol(sym_id.0) {
            // Populate local caches for future fast-path hits
            self.symbol_to_def.borrow_mut().insert(sym_id, def_id);
            self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
            return Some(def_id);
        }

        None
    }

    /// Create a `TypeFormatter` with full context for displaying types (Phase 4.2.1).
    ///
    /// This includes symbol arena and definition store, which allows the formatter
    /// to display type names for Lazy(DefId) types instead of the internal "`Lazy(def_id)`"
    /// representation.
    ///
    /// # Example
    /// ```text
    /// let formatter = self.create_type_formatter();
    /// let type_str = formatter.format(type_id);  // Shows "List<number>" not "Lazy(1)<number>"
    /// ```
    pub fn create_type_formatter(&self) -> tsz_solver::TypeFormatter<'_> {
        use tsz_solver::TypeFormatter;

        TypeFormatter::with_symbols(self.types, &self.binder.symbols)
            .with_def_store(&self.definition_store)
            .with_namespace_module_names(&self.namespace_module_names)
    }

    /// Create a type formatter configured for diagnostic error messages.
    /// Skips union optionalization (synthetic `?: undefined` members) that
    /// tsc only uses in hover/quickinfo, not in error messages.
    pub fn create_diagnostic_type_formatter(&self) -> tsz_solver::TypeFormatter<'_> {
        self.create_type_formatter().with_diagnostic_mode()
    }

    /// Register a resolved type in the `TypeEnvironment` for both `SymbolRef` and `DefId`.
    ///
    /// This ensures that both the old `TypeData::Ref(SymbolRef)` and new `TypeData::Lazy(DefId)`
    /// paths can resolve the type during evaluation.
    ///
    /// The `SymbolRef` mapping is written to `type_environment` only (legacy flow-analyzer
    /// path). The DefId mapping is written to **both** environments via the dual-env
    /// helpers so the evaluator (`type_env`) and flow analyzer (`type_environment`)
    /// stay consistent.
    ///
    /// Should be called when a symbol's type is resolved via `get_type_of_symbol`.
    pub fn register_resolved_type(
        &self,
        sym_id: SymbolId,
        type_id: TypeId,
        type_params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        use tsz_solver::SymbolRef;

        // Insert SymbolRef key into type_environment only (legacy path —
        // type_env never uses SymbolRef-keyed lookups).
        if let Ok(mut env) = self.type_environment.try_borrow_mut() {
            if type_params.is_empty() {
                env.insert(SymbolRef(sym_id.0), type_id);
            } else {
                env.insert_with_params(SymbolRef(sym_id.0), type_id, type_params.clone());
            }
        }

        // Insert DefId key into BOTH environments via dual-env helpers.
        // Previously this only wrote to type_environment, leaving type_env
        // without the DefId mapping — a consistency bug that could cause
        // resolve_lazy(DefId) to return None in the evaluator.
        if let Some(def_id) = self.get_existing_def_id(sym_id) {
            self.register_def_auto_params_in_envs(def_id, type_id, type_params);

            // Register mapping for InheritanceGraph bridge (Phase 3.2)
            // This enables Lazy(DefId) types to use the O(1) InheritanceGraph
            if let Ok(mut env) = self.type_environment.try_borrow_mut() {
                env.register_def_symbol_mapping(def_id, sym_id);
            }

            // Set the body on the DefinitionInfo so the type formatter can
            // find type alias names via find_type_alias_by_body(). Without
            // this, type aliases show their structural expansion in diagnostics
            // (e.g., "{ r: number; g: number; b: number }" instead of "Color").
            self.definition_store.set_body(def_id, type_id);
        }
    }

    /// Pre-populate `symbol_to_def` and `def_to_symbol` from the binder's
    /// `semantic_defs` index (Phase 1 DefId-first stable identity).
    ///
    /// Called once during checker construction so that `get_or_create_def_id`
    /// finds stable `DefIds` already present for top-level declarations. This
    /// moves identity creation to bind time (deterministic, early) rather than
    /// being recovered on-demand in hot checker paths (late, order-dependent).
    ///
    /// Returns the number of `DefIds` pre-populated.
    pub fn pre_populate_def_ids_from_binder(&self) -> usize {
        self.populate_def_ids_from_semantic_defs(&self.binder.semantic_defs)
    }

    /// Pre-populate `symbol_to_def` and `def_to_symbol` from all lib binders'
    /// `semantic_defs` indices.
    ///
    /// Lib binders contain definitions for standard library types (Array, Promise,
    /// Error, Map, etc.). Without this, every `get_or_create_def_id` call for a
    /// lib symbol falls through to the Step 3 O(N) `lib_contexts.iter()` scan to
    /// find the symbol and create its DefId on demand. By pre-populating here, these
    /// symbols hit the O(1) `find_def_by_symbol` path in Step 2 instead.
    ///
    /// Returns the total number of `DefIds` pre-populated across all lib binders.
    pub fn pre_populate_def_ids_from_lib_binders(&self) -> usize {
        let mut total = 0;
        for lib_ctx in &self.lib_contexts {
            total += self.populate_def_ids_from_semantic_defs(&lib_ctx.binder.semantic_defs);
        }
        total
    }

    /// Pre-populate `symbol_to_def` and `def_to_symbol` from all cross-file
    /// binders' `semantic_defs` indices (multi-file stable identity).
    ///
    /// In multi-file compilation, each file has its own binder with its own
    /// `semantic_defs`. Without this, cross-file type references (e.g.,
    /// importing a class from another file) hit the O(N) `all_binders` scan
    /// in `get_or_create_def_id` Step 3 and create `DefIds` on demand.
    ///
    /// By pre-populating here, those `SymbolIds` are already registered in the
    /// `DefinitionStore`'s `symbol_only_index`, so `get_or_create_def_id`
    /// Step 2 finds them in O(1) without the repair path.
    ///
    /// Called from `ProjectEnv::apply_to` after `set_all_binders`.
    /// Safe to overlap with `pre_populate_def_ids_from_binder` (the current
    /// file's binder may also appear in `all_binders`); the dedup check in
    /// `populate_def_ids_from_semantic_defs` skips already-registered entries.
    ///
    /// Returns the total number of new `DefIds` pre-populated.
    pub fn pre_populate_def_ids_from_all_binders(&self) -> usize {
        let Some(ref binders) = self.all_binders else {
            return 0;
        };
        let mut total = 0;
        for binder in binders.iter() {
            total += self.populate_def_ids_from_semantic_defs(&binder.semantic_defs);
        }
        total
    }

    /// Core helper: populate DefId mappings from a `semantic_defs` map.
    ///
    /// Used by both `pre_populate_def_ids_from_binder` (primary binder) and
    /// `pre_populate_def_ids_from_lib_binders` (lib binders). The logic is
    /// identical: convert `SemanticDefEntry` to `DefinitionInfo`, register in
    /// the `DefinitionStore`, and populate local caches.
    fn populate_def_ids_from_semantic_defs(
        &self,
        semantic_defs: &rustc_hash::FxHashMap<tsz_binder::SymbolId, tsz_binder::SemanticDefEntry>,
    ) -> usize {
        use tsz_solver::def::{DefKind, DefinitionInfo};

        if semantic_defs.is_empty() {
            return 0;
        }

        let mut count = 0;
        for (&sym_id, entry) in semantic_defs {
            // Skip if already mapped (e.g., from a previous lib merge pass
            // or the primary binder's pre-population).
            if self.symbol_to_def.borrow().contains_key(&sym_id) {
                continue;
            }

            // Also skip if the DefinitionStore already has a mapping for this
            // symbol (e.g., from another lib binder that declared the same
            // global interface via declaration merging).
            if self.definition_store.find_def_by_symbol(sym_id.0).is_some() {
                continue;
            }

            // Convert binder's SemanticDefKind to solver's DefKind
            let kind = match entry.kind {
                tsz_binder::SemanticDefKind::TypeAlias => DefKind::TypeAlias,
                tsz_binder::SemanticDefKind::Interface => DefKind::Interface,
                tsz_binder::SemanticDefKind::Class => DefKind::Class,
                tsz_binder::SemanticDefKind::Enum => DefKind::Enum,
                tsz_binder::SemanticDefKind::Namespace => DefKind::Namespace,
                tsz_binder::SemanticDefKind::Function => DefKind::Function,
                tsz_binder::SemanticDefKind::Variable => DefKind::Variable,
            };

            // Use the SemanticDefEntry's self-contained data (name, file_id,
            // span_start) instead of looking up the symbol table. This makes
            // pre-population independent of full symbol residency, which is a
            // prerequisite for file-skeleton decomposition (Phase 2).
            let name = self.types.intern_string(&entry.name);

            // Create type parameter entries preserving arity and names.
            // Binder captures type param names at bind time; we use them here
            // so DefinitionInfo has real names from the start. Constraints and
            // defaults are still filled in later by the checker walk via
            // DefinitionStore::set_type_params().
            let type_params = if entry.type_param_count > 0 {
                (0..entry.type_param_count)
                    .map(|i| {
                        let name = entry
                            .type_param_names
                            .get(i as usize)
                            .map(|n| self.types.intern_string(n))
                            .unwrap_or(tsz_common::interner::Atom(0));
                        tsz_solver::TypeParamInfo {
                            name,
                            constraint: None,
                            default: None,
                            is_const: false,
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            };

            // Propagate enum member names from binder's SemanticDefEntry.
            // Values are set to Computed; real values are resolved later by
            // the checker walk. This enables enum identity to be established
            // at pre-population time without waiting for full type resolution.
            let enum_members: Vec<(tsz_common::interner::Atom, tsz_solver::def::EnumMemberValue)> =
                entry
                    .enum_member_names
                    .iter()
                    .map(|name| {
                        (
                            self.types.intern_string(name),
                            tsz_solver::def::EnumMemberValue::Computed,
                        )
                    })
                    .collect();

            let info = DefinitionInfo {
                kind,
                name,
                type_params,
                body: None,
                instance_shape: None,
                static_shape: None,
                extends: None,
                implements: Vec::new(),
                enum_members,
                exports: Vec::new(),
                file_id: Some(entry.file_id),
                span: Some((entry.span_start, entry.span_start)),
                symbol_id: Some(sym_id.0),
                heritage_names: entry.heritage_names(),
                is_abstract: entry.is_abstract,
                is_const: entry.is_const,
                is_exported: entry.is_exported,
                is_global_augmentation: entry.is_global_augmentation,
            };

            let def_id = self.definition_store.register(info);
            trace!(
                symbol_name = %entry.name,
                symbol_id = %sym_id.0,
                def_id = %def_id.0,
                kind = ?kind,
                "Pre-populated DefId from semantic_defs"
            );

            // Register in the authoritative index so other checker contexts
            // can find this DefId via lookup_by_symbol() without creating
            // duplicates. This closes the gap where pre-populated DefIds
            // were only in the local cache but invisible to the shared store.
            self.definition_store
                .register_symbol_mapping(sym_id.0, entry.file_id, def_id);

            self.symbol_to_def.borrow_mut().insert(sym_id, def_id);
            self.def_to_symbol.borrow_mut().insert(def_id, sym_id);

            // Propagate DefKind to both TypeEnvironments (evaluator + flow-analyzer)
            self.register_def_kind_in_envs(def_id, kind);

            count += 1;
        }

        // Pass 2: Wire namespace exports from parent_namespace relationships.
        // After all DefIds are created/warmed, walk entries with parent_namespace
        // and register them as exports of their parent's DefinitionInfo.
        for (&sym_id, entry) in semantic_defs {
            if let Some(parent_sym) = entry.parent_namespace {
                let child_def = self.definition_store.find_def_by_symbol(sym_id.0);
                let parent_def = self.definition_store.find_def_by_symbol(parent_sym.0);
                if let (Some(child_def_id), Some(parent_def_id)) = (child_def, parent_def) {
                    let name = self.types.intern_string(&entry.name);
                    self.definition_store
                        .add_export(parent_def_id, name, child_def_id);
                }
            }
        }

        count
    }

    /// Warm local `symbol_to_def` / `def_to_symbol` caches from the shared
    /// `DefinitionStore` in a single pass.
    ///
    /// When the checker receives a pre-populated `DefinitionStore` from the
    /// merge pipeline (via `with_options_and_shared_def_store`), this method
    /// is more efficient than `pre_populate_def_ids_from_binder()` +
    /// `pre_populate_def_ids_from_lib_binders()` because it reads directly
    /// from the store's authoritative symbol→DefId index instead of
    /// re-iterating each binder's `semantic_defs` and re-converting
    /// `SemanticDefEntry` → `DefinitionInfo`.
    ///
    /// Returns the number of mappings warmed.
    pub fn warm_local_caches_from_shared_store(&self) -> usize {
        if self.definition_store.is_empty() {
            return 0;
        }

        let mappings = self.definition_store.all_symbol_mappings();
        let mut count = 0;

        for (raw_sym_id, def_id) in &mappings {
            let sym_id = tsz_binder::SymbolId(*raw_sym_id);

            // Skip if already in local cache (e.g., from a prior warm pass).
            if self.symbol_to_def.borrow().contains_key(&sym_id) {
                continue;
            }

            self.symbol_to_def.borrow_mut().insert(sym_id, *def_id);
            self.def_to_symbol.borrow_mut().insert(*def_id, sym_id);

            // Propagate DefKind to both TypeEnvironments so the evaluator
            // and flow-analyzer can query it without waiting for first access.
            if let Some(info) = self.definition_store.get(*def_id) {
                self.register_def_kind_in_envs(*def_id, info.kind);
            }

            count += 1;
        }

        trace!(
            count,
            total_mappings = mappings.len(),
            "Warmed local caches from shared DefinitionStore"
        );

        count
    }

    /// Returns `true` if the shared `DefinitionStore` has been pre-populated
    /// (i.e., it contains definitions registered at merge time, not just an
    /// empty store created by the default constructor).
    ///
    /// When true, `warm_local_caches_from_shared_store()` can replace the
    /// more expensive `pre_populate_def_ids_from_binder()` +
    /// `pre_populate_def_ids_from_lib_binders()` calls.
    pub fn has_shared_store(&self) -> bool {
        !self.definition_store.is_empty()
    }

    /// Resolve heritage for definitions whose extends/implements targets were
    /// not found during their batch's pass 2 (cross-batch heritage).
    ///
    /// This handles the common case where a user class extends a lib type
    /// (e.g., `class MyError extends Error`): when `pre_populate_def_ids_from_binder`
    /// processes the user file, the lib type's `DefId` hasn't been registered yet
    /// (lib binders are pre-populated separately). After ALL pre-population batches
    /// complete, this method resolves the remaining heritage using the
    /// `DefinitionStore`'s name index, which now contains entries from all batches.
    ///
    /// Called once during checker construction after all `pre_populate_*` methods.
    /// Returns the number of heritage links resolved.
    pub fn resolve_cross_batch_heritage(&self) -> usize {
        use tsz_solver::def::DefKind;

        let mut resolved_count = 0;

        // Collect all semantic_defs from all sources (primary binder + all_binders).
        // The shared DefinitionStore's name_to_defs index is already populated from
        // all pre-population batches, so name-based lookups will find targets from
        // any batch (user files, lib files, cross-file binders).
        let sources: Vec<
            &rustc_hash::FxHashMap<tsz_binder::SymbolId, tsz_binder::SemanticDefEntry>,
        > = {
            let mut v = vec![&self.binder.semantic_defs];
            for lib_ctx in &self.lib_contexts {
                v.push(&lib_ctx.binder.semantic_defs);
            }
            if let Some(ref binders) = self.all_binders {
                for binder in binders.iter() {
                    v.push(&binder.semantic_defs);
                }
            }
            v
        };

        for source in &sources {
            for (&sym_id, entry) in *source {
                let def_id = match self.definition_store.find_def_by_symbol(sym_id.0) {
                    Some(id) => id,
                    None => continue,
                };

                // Skip if extends is already wired (from pre-populate Pass 3)
                if let Some(info) = self.definition_store.get(def_id) {
                    if info.extends.is_some() {
                        continue;
                    }
                }

                // Resolve extends_names → extends
                for name_str in &entry.extends_names {
                    if name_str.contains('.') {
                        continue;
                    }
                    let name_atom = self.types.intern_string(name_str);
                    if let Some(candidates) = self.definition_store.find_defs_by_name(name_atom) {
                        for &candidate_id in &candidates {
                            if candidate_id == def_id {
                                continue;
                            }
                            if let Some(info) = self.definition_store.get(candidate_id)
                                && matches!(info.kind, DefKind::Class | DefKind::Interface)
                            {
                                self.definition_store.set_extends(def_id, candidate_id);
                                resolved_count += 1;
                                break;
                            }
                        }
                    }
                    break; // only first extends name
                }

                // Resolve implements_names → implements
                if !entry.implements_names.is_empty() {
                    let mut resolved = Vec::new();
                    for name_str in &entry.implements_names {
                        if name_str.contains('.') {
                            continue;
                        }
                        let name_atom = self.types.intern_string(name_str);
                        if let Some(candidates) = self.definition_store.find_defs_by_name(name_atom)
                        {
                            for &candidate_id in &candidates {
                                if candidate_id == def_id {
                                    continue;
                                }
                                if let Some(info) = self.definition_store.get(candidate_id)
                                    && matches!(info.kind, DefKind::Interface | DefKind::Class)
                                {
                                    resolved.push(candidate_id);
                                    break;
                                }
                            }
                        }
                    }
                    if !resolved.is_empty() {
                        self.definition_store
                            .set_implements(def_id, resolved.clone());
                        resolved_count += resolved.len();
                    }
                }
            }
        }

        resolved_count
    }
}
