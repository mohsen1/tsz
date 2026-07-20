//! Whole-RHS compatibility validation for assignment-flow fallbacks.

use super::FlowAnalyzer;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> FlowAnalyzer<'a> {
    /// Validate the whole RHS before flow; literal initializers are handled earlier.
    pub(super) fn compatible_simple_assignment_type(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
        assigned_type: TypeId,
        compatibility_target_fallback: Option<TypeId>,
        assignment_type_is_provisional: &mut bool,
        preserve_declared_assignment_flow: &mut bool,
    ) -> Option<TypeId> {
        let assignment_started_provisional = *assignment_type_is_provisional;
        let Some(flow_type) = self.assigned_type_respecting_access_read_surface(
            assignment_node,
            target,
            assigned_type,
        ) else {
            *preserve_declared_assignment_flow = true;
            return None;
        };
        let node = self.arena.get(assignment_node)?;
        let option_sensitive_relation;
        let (value_declaration, compatibility_node) =
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                let bin = self.arena.get_binary_expr(node)?;
                if bin.operator_token != SyntaxKind::EqualsToken as u16
                    || !self.is_matching_reference(bin.left, target)
                {
                    return Some(flow_type);
                }
                option_sensitive_relation = assignment_started_provisional;
                let rhs = self.skip_parens_and_assertions(bin.right);
                if option_sensitive_relation
                    && !self
                        .arena
                        .get(rhs)
                        .is_some_and(|rhs_node| rhs_node.kind == syntax_kind_ext::CALL_EXPRESSION)
                {
                    // Reduced syntax-only surfaces keep the established flow path.
                    return Some(flow_type);
                }
                let declaration = self
                    .binder
                    .resolve_identifier(self.arena, bin.left)
                    .and_then(|sym| self.binder.get_symbol(sym))
                    .map(|sym| sym.value_declaration)
                    .filter(|decl| decl.is_some());
                (declaration, bin.left)
            } else if node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                && self.is_var_decl_with_type_annotation(assignment_node)
            {
                option_sensitive_relation = true;
                (Some(assignment_node), target)
            } else {
                return Some(flow_type);
            };
        let node_types = self.node_types;
        let declared_target_type = value_declaration
            .and_then(|declaration| {
                if option_sensitive_relation {
                    let (type_id, provisional) =
                        self.declared_type_from_value_declaration_with_stability(declaration);
                    *assignment_type_is_provisional |= provisional;
                    type_id
                } else {
                    self.annotation_type_from_var_decl_node(declaration)
                        .or_else(|| node_types.and_then(|types| types.get(&declaration.0).copied()))
                }
            })
            .or_else(|| node_types.and_then(|types| types.get(&compatibility_node.0).copied()))
            .or(compatibility_target_fallback.filter(|&type_id| type_id != TypeId::ERROR));
        if declared_target_type.is_none() && *assignment_type_is_provisional {
            *preserve_declared_assignment_flow = true;
            return None;
        }
        if let Some(lhs_type) = declared_target_type {
            let related = if option_sensitive_relation {
                let env = self.type_environment.map(std::cell::RefCell::borrow);
                let relation_flags = self.checker_context.map_or(
                    crate::query_boundaries::assignability::RelationFlags::STRICT_NULL_CHECKS,
                    |ctx| ctx.pack_relation_flags(),
                );
                crate::query_boundaries::flow_analysis::whole_assignment_rhs_is_compatible(
                    self.interner,
                    env.as_deref(),
                    self.concrete_this_type,
                    assigned_type,
                    lhs_type,
                    relation_flags,
                )
            } else {
                // Cached assignments retain their established relation path.
                self.assignment_relation_outcome(assigned_type, lhs_type, false)
                    .related
            };
            if !related {
                *preserve_declared_assignment_flow = true;
                return None;
            }
        }
        Some(flow_type)
    }
}
