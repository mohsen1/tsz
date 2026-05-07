//! Generic-call display normalization helpers.

use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn generic_direct_primitive_mismatch_display(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<(String, String)> {
        let arg_base = self.primitive_display_base(arg_type)?;
        let param_base = self.primitive_display_base(param_type)?;
        if arg_base == param_base {
            return None;
        }

        let parent_idx = self.ctx.arena.get_extended(arg_idx)?.parent;
        let parent = self.ctx.arena.get(parent_idx)?;
        let call = self.ctx.arena.get_call_expr(parent)?;
        if call.type_arguments.is_some() {
            return None;
        }
        let args = call.arguments.as_ref()?;
        let arg_index = args.nodes.iter().position(|&n| n == arg_idx)?;

        let raw_callee_type = self
            .resolve_qualified_symbol(call.expression)
            .or_else(|| self.resolve_identifier_symbol(call.expression))
            .map(|sym| self.get_type_of_symbol(sym))
            .unwrap_or_else(|| self.get_type_of_node(call.expression));
        let raw_sig = crate::query_boundaries::checkers::call::get_call_signature(
            self.ctx.types,
            raw_callee_type,
            args.nodes.len(),
        );

        let raw_param = raw_sig
            .as_ref()
            .and_then(|sig| Self::raw_param_for_call_arg(&sig.params, arg_index));
        if raw_param.is_some_and(|param| !param.rest) {
            return None;
        }

        let type_param_name = raw_param
            .and_then(|param| self.rest_generic_param_name_for_call_arg(param))
            .or_else(|| {
                self.ast_rest_generic_param_name_for_call_arg(call.expression, arg_index)
            })?;

        let previous_arg_with_same_param_base =
            args.nodes
                .iter()
                .take(arg_index)
                .enumerate()
                .any(|(prev_index, &prev_arg_idx)| {
                    let prev_type_param_name = raw_sig
                        .as_ref()
                        .and_then(|sig| Self::raw_param_for_call_arg(&sig.params, prev_index))
                        .filter(|param| param.rest)
                        .and_then(|param| self.rest_generic_param_name_for_call_arg(param))
                        .or_else(|| {
                            self.ast_rest_generic_param_name_for_call_arg(
                                call.expression,
                                prev_index,
                            )
                        });
                    if prev_type_param_name != Some(type_param_name) {
                        return false;
                    }
                    let prev_type = self
                        .literal_type_from_initializer(prev_arg_idx)
                        .unwrap_or_else(|| self.elaboration_source_expression_type(prev_arg_idx));
                    self.primitive_display_base(prev_type) == Some(param_base)
                });

        if !previous_arg_with_same_param_base {
            return None;
        }

        Some((
            self.format_type_for_assignability_message(arg_base),
            self.format_type_for_assignability_message(param_base),
        ))
    }

    fn primitive_display_base(&self, ty: TypeId) -> Option<TypeId> {
        let base = query_common::widen_literal_to_primitive(self.ctx.types, ty);
        match base {
            TypeId::STRING | TypeId::NUMBER | TypeId::BOOLEAN | TypeId::BIGINT | TypeId::SYMBOL => {
                Some(base)
            }
            _ => None,
        }
    }

    fn raw_param_for_call_arg(
        params: &[tsz_solver::ParamInfo],
        arg_index: usize,
    ) -> Option<&tsz_solver::ParamInfo> {
        params.get(arg_index).or_else(|| {
            let last = params.last()?;
            last.rest.then_some(last)
        })
    }

    fn rest_generic_param_name_for_call_arg(
        &mut self,
        raw_param: &tsz_solver::ParamInfo,
    ) -> Option<tsz_common::interner::Atom> {
        if !raw_param.rest {
            return None;
        }
        let raw_type = query_common::array_element_type(self.ctx.types, raw_param.type_id)
            .unwrap_or(raw_param.type_id);
        let info = query_common::type_param_info(self.ctx.types.as_type_database(), raw_type)?;
        Some(info.name)
    }

    fn ast_rest_generic_param_name_for_call_arg(
        &mut self,
        callee_expr: NodeIndex,
        arg_index: usize,
    ) -> Option<tsz_common::interner::Atom> {
        let callee_sym = self
            .resolve_identifier_symbol(callee_expr)
            .or_else(|| self.resolve_qualified_symbol(callee_expr))?;
        let declarations = self.ctx.binder.get_symbol(callee_sym)?.declarations.clone();

        for decl_idx in declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(func) = self.ctx.arena.get_function(node) else {
                continue;
            };
            let Some(type_params) = func.type_parameters.as_ref() else {
                continue;
            };

            let mut type_param_names = Vec::new();
            for &type_param_idx in &type_params.nodes {
                let Some(type_param) = self.ctx.arena.get_type_parameter_at(type_param_idx) else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(type_param.name) else {
                    continue;
                };
                let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                    continue;
                };
                type_param_names.push(ident.escaped_text.clone());
            }
            if type_param_names.is_empty() {
                continue;
            }

            let param_idx = func.parameters.nodes.get(arg_index).copied().or_else(|| {
                let &last_param_idx = func.parameters.nodes.last()?;
                let last_param_node = self.ctx.arena.get(last_param_idx)?;
                let last_param = self.ctx.arena.get_parameter(last_param_node)?;
                last_param.dot_dot_dot_token.then_some(last_param_idx)
            })?;
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            if !param.dot_dot_dot_token {
                continue;
            }
            let Some(annotation) = param.type_annotation.into_option() else {
                continue;
            };
            let Some(display) = self.sanitized_type_node_display(annotation) else {
                continue;
            };
            let display = display.trim();
            let candidate = display.strip_suffix("[]").or_else(|| {
                display
                    .strip_prefix("Array<")
                    .and_then(|inner| inner.strip_suffix('>'))
            })?;
            if type_param_names.iter().any(|name| name == candidate) {
                return Some(self.ctx.types.intern_string(candidate));
            }
        }

        None
    }
}
