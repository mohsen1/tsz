//! Whether an object-literal element that failed the whole-collection relation
//! did so only through literal widening (suppress) or through a genuinely
//! missing required property (surface per element). Split out of
//! `elaboration_object_properties.rs` to keep that file under the 2000-line cap.

use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check if all properties of an object literal are assignable to the
    /// target type when using literal types from the initializers. This catches
    /// cases where the widened object type (e.g., `{ kind: string }`) fails
    /// assignability against a discriminated union, but the literal property
    /// values (e.g., `"bluray"`) actually match a union member.
    pub(super) fn all_object_literal_properties_assignable_with_literals(
        &mut self,
        obj_idx: NodeIndex,
        source_type: TypeId,
        target_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let obj_node = match self.ctx.arena.get(obj_idx) {
            Some(node) if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => node,
            _ => return false,
        };

        let obj = match self.ctx.arena.get_literal_expr(obj_node) {
            Some(obj) => obj.clone(),
            None => return false,
        };

        if obj.elements.nodes.is_empty() {
            return false;
        }

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            let (prop_name_idx, prop_value_idx) = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    match self.ctx.arena.get_property_assignment(elem_node) {
                        Some(prop) => (prop.name, prop.initializer),
                        None => continue,
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    match self.ctx.arena.get_shorthand_property(elem_node) {
                        Some(prop) => (prop.name, prop.name),
                        None => continue,
                    }
                }
                _ => continue,
            };

            let Some(prop_name) = self.object_literal_property_name_text(prop_name_idx) else {
                continue;
            };

            let Some((target_prop_type, _)) =
                self.object_literal_target_property_type(target_type, prop_name_idx, &prop_name)
            else {
                // Target doesn't have this property — can't confirm assignability
                return false;
            };

            if target_prop_type == TypeId::ERROR || target_prop_type == TypeId::ANY {
                continue;
            }

            // Try literal type first, then cached type
            let source_prop_type =
                if let Some(literal_type) = self.literal_type_from_initializer(prop_value_idx) {
                    literal_type
                } else {
                    self.get_type_of_node(prop_value_idx)
                };

            if source_prop_type == TypeId::ERROR || source_prop_type == TypeId::ANY {
                continue;
            }

            if !self
                .call_arg_relation_outcome(source_prop_type, target_prop_type)
                .related
            {
                return false;
            }
        }

        // Every property the literal *wrote* is assignable, so the failure is
        // not a per-property type mismatch. It can still be a genuinely *missing*
        // required target property — the loop above only walks the source's own
        // members and never notices one the target requires but the literal omits.
        // That is a real `TS2741`, not the widening-only false positive this
        // suppression exists for (a discriminated-union literal that relates once
        // its property literals are preserved), so it must surface per element
        // rather than being swallowed into the coarse whole-array/tuple relation.
        // Decline the suppression when the analyzed failure is a missing property;
        // the widening case reports a literal/type mismatch reason instead and
        // stays suppressed. `source_type` is the element's already-computed type
        // from the caller's loop, so this reuses it rather than re-deriving it.
        if matches!(
            self.analyze_assignability_failure(source_type, target_type)
                .failure_reason,
            Some(
                query_common::SubtypeFailureReason::MissingProperty { .. }
                    | query_common::SubtypeFailureReason::MissingProperties { .. }
            )
        ) {
            return false;
        }

        true
    }
}
