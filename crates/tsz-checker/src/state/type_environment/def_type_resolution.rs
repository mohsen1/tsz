//! `DefId` -> concrete-type resolution with type-environment insertion.
//!
//! Split from `lazy.rs` to keep that file under the architecture size
//! ceiling; `resolve_and_insert_def_type` is the "resolve a `Lazy(DefId)`
//! and publish the answer into the type environment" entry consumed by
//! `evaluate_type_with_resolution` and relation preparation.

use crate::query_boundaries::definition_identity::is_lazy_def_identity;
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Resolve a `DefId` to a concrete type and insert a `DefId` mapping into the type environment.
    ///
    /// Returns the resolved type when a symbol bridge exists; returns `None` when the `DefId`
    /// is unknown to the checker. For `ANY`/`ERROR`, we intentionally skip env insertion.
    pub(crate) fn resolve_and_insert_def_type(
        &mut self,
        def_id: tsz_solver::DefId,
    ) -> Option<TypeId> {
        // Most relation-readiness calls resolve program definitions. Do not
        // enter the actual-lib materializer unless both structural preconditions
        // already hold; the helper repeats the checks as its authority guard.
        if self.ctx.has_lib_loaded()
            && self.ctx.definition_store.def_is_non_program(def_id)
            && let Some(body) = self.materialize_actual_lib_alias_body(def_id)
        {
            self.try_insert_def_in_type_env(def_id, body);
            return Some(body);
        }

        let lib_name = self.ctx.definition_store.get(def_id).and_then(|info| {
            (info.file_id == Some(u32::MAX)).then(|| self.ctx.types.resolve_atom(info.name))
        });
        if let Some(name) = lib_name
            && Self::in_cross_arena_interface_delegation()
            && self.ctx.has_lib_loaded()
        {
            if let Some(resolved) = self.resolve_lib_type_by_name(&name) {
                self.try_insert_def_in_type_env(def_id, resolved);
                return Some(resolved);
            }
            return Some(self.ctx.types.lazy(def_id));
        }

        if let Some(body) = self.published_program_alias_body(def_id) {
            if body != TypeId::ANY {
                self.try_insert_def_in_type_env(def_id, body);
            }
            return Some(body);
        }

        let (sym_id, owner_file_idx) = self.ctx.def_symbol_identity(def_id)?;
        if let Some(file_idx) = owner_file_idx
            && file_idx != self.ctx.current_file_idx
        {
            self.ctx.register_symbol_file_target(sym_id, file_idx);
        }
        let resolved = if let Some(symbol) = self.get_cross_file_symbol(sym_id) {
            if symbol.has_any_flags(symbol_flags::CLASS) {
                // Keep class references in type position as instance types to avoid
                // constructor/instance split diagnostics (e.g. `Type 'Dataset' is not
                // assignable to type 'Dataset'` in parser harness regressions).
                // Also check class_instance_type_cache for in-progress builds
                // (Phase 2 partial type), preventing constructor type fallback.
                self.ctx
                    .symbol_instance_types
                    .get(&sym_id)
                    .or_else(|| {
                        symbol.primary_declaration().and_then(|idx| {
                            self.ctx
                                .class_instance_type_cache
                                .borrow()
                                .get(&idx)
                                .copied()
                        })
                    })
                    .or_else(|| {
                        owner_file_idx
                            .filter(|file_idx| *file_idx != self.ctx.current_file_idx)
                            .and_then(|file_idx| {
                                self.ctx
                                    .cached_cross_file_class_instance_type(sym_id, file_idx as u32)
                                    .map(|(instance_type, _)| instance_type)
                            })
                    })
                    .or_else(|| {
                        owner_file_idx
                            .filter(|file_idx| *file_idx != self.ctx.current_file_idx)
                            .and_then(|_| {
                                self.delegate_cross_arena_class_instance_type(sym_id)
                                    .map(|(instance_type, _)| instance_type)
                            })
                    })
                    .unwrap_or_else(|| {
                        // Trigger symbol typing (which builds the instance as a
                        // side effect when it can), then prefer that instance.
                        // Never fall back to a constructor-shaped VALUE result:
                        // it is not the type-position meaning of a class
                        // reference, and substituting it here fails constraint
                        // checks the instance satisfies (#17570). Keep the
                        // deferred `Lazy` instead so a later resolution — after
                        // the class statement finishes building — sees the
                        // real instance type.
                        let value_side = self.get_type_of_symbol(sym_id);
                        if let Some(instance) = self.ctx.symbol_instance_types.get(&sym_id) {
                            instance
                        } else if self.ctx.is_class_value_side_body(def_id, value_side) {
                            crate::class_type::note_class_self_reference_deferral();
                            self.ctx.types.lazy(def_id)
                        } else {
                            value_side
                        }
                    })
            } else if symbol.has_any_flags(symbol_flags::VARIABLE)
                && !symbol.has_any_flags(symbol_flags::TYPE)
            {
                // `typeof value` can be represented as a Lazy(value DefId) once it
                // flows through aliases/mapped types instead of as a direct
                // TypeQuery(SymbolRef). Relation prep must register the value-space
                // type for value-only DefIds so solver-side mapped/keyof evaluation
                // does not classify the unresolved Lazy as a non-object. Merged
                // interface/var symbols still have type-space meaning, so bare type
                // references like `HTMLDivElement` must continue resolving to the
                // instance type instead of the constructor value.
                self.type_of_value_declaration_for_symbol(sym_id, symbol.value_declaration)
            } else {
                self.get_type_of_symbol(sym_id)
            }
        } else {
            self.get_type_of_symbol(sym_id)
        };

        // If `get_type_of_symbol` returned the Lazy placeholder for this same def_id
        // (cycle-break), inserting it into `type_env` would shadow the DefinitionStore
        // fallback and cause the `resolved == type_id` guard in the caller to short-circuit.
        // Prefer the concrete body from DefinitionStore when it is already available.
        if is_lazy_def_identity(self.ctx.types, resolved, def_id) {
            if let Some(body) = self.ctx.definition_store.get_body(def_id)
                && body != resolved
                && body != TypeId::ERROR
                && body != TypeId::ANY
                // A class def's published body can be the VALUE (constructor)
                // side — never a valid type-position resolution (#17570).
                && !self.ctx.is_class_value_side_body(def_id, body)
            {
                self.try_insert_def_in_type_env(def_id, body);
                return Some(body);
            }
            return Some(resolved);
        }

        if resolved != TypeId::ERROR && resolved != TypeId::ANY {
            // Carry type params so Application evaluation via TypeEnvironment can
            // instantiate generic types correctly across checker contexts.
            self.try_insert_def_in_type_env(def_id, resolved);
        }
        Some(resolved)
    }
}
