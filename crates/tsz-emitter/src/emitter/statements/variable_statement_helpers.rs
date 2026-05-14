use super::super::{ModuleKind, Printer};
use tsz_parser::parser::node::Node;
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_statement_list_with_using_scope(
        &mut self,
        statements: &NodeList,
    ) -> bool {
        if self.ctx.options.target.supports_es2025()
            || !self.block_has_using_declarations(statements)
        {
            return false;
        }

        let using_async = self.block_has_await_using(statements);
        let (env_name, error_name, result_name) = self.next_disposable_env_names();
        let env_decl_keyword = if self.ctx.target_es5 { "var" } else { "const" };
        let prev_block_using_env = self
            .block_using_env
            .replace((env_name.clone(), using_async));

        self.write(env_decl_keyword);
        self.write(" ");
        self.write(&env_name);
        self.write(" = { stack: [], error: void 0, hasError: false };");
        self.write_line();
        self.write("try {");
        self.write_line();
        self.increase_indent();

        for &stmt in &statements.nodes {
            if let Some(stmt_node) = self.arena.get(stmt) {
                let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                self.emit_comments_before_pos(actual_start);
            }
            let before_emit_len = self.writer.len();
            self.emit(stmt);
            if self.writer.len() > before_emit_len && !self.writer.is_at_line_start() {
                self.write_line();
            }
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
            let await_kw = if self.ctx.emit_await_as_yield || self.ctx.emit_await_as_yield_await {
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
            if self.ctx.emit_await_as_yield_await {
                self.write_helper("__await");
                self.write("(");
                self.write(&result_name);
                self.write(")");
            } else {
                self.write(&result_name);
            }
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
        self.block_using_env = prev_block_using_env;
        true
    }

    pub(in crate::emitter) fn emit_recovered_class_keyword_variable_statement_tail(
        &mut self,
        node: &Node,
    ) {
        if !self.is_recovered_class_keyword_variable_statement(node) {
            return;
        }

        self.write_line();
        self.write("class {");
        self.write_line();
        self.write("}");
        self.write_line();
        self.write(";");
    }

    fn is_recovered_class_keyword_variable_statement(&self, node: &Node) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let Some(source) = text.get(node.pos as usize..node.end as usize) else {
            return false;
        };
        let trimmed = source.trim_start();
        let Some(rest) = trimmed.strip_prefix("var") else {
            return false;
        };
        let rest = rest.trim_start();
        if !rest.starts_with("class") {
            return false;
        }
        rest["class".len()..]
            .chars()
            .next()
            .is_none_or(|ch| ch.is_whitespace() || ch == ';')
    }

    /// Lower `using`/`await using` declarations for non-ES5 targets (ES2015+).
    /// Transforms:
    ///   `using d = expr;`
    /// Into:
    ///   `var d;`
    ///   `const env_1 = { stack: [], error: void 0, hasError: false };`
    ///   `try { d = __addDisposableResource(env_1, expr, false); }`
    ///   `catch (e_1) { env_1.error = e_1; env_1.hasError = true; }`
    ///   `finally { __disposeResources(env_1); }`
    pub(in crate::emitter) fn emit_using_declaration_lowered(
        &mut self,
        decl_list: &tsz_parser::parser::node::VariableData,
        flags: u32,
    ) {
        let using_async = node_flags::is_await_using(flags);
        let (env_name, error_name, result_name) = self.next_disposable_env_names();

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

        // Hoist `var` declarations before the try block — variables must remain
        // accessible after the try/catch/finally completes.
        if !initialized_decls.is_empty() {
            let mut var_names = Vec::new();
            for &decl_idx in &initialized_decls {
                if let Some(decl_node) = self.arena.get(decl_idx)
                    && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                {
                    self.collect_binding_names(decl.name, &mut var_names);
                }
            }
            if !var_names.is_empty() {
                self.write("var ");
                self.write(&var_names.join(", "));
                self.write(";");
                self.write_line();
            }
        }

        self.write("const ");
        self.write(&env_name);
        self.write(" = { stack: [], error: void 0, hasError: false };");
        self.write_line();

        self.write("try {");
        self.write_line();
        self.increase_indent();

        // Emit assignments (no `const`/`let` prefix — vars are hoisted above)
        if !initialized_decls.is_empty() {
            for (i, &decl_idx) in initialized_decls.iter().enumerate() {
                if let Some(decl_node) = self.arena.get(decl_idx)
                    && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                {
                    self.emit(decl.name);
                    self.write(" = ");
                    self.write_helper("__addDisposableResource");
                    self.write("(");
                    self.write(&env_name);
                    self.write(", ");
                    self.emit(decl.initializer);
                    self.write(", ");
                    self.write(if using_async { "true" } else { "false" });
                    self.write(")");
                    if i + 1 < initialized_decls.len() {
                        self.write(", ");
                    }
                }
            }
            self.write(";");
            self.write_line();
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
            // tsc emits: const result_N = __disposeResources(env_N);
            //            if (result_N) await result_N;
            // (inside __awaiter generator, `await` becomes `yield`)
            let await_kw = if self.ctx.emit_await_as_yield {
                "yield"
            } else {
                "await"
            };
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
    }

    /// Compute the source position at the end of the last emitted content for
    /// a variable statement, excluding erased type annotations. This prevents
    /// `emit_trailing_comment_after_semicolon` from finding semicolons inside
    /// erased type annotations (e.g., `var v: { (x: number); // comment }`).
    pub(in crate::emitter) fn variable_statement_effective_end(
        &self,
        declarations: &NodeList,
    ) -> u32 {
        // Walk the declaration list to find the last variable declaration's
        // name or initializer end position.
        let mut effective_end = 0u32;
        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            // Use the full node end as baseline
            effective_end = effective_end.max(decl_list_node.end);

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
                // If the declaration has a type annotation but no initializer,
                // use the name's end as the effective boundary (the type annotation
                // is erased and its semicolons should not be scanned).
                if decl.type_annotation.is_some()
                    && decl.initializer.is_none()
                    && let Some(name_node) = self.arena.get(decl.name)
                {
                    effective_end = self
                        .find_declaration_semicolon_after(name_node.end, decl_node.end)
                        .unwrap_or(name_node.end);
                }
            }
        }
        effective_end
    }

    pub(in crate::emitter) fn variable_statement_last_emitted_declaration_end(
        &self,
        declarations: &NodeList,
    ) -> Option<u32> {
        let mut last_end = None;
        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
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
                if let Some(init_node) = self.arena.get(decl.initializer) {
                    last_end = Some(init_node.end);
                } else if let Some(name_node) = self.arena.get(decl.name) {
                    last_end = Some(name_node.end);
                }
            }
        }
        last_end
    }

    pub(in crate::emitter) fn find_declaration_semicolon_after(
        &self,
        start: u32,
        end: u32,
    ) -> Option<u32> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let mut i = std::cmp::min(start as usize, bytes.len());
        let limit = std::cmp::min(end as usize, bytes.len());
        let mut depth = 0i32;
        while i < limit {
            match bytes[i] {
                b'{' | b'(' | b'[' | b'<' => {
                    depth += 1;
                    i += 1;
                }
                b'}' | b')' | b']' | b'>' => {
                    depth -= 1;
                    i += 1;
                }
                b';' if depth == 0 => return Some((i + 1) as u32),
                b'/' if i + 1 < limit && bytes[i + 1] == b'/' => {
                    while i < limit && bytes[i] != b'\n' && bytes[i] != b'\r' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < limit && bytes[i + 1] == b'*' => {
                    i += 2;
                    while i + 1 < limit && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                        i += 1;
                    }
                    i = std::cmp::min(i + 2, limit);
                }
                b'\'' | b'"' | b'`' => {
                    let quote = bytes[i];
                    i += 1;
                    while i < limit {
                        if bytes[i] == b'\\' {
                            i = std::cmp::min(i + 2, limit);
                        } else if bytes[i] == quote {
                            i += 1;
                            break;
                        } else {
                            i += 1;
                        }
                    }
                }
                _ => i += 1,
            }
        }
        None
    }

    /// Check if all variable declarations in a declaration list lack initializers
    pub(in crate::emitter) fn all_declarations_lack_initializer(
        &self,
        declarations: &NodeList,
    ) -> bool {
        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
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
                if decl.initializer.is_some() {
                    return false;
                }
            }
        }
        true
    }

    /// Check if all declared names in a variable declaration list are present
    /// in the `commonjs_exported_var_names` set (already handled by the CJS
    /// preamble `exports.X = void 0;`).
    pub(in crate::emitter) fn all_declaration_names_in_exported_set(
        &self,
        declarations: &NodeList,
    ) -> bool {
        let names = self.collect_variable_names(declarations);
        !names.is_empty()
            && names
                .iter()
                .all(|n| self.commonjs_exported_var_names.contains(n))
    }

    /// Collect variable names from a declaration list for `CommonJS` export
    pub(in crate::emitter) fn collect_variable_names(
        &self,
        declarations: &NodeList,
    ) -> Vec<String> {
        let mut names = Vec::new();
        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
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
                self.collect_binding_names(decl.name, &mut names);
            }
        }
        names
    }

    pub(in crate::emitter) fn is_es5_empty_binding_pattern_export_statement(
        &self,
        node: &Node,
    ) -> bool {
        if !self.ctx.target_es5
            || !matches!(
                self.ctx.options.module,
                ModuleKind::ES2015 | ModuleKind::ESNext
            )
        {
            return false;
        }

        let Some(var_stmt) = self.arena.get_variable(node) else {
            return false;
        };
        if !self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
        {
            return false;
        }

        self.variable_declarations_are_initialized_empty_binding_patterns(&var_stmt.declarations)
    }

    fn variable_declarations_are_initialized_empty_binding_patterns(
        &self,
        declarations: &NodeList,
    ) -> bool {
        let mut has_initializer = false;

        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                return false;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                return false;
            };

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    return false;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    return false;
                };
                if !self.binding_pattern_is_empty(decl.name) || decl.initializer.is_none() {
                    return false;
                }
                has_initializer = true;
            }
        }

        has_initializer
    }

    pub(in crate::emitter) fn collect_binding_names(
        &self,
        name_idx: NodeIndex,
        names: &mut Vec<String>,
    ) {
        if name_idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(name_idx) else {
            return;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            if let Some(id) = self.arena.get_identifier(node) {
                // Use original_text (preserving unicode escapes) when available,
                // falling back to escaped_text. TSC preserves unicode escapes
                // in CJS export assignments (exports.\u0078 = \u0078;).
                let text = id
                    .original_text
                    .as_deref()
                    .unwrap_or(&id.escaped_text)
                    .to_string();
                names.push(text);
            }
            return;
        }

        match node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    for &elem_idx in &pattern.elements.nodes {
                        self.collect_binding_names_from_element(elem_idx, names);
                    }
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(elem) = self.arena.get_binding_element(node) {
                    self.collect_binding_names(elem.name, names);
                }
            }
            _ => {}
        }
    }

    pub(in crate::emitter) fn collect_binding_names_from_element(
        &self,
        elem_idx: NodeIndex,
        names: &mut Vec<String>,
    ) {
        if elem_idx.is_none() {
            return;
        }

        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };

        if let Some(elem) = self.arena.get_binding_element(elem_node) {
            self.collect_binding_names(elem.name, names);
        }
    }

    pub(in crate::emitter) fn emit_async_generator_shadow_variable_statement(
        &mut self,
        node: &Node,
    ) -> bool {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return false;
        };

        let mut initialized_decls = Vec::new();
        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            if !self.async_generator_shadow_decl_list_applies(decl_list_node) {
                return false;
            }
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };
            for &decl_idx in &decl_list.declarations.nodes {
                if self
                    .arena
                    .get(decl_idx)
                    .and_then(|decl_node| self.arena.get_variable_declaration(decl_node))
                    .is_some_and(|decl| decl.initializer.is_some())
                {
                    initialized_decls.push(decl_idx);
                }
            }
        }

        for (i, decl_idx) in initialized_decls.iter().copied().enumerate() {
            if i > 0 {
                self.write_line();
            }
            let mut first = true;
            if self.emit_async_generator_shadow_assignment(decl_idx, true, &mut first) {
                self.write_semicolon();
            }
        }
        true
    }

    pub(in crate::emitter) fn emit_async_generator_shadow_for_initializer(
        &mut self,
        initializer: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if !self.async_generator_shadow_decl_list_applies(init_node) {
            return false;
        }
        let Some(decl_list) = self.arena.get_variable(init_node) else {
            return false;
        };

        let mut first = true;
        for &decl_idx in &decl_list.declarations.nodes {
            self.emit_async_generator_shadow_assignment(decl_idx, false, &mut first);
        }
        true
    }

    pub(in crate::emitter) fn emit_async_generator_shadow_for_in_of_initializer(
        &mut self,
        initializer: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if !self.async_generator_shadow_decl_list_applies(init_node) {
            return false;
        }
        let Some(decl_list) = self.arena.get_variable(init_node) else {
            return false;
        };

        for (i, &decl_idx) in decl_list.declarations.nodes.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            if let Some(decl_node) = self.arena.get(decl_idx)
                && let Some(decl) = self.arena.get_variable_declaration(decl_node)
            {
                self.emit_decl_name(decl.name);
            }
        }
        true
    }

    pub(in crate::emitter) fn async_generator_shadow_decl_list_applies(
        &self,
        decl_list_node: &Node,
    ) -> bool {
        if self.ctx.async_generator_shadowed_parameter_names.is_empty()
            || decl_list_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST
        {
            return false;
        }

        let flags = decl_list_node.flags as u32;
        if flags & (node_flags::LET | node_flags::CONST | node_flags::USING) != 0 {
            return false;
        }

        let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
            return false;
        };
        decl_list
            .declarations
            .nodes
            .iter()
            .copied()
            .any(|decl_idx| {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    return false;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    return false;
                };
                let mut names = Vec::new();
                self.collect_binding_names(decl.name, &mut names);
                names.iter().any(|name| {
                    self.ctx
                        .async_generator_shadowed_parameter_names
                        .iter()
                        .any(|param| param == name)
                })
            })
    }

    fn emit_async_generator_shadow_assignment(
        &mut self,
        decl_idx: NodeIndex,
        statement_position: bool,
        first: &mut bool,
    ) -> bool {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return false;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        if decl.initializer.is_none() {
            return false;
        }

        if !*first {
            self.write(", ");
        }
        *first = false;

        if self.ctx.needs_es2018_lowering && self.pattern_has_object_rest(decl.name) {
            self.emit_object_rest_var_decl(decl.name, decl.initializer, None);
            return true;
        }

        let is_object_pattern = self
            .arena
            .get(decl.name)
            .is_some_and(|name| name.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN);
        if statement_position && is_object_pattern {
            self.write("(");
        }
        self.emit_decl_name(decl.name);
        self.write(" = ");
        self.emit_expression(decl.initializer);
        if statement_position && is_object_pattern {
            self.write(")");
        }
        true
    }
}
