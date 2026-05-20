//! Class-chain member lookup and visibility helpers.

use crate::class_checker::{ClassMemberInfo, MemberVisibility};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Find a member by name in a class, searching up the inheritance chain.
    /// Returns the member info if found, or None.
    /// Uses cycle detection to handle circular inheritance safely.
    pub(crate) fn find_member_in_class_chain(
        &mut self,
        class_idx: NodeIndex,
        target_name: &str,
        target_is_static: bool,
        _depth: usize,
        skip_private: bool,
    ) -> Option<ClassMemberInfo> {
        self.summarize_class_chain(class_idx)
            .member_info(target_name, target_is_static, skip_private)
            .cloned()
    }

    /// Internal implementation of `find_member_in_class_chain` with recursion guard.
    #[allow(dead_code)]
    fn find_member_in_class_chain_impl(
        &mut self,
        class_idx: NodeIndex,
        target_name: &str,
        target_is_static: bool,
        skip_private: bool,
        guard: &mut tsz_solver::recursion::RecursionGuard<NodeIndex>,
    ) -> Option<ClassMemberInfo> {
        use tsz_solver::recursion::RecursionResult;

        // Check for cycles using the recursion guard
        match guard.enter(class_idx) {
            RecursionResult::Cycle
            | RecursionResult::DepthExceeded
            | RecursionResult::IterationExceeded => {
                // Circular inheritance/depth/iteration limits detected - return None gracefully
                // Exceeded limits - bail out
                return None;
            }
            RecursionResult::Entered => {
                // Proceed with the search
            }
        }
        let result = (|| {
            let class_node = self.ctx.arena.get(class_idx)?;
            let class_data = self.ctx.arena.get_class(class_node)?;

            // Search direct members
            for &member_idx in &class_data.members.nodes {
                if let Some(info) = self.extract_class_member_info(member_idx, skip_private)
                    && info.name == target_name
                    && info.is_static == target_is_static
                {
                    return Some(info);
                }

                if !target_is_static
                    && let Some(member_node) = self.ctx.arena.get(member_idx)
                    && member_node.kind == tsz_parser::parser::syntax_kind_ext::CONSTRUCTOR
                    && let Some(ctor) = self.ctx.arena.get_constructor(member_node)
                {
                    for &param_idx in &ctor.parameters.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                            && self.has_parameter_property_modifier(&param.modifiers)
                            && let Some(name) = self.get_property_name(param.name)
                            && name == target_name
                        {
                            if skip_private && self.has_private_modifier(&param.modifiers) {
                                continue;
                            }
                            let visibility = if self.has_private_modifier(&param.modifiers) {
                                MemberVisibility::Private
                            } else if self.has_protected_modifier(&param.modifiers) {
                                MemberVisibility::Protected
                            } else {
                                MemberVisibility::Public
                            };
                            let prop_type = if param.type_annotation.is_some() {
                                self.get_type_from_type_node(param.type_annotation)
                            } else {
                                tsz_solver::TypeId::ANY
                            };
                            let info = ClassMemberInfo {
                                name,
                                type_id: prop_type,
                                name_idx: param.name,
                                visibility,
                                is_method: false,
                                is_static: false,
                                is_accessor: false,
                                is_abstract: false,
                                has_override: self.has_override_modifier(&param.modifiers)
                                    || self.has_jsdoc_override_tag(param_idx),
                                is_jsdoc_override: !self.has_override_modifier(&param.modifiers)
                                    && self.has_jsdoc_override_tag(param_idx),
                                has_dynamic_name: false,
                                has_computed_non_literal_name: false,
                                from_interface: false,
                            };
                            return Some(info);
                        }
                    }
                }
            }

            // Walk up to base class
            let heritage_clauses = class_data.heritage_clauses.as_ref()?;

            for &clause_idx in &heritage_clauses.nodes {
                let clause_node = self.ctx.arena.get(clause_idx)?;
                let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                let type_idx = *heritage.types.nodes.first()?;
                let type_node = self.ctx.arena.get(type_idx)?;
                let expr_idx =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };
                let expr_node = self.ctx.arena.get(expr_idx)?;
                let ident = self.ctx.arena.get_identifier(expr_node)?;
                let base_name = &ident.escaped_text;
                let sym_id = self.ctx.binder.file_locals.get(base_name)?;
                let symbol = self.ctx.binder.get_symbol(sym_id)?;
                let base_idx = symbol.primary_declaration()?;

                return self.find_member_in_class_chain_impl(
                    base_idx,
                    target_name,
                    target_is_static,
                    skip_private,
                    guard,
                );
            }

            None
        })();

        guard.leave(class_idx);
        result
    }

    pub(crate) const fn class_member_visibility_conflicts(
        &self,
        derived_visibility: MemberVisibility,
        base_visibility: MemberVisibility,
    ) -> bool {
        matches!(
            (derived_visibility, base_visibility),
            (
                MemberVisibility::Private,
                MemberVisibility::Private | MemberVisibility::Protected | MemberVisibility::Public
            ) | (
                MemberVisibility::Protected,
                MemberVisibility::Public | MemberVisibility::Private
            ) | (MemberVisibility::Public, MemberVisibility::Private)
        )
    }

    /// Count required (non-optional, non-rest, no-initializer) parameters in a
    /// method/function signature node, excluding `this` parameters.
    pub(crate) fn count_required_params_from_signature_node(&self, node_idx: NodeIndex) -> usize {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return 0;
        };
        let Some(sig) = self.ctx.arena.get_signature(node) else {
            return 0;
        };
        let Some(ref params) = sig.parameters else {
            return 0;
        };
        let mut count = 0;
        for &param_idx in &params.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            // Skip `this` pseudo-parameter
            if let Some(name_node) = self.ctx.arena.get(param.name)
                && name_node.kind == SyntaxKind::ThisKeyword as u16
            {
                continue;
            }
            // Rest parameters are not counted as required
            if param.dot_dot_dot_token {
                continue;
            }
            // Optional or has-default parameters are not required
            if param.question_token || param.initializer.is_some() {
                continue;
            }
            count += 1;
        }
        count
    }
}
