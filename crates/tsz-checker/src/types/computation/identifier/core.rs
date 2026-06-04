//! Core identifier type computation — `get_type_of_identifier` and its
//! direct helpers (TDZ, definite assignment, flow narrowing).

include!("core_large_methods/get_type_of_identifier_with_request_9_7.rs");

use crate::context::{PendingImplicitAnyKind, TypingRequest};
use crate::query_boundaries::common as common_query;
use crate::query_boundaries::type_computation::complex as query;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tracing::trace;
use tsz_binder::symbol_flags;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn identifier_is_property_access_receiver(&self, idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(idx) else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
            return false;
        };
        matches!(
            parent_node.kind,
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        ) && self
            .ctx
            .arena
            .get_access_expr(parent_node)
            .is_some_and(|access| access.expression == idx)
    }

    fn should_preserve_declared_generic_index_access_for_fresh_flow(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        declared_type: TypeId,
        flow_type: TypeId,
    ) -> bool {
        if !self.symbol_value_decl_has_explicit_type_annotation(sym_id) {
            return false;
        }

        let Some((_, index_type)) = common_query::index_access_parts(self.ctx.types, declared_type)
        else {
            return false;
        };

        let has_type_parameter_key =
            common_query::is_type_parameter_like(self.ctx.types, index_type)
                || common_query::is_type_parameter_like(
                    self.ctx.types,
                    self.resolve_lazy_type(index_type),
                );
        has_type_parameter_key
            && (flow_type == TypeId::ANY
                || common_query::is_fresh_object_type(self.ctx.types, flow_type))
    }

    fn import_equals_alias_value_type(&mut self, sym_id: tsz_binder::SymbolId) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(symbol_flags::ALIAS) {
            return None;
        }

        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .iter()
                .copied()
                .find(|idx| idx.is_some())
                .unwrap_or(NodeIndex::NONE)
        };
        if !decl_idx.is_some() {
            return None;
        }
        let decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return None;
        }
        let import = self.ctx.arena.get_import_decl(decl_node)?;
        if let Some(module_specifier) = self.get_require_module_specifier(import.module_specifier) {
            if let Some(exports) = self
                .resolve_effective_module_exports_from_file(
                    &module_specifier,
                    Some(self.ctx.current_file_idx),
                )
                .or_else(|| self.resolve_effective_module_exports(&module_specifier))
                && let Some(export_equals_sym) = exports.get("export=")
            {
                let mut candidates = Vec::new();

                if let Some(export_equals_symbol) = self.get_cross_file_symbol(export_equals_sym)
                    && export_equals_symbol.has_any_flags(symbol_flags::ALIAS)
                {
                    let mut visited = AliasCycleTracker::new();
                    if let Some(resolved) =
                        self.resolve_alias_symbol(export_equals_sym, &mut visited)
                    {
                        candidates.push(resolved);
                    }
                }
                candidates.push(export_equals_sym);

                for candidate in candidates {
                    let ty = self.get_type_of_symbol(candidate);
                    if ty != TypeId::UNKNOWN && ty != TypeId::ERROR {
                        return Some(ty);
                    }
                }
            }

            return self
                .commonjs_module_value_type(&module_specifier, Some(self.ctx.current_file_idx));
        }

        let target_sym = self.resolve_qualified_symbol(import.module_specifier)?;
        let mut candidates = Vec::new();
        if let Some(partner) = self.ctx.alias_partner_for(self.ctx.binder, target_sym) {
            candidates.push(partner);
        }

        let mut visited = AliasCycleTracker::new();
        if let Some(resolved) = self.resolve_alias_symbol(target_sym, &mut visited) {
            if let Some(partner) = self.ctx.alias_partner_for(self.ctx.binder, resolved) {
                candidates.push(partner);
            }
            candidates.push(resolved);
        }
        candidates.push(target_sym);

        let lib_binders = self.get_lib_binders();
        let mut seen = rustc_hash::FxHashSet::default();
        for candidate in candidates {
            if !seen.insert(candidate) {
                continue;
            }
            let Some(candidate_symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(candidate, &lib_binders)
            else {
                continue;
            };
            if !candidate_symbol.has_any_flags(symbol_flags::VALUE | symbol_flags::ALIAS)
                && candidate_symbol.value_declaration.is_none()
            {
                continue;
            }

            let mut ty = candidate_symbol
                .value_declaration
                .is_some()
                .then(|| {
                    self.ctx
                        .arena
                        .get(candidate_symbol.value_declaration)
                        .and_then(|node| self.ctx.arena.get_variable_declaration(node))
                        .map(|decl| decl.initializer)
                        .filter(|initializer| initializer.is_some())
                })
                .flatten()
                .map(|initializer| self.get_type_of_node(initializer))
                .unwrap_or(TypeId::UNKNOWN);
            if (ty == TypeId::UNKNOWN || ty == TypeId::ERROR)
                && candidate_symbol.value_declaration.is_some()
                && candidate_symbol.has_any_flags(symbol_flags::VALUE)
            {
                ty = if self
                    .ctx
                    .arena
                    .get(candidate_symbol.value_declaration)
                    .is_some()
                {
                    self.type_of_value_declaration_for_symbol(
                        candidate,
                        candidate_symbol.value_declaration,
                    )
                } else if let Some(file_idx) = self.ctx.resolve_symbol_file_index(candidate) {
                    self.type_of_value_declaration_for_cross_file_symbol(
                        candidate,
                        candidate_symbol.value_declaration,
                        file_idx,
                    )
                } else {
                    TypeId::UNKNOWN
                };
            }
            if ty == TypeId::UNKNOWN || ty == TypeId::ERROR {
                ty = self.get_type_of_symbol(candidate);
            }
            if ty != TypeId::UNKNOWN && ty != TypeId::ERROR {
                return Some(ty);
            }
        }

        None
    }

    pub(crate) fn same_file_value_symbol_for_type_symbol(
        &self,
        type_sym_id: tsz_binder::SymbolId,
    ) -> Option<(tsz_binder::SymbolId, NodeIndex, usize)> {
        let type_symbol = self.get_symbol_globally(type_sym_id)?;
        if (type_symbol.flags & tsz_binder::symbol_flags::VALUE) != 0 {
            return None;
        }
        let file_idx = self.ctx.resolve_symbol_file_index(type_sym_id)?;
        let binder = self.ctx.get_binder_for_file(file_idx)?;
        for &candidate_id in binder
            .get_symbols()
            .find_all_by_name(&type_symbol.escaped_name)
        {
            if candidate_id == type_sym_id {
                continue;
            }
            let Some(candidate) = binder.get_symbol(candidate_id) else {
                continue;
            };
            if candidate.escaped_name != type_symbol.escaped_name
                || (candidate.flags & tsz_binder::symbol_flags::VALUE) == 0
                || (candidate.flags & tsz_binder::symbol_flags::ALIAS) != 0
                || candidate.import_module.is_some()
                || !candidate.value_declaration.is_some()
            {
                continue;
            }
            self.ctx.register_symbol_file_target(candidate_id, file_idx);
            return Some((candidate_id, candidate.value_declaration, file_idx));
        }
        None
    }

    pub(crate) fn local_current_file_value_symbol_named(
        &self,
        name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        self.ctx
            .binder
            .get_symbols()
            .find_all_by_name(name)
            .iter()
            .copied()
            .find(|&candidate_id| {
                self.ctx
                    .binder
                    .get_symbol(candidate_id)
                    .is_some_and(|candidate| {
                        candidate.has_any_flags(symbol_flags::VALUE)
                            && candidate.value_declaration.is_some()
                            && (candidate.decl_file_idx == u32::MAX
                                || candidate.decl_file_idx == self.ctx.current_file_idx as u32)
                    })
            })
    }

    fn has_recursive_alias_shape_for_flow_compare(&self, type_id: TypeId) -> bool {
        common_query::contains_lazy_or_recursive(self.ctx.types.as_type_database(), type_id)
    }

    /// Get the type of an identifier expression.
    ///
    /// This function resolves the type of an identifier by:
    /// 1. Looking up the symbol through the binder
    /// 2. Getting the declared type of the symbol
    /// 3. Checking for TDZ (temporal dead zone) violations
    /// 4. Checking definite assignment for block-scoped variables
    /// 5. Applying flow-based type narrowing
    ///
    /// ## Symbol Resolution:
    /// - Uses `resolve_identifier_symbol` to find the symbol
    /// - Checks for type-only aliases (error if used as value)
    /// - Validates that symbol has a value declaration
    ///
    /// ## TDZ Checking:
    /// - Static block TDZ: variable used in static block before declaration
    /// - Computed property TDZ: variable in computed property before declaration
    /// - Heritage clause TDZ: variable in extends/implements before declaration
    ///
    /// ## Definite Assignment:
    /// - Checks if variable is definitely assigned before use
    /// - Only applies to block-scoped variables without initializers
    /// - Skipped for parameters, ambient contexts, and captured variables
    ///
    /// ## Flow Narrowing:
    /// - If definitely assigned, applies type narrowing based on control flow
    /// - Refines union types based on typeof guards, null checks, etc.
    ///
    /// ## Intrinsic Names:
    /// - `undefined` → UNDEFINED type
    /// - `NaN` / `Infinity` → NUMBER type
    /// - `Symbol` → Symbol constructor type (if available in lib)
    ///
    /// ## Global Value Names:
    /// - Returns ANY for available globals (Array, Object, etc.)
    /// - Emits error for unavailable ES2015+ types
    ///
    /// ## Error Handling:
    /// - Returns ERROR for:
    ///   - Type-only aliases used as values
    ///   - Variables used before declaration (TDZ)
    ///   - Variables not definitely assigned
    ///   - Static members accessed without `this`
    ///   - `await` in default parameters
    ///   - Unresolved names (with "cannot find name" error)
    /// - Returns ANY for unresolved imports (TS2307 already emitted)
    pub(crate) fn get_type_of_identifier(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_identifier_with_request(idx, &TypingRequest::NONE)
    }

    __tsz_split_core_get_type_of_identifier_with_request_9_7!();
}
