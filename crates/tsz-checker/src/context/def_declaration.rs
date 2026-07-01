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
    pub(crate) fn def_info_matches_symbol_declaration(
        &self,
        info: &DefinitionInfo,
        sym_id: SymbolId,
        symbol: &tsz_binder::Symbol,
        file_idx: u32,
        expected_name: &str,
    ) -> bool {
        if self.types.resolve_atom(info.name) != expected_name
            || info.symbol_id != Some(sym_id.0)
            || info.file_id != Some(file_idx)
        {
            return false;
        }

        let span = symbol.first_declaration_span().or_else(|| {
            if symbol.value_declaration.is_some() {
                symbol.value_declaration_span()
            } else {
                None
            }
        });
        match (span, info.span) {
            (Some(symbol_span), Some(info_span)) => symbol_span == info_span,
            (Some(_), None) => false,
            _ => true,
        }
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
                .definition_store
                .get(def_id)
                .is_some_and(|info| self.types.resolve_atom(info.name) == expected_name)
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
            file_id: Some(file_idx),
            span,
            symbol_id: Some(sym_id.0),
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        };

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
}
