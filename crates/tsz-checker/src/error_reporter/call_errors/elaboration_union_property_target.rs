//! Per-property elaboration target for an object literal against a union
//! target.
//!
//! `tsc`'s `elaborateElementwise` derives each property's target through
//! `getBestMatchIndexedAccessTypeOrUndefined`: the first step
//! (`getIndexedAccessTypeOrUndefined`) is defined on a union only when EVERY
//! constituent exposes the key, and its result is the union of the
//! constituents' property types. Only when some constituent lacks the key does
//! `tsc` fall back to the best-matching (discriminant-matched) member. The
//! derived target drives the property check, the leaf display, and the
//! nested-literal recursion alike — so a nested leaf reports against the
//! cross-arm union (`Type '2' is not assignable to type '1 | 9'.`), and a
//! property value that satisfies the cross-arm union produces no inner anchor
//! at all (the outer head with the folded property chain reports instead).

use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// The `(check, display)` per-property target pair derived from the FULL
    /// (pre-narrowing, pre-nullish-split) union target, mirroring `tsc`'s
    /// `getIndexedAccessTypeOrUndefined` step on a union.
    ///
    /// Returns `None` — the caller keeps its discriminant-narrowed derivation,
    /// which owns `tsc`'s best-matching-member fallback — when the target is
    /// not a multi-member union, when some constituent does not expose the
    /// key (a nullish arm of `A | B | undefined` never does, exactly like
    /// `tsc`'s undefined indexed access over that union), or when the
    /// union-level derivation itself declines.
    pub(in crate::error_reporter::call_errors) fn full_union_object_literal_property_target(
        &mut self,
        pre_narrow_target: TypeId,
        prop_name_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<(TypeId, TypeId)> {
        let resolved = self.resolve_type_for_property_access(pre_narrow_target);
        let members: Vec<TypeId> = query_common::union_members(self.ctx.types, resolved)
            .or_else(|| {
                let evaluated = self.evaluate_type_with_env(resolved);
                query_common::union_members(self.ctx.types, evaluated)
            })?
            .as_ref()
            .to_vec();
        if members.len() < 2 {
            return None;
        }
        // Every constituent must expose the key through the same derivation
        // the per-member elaboration uses; one member lacking it means the
        // indexed access over the union is undefined in `tsc`, and the
        // best-matching member owns the target instead.
        for member in members {
            self.object_literal_target_property_type(member, prop_name_idx, prop_name)?;
        }
        self.object_literal_target_property_type(pre_narrow_target, prop_name_idx, prop_name)
    }

    /// Per-property elaboration target when the union target was NOT
    /// discriminant-narrowed and has no array-like member — `tsc`'s
    /// `getBestMatchIndexedAccessTypeOrUndefined` in full.
    ///
    /// The indexed access over the whole union owns the target only when
    /// EVERY constituent exposes the key (the cross-arm union). Otherwise the
    /// access is undefined and `getBestMatchingType`'s final
    /// `findMostOverlappyType` step selects the single member sharing the
    /// most keys with the source (ties to the LAST member; primitive and
    /// nullish arms never score), and the property elaborates against that
    /// member alone — with every discriminator failing (`kind: "zz"`, `n: 2`
    /// against `{ kind: "a"; n: 1 } | { kind: "b"; n: 9 } | { kind: "c" }`),
    /// the `n` leaf reports `'9'`, not the key-bearing arms' `'1 | 9'`. A
    /// non-union target keeps the plain derivation. `None` — the selected
    /// member lacks the key, or no member is selected — skips the drill-in so
    /// the outer relation error reports.
    pub(in crate::error_reporter::call_errors) fn unnarrowed_union_object_literal_property_target(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
        prop_name_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<(TypeId, TypeId)> {
        let resolved = self.resolve_type_for_property_access(target_type);
        let union_with_members: Option<(TypeId, Vec<TypeId>)> =
            query_common::union_members(self.ctx.types, resolved)
                .map(|list| (resolved, list.as_ref().to_vec()))
                .or_else(|| {
                    let evaluated = self.evaluate_type_with_env(resolved);
                    query_common::union_members(self.ctx.types, evaluated)
                        .map(|list| (evaluated, list.as_ref().to_vec()))
                });
        let Some((union_type_id, members)) = union_with_members.filter(|(_, list)| list.len() >= 2)
        else {
            return self.object_literal_target_property_type(target_type, prop_name_idx, prop_name);
        };
        if let Some(pair) =
            self.full_union_object_literal_property_target(target_type, prop_name_idx, prop_name)
        {
            return Some(pair);
        }
        // `findMostOverlappyType` ties to the LAST scanned member, so the
        // scan order must be declaration order the way `tsc`'s own `types`
        // array is, not the interner's canonical (identity-sort) order: a
        // generic union alias re-interns a substituted arm at instantiation
        // time, which can sort it after non-generic sibling arms in the
        // canonical list even though it was declared first. The as-written
        // origin (recorded by `instantiate`'s `TypeData::Union` arm the same
        // way the printer already prefers it, see `format/key.rs`) is the
        // declaration-ordered list when one was stored; otherwise the
        // canonical order already matches declaration order.
        let members = self
            .ctx
            .types
            .get_union_origin(union_type_id)
            .map(|origin| origin.as_ref().clone())
            .unwrap_or(members);
        let member = crate::query_boundaries::assignability::union_target_best_elaboration_member(
            self.ctx.types,
            &self.ctx,
            source_type,
            &members,
        )?;
        self.object_literal_target_property_type(member, prop_name_idx, prop_name)
    }

    /// tsc's `findBestTypeForObjectLiteral` (the object-literal branch of
    /// `getBestMatchingType`, used by `getBestMatchIndexedAccessTypeOrUndefined`):
    /// when a fresh object-literal source is related to a union target that has
    /// an array-like member, the best-matching member for per-property
    /// elaboration is the first non-array-like member in union order. Returns
    /// that member, or `None` when the target does not resolve to such a union
    /// (no array-like member, or not a union at all).
    ///
    /// This is a deliberate narrowing of tsc's full `getBestMatchingType`
    /// (which also scores members by discriminant/property overlap): the
    /// array-like branch is the one that governs the recursive-JSON-alias shape
    /// this gate targets (`… | Json[] | { [k: string]: Json }`), where the
    /// leading primitive/object member is selected. Array-like = `T[]`, tuple,
    /// or `readonly T[]` (tsc's `isArrayLikeType`).
    pub(crate) fn object_literal_array_union_best_match_member(
        &mut self,
        param_type: TypeId,
    ) -> Option<TypeId> {
        use crate::query_boundaries::type_checking_utilities::{
            ArrayLikeKind, classify_array_like,
        };

        let resolved = self.resolve_type_for_property_access(param_type);
        let evaluated = self.judge_evaluate(resolved);
        for candidate in [param_type, resolved, evaluated] {
            let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, candidate)
            else {
                continue;
            };
            let db = self.ctx.types.as_type_database();
            let is_array_like = |member: TypeId| {
                matches!(
                    classify_array_like(db, member),
                    ArrayLikeKind::Array(_) | ArrayLikeKind::Tuple | ArrayLikeKind::Readonly(_)
                )
            };
            if !members.iter().any(|&member| is_array_like(member)) {
                continue;
            }
            return members
                .iter()
                .copied()
                .find(|&member| !is_array_like(member));
        }
        None
    }

    pub(in crate::error_reporter::call_errors) fn target_has_never_indexed_access_surface(
        &self,
        target_type: TypeId,
    ) -> bool {
        crate::query_boundaries::diagnostics::contains_never_index_access_surface(
            self.ctx.types.as_type_database(),
            &self.ctx.definition_store,
            target_type,
            8,
        )
    }

    pub(in crate::error_reporter::call_errors) fn target_has_indexed_access_surface(
        &self,
        target_type: TypeId,
    ) -> bool {
        self.type_has_indexed_access_surface(target_type, 0)
    }

    /// `true` when any shape reachable from `target_type` has a named property
    /// (not an index signature) whose atom equals `prop_name`.
    pub(in crate::error_reporter::call_errors) fn target_has_named_property_for_key(
        &mut self,
        target_type: TypeId,
        prop_name: &str,
    ) -> bool {
        let prop_atom = self.ctx.types.intern_string(prop_name);
        let resolved = self.resolve_type_for_property_access(target_type);
        let evaluated = self.evaluate_type_with_env(target_type);
        let has_named = |type_id: TypeId| {
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
                .is_some_and(|shape| shape.properties.iter().any(|p| p.name == prop_atom))
        };
        [target_type, resolved, evaluated]
            .into_iter()
            .any(|candidate| {
                crate::query_boundaries::common::union_members(self.ctx.types, candidate)
                    .map_or_else(
                        || has_named(candidate),
                        |ms| ms.iter().copied().any(has_named),
                    )
            })
    }

    /// `true` when `target_type` is a literal type, or a union any member of
    /// which is, so a fresh literal written under it keeps its literal type
    /// instead of widening. This is the contextual half of ordinary
    /// object-literal freshness, asked of the computed member's *target* type.
    pub(in crate::error_reporter::call_errors) fn computed_member_target_is_literal_bearing(
        &mut self,
        target_type: TypeId,
    ) -> bool {
        let evaluated = self.evaluate_type_with_env(target_type);
        let is_literal = |type_id: TypeId| {
            crate::query_boundaries::common::is_literal_type(self.ctx.types, type_id)
        };
        [target_type, evaluated].into_iter().any(|candidate| {
            crate::query_boundaries::common::union_members(self.ctx.types, candidate).map_or_else(
                || is_literal(candidate),
                |ms| ms.iter().copied().any(is_literal),
            )
        })
    }

    fn type_has_indexed_access_surface(&self, target_type: TypeId, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        let db = self.ctx.types.as_type_database();
        if crate::query_boundaries::common::index_access_types(db, target_type).is_some() {
            return true;
        }
        if let Some(members) = crate::query_boundaries::common::union_members(db, target_type)
            && members
                .iter()
                .any(|&member| self.type_has_indexed_access_surface(member, depth + 1))
        {
            return true;
        }
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(db, target_type)
            && members
                .iter()
                .any(|&member| self.type_has_indexed_access_surface(member, depth + 1))
        {
            return true;
        }
        if crate::query_boundaries::common::is_generic_application(self.ctx.types, target_type)
            && let Some(def_id) = crate::query_boundaries::common::get_application_lazy_def_id(
                self.ctx.types,
                target_type,
            )
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && def.kind == tsz_solver::def::DefKind::TypeAlias
            && let Some(body) = def.body
        {
            return self.type_has_indexed_access_surface(body, depth + 1);
        }

        false
    }
}
