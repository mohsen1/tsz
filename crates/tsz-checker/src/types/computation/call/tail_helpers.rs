use crate::query_boundaries::type_computation::complex as query;
use crate::state::CheckerState;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn explicit_identifier_callee_annotation_type(
        &mut self,
        callee_expr: NodeIndex,
    ) -> Option<TypeId> {
        let callee_node = self.ctx.arena.get(callee_expr)?;
        if callee_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        let initial_sym_id = self
            .ctx
            .binder
            .node_symbols
            .get(&callee_expr.0)
            .copied()
            .or_else(|| self.resolve_identifier_symbol(callee_expr))?;
        let sym_id = self
            .ctx
            .alias_partner_for(self.ctx.binder, initial_sym_id)
            .unwrap_or(initial_sym_id);
        let value_declaration = self.ctx.binder.get_symbol(sym_id).and_then(|symbol| {
            symbol
                .value_declaration
                .is_some()
                .then_some(symbol.value_declaration)
                .or_else(|| symbol.declarations.first().copied())
        })?;
        let declaration_node = self.ctx.arena.get(value_declaration)?;
        if declaration_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.ctx.arena.get_variable_declaration(declaration_node)?;
        if var_decl.type_annotation.is_none()
            || self
                .find_circular_reference_in_type_node(var_decl.type_annotation, sym_id, false)
                .is_some()
        {
            return None;
        }

        let annotated_type_node = self.get_type_from_type_node(var_decl.type_annotation);
        let annotated_type = self.resolve_ref_type(annotated_type_node);
        if let Some(callable) = self.callable_callee_type_from_candidate(annotated_type) {
            return Some(callable);
        }

        if var_decl.initializer.is_some() {
            let initializer_type = self.get_type_of_node(var_decl.initializer);
            return self.callable_callee_type_from_candidate(initializer_type);
        }

        None
    }

    fn callable_callee_type_from_candidate(&mut self, candidate: TypeId) -> Option<TypeId> {
        if matches!(candidate, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }

        let evaluated = self.evaluate_application_type(candidate);
        let resolved = self.resolve_lazy_type(evaluated);
        match query::classify_for_call_signatures(self.ctx.types, resolved) {
            query::CallSignaturesKind::Callable(_)
            | query::CallSignaturesKind::MultipleSignatures(_) => Some(candidate),
            query::CallSignaturesKind::NoSignatures => None,
        }
    }

    pub(super) fn report_checked_js_nullable_this_property_method_call(
        &mut self,
        callee_expr: NodeIndex,
    ) {
        if !self.is_js_file()
            || !self.ctx.compiler_options.check_js
            || !self.ctx.compiler_options.strict_null_checks
        {
            return;
        }

        let Some(callee_node) = self.ctx.arena.get(callee_expr) else {
            return;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return;
        }
        let Some(callee_access) = self.ctx.arena.get_access_expr(callee_node) else {
            return;
        };
        if callee_access.question_dot_token {
            return;
        }

        let receiver_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(callee_access.expression);
        let Some(receiver_node) = self.ctx.arena.get(receiver_idx) else {
            return;
        };
        if receiver_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return;
        }
        let Some(receiver_access) = self.ctx.arena.get_access_expr(receiver_node) else {
            return;
        };

        let base_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(receiver_access.expression);
        if self
            .ctx
            .arena
            .get(base_idx)
            .is_none_or(|node| node.kind != tsz_scanner::SyntaxKind::ThisKeyword as u16)
        {
            return;
        }

        let receiver_type =
            if let Some(receiver_type) = self.ctx.node_types.get(&receiver_idx.0).copied() {
                receiver_type
            } else {
                self.get_type_of_node(receiver_idx)
            };
        let receiver_type = self.evaluate_type_with_env(receiver_type);
        let (_, cause) = self.split_nullish_type(receiver_type);
        let Some(cause) = cause else {
            return;
        };

        let Some(receiver_span) = self
            .ctx
            .arena
            .get(receiver_idx)
            .map(|node| (node.pos, node.end))
        else {
            return;
        };

        let (code, message) = if cause == TypeId::NULL {
            (
                diagnostic_codes::OBJECT_IS_POSSIBLY_NULL,
                crate::diagnostics::diagnostic_messages::OBJECT_IS_POSSIBLY_NULL,
            )
        } else if cause == TypeId::UNDEFINED {
            (
                diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED,
                crate::diagnostics::diagnostic_messages::OBJECT_IS_POSSIBLY_UNDEFINED,
            )
        } else {
            (
                diagnostic_codes::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED,
                crate::diagnostics::diagnostic_messages::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED,
            )
        };

        let already_reported = self.ctx.diagnostics.iter().any(|diag| {
            matches!(
                diag.code,
                diagnostic_codes::OBJECT_IS_POSSIBLY_NULL
                    | diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED
                    | diagnostic_codes::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED
                    | diagnostic_codes::IS_POSSIBLY_NULL
                    | diagnostic_codes::IS_POSSIBLY_UNDEFINED
                    | diagnostic_codes::IS_POSSIBLY_NULL_OR_UNDEFINED
            ) && diag.start >= receiver_span.0
                && diag.start < receiver_span.1
        });
        if !already_reported {
            self.error_at_node(receiver_idx, message, code);
        }
    }
}
