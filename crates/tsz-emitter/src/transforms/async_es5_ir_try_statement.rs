use crate::transforms::async_es5_ir::try_region::{
    TryRegionPlaceholders, TryRegionResolution, patch_try_region_placeholders,
};
use crate::transforms::async_es5_ir::{AsyncES5Transformer, opcodes};
use crate::transforms::ir::{IRGeneratorCase, IRNode};
use tsz_parser::parser::NodeIndex;

impl<'a> AsyncES5Transformer<'a> {
    /// Process a try/catch/finally statement inside an async function body.
    ///
    /// When none of the blocks contain await, falls through to raw IR emission.
    /// When blocks contain await, generates proper state machine labels with
    /// try/catch/finally opcodes.
    pub(in crate::transforms) fn process_try_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(try_data) = self.arena.get_try(node) else {
            return;
        };

        let try_has_await = self.contains_await_recursive(try_data.try_block);
        let catch_has_await = self.contains_await_recursive(try_data.catch_clause);
        let finally_has_await = self.contains_await_recursive(try_data.finally_block);

        if !try_has_await && !catch_has_await && !finally_has_await {
            // No await in any block -- emit as-is
            let ir = self.statement_to_ir(idx);
            current_statements.push(ir);
            return;
        }

        let has_catch =
            try_data.catch_clause.is_some() && self.arena.get(try_data.catch_clause).is_some();
        let has_finally =
            try_data.finally_block.is_some() && self.arena.get(try_data.finally_block).is_some();

        if !has_catch && !has_finally {
            self.process_block_or_statement_in_async(
                try_data.try_block,
                cases,
                current_statements,
                current_label,
            );
            return;
        }

        // Sentinels share `next_loop_exit_placeholder` so the patch sweep cannot
        // collide with loop-exit placeholders still living in a surrounding loop.
        let placeholders = TryRegionPlaceholders {
            catch_slot: self.next_loop_exit_placeholder(),
            finally_slot: self.next_loop_exit_placeholder(),
            end_slot: self.next_loop_exit_placeholder(),
            exit_break: self.next_loop_exit_placeholder(),
        };
        let start_label = *current_label;
        let cases_start = cases.len();

        current_statements.push(IRNode::generator_try_push(
            start_label,
            has_catch.then_some(placeholders.catch_slot),
            has_finally.then_some(placeholders.finally_slot),
            placeholders.end_slot,
        ));

        self.process_block_or_statement_in_async(
            try_data.try_block,
            cases,
            current_statements,
            current_label,
        );
        current_statements.push(Self::generator_break_statement(placeholders.exit_break));

        let catch_label = if has_catch {
            let cl = self.state.next_label();
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            *current_label = cl;

            if let Some(catch_node) = self.arena.get(try_data.catch_clause)
                && let Some(catch_data) = self.arena.get_catch_clause(catch_node)
            {
                let catch_rename_depth = self.catch_binding_renames.len();
                if catch_data.variable_declaration.is_some() {
                    let catch_var_name =
                        self.get_catch_variable_name(catch_data.variable_declaration);
                    if !catch_var_name.is_empty() {
                        let catch_temp = self
                            .planned_catch_binding_temps
                            .borrow()
                            .get(&try_data.catch_clause.0)
                            .cloned()
                            .unwrap_or_else(|| {
                                self.fresh_catch_binding_temp(
                                    &catch_var_name,
                                    try_data.catch_clause,
                                )
                            });
                        self.blocked_temp_names
                            .borrow_mut()
                            .insert(catch_temp.clone());
                        current_statements.push(IRNode::VarDecl {
                            name: catch_temp.clone().into(),
                            initializer: None,
                        });
                        // tsc binds the exception via `_a.sent()`, not `_a[1]`.
                        current_statements.push(IRNode::ExpressionStatement(Box::new(
                            IRNode::assign(IRNode::id(catch_temp.clone()), IRNode::GeneratorSent),
                        )));
                        self.catch_binding_renames
                            .push((catch_var_name, catch_temp));
                    }
                }
                self.process_block_or_statement_in_async(
                    catch_data.block,
                    cases,
                    current_statements,
                    current_label,
                );
                if self.catch_binding_renames.len() > catch_rename_depth {
                    self.catch_binding_renames.pop();
                }
            }

            if !Self::async_statements_end_control_flow(current_statements) {
                current_statements.push(Self::generator_break_statement(placeholders.exit_break));
            }
            Some(cl)
        } else {
            None
        };

        let finally_label = if has_finally {
            let fl = self.state.next_label();
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
            *current_label = fl;

            self.process_block_or_statement_in_async(
                try_data.finally_block,
                cases,
                current_statements,
                current_label,
            );

            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::END_FINALLY,
                    value: None,
                    comment: Some("endfinally".to_string().into()),
                },
            ))));
            Some(fl)
        } else {
            None
        };

        // End label is allocated last so its number is past every interior resume.
        let end_label = self.state.next_label();

        let resolution = TryRegionResolution {
            placeholders,
            catch_label,
            finally_label,
            end_label,
            // Breaks from try/catch must target the region's end label even when
            // a finally exists; tsc's `__generator` driver detects the active try
            // entry on a `[3 /*break*/, end]` op, pushes the pending break onto
            // `_.ops`, then jumps to the finally label. After `[7 /*endfinally*/]`
            // pops `_.ops`, the driver resumes the original break against an
            // empty `_.trys` stack and lands at `end`. Breaking directly to the
            // finally label would jump there without pushing onto `_.ops`, so
            // `endfinally` would pop an empty stack and the state machine would
            // wedge.
            exit_target: end_label,
        };
        let cases_tail = cases[cases_start..]
            .iter_mut()
            .flat_map(|case| case.statements.iter_mut())
            .chain(current_statements.iter_mut());
        for stmt in cases_tail {
            patch_try_region_placeholders(stmt, &resolution);
        }

        if !current_statements.is_empty() {
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
        }
        *current_label = end_label;
    }
}
