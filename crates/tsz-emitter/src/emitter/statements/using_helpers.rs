use super::super::Printer;
use tsz_parser::parser::base::NodeList;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    /// Check if a statement list contains any `using`/`await using` declarations.
    pub(in crate::emitter) fn block_has_using_declarations(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                if self.variable_statement_source_using_flags(stmt_node) != 0 {
                    return true;
                }
                let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                    continue;
                };
                for &decl_list_idx in &var_stmt.declarations.nodes {
                    if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                        && let Some(decl_list) = self.arena.get_variable(decl_list_node)
                    {
                        let flags = decl_list.declarations.nodes.iter().fold(
                            decl_list_node.flags as u32,
                            |flags, &decl_idx| {
                                flags | self.arena.get_variable_declaration_flags(decl_idx)
                            },
                        );
                        if (flags & node_flags::USING) != 0 {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    pub(in crate::emitter) fn variable_statement_source_using_flags(&self, node: &Node) -> u32 {
        if node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return 0;
        }
        let Some(source_text) = self.source_text else {
            return 0;
        };
        let start = (node.pos as usize).min(source_text.len());
        let end = (node.end as usize).min(source_text.len());
        let text = source_text[start..end].trim_start();
        if text.starts_with("await using")
            && text
                .as_bytes()
                .get("await using".len())
                .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_' && *byte != b'$')
        {
            return node_flags::AWAIT_USING;
        }
        if text.starts_with("using")
            && text
                .as_bytes()
                .get("using".len())
                .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_' && *byte != b'$')
        {
            return node_flags::USING;
        }
        0
    }

    /// Check if a statement list contains any `await using` declarations.
    pub(in crate::emitter) fn block_has_await_using(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                    continue;
                };
                for &decl_list_idx in &var_stmt.declarations.nodes {
                    if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                        && (decl_list_node.flags as u32 & node_flags::AWAIT_USING)
                            == node_flags::AWAIT_USING
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Emit just the `__addDisposableResource` calls for a using declaration,
    /// without the try/catch/finally wrapper (used when block-level wrapping is active).
    pub(crate) fn emit_using_addresource_only(
        &mut self,
        decl_list: &tsz_parser::parser::node::VariableData,
        env_name: &str,
        using_async: bool,
    ) {
        let initialized_decls: Vec<_> = decl_list
            .declarations
            .nodes
            .iter()
            .copied()
            .filter(|&decl_idx| {
                self.arena
                    .get(decl_idx)
                    .and_then(|n| self.arena.get_variable_declaration(n))
                    .is_some_and(|d| d.initializer.is_some())
            })
            .collect();

        // Block-level using: tsc emits `const`/`var d1 = __addDisposableResource(env, expr, false)`
        // inside the try block. Uses `var` for ES5, `const` otherwise.
        if !initialized_decls.is_empty() {
            let kw = if self.ctx.target_es5 { "var" } else { "const" };
            self.write(kw);
            self.write(" ");
            for (i, &decl_idx) in initialized_decls.iter().enumerate() {
                if let Some(decl_node) = self.arena.get(decl_idx)
                    && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                {
                    self.emit(decl.name);
                    self.write(" = ");
                    self.write_helper("__addDisposableResource");
                    self.write("(");
                    self.write(env_name);
                    self.write(", ");
                    if !self
                        .try_emit_object_literal_es5_inline_computed_expression(decl.initializer)
                    {
                        self.emit(decl.initializer);
                    }
                    self.write(", ");
                    self.write(if using_async { "true" } else { "false" });
                    self.write(")");
                    if i + 1 < initialized_decls.len() {
                        self.write(", ");
                    }
                }
            }
            self.write(";");
        }
    }
}
