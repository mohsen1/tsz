//! The property-pair relation frame (#17687).
//!
//! Split out of `nested_application_property_mismatch.rs` to keep that file
//! under the source-size ceiling. See
//! [`CheckerState::push_property_relation_with_pair_frame`] for the rule.

use crate::diagnostics::{Diagnostic, diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Emit a property relation's drill beneath its `Types of property 'X' are
    /// incompatible.` header, reproducing `tsc`'s intermediate property-pair
    /// relation line.
    ///
    /// `tsc` always reports the failing property's own relation, which leads
    /// with `Type '<src>' is not assignable to type '<tgt>'.` before drilling
    /// into the sub-reason. When the property types are primitives that reduce
    /// to the leaf (`{ m: boolean }` vs `{ m: string }`) that line *is* the leaf
    /// and no separate frame appears; but when the property types are structural
    /// — an optional-widened union (`boolean | undefined` vs `string | undefined`)
    /// or an object measured against an index signature (`{ extra: boolean }` vs
    /// `{ [k: string]: string }`) — the reduced solver reason renders only the
    /// deeper leaf (`boolean` vs `string`, or `Property 'extra' is incompatible
    /// with index signature.`), and the property-pair line must be synthesized so
    /// the chain matches `tsc`. The line is inserted exactly when the property
    /// types differ (by rendered form) from the reduced sub-reason's displayed
    /// types, keeping the path-compressed `The types of 'a.b' …` form (handled by
    /// the caller for multi-link runs) untouched.
    pub(super) fn push_property_relation_with_pair_frame(
        &mut self,
        diag: &mut Diagnostic,
        idx: tsz_parser::parser::NodeIndex,
        span: (u32, u32),
        property_types: (TypeId, TypeId),
        nested: &tsz_solver::SubtypeFailureReason,
        nested_display_types: (TypeId, TypeId),
        nested_base_depth: u32,
    ) {
        let (start, length) = span;
        let (source_property_type, target_property_type) = property_types;
        let (nested_source, nested_target) = nested_display_types;
        // Fast path: when the property types are the very types the reduced
        // sub-reason displays, the pair line would duplicate the leaf, so drill
        // directly (the primitive-property case). Otherwise compare rendered
        // forms — distinct ids that render identically (an alias of the leaf) are
        // also no distinct frame; only a structurally different property relation
        // (optional-widened union, object-vs-index-signature) earns the frame.
        let identical =
            source_property_type == nested_source && target_property_type == nested_target;
        let pair_frame = (!identical).then(|| {
            let prop_src = self.format_type_diagnostic(source_property_type);
            let prop_tgt = self.format_type_diagnostic(target_property_type);
            let leaf_src = self.format_type_diagnostic(nested_source);
            let leaf_tgt = self.format_type_diagnostic(nested_target);
            (prop_src != leaf_src || prop_tgt != leaf_tgt).then(|| {
                format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&prop_src, &prop_tgt],
                )
            })
        });
        let nested_depth = if let Some(Some(pair)) = pair_frame {
            diag.push_elaboration_in_span(
                start,
                length,
                pair,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                nested_base_depth,
            );
            nested_base_depth + 1
        } else {
            nested_base_depth
        };
        let nested_diag =
            self.render_failure_reason(nested, nested_source, nested_target, idx, nested_depth);
        Self::push_nested_chain(diag, nested_diag, nested_depth);
    }
}
