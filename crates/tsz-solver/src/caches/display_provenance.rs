//! Diagnostic display provenance capability for interned types.

use crate::intern::TypeInterner;
use crate::types::{PropertyInfo, TypeId};
use std::sync::Arc;

/// Diagnostic display and provenance hooks for interned types.
///
/// These methods preserve source-facing type identities and display-only object
/// facts after solver normalization. Keeping them separate from
/// [`crate::caches::db::TypeDatabase`] makes display provenance a visible,
/// narrower capability.
pub trait TypeDisplayProvenance {
    /// Store display-only properties for a fresh object literal.
    ///
    /// These are the pre-widened property types shown in error messages.
    /// The `shape_id` is the widened (interned) shape; `props` contains
    /// the original literal types from the source code.
    fn store_display_properties(&self, _type_id: TypeId, _props: Vec<PropertyInfo>) {}

    /// Retrieve display-only properties for a fresh object literal.
    ///
    /// Returns `None` if no display properties were stored.
    fn get_display_properties(&self, _type_id: TypeId) -> Option<Arc<Vec<PropertyInfo>>> {
        None
    }

    /// Store a reverse mapping from an evaluated `Application` result back to
    /// its original `Application` `TypeId` for diagnostic display.
    fn store_display_alias(&self, _evaluated: TypeId, _application: TypeId) {}

    /// Store an `Application` display alias even when structural provenance was
    /// recorded earlier for the same evaluated type.
    fn store_display_alias_preferring_application(&self, evaluated: TypeId, application: TypeId) {
        self.store_display_alias(evaluated, application);
    }

    /// Look up the original `Application` `TypeId` for a type produced by
    /// evaluating an `Application`. Returns `None` if no mapping exists.
    fn get_display_alias(&self, _type_id: TypeId) -> Option<TypeId> {
        None
    }

    /// Record that a merged-intersection object originated from `intersection`.
    /// See [`crate::intern::TypeInterner::store_merged_intersection_origin`].
    fn store_merged_intersection_origin(&self, _merged: TypeId, _intersection: TypeId) {}

    /// Look up the original `Intersection` a merged object was synthesized from.
    fn get_merged_intersection_origin(&self, _type_id: TypeId) -> Option<TypeId> {
        None
    }

    /// Record semantic provenance from an evaluated structural result back
    /// to the nominal `Application` it was produced from (relation-layer
    /// accept-only variance recovery; never read by the printer).
    fn record_application_eval_origin(&self, _evaluated: TypeId, _application: TypeId) {}

    /// Look up the semantic application origin of an evaluated structural
    /// result. Returns `None` when no origin was recorded.
    fn get_application_eval_origin(&self, _type_id: TypeId) -> Option<TypeId> {
        None
    }

    /// Mark an application base whose type-alias body is a conditional type.
    fn mark_conditional_alias_base(&self, _base: TypeId) {}

    /// Return whether an application base was marked as a conditional alias.
    fn is_conditional_alias_base(&self, _base: TypeId) -> bool {
        false
    }

    /// Mark `type_id` as the synthetic `typeof globalThis` surface object.
    fn mark_global_this_surface_display(&self, _type_id: TypeId) {}

    /// Return whether `type_id` is the synthetic `typeof globalThis` surface.
    fn is_global_this_surface_display(&self, _type_id: TypeId) -> bool {
        false
    }

    /// Mark `type_id` as a hand-written object-type literal annotation
    /// (`{ ... }`). See `TypeInterner::mark_literal_object_annotation`.
    fn mark_literal_object_annotation(&self, _type_id: TypeId) {}

    /// Return whether `type_id` was recorded as a hand-written object-type
    /// literal annotation.
    fn is_literal_object_annotation(&self, _type_id: TypeId) -> bool {
        false
    }

    /// Record the as-written origin members for a flattened `Union` `TypeId`.
    ///
    /// The checker calls this from `get_type_from_union_type` so that the
    /// printer can recover top-level alias names lost during flattening.
    /// See `TypeInterner::store_union_origin` for the full contract.
    fn store_union_origin(&self, _union_type_id: TypeId, _origin_members: Vec<TypeId>) {}

    /// Replace display-origin members for a union in a diagnostic-specific context.
    fn replace_union_origin_for_display(
        &self,
        _union_type_id: TypeId,
        _origin_members: Vec<TypeId>,
    ) {
    }

    /// Look up the as-written origin members for a flattened `Union` `TypeId`.
    fn get_union_origin(&self, _type_id: TypeId) -> Option<Arc<Vec<TypeId>>> {
        None
    }

    /// Atomically read and clear the "union too complex" flag.
    ///
    /// Returns `true` if a union construction was aborted due to complexity
    /// since the last call. The checker uses this to emit `TS2590`.
    fn take_union_too_complex(&self) -> bool {
        false
    }

    /// Peek at the "union too complex" flag without clearing it.
    ///
    /// Used by the evaluator to skip caching an evaluation that tripped the
    /// `TS2590` limit, so a cached read cannot suppress the diagnostic on
    /// re-evaluation. Default is `false`.
    fn is_union_too_complex(&self) -> bool {
        false
    }

    /// Mark the current operation as having produced a too-complex union.
    ///
    /// This mirrors `take_union_too_complex` for solver paths that discover the
    /// complexity limit during evaluation rather than initial construction.
    fn mark_union_too_complex(&self) {}
}

impl TypeDisplayProvenance for TypeInterner {
    fn store_display_properties(&self, type_id: TypeId, props: Vec<PropertyInfo>) {
        Self::store_display_properties(self, type_id, props);
    }

    fn get_display_properties(&self, type_id: TypeId) -> Option<Arc<Vec<PropertyInfo>>> {
        Self::get_display_properties(self, type_id)
    }

    fn store_display_alias(&self, evaluated: TypeId, application: TypeId) {
        Self::store_display_alias(self, evaluated, application);
    }

    fn store_display_alias_preferring_application(&self, evaluated: TypeId, application: TypeId) {
        Self::store_display_alias_preferring_application(self, evaluated, application);
    }

    fn get_display_alias(&self, type_id: TypeId) -> Option<TypeId> {
        Self::get_display_alias(self, type_id)
    }

    fn store_merged_intersection_origin(&self, merged: TypeId, intersection: TypeId) {
        Self::store_merged_intersection_origin(self, merged, intersection);
    }

    fn get_merged_intersection_origin(&self, type_id: TypeId) -> Option<TypeId> {
        Self::get_merged_intersection_origin(self, type_id)
    }

    fn record_application_eval_origin(&self, evaluated: TypeId, application: TypeId) {
        Self::record_application_eval_origin(self, evaluated, application);
    }

    fn get_application_eval_origin(&self, type_id: TypeId) -> Option<TypeId> {
        Self::get_application_eval_origin(self, type_id)
    }

    fn mark_conditional_alias_base(&self, base: TypeId) {
        Self::mark_conditional_alias_base(self, base);
    }

    fn is_conditional_alias_base(&self, base: TypeId) -> bool {
        Self::is_conditional_alias_base(self, base)
    }

    fn mark_global_this_surface_display(&self, type_id: TypeId) {
        Self::mark_global_this_surface_display(self, type_id);
    }

    fn is_global_this_surface_display(&self, type_id: TypeId) -> bool {
        Self::is_global_this_surface_display(self, type_id)
    }

    fn mark_literal_object_annotation(&self, type_id: TypeId) {
        Self::mark_literal_object_annotation(self, type_id);
    }

    fn is_literal_object_annotation(&self, type_id: TypeId) -> bool {
        Self::is_literal_object_annotation(self, type_id)
    }

    fn store_union_origin(&self, union_type_id: TypeId, origin_members: Vec<TypeId>) {
        Self::store_union_origin(self, union_type_id, origin_members);
    }

    fn replace_union_origin_for_display(&self, union_type_id: TypeId, origin_members: Vec<TypeId>) {
        Self::replace_union_origin_for_display(self, union_type_id, origin_members);
    }

    fn get_union_origin(&self, type_id: TypeId) -> Option<Arc<Vec<TypeId>>> {
        Self::get_union_origin(self, type_id)
    }

    fn take_union_too_complex(&self) -> bool {
        Self::take_union_too_complex(self)
    }

    fn is_union_too_complex(&self) -> bool {
        Self::is_union_too_complex(self)
    }

    fn mark_union_too_complex(&self) {
        self.set_union_too_complex();
    }
}
