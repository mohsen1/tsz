//! Display skeleton, constructor-propagation, and IIFE helpers for call expression resolution.
//!
//! Extracted from `call.rs` to keep that file under the 2000 LOC limit.
//! Contains:
//! - `refreshed_generic_call_arg_type_with_context` — re-evaluate context-sensitive args
//! - `setup_iife_contextual_type` — IIFE contextual type wrapping
//! - `type_display_skeleton` / `call_signature_display_skeleton` — structural fingerprint helpers
//! - `propagate_generic_constructor_display_defs` — DefId propagation after generic inference
//! - `object_literal_has_computed_property_names` — computed property name detection

use crate::call_checker::CallableContext;
use crate::context::TypingRequest;
use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::common;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{FunctionShape, TypeId};

impl<'a> CheckerState<'a> {
    fn generic_callable_refresh_context_is_useful(&mut self, type_id: TypeId) -> bool {
        if type_id == TypeId::UNKNOWN
            || type_id == TypeId::ERROR
            || common::contains_infer_types(self.ctx.types, type_id)
        {
            return false;
        }

        call_checker::get_contextual_signature(self.ctx.types, type_id)
            .or_else(|| {
                let evaluated = self.evaluate_type_with_env(type_id);
                call_checker::get_contextual_signature(self.ctx.types, evaluated)
            })
            .is_some()
    }

    pub(crate) fn is_assignable_via_contextual_signatures(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let source = {
            let source_shape =
                call_checker::get_contextual_signature(self.ctx.types, source).or_else(|| {
                    let evaluated = self.evaluate_type_with_env(source);
                    call_checker::get_contextual_signature(self.ctx.types, evaluated)
                });
            if source_shape.as_ref().is_some_and(|shape| !shape.type_params.is_empty()) {
                if self.target_has_concrete_return_context_for_generic_refinement(target) {
                    self.instantiate_generic_function_argument_against_target_for_refinement(
                        source, target,
                    )
                } else {
                    self.instantiate_generic_function_argument_against_target_params(source, target)
                }
            } else {
                source
            }
        };
        let normalize = |shape: FunctionShape| {
            let mut normalized = shape.clone();
            normalized.params = shape
                .params
                .iter()
                .flat_map(|param| common::unpack_tuple_rest_parameter(self.ctx.types, param))
                .collect();
            normalized
        };
        let source_shape =
            call_checker::get_contextual_signature(self.ctx.types, source).or_else(|| {
                let evaluated = self.evaluate_type_with_env(source);
                call_checker::get_contextual_signature(self.ctx.types, evaluated)
            });
        let target_shape =
            call_checker::get_contextual_signature(self.ctx.types, target).or_else(|| {
                let evaluated = self.evaluate_type_with_env(target);
                call_checker::get_contextual_signature(self.ctx.types, evaluated)
            });
        let (Some(source_shape), Some(target_shape)) = (source_shape, target_shape) else {
            return false;
        };

        let source_fn = self.ctx.types.factory().function(normalize(source_shape));
        let target_fn = self.ctx.types.factory().function(normalize(target_shape));
        self.is_assignable_to_with_env(source_fn, target_fn)
    }

    fn generic_arg_refresh_context_is_concrete(&self, type_id: TypeId) -> bool {
        type_id != TypeId::UNKNOWN
            && type_id != TypeId::ERROR
            && !common::contains_infer_types(self.ctx.types, type_id)
            && !common::contains_type_parameters(self.ctx.types, type_id)
    }

    fn generic_object_literal_refresh_context_is_useful(&mut self, type_id: TypeId) -> bool {
        if !self.generic_arg_refresh_context_is_concrete(type_id) {
            return false;
        }

        if self
            .contextual_callable_property_fallback_type(type_id, None)
            .is_some_and(|candidate| !matches!(candidate, TypeId::ANY | TypeId::UNKNOWN))
        {
            return true;
        }

        let resolved = self.evaluate_contextual_type(type_id);
        let resolved = self.resolve_type_for_property_access(resolved);
        let resolved = self.resolve_lazy_type(resolved);
        let resolved = self.evaluate_application_type(resolved);

        common::object_shape_for_type(self.ctx.types, resolved).is_some_and(|shape| {
            !shape.properties.is_empty()
                || shape.string_index.is_some()
                || shape.number_index.is_some()
        })
    }

    /// For IIFEs (immediately invoked function expressions), wrap the call expression's
    /// contextual type into a callable type so the function expression resolver can extract
    /// the return type (and for generators, the yield type).
    ///
    /// Returns `Some((wrapper_fn, original_ctx_type))` if wrapping is needed, or `None`
    /// if the callee is not a function expression. The caller is responsible for installing
    /// the wrapper via the `TypingRequest` API and restoring the original after resolution.
    pub(crate) fn setup_iife_contextual_type(
        &mut self,
        callee_expression: NodeIndex,
        contextual_type: Option<TypeId>,
    ) -> Option<(TypeId, TypeId)> {
        let ctx_type = contextual_type?;

        // Unwrap parenthesized expressions to find the actual callee.
        // Handles both `function*(){}()` and `(function*(){})()`.
        let is_function_expr = {
            let mut expr_idx = callee_expression;
            loop {
                match self.ctx.arena.get(expr_idx) {
                    Some(n)
                        if n.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                            || n.kind == syntax_kind_ext::ARROW_FUNCTION =>
                    {
                        break true;
                    }
                    Some(n) if n.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                        if let Some(paren) = self.ctx.arena.get_parenthesized(n) {
                            expr_idx = paren.expression;
                        } else {
                            break false;
                        }
                    }
                    _ => break false,
                }
            }
        };

        if is_function_expr {
            // Wrap contextual type as `() => ctx_type` so the function expression
            // resolver can use get_return_type() to extract the expected return type.
            let wrapper_fn = self
                .ctx
                .types
                .factory()
                .function(FunctionShape::new(vec![], ctx_type));
            Some((wrapper_fn, ctx_type))
        } else {
            None
        }
    }

    pub(crate) fn refreshed_generic_call_arg_type_with_context(
        &mut self,
        arg_idx: NodeIndex,
        cached_arg_type: TypeId,
        expected_type: Option<TypeId>,
    ) -> TypeId {
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return cached_arg_type;
        };

        if expected_type
            .is_some_and(|expected| common::contains_infer_types(self.ctx.types, expected))
        {
            return cached_arg_type;
        }

        let has_only_simple_parameters = || {
            self.ctx
                .arena
                .get_function(arg_node)
                .map(|func| {
                    func.parameters.nodes.iter().all(|&param_idx| {
                        self.ctx
                            .arena
                            .get(param_idx)
                            .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                            .and_then(|param| self.ctx.arena.get(param.name))
                            .is_some_and(|name_node| {
                                name_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
                                    && name_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
                            })
                    })
                })
                .unwrap_or(false)
        };

        let replace_arg_span_diagnostics =
            |checker: &mut CheckerState<'a>,
             idx: NodeIndex,
             f: &mut dyn FnMut(&mut CheckerState<'a>) -> TypeId| {
                let (start, end) = checker
                    .ctx
                    .arena
                    .get(idx)
                    .map(|node| (node.pos, node.end))
                    .unwrap_or((0, 0));
                let preserved_implicit_any_spans = checker
                    .ctx
                    .arena
                    .get(idx)
                    .filter(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
                    .map(|_| checker.object_literal_noncontextual_function_param_spans(idx))
                    .unwrap_or_default();
                checker
                    .ctx
                    .diagnostics
                    .retain(|diag| {
                        diag.start < start
                            || diag.start >= end
                            // Preserve TS2454 (variable used before assignment).
                            // The emitted_ts2454_errors dedup set is NOT rebuilt
                            // by rebuild_emitted_diagnostics_from_current, so once
                            // removed the error cannot be re-emitted.
                            || diag.code == 2454
                            || matches!(
                                diag.code,
                                crate::diagnostics::diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
                                    | crate::diagnostics::diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID
                            )
                            || (matches!(
                                diag.code,
                                crate::diagnostics::diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                                    | crate::diagnostics::diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE
                                    | crate::diagnostics::diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
                                    | crate::diagnostics::diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN
                            ) && preserved_implicit_any_spans.iter().any(|(span_start, span_end)| {
                                diag.start >= *span_start && diag.start < *span_end
                            }))
                    });
                checker.ctx.rebuild_emitted_diagnostics_from_current();
                f(checker)
            };

        match arg_node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                let ctx_type = self.contextual_type_option_for_expression(expected_type);
                let Some(ctx_type) = ctx_type
                    .filter(|ty| self.generic_object_literal_refresh_context_is_useful(*ty))
                else {
                    return cached_arg_type;
                };
                let request = TypingRequest::with_contextual_type(ctx_type);
                let mut recompute = |checker: &mut CheckerState<'a>| {
                    checker.invalidate_expression_for_contextual_retry(arg_idx);
                    checker.get_type_of_node_with_request(arg_idx, &request)
                };
                replace_arg_span_diagnostics(self, arg_idx, &mut recompute)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || ((k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION)
                    && has_only_simple_parameters()) =>
            {
                let expected_has_unresolved_callable_context =
                    expected_type.is_some_and(|expected| {
                        (common::contains_type_parameters(self.ctx.types, expected)
                            || common::contains_infer_types(self.ctx.types, expected))
                            && call_checker::get_contextual_signature(self.ctx.types, expected)
                                .or_else(|| {
                                    let evaluated = self.evaluate_type_with_env(expected);
                                    call_checker::get_contextual_signature(
                                        self.ctx.types,
                                        evaluated,
                                    )
                                })
                                .is_some()
                    });
                if (k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION)
                    && expected_has_unresolved_callable_context
                    && call_checker::get_contextual_signature(self.ctx.types, cached_arg_type)
                        .or_else(|| {
                            let evaluated = self.evaluate_type_with_env(cached_arg_type);
                            call_checker::get_contextual_signature(self.ctx.types, evaluated)
                        })
                        .is_some()
                {
                    return cached_arg_type;
                }

                let expected_is_concrete = expected_type
                    .is_some_and(|expected| self.generic_arg_refresh_context_is_concrete(expected));
                if (k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION)
                    && expected_is_concrete
                    && expected_type.is_some_and(|expected| {
                        call_checker::get_contextual_signature(self.ctx.types, expected)
                            .or_else(|| {
                                let evaluated = self.evaluate_type_with_env(expected);
                                call_checker::get_contextual_signature(self.ctx.types, evaluated)
                            })
                            .is_some()
                    })
                    && call_checker::get_contextual_signature(self.ctx.types, cached_arg_type)
                        .or_else(|| {
                            let evaluated = self.evaluate_type_with_env(cached_arg_type);
                            call_checker::get_contextual_signature(self.ctx.types, evaluated)
                        })
                        .is_some()
                {
                    return cached_arg_type;
                }

                // Re-evaluate context-sensitive arguments under the final instantiated
                // parameter type. Generic round-2 collection can still leave behind
                // provisional diagnostics from a less-specific contextual pass.
                // Object literals are excluded here: request-first typing no longer
                // stores contextual object-literal results in the ambient node cache,
                // so re-entering them late is prone to dropping nested member context.
                let ctx_type = self.contextual_type_option_for_call_argument(
                    expected_type,
                    arg_idx,
                    CallableContext::none(),
                );
                let Some(ctx_type) = ctx_type.filter(|ty| {
                    if k == syntax_kind_ext::FUNCTION_EXPRESSION
                        || k == syntax_kind_ext::ARROW_FUNCTION
                    {
                        self.generic_arg_refresh_context_is_concrete(*ty)
                            || self.generic_callable_refresh_context_is_useful(*ty)
                    } else {
                        self.generic_arg_refresh_context_is_concrete(*ty)
                    }
                }) else {
                    return cached_arg_type;
                };
                let request = TypingRequest::with_contextual_type(ctx_type);
                let mut recompute = |checker: &mut CheckerState<'a>| {
                    checker.invalidate_expression_for_contextual_retry(arg_idx);
                    checker.get_type_of_node_with_request(arg_idx, &request)
                };
                replace_arg_span_diagnostics(self, arg_idx, &mut recompute)
            }
            _ => cached_arg_type,
        }
    }

    pub(crate) fn class_constructor_display_def_id(
        &self,
        type_id: TypeId,
    ) -> Option<tsz_solver::def::DefId> {
        let def_id = self.ctx.definition_store.find_def_for_type(type_id)?;
        self.ctx
            .definition_store
            .get(def_id)
            .filter(|def| def.is_class_constructor())
            .map(|_| def_id)
    }

    pub(crate) fn call_signature_accepts_arg_count(
        &self,
        sig: &tsz_solver::CallSignature,
        arg_count: usize,
    ) -> bool {
        let required_count = sig.params.iter().filter(|param| !param.optional).count();
        let has_rest = sig.params.iter().any(|param| param.rest);
        if has_rest {
            arg_count >= required_count
        } else {
            arg_count >= required_count && arg_count <= sig.params.len()
        }
    }

    pub(crate) fn raw_param_for_argument_index<'b>(
        &self,
        sig: &'b tsz_solver::CallSignature,
        index: usize,
    ) -> Option<&'b tsz_solver::ParamInfo> {
        sig.params
            .get(index)
            .or_else(|| sig.params.last().filter(|param| param.rest))
    }

    pub(crate) fn type_display_skeleton(&self, type_id: TypeId, depth: usize) -> Option<String> {
        if depth == 0 {
            return Some("*".to_string());
        }

        if let Some(shape) = common::callable_shape_for_type(self.ctx.types, type_id) {
            let properties = shape
                .properties
                .iter()
                .map(|prop| {
                    format!(
                        "{}:{}:{}:{}",
                        self.ctx.types.resolve_atom_ref(prop.name),
                        u8::from(prop.optional),
                        u8::from(prop.is_method),
                        u8::from(prop.is_class_prototype),
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let call_sigs = shape
                .call_signatures
                .iter()
                .filter_map(|sig| self.call_signature_display_skeleton(sig, depth - 1))
                .collect::<Vec<_>>()
                .join(",");
            let construct_sigs = shape
                .construct_signatures
                .iter()
                .filter_map(|sig| self.call_signature_display_skeleton(sig, depth - 1))
                .collect::<Vec<_>>()
                .join(",");
            return Some(format!(
                "callable|a{}|si{}|ni{}|props[{properties}]|calls[{call_sigs}]|ctors[{construct_sigs}]",
                u8::from(shape.is_abstract),
                u8::from(shape.string_index.is_some()),
                u8::from(shape.number_index.is_some()),
            ));
        }

        if let Some(shape) = common::object_shape_for_type(self.ctx.types, type_id) {
            let properties = shape
                .properties
                .iter()
                .map(|prop| {
                    format!(
                        "{}:{}:{}:{}",
                        self.ctx.types.resolve_atom_ref(prop.name),
                        u8::from(prop.optional),
                        u8::from(prop.is_method),
                        u8::from(prop.is_class_prototype),
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            return Some(format!(
                "object|si{}|ni{}|props[{properties}]",
                u8::from(shape.string_index.is_some()),
                u8::from(shape.number_index.is_some()),
            ));
        }

        if let Some(info) = common::type_param_info(self.ctx.types, type_id) {
            return Some(format!(
                "type_param:{}",
                self.ctx.types.resolve_atom_ref(info.name)
            ));
        }
        // Fallback: use the interned TypeId as a stable discriminator for matching.
        Some(format!("tid:{}", type_id.0))
    }

    pub(crate) fn call_signature_display_skeleton(
        &self,
        sig: &tsz_solver::CallSignature,
        depth: usize,
    ) -> Option<String> {
        let params = sig
            .params
            .iter()
            .map(|param| format!("{}:{}", u8::from(param.optional), u8::from(param.rest),))
            .collect::<Vec<_>>()
            .join(",");
        let return_type = self.type_display_skeleton(sig.return_type, depth)?;
        Some(format!(
            "sig|params[{params}]|this{}|ret[{return_type}]",
            u8::from(sig.this_type.is_some()),
        ))
    }

    pub(crate) fn propagate_generic_constructor_display_defs(
        &mut self,
        callee_type: TypeId,
        arg_count: usize,
        instantiated_params: &[tsz_solver::ParamInfo],
    ) {
        let applicable: Vec<tsz_solver::CallSignature> =
            if let Some(shape) = common::function_shape_for_type(self.ctx.types, callee_type) {
                let sig = tsz_solver::CallSignature {
                    type_params: shape.type_params.clone(),
                    params: shape.params.clone(),
                    this_type: shape.this_type,
                    return_type: shape.return_type,
                    type_predicate: shape.type_predicate.clone(),
                    is_method: shape.is_method,
                };
                self.call_signature_accepts_arg_count(&sig, arg_count)
                    .then_some(vec![sig])
                    .unwrap_or_default()
            } else if let Some(signatures) =
                common::call_signatures_for_type(self.ctx.types, callee_type)
            {
                signatures
                    .into_iter()
                    .filter(|sig| self.call_signature_accepts_arg_count(sig, arg_count))
                    .collect()
            } else {
                Vec::new()
            };
        if applicable.is_empty() {
            return;
        }

        for (index, instantiated_param) in instantiated_params.iter().enumerate() {
            if self
                .ctx
                .definition_store
                .find_def_for_type(instantiated_param.type_id)
                .is_some()
            {
                continue;
            }

            let Some(instantiated_skeleton) =
                self.type_display_skeleton(instantiated_param.type_id, 2)
            else {
                continue;
            };

            let mut matched_def = None;
            let mut ambiguous = false;

            for sig in &applicable {
                let Some(raw_param) = self.raw_param_for_argument_index(sig, index) else {
                    continue;
                };
                let Some(def_id) = self.class_constructor_display_def_id(raw_param.type_id) else {
                    continue;
                };
                let Some(raw_skeleton) = self.type_display_skeleton(raw_param.type_id, 2) else {
                    continue;
                };
                if raw_skeleton != instantiated_skeleton {
                    continue;
                }
                if matched_def.is_some_and(|existing| existing != def_id) {
                    ambiguous = true;
                    break;
                }
                matched_def = Some(def_id);
            }

            if !ambiguous && let Some(def_id) = matched_def {
                self.ctx
                    .definition_store
                    .register_type_to_def(instantiated_param.type_id, def_id);
            }
        }
    }

    pub(crate) fn object_literal_has_computed_property_names(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
            return false;
        };

        obj.elements.nodes.iter().any(|&elem_idx| {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                return false;
            };
            let name_idx = if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                Some(prop.name)
            } else if let Some(method) = self.ctx.arena.get_method_decl(elem_node) {
                Some(method.name)
            } else {
                self.ctx
                    .arena
                    .get_accessor(elem_node)
                    .map(|accessor| accessor.name)
            };

            name_idx
                .and_then(|name_idx| self.ctx.arena.get(name_idx))
                .is_some_and(|name_node| name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
        })
    }
}
