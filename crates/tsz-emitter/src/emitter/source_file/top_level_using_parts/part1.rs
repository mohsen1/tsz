impl<'a> Printer<'a> {
    pub(in crate::emitter) fn statement_is_top_level_using(&self, node: &Node) -> bool {
        if node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return false;
        }

        self.arena.get_variable(node).is_some_and(|var_stmt| {
            var_stmt.declarations.nodes.iter().any(|&decl_list_idx| {
                self.arena.get(decl_list_idx).is_some_and(|decl_list_node| {
                    let flags = decl_list_node.flags as u32;
                    (flags & tsz_parser::parser::node_flags::USING) != 0
                        || tsz_parser::parser::node_flags::is_await_using(flags)
                })
            })
        })
    }

    pub(in crate::emitter) fn count_es5_resource_expression_hoisted_temps(
        &self,
        idx: NodeIndex,
    ) -> usize {
        if idx.is_none() {
            return 0;
        }
        let Some(node) = self.arena.get(idx) else {
            return 0;
        };

        match node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.count_es5_resource_object_literal_hoisted_temps(node)
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::METHOD_DECLARATION
                || k == syntax_kind_ext::CONSTRUCTOR
                || k == syntax_kind_ext::GET_ACCESSOR
                || k == syntax_kind_ext::SET_ACCESSOR =>
            {
                0
            }
            _ => self
                .arena
                .get_children(idx)
                .into_iter()
                .map(|child_idx| self.count_es5_resource_expression_hoisted_temps(child_idx))
                .sum(),
        }
    }

    fn count_es5_resource_object_literal_hoisted_temps(&self, node: &Node) -> usize {
        let Some(literal) = self.arena.get_literal_expr(node) else {
            return 0;
        };

        let elements = &literal.elements.nodes;
        let mut count = self.count_es5_object_literal_lowering_temp_slots(elements);
        for &element_idx in elements {
            count += self.count_es5_resource_object_element_nested_temps(element_idx);
        }
        count
    }

    fn count_es5_object_literal_lowering_temp_slots(&self, elements: &[NodeIndex]) -> usize {
        if elements.is_empty() {
            return 0;
        }

        let has_spread = elements
            .iter()
            .copied()
            .any(|idx| emit_utils::is_spread_element(self.arena, idx));
        if !has_spread {
            return usize::from(
                elements
                    .iter()
                    .copied()
                    .any(|idx| emit_utils::is_computed_property_member(self.arena, idx)),
            );
        }

        let mut count = 0usize;
        let mut segment_has_computed = false;
        for &element_idx in elements {
            if emit_utils::is_spread_element(self.arena, element_idx) {
                if segment_has_computed {
                    count += 1;
                    segment_has_computed = false;
                }
            } else if emit_utils::is_computed_property_member(self.arena, element_idx) {
                segment_has_computed = true;
            }
        }
        if segment_has_computed {
            count += 1;
        }
        count
    }

    fn count_es5_resource_object_element_nested_temps(&self, element_idx: NodeIndex) -> usize {
        let Some(element_node) = self.arena.get(element_idx) else {
            return 0;
        };

        match element_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let Some(prop) = self.arena.get_property_assignment(element_node) else {
                    return 0;
                };
                self.count_computed_property_name_expression_temps(prop.name)
                    + self.count_es5_resource_expression_hoisted_temps(prop.initializer)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let Some(method) = self.arena.get_method_decl(element_node) else {
                    return 0;
                };
                self.count_computed_property_name_expression_temps(method.name)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                let Some(accessor) = self.arena.get_accessor(element_node) else {
                    return 0;
                };
                self.count_computed_property_name_expression_temps(accessor.name)
            }
            k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                self.arena.get_spread(element_node).map_or(0, |spread| {
                    self.count_es5_resource_expression_hoisted_temps(spread.expression)
                })
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT => self
                .arena
                .unary_exprs_ex
                .get(element_node.data_index as usize)
                .map_or(0, |spread| {
                    self.count_es5_resource_expression_hoisted_temps(spread.expression)
                }),
            _ => 0,
        }
    }

    fn count_computed_property_name_expression_temps(&self, name_idx: NodeIndex) -> usize {
        let Some(name_node) = self.arena.get(name_idx) else {
            return 0;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return 0;
        }
        self.arena
            .get_computed_property(name_node)
            .map_or(0, |name| {
                self.count_es5_resource_expression_hoisted_temps(name.expression)
            })
    }

    fn reserve_top_level_using_env_names(
        &mut self,
        env_start_id: u32,
        outer_error_id: u32,
        block_indices: &[NodeIndex],
    ) -> (String, String, String) {
        let outer_names = (
            format!("env_{env_start_id}"),
            format!("e_{outer_error_id}"),
            format!("result_{outer_error_id}"),
        );
        self.generated_temp_names.insert(outer_names.0.clone());
        self.generated_temp_names.insert(outer_names.1.clone());
        self.generated_temp_names.insert(outer_names.2.clone());

        for (offset, &block_idx) in block_indices.iter().enumerate() {
            let error_id = env_start_id + offset as u32;
            let env_id = env_start_id + 1 + offset as u32;
            let names = (
                format!("env_{env_id}"),
                format!("e_{error_id}"),
                format!("result_{error_id}"),
            );
            self.generated_temp_names.insert(names.0.clone());
            self.generated_temp_names.insert(names.1.clone());
            self.generated_temp_names.insert(names.2.clone());
            self.reserved_disposable_env_names.insert(block_idx, names);
        }

        self.next_disposable_env_id = outer_error_id + 1;
        outer_names
    }

    pub(in crate::emitter) fn emit_top_level_using_scope(
        &mut self,
        statements: &NodeList,
        start_idx: usize,
        is_es_module_output: bool,
        cjs_deferred_export_names: &FxHashSet<String>,
    ) {
        let using_async = statements.nodes[start_idx..].iter().any(|&stmt_idx| {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                return false;
            };
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                return false;
            }
            let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                return false;
            };
            var_stmt.declarations.nodes.iter().any(|&decl_list_idx| {
                self.arena.get(decl_list_idx).is_some_and(|decl_list_node| {
                    tsz_parser::parser::node_flags::is_await_using(decl_list_node.flags as u32)
                })
            })
        });
        let reserved_blocks = self.collect_top_level_using_block_envs(statements, start_idx);
        let env_start_id = self.next_disposable_env_id;
        let outer_error_id = env_start_id + reserved_blocks.len() as u32;
        let (env_name, error_name, result_name) =
            self.reserve_top_level_using_env_names(env_start_id, outer_error_id, &reserved_blocks);
        let resource_temp_count =
            self.count_top_level_using_es5_resource_initializer_temps(statements, start_idx);
        if resource_temp_count > 0 {
            self.preallocate_hoisted_temp_names(resource_temp_count);
        }
        self.reserve_top_level_using_deferred_static_class_result_temps(statements, start_idx);
        let env_decl_keyword = if self.ctx.target_es5 { "var" } else { "const" };

        if is_es_module_output {
            self.emit_top_level_using_pre_named_exports(statements, start_idx);
        }
        let hoisted_function_indices = self.emit_top_level_using_hoists(
            statements,
            start_idx,
            is_es_module_output,
            cjs_deferred_export_names,
        );
        self.write(env_decl_keyword);
        self.write(" ");
        self.write(&env_name);
        self.write(" = { stack: [], error: void 0, hasError: false };");
        self.write_line();
        self.write("try {");
        self.write_line();
        self.increase_indent();

        let cjs_deferred_export_bindings = if is_es_module_output {
            None
        } else {
            Some(self.collect_cjs_deferred_export_bindings(statements))
        };
        let cjs_deferred_export_bindings_all = if is_es_module_output {
            None
        } else {
            Some(self.collect_cjs_deferred_export_bindings_all(statements))
        };
        let prev_deferred_local_export_bindings = if is_es_module_output {
            None
        } else {
            self.deferred_local_export_bindings
                .replace(cjs_deferred_export_bindings.unwrap_or_default())
        };
        let prev_deferred_local_export_bindings_all = if is_es_module_output {
            None
        } else {
            self.deferred_local_export_bindings_all
                .replace(cjs_deferred_export_bindings_all.unwrap_or_default())
        };
        let prev_block_using_env = self
            .block_using_env
            .replace((env_name.clone(), using_async));
        let prev_in_top_level_using_scope = self.in_top_level_using_scope;
        self.in_top_level_using_scope = true;
        for &stmt_idx in &statements.nodes[start_idx..] {
            if hoisted_function_indices.contains(&stmt_idx) {
                continue;
            }
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if self.is_erased_statement(stmt_node) {
                continue;
            }
            if self.emit_top_level_using_statement(
                stmt_node,
                stmt_idx,
                is_es_module_output,
                cjs_deferred_export_names,
            ) && !self.writer.is_at_line_start()
            {
                self.write_line();
            }
        }
        self.in_top_level_using_scope = prev_in_top_level_using_scope;
        self.block_using_env = prev_block_using_env;
        if !is_es_module_output {
            self.deferred_local_export_bindings = prev_deferred_local_export_bindings;
            self.deferred_local_export_bindings_all = prev_deferred_local_export_bindings_all;
        }

        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.write("catch (");
        self.write(&error_name);
        self.write(") {");
        self.write_line();
        self.increase_indent();
        self.write(&env_name);
        self.write(".error = ");
        self.write(&error_name);
        self.write(";");
        self.write_line();
        self.write(&env_name);
        self.write(".hasError = true;");
        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.write("finally {");
        self.write_line();
        self.increase_indent();
        if using_async {
            let await_kw = if self.ctx.emit_await_as_yield {
                "yield"
            } else {
                "await"
            };
            self.write(env_decl_keyword);
            self.write(" ");
            self.write(&result_name);
            self.write(" = ");
            self.write_helper("__disposeResources");
            self.write("(");
            self.write(&env_name);
            self.write(");");
            self.write_line();
            self.write("if (");
            self.write(&result_name);
            self.write(")");
            self.write_line();
            self.increase_indent();
            self.write(await_kw);
            self.write(" ");
            self.write(&result_name);
            self.write(";");
            self.write_line();
            self.decrease_indent();
        } else {
            self.write_helper("__disposeResources");
            self.write("(");
            self.write(&env_name);
            self.write(");");
            self.write_line();
        }
        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.reserved_top_level_using_class_result_temps.clear();

        if !is_es_module_output
            && statements.nodes[start_idx..].iter().any(|&stmt_idx| {
                self.arena.get(stmt_idx).is_some_and(|stmt_node| {
                    stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                        && self
                            .arena
                            .get_export_assignment(stmt_node)
                            .is_some_and(|export_assignment| export_assignment.is_export_equals)
                })
            })
        {
            if matches!(
                self.ctx.original_module_kind,
                Some(ModuleKind::AMD) | Some(ModuleKind::UMD)
            ) {
                self.write("return _default;");
            } else {
                self.write("module.exports = _default;");
            }
            self.write_line();
        }
    }

    pub(in crate::emitter) fn has_pre_top_level_using_named_exports(
        &self,
        statements: &NodeList,
        end_idx: usize,
    ) -> bool {
        statements.nodes[..end_idx].iter().any(|&stmt_idx| {
            self.arena.get(stmt_idx).is_some_and(|stmt_node| {
                stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    && self
                        .arena
                        .get_export_decl(stmt_node)
                        .is_some_and(|export_decl| {
                            !export_decl.is_type_only
                                && export_decl.module_specifier.is_none()
                                && !export_decl.is_default_export
                                && self.arena.get(export_decl.export_clause).is_some_and(
                                    |clause_node| {
                                        clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
                                            && self
                                                .arena
                                                .get_named_imports(clause_node)
                                                .is_some_and(|named_exports| {
                                                    !named_exports.elements.nodes.is_empty()
                                                })
                                    },
                                )
                        })
            })
        })
    }

    pub(in crate::emitter) fn top_level_using_scope_has_runtime_export(
        &self,
        statements: &NodeList,
        start_idx: usize,
    ) -> bool {
        statements.nodes[start_idx..].iter().any(|&stmt_idx| {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                return false;
            };
            match stmt_node.kind {
                k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => self
                    .arena
                    .get_export_assignment(stmt_node)
                    .is_some_and(|export_assignment| !export_assignment.is_export_equals),
                k if k == syntax_kind_ext::EXPORT_DECLARATION => self
                    .arena
                    .get_export_decl(stmt_node)
                    .is_some_and(|export_decl| {
                        if export_decl.is_type_only {
                            return false;
                        }
                        if export_decl.module_specifier.is_some() {
                            return true;
                        }
                        self.arena
                            .get(export_decl.export_clause)
                            .is_some_and(|clause_node| match clause_node.kind {
                                k if k == syntax_kind_ext::NAMED_EXPORTS => self
                                    .arena
                                    .get_named_imports(clause_node)
                                    .is_some_and(|named_exports| {
                                        named_exports.elements.nodes.iter().any(|&spec_idx| {
                                            self.arena
                                                .get(spec_idx)
                                                .and_then(|spec_node| {
                                                    self.arena.get_specifier(spec_node)
                                                })
                                                .is_some_and(|spec| !spec.is_type_only)
                                        })
                                    }),
                                _ => true,
                            })
                    }),
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    self.arena.get_variable(stmt_node).is_some_and(|var_stmt| {
                        self.arena
                            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
                    })
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    self.arena.get_class(stmt_node).is_some_and(|class_decl| {
                        self.arena
                            .has_modifier(&class_decl.modifiers, SyntaxKind::ExportKeyword)
                    })
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    self.arena.get_function(stmt_node).is_some_and(|func_decl| {
                        self.arena
                            .has_modifier(&func_decl.modifiers, SyntaxKind::ExportKeyword)
                    })
                }
                _ => false,
            }
        })
    }

    pub(in crate::emitter) fn has_aliased_value_named_exports(&self, clause_node: &Node) -> bool {
        let Some(named_exports) = self.arena.get_named_imports(clause_node) else {
            return false;
        };
        self.collect_value_specifiers(&named_exports.elements)
            .iter()
            .any(|&spec_idx| {
                self.arena
                    .get(spec_idx)
                    .and_then(|spec_node| self.arena.get_specifier(spec_node))
                    .is_some_and(|spec| spec.property_name.is_some())
            })
    }

    pub(in crate::emitter) fn named_exports_have_prior_runtime_declaration(
        &self,
        statements: &NodeList,
        end_idx: usize,
        clause_node: &Node,
    ) -> bool {
        let Some(named_exports) = self.arena.get_named_imports(clause_node) else {
            return false;
        };
        self.collect_value_specifiers(&named_exports.elements)
            .iter()
            .filter_map(|&spec_idx| {
                let spec_node = self.arena.get(spec_idx)?;
                let spec = self.arena.get_specifier(spec_node)?;
                let local_name = if spec.property_name.is_some() {
                    self.get_specifier_name_text(spec.property_name)
                } else {
                    self.get_specifier_name_text(spec.name)
                }?;
                Some(local_name)
            })
            .any(|local_name| {
                statements.nodes[..end_idx].iter().any(|&stmt_idx| {
                    self.arena.get(stmt_idx).is_some_and(|stmt_node| {
                        self.statement_declares_runtime_name(stmt_node, &local_name)
                    })
                })
            })
    }

    fn statement_declares_runtime_name(&self, stmt_node: &Node, name: &str) -> bool {
        match stmt_node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.arena.get_variable(stmt_node).is_some_and(|var_stmt| {
                    var_stmt.declarations.nodes.iter().any(|&decl_list_idx| {
                        self.arena.get(decl_list_idx).is_some_and(|decl_list_node| {
                            self.arena
                                .get_variable(decl_list_node)
                                .is_some_and(|decl_list| {
                                    decl_list.declarations.nodes.iter().any(|&decl_idx| {
                                        self.arena.get(decl_idx).is_some_and(|decl_node| {
                                            self.arena
                                                .get_variable_declaration(decl_node)
                                                .is_some_and(|decl| {
                                                    self.get_identifier_text_idx(decl.name) == name
                                                })
                                        })
                                    })
                                })
                        })
                    })
                })
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => self
                .arena
                .get_class(stmt_node)
                .and_then(|class| self.get_identifier_text_opt(class.name))
                .is_some_and(|class_name| class_name == name),
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                .arena
                .get_function(stmt_node)
                .and_then(|func| self.get_identifier_text_opt(func.name))
                .is_some_and(|func_name| func_name == name),
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                self.arena.get_export_decl(stmt_node).is_some_and(|export| {
                    if export.is_type_only || export.module_specifier.is_some() {
                        return false;
                    }
                    self.arena
                        .get(export.export_clause)
                        .is_some_and(|clause_node| {
                            self.statement_declares_runtime_name(clause_node, name)
                        })
                })
            }
            _ => false,
        }
    }

    fn emit_top_level_using_pre_named_exports(&mut self, statements: &NodeList, end_idx: usize) {
        for &stmt_idx in &statements.nodes[..end_idx] {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export_decl.is_type_only
                || export_decl.module_specifier.is_some()
                || export_decl.is_default_export
            {
                continue;
            }
            let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
                continue;
            };
            if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                continue;
            }
            let Some(named_exports) = self.arena.get_named_imports(clause_node) else {
                continue;
            };
            if named_exports.elements.nodes.is_empty() {
                continue;
            }
            self.emit_top_level_using_named_export_clause(clause_node);
            self.write_line();
        }
    }

    fn emit_top_level_using_hoists(
        &mut self,
        statements: &NodeList,
        start_idx: usize,
        is_es_module_output: bool,
        cjs_deferred_export_names: &FxHashSet<String>,
    ) -> FxHashSet<NodeIndex> {
        let mut local_names = Vec::new();
        let mut seen_local = FxHashSet::default();
        let mut export_let_names = Vec::new();
        let mut seen_export_let = FxHashSet::default();
        let mut export_named_bindings = Vec::new();
        let mut hoisted_function_indices = Vec::new();
        let mut hoisted_function_index_set = FxHashSet::default();

        for &stmt_idx in &statements.nodes[start_idx..] {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            match stmt_node.kind {
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    self.collect_top_level_using_variable_hoists(
                        stmt_node,
                        is_es_module_output,
                        cjs_deferred_export_names,
                        &mut local_names,
                        &mut seen_local,
                        &mut export_let_names,
                        &mut seen_export_let,
                    );
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_DECLARATION =>
                {
                    if stmt_node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                        if hoisted_function_index_set.insert(stmt_idx) {
                            hoisted_function_indices.push(stmt_idx);
                        }
                    } else {
                        self.collect_top_level_using_named_decl_hoist(
                            stmt_node,
                            false,
                            false,
                            is_es_module_output,
                            &mut local_names,
                            &mut seen_local,
                            &mut export_named_bindings,
                        );
                    }
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    self.collect_top_level_using_namespace_hoist(
                        stmt_node,
                        &mut local_names,
                        &mut seen_local,
                    );
                }
                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                    let Some(export) = self.arena.get_export_decl(stmt_node) else {
                        continue;
                    };
                    if export.is_type_only || export.module_specifier.is_some() {
                        continue;
                    }
                    let Some(clause_node) = self.arena.get(export.export_clause) else {
                        continue;
                    };
                    match clause_node.kind {
                        k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                            self.collect_top_level_using_variable_hoists(
                                clause_node,
                                is_es_module_output,
                                cjs_deferred_export_names,
                                &mut local_names,
                                &mut seen_local,
                                &mut export_let_names,
                                &mut seen_export_let,
                            );
                        }
                        k if k == syntax_kind_ext::CLASS_DECLARATION
                            || k == syntax_kind_ext::FUNCTION_DECLARATION =>
                        {
                            if clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                                && hoisted_function_index_set.insert(stmt_idx)
                            {
                                hoisted_function_indices.push(stmt_idx);
                            }
                            self.collect_top_level_using_named_decl_hoist(
                                clause_node,
                                true,
                                export.is_default_export,
                                is_es_module_output,
                                &mut local_names,
                                &mut seen_local,
                                &mut export_named_bindings,
                            );
                        }
                        k if k == syntax_kind_ext::MODULE_DECLARATION => {
                            self.collect_top_level_using_namespace_hoist(
                                clause_node,
                                &mut local_names,
                                &mut seen_local,
                            );
                        }
                        _ if export.is_default_export => {
                            if seen_local.insert("_default".to_string()) {
                                local_names.push("_default".to_string());
                            }
                            if is_es_module_output {
                                export_named_bindings
                                    .push("export { _default as default };".to_string());
                            }
                        }
                        k if k == syntax_kind_ext::NAMED_EXPORTS && is_es_module_output => {
                            let before_len = self.writer.len();
                            self.emit_top_level_using_named_export_clause(clause_node);
                            if self.writer.len() > before_len && !self.writer.is_at_line_start() {
                                self.write_line();
                            }
                        }
                        _ => {}
                    }
                }
                k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    let Some(export_assignment) = self.arena.get_export_assignment(stmt_node)
                    else {
                        continue;
                    };
                    if export_assignment.is_export_equals {
                        if !is_es_module_output && seen_local.insert("_default".to_string()) {
                            local_names.push("_default".to_string());
                        }
                        continue;
                    }
                    if seen_local.insert("_default".to_string()) {
                        local_names.push("_default".to_string());
                    }
                    if is_es_module_output {
                        export_named_bindings.push("export { _default as default };".to_string());
                    }
                }
                _ => {}
            }
        }

        if is_es_module_output {
            for binding in export_named_bindings {
                self.write(&binding);
                self.write_line();
            }
        }
        for &stmt_idx in &hoisted_function_indices {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            self.emit_function_declaration(stmt_node, stmt_idx);
            if !self.writer.is_at_line_start() {
                self.write_line();
            }
        }
        if !local_names.is_empty() {
            self.write("var ");
            self.write(&local_names.join(", "));
            self.write(";");
            self.write_line();
        }
        if is_es_module_output && !export_let_names.is_empty() {
            self.write("export let ");
            self.write(&export_let_names.join(", "));
            self.write(";");
            self.write_line();
        }

        hoisted_function_index_set
    }

    fn collect_top_level_using_namespace_hoist(
        &mut self,
        node: &Node,
        local_names: &mut Vec<String>,
        seen_local: &mut FxHashSet<String>,
    ) {
        let Some(module) = self.arena.get_module(node) else {
            return;
        };
        if self.arena.is_declare(&module.modifiers) || !self.is_instantiated_module(module.body) {
            return;
        }
        let name = self.get_identifier_text_idx(module.name);
        if name.is_empty() {
            return;
        }
        if seen_local.insert(name.clone()) {
            local_names.push(name.clone());
        }
        self.declared_namespace_names.insert(name);
    }

    fn collect_top_level_using_variable_hoists(
        &self,
        node: &Node,
        is_es_module_output: bool,
        cjs_deferred_export_names: &FxHashSet<String>,
        local_names: &mut Vec<String>,
        seen_local: &mut FxHashSet<String>,
        export_let_names: &mut Vec<String>,
        seen_export_let: &mut FxHashSet<String>,
    ) {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return;
        };
        let is_exported = self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword);
        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };
            let flags = decl_list_node.flags as u32;
            let is_using = (flags & tsz_parser::parser::node_flags::USING) != 0;
            let mut names = Vec::new();
            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                self.collect_binding_names(decl.name, &mut names);
            }
            for name in names {
                if is_exported && is_es_module_output && !is_using {
                    if seen_export_let.insert(name.clone()) {
                        export_let_names.push(name);
                    }
                } else if (!is_exported || is_using || cjs_deferred_export_names.contains(&name))
                    && seen_local.insert(name.clone())
                {
                    local_names.push(name);
                }
            }
        }
    }

    fn collect_top_level_using_named_decl_hoist(
        &self,
        node: &Node,
        is_exported: bool,
        is_default_export: bool,
        is_es_module_output: bool,
        local_names: &mut Vec<String>,
        seen_local: &mut FxHashSet<String>,
        export_named_bindings: &mut Vec<String>,
    ) {
        let (name, uses_lowered_default_tracker) = match node.kind {
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                let Some(class) = self.arena.get_class(node) else {
                    return;
                };
                let has_class_decorators =
                    !self.collect_class_decorators(&class.modifiers).is_empty();
                let name = self.get_identifier_text_opt(class.name).or_else(|| {
                    if is_default_export {
                        Some(
                            self.anonymous_default_export_name
                                .clone()
                                .unwrap_or_else(|| "default_1".to_string()),
                        )
                    } else {
                        None
                    }
                });
                (
                    name,
                    is_default_export
                        && has_class_decorators
                        && !self.ctx.options.target.supports_es2025(),
                )
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                .arena
                .get_function(node)
                .and_then(|func| self.get_identifier_text_opt(func.name))
                .map(|name| (Some(name), false))
                .unwrap_or((None, false)),
            _ => (None, false),
        };
        let Some(name) = name else {
            return;
        };
        let skip_lowered_default_temp = uses_lowered_default_tracker
            && !self.ctx.target_es5
            && !self.ctx.options.legacy_decorators
            && name == "default_1";
        if !skip_lowered_default_temp && seen_local.insert(name.clone()) {
            local_names.push(name.clone());
        }
        if uses_lowered_default_tracker {
            if seen_local.insert("_default".to_string()) {
                local_names.push("_default".to_string());
            }
            if is_es_module_output {
                export_named_bindings.push("export { _default as default };".to_string());
            }
        } else if is_exported && is_es_module_output {
            if is_default_export {
                export_named_bindings.push(format!("export {{ {name} as default }};"));
            } else {
                export_named_bindings.push(format!("export {{ {name} }};"));
            }
        }
    }

    fn emit_top_level_using_statement(
        &mut self,
        stmt_node: &Node,
        stmt_idx: NodeIndex,
        is_es_module_output: bool,
        cjs_deferred_export_names: &FxHashSet<String>,
    ) -> bool {
        match stmt_node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => self
                .emit_top_level_using_variable_statement(
                    stmt_node,
                    is_es_module_output,
                    cjs_deferred_export_names,
                ),
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                let export_name = if is_es_module_output {
                    None
                } else {
                    self.arena
                        .get_class(stmt_node)
                        .and_then(|class| self.get_identifier_text_opt(class.name))
                        .filter(|name| cjs_deferred_export_names.contains(name))
                        .map(|name| {
                            self.deferred_local_export_bindings
                                .as_ref()
                                .and_then(|bindings| bindings.get(&name))
                                .cloned()
                                .unwrap_or(name)
                        })
                };
                self.emit_top_level_using_class_assignment(
                    stmt_node,
                    stmt_idx,
                    export_name,
                    false,
                    is_es_module_output,
                )
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                let export_name = if is_es_module_output {
                    None
                } else {
                    self.arena
                        .get_function(stmt_node)
                        .and_then(|func| self.get_identifier_text_opt(func.name))
                        .filter(|name| cjs_deferred_export_names.contains(name))
                };
                self.emit_top_level_using_function_assignment(stmt_node, stmt_idx, export_name)
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                let Some(export) = self.arena.get_export_decl(stmt_node) else {
                    return false;
                };
                if export.is_type_only || export.module_specifier.is_some() {
                    return false;
                }
                let Some(clause_node) = self.arena.get(export.export_clause) else {
                    return false;
                };
                match clause_node.kind {
                    k if k == syntax_kind_ext::VARIABLE_STATEMENT => self
                        .emit_top_level_using_variable_statement(
                            clause_node,
                            is_es_module_output,
                            cjs_deferred_export_names,
                        ),
                    k if k == syntax_kind_ext::CLASS_DECLARATION => self
                        .emit_top_level_using_class_assignment(
                            clause_node,
                            export.export_clause,
                            if export.is_default_export {
                                Some("default".to_string())
                            } else if is_es_module_output {
                                None
                            } else {
                                self.arena
                                    .get_class(clause_node)
                                    .and_then(|class| self.get_identifier_text_opt(class.name))
                            },
                            !export.is_default_export,
                            is_es_module_output,
                        ),
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                        .emit_top_level_using_function_assignment(
                            clause_node,
                            export.export_clause,
                            if is_es_module_output {
                                None
                            } else if export.is_default_export {
                                Some("default".to_string())
                            } else {
                                self.arena
                                    .get_function(clause_node)
                                    .and_then(|func| self.get_identifier_text_opt(func.name))
                            },
                        ),
                    _ if export.is_default_export => {
                        if !is_es_module_output {
                            self.write_export_binding_start("default");
                        }
                        self.write("_default = ");
                        self.emit(export.export_clause);
                        if !is_es_module_output {
                            self.write_export_binding_end();
                        } else {
                            self.write(";");
                        }
                        true
                    }
                    k if k == syntax_kind_ext::NAMED_EXPORTS && !is_es_module_output => {
                        let Some(named_exports) = self.arena.get_named_imports(clause_node) else {
                            return false;
                        };
                        let value_specs = self.collect_value_specifiers(&named_exports.elements);
                        let mut emitted_any = false;

                        for &spec_idx in &value_specs {
                            let Some(spec_node) = self.arena.get(spec_idx) else {
                                continue;
                            };
                            let Some(spec) = self.arena.get_specifier(spec_node) else {
                                continue;
                            };
                            if spec.property_name.is_none() {
                                continue;
                            }

                            let Some(export_name) = self.get_specifier_name_text(spec.name) else {
                                continue;
                            };
                            let local_name = self
                                .get_specifier_name_text(spec.property_name)
                                .unwrap_or_else(|| export_name.clone());

                            if self.ctx.module_state.hoisted_func_exports.iter().any(
                                |(exported, local)| {
                                    exported == &export_name && local == &local_name
                                },
                            ) {
                                continue;
                            }

                            if self
                                .ctx
                                .module_state
                                .iife_exported_names
                                .contains(&local_name)
                                || self
                                    .ctx
                                    .module_state
                                    .inline_exported_names
                                    .contains(&export_name)
                            {
                                continue;
                            }

                            if emitted_any {
                                self.write_line();
                            }
                            self.write_export_binding_start(&export_name);
                            self.write(&local_name);
                            self.write_export_binding_end();
                            self.ctx
                                .module_state
                                .inline_exported_names
                                .insert(export_name);
                            emitted_any = true;
                        }

                        emitted_any
                    }
                    _ => false,
                }
            }
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                let Some(export_assignment) = self.arena.get_export_assignment(stmt_node) else {
                    return false;
                };
                if export_assignment.is_export_equals {
                    if is_es_module_output {
                        return false;
                    }
                    self.write("_default = ");
                    self.emit(export_assignment.expression);
                    self.write(";");
                    return true;
                }
                if !is_es_module_output {
                    self.write_export_binding_start("default");
                }
                self.write("_default = ");
                self.emit(export_assignment.expression);
                if !is_es_module_output {
                    self.write_export_binding_end();
                } else {
                    self.write(";");
                }
                true
            }
            _ => {
                self.emit(stmt_idx);
                true
            }
        }
    }

    fn emit_top_level_using_variable_statement(
        &mut self,
        node: &Node,
        is_es_module_output: bool,
        cjs_deferred_export_names: &FxHashSet<String>,
    ) -> bool {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return false;
        };
        let is_exported = self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword);
        let mut emitted = false;

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };
            let flags = decl_list_node.flags as u32;
            let is_using = (flags & tsz_parser::parser::node_flags::USING) != 0;
            let using_async = tsz_parser::parser::node_flags::is_await_using(flags);

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                let Some(name_node) = self.arena.get(decl.name) else {
                    continue;
                };
                if name_node.kind != SyntaxKind::Identifier as u16 {
                    continue;
                }
                let name = self.get_identifier_text_idx(decl.name);
                if name.is_empty() {
                    continue;
                }

                if emitted {
                    self.write_line();
                }

                if is_using {
                    let env_name = self
                        .block_using_env
                        .as_ref()
                        .map(|(env_name, _)| env_name.clone())
                        .unwrap_or_default();
                    self.write(&name);
                    self.write(" = ");
                    self.write_helper("__addDisposableResource");
                    self.write("(");
                    self.write(&env_name);
                    self.write(", ");
                    if decl.initializer.is_some() {
                        self.emit_top_level_using_initializer(decl.initializer, &name);
                    } else {
                        self.write("void 0");
                    }
                    self.write(", ");
                    self.write(if using_async { "true" } else { "false" });
                    self.write(");");
                } else if is_exported && !is_es_module_output {
                    self.write_export_binding_start(&name);
                    if cjs_deferred_export_names.contains(&name) {
                        self.write(&name);
                        self.write(" = ");
                    }
                    if decl.initializer.is_some() {
                        self.emit(decl.initializer);
                    } else {
                        self.write("void 0");
                    }
                    self.write_export_binding_end();
                } else if !is_exported && cjs_deferred_export_names.contains(&name) {
                    let export_name = self
                        .deferred_local_export_bindings
                        .as_ref()
                        .and_then(|bindings| bindings.get(&name))
                        .cloned()
                        .unwrap_or_else(|| name.clone());
                    self.write_export_binding_start(&export_name);
                    self.write(&name);
                    self.write(" = ");
                    if decl.initializer.is_some() {
                        self.emit(decl.initializer);
                    } else {
                        self.write("void 0");
                    }
                    self.write_export_binding_end();
                } else if decl.initializer.is_some() {
                    self.write(&name);
                    self.write(" = ");
                    self.emit(decl.initializer);
                    self.write(";");
                }
                emitted = true;
            }
        }

        emitted
    }

    fn emit_top_level_using_named_export_clause(&mut self, clause_node: &Node) {
        let Some(named_exports) = self.arena.get_named_imports(clause_node) else {
            return;
        };
        let value_specs = self.collect_value_specifiers(&named_exports.elements);
        if value_specs.is_empty() {
            return;
        }
        self.write("export {");
        let mut first = true;
        for &spec_idx in &value_specs {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                continue;
            };
            let Some(spec) = self.arena.get_specifier(spec_node) else {
                continue;
            };
            let Some(export_name) = self.get_specifier_name_text(spec.name) else {
                continue;
            };
            let local_name = if spec.property_name.is_some() {
                self.get_specifier_name_text(spec.property_name)
                    .unwrap_or_else(|| export_name.clone())
            } else {
                export_name.clone()
            };
            if !first {
                self.write(", ");
            } else {
                self.write(" ");
            }
            self.write(&local_name);
            if local_name != export_name {
                self.write(" as ");
                self.write(&export_name);
            }
            first = false;
        }
        if !first {
            self.write(" ");
        }
        self.write("};");
    }

    fn top_level_using_export_binding_stmt(&self, export_name: &str, local_name: &str) -> String {
        if self.in_system_execute_body {
            format!("exports_1(\"{export_name}\", {local_name});")
        } else if super::super::is_valid_identifier_name(export_name) {
            format!("exports.{export_name} = {local_name};")
        } else {
            format!("exports[\"{export_name}\"] = {local_name};")
        }
    }

    fn top_level_using_export_binding_prefix(&self, export_name: &str) -> String {
        if self.in_system_execute_body {
            format!("exports_1(\"{export_name}\", ")
        } else if super::super::is_valid_identifier_name(export_name) {
            format!("exports.{export_name} = ")
        } else {
            format!("exports[\"{export_name}\"] = ")
        }
    }

    const fn top_level_using_export_binding_suffix(&self) -> &'static str {
        if self.in_system_execute_body {
            ");"
        } else {
            ";"
        }
    }

    fn top_level_using_class_assignment_text(
        emitted: &str,
        binding_name: &str,
        class_has_name: bool,
    ) -> String {
        if let Some(rewritten) = Self::splice_top_level_using_assignment_head(
            emitted,
            &format!("let {binding_name} = "),
            &format!("{binding_name} = "),
        ) {
            return rewritten;
        }
        if let Some(rewritten) = Self::splice_top_level_using_assignment_head(
            emitted,
            &format!("var {binding_name} = "),
            &format!("{binding_name} = "),
        ) {
            return rewritten;
        }

        let class_head = format!("class {binding_name}");
        let assignment_head = if class_has_name {
            format!("{binding_name} = class {binding_name}")
        } else {
            format!("{binding_name} = class")
        };
        Self::splice_top_level_using_assignment_head(emitted, &class_head, &assignment_head)
            .unwrap_or_else(|| emitted.to_string())
    }

    fn splice_top_level_using_assignment_head(
        emitted: &str,
        needle: &str,
        replacement: &str,
    ) -> Option<String> {
        let start = emitted.find(needle)?;
        let mut rewritten =
            String::with_capacity(emitted.len() + replacement.len().saturating_sub(needle.len()));
        rewritten.push_str(&emitted[..start]);
        rewritten.push_str(replacement);
        rewritten.push_str(&emitted[start + needle.len()..]);
        Some(rewritten)
    }

    fn top_level_using_assignment_rhs<'b>(emitted: &'b str, binding_name: &str) -> Option<&'b str> {
        Some(
            emitted
                .strip_prefix(binding_name)?
                .trim_start()
                .strip_prefix('=')?
                .trim_start(),
        )
    }

    fn mark_top_level_using_inline_cjs_export(
        &mut self,
        export_name: Option<&String>,
        is_es_module_output: bool,
    ) {
        if let Some(export_name) = export_name
            && !is_es_module_output
        {
            self.ctx
                .module_state
                .inline_exported_names
                .insert(export_name.clone());
        }
    }

    fn rewrite_direct_top_level_using_class_export(
        &self,
        mut emitted: String,
        binding_name: &str,
        export_name: &str,
        is_legacy_decorator_class: bool,
    ) -> String {
        let current_indent = "    ".repeat(self.writer.indent_level() as usize);
        if let Some(stripped) = emitted.strip_prefix(&current_indent) {
            emitted = stripped.to_string();
        }

        if is_legacy_decorator_class && !self.in_top_level_using_scope {
            let export_stmt = self.top_level_using_export_binding_stmt(export_name, binding_name);
            if self.ctx.target_es5 {
                if !emitted.ends_with('\n') {
                    emitted.push('\n');
                }
                emitted.push_str(&export_stmt);
            } else {
                let export_prefix = self.top_level_using_export_binding_prefix(export_name);
                if let Some(first_stmt_end) = emitted.find(';') {
                    emitted.insert_str(first_stmt_end + 1, &format!("\n{export_stmt}"));
                }
                emitted = export_decorate_assignment(
                    emitted,
                    &export_prefix,
                    binding_name,
                    self.in_system_execute_body,
                )
                .0;
            }
            return emitted;
        }

        let export_stmt = self.top_level_using_export_binding_stmt(export_name, binding_name);
        emitted = emitted
            .lines()
            .filter(|line| line.trim() != export_stmt)
            .collect::<Vec<_>>()
            .join("\n");

        let export_prefix = self.top_level_using_export_binding_prefix(export_name);
        let export_suffix = self.top_level_using_export_binding_suffix();

        if is_legacy_decorator_class && self.ctx.target_es5 && self.in_top_level_using_scope {
            emitted = strip_decorate_export_prefix(&emitted, &export_prefix, binding_name);
        }

        if is_legacy_decorator_class
            && !self.ctx.target_es5
            && self.in_top_level_using_scope
            && let Some(first_stmt_end) = emitted.find(';')
        {
            let first_stmt = emitted[..first_stmt_end].trim_start();
            let mut remainder = emitted[first_stmt_end + 1..]
                .trim_start_matches(['\n', '\r'])
                .to_string();
            remainder = export_decorate_assignment(
                remainder,
                &export_prefix,
                binding_name,
                self.in_system_execute_body,
            )
            .0;
            let mut rewritten = format!("{export_prefix}{first_stmt}{export_suffix}");
            if !remainder.trim().is_empty() {
                rewritten.push('\n');
                rewritten.push_str(&remainder);
            }
            return rewritten;
        }

        let trimmed = emitted.trim_end();
        let trimmed = trimmed.strip_suffix(';').unwrap_or(trimmed);
        format!("{export_prefix}{trimmed}{export_suffix}")
    }
}
