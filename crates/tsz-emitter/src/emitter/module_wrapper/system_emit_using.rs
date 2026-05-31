use super::super::Printer;
use crate::transforms::ClassES5Emitter;
use rustc_hash::FxHashMap;
use std::collections::{HashMap, HashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(super) fn emit_system_top_level_using_scope(
        &mut self,
        source: &tsz_parser::parser::node::SourceFileData,
        start_idx: usize,
        dep_vars: &HashMap<String, String>,
        hoisted_func_stmts: &HashSet<NodeIndex>,
    ) {
        let mut deferred_named_exports: FxHashMap<String, String> = FxHashMap::default();
        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export_decl.module_specifier.is_some() {
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
            for &spec_idx in &named_exports.elements.nodes {
                if let Some(spec) = self.arena.get_specifier_at(spec_idx) {
                    let local_name = if spec.property_name.is_some() {
                        self.get_specifier_name_text(spec.property_name)
                    } else {
                        self.get_specifier_name_text(spec.name)
                    }
                    .unwrap_or_default();
                    let export_name = self.get_specifier_name_text(spec.name).unwrap_or_default();
                    if !local_name.is_empty() && !export_name.is_empty() {
                        deferred_named_exports.insert(local_name, export_name);
                    }
                }
            }
        }
        let prev_in_system_top_level_using_prelude = self.in_system_top_level_using_prelude;
        self.in_system_top_level_using_prelude = true;
        for &stmt_idx in &source.statements.nodes[..start_idx] {
            if hoisted_func_stmts.contains(&stmt_idx) {
                continue;
            }
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }
            if stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                let before_len = self.writer.len();
                if self.emit_system_import_equals_declaration(stmt_node, dep_vars, false)
                    && self.writer.len() > before_len
                {
                    self.write_line();
                }
                continue;
            }
            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_decl) = self.arena.get_export_decl(stmt_node)
                && export_decl.module_specifier.is_none()
                && let Some(clause_node) = self.arena.get(export_decl.export_clause)
                && clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
            {
                continue;
            }

            let before_len = self.writer.len();
            if self.emit_system_top_level_using_statement(
                stmt_node,
                stmt_idx,
                dep_vars,
                &deferred_named_exports,
            ) {
                if self.writer.len() > before_len
                    && stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION
                    && !self.writer.is_at_line_start()
                {
                    self.write_line();
                }
                continue;
            } else if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && self.emit_system_export_declaration(stmt_node, dep_vars)
            {
                if self.writer.len() > before_len {
                    self.write_line();
                }
                continue;
            }

            if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                self.emit_system_variable_initializers(stmt_node);
            } else {
                self.emit(stmt_idx);
            }

            if self.writer.len() > before_len
                && stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION
            {
                self.write_line();
            }
        }
        self.in_system_top_level_using_prelude = prev_in_system_top_level_using_prelude;

        if self.ctx.options.target.supports_es2025() {
            let prev_deferred_local_export_bindings = self
                .deferred_local_export_bindings
                .replace(deferred_named_exports.clone());
            for &stmt_idx in &source.statements.nodes[start_idx..] {
                if hoisted_func_stmts.contains(&stmt_idx) {
                    continue;
                }
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    continue;
                };
                if self.is_erased_statement(stmt_node) {
                    continue;
                }
                if self.emit_system_top_level_using_statement(
                    stmt_node,
                    stmt_idx,
                    dep_vars,
                    &deferred_named_exports,
                ) && !self.writer.is_at_line_start()
                {
                    self.write_line();
                }
            }
            self.deferred_local_export_bindings = prev_deferred_local_export_bindings;
            return;
        }

        let using_async = source.statements.nodes[start_idx..]
            .iter()
            .any(|&stmt_idx| {
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
        let (env_name, error_name, result_name) = self.next_disposable_env_names();
        self.write(&env_name);
        self.write(" = { stack: [], error: void 0, hasError: false };");
        self.write_line();
        self.write("try {");
        self.write_line();
        self.increase_indent();

        let prev_deferred_local_export_bindings = self
            .deferred_local_export_bindings
            .replace(deferred_named_exports.clone());
        let prev_block_using_env = self
            .block_using_env
            .replace((env_name.clone(), using_async));
        let prev_in_top_level_using_scope = self.in_top_level_using_scope;
        self.in_top_level_using_scope = true;
        for &stmt_idx in &source.statements.nodes[start_idx..] {
            if hoisted_func_stmts.contains(&stmt_idx) {
                continue;
            }
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if self.is_erased_statement(stmt_node) {
                continue;
            }
            if self.emit_system_top_level_using_statement(
                stmt_node,
                stmt_idx,
                dep_vars,
                &deferred_named_exports,
            ) && !self.writer.is_at_line_start()
            {
                self.write_line();
            }
        }
        self.in_top_level_using_scope = prev_in_top_level_using_scope;
        self.block_using_env = prev_block_using_env;
        self.deferred_local_export_bindings = prev_deferred_local_export_bindings;

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
            self.write("const ");
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
            self.write("await ");
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
    }

    fn emit_system_top_level_using_statement(
        &mut self,
        stmt_node: &tsz_parser::parser::node::Node,
        stmt_idx: NodeIndex,
        dep_vars: &HashMap<String, String>,
        deferred_named_exports: &FxHashMap<String, String>,
    ) -> bool {
        match stmt_node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => self
                .emit_system_top_level_using_variable_statement(stmt_node, deferred_named_exports),
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                let export_name = self
                    .arena
                    .get_class(stmt_node)
                    .and_then(|class| self.get_identifier_text_opt(class.name))
                    .and_then(|name| deferred_named_exports.get(&name).cloned());
                self.emit_top_level_using_class_assignment(
                    stmt_node,
                    stmt_idx,
                    export_name,
                    false,
                    false,
                )
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                let export_name = self
                    .arena
                    .get_function(stmt_node)
                    .and_then(|func| self.get_identifier_text_opt(func.name))
                    .and_then(|name| deferred_named_exports.get(&name).cloned());
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
                        .emit_system_top_level_using_variable_statement(
                            clause_node,
                            deferred_named_exports,
                        ),
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        let export_name = if export.is_default_export {
                            Some("default".to_string())
                        } else {
                            self.arena
                                .get_class(clause_node)
                                .and_then(|class| self.get_identifier_text_opt(class.name))
                        };
                        if let Some(export_name) = export_name {
                            // When a legacy-decorated ES5 class with value-position self-references
                            // is inside a System module's top-level `using` try block, tsc uses the
                            // outer-alias pattern (C_1) and emits two separate exports_1 calls:
                            //   exports_1("C", C = C_1 = /** @class */ (...));
                            //   exports_1("C", C = C_1 = __decorate([dec], C_1));
                            // The generic emit_top_level_using_class_assignment path uses the
                            // inline-alias pattern instead, producing a structural mismatch.
                            if !export.is_default_export
                                && self.ctx.options.legacy_decorators
                                && self.ctx.target_es5
                                && self.in_top_level_using_scope
                                && !self.in_system_top_level_using_prelude
                            {
                                let class_data =
                                    self.arena.get_class(clause_node).map(|class_decl| {
                                        let decorators =
                                            self.collect_class_decorators(&class_decl.modifiers);
                                        let class_name =
                                            self.get_identifier_text_idx(class_decl.name);
                                        let members: Vec<NodeIndex> =
                                            class_decl.members.nodes.clone();
                                        (class_name, decorators, members)
                                    });
                                if let Some((class_name, decorators, members)) = class_data {
                                    let has_decorators = !decorators.is_empty()
                                        || !self
                                            .collect_constructor_param_decorators(&members)
                                            .is_empty();
                                    if has_decorators {
                                        let alias_name = self.system_legacy_decorated_class_alias(
                                            export.export_clause,
                                            &class_name,
                                            &members,
                                        );
                                        if alias_name.is_some() {
                                            self.emit_system_using_legacy_decorated_es5_class_export(
                                                clause_node,
                                                export.export_clause,
                                                &export_name,
                                                &class_name,
                                                &decorators,
                                                &members,
                                                alias_name.as_deref(),
                                            );
                                            return true;
                                        }
                                    }
                                }
                            }
                            self.emit_top_level_using_class_assignment(
                                clause_node,
                                export.export_clause,
                                Some(export_name),
                                !export.is_default_export,
                                false,
                            )
                        } else {
                            false
                        }
                    }
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        let export_name = if export.is_default_export {
                            Some("default".to_string())
                        } else {
                            self.arena
                                .get_function(clause_node)
                                .and_then(|func| self.get_identifier_text_opt(func.name))
                        };
                        if let Some(export_name) = export_name {
                            self.emit_top_level_using_function_assignment(
                                clause_node,
                                export.export_clause,
                                Some(export_name),
                            )
                        } else {
                            false
                        }
                    }
                    k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                        self.emit_system_import_equals_declaration(clause_node, dep_vars, true)
                    }
                    k if k == syntax_kind_ext::NAMED_EXPORTS => true,
                    _ if export.is_default_export => {
                        self.write_export_binding_start("default");
                        self.write("_default = ");
                        self.emit(export.export_clause);
                        self.write_export_binding_end();
                        true
                    }
                    _ => false,
                }
            }
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                let Some(export_assignment) = self.arena.get_export_assignment(stmt_node) else {
                    return false;
                };
                if export_assignment.is_export_equals {
                    return false;
                }
                self.write_export_binding_start("default");
                self.write("_default = ");
                self.emit(export_assignment.expression);
                self.write_export_binding_end();
                true
            }
            _ => {
                self.emit(stmt_idx);
                true
            }
        }
    }

    /// Emit a legacy-decorated ES5 class with value-position self-references inside
    /// a System module's top-level `using` try block, using the outer-alias pattern:
    ///
    /// ```text
    /// exports_1("C", C = C_1 = /** @class */ (function () { … return C; }()));
    /// exports_1("C", C = C_1 = __decorate([dec], C_1));
    /// ```
    ///
    /// The outer-alias pattern (`C_1`) is what `tsc` produces. The generic
    /// `emit_top_level_using_class_assignment` path would instead use the inline
    /// self-capture pattern (`C_2 = C; var C_2;` inside the IIFE), which diverges.
    fn emit_system_using_legacy_decorated_es5_class_export(
        &mut self,
        _class_node: &tsz_parser::parser::node::Node,
        class_idx: NodeIndex,
        export_name: &str,
        class_name: &str,
        decorators: &[NodeIndex],
        members: &[NodeIndex],
        alias_name: Option<&str>,
    ) {
        // Build an ES5 class emitter WITHOUT decorator info so that __decorate
        // is NOT inlined into the IIFE body. The outer-alias pattern that tsc
        // produces requires two separate export calls:
        //   exports_1("C", C = C_1 = /** @class */ (function () { … }()));
        //   exports_1("C", C = C_1 = __decorate([dec], C_1));
        let mut es5_emitter = ClassES5Emitter::new(self.arena);
        es5_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
        es5_emitter
            .set_async_generator_inner_name_counts(self.async_generator_inner_name_counts.clone());
        self.configure_es5_class_emitter_disposable_context(&mut es5_emitter);
        es5_emitter.set_indent_level(self.writer.indent_level());
        es5_emitter.set_transforms(self.transforms.clone());
        es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
        es5_emitter.set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);
        es5_emitter.set_printer_options(self.ctx.options.clone());
        es5_emitter.set_module_kind(self.ctx.outer_module_kind());
        if let Some(text) = self.source_text_for_map() {
            es5_emitter.set_source_text(text);
        }
        if let Some(alias) = alias_name {
            es5_emitter.set_class_self_reference_alias(alias.to_string());
        }

        let (iife_expr, _computed_decls, _computed_inits) =
            es5_emitter.emit_class_as_iife_expr(class_idx, class_name);
        self.sync_es5_class_emitter_state(&mut es5_emitter);

        self.write("exports_1(\"");
        self.write(export_name);
        self.write("\", ");
        self.write(class_name);
        self.write(" = ");
        if let Some(alias) = alias_name {
            self.write(alias);
            self.write(" = ");
        }
        self.write(&iife_expr);
        self.write(");");
        self.write_line();
        self.emit_system_legacy_class_decorator_export(
            export_name,
            class_name,
            decorators,
            members,
            alias_name,
        );
    }

    fn emit_system_top_level_using_variable_statement(
        &mut self,
        node: &tsz_parser::parser::node::Node,
        deferred_named_exports: &FxHashMap<String, String>,
    ) -> bool {
        if self.ctx.options.target.supports_es2025() {
            return self.emit_system_native_top_level_using_variable_statement(
                node,
                deferred_named_exports,
            );
        }
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
                        if !self.try_emit_object_literal_es5_inline_computed_expression(
                            decl.initializer,
                        ) {
                            self.emit(decl.initializer);
                        }
                    } else {
                        self.write("void 0");
                    }
                    self.write(", ");
                    self.write(if using_async { "true" } else { "false" });
                    self.write(");");
                } else if is_exported {
                    self.write_export_binding_start(&name);
                    self.write(&name);
                    self.write(" = ");
                    if decl.initializer.is_some() {
                        self.emit(decl.initializer);
                    } else {
                        self.write("void 0");
                    }
                    self.write_export_binding_end();
                } else if let Some(export_name) = deferred_named_exports.get(&name) {
                    self.write_export_binding_start(export_name);
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

    fn emit_system_native_top_level_using_variable_statement(
        &mut self,
        node: &tsz_parser::parser::node::Node,
        deferred_named_exports: &FxHashMap<String, String>,
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
            let is_await_using = tsz_parser::parser::node_flags::is_await_using(flags);

            if is_using || is_await_using {
                if emitted {
                    self.write_line();
                }
                if is_await_using {
                    self.write("await using ");
                } else {
                    self.write("using ");
                }

                let mut first = true;
                for &decl_idx in &decl_list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                        continue;
                    };
                    let name = self.get_identifier_text_idx(decl.name);
                    if name.is_empty() {
                        continue;
                    }
                    if !first {
                        self.write(", ");
                    }
                    let temp_name = self.make_unique_name_from_base(&name);
                    self.write(&temp_name);
                    self.write(" = ");
                    self.write(&name);
                    self.write(" = ");
                    if decl.initializer.is_some() {
                        self.emit_expression(decl.initializer);
                    } else {
                        self.write("void 0");
                    }
                    first = false;
                }
                if !first {
                    self.write(";");
                    emitted = true;
                }
                continue;
            }

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };

                if decl.initializer.is_none() {
                    continue;
                }

                let name = self.get_identifier_text_idx(decl.name);
                if name.is_empty() {
                    continue;
                }

                if emitted {
                    self.write_line();
                }

                if is_exported {
                    self.write("exports_1(\"");
                    self.write(&name);
                    self.write("\", ");
                    self.write(&name);
                    self.write(" = ");
                    self.emit_expression(decl.initializer);
                    self.write(");");
                } else if let Some(export_name) = deferred_named_exports.get(&name) {
                    self.write_export_binding_start(export_name);
                    self.write(&name);
                    self.write(" = ");
                    self.emit_expression(decl.initializer);
                    self.write_export_binding_end();
                } else {
                    self.write(&name);
                    self.write(" = ");
                    self.emit_expression(decl.initializer);
                    self.write(";");
                }
                emitted = true;
            }
        }

        emitted
    }
}
