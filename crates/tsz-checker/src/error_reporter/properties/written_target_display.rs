//! Written-annotation-gated display for the TS2353 excess-property target
//! slot.
//!
//! tsc renders the type in `'x' does not exist in type 'T'` through the
//! target's `aliasSymbol`, which exists only when the written annotation (or
//! the written union arm the excess check narrowed to) is an alias
//! reference. An inline `{ ... }` annotation or arm has no `aliasSymbol` and
//! renders structurally — a coincidentally-shaped `type` alias elsewhere in
//! the file never repaints it, and a written reference keeps the name that
//! was written even when another alias with the identical body exists.
//!
//! tsz interns identically-shaped types to one `TypeId`, so by the time the
//! diagnostic is formatted the reverse type-to-def recovery
//! ([`CheckerState::lookup_type_alias_name_for_display`]) cannot tell the
//! written forms apart and picks an arbitrary same-shaped alias. These
//! helpers re-derive the display from the written annotation node at the
//! diagnostic site instead, mirroring the annotation-gated structural
//! display family used by the TS2322 head renderers.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Display for a TS2353 target that matches a written form at the
    /// diagnostic site: the whole annotation, an arm of an inline union
    /// annotation, or an arm of the union body of a local non-generic alias
    /// the annotation references.
    ///
    /// Returns `None` when no written form lowers to `target`, leaving the
    /// established display pipeline untouched.
    pub(super) fn excess_property_written_target_display(
        &mut self,
        target: TypeId,
        idx: NodeIndex,
    ) -> Option<String> {
        let (_, from_nested_container, annotation_node) =
            self.excess_property_target_annotation_for_site(idx)?;
        // A nested-container site reports against a property's type, not the
        // annotation itself; the established nested display path owns it.
        if from_nested_container {
            return None;
        }
        let annotation_idx = self.skip_parenthesized_type_nodes(annotation_node?);
        if let Some(display) = self.written_type_node_display_for_target(annotation_idx, target) {
            return Some(display);
        }
        // A union arm as written: either an inline union annotation, or the
        // union body of the local non-generic alias the annotation names.
        let union_idx = if self.type_node_is_union(annotation_idx) {
            annotation_idx
        } else {
            let body = self.local_non_generic_type_alias_body_for_reference(annotation_idx)?;
            let body = self.skip_parenthesized_type_nodes(body);
            if !self.type_node_is_union(body) {
                return None;
            }
            body
        };
        let arm_nodes: Vec<NodeIndex> = {
            let node = self.ctx.arena.get(union_idx)?;
            let composite = self.ctx.arena.get_composite_type(node)?;
            composite.types.nodes.to_vec()
        };
        arm_nodes.into_iter().find_map(|arm_idx| {
            let arm_idx = self.skip_parenthesized_type_nodes(arm_idx);
            self.written_type_node_display_for_target(arm_idx, target)
        })
    }

    /// If `type_node` lowers to `target`, render it the way tsc's
    /// `aliasSymbol` rule would: a written type-literal renders structurally,
    /// and a written non-generic reference keeps the name it was written
    /// with. Every other node kind returns `None` so the caller's established
    /// display path decides.
    fn written_type_node_display_for_target(
        &mut self,
        type_node: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        let node = self.ctx.arena.get(type_node)?;
        let is_type_literal = node.kind == syntax_kind_ext::TYPE_LITERAL;
        let is_bare_reference = node.kind == syntax_kind_ext::TYPE_REFERENCE
            && self
                .ctx
                .arena
                .get_type_ref(node)
                .is_some_and(|type_ref| type_ref.type_arguments.is_none());
        if !is_type_literal && !is_bare_reference {
            return None;
        }
        let lowered = self.get_type_from_type_node(type_node);
        if !self.written_type_matches_display_target(lowered, target) {
            return None;
        }
        if is_type_literal {
            let resolved = self.resolve_lazy_type(target);
            return Some(
                self.format_type_for_assignability_message_anonymous_composite_structural(resolved),
            );
        }
        // A bare reference to a GENERIC declaration is an implicit
        // instantiation (`Test` with a defaulted parameter renders as
        // `Test<any>`); the established application display owns it.
        if crate::query_boundaries::diagnostics::type_application(self.ctx.types, lowered).is_some()
        {
            return None;
        }
        if let Some(alias) = self.ctx.types.get_display_alias(lowered)
            && crate::query_boundaries::diagnostics::type_application(self.ctx.types, alias)
                .is_some()
        {
            return None;
        }
        // A written reference that still lowers to `Lazy(DefId)` prints the
        // referenced definition's own name without consulting the reverse
        // type-to-def lookup.
        if let Some(def_id) =
            crate::query_boundaries::diagnostics::lazy_def_id(self.ctx.types, lowered)
        {
            let def = self.ctx.definition_store.get(def_id)?;
            if !def.type_params.is_empty() {
                return None;
            }
            return Some(self.format_type_diagnostic_widened(lowered));
        }
        // Otherwise lowering resolved the reference; the written name is the
        // resolved symbol of the reference's identifier — tsc's `aliasSymbol`
        // for this type node.
        self.written_type_reference_symbol_name(type_node)
    }

    /// The name of the type symbol a written bare reference resolves to
    /// (`: P` → `P`), or `None` for qualified names, non-type resolutions,
    /// and generic declarations (whose bare reference is an implicit
    /// instantiation, not a plain name).
    fn written_type_reference_symbol_name(&self, type_node: NodeIndex) -> Option<String> {
        use crate::symbol_resolver::TypeSymbolResolution;
        let node = self.ctx.arena.get(type_node)?;
        let type_ref = self.ctx.arena.get_type_ref(node)?;
        let TypeSymbolResolution::Type(sym_id) =
            self.resolve_identifier_symbol_in_type_position_without_tracking(type_ref.type_name)
        else {
            return None;
        };
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(
            tsz_binder::symbol_flags::TYPE_ALIAS
                | tsz_binder::symbol_flags::INTERFACE
                | tsz_binder::symbol_flags::CLASS,
        ) {
            return None;
        }
        if self.symbol_declaration_has_type_parameters(sym_id) {
            return None;
        }
        Some(symbol.escaped_name.clone())
    }

    /// Written-form identity: the lowered annotation type and the diagnostic
    /// target denote the same type, compared through `Lazy` resolution so a
    /// written reference matches the resolved arm the excess check narrowed
    /// to.
    fn written_type_matches_display_target(&mut self, lowered: TypeId, target: TypeId) -> bool {
        if lowered == TypeId::ERROR || target == TypeId::ERROR {
            return false;
        }
        if lowered == target {
            return true;
        }
        let lowered_resolved = self.resolve_lazy_type(lowered);
        let target_resolved = self.resolve_lazy_type(target);
        lowered_resolved != TypeId::ERROR && lowered_resolved == target_resolved
    }

    fn type_node_is_union(&self, type_node: NodeIndex) -> bool {
        self.ctx
            .arena
            .get(type_node)
            .is_some_and(|node| node.kind == syntax_kind_ext::UNION_TYPE)
    }

    fn skip_parenthesized_type_nodes(&self, type_node: NodeIndex) -> NodeIndex {
        let mut current = type_node;
        for _ in 0..32 {
            let Some(node) = self.ctx.arena.get(current) else {
                return current;
            };
            if node.kind != syntax_kind_ext::PARENTHESIZED_TYPE {
                return current;
            }
            let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) else {
                return current;
            };
            current = wrapped.type_node;
        }
        current
    }
}
