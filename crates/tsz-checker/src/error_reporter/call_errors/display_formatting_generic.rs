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
        )
        .or_else(|| {
            // `get_call_signature` only inspects `TypeData::Callable` shapes;
            // free function declarations produce `TypeData::Function` and are
            // handled here via the dedicated `function_shape_for_type` query.
            let shape = query_common::function_shape_for_type(self.ctx.types, raw_callee_type)?;
            Some((*shape).clone())
        });

        let raw_param = raw_sig
            .as_ref()
            .and_then(|sig| Self::raw_param_for_call_arg(&sig.params, arg_index));

        // Resolve the underlying type parameter that this argument is being
        // checked against. These cases produce a primitive-base widened display:
        //
        // 1. Rest parameter whose element is a generic — the legacy
        //    `f<T>(...args: T[])` path. Gated on a sibling argument having
        //    already established the parameter base (otherwise the first
        //    mismatching argument has no comparison anchor).
        // 2. Non-rest parameter whose declared type is itself a bare type
        //    parameter AND whose constraint chain bottoms out in a primitive
        //    type matching `param_base`. This mirrors tsc's behaviour: when
        //    the type parameter's effective constraint is a primitive
        //    (e.g. `<U extends number>` or `<U extends T>` with `T` already
        //    fixed to `number`), the argument-not-assignable diagnostic
        //    renders both source and target as primitives ('string' /
        //    'number') instead of preserving the inference candidate's
        //    literal display.
        // 3. Bare implementation-signature type parameters that are not part
        //    of the annotated return surface widen after a previous argument
        //    fixes the primitive base. If the return annotation exposes the
        //    type parameter, tsc preserves literal candidates.
        let (type_param_name, requires_prev_arg_match) = if raw_param
            .is_some_and(|param| param.rest)
        {
            let name = raw_param
                .and_then(|param| self.rest_generic_param_name_for_call_arg(param))
                .or_else(|| {
                    self.ast_rest_generic_param_name_for_call_arg(call.expression, arg_index)
                })?;
            (name, true)
        } else if let Some(param) = raw_param {
            let info =
                query_common::type_param_info(self.ctx.types.as_type_database(), param.type_id)?;
            // Widen when the type parameter's declared-constraint chain
            // bottoms out at a primitive whose base matches `param_base`.
            // The `<T, U extends T>` conformance case also widens when the
            // return annotation exposes `U`; the existing `: void` case keeps
            // literal candidates, so do not treat every unconstrained terminal
            // type parameter as a widening trigger.
            let constraint_matches_param_base = self
                .resolve_type_parameter_primitive_constraint_base(info)
                .is_some_and(|base| base == param_base)
                || (self.declared_constraint_chain_ends_at_unconstrained_type_param(info)
                    && self.ast_function_return_mentions_type_param(call.expression, info.name));
            if !constraint_matches_param_base {
                let implemented_signature_param = self
                    .ast_generic_implementation_param_name_for_call_arg(call.expression, arg_index)
                    == Some(info.name);
                if !implemented_signature_param {
                    return None;
                }
                (info.name, true)
            } else {
                (info.name, false)
            }
        } else {
            let name = self.ast_rest_generic_param_name_for_call_arg(call.expression, arg_index)?;
            (name, true)
        };

        if requires_prev_arg_match {
            let previous_arg_with_same_param_base = args
                .nodes
                .iter()
                .take(arg_index)
                .enumerate()
                .any(|(prev_index, &prev_arg_idx)| {
                    let prev_type_param_name = raw_sig
                        .as_ref()
                        .and_then(|sig| Self::raw_param_for_call_arg(&sig.params, prev_index))
                        .and_then(|param| self.raw_generic_param_name_for_call_arg(param))
                        .or_else(|| {
                            self.ast_rest_generic_param_name_for_call_arg(
                                call.expression,
                                prev_index,
                            )
                        })
                        .or_else(|| {
                            self.ast_generic_implementation_param_name_for_call_arg(
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

    /// Follow a type parameter's constraint chain looking for a primitive
    /// terminator. Returns the primitive `TypeId` (`STRING` / `NUMBER` / ...)
    /// when the constraint resolves to one within a small budget, or `None`
    /// when there is no constraint, the chain is non-primitive, or recursion
    /// guard trips. The bounded depth keeps cyclic / self-referential
    /// constraints (`T extends T`) from looping.
    fn resolve_type_parameter_primitive_constraint_base(
        &self,
        info: tsz_solver::TypeParamInfo,
    ) -> Option<TypeId> {
        const MAX_DEPTH: usize = 8;
        let db = self.ctx.types.as_type_database();
        let mut current = info.constraint?;
        for _ in 0..MAX_DEPTH {
            if let Some(base) = self.primitive_display_base(current) {
                return Some(base);
            }
            let inner = query_common::type_param_info(db, current)?;
            current = inner.constraint?;
        }
        None
    }

    /// Returns true when the declared-constraint chain of a type parameter
    /// (e.g. U in `<T, U extends T>`) bottoms out at *another* type
    /// parameter that itself has no declared constraint. The inference
    /// algorithm then widens fresh-literal candidates for both U and its
    /// terminal constraint (T), and the failing constraint check uses
    /// those widened values for the TS2345 diagnostic. Type parameters
    /// with no declared constraint at all (`<T>(a: T)`) and chains that
    /// reach a non-type-parameter non-primitive (e.g. `<U extends T[]>`)
    /// must continue to preserve their literal candidate display.
    fn declared_constraint_chain_ends_at_unconstrained_type_param(
        &self,
        info: tsz_solver::TypeParamInfo,
    ) -> bool {
        const MAX_DEPTH: usize = 8;
        let db = self.ctx.types.as_type_database();
        let Some(mut current) = info.constraint else {
            return false;
        };
        for _ in 0..MAX_DEPTH {
            if self.primitive_display_base(current).is_some() {
                return false;
            }
            let Some(inner) = query_common::type_param_info(db, current) else {
                return false;
            };
            match inner.constraint {
                Some(next) => current = next,
                None => return true,
            }
        }
        false
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

    fn raw_generic_param_name_for_call_arg(
        &self,
        raw_param: &tsz_solver::ParamInfo,
    ) -> Option<tsz_common::interner::Atom> {
        let raw_type = if raw_param.rest {
            query_common::array_element_type(self.ctx.types, raw_param.type_id)
                .unwrap_or(raw_param.type_id)
        } else {
            raw_param.type_id
        };
        query_common::type_param_info(self.ctx.types.as_type_database(), raw_type)
            .map(|info| info.name)
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

    fn ast_generic_implementation_param_name_for_call_arg(
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
            if self
                .ctx
                .arena
                .get(func.body)
                .is_none_or(|body| body.kind != tsz_parser::parser::syntax_kind_ext::BLOCK)
            {
                continue;
            }
            let Some(type_params) = func.type_parameters.as_ref() else {
                continue;
            };
            let Some(param_idx) = func.parameters.nodes.get(arg_index).copied() else {
                continue;
            };
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            if param.dot_dot_dot_token {
                continue;
            }
            let Some(annotation) = param.type_annotation.into_option() else {
                continue;
            };
            let Some(display) = self.sanitized_type_node_display(annotation) else {
                continue;
            };
            let display = display.trim();
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
                if display == ident.escaped_text {
                    if let Some(return_annotation) = func.type_annotation.into_option()
                        && let Some(return_display) =
                            self.sanitized_type_node_display(return_annotation)
                        && return_display.contains(ident.escaped_text.as_str())
                    {
                        continue;
                    }
                    return Some(self.ctx.types.intern_string(display));
                }
            }
        }

        None
    }

    fn ast_function_return_mentions_type_param(
        &mut self,
        callee_expr: NodeIndex,
        type_param_name: tsz_common::interner::Atom,
    ) -> bool {
        let type_param_name = self.ctx.types.resolve_atom_ref(type_param_name).to_string();
        let Some(callee_sym) = self
            .resolve_identifier_symbol(callee_expr)
            .or_else(|| self.resolve_qualified_symbol(callee_expr))
        else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(callee_sym) else {
            return false;
        };
        let declarations = symbol.declarations.clone();

        for decl_idx in declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(func) = self.ctx.arena.get_function(node) else {
                continue;
            };
            let Some(return_annotation) = func.type_annotation.into_option() else {
                continue;
            };
            if self
                .sanitized_type_node_display(return_annotation)
                .is_some_and(|display| display.contains(type_param_name.as_str()))
            {
                return true;
            }
        }

        false
    }
}
