use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn check_jsx_special_attribute_function_body(
        &mut self,
        function_idx: NodeIndex,
        contextual_function_type: TypeId,
        request: &TypingRequest,
    ) {
        let Some(function_node) = self.ctx.arena.get(function_idx) else {
            return;
        };
        let Some(function) = self.ctx.arena.get_function(function_node) else {
            return;
        };
        let Some(body_node) = self.ctx.arena.get(function.body) else {
            return;
        };
        if body_node.kind == syntax_kind_ext::BLOCK {
            return;
        }

        let helper = tsz_solver::ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            contextual_function_type,
            self.ctx.compiler_options.no_implicit_any,
        );
        let mut param_contexts: Vec<(String, TypeId)> = Vec::new();
        let param_types: Vec<Option<TypeId>> = function
            .parameters
            .nodes
            .iter()
            .enumerate()
            .map(|(index, &param_idx)| {
                let param_type = helper.get_parameter_type(index);
                if let Some(param_type) = param_type
                    && let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                    && let Some(name_node) = self.ctx.arena.get(param.name)
                    && let Some(name) = self.ctx.arena.get_identifier(name_node)
                {
                    param_contexts.push((name.escaped_text.clone(), param_type));
                }
                param_type
            })
            .collect();
        self.cache_parameter_types(&function.parameters.nodes, Some(&param_types));
        self.check_jsx_special_attribute_parameter_property_accesses(
            function.body,
            &param_contexts,
        );
        self.invalidate_expression_for_contextual_retry(function.body);
        let body_request = request.read().normal_origin().contextual_opt(None);
        self.compute_type_of_node_with_request(function.body, &body_request);
    }

    fn check_jsx_special_attribute_parameter_property_accesses(
        &mut self,
        expr_idx: NodeIndex,
        param_contexts: &[(String, TypeId)],
    ) {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return;
        };
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
                return;
            };
            if let Some(base_ident) = self.ctx.arena.get_identifier_at(access.expression)
                && let Some((_, param_type)) = param_contexts
                    .iter()
                    .find(|(name, _)| name == &base_ident.escaped_text)
                && let Some(prop_ident) = self.ctx.arena.get_identifier_at(access.name_or_argument)
            {
                use crate::query_boundaries::common::PropertyAccessResult;

                let access_type = self.resolve_type_for_property_access(*param_type);
                let property_result =
                    self.resolve_property_access_with_env(access_type, &prop_ident.escaped_text);
                let should_report = match property_result {
                    PropertyAccessResult::PropertyNotFound { .. } => true,
                    PropertyAccessResult::Success {
                        type_id,
                        from_index_signature,
                        ..
                    } => {
                        type_id == TypeId::ANY
                            && !from_index_signature
                            && !self.source_text_declares_property_name(&prop_ident.escaped_text)
                    }
                    _ => false,
                };
                if should_report {
                    let type_display = self.format_type(*param_type);
                    self.error_property_not_exist_with_apparent_type(
                        &prop_ident.escaped_text,
                        &type_display,
                        access.name_or_argument,
                    );
                }
            }
            self.check_jsx_special_attribute_parameter_property_accesses(
                access.expression,
                param_contexts,
            );
        } else if expr_node.kind == syntax_kind_ext::CALL_EXPRESSION {
            if let Some(call) = self.ctx.arena.get_call_expr(expr_node) {
                self.check_jsx_special_attribute_parameter_property_accesses(
                    call.expression,
                    param_contexts,
                );
            }
        } else if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.ctx.arena.get_parenthesized(expr_node)
        {
            self.check_jsx_special_attribute_parameter_property_accesses(
                paren.expression,
                param_contexts,
            );
        }
    }

    fn source_text_declares_property_name(&self, property_name: &str) -> bool {
        // DOM replacement interfaces can resolve inherited members to a bare
        // `any` in this synthetic recovery path. Treat well-known DOM members as
        // declared so the recovery reports only genuinely missing properties.
        if matches!(
            property_name,
            "innerText"
                | "innerHTML"
                | "outerHTML"
                | "textContent"
                | "style"
                | "id"
                | "className"
                | "children"
                | "parentElement"
                | "addEventListener"
                | "removeEventListener"
        ) {
            return true;
        }
        let property_pattern = format!("{property_name}:");
        let readonly_property_pattern = format!("readonly {property_name}:");
        let method_pattern = format!("{property_name}(");
        let source_has_decl = |source: &tsz_parser::parser::node::SourceFileData| {
            let text = source.text.as_ref();
            text.contains(&property_pattern)
                || text.contains(&readonly_property_pattern)
                || text.contains(&method_pattern)
        };
        self.ctx.arena.source_files.iter().any(source_has_decl)
            || self.ctx.all_arenas.as_ref().is_some_and(|arenas| {
                arenas
                    .iter()
                    .any(|arena| arena.source_files.iter().any(source_has_decl))
            })
    }
}
