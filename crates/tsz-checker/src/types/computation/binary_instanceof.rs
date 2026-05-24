//! `instanceof` binary-operator checks.

use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn global_function_interface_type_for_instanceof(&mut self) -> Option<TypeId> {
        if !self.ctx.compiler_options.no_lib {
            return Some(TypeId::FUNCTION);
        }

        let function_sym_id = self.ctx.binder.lib_symbol_ids.iter().find_map(|&sym_id| {
            self.ctx.binder.get_symbol(sym_id).and_then(|symbol| {
                (symbol.escaped_name == "Function" && symbol.has_any_flags(symbol_flags::INTERFACE))
                    .then_some(sym_id)
            })
        });

        function_sym_id
            .map(|sym_id| self.get_type_of_symbol(sym_id))
            .or_else(|| {
                self.resolve_actual_lib_name_to_def_id_for_lowering("Function")
                    .map(|def_id| self.ctx.types.lazy(def_id))
            })
            .or_else(|| self.resolve_lib_type_by_name("Function"))
    }

    fn declared_instanceof_left_operand_type(
        &mut self,
        left_idx: NodeIndex,
        left_type: TypeId,
    ) -> TypeId {
        let evaluator = crate::query_boundaries::common::new_binary_op_evaluator(self.ctx.types);
        if evaluator.is_valid_instanceof_left_operand(left_type) {
            return left_type;
        }

        let Some(node) = self.ctx.arena.get(left_idx) else {
            return left_type;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return left_type;
        }

        let Some(sym_id) = self.resolve_identifier_symbol(left_idx) else {
            return left_type;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return left_type;
        };

        let mut decl_idx = symbol.value_declaration;
        let Some(mut decl_node) = self.ctx.arena.get(decl_idx) else {
            return left_type;
        };
        if decl_node.kind == SyntaxKind::Identifier as u16
            && let Some(ext) = self.ctx.arena.get_extended(decl_idx)
            && ext.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
        {
            decl_idx = ext.parent;
            decl_node = parent_node;
        }
        if !self.is_const_variable_declaration(decl_idx) {
            return left_type;
        }
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return left_type;
        };
        if var_decl.type_annotation.is_none() {
            return left_type;
        }
        if !self
            .ctx
            .binder
            .get_node_flow(left_idx)
            .and_then(|flow_id| self.ctx.binder.flow_nodes.get(flow_id))
            .is_some_and(|flow| flow.has_any_flags(tsz_binder::flow_flags::ASSIGNMENT))
        {
            return left_type;
        }

        let declared_type = self.get_type_of_symbol(sym_id);
        if evaluator.is_valid_instanceof_left_operand(declared_type) {
            declared_type
        } else {
            left_type
        }
    }

    /// Check the `instanceof` operator.
    ///
    /// Validates:
    /// - TS2848: RHS is not an instantiation expression
    /// - TS2358: LHS is of type any, an object type, or a type parameter
    /// - RHS is assignable to Function or has [Symbol.hasInstance]
    /// - TS2860/TS2861: Symbol.hasInstance param/return type checks
    pub(super) fn check_instanceof_operator(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        left_type: TypeId,
        right_type: TypeId,
    ) -> TypeId {
        use crate::diagnostics::diagnostic_codes;

        // TS2848: The right-hand side of an instanceof must not be an instantiation expression
        let unwrapped_right = self.ctx.arena.skip_parenthesized(right_idx);
        if let Some(right_node) = self.ctx.arena.get(unwrapped_right)
            && right_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
        {
            self.error_at_node(
                unwrapped_right,
                crate::diagnostics::diagnostic_messages::THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_NOT_BE_AN_INSTANTIATION_EXP,
                diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_NOT_BE_AN_INSTANTIATION_EXP,
            );
        }

        // Validate left operand
        if left_type != TypeId::ERROR {
            let evaluator =
                crate::query_boundaries::common::new_binary_op_evaluator(self.ctx.types);
            let lhs_type = self.declared_instanceof_left_operand_type(left_idx, left_type);
            if !evaluator.is_valid_instanceof_left_operand(lhs_type) {
                self.error_at_node_msg(
                    left_idx,
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYP,
                    &[],
                );
            }
        }

        let eval_right = self.evaluate_type_for_assignability(right_type);
        if eval_right != TypeId::ERROR {
            let mut is_valid_rhs = false;

            let func_ty_opt = self.global_function_interface_type_for_instanceof();

            if let Some(func_ty) = func_ty_opt {
                let evaluator =
                    crate::query_boundaries::common::new_binary_op_evaluator(self.ctx.types);
                is_valid_rhs = evaluator.is_valid_instanceof_right_operand(
                    eval_right,
                    func_ty,
                    &mut |src, tgt| self.is_assignable_to(src, tgt),
                );
            } else if self.ctx.compiler_options.no_lib {
                // Under `--noLib`, the global `Function` type is deliberately
                // absent. tsc suppresses TS2359 in that regime rather than
                // cascading on every `instanceof X`; mirror that.
                is_valid_rhs = true;
            } else if eval_right == TypeId::ANY
                || eval_right == TypeId::UNKNOWN
                || eval_right == TypeId::FUNCTION
            {
                is_valid_rhs = true;
            }

            if !is_valid_rhs
                && self.ctx.is_js_file()
                && self
                    .synthesize_js_constructor_instance_type(right_idx, eval_right, &[])
                    .is_some()
            {
                is_valid_rhs = true;
            }

            // Check for [Symbol.hasInstance] on the RHS type
            {
                use crate::query_boundaries::common::PropertyAccessResult;
                if let PropertyAccessResult::Success {
                    type_id: has_instance_type,
                    ..
                } = self.resolve_property_access_with_env(eval_right, "[Symbol.hasInstance]")
                {
                    is_valid_rhs = true;
                    let sig_info: Option<(Vec<tsz_solver::ParamInfo>, tsz_solver::TypeId)> =
                        if let Some(fn_id) = crate::query_boundaries::common::function_shape_id(
                            self.ctx.types,
                            has_instance_type,
                        ) {
                            let shape = self.ctx.types.function_shape(fn_id);
                            Some((shape.params.clone(), shape.return_type))
                        } else if let Some(shape_id) =
                            crate::query_boundaries::common::callable_shape_id(
                                self.ctx.types,
                                has_instance_type,
                            )
                        {
                            let shape = self.ctx.types.callable_shape(shape_id);
                            shape
                                .call_signatures
                                .first()
                                .map(|sig| (sig.params.clone(), sig.return_type))
                        } else {
                            None
                        };

                    if let Some((params, return_type)) = sig_info {
                        // TS2861: return type must be boolean
                        let ret = self.evaluate_type_for_assignability(return_type);
                        if ret != TypeId::BOOLEAN
                            && ret != TypeId::ANY
                            && ret != TypeId::ERROR
                            && !self.is_assignable_to(ret, TypeId::BOOLEAN)
                        {
                            self.error_at_node_msg(
                                right_idx,
                                diagnostic_codes::AN_OBJECTS_SYMBOL_HASINSTANCE_METHOD_MUST_RETURN_A_BOOLEAN_VALUE_FOR_IT_TO_BE_US,
                                &[],
                            );
                        }
                        // TS2860: LHS must be assignable to first parameter
                        if let Some(first_param) = params.first() {
                            let param_type =
                                self.evaluate_type_for_assignability(first_param.type_id);
                            let lhs_type =
                                self.declared_instanceof_left_operand_type(left_idx, left_type);
                            if lhs_type != TypeId::ANY
                                && lhs_type != TypeId::ERROR
                                && param_type != TypeId::ANY
                                && param_type != TypeId::UNKNOWN
                                && param_type != TypeId::ERROR
                                && !self.is_assignable_to(lhs_type, param_type)
                            {
                                self.error_at_node_msg(
                                    left_idx,
                                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_ASSIGNABLE_TO_THE_FIRST_A,
                                    &[],
                                );
                            }
                        }
                    }
                }
            }

            if !is_valid_rhs {
                self.error_at_node_msg(
                    right_idx,
                    diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_EITHER_OF_TYPE_ANY_A_CLA,
                    &[],
                );
            }
        }

        TypeId::BOOLEAN
    }
}
