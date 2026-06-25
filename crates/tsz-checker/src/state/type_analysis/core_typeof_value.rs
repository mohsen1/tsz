//! Value-space type resolution for `typeof X` queries on merged
//! interface+value symbols (declaration merging).
//!
//! A symbol declared as both an interface and a value (e.g. lib `Date`/`Request`
//! through `interface Date {} declare var Date: DateConstructor`, or user
//! `interface Foo {} declare var Foo: {...}`) stores its TYPE-space (instance)
//! interface type under the shared `SymbolRef`/`DefId`, because that is what
//! type-position references (`x: Date`) need. A `typeof X` query needs the
//! VALUE-space type (the var's type) instead. `get_type_of_symbol` registers
//! the value type computed here in the environment's `typeof_value_types` map so
//! the solver's `resolve_type_query` returns the value/constructor side for a
//! deferred `TypeQuery(SymbolRef)` produced by nested `typeof` positions
//! (indexed-access, conditional, tuple).

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Compute the VALUE-space type a `typeof X` query should resolve to for a
    /// symbol whose value meaning is merged with a deferred type-space
    /// declaration sharing its `SymbolRef`/`DefId`.
    ///
    /// `typeof X` is always value-space, but `resolve_ref`/`symbol_types` holds
    /// the TYPE-space type when the symbol also carries an interface or type
    /// alias declaration. Both forms occur in real code:
    /// - interface+value: lib `Date`/`Request`, or user `interface Foo {}
    ///   declare var Foo: {...}` — the instance interface type is stored.
    /// - type-alias+value: the fp-ts higher-kinded-types tag idiom `const URI =
    ///   "IOEither"; type URI = typeof URI` — the (self-referential) alias body
    ///   is stored, so resolving `typeof URI` through it cycles to `undefined`.
    ///
    /// Returns the lib `*Constructor` companion when present (e.g. `Date` ->
    /// `DateConstructor`), otherwise the value declaration's type. Returns `None`
    /// for non-merged symbols, class merges (whose constructor type is already
    /// routed through the class env entry), and when no value type can be
    /// determined — leaving the normal `SymbolRef`/`DefId` resolution untouched.
    pub(crate) fn merged_interface_value_typeof_type(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !(symbol.has_any_flags(symbol_flags::VALUE)
            && symbol.has_any_flags(symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS))
        {
            return None;
        }
        // A class+interface merge already routes the constructor type through the
        // class env entry; only non-class value merges need this.
        if symbol.has_any_flags(symbol_flags::CLASS) {
            return None;
        }
        let name = symbol.escaped_name.clone();
        let value_decl = symbol.value_declaration;
        let declarations = symbol.declarations.clone();

        // Lib globals model the value side through a sibling `*Constructor`
        // interface (e.g. `Date` -> `DateConstructor`). Prefer it so the value
        // type is the constructor rather than the instance interface.
        if self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
            && (self.is_known_global_value_name(&name)
                || tsz_binder::lib_loader::is_es2015_plus_type(&name))
        {
            let constructor_name = format!("{name}Constructor");
            if let Some(ctor) = self
                .resolve_lib_type_by_name(&constructor_name)
                .filter(|&ty| ty != TypeId::UNKNOWN && ty != TypeId::ERROR)
            {
                return Some(ctor);
            }
        }

        // Otherwise use the value declaration's type (the `var`/`function`
        // declaration), which carries the value-space type for the merge.
        let preferred_value_decl = self
            .preferred_value_declaration(sym_id, value_decl, &declarations)
            .unwrap_or(value_decl);
        let value_type = if let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id)
            && file_idx != self.ctx.current_file_idx
        {
            self.type_of_value_declaration_for_cross_file_symbol(
                sym_id,
                preferred_value_decl,
                file_idx,
            )
        } else {
            self.type_of_value_declaration_for_symbol(sym_id, preferred_value_decl)
        };
        (value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR && value_type != TypeId::ANY)
            .then_some(value_type)
    }

    /// Register a merged interface+value symbol's `typeof` value-space type in
    /// both resolver environments (`type_env` for the evaluator, `type_environment`
    /// for the flow analyzer) so `resolve_type_query` returns it consistently.
    pub(crate) fn register_typeof_value_type_in_envs(
        &mut self,
        sym_id: SymbolId,
        value_type: TypeId,
    ) {
        let symbol_ref = tsz_solver::SymbolRef(sym_id.0);
        self.ctx
            .register_typeof_value_type_in_envs(symbol_ref, value_type);
    }
}
