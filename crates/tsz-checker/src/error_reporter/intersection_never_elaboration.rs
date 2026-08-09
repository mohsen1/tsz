//! `TS18031`/`TS18032` related-info elaboration for a `TS2339` property
//! access whose receiver is `never` because a *directly written*
//! intersection reduced to it. Split out of `properties.rs` to keep that
//! file under its size ratchet; the two are otherwise one unit of work.

use crate::diagnostics::diagnostic_codes;
use crate::error_reporter::fingerprint_policy::DiagnosticAnchorKind;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// `TS2339` ("Property does not exist on type") emitted with the
    /// `TS18031` related-info attached when applicable — the shared tail of
    /// `properties.rs`'s `error_property_not_exist_at`, kept here so that
    /// file's call site stays one line under its size ratchet.
    pub(super) fn error_property_does_not_exist_with_never_elaboration(
        &mut self,
        idx: NodeIndex,
        code: u32,
        message: &str,
        type_id: TypeId,
    ) {
        let related = self
            .intersection_reduced_to_never_related_info(type_id, idx)
            .into_iter()
            .collect();
        self.error_at_anchor_with_related(
            idx,
            DiagnosticAnchorKind::PropertyToken,
            message,
            code,
            related,
        );
    }

    /// The receiver (object) expression of a property/element access, from
    /// either the access expression node itself or its name/argument node —
    /// mirroring the two shapes [`crate::error_reporter::fingerprint_policy::DiagnosticAnchorKind::PropertyToken`]'s
    /// own anchor resolution (`property_token_anchor_node`) accepts for `idx`.
    pub(super) fn property_access_receiver_expr(&self, idx: NodeIndex) -> Option<NodeIndex> {
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
        let parent_idx = self.ctx.arena.get_extended(idx).map(|ext| ext.parent)?;
        let parent_node = self.ctx.arena.get(parent_idx)?;
        if is_access_expr_kind(parent_node.kind)
            && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
            && access.name_or_argument == idx
        {
            return Some(access.expression);
        }
        None
    }

    /// The `TS18031`/`TS18032` related-info line for a property access on a
    /// `never` receiver, when that `never` came from a *directly written*
    /// intersection whose members conflict either over a required literal
    /// property (`declare const c: A & B` where `A`/`B` disagree on it,
    /// TS18031) or over a private-brand-carrying property declared on two
    /// or more members (TS18032).
    ///
    /// `TypeInterner::intern` collapses such an intersection to the single
    /// canonical `TypeId::NEVER` at construction time, so by the time this
    /// `TS2339` is reported the member list is gone from `type_id` itself —
    /// this recovers it from the receiver's own declared-type syntax instead.
    /// Deliberately narrow (only the single-literal-per-member discriminant
    /// shape for TS18031, only a directly-written intersection annotation,
    /// not an alias/generic-application/heritage chain): every early return
    /// here just leaves the diagnostic as it is today, with no elaboration
    /// line, so under-covering is safe and never produces a wrong message.
    pub(super) fn intersection_reduced_to_never_related_info(
        &mut self,
        type_id: TypeId,
        idx: NodeIndex,
    ) -> Option<crate::diagnostics::DiagnosticRelatedInformation> {
        if type_id != TypeId::NEVER {
            return None;
        }
        let receiver_idx = self.property_access_receiver_expr(idx)?;
        // Recover the written intersection behind the `never` receiver, following
        // any type-alias references (`type C = A & B; declare const c: C`). The
        // recovered display is `None` for a directly-written intersection (render
        // the members structurally) or the naming alias otherwise (`C`,
        // `Pair<"y">`) — matching `tsc`'s `typeToString` of the reduced type.
        let (members, alias_display) =
            self.declared_never_intersection_for_expression(receiver_idx)?;
        // Each member is resolved through the same lazy-type machinery
        // property access itself uses (`Lazy(DefId)` interface/type-alias
        // references do not carry a structural shape the solver query below
        // can read until stabilized against the type environment; a generic
        // application member such as `WithKind<"a">` reached through an alias
        // must materialize here before its literal property is comparable).
        let members: Vec<TypeId> = members
            .into_iter()
            .map(|member| self.resolve_type_for_property_access(member))
            .collect();
        // Two independent reasons an intersection reduces to `never`: a
        // literal-discriminant conflict (TS18031) or a private-brand
        // conflict (TS18032). Try the literal check first — it matches
        // tsc's own message-selection precedent from the sibling TS18015
        // (ES-private) vs TS2442 (modifier-private "separate declarations")
        // split, where the more specific structural reason wins when both
        // could apply.
        let (code, conflict_atom) =
            if let Some(atom) = crate::query_boundaries::intersection_display::find_disjoint_literal_property_across_intersection(
                self.ctx.types,
                &members,
            ) {
                (
                    diagnostic_codes::THE_INTERSECTION_WAS_REDUCED_TO_NEVER_BECAUSE_PROPERTY_HAS_CONFLICTING_TYPES_IN,
                    atom,
                )
            } else {
                let atom = crate::query_boundaries::intersection_display::find_private_brand_conflict_property(
                    self.ctx.types,
                    &members,
                )?;
                (
                    diagnostic_codes::THE_INTERSECTION_WAS_REDUCED_TO_NEVER_BECAUSE_PROPERTY_EXISTS_IN_MULTIPLE_CONSTI,
                    atom,
                )
            };
        // tsc's elaboration names the discriminant that reduced the whole
        // intersection to `never`, not necessarily the property actually
        // accessed — once the receiver type itself is `never`, every access
        // on it carries the same reason.
        let conflict_prop_name = self.ctx.types.resolve_atom(conflict_atom);
        // A directly-written intersection is rendered structurally from the
        // recovered member types (rather than
        // `declared_intersection_annotation_display_for_expression`, which gates
        // its result on seeing a type-literal member — it exists to improve
        // *assignability*-message display, not to answer "was this written as an
        // intersection" — and so declines exactly the plain-interface-member
        // shape `A & B` this diagnostic needs most). When the intersection was
        // reached through a type alias, `tsc` names that alias instead of its
        // members, so the recovered alias display wins.
        let intersection_display = alias_display.unwrap_or_else(|| {
            members
                .iter()
                .map(|&member| self.format_type(member))
                .collect::<Vec<_>>()
                .join(" & ")
        });
        use crate::diagnostics::{Diagnostic, diagnostic_messages, format_message};
        let message_template = if code
            == diagnostic_codes::THE_INTERSECTION_WAS_REDUCED_TO_NEVER_BECAUSE_PROPERTY_HAS_CONFLICTING_TYPES_IN
        {
            diagnostic_messages::THE_INTERSECTION_WAS_REDUCED_TO_NEVER_BECAUSE_PROPERTY_HAS_CONFLICTING_TYPES_IN
        } else {
            diagnostic_messages::THE_INTERSECTION_WAS_REDUCED_TO_NEVER_BECAUSE_PROPERTY_EXISTS_IN_MULTIPLE_CONSTI
        };
        Some(Diagnostic::related_message(
            code,
            self.ctx.file_name.clone(),
            0,
            0,
            format_message(
                message_template,
                &[&intersection_display, &conflict_prop_name],
            ),
        ))
    }
}
