//! Return-surface helpers for source call declaration inference.

use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn call_expression_declared_return_surface_text(
        &self,
        expr_idx: NodeIndex,
        source_arena: &NodeArena,
        type_annotation: NodeIndex,
        type_text: &str,
        explicit_type_args: &[String],
        has_call_site_type_param_substitutions: bool,
    ) -> Option<String> {
        if Self::leading_type_reference_name(type_text)
            .is_some_and(Self::is_builtin_conditional_utility_type_name)
            && let Some(type_id) = self.get_node_type_or_names(&[expr_idx])
        {
            return Some(self.print_type_id_expanded_for_inferred_declaration(type_id));
        }
        if explicit_type_args.is_empty()
            && !has_call_site_type_param_substitutions
            && std::ptr::eq(source_arena, self.arena)
            && let Some(type_id) = self.get_node_type_or_names(&[type_annotation])
            && let Some(surface) = self.inferred_declaration_mapped_constraint_surface(type_id)
        {
            return Some(self.print_type_id_for_inferred_declaration(surface));
        }
        None
    }
}
