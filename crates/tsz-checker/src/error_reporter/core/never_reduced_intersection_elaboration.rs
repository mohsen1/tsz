//! TS18031 elaboration: naming the conflicting property when a
//! disjoint-object-literal intersection collapses to `never`.

use crate::diagnostics::{
    DiagnosticCategory, DiagnosticRelatedInformation, RelatedInformationKind, diagnostic_codes,
    diagnostic_messages, format_message,
};
use crate::error_reporter::fingerprint_policy::DiagnosticAnchorKind;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// `error_property_not_exist_at`'s final-fallback `error_at_anchor` call,
    /// with the TS18031 elaboration attached when it applies: when a
    /// `never`-typed property-access receiver came from a
    /// disjoint-object-literal intersection collapsing to `never` at intern
    /// time (e.g. `interface A { x: 1 } interface B { x: 2 };
    /// declare const c: A & B;`), tsc attaches `The intersection '...' was
    /// reduced to 'never' because property '...' has conflicting types in
    /// some constituents.` as a chain-linked related message. Every other
    /// `never` cause (primitive-disjoint intersections, explicit `never`,
    /// exhausted narrowing, ...) falls through to the plain diagnostic —
    /// oracle-verified against `typescript@7.0.2` that those have no such
    /// elaboration.
    pub(in crate::error_reporter) fn error_property_not_exist_with_never_elaboration(
        &mut self,
        idx: NodeIndex,
        type_id: TypeId,
        message: &str,
        code: u32,
    ) {
        let Some(anchor) = self.resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::PropertyToken)
        else {
            return;
        };
        let related = self.never_reduced_intersection_conflict_related_info(type_id, idx, &anchor);
        if related.is_empty() {
            self.error(anchor.start, anchor.length, message.to_string(), code);
        } else {
            self.error_at_span_with_related(anchor.start, anchor.length, message, code, related);
        }
    }

    fn never_reduced_intersection_conflict_related_info(
        &mut self,
        type_id: TypeId,
        idx: NodeIndex,
        anchor: &crate::error_reporter::fingerprint_policy::ResolvedDiagnosticAnchor,
    ) -> Vec<DiagnosticRelatedInformation> {
        if type_id != TypeId::NEVER {
            return Vec::new();
        }
        let Some(receiver_idx) = self.property_access_receiver_expression(idx) else {
            return Vec::new();
        };
        let Some((intersection_display, member_types)) =
            self.declared_intersection_display_and_members_for_expression(receiver_idx)
        else {
            return Vec::new();
        };
        let Some(atom) = tsz_solver::type_queries::find_disjoint_object_literal_conflict_property(
            self.ctx.types,
            &member_types,
        ) else {
            return Vec::new();
        };
        let prop_name = self.ctx.types.resolve_atom(atom);
        let message = format_message(
            diagnostic_messages::THE_INTERSECTION_WAS_REDUCED_TO_NEVER_BECAUSE_PROPERTY_HAS_CONFLICTING_TYPES_IN,
            &[&intersection_display, &prop_name],
        );
        vec![DiagnosticRelatedInformation {
            category: DiagnosticCategory::Error,
            code: diagnostic_codes::THE_INTERSECTION_WAS_REDUCED_TO_NEVER_BECAUSE_PROPERTY_HAS_CONFLICTING_TYPES_IN,
            file: self.ctx.file_name.clone(),
            start: anchor.start,
            length: anchor.length,
            message_text: message,
            depth: 0,
            kind: RelatedInformationKind::ChainLink,
        }]
    }

    /// The receiver expression of a property/element access, given either the
    /// access expression node itself or its name/argument node — mirrors
    /// `property_token_anchor_node`'s node-or-parent dispatch, since
    /// `error_property_not_exist_at` callers pass either one depending on call
    /// site.
    fn property_access_receiver_expression(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let is_access_expr_kind = |kind: u16| {
            kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        };

        if let Some(node) = self.ctx.arena.get(idx)
            && is_access_expr_kind(node.kind)
            && let Some(access) = self.ctx.arena.get_access_expr(node)
        {
            return Some(access.expression);
        }

        let parent_idx = self.ctx.arena.get_extended(idx)?.parent;
        let parent_node = self.ctx.arena.get(parent_idx)?;
        if is_access_expr_kind(parent_node.kind)
            && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
            && access.name_or_argument == idx
        {
            return Some(access.expression);
        }
        None
    }
}
