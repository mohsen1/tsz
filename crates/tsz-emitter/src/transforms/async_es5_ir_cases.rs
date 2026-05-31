use crate::transforms::async_es5_ir::{AsyncES5Transformer, opcodes};
use crate::transforms::ir::{IRGeneratorCase, IRNode};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;

impl AsyncES5Transformer<'_> {
    /// Build generator cases for the state machine.
    pub(super) fn build_generator_cases(
        &mut self,
        body_idx: NodeIndex,
        _has_await: bool,
        skipped_statements: &[NodeIndex],
    ) -> Vec<IRGeneratorCase> {
        let previous_planned_catches =
            std::mem::take(&mut *self.planned_catch_binding_temps.borrow_mut());
        self.reset_temp_name_reservations(body_idx);
        self.plan_catch_binding_temps(body_idx);
        self.plan_body_level_helpers(body_idx);
        let mut cases = Vec::new();
        let mut current_statements = Vec::new();
        let mut current_label = self.state.next_label();

        self.process_async_body(
            body_idx,
            &mut cases,
            &mut current_statements,
            &mut current_label,
            skipped_statements,
        );

        if !current_statements.is_empty() {
            let needs_implicit_return =
                !matches!(current_statements.last(), Some(IRNode::ReturnStatement(_)));
            if needs_implicit_return {
                current_statements.push(Self::async_return_none());
            }
            cases.push(IRGeneratorCase {
                label: current_label,
                statements: current_statements,
            });
        } else if !cases.is_empty() {
            cases.push(IRGeneratorCase {
                label: current_label,
                statements: vec![Self::async_return_none()],
            });
        } else {
            cases.push(IRGeneratorCase {
                label: 0,
                statements: vec![Self::async_return_none()],
            });
        }

        *self.planned_catch_binding_temps.borrow_mut() = previous_planned_catches;
        cases
    }

    fn async_return_none() -> IRNode {
        IRNode::ReturnStatement(Some(Box::new(IRNode::GeneratorOp {
            opcode: opcodes::RETURN,
            value: None,
            comment: Some("return".to_string().into()),
        })))
    }

    fn plan_body_level_helpers(&mut self, body_idx: NodeIndex) {
        if self.contains_for_await_recursive(body_idx) {
            self.helpers_needed.mark_async_values();
        }
        if self.contains_array_spread_recursive(body_idx) {
            self.helpers_needed.mark_spread_array();
        }
    }

    fn plan_catch_binding_temps(&self, idx: NodeIndex) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.is_function_expression_or_arrow()
        {
            return;
        }

        if node.kind == syntax_kind_ext::CLASS_DECLARATION
            || node.kind == syntax_kind_ext::CLASS_EXPRESSION
        {
            return;
        }

        if node.kind == syntax_kind_ext::TRY_STATEMENT
            && let Some(try_data) = self.arena.get_try(node)
        {
            let try_has_await = self.contains_await_recursive(try_data.try_block);
            let catch_has_await = self.contains_await_recursive(try_data.catch_clause);
            let finally_has_await = self.contains_await_recursive(try_data.finally_block);
            if (try_has_await || catch_has_await || finally_has_await)
                && let Some(catch_node) = self.arena.get(try_data.catch_clause)
                && let Some(catch_data) = self.arena.get_catch_clause(catch_node)
                && catch_data.variable_declaration.is_some()
            {
                let catch_var_name = self.get_catch_variable_name(catch_data.variable_declaration);
                if !catch_var_name.is_empty() {
                    let catch_temp =
                        self.fresh_catch_binding_temp(&catch_var_name, try_data.catch_clause);
                    self.planned_catch_binding_temps
                        .borrow_mut()
                        .insert(try_data.catch_clause.0, catch_temp);
                }
            }
        }

        for child in self.arena.get_children(idx) {
            self.plan_catch_binding_temps(child);
        }
    }
}
