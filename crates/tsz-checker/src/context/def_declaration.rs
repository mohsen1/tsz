//! Declaration-keyed `DefId` minting for `CheckerContext`.
//!
//! Splits the file-keyed canonical-`DefId` helpers out of `def_mapping` (which
//! is at the per-file line ceiling). These key a `DefId` strictly on a
//! declaration's `(SymbolId, file_idx)` so re-export-chain resolvers can attach
//! a generic's body to the declaration rather than an intermediate hop.

use tsz_binder::SymbolId;
use tsz_solver::def::{DefId, DefinitionInfo};

use crate::context::CheckerContext;

impl CheckerContext<'_> {
    /// Whether `def_id`'s recorded name matches `expected_name`.
    ///
    /// Raw `SymbolId`s are binder-relative, so every def lookup that starts
    /// from a raw id must validate the def's name against the symbol it
    /// thinks it resolved (issue #15687). Returns `None` when the def has no
    /// recorded name; callers choose strict (`unwrap_or(false)`) or lenient
    /// (`unwrap_or(true)`) handling. Uses the non-allocating atom read — this
    /// runs on lazy-resolution hot paths.
    pub(crate) fn def_name_matches(&self, def_id: DefId, expected_name: &str) -> Option<bool> {
        self.definition_store
            .get_name(def_id)
            .map(|name| &*self.types.resolve_atom_ref(name) == expected_name)
    }

    /// Lenient raw-id collision guard: `true` unless THIS binder resolves
    /// `sym_id` to a symbol whose name contradicts `def_id`'s recorded name.
    /// Used where a missing local symbol or an unnamed def must not block
    /// resolution (see [`CheckerContext::def_name_matches`]).
    pub(crate) fn def_matches_local_symbol(&self, def_id: DefId, sym_id: SymbolId) -> bool {
        self.binder.get_symbol(sym_id).is_none_or(|local| {
            self.def_name_matches(def_id, &local.escaped_name)
                .unwrap_or(true)
        })
    }

    pub(crate) fn def_info_matches_symbol_declaration(
        &self,
        info: &DefinitionInfo,
        sym_id: SymbolId,
        _symbol: &tsz_binder::Symbol,
        file_idx: u32,
        expected_name: &str,
    ) -> bool {
        if &*self.types.resolve_atom_ref(info.name) != expected_name
            || info.symbol_id != Some(sym_id.0)
            || info.file_id != Some(file_idx)
        {
            return false;
        }

        true
    }

    /// Resolve-or-mint the canonical `DefId` for a declaration identified by its
    /// declaring `(SymbolId, file_idx)`, requiring the declaration's name to
    /// match `expected_name`.
    ///
    /// Unlike [`CheckerContext::get_or_create_def_id_for_symbol_name`], this keys
    /// strictly on the *declaring* file rather than deriving it from
    /// `decl_file_idx` / current-file-local heuristics, and it does not consult
    /// the raw-`SymbolId`-keyed `symbol_to_def` cache (which collides across
    /// binders). Callers use it when they have already resolved a re-export
    /// chain to the declaration together with its file index, so the `DefId`
    /// is attributed to the declaration — not to an intermediate re-export hop
    /// whose body is never registered. Returns `None` when `file_idx` has no
    /// binder or the symbol there does not match `expected_name`.
    pub fn def_id_for_declaration_in_file(
        &self,
        sym_id: SymbolId,
        file_idx: usize,
        expected_name: &str,
    ) -> Option<DefId> {
        let file_idx_u32 = file_idx as u32;
        if let Some(def_id) = self
            .definition_store
            .lookup_by_symbol(sym_id.0, file_idx_u32)
            && self
                .def_name_matches(def_id, expected_name)
                .unwrap_or(false)
        {
            return Some(def_id);
        }
        let binder = self.get_binder_for_file(file_idx)?;
        let symbol = binder
            .get_symbol(sym_id)
            .filter(|symbol| symbol.escaped_name == expected_name)?;
        Some(self.mint_def_for_symbol_in_file(sym_id, symbol, file_idx_u32))
    }

    /// Build a `DefinitionInfo` skeleton from `symbol` and atomically
    /// resolve-or-register it under `(sym_id, file_idx)` in the shared
    /// [`tsz_solver::def::DefinitionStore`], updating the local caches. Shared by
    /// [`CheckerContext::get_or_create_def_id_for_symbol_name`] and
    /// [`CheckerContext::def_id_for_declaration_in_file`].
    pub(crate) fn mint_def_for_symbol_in_file(
        &self,
        sym_id: SymbolId,
        symbol: &tsz_binder::Symbol,
        file_idx: u32,
    ) -> DefId {
        let info = self.build_symbol_def_info(sym_id, symbol, file_idx);
        let kind = info.kind;
        // Atomic mint — see `get_or_create_def_id`: concurrent checkers
        // stabilizing the same `(symbol, file)` identity converge on one
        // `DefId` instead of each minting its own.
        let (def_id, _minted) = self
            .definition_store
            .register_for_symbol(sym_id.0, file_idx, info);
        self.symbol_to_def.borrow_mut().insert(sym_id, def_id);
        self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
        self.register_def_kind_in_envs(def_id, kind);
        def_id
    }

    /// Register a FRESH `DefId` for a lib `symbol`, keyed only by the
    /// collision-free NAME index — bypassing the `(SymbolId, file)`
    /// `symbol_def_index` used by [`Self::mint_def_for_symbol_in_file`].
    ///
    /// Every lib-binder symbol carries the `u32::MAX` declaration-file
    /// sentinel, so two lib binders' symbols that share a raw index collide in
    /// `register_for_symbol`; and `DeclSiteKey` is disabled for the sentinel
    /// file, so decl-site disambiguation does not rescue it (it returns the
    /// already-registered, wrongly-named def). When
    /// [`Self::get_canonical_lib_def_id`] has already observed such a collision
    /// (the raw-id resolution names the wrong symbol) and the name index has no
    /// entry yet, it mints here instead. `DefinitionStore::register` allocates a
    /// fresh id and populates `name_to_defs`, so this reference and the later
    /// on-demand body resolution converge through the name index and
    /// `def_symbol_identity` (which re-verifies the symbol by name). Idempotent
    /// in practice: the caller's name-index probe runs first, so a second
    /// resolution of the same name returns the already-registered def rather
    /// than minting again.
    pub(crate) fn register_named_lib_def(
        &self,
        sym_id: SymbolId,
        symbol: &tsz_binder::Symbol,
        file_idx: u32,
    ) -> DefId {
        let info = self.build_symbol_def_info(sym_id, symbol, file_idx);
        let kind = info.kind;
        let def_id = self.definition_store.register(info);
        self.symbol_to_def.borrow_mut().insert(sym_id, def_id);
        self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
        self.register_def_kind_in_envs(def_id, kind);
        def_id
    }

    /// Build the [`DefinitionInfo`] for a symbol/file identity, shared by
    /// [`Self::mint_def_for_symbol_in_file`] and
    /// [`Self::register_named_lib_def`].
    fn build_symbol_def_info(
        &self,
        sym_id: SymbolId,
        symbol: &tsz_binder::Symbol,
        file_idx: u32,
    ) -> tsz_solver::def::DefinitionInfo {
        use tsz_solver::def::DefinitionInfo;

        let name = self.types.intern_string(&symbol.escaped_name);
        let kind = if symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS) {
            tsz_solver::def::DefKind::TypeAlias
        } else if symbol.has_any_flags(tsz_binder::symbol_flags::CLASS) {
            tsz_solver::def::DefKind::Class
        } else if symbol.has_any_flags(tsz_binder::symbol_flags::INTERFACE) {
            tsz_solver::def::DefKind::Interface
        } else if symbol.has_any_flags(tsz_binder::symbol_flags::ENUM) {
            tsz_solver::def::DefKind::Enum
        } else if symbol.has_any_flags(
            tsz_binder::symbol_flags::NAMESPACE_MODULE | tsz_binder::symbol_flags::VALUE_MODULE,
        ) {
            tsz_solver::def::DefKind::Namespace
        } else if symbol.has_any_flags(tsz_binder::symbol_flags::FUNCTION) {
            tsz_solver::def::DefKind::Function
        } else if symbol.has_any_flags(
            tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE
                | tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE,
        ) {
            tsz_solver::def::DefKind::Variable
        } else {
            tsz_solver::def::DefKind::TypeAlias
        };
        let span = symbol.first_declaration_span().or_else(|| {
            if symbol.value_declaration.is_some() {
                symbol.value_declaration_span()
            } else {
                None
            }
        });

        DefinitionInfo {
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
            file_id: Some(file_idx),
            span,
            symbol_id: Some(sym_id.0),
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        }
    }
}
