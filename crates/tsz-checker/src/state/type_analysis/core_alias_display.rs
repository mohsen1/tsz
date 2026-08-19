//! Alias display provenance helpers.

use crate::query_boundaries::checkers::generic as generic_query;
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_solver::{TypeId, def::DefId};

impl CheckerState<'_> {
    pub(super) fn mark_tuple_spread_flattened_alias_def(
        &mut self,
        sym_id: SymbolId,
        def_id: DefId,
        result: TypeId,
        alias_is_non_generic: bool,
    ) {
        let body_has_top_level_spread =
            alias_is_non_generic && self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                symbol.declarations.iter().any(|&decl_idx| {
                    super::source_alias_attribution::tuple_alias_declaration_body_has_top_level_spread(
                        self.ctx.arena,
                        decl_idx,
                    )
                })
            });
        if !body_has_top_level_spread
            || generic_query::contains_free_type_parameters(self.ctx.types, result)
        {
            return;
        }

        // A spread element flattens into a fresh tuple only when it spreads a
        // fixed tuple (`...[a, b]` or `...Inner` where `Inner` is a fixed tuple).
        // A rest array (`...number[]`) stays variadic and keeps its alias name.
        let evaluated = self.evaluate_type_with_env(result);
        let is_non_variadic_tuple = crate::query_boundaries::common::tuple_elements(
            self.ctx.types.as_type_database(),
            evaluated,
        )
        .is_some_and(|elements| !elements.iter().any(|element| element.rest));
        if is_non_variadic_tuple {
            self.ctx
                .definition_store
                .mark_tuple_spread_flattened_alias(def_id);
        }
    }

    /// Record a non-generic type alias whose declared body is a *bare*
    /// (argument-free) type reference resolving to a non-generic interface or
    /// class declaration. `tsc` attaches no `aliasSymbol` to the declaration's
    /// shared nominal type, so diagnostics render the declaration's own name
    /// (`type IA = Iface` renders `Iface`; `type CA = Cls` renders `Cls`).
    ///
    /// The record is keyed per alias def because the resolved body may flatten
    /// to the declaration's structural shape (class instance types and alias
    /// chains do), erasing which reference produced it. A generic declaration
    /// — even fully defaulted (`class GC<T = string>`; `type GCA = GC`) —
    /// instantiates a fresh reference that keeps the alias symbol, so those
    /// are excluded and the alias keeps its declared name.
    pub(super) fn mark_bare_nominal_ref_alias_def(
        &mut self,
        sym_id: SymbolId,
        def_id: DefId,
        result: TypeId,
        alias_is_non_generic: bool,
    ) {
        let body_is_bare_reference = alias_is_non_generic
            && self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                symbol.declarations.iter().any(|&decl_idx| {
                    super::source_alias_attribution::alias_declaration_body_is_bare_type_reference(
                        self.ctx.arena,
                        decl_idx,
                    )
                })
            });
        if !body_is_bare_reference {
            return;
        }
        if let Some(target_def) = self.bare_nominal_ref_display_target(result) {
            self.ctx
                .definition_store
                .record_bare_nominal_ref_alias(def_id, target_def);
        }
    }

    /// Resolve the alias body `result` to the non-generic interface/class
    /// declaration it names, if any: a still-deferred `Lazy` declaration ref,
    /// a `Lazy` ref to another alias that already recorded such a target (an
    /// alias chain), or a resolved nominal shape carrying its declaring
    /// symbol.
    fn bare_nominal_ref_display_target(&self, result: TypeId) -> Option<DefId> {
        if let Some(body_def) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, result)
        {
            let def = self.ctx.definition_store.get(body_def)?;
            return match def.kind {
                tsz_solver::def::DefKind::Interface | tsz_solver::def::DefKind::Class => (def
                    .type_params
                    .is_empty()
                    && self
                        .nominal_symbol_declarations_are_non_generic(def.symbol_id.map(SymbolId)))
                .then_some(body_def),
                // An alias-to-alias reference that stayed deferred: reuse the
                // referenced alias' already-vetted record (bodies resolve
                // bottom-up, so the inner alias published first).
                tsz_solver::def::DefKind::TypeAlias => self
                    .ctx
                    .definition_store
                    .bare_nominal_ref_alias_target(body_def),
                _ => None,
            };
        }
        let shape = crate::query_boundaries::common::object_shape_for_type(self.ctx.types, result)?;
        let shape_sym = shape.symbol?;
        let symbol = self.ctx.binder.get_symbol(shape_sym)?;
        if !symbol.has_any_flags(symbol_flags::INTERFACE | symbol_flags::CLASS) {
            return None;
        }
        if !self.nominal_symbol_declarations_are_non_generic(Some(shape_sym)) {
            return None;
        }
        let target_def = self.ctx.symbol_to_def.borrow().get(&shape_sym).copied()?;
        let target = self.ctx.definition_store.get(target_def)?;
        (matches!(
            target.kind,
            tsz_solver::def::DefKind::Interface | tsz_solver::def::DefKind::Class
        ) && target.type_params.is_empty())
        .then_some(target_def)
    }

    /// True when every interface/class declaration of `sym_id` declares no
    /// type parameters. Merged declarations count: a single generic
    /// declaration makes the whole nominal generic, and a bare reference to a
    /// generic declaration keeps its alias name. `None`/unresolvable symbols
    /// decline conservatively.
    fn nominal_symbol_declarations_are_non_generic(&self, sym_id: Option<SymbolId>) -> bool {
        let Some(sym_id) = sym_id else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        !symbol.declarations.is_empty()
            && symbol.declarations.iter().all(|&decl_idx| {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    return false;
                };
                if let Some(interface) = self.ctx.arena.get_interface(node) {
                    return interface
                        .type_parameters
                        .as_ref()
                        .is_none_or(|params| params.nodes.is_empty());
                }
                if let Some(class) = self.ctx.arena.get_class(node) {
                    return class
                        .type_parameters
                        .as_ref()
                        .is_none_or(|params| params.nodes.is_empty());
                }
                // A merged non-type declaration (e.g. a namespace) does not
                // make the nominal generic.
                true
            })
    }

    pub(crate) fn symbol_is_type_alias(&self, sym_id: SymbolId) -> bool {
        self.ctx
            .binder
            .symbols
            .get(sym_id)
            .is_some_and(|s| s.has_any_flags(symbol_flags::TYPE_ALIAS))
    }
}
