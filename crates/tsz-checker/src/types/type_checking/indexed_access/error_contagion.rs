//! Error-type contagion for indexed access whose object is rooted at an
//! *unresolved imported alias* (e.g. `Simplify<…>`/`TupleParts<…>` from a
//! module that failed to resolve — already flagged `TS2307`).
//!
//! `tsc` gives such references the permissive `error` apparent type, whose key
//! space is the universal `string | number | symbol`, so indexing them — and
//! chaining further indices / spreads over the result — accepts any key. This
//! module centralizes the structural detection and the index-key-space
//! re-derivation that mirror that behavior, suppressing the spurious
//! `TS2536`/`TS2574` those positions would otherwise produce while keeping the
//! concrete-branch key-space restriction for well-formed conditionals. Split out
//! of `indexed_access_helpers.rs` to keep that file under the per-file LOC
//! ceiling; behavior is unchanged.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Whether `object_type` is a deferred conditional whose apparent type (tsc's
    /// `getDefaultConstraintOfConditionalType` — the union of branch results) has
    /// a concrete key space that admits the index key. Mirrors tsc's
    /// `getApparentType` resolution in `checkIndexedAccessIndexType`. Returns
    /// `false` (so normal validation/error paths proceed) when the object is not
    /// a deferred conditional, the constraint is unresolved, or the key is not in
    /// the constraint's key space.
    pub(crate) fn deferred_conditional_index_is_in_key_space(
        &mut self,
        object_type: TypeId,
        index_type_for_check: TypeId,
        index_type: TypeId,
    ) -> bool {
        let Some(constraint) = crate::query_boundaries::common::conditional_default_constraint(
            self.ctx.types,
            object_type,
        ) else {
            return false;
        };
        let evaluated_constraint = self.evaluate_type_with_env(constraint);
        // Error-type contagion: a branch of the conditional may be the application
        // of an *unresolved* imported alias (e.g. `Simplify<…>` when `type-fest`
        // is absent — already flagged TS2307). tsc resolves the access through
        // that branch's permissive `error` apparent type, whose key space is the
        // universal `string | number | symbol`, so the access accepts any key. For
        // a union of branches `keyof` distributes to the *intersection* of the
        // per-branch key spaces, and the error branch's universal key space is the
        // identity element, so only the concrete branches restrict the valid keys
        // (`universal ∩ concrete = concrete`). Handle that before the strict
        // ERROR/ANY/unresolved-keyof bails below would fall through to a spurious
        // TS2536.
        if let Some(result) = self.error_contagious_conditional_index_in_key_space(
            constraint,
            evaluated_constraint,
            index_type_for_check,
            index_type,
        ) {
            return result;
        }
        if evaluated_constraint == TypeId::ERROR || evaluated_constraint == TypeId::ANY {
            return false;
        }
        let keyof = self.ctx.types.evaluate_keyof(evaluated_constraint);
        if !self.indexed_access_key_space_is_resolved(keyof) {
            return false;
        }
        self.indexed_access_key_space_relation_outcome(index_type_for_check, keyof)
            .related
            || self
                .indexed_access_key_space_relation_outcome(index_type, keyof)
                .related
    }

    /// Whether the indexed-access object type is (or is rooted at) an *unresolved
    /// imported alias* — e.g. `TupleParts<T>` or `TupleParts<T>["required"]`
    /// where `TupleParts` comes from a module that failed to resolve (already
    /// flagged TS2307). tsc gives such a type the permissive `error` apparent
    /// type, whose key space is universal, so indexing it — and chaining further
    /// indices / spreads over the result — accepts any key. Used to suppress the
    /// spurious TS2536 those positions would otherwise produce.
    ///
    /// The detection walks the object type's *base spine* (peeling generic
    /// `Application`s and `IndexAccess` objects) so a deferred nested access whose
    /// root is an unresolved import is recognized, but a normal concrete object
    /// that merely contains an unresolved-import value somewhere is not — only the
    /// object actually being indexed must be error-typed.
    pub(crate) fn indexed_access_object_is_unresolved_import_error(
        &mut self,
        object_type: TypeId,
        object_type_for_check: TypeId,
    ) -> bool {
        self.index_object_base_spine_references_unresolved_import(object_type)
            || self.index_object_base_spine_references_unresolved_import(object_type_for_check)
    }

    /// Whether a conditional's branch-union (default) constraint is error-typed
    /// from contagion: it collapsed to `error`/`any`, or it (recursively)
    /// references an unresolved imported alias. Both forms mean a deferred index
    /// into the conditional has the permissive `error` apparent type, so chaining
    /// a further index over it accepts any key.
    fn constraint_is_error_contagious(&mut self, constraint: TypeId) -> bool {
        if constraint == TypeId::ERROR || constraint == TypeId::ANY {
            return true;
        }
        if self.type_references_unresolved_import(constraint) {
            return true;
        }
        let evaluated = self.evaluate_type_with_env(constraint);
        evaluated == TypeId::ERROR
            || evaluated == TypeId::ANY
            || self.type_references_unresolved_import(evaluated)
    }

    /// Walk the base spine of an indexed-access object type, peeling generic
    /// `Application`s and `IndexAccess` bases, and report whether the *root* of
    /// the spine is itself an unresolved-import reference / application.
    ///
    /// Crucially this only fires when the spine root is *directly* the unresolved
    /// alias (e.g. `TupleParts<T>` or `TupleParts<T>["required"]`). A conditional
    /// object — even one with an error-typed branch (`Cond<T> = … : Simplify<…>`)
    /// — is *not* treated as error here: those are owned by
    /// [`Self::deferred_conditional_index_is_in_key_space`], which keeps the
    /// concrete branches' key-space restriction (so `Cond<T>["missing"]` still
    /// emits TS2536). Bounded against pathological nesting.
    fn index_object_base_spine_references_unresolved_import(&mut self, ty: TypeId) -> bool {
        use crate::query_boundaries::common as q;
        let mut current = ty;
        // Whether we have already peeled at least one *indexed-access* layer. A
        // top-level conditional object (`Cond<T>[k]`) is owned by the per-branch
        // key-space path, so it must NOT be blanket-suppressed here. But once an
        // inner index has been applied (`Cond<T>[k1][k2]`), the inner
        // `Cond<T>[k1]` selected the conditional's branch union — and if that
        // union is error-contagious its apparent type is `error`, so the outer
        // `[k2]` accesses an error type and accepts any key.
        let mut peeled_index = false;
        for _ in 0..64 {
            if q::is_conditional_type(self.ctx.types, current) {
                // Only treat a conditional as error-typed when reached *through* an
                // inner index and its branch union references an unresolved import.
                if !peeled_index {
                    return false;
                }
                let Some(constraint) =
                    crate::query_boundaries::common::conditional_default_constraint(
                        self.ctx.types,
                        current,
                    )
                else {
                    return false;
                };
                return self.constraint_is_error_contagious(constraint);
            }
            if let Some((base, _args)) = q::application_info(self.ctx.types, current) {
                // The application is error-typed when its *base* (the alias being
                // applied) is the unresolved import — not when an *argument*
                // merely carries one.
                if self.spine_node_is_unresolved_import_reference(base) {
                    return true;
                }
                // A generic application of a *conditional* alias (e.g.
                // `TupleParts<T, []>`) whose branch union is error-contagious has
                // an `error` apparent type once an inner index has been applied.
                // Its branch union is reachable through the conditional default
                // constraint of the application itself (the alias body is resolved
                // internally), so check that before giving up on a non-reducing
                // generic application.
                if peeled_index
                    && let Some(constraint) =
                        crate::query_boundaries::common::conditional_default_constraint(
                            self.ctx.types,
                            current,
                        )
                    && self.constraint_is_error_contagious(constraint)
                {
                    return true;
                }
                // Peel a generic application whose body reduces to a further
                // indexed access / application / conditional (alias wrappers); a
                // concrete body stops the walk.
                let expanded = self.evaluate_application_type(current);
                if expanded != current
                    && (q::is_index_access_type(self.ctx.types, expanded)
                        || q::is_generic_application(self.ctx.types, expanded)
                        || q::is_conditional_type(self.ctx.types, expanded))
                {
                    current = expanded;
                    continue;
                }
                return false;
            }
            if let Some((inner_base, _index)) = q::index_access_types(self.ctx.types, current) {
                peeled_index = true;
                current = inner_base;
                continue;
            }
            // Spine bottoms out: a bare unresolved-import reference
            // (Lazy/UnresolvedTypeName) is error-typed.
            return self.spine_node_is_unresolved_import_reference(current);
        }
        false
    }

    /// Whether a single spine node is *itself* an unresolved-import reference: a
    /// `Lazy(DefId)`/`UnresolvedTypeName` (optionally wrapped in an application
    /// whose base is one) bound to an `import` from an unresolvable module. Unlike
    /// the recursive [`CheckerState::type_references_unresolved_import`], this does
    /// NOT descend into a composite shape (union/conditional/object), so a
    /// concrete type that merely *contains* an unresolved import deeper inside is
    /// not misclassified as error-typed.
    fn spine_node_is_unresolved_import_reference(&self, node: TypeId) -> bool {
        use crate::query_boundaries::common as q;
        let candidate = match q::application_info(self.ctx.types, node) {
            Some((base, _)) => base,
            None => node,
        };
        if crate::query_boundaries::spread::unresolved_type_name_atom(self.ctx.types, candidate)
            .is_some()
        {
            return true;
        }
        q::lazy_def_id(self.ctx.types, candidate)
            .and_then(|def_id| self.ctx.def_to_symbol_id(def_id))
            .is_some_and(|sym_id| self.is_unresolved_import_symbol_id(sym_id))
    }

    /// Error-type contagion handling for
    /// [`Self::deferred_conditional_index_is_in_key_space`].
    ///
    /// When a deferred conditional's default-constraint (branch union) carries an
    /// *unresolved-alias application* member, tsc treats that branch's apparent
    /// type as the permissive `error` type whose key space is universal. This
    /// re-derives the validation key space as the intersection of the *concrete*
    /// branches' key spaces only (the error branches contribute the universal
    /// identity), exactly matching tsc's `keyof (error | concrete) = concrete`.
    ///
    /// Returns:
    /// - `None` when no branch is error-contagious — the caller keeps its strict
    ///   path, so well-formed conditionals (including the
    ///   `keyof(A | B) = keyof A ∩ keyof B` shared-key case) are unaffected;
    /// - `Some(true)` when every branch is error-contagious (the whole branch
    ///   union collapsed to `error`/an unresolved application): any key is
    ///   accepted, matching `keyof error = string | number | symbol`;
    /// - `Some(true | false)` from validating the index against the intersection
    ///   of the concrete branches' key spaces otherwise. When that key space is
    ///   still not concrete, returns `None` to defer to the caller.
    fn error_contagious_conditional_index_in_key_space(
        &mut self,
        constraint: TypeId,
        evaluated_constraint: TypeId,
        index_type_for_check: TypeId,
        index_type: TypeId,
    ) -> Option<bool> {
        use crate::query_boundaries::common as q;
        // A branch is error-contagious when it (recursively) references an
        // unresolved imported alias — an `Application`/reference whose base is a
        // `Lazy(DefId)` mapping to an unresolved-import symbol, or an
        // `UnresolvedTypeName`. tsc gives such a branch the permissive `error`
        // apparent type. (`TypeId::ERROR` itself is also treated as error here.)
        let is_error_contagious = |this: &Self, ty: TypeId| -> bool {
            ty == TypeId::ERROR || this.type_references_unresolved_import(ty)
        };
        // Decompose both the raw and evaluated branch unions; the unresolved-alias
        // application survives in one or the other depending on how far evaluation
        // reduced the branches. Prefer the evaluated form when it still carries the
        // union shape, else fall back to the raw constraint.
        let members: Vec<TypeId> = q::union_members(self.ctx.types, evaluated_constraint)
            .or_else(|| q::union_members(self.ctx.types, constraint))
            .map(|list| list.iter().copied().collect())
            .unwrap_or_else(|| vec![evaluated_constraint, constraint]);
        if !members.iter().any(|&m| is_error_contagious(self, m)) {
            return None;
        }
        let concrete: Vec<TypeId> = members
            .into_iter()
            .filter(|&m| !is_error_contagious(self, m))
            .collect();
        if concrete.is_empty() {
            // Every branch is error-typed -> universal key space -> any key valid.
            return Some(true);
        }
        // `evaluate_keyof` over the union of the concrete branches already computes
        // their key-space intersection.
        let concrete_union = self.ctx.types.union(concrete);
        let keyof_concrete = self.ctx.types.evaluate_keyof(concrete_union);
        if !self.indexed_access_key_space_is_resolved(keyof_concrete) {
            return None;
        }
        Some(
            self.indexed_access_key_space_relation_outcome(index_type_for_check, keyof_concrete)
                .related
                || self
                    .indexed_access_key_space_relation_outcome(index_type, keyof_concrete)
                    .related,
        )
    }
}
