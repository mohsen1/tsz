impl<'a> AsyncES5Transformer<'a> {
    /// Create a new `AsyncES5Transformer`
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            source_text: None,
            state: AsyncTransformState::new(),
            helpers_needed: HelpersNeeded::default(),
            generator_mode: false,
            async_generator_mode: false,
            downlevel_iteration: false,
            temp_var_counter: Cell::new(0),
            blocked_temp_names: RefCell::new(FxHashSet::default()),
            disposable_env_counter: Cell::new(1),
            blocked_disposable_env_names: FxHashSet::default(),
            generated_disposable_env_names: Vec::new(),
            lexical_this_capture: Cell::new(false),
            capture_this_references: Cell::new(false),
            loop_exit_placeholder_counter: Cell::new(0),
            pending_lowering_hoists: RefCell::new(Vec::new()),
            class_has_super: false,
            class_super_name: "_super".to_string(),
            class_super_is_static: false,
            module_kind: ModuleKind::None,
            target_es5: false,
            dynamic_import_promise_counter: Cell::new(1),
            labeled_continue_targets: Vec::new(),
            labeled_break_targets: Vec::new(),
            catch_binding_renames: Vec::new(),
            catch_binding_ordinals: RefCell::new(rustc_hash::FxHashMap::default()),
            planned_catch_binding_temps: RefCell::new(FxHashMap::default()),
        }
    }

    /// Record a hoisted-temp name produced by an IR-conversion lowering
    /// (`??`, `?.`, etc.) so the surrounding `transform_*` entry point can
    /// declare it alongside the rest of the state-machine var hoists.
    /// Transform an async function declaration to IR
    ///
    /// Returns an `IRNode::AwaiterCall` with a nested `IRNode::GeneratorBody`
    pub fn transform_async_function(&mut self, func_idx: NodeIndex) -> IRNode {
        self.state.reset();
        self.reset_loop_exit_placeholders();
        self.helpers_needed.awaiter = true;
        self.helpers_needed.generator = true;

        let Some(node) = self.arena.get(func_idx) else {
            return IRNode::Undefined;
        };

        // Get function details - all function types use FunctionData
        let (
            name,
            params,
            param_binding_names,
            body_idx,
            await_default_param_name,
            recover_await_default,
            type_annotation,
        ) = if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.is_function_expression_or_arrow()
        {
            if let Some(func) = self.arena.get_function(node) {
                let name = if func.name.is_none() {
                    None
                } else {
                    Some(crate::transforms::emit_utils::identifier_text_or_empty(
                        self.arena, func.name,
                    ))
                };
                let params = self.collect_parameters(&func.parameters);
                let mut param_binding_names = Vec::new();
                self.collect_parameter_binding_names(&func.parameters, &mut param_binding_names);
                let await_default_param_name =
                    self.first_await_default_param_name(&func.parameters);
                let recover_await_default =
                    super::emit_utils::block_is_empty(self.arena, func.body)
                        && await_default_param_name.is_some()
                        && func
                            .parameters
                            .nodes
                            .iter()
                            .copied()
                            .any(|p| self.param_initializer_has_top_level_await(p));
                (
                    name,
                    params,
                    param_binding_names,
                    func.body,
                    await_default_param_name,
                    recover_await_default,
                    func.type_annotation,
                )
            } else {
                return IRNode::Undefined;
            }
        } else {
            return IRNode::Undefined;
        };

        // Check if body contains await
        let has_await = self.body_contains_await(body_idx);
        self.state.has_await = has_await;

        // Check if body references `arguments`
        let captures_arguments =
            tsz_parser::syntax::transform_utils::contains_arguments_reference(self.arena, body_idx);
        self.state.captures_arguments = captures_arguments;
        if captures_arguments {
            self.state.arguments_capture_name =
                self.fresh_arguments_capture_name(body_idx, &param_binding_names);
        }

        if recover_await_default {
            let mut generated = String::new();
            generated.push_str("return __awaiter(this, arguments, void 0, function (");
            generated.push_str(&params.join(", "));
            generated.push_str(") {\n");
            if let Some(param_name) = await_default_param_name {
                generated.push_str("    if (");
                generated.push_str(&param_name);
                generated.push_str(" === void 0) { ");
                generated.push_str(&param_name);
                generated.push_str(" = _a.sent(); }\n");
            }
            generated.push_str("    return __generator(this, function (_a) {\n");
            generated.push_str("        switch (_a.label) {\n");
            generated.push_str("            case 0: return [4 /*yield*/, ];\n");
            generated.push_str("            case 1: return [2 /*return*/];\n");
            generated.push_str("        }\n");
            generated.push_str("    });\n");
            generated.push_str("});");

            if let Some(func_name) = name {
                return IRNode::FunctionDecl {
                    name: func_name.into(),
                    parameters: Vec::new(),
                    body: vec![IRNode::Raw(generated.into())],
                    body_source_range: None,
                    leading_comment: None,
                };
            }
            return IRNode::FunctionExpr {
                name: None,
                parameters: Vec::new(),
                body: vec![IRNode::Raw(generated.into())],
                is_expression_body: false,
                body_source_range: None,
            };
        }

        let mut hoisted_decls = Vec::new();
        let mut skipped_statements = Vec::new();
        // Function declarations inside async function bodies are always hoisted to
        // the __awaiter callback scope (before __generator), regardless of whether
        // the body contains await expressions.  This matches tsc behavior.
        if let Some(body_node) = self.arena.get(body_idx)
            && body_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.arena.get_block(body_node)
        {
            for &stmt_idx in &block.statements.nodes {
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                    continue;
                }
                if let Some(comment) = self.extract_preceding_line_comment(stmt_node.pos) {
                    hoisted_decls.push(IRNode::Raw(comment.into()));
                }
                skipped_statements.push(stmt_idx);
                if let Some(func) = self.arena.get_function(stmt_node) {
                    if func.is_async {
                        hoisted_decls.push(self.transform_async_function(stmt_idx));
                    } else {
                        hoisted_decls.push(IRNode::ASTRef(stmt_idx));
                    }
                } else {
                    hoisted_decls.push(IRNode::ASTRef(stmt_idx));
                }
            }
        }

        // Build the generator body
        let mut generator_body =
            self.build_generator_body(body_idx, has_await, &skipped_statements);

        // Extract directive prologues (e.g. "use strict") from the start of the
        // generator body.  tsc places these inside the __awaiter callback before
        // any var declarations and before __generator, so we pull them out here
        // and pass them to AwaiterCall for correct placement.
        let directives = Self::extract_and_remove_directive_prologue(&mut generator_body);

        // Hoist var declarations from generator cases to the awaiter wrapper scope.
        // In tsc output, var declarations inside async function bodies are placed
        // before `return __generator(...)`, not inside the switch/case statements.
        let hoisted_var_groups = self.extract_hoisted_var_groups(&mut generator_body);

        // Extract promise constructor from return type annotation
        let promise_constructor = self.extract_promise_constructor(type_annotation);

        // Build the awaiter call
        let awaiter_call = IRNode::AwaiterCall {
            this_arg: Box::new(IRNode::This { captured: false }),
            needs_lexical_this_capture: generator_body.contains_captured_this_reference(),
            generator_body: Box::new(generator_body),
            hoisted_var_groups,
            promise_constructor,
            multiline_callback: captures_arguments,
            directives,
        };

        // Build the function declaration/expression wrapper
        let ir_params: Vec<IRParam> = params.iter().map(|p| IRParam::new(p.clone())).collect();

        if let Some(func_name) = name {
            let mut body = hoisted_decls;
            self.emit_arguments_capture_decl(&mut body);
            body.push(awaiter_call);
            IRNode::FunctionDecl {
                name: func_name.into(),
                parameters: ir_params,
                body,
                body_source_range: None,
                leading_comment: None,
            }
        } else {
            let mut body = hoisted_decls;
            self.emit_arguments_capture_decl(&mut body);
            body.push(awaiter_call);
            IRNode::FunctionExpr {
                name: None,
                parameters: ir_params,
                body,
                is_expression_body: false,
                body_source_range: None,
            }
        }
    }

    pub fn transform_async_function_expression(&mut self, func_idx: NodeIndex) -> IRNode {
        match self.transform_async_function(func_idx) {
            IRNode::FunctionDecl {
                name,
                parameters,
                body,
                ..
            } => IRNode::FunctionExpr {
                name: Some(name),
                parameters,
                body,
                is_expression_body: false,
                body_source_range: None,
            },
            node => node,
        }
    }

    pub fn transform_async_generator_inner_function(
        &mut self,
        name: Option<String>,
        params: &[NodeIndex],
        body_idx: NodeIndex,
        include_params: bool,
    ) -> IRNode {
        self.state.reset();
        self.reset_loop_exit_placeholders();
        self.generator_mode = true;
        self.async_generator_mode = true;
        self.helpers_needed.await_helper = true;
        self.helpers_needed.async_generator = true;
        self.helpers_needed.generator = true;

        let mut param_binding_names = Vec::new();
        for &param_idx in params {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            self.collect_binding_name(param.name, &mut param_binding_names);
        }

        let has_yield = self.body_contains_await(body_idx);
        self.state.has_await = has_yield;
        self.state.captures_arguments =
            tsz_parser::syntax::transform_utils::contains_arguments_reference(self.arena, body_idx);
        if self.state.captures_arguments {
            self.state.arguments_capture_name =
                self.fresh_arguments_capture_name(body_idx, &param_binding_names);
        }

        let mut generator_body = self.build_generator_body(body_idx, has_yield, &[]);
        let hoisted_var_groups = self.extract_hoisted_var_groups(&mut generator_body);
        let mut body = Vec::new();
        for group in hoisted_var_groups {
            let declarations = group
                .into_iter()
                .map(|name| IRNode::VarDecl {
                    name: name.into(),
                    initializer: None,
                })
                .collect();
            body.push(IRNode::VarDeclList(declarations));
        }
        if self.state.captures_arguments {
            body.push(IRNode::VarDecl {
                name: self.state.arguments_capture_name.clone().into(),
                initializer: Some(Box::new(IRNode::Raw("arguments".to_string().into()))),
            });
        }
        body.push(generator_body);

        self.generator_mode = false;
        self.async_generator_mode = false;

        let ir_params = if include_params {
            params
                .iter()
                .filter_map(|&param_idx| {
                    let param_node = self.arena.get(param_idx)?;
                    let param = self.arena.get_parameter(param_node)?;
                    Some(IRParam::new(
                        crate::transforms::emit_utils::identifier_text_or_empty(
                            self.arena, param.name,
                        ),
                    ))
                })
                .collect()
        } else {
            Vec::new()
        };

        IRNode::FunctionExpr {
            name: name.map(Into::into),
            parameters: ir_params,
            body,
            is_expression_body: false,
            body_source_range: None,
        }
    }

    /// Extract a custom promise constructor expression from a function's return type annotation.
    fn extract_promise_constructor(&self, type_annotation: NodeIndex) -> Option<String> {
        let type_node = self.arena.get(type_annotation)?;
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.arena.get_type_ref(type_node)?;
        let type_name_node = self.arena.get(type_ref.type_name)?;
        if type_name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
            Some(self.qualified_name_to_expression(type_ref.type_name))
        } else {
            None
        }
    }

    /// Convert a type name node (identifier or qualified name) to a JS expression string.
    fn qualified_name_to_expression(&self, idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(idx) else {
            return String::new();
        };
        if node.kind == syntax_kind_ext::QUALIFIED_NAME
            && let Some(qn) = self.arena.get_qualified_name(node)
        {
            let left = self.qualified_name_to_expression(qn.left);
            let right =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, qn.right);
            return format!("{left}.{right}");
        }
        crate::transforms::emit_utils::identifier_text_or_empty(self.arena, idx)
    }

    /// Transform just the generator body (for use by the wrapper)
    pub fn transform_generator_body(&mut self, body_idx: NodeIndex, has_await: bool) -> IRNode {
        self.state.reset();
        self.reset_loop_exit_placeholders();
        self.state.has_await = has_await;
        self.helpers_needed.generator = true;

        // Check if body references `arguments` — if so, rewrite to `arguments_1`
        // (the caller is responsible for emitting `var arguments_1 = arguments;`)
        self.state.captures_arguments =
            tsz_parser::syntax::transform_utils::contains_arguments_reference(self.arena, body_idx);
        if self.state.captures_arguments && self.state.arguments_capture_name.is_empty() {
            self.state.arguments_capture_name = self.fresh_arguments_capture_name(body_idx, &[]);
        }

        self.build_generator_body(body_idx, has_await, &[])
    }

    pub fn transform_generator_body_skipping(
        &mut self,
        body_idx: NodeIndex,
        has_await: bool,
        skipped_statements: &[NodeIndex],
    ) -> IRNode {
        self.state.reset();
        self.reset_loop_exit_placeholders();
        self.state.has_await = has_await;
        self.helpers_needed.generator = true;

        self.state.captures_arguments =
            tsz_parser::syntax::transform_utils::contains_arguments_reference(self.arena, body_idx);
        if self.state.captures_arguments && self.state.arguments_capture_name.is_empty() {
            self.state.arguments_capture_name = self.fresh_arguments_capture_name(body_idx, &[]);
        }

        self.build_generator_body(body_idx, has_await, skipped_statements)
    }

    /// Build the generator body IR
    fn build_generator_body(
        &mut self,
        body_idx: NodeIndex,
        has_await: bool,
        skipped_statements: &[NodeIndex],
    ) -> IRNode {
        self.state.in_async_body = true;
        self.state.label_counter = 0;

        let cases = self.build_generator_cases(body_idx, has_await, skipped_statements);

        self.state.in_async_body = false;

        IRNode::GeneratorBody { has_await, cases }
    }

    fn process_async_body(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        skipped_statements: &[NodeIndex],
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        // Handle block statements
        if node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.arena.get_block(node) {
                self.process_async_statement_list(
                    &block.statements.nodes,
                    cases,
                    current_statements,
                    current_label,
                    skipped_statements,
                );
            }
            return;
        }

        // Handle concise arrow body (expression)
        // For concise arrow functions like `async () => await foo()`, the body is an expression
        // not a statement. We treat this as an implicit return of the expression.
        if node.kind == self.suspension_kind() {
            // return await/yield expr -> yield, then return _a.sent()
            self.process_await_expression(idx, cases, current_statements, current_label);
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::RETURN,
                    value: Some(Box::new(IRNode::GeneratorSent)),
                    comment: Some("return".to_string().into()),
                },
            ))));
        } else if self.contains_await_recursive(idx) {
            let value = if let Some(lowered_object) = self.lower_object_literal_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                lowered_object
            } else if let Some(lowered_call) = self.lower_call_callee_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                lowered_call
            } else if let Some(lowered_array) = self.lower_array_literal_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                lowered_array
            } else if let Some(lowered_access) = self.lower_element_access_object_before_suspension(
                idx,
                cases,
                current_statements,
                current_label,
            ) {
                lowered_access
            } else {
                self.emit_nested_suspension(idx, cases, current_statements, current_label);
                self.expression_to_ir(idx)
            };
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::RETURN,
                    value: Some(Box::new(value)),
                    comment: Some("return".to_string().into()),
                },
            ))));
        } else {
            // Non-await expression body: return the expression directly
            let value = self.expression_to_ir(idx);
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::RETURN,
                    value: Some(Box::new(value)),
                    comment: Some("return".to_string().into()),
                },
            ))));
        }
    }

    fn process_async_statement_list(
        &mut self,
        statements: &[NodeIndex],
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        skipped_statements: &[NodeIndex],
    ) {
        let mut index = 0;
        while index < statements.len() {
            let stmt_idx = statements[index];
            if skipped_statements.contains(&stmt_idx) {
                index += 1;
                continue;
            }
            if self.statement_is_using_variable_statement(stmt_idx) {
                self.process_async_disposable_region(
                    &statements[index..],
                    cases,
                    current_statements,
                    current_label,
                    skipped_statements,
                );
                break;
            }
            self.push_preceding_line_comment(stmt_idx, current_statements);
            self.process_async_statement(stmt_idx, cases, current_statements, current_label);
            index += 1;
        }
    }

    fn push_preceding_line_comment(
        &self,
        stmt_idx: NodeIndex,
        current_statements: &mut Vec<IRNode>,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let actual_start =
            super::emit_utils::skip_trivia_forward(self.source_text, stmt_node.pos, stmt_node.end);
        if let Some(comment) = self.extract_preceding_line_comment(actual_start) {
            current_statements.push(IRNode::Raw(comment.into()));
        }
    }

    fn statement_is_using_variable_statement(&self, stmt_idx: NodeIndex) -> bool {
        self.using_variable_statement_flags(stmt_idx)
            .is_some_and(|flags| (flags & node_flags::USING) != 0)
    }

    fn using_variable_statement_flags(&self, stmt_idx: NodeIndex) -> Option<u32> {
        let stmt_node = self.arena.get(stmt_idx)?;
        if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return None;
        }
        let var_stmt = self.arena.get_variable(stmt_node)?;
        var_stmt
            .declarations
            .nodes
            .iter()
            .find_map(|&decl_list_idx| {
                self.arena.get(decl_list_idx).and_then(|decl_list_node| {
                    ((decl_list_node.flags as u32 & node_flags::USING) != 0)
                        .then_some(decl_list_node.flags as u32)
                })
            })
    }

    fn process_async_disposable_region(
        &mut self,
        statements: &[NodeIndex],
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
        skipped_statements: &[NodeIndex],
    ) {
        let (env_name, error_name, result_name) = self.next_disposable_env_names();
        let using_async = self.statement_slice_has_await_using(statements, skipped_statements);
        let using_binding_names = self.collect_using_binding_names(statements, skipped_statements);
        let start_label = self.state.next_label();
        let try_push_placeholder = u32::MAX;

        current_statements.push(IRNode::VarDecl {
            name: env_name.clone().into(),
            initializer: Some(Box::new(self.disposable_env_initializer())),
        });
        for name in using_binding_names {
            current_statements.push(IRNode::VarDecl {
                name: name.into(),
                initializer: None,
            });
        }
        current_statements.push(IRNode::VarDecl {
            name: error_name.clone().into(),
            initializer: None,
        });
        // Only hoist `result_N` when the region awaits disposal. For pure-`using`
        // regions tsc emits `__disposeResources(env_N);` as a plain expression
        // statement and never assigns to `result_N`, so it never declares the
        // variable either.
        if using_async {
            current_statements.push(IRNode::VarDecl {
                name: result_name.clone().into(),
                initializer: None,
            });
        }
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(start_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });
        *current_label = start_label;

        current_statements.push(IRNode::GeneratorTryPush {
            start_label,
            catch_label: try_push_placeholder,
            finally_label: try_push_placeholder,
            end_label: try_push_placeholder,
        });

        for &stmt_idx in statements {
            if skipped_statements.contains(&stmt_idx) {
                continue;
            }
            self.push_preceding_line_comment(stmt_idx, current_statements);
            if self.statement_is_using_variable_statement(stmt_idx) {
                self.process_using_variable_statement_in_region(
                    stmt_idx,
                    &env_name,
                    current_statements,
                );
            } else {
                self.process_async_statement(stmt_idx, cases, current_statements, current_label);
            }
        }

        let catch_label = self.state.next_label();
        let finally_label = self.state.next_label();
        let dispose_resume_label = using_async.then(|| self.state.next_label());
        let dispose_done_label = if using_async {
            self.state.next_label()
        } else {
            finally_label
        };
        let end_label = self.state.next_label();
        Self::patch_generator_try_push(
            cases,
            current_statements,
            start_label,
            catch_label,
            finally_label,
            end_label,
        );

        current_statements.push(Self::generator_break_statement(end_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = catch_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(error_name.clone()),
            IRNode::GeneratorSent,
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::prop(IRNode::id(env_name.clone()), "error"),
            IRNode::id(error_name),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::prop(IRNode::id(env_name.clone()), "hasError"),
            IRNode::BooleanLiteral(true),
        ))));
        current_statements.push(Self::generator_break_statement(end_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = finally_label;
        // For pure-`using` regions tsc emits the dispose call as a bare
        // expression statement; only `await using` regions need the
        // `result_N = __disposeResources(env_N);` capture so the value can be
        // awaited before endfinally.
        let dispose_call = IRNode::CallExpr {
            callee: Box::new(IRNode::RuntimeHelper("__disposeResources".into())),
            arguments: vec![IRNode::id(env_name)],
        };
        if using_async {
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::id(result_name.clone()),
                dispose_call,
            ))));
        } else {
            current_statements.push(IRNode::ExpressionStatement(Box::new(dispose_call)));
        }

        if using_async {
            current_statements.push(IRNode::IfBreak {
                condition: Box::new(IRNode::PrefixUnaryExpr {
                    operator: "!".into(),
                    operand: Box::new(IRNode::id(result_name.clone())),
                }),
                target_label: dispose_done_label,
            });
            let dispose_yield_value = if self.async_generator_mode {
                IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__await".into())),
                    arguments: vec![IRNode::id(result_name)],
                }
            } else {
                IRNode::id(result_name)
            };
            current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                IRNode::GeneratorOp {
                    opcode: opcodes::YIELD,
                    value: Some(Box::new(dispose_yield_value)),
                    comment: Some("yield".into()),
                },
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });

            *current_label =
                dispose_resume_label.expect("async disposable regions reserve a resume label");
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
            current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
                IRNode::GeneratorLabel,
                IRNode::number(dispose_done_label.to_string()),
            ))));
            cases.push(IRGeneratorCase {
                label: *current_label,
                statements: std::mem::take(current_statements),
            });
        }

        *current_label = dispose_done_label;
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::END_FINALLY,
                value: None,
                comment: Some("endfinally".into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });
        *current_label = end_label;
    }

    fn disposable_env_initializer(&self) -> IRNode {
        IRNode::object(vec![
            crate::transforms::ir::IRProperty {
                key: crate::transforms::ir::IRPropertyKey::Identifier("stack".into()),
                value: IRNode::ArrayLiteral(Vec::new()),
                kind: crate::transforms::ir::IRPropertyKind::Init,
            },
            crate::transforms::ir::IRProperty {
                key: crate::transforms::ir::IRPropertyKey::Identifier("error".into()),
                value: IRNode::Undefined,
                kind: crate::transforms::ir::IRPropertyKind::Init,
            },
            crate::transforms::ir::IRProperty {
                key: crate::transforms::ir::IRPropertyKey::Identifier("hasError".into()),
                value: IRNode::BooleanLiteral(false),
                kind: crate::transforms::ir::IRPropertyKind::Init,
            },
        ])
    }

    fn add_disposable_resource_call(
        &self,
        env_name: &str,
        value_name: &str,
        using_async: bool,
    ) -> IRNode {
        IRNode::CallExpr {
            callee: Box::new(IRNode::RuntimeHelper("__addDisposableResource".into())),
            arguments: vec![
                IRNode::id(env_name.to_string()),
                IRNode::id(value_name.to_string()),
                IRNode::BooleanLiteral(using_async),
            ],
        }
    }

    fn generator_break_statement(target_label: u32) -> IRNode {
        IRNode::ReturnStatement(Some(Box::new(IRNode::GeneratorOp {
            opcode: opcodes::BREAK,
            value: Some(Box::new(IRNode::NumericLiteral(
                target_label.to_string().into(),
            ))),
            comment: Some("break".into()),
        })))
    }

    fn patch_generator_try_push(
        cases: &mut [IRGeneratorCase],
        current_statements: &mut [IRNode],
        start_label: u32,
        catch_label: u32,
        finally_label: u32,
        end_label: u32,
    ) {
        for case in cases {
            Self::patch_generator_try_push_in_statements(
                &mut case.statements,
                start_label,
                catch_label,
                finally_label,
                end_label,
            );
        }
        Self::patch_generator_try_push_in_statements(
            current_statements,
            start_label,
            catch_label,
            finally_label,
            end_label,
        );
    }

    fn patch_generator_try_push_in_statements(
        statements: &mut [IRNode],
        start_label: u32,
        catch_label: u32,
        finally_label: u32,
        end_label: u32,
    ) {
        for statement in statements {
            if let IRNode::GeneratorTryPush {
                start_label: candidate_start,
                catch_label: candidate_catch,
                finally_label: candidate_finally,
                end_label: candidate_end,
            } = statement
                && *candidate_start == start_label
                && *candidate_catch == u32::MAX
            {
                *candidate_catch = catch_label;
                *candidate_finally = finally_label;
                *candidate_end = end_label;
            }
        }
    }

    fn statement_slice_has_await_using(
        &self,
        statements: &[NodeIndex],
        skipped_statements: &[NodeIndex],
    ) -> bool {
        statements.iter().copied().any(|stmt_idx| {
            !skipped_statements.contains(&stmt_idx)
                && self
                    .using_variable_statement_flags(stmt_idx)
                    .is_some_and(node_flags::is_await_using)
        })
    }

    fn collect_using_binding_names(
        &self,
        statements: &[NodeIndex],
        skipped_statements: &[NodeIndex],
    ) -> Vec<String> {
        let mut names = Vec::new();
        for &stmt_idx in statements {
            if skipped_statements.contains(&stmt_idx)
                || !self.statement_is_using_variable_statement(stmt_idx)
            {
                continue;
            }
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                continue;
            };
            for &decl_list_idx in &var_stmt.declarations.nodes {
                let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                    continue;
                };
                if (decl_list_node.flags as u32 & node_flags::USING) == 0 {
                    continue;
                }
                let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                    continue;
                };
                for &decl_idx in &decl_list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                        continue;
                    };
                    let name = crate::transforms::emit_utils::identifier_text_or_empty(
                        self.arena, decl.name,
                    );
                    if !name.is_empty() && !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
        names
    }

    fn process_using_variable_statement_in_region(
        &mut self,
        stmt_idx: NodeIndex,
        env_name: &str,
        current_statements: &mut Vec<IRNode>,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return;
        };
        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            if (decl_list_node.flags as u32 & node_flags::USING) == 0 {
                continue;
            }
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };
            for &decl_idx in &decl_list.declarations.nodes {
                self.process_using_variable_declaration_in_region(
                    decl_idx,
                    env_name,
                    node_flags::is_await_using(decl_list_node.flags as u32),
                    current_statements,
                );
            }
        }
    }

    fn process_using_variable_declaration_in_region(
        &mut self,
        decl_idx: NodeIndex,
        env_name: &str,
        using_async: bool,
        current_statements: &mut Vec<IRNode>,
    ) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };
        let name = crate::transforms::emit_utils::identifier_text_or_empty(self.arena, decl.name);
        if name.is_empty() {
            return;
        }
        let value = if decl.initializer.is_none() {
            IRNode::Undefined
        } else if let Some((temp, lowered_init)) =
            self.lower_object_literal_es5_with_computed_properties(decl.initializer)
        {
            current_statements.push(IRNode::HoistedVarGroupBreak);
            current_statements.push(IRNode::VarDecl {
                name: temp.into(),
                initializer: None,
            });
            lowered_init
        } else {
            self.expression_to_ir(decl.initializer)
        };
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(name),
            IRNode::CallExpr {
                callee: Box::new(IRNode::RuntimeHelper("__addDisposableResource".into())),
                arguments: vec![
                    IRNode::id(env_name.to_string()),
                    value,
                    IRNode::BooleanLiteral(using_async),
                ],
            },
        ))));
    }

    fn process_for_of_using_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FOR_OF_STATEMENT {
            return false;
        }
        let Some(for_in_of) = self.arena.get_for_in_of(node) else {
            return false;
        };
        if for_in_of.await_modifier {
            return false;
        }
        let Some(using_info) =
            super::emit_utils::for_of_using_info(self.arena, for_in_of.initializer)
        else {
            return false;
        };

        let env_id = self.disposable_env_counter.get();
        let (env_name, error_name, result_name) = self.next_disposable_env_names();
        let index_name = self.fresh_reserved_name("_i");
        let array_name = self.for_of_iterable_temp_name(for_in_of.expression, env_id);
        let value_temp_name =
            self.fresh_reserved_name(format!("{}_{}", using_info.binding_name, env_id));

        for name in [
            &index_name,
            &array_name,
            &value_temp_name,
            &env_name,
            &using_info.binding_name,
            &error_name,
            &result_name,
        ] {
            current_statements.push(IRNode::VarDecl {
                name: name.to_string().into(),
                initializer: None,
            });
        }

        let iterable = self.for_of_iterable_to_ir_with_es5_computed_temps(
            for_in_of.expression,
            current_statements,
        );
        let loop_label = self.state.next_label();
        let try_start_label = self.state.next_label();
        let catch_label = self.state.next_label();
        let finally_label = self.state.next_label();
        let dispose_resume_label = self.state.next_label();
        let dispose_done_label = self.state.next_label();
        let iteration_label = self.state.next_label();
        let end_label = self.state.next_label();

        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::binary(
            IRNode::assign(IRNode::id(index_name.clone()), IRNode::number("0")),
            ",",
            IRNode::assign(IRNode::id(array_name.clone()), iterable),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(loop_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = loop_label;
        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".into(),
                operand: Box::new(IRNode::Parenthesized(Box::new(IRNode::binary(
                    IRNode::id(index_name.clone()),
                    "<",
                    IRNode::prop(IRNode::id(array_name.clone()), "length"),
                )))),
            }),
            target_label: end_label,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(value_temp_name.clone()),
            IRNode::elem(IRNode::id(array_name), IRNode::id(index_name.clone())),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(env_name.clone()),
            self.disposable_env_initializer(),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(try_start_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = try_start_label;
        current_statements.push(IRNode::GeneratorTryPush {
            start_label: try_start_label,
            catch_label,
            finally_label,
            end_label: iteration_label,
        });
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(using_info.binding_name),
            IRNode::CallExpr {
                callee: Box::new(IRNode::RuntimeHelper("__addDisposableResource".into())),
                arguments: vec![
                    IRNode::id(env_name.clone()),
                    IRNode::id(value_temp_name),
                    IRNode::BooleanLiteral(using_info.using_async),
                ],
            },
        ))));
        self.process_block_or_statement_in_async(
            for_in_of.statement,
            cases,
            current_statements,
            current_label,
        );
        current_statements.push(Self::generator_break_statement(iteration_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = catch_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(error_name.clone()),
            IRNode::GeneratorSent,
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::prop(IRNode::id(env_name.clone()), "error"),
            IRNode::id(error_name),
        ))));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::prop(IRNode::id(env_name.clone()), "hasError"),
            IRNode::BooleanLiteral(true),
        ))));
        current_statements.push(Self::generator_break_statement(iteration_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = finally_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::id(result_name.clone()),
            IRNode::CallExpr {
                callee: Box::new(IRNode::RuntimeHelper("__disposeResources".into())),
                arguments: vec![IRNode::id(env_name)],
            },
        ))));
        current_statements.push(IRNode::IfBreak {
            condition: Box::new(IRNode::PrefixUnaryExpr {
                operator: "!".into(),
                operand: Box::new(IRNode::id(result_name.clone())),
            }),
            target_label: dispose_done_label,
        });
        let dispose_yield_value = if self.async_generator_mode {
            IRNode::CallExpr {
                callee: Box::new(IRNode::RuntimeHelper("__await".into())),
                arguments: vec![IRNode::id(result_name)],
            }
        } else {
            IRNode::id(result_name)
        };
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::YIELD,
                value: Some(Box::new(dispose_yield_value)),
                comment: Some("yield".into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = dispose_resume_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::GeneratorSent)));
        current_statements.push(IRNode::ExpressionStatement(Box::new(IRNode::assign(
            IRNode::GeneratorLabel,
            IRNode::number(dispose_done_label.to_string()),
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = dispose_done_label;
        current_statements.push(IRNode::ReturnStatement(Some(Box::new(
            IRNode::GeneratorOp {
                opcode: opcodes::END_FINALLY,
                value: None,
                comment: Some("endfinally".into()),
            },
        ))));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = iteration_label;
        current_statements.push(IRNode::ExpressionStatement(Box::new(
            IRNode::PostfixUnaryExpr {
                operand: Box::new(IRNode::id(index_name)),
                operator: "++".into(),
            },
        )));
        current_statements.push(Self::generator_break_statement(loop_label));
        cases.push(IRGeneratorCase {
            label: *current_label,
            statements: std::mem::take(current_statements),
        });

        *current_label = end_label;
        true
    }
}
