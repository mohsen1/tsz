//! Function parameter prologue planning.
//!
//! ES2015+ can usually keep native parameter syntax, but class-expression
//! lowering may reserve function-scoped temps that must be declared in the
//! function body. This module detects those cases before printing.

use super::*;

impl<'a> LoweringPass<'a> {
    pub(super) fn function_parameters_need_body_prologue_transform(
        &self,
        params: &NodeList,
    ) -> bool {
        if self.ctx.target_es5 {
            return self.function_parameters_need_es5_transform(params);
        }

        params.nodes.iter().any(|&param_idx| {
            let Some(param_node) = self.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                return false;
            };

            (param.initializer.is_some()
                && self.parameter_expression_generates_function_temp(param.initializer))
                || self.parameter_binding_generates_function_temp(param.name)
        })
    }

    fn parameter_binding_generates_function_temp(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(node)
        {
            return self.parameter_expression_generates_function_temp(computed.expression);
        }

        if (node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
            && let Some(pattern) = self.arena.get_binding_pattern(node)
        {
            return pattern.elements.nodes.iter().copied().any(|elem_idx| {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    return false;
                };
                let Some(elem) = self.arena.get_binding_element(elem_node) else {
                    return false;
                };

                (elem.property_name.is_some()
                    && self.parameter_expression_generates_function_temp(elem.property_name))
                    || self.parameter_binding_generates_function_temp(elem.name)
                    || (elem.initializer.is_some()
                        && self.parameter_expression_generates_function_temp(elem.initializer))
            });
        }

        false
    }

    fn parameter_expression_generates_function_temp(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::CLASS_EXPRESSION
            && let Some(class) = self.arena.get_class(node)
        {
            return self.class_expression_parameter_needs_function_temp(class);
        }

        if let Some(computed) = self.arena.get_computed_property(node) {
            return self.parameter_expression_generates_function_temp(computed.expression);
        }

        if let Some(paren) = self.arena.get_parenthesized(node) {
            return self.parameter_expression_generates_function_temp(paren.expression);
        }

        if let Some(assertion) = self.arena.get_type_assertion(node) {
            return self.parameter_expression_generates_function_temp(assertion.expression);
        }

        if let Some(binary) = self.arena.get_binary_expr(node) {
            return self.parameter_expression_generates_function_temp(binary.left)
                || self.parameter_expression_generates_function_temp(binary.right);
        }

        if let Some(access) = self.arena.get_access_expr(node) {
            return self.parameter_expression_generates_function_temp(access.expression)
                || self.parameter_expression_generates_function_temp(access.name_or_argument);
        }

        if let Some(call) = self.arena.get_call_expr(node) {
            if self.parameter_expression_generates_function_temp(call.expression) {
                return true;
            }
            return call.arguments.as_ref().is_some_and(|args| {
                args.nodes
                    .iter()
                    .copied()
                    .any(|arg| self.parameter_expression_generates_function_temp(arg))
            });
        }

        if let Some(cond) = self.arena.get_conditional_expr(node) {
            return self.parameter_expression_generates_function_temp(cond.condition)
                || self.parameter_expression_generates_function_temp(cond.when_true)
                || self.parameter_expression_generates_function_temp(cond.when_false);
        }

        if let Some(unary) = self.arena.get_unary_expr(node) {
            return self.parameter_expression_generates_function_temp(unary.operand);
        }

        if let Some(unary) = self.arena.get_unary_expr_ex(node) {
            return self.parameter_expression_generates_function_temp(unary.expression);
        }

        if let Some(literal) = self.arena.get_literal_expr(node) {
            return literal
                .elements
                .nodes
                .iter()
                .copied()
                .any(|element| self.parameter_expression_generates_function_temp(element));
        }

        false
    }

    fn class_expression_parameter_needs_function_temp(
        &self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        let target_needs_field_lowering =
            self.ctx.needs_es2022_lowering || !self.ctx.options.use_define_for_class_fields;
        let target_needs_static_block_lowering = self.ctx.needs_es2022_lowering;
        let needs_private_lowering = self.ctx.needs_es2022_lowering;

        class.members.nodes.iter().copied().any(|member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };

            if target_needs_static_block_lowering
                && member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
            {
                return true;
            }

            if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && let Some(prop) = self.arena.get_property_decl(member_node)
            {
                if self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                    || self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                {
                    return false;
                }

                if needs_private_lowering && is_private_identifier(self.arena, prop.name) {
                    return true;
                }

                if target_needs_field_lowering {
                    let is_static = self.arena.is_static(&prop.modifiers);
                    let is_computed = self
                        .arena
                        .get(prop.name)
                        .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);
                    if is_static || is_computed {
                        return true;
                    }
                }
            }

            needs_private_lowering
                && (self
                    .arena
                    .get_method_decl(member_node)
                    .is_some_and(|method| is_private_identifier(self.arena, method.name))
                    || self
                        .arena
                        .get_accessor(member_node)
                        .is_some_and(|accessor| is_private_identifier(self.arena, accessor.name)))
        })
    }
}
