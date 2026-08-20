//! Candidate-set query predicates for `InferenceContext`.
//!
//! Read-only (modulo union-find path compression) questions about a
//! variable's collected candidates and contra-candidates, split from
//! `infer.rs` to keep that shard under the architecture size cap.

use super::infer::{ConstraintSet, InferenceContext, InferenceVar};
use crate::types::{InferencePriority, TypeData, TypeId};
use crate::visitor::contains_type_parameter_named;
use tsz_common::Atom;

impl<'a> InferenceContext<'a> {
    /// Get the constraints for a variable
    pub fn get_constraints(&mut self, var: InferenceVar) -> Option<ConstraintSet> {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        if info.is_empty() {
            None
        } else {
            Some(ConstraintSet::from_info(&info))
        }
    }

    /// Check whether an inference variable has any candidates (covariant or contravariant).
    pub fn var_has_candidates(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        !info.candidates.is_empty() || !info.contra_candidates.is_empty()
    }

    /// Check whether an inference variable has `contra_candidates` with at least one
    /// concrete (non-TypeParameter) type. `TypeParameter` types in `contra_candidates`
    /// are typically unresolved source inference placeholders from generic function
    /// arguments and should not drive the resolution gate.
    #[expect(dead_code)] // Reserved contra-candidate resolution-gate query
    pub fn has_concrete_contra_candidates(
        &mut self,
        var: InferenceVar,
        db: &dyn crate::caches::db::TypeDatabase,
    ) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.contra_candidates.iter().any(|c| {
            c.type_id.is_intrinsic()
                || !matches!(db.lookup(c.type_id), Some(TypeData::TypeParameter(_)))
        })
    }

    /// Returns `true` if `type_id` is a **call-local** bare inference placeholder —
    /// a bare `__infer_*` `TypeParameter` whose name-atom is registered in this
    /// context's `type_params`. Placeholders from outer generic call scopes have
    /// atoms that are not in `type_params` and must not be filtered: they carry
    /// real cross-call inference evidence (e.g. a recursive call's argument type
    /// constrained by the outer function's unresolved type parameter).
    pub(crate) fn is_local_inference_placeholder(&self, type_id: TypeId) -> bool {
        if !crate::type_queries::data::is_bare_current_infer_placeholder_db(self.interner, type_id)
        {
            return false;
        }
        match self.interner.lookup(type_id) {
            // `TypeData::Infer` nodes are always created within the current context.
            Some(TypeData::TypeParameter(tp)) => {
                self.type_params.iter().any(|(atom, _, _)| *atom == tp.name)
            }
            _ => true,
        }
    }

    /// Check whether an inference variable has any contravariant candidates that are
    /// usable for resolution. Call-local inference placeholders like `__infer_*`
    /// are excluded, but higher-order source placeholders (`__infer_src_*`) and real
    /// outer type parameters are preserved because they carry cross-generic evidence.
    pub fn has_usable_contra_candidates(
        &mut self,
        var: InferenceVar,
        _db: &dyn crate::caches::db::TypeDatabase,
    ) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.contra_candidates
            .iter()
            .any(|c| !self.is_local_inference_placeholder(c.type_id))
    }

    /// Returns `true` when `candidate` should be kept as a concrete
    /// contra-variance candidate. Call-local `__infer_*` placeholders are
    /// excluded; foreign bare placeholders and composite types that contain
    /// real type parameters are kept.
    pub(crate) fn is_concrete_contra_candidate(&self, type_id: TypeId) -> bool {
        if self.is_local_inference_placeholder(type_id) {
            return false;
        }
        if crate::type_queries::data::is_bare_current_infer_placeholder_db(self.interner, type_id) {
            return true;
        }
        // Composite types built entirely from local placeholders are stale.
        if crate::type_queries::data::contains_current_infer_placeholder_db(self.interner, type_id)
            && !crate::type_queries::data::contains_non_infer_type_parameters_db(
                self.interner,
                type_id,
            )
        {
            return false;
        }
        true
    }

    /// Returns `true` if any covariant candidate for `var` is or contains an
    /// `IndexAccess` type (`T[K]` pattern). The circular-inference guard uses
    /// this to distinguish true circular inference (passing `T[K]` to `T`)
    /// from legitimate outer-`TypeParameter` forwarding (passing `T_outer` to
    /// `T_inner` where they happen to resolve to the same `TypeParameter`).
    pub fn has_index_access_covariant_candidate(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let db = self.interner;
        self.table
            .probe_value(root)
            .candidates
            .iter()
            .any(|c| type_contains_index_access(db, c.type_id))
    }

    /// Returns `true` when a covariant candidate for `var` is or contains an
    /// `IndexAccess` that references `var`'s OWN original declared type
    /// parameter — the circular self-inference signal of a recursive generic
    /// call, where `deepMap(value[key], fn)` infers `T` from `T[K]` with `T`
    /// being the very parameter under inference.
    ///
    /// The circular-inference guard normally recognizes this shape through a
    /// contravariant candidate contributed by the recursive call's other
    /// arguments (e.g. `fn: (v: T) => U`). In a self-recursive call, however,
    /// that contra candidate is the callee's OWN type parameter, so
    /// [`add_contra_candidate`](Self::add_contra_candidate) deliberately drops
    /// it as a non-informative self-reference. This covariant-side check keeps
    /// the guard able to fire without the suppressed contra candidate, while an
    /// independent call like `identity(value[key])` does not fire: there the
    /// index access references a *foreign* parameter (`deepMap`'s `T`), not
    /// `identity`'s own `U`.
    pub fn has_own_type_param_index_access_covariant_candidate(
        &mut self,
        var: InferenceVar,
    ) -> bool {
        let root = self.table.find(var);
        // Original declared names whose inference var unifies with this root.
        let entries: Vec<(Atom, InferenceVar)> = self
            .original_type_param_for_var
            .iter()
            .map(|(&name, &mapped)| (name, mapped))
            .collect();
        let own_names: Vec<Atom> = entries
            .into_iter()
            .filter(|&(_, mapped)| self.table.find(mapped) == root)
            .map(|(name, _)| name)
            .collect();
        if own_names.is_empty() {
            return false;
        }
        let candidate_types: Vec<TypeId> = self
            .table
            .probe_value(root)
            .candidates
            .iter()
            .map(|c| c.type_id)
            .collect();
        let db = self.interner;
        candidate_types.into_iter().any(|ty| {
            type_contains_index_access(db, ty)
                && own_names
                    .iter()
                    .any(|&name| contains_type_parameter_named(db, ty, name))
        })
    }

    /// Check whether a variable's inference came exclusively from contravariant positions.
    pub fn has_only_contra_candidates(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.candidates.is_empty() && !info.contra_candidates.is_empty()
    }

    /// Return deduplicated contravariant candidate types for an inference variable.
    pub fn get_contra_candidate_types(&mut self, var: InferenceVar) -> Vec<TypeId> {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        let mut out = Vec::with_capacity(info.contra_candidates.len());
        for candidate in &info.contra_candidates {
            if !out.contains(&candidate.type_id) {
                out.push(candidate.type_id);
            }
        }
        out
    }

    /// Return deduplicated contravariant candidate types for an inference
    /// variable, **excluding** those contributed by unannotated
    /// (context-sensitive) callback parameters (issue #17282). Such candidates
    /// carry no inference evidence in tsc, so the Round-1-fix restore must not
    /// treat them as divergent.
    pub fn get_annotated_contra_candidate_types(&mut self, var: InferenceVar) -> Vec<TypeId> {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        let mut out = Vec::with_capacity(info.contra_candidates.len());
        for candidate in &info.contra_candidates {
            if candidate.from_unannotated_callback_param {
                continue;
            }
            if !out.contains(&candidate.type_id) {
                out.push(candidate.type_id);
            }
        }
        out
    }

    /// Whether `var` has any contra-candidate contributed by an unannotated
    /// (context-sensitive) callback parameter (issue #17282).
    pub fn var_has_unannotated_contra_candidate(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.contra_candidates
            .iter()
            .any(|candidate| candidate.from_unannotated_callback_param)
    }

    pub fn has_index_signature_candidates(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.candidates
            .iter()
            .any(|candidate| candidate.from_index_signature)
    }

    /// Check if all inference candidates for a variable have `ReturnType` priority.
    /// This indicates the type was inferred from callback return types (Round 2),
    /// not from direct arguments (Round 1).
    pub fn all_candidates_are_return_type(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        !info.candidates.is_empty()
            && info
                .candidates
                .iter()
                .all(|c| c.priority == InferencePriority::ReturnType)
    }

    /// Get the original un-widened literal candidate types for an inference variable.
    pub fn get_literal_candidates(&mut self, var: InferenceVar) -> Vec<TypeId> {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.candidates
            .iter()
            .filter(|c| c.is_fresh_literal)
            .map(|c| c.type_id)
            .collect()
    }

    /// Check if all covariant candidates for a variable are fresh literals.
    /// When false, the resolved type should NOT be widened by `widen_literal_type`
    /// (matches tsc's `getWidenedLiteralType` which only widens fresh literals).
    pub fn all_candidates_are_fresh_literals(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        !info.candidates.is_empty() && info.candidates.iter().all(|c| c.is_fresh_literal)
    }

    /// Returns true when every candidate for `var` was inferred from an array
    /// element match (`T[]` vs `"a"[]`). Used to widen scalar fresh literals in
    /// `NoInfer<T>` positions, matching tsc's BCT widening of array literals.
    pub fn all_candidates_from_array_elements(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        !info.candidates.is_empty() && info.candidates.iter().all(|c| c.from_array_element)
    }

    /// Returns true when every candidate for `var` was inferred from an
    /// object-literal property match (`{ value: T }` vs `{ value: 1 }`). A
    /// literal inferred through an object-literal property is widened to its
    /// primitive in tsc's `getInferredType` regardless of whether the type
    /// parameter is also at the top level of the return type, so a sibling
    /// `NoInfer<T>` position must be checked against the widened type. Only a
    /// *direct* top-level argument (`value: T`) preserves the literal. Mirrors
    /// `all_candidates_from_array_elements` for the object-property case.
    pub fn all_candidates_from_object_property(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        !info.candidates.is_empty() && info.candidates.iter().all(|c| c.from_object_property)
    }

    /// Returns true when at least one fresh literal candidate came from array
    /// element inference. This is narrower than `all_candidates_from_array_elements`
    /// so mixed direct/callback inference can still recognize literal-array evidence.
    pub fn has_fresh_array_element_candidate(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.candidates
            .iter()
            .any(|c| c.from_array_element && c.is_fresh_literal)
    }

    /// Returns `true` if any covariant candidate came from a type assertion (`expr as T`).
    /// Asserted types are non-fresh and must not be widened.
    pub fn has_type_annotation_candidates(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.candidates.iter().any(|c| c.source_is_type_annotation)
    }

    /// Returns true when the winning covariant candidate type was produced
    /// while descending through a readonly array/tuple source.
    pub fn has_readonly_source_candidate_for(&mut self, var: InferenceVar, ty: TypeId) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.candidates
            .iter()
            .any(|candidate| candidate.type_id == ty && candidate.from_readonly_source)
    }

    pub fn set_resolved_type(&mut self, var: InferenceVar, ty: TypeId) {
        let root = self.table.find(var);
        let mut info = self.table.probe_value(root);
        info.resolved = Some(ty);
        self.table.union_value(root, info);
    }
}

/// Returns `true` when `ty` is or structurally contains an `IndexAccess` type.
fn type_contains_index_access(db: &dyn crate::construction::TypeDatabase, ty: TypeId) -> bool {
    if ty.is_intrinsic() {
        return false;
    }
    match db.lookup(ty) {
        Some(TypeData::IndexAccess(_, _)) => true,
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => db
            .type_list(list_id)
            .iter()
            .any(|&m| type_contains_index_access(db, m)),
        _ => false,
    }
}
