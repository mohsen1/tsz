//! Primitive-to-boxed-wrapper and apparent-primitive-shape subtype relations,
//! extracted from `core_dispatch.rs`.
//!
//! This is a contiguous run of guards lifted verbatim out of
//! [`SubtypeChecker::check_subtype_inner_impl`] so the `core_dispatch.rs`
//! shard stays under the 2000-line file-size cap (§19). The dispatch body is
//! an *ordered* guard chain — each block either answers the relation or
//! declines and falls through to the next — so the extraction returns
//! `Option<SubtypeResult>`: `Some` is "this guard answered", `None` is
//! "declined, continue the chain". The caller invokes it at exactly the
//! original position, which is what makes the move behavior-preserving.
//!
//! `use super::*` re-exposes the parent module's imports and `SubtypeChecker`.

use super::*;

impl<R: TypeResolver> SubtypeChecker<'_, R> {
    /// Boxed-wrapper and apparent-primitive-shape relations for a primitive or
    /// literal `source`.
    ///
    /// Returns `Some` when this run of guards decides the relation, `None` when
    /// the dispatch chain should continue. Must run before the conditional-type
    /// guards, and the boxed-wrapper checks inside it must run before
    /// `apparent_primitive_shape_for_type` (a structural comparison against the
    /// apparent shape of `string` does not match `String`).
    pub(crate) fn primitive_boxed_relation(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeResult> {
        // Primitive-to-boxed-wrapper assignability: `string -> String`, `number -> Number`, etc.
        // Must run BEFORE apparent_primitive_shape_for_type which would do a structural
        // comparison that fails (the apparent shape of `string` doesn't structurally match `String`).
        if let Some(s_kind) = intrinsic_kind(self.interner, source)
            && let Some(kind) = boxable_intrinsic_kind(s_kind)
            && union_list_id(self.interner, target).is_none()
            && self.is_target_boxed_type(target, kind)
        {
            return Some(SubtypeResult::True);
        }

        // Also handle string/number/boolean literals -> boxed wrapper
        if let Some(lit) = literal_value(self.interner, source) {
            let kind = match lit {
                LiteralValue::String(_) => Some(IntrinsicKind::String),
                LiteralValue::Number(_) => Some(IntrinsicKind::Number),
                LiteralValue::Boolean(_) => Some(IntrinsicKind::Boolean),
                LiteralValue::BigInt(_) => Some(IntrinsicKind::Bigint),
            };
            if let Some(kind) = kind
                && union_list_id(self.interner, target).is_none()
                && self.is_target_boxed_type(target, kind)
            {
                return Some(SubtypeResult::True);
            }
        }

        if let Some(shape) = self.apparent_primitive_shape_for_type(source) {
            // A deferred mapped target may be structurally a pure index-signature
            // object — e.g. `{ [P in any]: V }` (the shape of `Record<any, V>` /
            // `Record<PropertyKey, V>`) is equivalent to
            // `{ [k: string]: V; [k: number]: V }`. Such a target is never
            // expanded eagerly during evaluation (to keep error-message display
            // stable), so `object_with_index_shape_id` reports `None` for it and
            // the relation would otherwise fall through to the boxed-wrapper
            // fallback below, wrongly accepting a primitive against a pure index
            // signature (a primitive has no index signature; tsc rejects it).
            // Expand it here so the pure-index guard in the
            // `object_with_index_shape_id` arm owns the decision, exactly as it
            // does for the written-out `{ [k: string]: V }` form.
            let target = self.expand_mapped_target_for_shape(target);
            if let Some(t_shape_id) = object_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                // Reset `in_intersection_member_check` for apparent primitive structural
                // comparison. When called from within a target intersection member loop,
                // the flag suppresses weak type checks. But the apparent-primitive
                // comparison is a fresh structural query — the String wrapper shape
                // should NOT bypass weak type detection when checked against weak types.
                let saved_inter_check = self.in_intersection_member_check;
                self.in_intersection_member_check = false;
                let result =
                    self.check_object_subtype(&shape, None, Some(source), &t_shape, Some(target));
                self.in_intersection_member_check = saved_inter_check;
                if result.is_true() {
                    return Some(result);
                }
                // Fallback: the hardcoded apparent shape may lack user-augmented members
                // (e.g., `interface Number extends ICloneable { }`), or missing iterable
                // interfaces (e.g., string <: Iterable<string>). Check the registered
                // boxed type which includes merged heritage from global augmentations.
                // Use apparent_primitive_kind to also handle literals (e.g., "test" <: Iterable<string>).
                if let Some(kind) = self.apparent_primitive_kind(source)
                    && union_list_id(self.interner, target).is_none()
                    && self.is_boxed_primitive_subtype(kind, target)
                {
                    return Some(SubtypeResult::True);
                }
                return Some(result);
            }
            if let Some(t_shape_id) = object_with_index_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                let source_kind = self.apparent_primitive_kind(source);
                let has_string_index = t_shape.string_index.is_some();
                let has_number_index = t_shape.number_index.is_some();
                let allow_indexed_structural = !has_string_index
                    && (!has_number_index || source_kind == Some(IntrinsicKind::String));
                if !allow_indexed_structural {
                    // Primitives must NOT be assignable to pure index-signature
                    // types (e.g., `string` to `{ [index: string]: any }`), even
                    // though their boxed wrappers would be structurally compatible.
                    // Only allow the boxed fallback when the target has named
                    // properties (a mixed interface, not a pure index type).
                    if !t_shape.properties.is_empty()
                        && let Some(s_kind) = source_kind
                        && self.is_boxed_primitive_subtype(s_kind, target)
                    {
                        return Some(SubtypeResult::True);
                    }
                    return Some(SubtypeResult::False);
                }
                let result = self.check_object_with_index_subtype(
                    &shape,
                    None,
                    Some(source),
                    &t_shape,
                    Some(target),
                );
                if result.is_true() {
                    return Some(result);
                }
                // Boxed fallback is safe here (no properties guard needed):
                // structural matching was already attempted above.
                if let Some(kind) = self.apparent_primitive_kind(source)
                    && self.is_boxed_primitive_subtype(kind, target)
                {
                    return Some(SubtypeResult::True);
                }
                return Some(result);
            }
            // Target is not a plain object/indexed-object (e.g., it's a generic
            // Application like `Iterable<string>`). The hardcoded apparent shape
            // can't match these. Fall back to the registered boxed type which
            // includes all heritage (e.g., String implements Iterable<string>).
            // Guard: skip for `object` type — primitives must NOT be subtypes of
            // `object` even though their boxed wrappers (Number, String, etc.) are.
            if target != TypeId::OBJECT
                && union_list_id(self.interner, target).is_none()
                && let Some(kind) = self.apparent_primitive_kind(source)
                && self.is_boxed_primitive_subtype(kind, target)
            {
                return Some(SubtypeResult::True);
            }
        }

        None
    }
}
