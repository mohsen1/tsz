use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

const HOTSPOT_TRACE_STATEMENT_LIMIT: usize = 10;
const HOTSPOT_TRACE_DECLARATION_LIMIT: usize = 8;
const HOTSPOT_TRACE_EXPRESSION_DEPTH_LIMIT: usize = 6;
const HOTSPOT_TRACE_ARRAY_ELEMENT_LIMIT: usize = 8;
const HOTSPOT_TRACE_CALL_ARG_LIMIT: usize = 6;

impl<'a> CheckerState<'a> {
    pub fn trace_exported_variable_hotspots_with_progress(
        &mut self,
        root_idx: NodeIndex,
        mut report: impl FnMut(&str),
    ) {
        let Some(node) = self.ctx.arena.get(root_idx) else {
            return;
        };
        let Some(sf) = self.ctx.arena.get_source_file(node) else {
            return;
        };

        for (stmt_position, &stmt_idx) in sf.statements.nodes.iter().enumerate() {
            let Some(export_decl) = self.ctx.arena.get_export_decl_at(stmt_idx) else {
                continue;
            };
            if export_decl.export_clause.is_none() {
                continue;
            }

            let clause_idx = export_decl.export_clause;
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            if clause_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                continue;
            }

            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            let statement_phase = format!(
                "check_top_level_statements::statement_{stmt_position}::kind_{}::check_export_clause_statement::variable_statement",
                stmt_node.kind
            );
            report(&format!("{statement_phase}:start"));
            self.trace_variable_statement_hotspots_with_progress(
                clause_idx,
                &statement_phase,
                &mut report,
            );
        }
    }

    fn trace_variable_statement_hotspots_with_progress<F: FnMut(&str)>(
        &mut self,
        stmt_idx: NodeIndex,
        prefix: &str,
        report: &mut F,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.ctx.arena.get_variable(node) else {
            return;
        };

        report(&format!(
            "{prefix}::declaration_list_count_{}:start",
            var_stmt.declarations.nodes.len()
        ));
        for (list_position, &list_idx) in var_stmt
            .declarations
            .nodes
            .iter()
            .take(HOTSPOT_TRACE_DECLARATION_LIMIT)
            .enumerate()
        {
            let list_phase = format!("{prefix}::declaration_list_{list_position}");
            report(&format!("{list_phase}:start"));
            self.trace_variable_declaration_list_hotspots_with_progress(
                list_idx,
                &list_phase,
                report,
            );
        }
    }

    fn trace_variable_declaration_list_hotspots_with_progress<F: FnMut(&str)>(
        &mut self,
        list_idx: NodeIndex,
        prefix: &str,
        report: &mut F,
    ) {
        let Some(node) = self.ctx.arena.get(list_idx) else {
            return;
        };
        let Some(var_list) = self.ctx.arena.get_variable(node) else {
            return;
        };

        report(&format!(
            "{prefix}::declaration_count_{}:start",
            var_list.declarations.nodes.len()
        ));
        for (decl_position, &decl_idx) in var_list
            .declarations
            .nodes
            .iter()
            .take(HOTSPOT_TRACE_DECLARATION_LIMIT)
            .enumerate()
        {
            let decl_phase = format!("{prefix}::declaration_{decl_position}");
            report(&format!("{decl_phase}:start"));
            self.trace_variable_declaration_hotspots_with_progress(decl_idx, &decl_phase, report);
        }
    }

    fn trace_variable_declaration_hotspots_with_progress<F: FnMut(&str)>(
        &mut self,
        decl_idx: NodeIndex,
        prefix: &str,
        report: &mut F,
    ) {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return;
        };
        if var_decl.initializer.is_none() {
            return;
        }

        let initializer = var_decl.initializer;
        let Some(init_node) = self.ctx.arena.get(initializer) else {
            return;
        };
        let init_phase = format!("{prefix}::initializer");
        report(&format!("{init_phase}::kind_{}:start", init_node.kind));

        match init_node.kind {
            syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::ARROW_FUNCTION => {
                self.trace_function_like_hotspots_with_progress(initializer, &init_phase, report);
            }
            _ => self.trace_expression_hotspots_with_progress(initializer, &init_phase, report, 0),
        }
    }

    fn trace_function_like_hotspots_with_progress<F: FnMut(&str)>(
        &mut self,
        fn_idx: NodeIndex,
        prefix: &str,
        report: &mut F,
    ) {
        let Some(node) = self.ctx.arena.get(fn_idx) else {
            return;
        };
        let Some(function) = self.ctx.arena.get_function(node) else {
            return;
        };
        if function.body.is_none() {
            return;
        }

        let body_idx = function.body;
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };
        report(&format!(
            "{prefix}::function_body::kind_{}:start",
            body_node.kind
        ));

        if body_node.kind != syntax_kind_ext::BLOCK {
            self.trace_expression_hotspots_with_progress(
                body_idx,
                &format!("{prefix}::function_body::expression"),
                report,
                0,
            );
            return;
        }

        let Some(stmts) = self
            .ctx
            .arena
            .get_block(body_node)
            .map(|block| block.statements.nodes.clone())
        else {
            return;
        };

        report(&format!(
            "{prefix}::function_body::statement_count_{}:start",
            stmts.len()
        ));
        for (statement_position, &stmt_idx) in
            stmts.iter().take(HOTSPOT_TRACE_STATEMENT_LIMIT).enumerate()
        {
            let statement_phase = if let Some(stmt_node) = self.ctx.arena.get(stmt_idx) {
                format!(
                    "{prefix}::function_body::statement_{statement_position}::kind_{}",
                    stmt_node.kind
                )
            } else {
                format!("{prefix}::function_body::statement_{statement_position}")
            };
            report(&format!("{statement_phase}:start"));
            self.trace_statement_hotspots_with_progress(stmt_idx, &statement_phase, report);
        }
    }

    fn trace_statement_hotspots_with_progress<F: FnMut(&str)>(
        &mut self,
        stmt_idx: NodeIndex,
        prefix: &str,
        report: &mut F,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::VARIABLE_STATEMENT => {
                self.trace_variable_statement_hotspots_with_progress(stmt_idx, prefix, report);
            }
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret_stmt) = self.ctx.arena.get_return_statement(node)
                    && ret_stmt.expression.is_some()
                {
                    self.trace_expression_hotspots_with_progress(
                        ret_stmt.expression,
                        &format!("{prefix}::return_expression"),
                        report,
                        0,
                    );
                }
            }
            syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
                    self.trace_expression_hotspots_with_progress(
                        expr_stmt.expression,
                        &format!("{prefix}::expression"),
                        report,
                        0,
                    );
                }
            }
            _ => {
                report(&format!("{prefix}::check_statement:start"));
                self.check_statement(stmt_idx);
            }
        }
    }

    pub(crate) fn trace_expression_hotspots_with_progress<F: FnMut(&str)>(
        &mut self,
        expr_idx: NodeIndex,
        prefix: &str,
        report: &mut F,
        depth: usize,
    ) {
        if expr_idx.is_none() || depth > HOTSPOT_TRACE_EXPRESSION_DEPTH_LIMIT {
            return;
        }

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return;
        };
        report(&format!("{prefix}::kind_{}:start", node.kind));

        match node.kind {
            syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.trace_expression_hotspots_with_progress(
                        paren.expression,
                        &format!("{prefix}::expression"),
                        report,
                        depth + 1,
                    );
                }
            }
            syntax_kind_ext::AWAIT_EXPRESSION => {
                report(&format!("{prefix}::check_await_expression:start"));
                self.check_await_expression(expr_idx);
                if let Some(await_expr) = self.ctx.arena.get_unary_expr_ex(node) {
                    self.trace_expression_hotspots_with_progress(
                        await_expr.expression,
                        &format!("{prefix}::operand"),
                        report,
                        depth + 1,
                    );
                }
                report(&format!("{prefix}::get_type_of_node:start"));
                self.get_type_of_node(expr_idx);
            }
            syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    if let Some(callee_node) = self.ctx.arena.get(call.expression) {
                        report(&format!(
                            "{prefix}::call_callee_kind_{}:start",
                            callee_node.kind
                        ));
                    }

                    let arg_indices = call
                        .arguments
                        .as_ref()
                        .map(|args| args.nodes.clone())
                        .unwrap_or_default();
                    report(&format!(
                        "{prefix}::call_arg_count_{}:start",
                        arg_indices.len()
                    ));

                    if let Some(access) = self
                        .ctx
                        .arena
                        .get(call.expression)
                        .and_then(|callee| self.ctx.arena.get_access_expr(callee))
                    {
                        if let Some(receiver_node) = self.ctx.arena.get(access.expression) {
                            report(&format!(
                                "{prefix}::call_callee_receiver_kind_{}:start",
                                receiver_node.kind
                            ));
                        }
                        self.trace_expression_hotspots_with_progress(
                            access.expression,
                            &format!("{prefix}::call_callee_receiver"),
                            report,
                            depth + 1,
                        );
                        if let Some(property_name) = self
                            .ctx
                            .arena
                            .get_identifier_at(access.name_or_argument)
                            .map(|ident| ident.escaped_text.clone())
                        {
                            report(&format!(
                                "{prefix}::call_callee_receiver_resolve_property_access_with_env:start"
                            ));
                            report(&format!(
                                "{prefix}::call_callee_receiver_resolve_property_access_with_env::resolve_type_query_type:start"
                            ));
                            let receiver_type = self.get_type_of_node(access.expression);
                            let prepared_receiver =
                                self.prepare_property_access_receiver_type(receiver_type);
                            let lookup_receiver = self.resolve_type_query_type(prepared_receiver);
                            report(&format!(
                                "{prefix}::call_callee_receiver_resolve_property_access_with_env::initial_query:start"
                            ));
                            let lookup_result =
                                self.ctx.types.resolve_property_access_with_options(
                                    lookup_receiver,
                                    &property_name,
                                    self.ctx.compiler_options.no_unchecked_indexed_access,
                                );
                            report(&format!(
                                "{prefix}::call_callee_receiver_resolve_property_access_with_env::post_query:start"
                            ));
                            let _ = self.resolve_property_access_with_env_post_query(
                                lookup_receiver,
                                &property_name,
                                lookup_result,
                            );
                        }
                    } else {
                        self.trace_expression_hotspots_with_progress(
                            call.expression,
                            &format!("{prefix}::call_callee"),
                            report,
                            depth + 1,
                        );
                    }

                    report(&format!("{prefix}::call_callee_get_type_of_node:start"));
                    self.get_type_of_node(call.expression);

                    for (arg_position, arg_idx) in arg_indices
                        .iter()
                        .copied()
                        .take(HOTSPOT_TRACE_CALL_ARG_LIMIT)
                        .enumerate()
                    {
                        self.trace_expression_hotspots_with_progress(
                            arg_idx,
                            &format!("{prefix}::call_arg_{arg_position}"),
                            report,
                            depth + 1,
                        );
                    }

                    report(&format!("{prefix}::get_type_of_node:start"));
                    self.get_type_of_node(expr_idx);
                }
            }
            syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(array) = self.ctx.arena.get_literal_expr(node) {
                    report(&format!(
                        "{prefix}::element_count_{}:start",
                        array.elements.nodes.len()
                    ));
                    for (element_position, element_idx) in array
                        .elements
                        .nodes
                        .iter()
                        .copied()
                        .take(HOTSPOT_TRACE_ARRAY_ELEMENT_LIMIT)
                        .enumerate()
                    {
                        self.trace_expression_hotspots_with_progress(
                            element_idx,
                            &format!("{prefix}::element_{element_position}"),
                            report,
                            depth + 1,
                        );
                    }
                }
                report(&format!("{prefix}::get_type_of_node:start"));
                self.get_type_of_node(expr_idx);
            }
            syntax_kind_ext::SPREAD_ELEMENT => {
                if let Some(spread) = self.ctx.arena.get_spread(node) {
                    self.trace_expression_hotspots_with_progress(
                        spread.expression,
                        &format!("{prefix}::spread_expression"),
                        report,
                        depth + 1,
                    );
                }
                report(&format!("{prefix}::get_type_of_node:start"));
                self.get_type_of_node(expr_idx);
            }
            _ => {
                report(&format!("{prefix}::get_type_of_node:start"));
                self.get_type_of_node(expr_idx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_solver::TypeInterner;

    #[test]
    fn trace_exported_variable_hotspots_with_progress_traces_arrow_body_subphases() {
        let source = r#"
export const summarize = async (values: readonly string[]) => {
    const joined = values.map((value) => value.toUpperCase()).join(",");
    return [joined, ...values].join("|");
};
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions::default(),
        );
        checker.ctx.set_lib_contexts(Vec::new());

        let mut phases = Vec::new();
        checker.trace_exported_variable_hotspots_with_progress(root, |phase| {
            phases.push(phase.to_string());
        });

        assert!(
            phases.iter().any(|phase| {
                phase.contains(
                    "check_export_clause_statement::variable_statement::declaration_list_0::declaration_0::initializer::function_body::statement_count_2:start"
                )
            }),
            "expected exported variable function body statement marker, got {phases:?}"
        );
        assert!(
            phases.iter().any(|phase| {
                phase.contains("function_body::statement_0::")
                    && phase.contains(
                        "declaration_list_0::declaration_0::initializer::call_callee_receiver_resolve_property_access_with_env::initial_query:start"
                    )
            }),
            "expected nested variable initializer property access marker, got {phases:?}"
        );
        assert!(
            phases.iter().any(|phase| {
                phase.contains(
                    "function_body::statement_1::kind_254::return_expression::call_callee_receiver::element_1::spread_expression::get_type_of_node:start"
                )
            }),
            "expected return expression spread tracing marker, got {phases:?}"
        );
    }
}
