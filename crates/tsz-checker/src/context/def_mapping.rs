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

        // ---- Step 2: look up the symbol to get its file_idx ----
        // We need the symbol to determine which binder it came from.
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
                    binders
                        .iter()
                        .find_map(|binder| binder.symbols.get(sym_id))
                })
            });

        let symbol = match symbol {
            Some(s) => s,
            None => return DefId::INVALID,
        };

        let file_idx = symbol.decl_file_idx;

        // ---- Step 3: authoritative index lookup ----
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
        } else {
            // Default to TypeAlias for other symbols
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
        };

        let def_id = self.definition_store.register(info);
        trace!(
            symbol_name = %symbol.escaped_name,
            symbol_id = %sym_id.0,
            def_id = %def_id.0,
            kind = ?kind,
            "Mapping symbol to DefId"
        );

        // Register in the authoritative index (shared across contexts).
        self.definition_store
            .register_symbol_mapping(sym_id.0, file_idx, def_id);

        // Populate local caches.
        self.symbol_to_def.borrow_mut().insert(sym_id, def_id);
        self.def_to_symbol.borrow_mut().insert(def_id, sym_id);

        // Propagate DefKind to TypeEnvironment so the solver can query it
        // (e.g., to distinguish class Callables from interface Callables).
        if let Ok(mut env) = self.type_env.try_borrow_mut() {
            env.insert_def_kind(def_id, kind);
        }

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
    /// Falls back to SymbolId-based lookup when the same interface has multiple `DefIds`
    /// (e.g., lib types like `PromiseLike` that get different `DefIds` in different contexts).
    pub fn get_def_type_params(&self, def_id: DefId) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        let params = self.def_type_params.borrow();
        if let Some(result) = params.get(&def_id) {
            return Some(result.clone());
        }

        // Fallback: look up via SymbolId. Multiple DefIds can map to the same symbol
        // when lib interfaces are referenced from different checker contexts.
        let sym_id = self.def_to_symbol.borrow().get(&def_id).copied()?;
        for (&other_def, other_params) in params.iter() {
            if other_def != def_id
                && self
                    .def_to_symbol
                    .borrow()
                    .get(&other_def)
                    .is_some_and(|&s| s == sym_id)
            {
                // Found type params registered under a different DefId for the same symbol.
                // Cache for future lookups.
                let result = other_params.clone();
                drop(params);
                self.def_type_params
                    .borrow_mut()
                    .insert(def_id, result.clone());
                return Some(result);
            }
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
    ///    `DashMap` lookup. This catches DefIds created in other checker contexts
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
    /// Should be called when a symbol's type is resolved via `get_type_of_symbol`.
    pub fn register_resolved_type(
        &mut self,
        sym_id: SymbolId,
        type_id: TypeId,
        type_params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        use tsz_solver::SymbolRef;

        // Try to borrow mutably - skip if already borrowed (during recursive resolution)
        if let Ok(mut env) = self.type_environment.try_borrow_mut() {
            // Insert with SymbolRef key (existing path)
            if type_params.is_empty() {
                env.insert(SymbolRef(sym_id.0), type_id);
            } else {
                env.insert_with_params(SymbolRef(sym_id.0), type_id, type_params.clone());
            }

            // Also insert with DefId key if one exists (Phase 4.3 migration)
            if let Some(def_id) = self.get_existing_def_id(sym_id) {
                if type_params.is_empty() {
                    env.insert_def(def_id, type_id);
                } else {
                    env.insert_def_with_params(def_id, type_id, type_params);
                }

                // Register mapping for InheritanceGraph bridge (Phase 3.2)
                // This enables Lazy(DefId) types to use the O(1) InheritanceGraph
                env.register_def_symbol_mapping(def_id, sym_id);

                // Set the body on the DefinitionInfo so the type formatter can
                // find type alias names via find_type_alias_by_body(). Without
                // this, type aliases show their structural expansion in diagnostics
                // (e.g., "{ r: number; g: number; b: number }" instead of "Color").
                self.definition_store.set_body(def_id, type_id);
            }
        }
    }

    /// Pre-populate `symbol_to_def` and `def_to_symbol` from the binder's
    /// `semantic_defs` index (Phase 1 DefId-first stable identity).
    ///
    /// Called once during checker construction so that `get_or_create_def_id`
    /// finds stable DefIds already present for top-level declarations. This
    /// moves identity creation to bind time (deterministic, early) rather than
    /// being recovered on-demand in hot checker paths (late, order-dependent).
    ///
    /// Returns the number of DefIds pre-populated.
    pub fn pre_populate_def_ids_from_binder(&self) -> usize {
        use tsz_solver::def::{DefKind, DefinitionInfo};

        let semantic_defs = &self.binder.semantic_defs;
        if semantic_defs.is_empty() {
            return 0;
        }

        let mut count = 0;
        for (&sym_id, entry) in semantic_defs {
            // Skip if already mapped (e.g., from a previous lib merge pass)
            if self.symbol_to_def.borrow().contains_key(&sym_id) {
                continue;
            }

            // Verify the symbol actually exists in our binder
            let symbol = match self.binder.symbols.get(sym_id) {
                Some(s) => s,
                None => continue,
            };

            // Convert binder's SemanticDefKind to solver's DefKind
            let kind = match entry.kind {
                tsz_binder::SemanticDefKind::TypeAlias => DefKind::TypeAlias,
                tsz_binder::SemanticDefKind::Interface => DefKind::Interface,
                tsz_binder::SemanticDefKind::Class => DefKind::Class,
                tsz_binder::SemanticDefKind::Enum => DefKind::Enum,
                tsz_binder::SemanticDefKind::Namespace => DefKind::Namespace,
            };

            let name = self.types.intern_string(&symbol.escaped_name);
            let span = symbol.declarations.first().map(|n| (n.0, n.0));

            let info = DefinitionInfo {
                kind,
                name,
                type_params: Vec::new(),
                body: None,
                instance_shape: None,
                static_shape: None,
                extends: None,
                implements: Vec::new(),
                enum_members: Vec::new(),
                exports: Vec::new(),
                file_id: Some(symbol.decl_file_idx),
                span,
                symbol_id: Some(sym_id.0),
            };

            let def_id = self.definition_store.register(info);
            trace!(
                symbol_name = %symbol.escaped_name,
                symbol_id = %sym_id.0,
                def_id = %def_id.0,
                kind = ?kind,
                "Pre-populated DefId from binder semantic_defs"
            );

            // Register in the authoritative index so other checker contexts
            // can find this DefId via lookup_by_symbol() without creating
            // duplicates. This closes the gap where pre-populated DefIds
            // were only in the local cache but invisible to the shared store.
            self.definition_store
                .register_symbol_mapping(sym_id.0, symbol.decl_file_idx, def_id);

            self.symbol_to_def.borrow_mut().insert(sym_id, def_id);
            self.def_to_symbol.borrow_mut().insert(def_id, sym_id);

            // Propagate DefKind to TypeEnvironment
            if let Ok(mut env) = self.type_env.try_borrow_mut() {
                env.insert_def_kind(def_id, kind);
            }

            count += 1;
        }

        count
    }
}
