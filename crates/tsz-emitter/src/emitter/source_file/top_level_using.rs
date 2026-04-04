use super::super::Printer;
use crate::transforms::{ClassDecoratorInfo, ClassES5Emitter};
use rustc_hash::FxHashSet;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::NodeList;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

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
                        || (flags & tsz_parser::parser::node_flags::AWAIT_USING)
                            == tsz_parser::parser::node_flags::AWAIT_USING
                })
            })
        })
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
                    (decl_list_node.flags as u32 & tsz_parser::parser::node_flags::AWAIT_USING)
                        == tsz_parser::parser::node_flags::AWAIT_USING
                })
            })
        });
        let (env_name, error_name, result_name) = self.next_disposable_env_names();
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
        let prev_deferred_local_export_bindings = if is_es_module_output {
            None
        } else {
            self.deferred_local_export_bindings
                .replace(cjs_deferred_export_bindings.unwrap_or_default())
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
        let mut hoisted_function_indices = FxHashSet::default();

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
                        hoisted_function_indices.insert(stmt_idx);
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
                            if clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                                hoisted_function_indices.insert(stmt_idx);
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

        hoisted_function_indices
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
        let name = match node.kind {
            k if k == syntax_kind_ext::CLASS_DECLARATION => self
                .arena
                .get_class(node)
                .and_then(|class| self.get_identifier_text_opt(class.name)),
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                .arena
                .get_function(node)
                .and_then(|func| self.get_identifier_text_opt(func.name)),
            _ => None,
        };
        let Some(name) = name else {
            return;
        };
        if seen_local.insert(name.clone()) {
            local_names.push(name.clone());
        }
        if is_exported && is_es_module_output {
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
                };
                self.emit_top_level_using_class_assignment(stmt_node, stmt_idx, export_name, false)
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
                            if is_es_module_output {
                                None
                            } else if export.is_default_export {
                                Some("default".to_string())
                            } else {
                                self.arena
                                    .get_class(clause_node)
                                    .and_then(|class| self.get_identifier_text_opt(class.name))
                            },
                            !export.is_default_export,
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
            let using_async = (flags & tsz_parser::parser::node_flags::AWAIT_USING)
                == tsz_parser::parser::node_flags::AWAIT_USING;

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
                        self.emit(decl.initializer);
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
            self.write("export {};");
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

        let export_stmt = self.top_level_using_export_binding_stmt(export_name, binding_name);
        emitted = emitted
            .lines()
            .filter(|line| line.trim() != export_stmt)
            .collect::<Vec<_>>()
            .join("\n");

        let export_prefix = self.top_level_using_export_binding_prefix(export_name);
        let export_suffix = self.top_level_using_export_binding_suffix();

        if is_legacy_decorator_class && self.ctx.target_es5 {
            let exported_decorate = format!("{export_prefix}{binding_name} = __decorate(");
            emitted = emitted.replace(&exported_decorate, &format!("{binding_name} = __decorate("));
        }

        if is_legacy_decorator_class && !self.ctx.target_es5 {
            if let Some(first_stmt_end) = emitted.find(';') {
                let first_stmt = emitted[..first_stmt_end].trim_start();
                let mut remainder = emitted[first_stmt_end + 1..]
                    .trim_start_matches(['\n', '\r'])
                    .to_string();
                let decorate_pattern = format!("{binding_name} = __decorate(");
                let exported_decorate = format!("{export_prefix}{binding_name} = __decorate(");
                if !remainder.contains(&exported_decorate) {
                    remainder = remainder.replacen(&decorate_pattern, &exported_decorate, 1);
                    if self.in_system_execute_body
                        && let Some(relative_end) = remainder.rfind(");")
                    {
                        let end = relative_end;
                        remainder.replace_range(end..end + 2, "));\n");
                        if remainder.ends_with("\n\n") {
                            remainder.pop();
                        }
                    }
                }
                let mut rewritten = format!("{export_prefix}{first_stmt}{export_suffix}");
                if !remainder.trim().is_empty() {
                    rewritten.push('\n');
                    rewritten.push_str(&remainder);
                }
                return rewritten;
            }
        }

        let trimmed = emitted.trim_end();
        let trimmed = trimmed.strip_suffix(';').unwrap_or(trimmed);
        format!("{export_prefix}{trimmed}{export_suffix}")
    }

    pub(in crate::emitter) fn rewrite_legacy_top_level_using_class_export(
        &self,
        mut emitted: String,
        binding_name: &str,
        export_name: &str,
    ) -> String {
        let leading_indent = if self.in_system_execute_body {
            Some("    ".repeat(self.writer.indent_level() as usize))
        } else {
            None
        };
        if let Some(indent) = leading_indent.as_ref()
            && let Some(stripped) = emitted.strip_prefix(indent)
        {
            emitted = stripped.to_string();
        }
        let export_stmt = if let Some(indent) = leading_indent.as_ref() {
            format!(
                "{indent}{}",
                self.top_level_using_export_binding_stmt(export_name, binding_name)
            )
        } else {
            self.top_level_using_export_binding_stmt(export_name, binding_name)
        };

        if export_name == "default" {
            if !emitted.ends_with('\n') {
                emitted.push('\n');
            }
            emitted.push_str(&export_stmt);
            return emitted;
        }

        if let Some(first_stmt_end) = emitted.find(';')
            && (!self.in_system_execute_body || !self.ctx.target_es5)
        {
            emitted.insert_str(first_stmt_end + 1, &format!("\n{export_stmt}"));
        }

        if self.in_system_execute_body && self.ctx.target_es5 {
            if !emitted.ends_with('\n') {
                emitted.push('\n');
            }
            emitted.push_str(&export_stmt);
            return emitted;
        }

        let decorate_pattern = format!("{binding_name} = __decorate(");
        let mut replaced_decorate_assignment = false;
        if let Some(decorate_start) = emitted.rfind(&decorate_pattern) {
            let replacement = format!(
                "{}{binding_name} = __decorate(",
                self.top_level_using_export_binding_prefix(export_name)
            );
            emitted.replace_range(
                decorate_start..decorate_start + decorate_pattern.len(),
                &replacement,
            );
            if self.in_system_execute_body
                && let Some(relative_end) = emitted[decorate_start..].rfind(");")
            {
                let end = decorate_start + relative_end;
                emitted.replace_range(end..end + 2, "));\n");
                if emitted.ends_with("\n\n") {
                    emitted.pop();
                }
            }
            replaced_decorate_assignment = true;
        }

        if self.in_system_execute_body {
            if !replaced_decorate_assignment {
                if !emitted.ends_with('\n') {
                    emitted.push('\n');
                }
                emitted.push_str(&export_stmt);
            }
            return emitted;
        }

        emitted
    }

    pub(in crate::emitter) fn emit_top_level_using_class_assignment(
        &mut self,
        node: &Node,
        idx: NodeIndex,
        export_name: Option<String>,
        rewrite_as_direct_export: bool,
    ) -> bool {
        let Some(class) = self.arena.get_class(node) else {
            return false;
        };
        let binding_name = self.get_identifier_text_opt(class.name).or_else(|| {
            if export_name.as_deref() == Some("default") {
                Some(
                    self.anonymous_default_export_name
                        .clone()
                        .unwrap_or_else(|| "default_1".to_string()),
                )
            } else {
                None
            }
        });
        let Some(binding_name) = binding_name else {
            return false;
        };
        let has_explicit_export_modifier = self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
            || self
                .arena
                .has_modifier(&class.modifiers, SyntaxKind::DefaultKeyword);
        let synth_default_name = class.name.is_none() && export_name.as_deref() == Some("default");
        let prev_anon_default_name = if synth_default_name {
            let prev = self.anonymous_default_export_name.clone();
            self.anonymous_default_export_name = Some(binding_name.clone());
            Some(prev)
        } else {
            None
        };
        let has_decorators = !self.collect_class_decorators(&class.modifiers).is_empty();
        let display_name = if export_name.as_deref() == Some("default") && class.name.is_none() {
            "default".to_string()
        } else {
            binding_name.clone()
        };
        if self.ctx.options.legacy_decorators
            && self.ctx.target_es5
            && has_decorators
            && export_name.as_deref() == Some("default")
            && class.name.is_none()
        {
            let mut es5_emitter = ClassES5Emitter::new(self.arena);
            es5_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
            es5_emitter.set_indent_level(self.writer.indent_level());
            es5_emitter.set_transforms(self.transforms.clone());
            es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
            if let Some(text) = self.source_text_for_map() {
                es5_emitter.set_source_text(text);
            }
            es5_emitter
                .set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);
            es5_emitter.set_decorator_info(ClassDecoratorInfo {
                class_decorators: self.collect_class_decorators(&class.modifiers),
                has_member_decorators: false,
                emit_decorator_metadata: self.ctx.options.emit_decorator_metadata,
            });
            let mut output = es5_emitter.emit_class_with_name(idx, &binding_name);
            self.ctx.destructuring_state.temp_var_counter = es5_emitter.temp_var_counter();
            output = output.replacen(
                &format!("var {binding_name} = "),
                &format!("{binding_name} = "),
                1,
            );
            if self.in_system_execute_body {
                let leading_indent = "    ".repeat(self.writer.indent_level() as usize);
                if let Some(stripped) = output.strip_prefix(&leading_indent) {
                    output = stripped.to_string();
                }
            }
            self.write(&output);
            if !self.writer.is_at_line_start() {
                self.write_line();
            }
            self.write_export_binding_start("default");
            self.write(&binding_name);
            self.write_export_binding_end();
            return true;
        }
        if self.ctx.options.target.supports_es2025()
            && has_decorators
            && !self.ctx.options.legacy_decorators
            && self.in_system_execute_body
        {
            let before_len = self.writer.len();
            self.emit_class_es6_with_options(node, idx, false, Some(("", binding_name.clone())));
            let after_len = self.writer.len();
            let full_output = self.writer.get_output().to_string();
            let emitted = &full_output[before_len..after_len];
            let assign_prefix = format!("{binding_name} = ");
            let rewritten = if let Some(assign_idx) = emitted.find(&assign_prefix) {
                let leading_modifiers = emitted[..assign_idx].trim_end_matches('\n');
                let class_text = &emitted[assign_idx + assign_prefix.len()..];
                let mut rewritten = String::new();
                rewritten.push_str(&assign_prefix);
                if !leading_modifiers.is_empty() {
                    rewritten.push('\n');
                    rewritten.push_str(leading_modifiers);
                    rewritten.push('\n');
                }
                rewritten.push_str(class_text);
                rewritten
            } else {
                emitted.to_string()
            };

            self.writer.truncate(before_len);
            self.write(&rewritten);
            if !rewritten.trim_end().ends_with(';') {
                self.write(";");
            }
            if let Some(export_name) = export_name.as_ref() {
                self.write_line();
                self.write_export_binding_start(export_name);
                self.write(&binding_name);
                self.write_export_binding_end();
            }
            if let Some(prev) = prev_anon_default_name {
                self.anonymous_default_export_name = prev;
            }
            return true;
        }
        let before_len = self.writer.len();
        self.emit(idx);
        let after_len = self.writer.len();
        if let Some(prev) = prev_anon_default_name {
            self.anonymous_default_export_name = prev;
        }
        let full_output = self.writer.get_output().to_string();
        let emitted = &full_output[before_len..after_len];

        let mut rewritten = emitted.replacen(
            &format!("let {binding_name} = "),
            &format!("{binding_name} = "),
            1,
        );
        if rewritten == emitted {
            rewritten = emitted.replacen(
                &format!("var {binding_name} = "),
                &format!("{binding_name} = "),
                1,
            );
        }
        if rewritten == emitted {
            let replacement = if class.name.is_some() {
                format!("{binding_name} = class {binding_name}")
            } else {
                format!("{binding_name} = class")
            };
            rewritten = emitted.replacen(&format!("class {binding_name}"), &replacement, 1);
        }

        self.writer.truncate(before_len);
        if let Some(export_name) = export_name.as_ref() {
            if rewrite_as_direct_export
                && export_name != "default"
                && !(self.in_system_execute_body
                    && self.ctx.options.target.supports_es2025()
                    && self.ctx.options.legacy_decorators
                    && has_decorators)
            {
                self.write(&self.rewrite_direct_top_level_using_class_export(
                    rewritten,
                    &binding_name,
                    export_name,
                    self.ctx.options.legacy_decorators && has_decorators,
                ));
            } else if self.ctx.options.legacy_decorators && has_decorators {
                self.write(&self.rewrite_legacy_top_level_using_class_export(
                    rewritten,
                    &binding_name,
                    export_name,
                ));
            } else if let Some(mut rewritten) =
                self.render_simple_tc39_decorated_class_es5(node, idx, &binding_name, &display_name)
            {
                rewritten = rewritten.replacen(
                    &format!("var {binding_name} = "),
                    &format!("{binding_name} = "),
                    1,
                );
                if self.in_system_execute_body {
                    let leading_indent = "    ".repeat(self.writer.indent_level() as usize);
                    if let Some(stripped) = rewritten.strip_prefix(&leading_indent) {
                        rewritten = stripped.to_string();
                    }
                }
                if self.in_system_execute_body && export_name == "default" && class.name.is_none() {
                    self.write(&rewritten);
                    if !rewritten.trim_end().ends_with(';') {
                        self.write(";");
                    }
                    self.write_line();
                    self.write_export_binding_start(export_name);
                    self.write(&binding_name);
                    self.write_export_binding_end();
                } else if self.in_system_execute_body
                    && (has_explicit_export_modifier || export_name == "default")
                {
                    let trimmed = rewritten.strip_suffix(';').unwrap_or(&rewritten);
                    self.write_export_binding_start(export_name);
                    if export_name == "default" && class.name.is_some() {
                        self.write("_default = ");
                    }
                    self.write(trimmed);
                    self.write_export_binding_end();
                } else if self.in_system_execute_body {
                    self.write(&rewritten);
                    if !rewritten.trim_end().ends_with(';') {
                        self.write(";");
                    }
                    self.write_line();
                    self.write_export_binding_start(export_name);
                    self.write(&binding_name);
                    self.write_export_binding_end();
                } else {
                    self.write_export_binding_start(export_name);
                    self.write(&rewritten);
                }
            } else if self.in_system_execute_body
                && export_name == "default"
                && !self.ctx.options.target.supports_es2025()
                && class.name.is_none()
            {
                let trimmed = rewritten
                    .strip_suffix(';')
                    .unwrap_or(&rewritten)
                    .trim_start();
                let inline_expr = if let Some(eq_idx) = trimmed.find(" = (() => {") {
                    let iife = trimmed[eq_idx + 3..].replace("class_1", "default_1");
                    iife.replace(
                        "__setFunctionName(_classThis, \"default_1\");",
                        "__setFunctionName(_classThis, \"default\");",
                    )
                } else {
                    trimmed.to_string()
                };
                self.write_export_binding_start(export_name);
                if self.in_top_level_using_scope {
                    self.write("_default = ");
                }
                self.write(&inline_expr);
                self.write_export_binding_end();
            } else if self.in_system_execute_body
                && (has_explicit_export_modifier
                    || (!self.ctx.options.target.supports_es2025() && export_name == "default"))
            {
                let trimmed = rewritten.strip_suffix(';').unwrap_or(&rewritten);
                self.write_export_binding_start(export_name);
                if export_name == "default"
                    && !self.ctx.options.target.supports_es2025()
                    && class.name.is_some()
                {
                    self.write("_default = ");
                }
                self.write(trimmed);
                self.write_export_binding_end();
            } else if self.in_system_execute_body {
                self.write(&rewritten);
                if !rewritten.trim_end().ends_with(';') {
                    self.write(";");
                }
                self.write_line();
                self.write_export_binding_start(export_name);
                self.write(&binding_name);
                self.write_export_binding_end();
            } else {
                self.write_export_binding_start(export_name);
                self.write(&rewritten);
            }
        } else {
            self.write(&rewritten);
        }
        true
    }

    pub(in crate::emitter) fn emit_top_level_using_function_assignment(
        &mut self,
        node: &Node,
        idx: NodeIndex,
        export_name: Option<String>,
    ) -> bool {
        let Some(func) = self.arena.get_function(node) else {
            return false;
        };
        let Some(name) = self.get_identifier_text_opt(func.name) else {
            return false;
        };
        if let Some(export_name) = export_name.as_ref() {
            self.write_export_binding_start(export_name);
        }
        self.write(&name);
        self.write(" = ");
        self.emit_function_expression(node, idx);
        if export_name.is_some() {
            self.write_export_binding_end();
        } else {
            self.write(";");
        }
        true
    }
}
