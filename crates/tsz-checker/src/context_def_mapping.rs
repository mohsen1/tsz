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
    pub fn get_or_create_def_id(&self, sym_id: SymbolId) -> DefId {
        use tsz_solver::def::DefinitionInfo;

        let existing_def_id = self.symbol_to_def.borrow().get(&sym_id).copied();
        if let Some(def_id) = existing_def_id {
            // Validate cached mapping to guard against cross-binder SymbolId collisions.
            // In multi-file/lib flows, the same raw SymbolId can refer to different symbols
            // in different binders; stale mappings can make Lazy(def) point to the wrong symbol.
            let mapped_symbol = self
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

            let is_valid_mapping = if let (Some(info), Some(sym)) =
                (self.definition_store.get(def_id), mapped_symbol)
            {
                let def_name = self.types.resolve_atom_ref(info.name);
                def_name.as_ref() == sym.escaped_name
                    && info.file_id.is_none_or(|fid| fid == sym.decl_file_idx)
            } else {
                false
            };

            if is_valid_mapping {
                return def_id;
            }

            self.symbol_to_def.borrow_mut().remove(&sym_id);
            if self
                .def_to_symbol
                .borrow()
                .get(&def_id)
                .is_some_and(|mapped| *mapped == sym_id)
            {
                self.def_to_symbol.borrow_mut().remove(&def_id);
            }
        }

        // Get symbol info to create DefinitionInfo
        // First try the main binder, then check lib binders
        let symbol = if let Some(sym) = self.binder.symbols.get(sym_id) {
            sym
        } else {
            // Try to find in lib binders
            let mut found = None;
            for lib_ctx in &self.lib_contexts {
                if let Some(lib_symbol) = lib_ctx.binder.symbols.get(sym_id) {
                    found = Some(lib_symbol);
                    break;
                }
            }
            match found {
                Some(s) => s,
                None => {
                    // Symbol not found anywhere - return invalid DefId
                    return DefId::INVALID;
                }
            }
        };
        let name = self.types.intern_string(&symbol.escaped_name);

        // Determine DefKind from symbol flags
        let kind = if (symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0 {
            tsz_solver::def::DefKind::TypeAlias
        } else if (symbol.flags & tsz_binder::symbol_flags::INTERFACE) != 0 {
            tsz_solver::def::DefKind::Interface
        } else if (symbol.flags & tsz_binder::symbol_flags::CLASS) != 0 {
            tsz_solver::def::DefKind::Class
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
            "Mapping symbol to DefId"
        );
        self.symbol_to_def.borrow_mut().insert(sym_id, def_id);
        self.def_to_symbol.borrow_mut().insert(def_id, sym_id);

        def_id
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

    /// Insert type parameters for a `DefId` (Phase 4.2.1: generic type alias support).
    ///
    /// This enables the Solver to expand Application(Lazy(DefId), Args) by providing
    /// the type parameters needed for generic substitution.
    ///
    /// # Example
    /// ```ignore
    /// // For type List<T> = { value: T; next: List<T> | null }
    /// let def_id = ctx.get_or_create_def_id(list_sym_id);
    /// let params = vec![TypeParamInfo { name: "T", ... }];
    /// ctx.insert_def_type_params(def_id, params);
    /// ```
    pub fn insert_def_type_params(&self, def_id: DefId, params: Vec<tsz_solver::TypeParamInfo>) {
        if !params.is_empty() {
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
        // Also handle the case where DefId was created from a raw SymbolId by
        // `interner.reference()` — use the raw value as a SymbolId candidate.
        let sym_id = self
            .def_to_symbol
            .borrow()
            .get(&def_id)
            .copied()
            .or_else(|| {
                let candidate = tsz_binder::SymbolId(def_id.0);
                if self.binder.symbols.get(candidate).is_some()
                    || self
                        .lib_contexts
                        .iter()
                        .any(|lib| lib.binder.symbols.get(candidate).is_some())
                {
                    Some(candidate)
                } else {
                    None
                }
            })?;
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
    /// ```ignore
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

        // 1. Try to get DefId from Lazy type - Phase 4.2+
        if let Some(def_id) = type_queries::get_lazy_def_id(self.types, type_id) {
            return self.def_to_symbol_id(def_id);
        }

        // 2. Try to get DefId from Enum type
        if let Some(def_id) = type_queries::get_enum_def_id(self.types, type_id) {
            return self.def_to_symbol_id(def_id);
        }

        // 3. Try to get SymbolId from ObjectShape
        if let Some(shape_id) = type_queries::get_object_shape_id(self.types, type_id) {
            return self
                .types
                .object_shape(shape_id)
                .symbol
                .map(|s| SymbolId(s.0));
        }

        None
    }

    /// Look up an existing `DefId` for a symbol without creating a new one.
    ///
    /// Returns None if the symbol doesn't have a `DefId` yet.
    /// This is used by the `DefId` resolver in `TypeLowering` to prefer
    /// `DefId` when available but fall back to `SymbolRef` otherwise.
    pub fn get_existing_def_id(&self, sym_id: SymbolId) -> Option<DefId> {
        self.symbol_to_def.borrow().get(&sym_id).copied()
    }

    /// Create a `TypeFormatter` with full context for displaying types (Phase 4.2.1).
    ///
    /// This includes symbol arena and definition store, which allows the formatter
    /// to display type names for Lazy(DefId) types instead of the internal "`Lazy(def_id)`"
    /// representation.
    ///
    /// # Example
    /// ```ignore
    /// let formatter = self.create_type_formatter();
    /// let type_str = formatter.format(type_id);  // Shows "List<number>" not "Lazy(1)<number>"
    /// ```
    pub fn create_type_formatter(&self) -> tsz_solver::TypeFormatter<'_> {
        use tsz_solver::TypeFormatter;

        TypeFormatter::with_symbols(self.types, &self.binder.symbols)
            .with_def_store(&self.definition_store)
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
            if let Some(&def_id) = self.symbol_to_def.borrow().get(&sym_id) {
                if type_params.is_empty() {
                    env.insert_def(def_id, type_id);
                } else {
                    env.insert_def_with_params(def_id, type_id, type_params);
                }

                // Register mapping for InheritanceGraph bridge (Phase 3.2)
                // This enables Lazy(DefId) types to use the O(1) InheritanceGraph
                env.register_def_symbol_mapping(def_id, sym_id);
            }
        }
    }
}
