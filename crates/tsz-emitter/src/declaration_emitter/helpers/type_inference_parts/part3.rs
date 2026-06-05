use super::*;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn inferred_declaration_preserves_initializer_type_arguments(
        &self,
        initializer: NodeIndex,
        type_id: tsz_solver::types::TypeId,
    ) -> bool {
        let Some(interner) = self.type_interner else {
            return false;
        };
        self.arena
            .get(initializer)
            .is_some_and(|node| node.kind == syntax_kind_ext::CALL_EXPRESSION)
            && tsz_solver::visitor::application_id(interner, type_id).is_some()
    }

    pub(in crate::declaration_emitter) fn widened_inferred_expression_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        if let Some(sym_id) = self.value_reference_symbol(expr_idx)
            && self.symbol_has_unique_symbol_type(sym_id)
        {
            return Some("symbol".to_string());
        }
        let type_id = self
            .get_node_type_or_names(&[expr_idx])
            .or_else(|| self.get_type_via_symbol(expr_idx))?;
        self.type_interner?;
        let widened = self.widen_unique_symbol_value_type_for_dts(type_id, 0);
        (widened != type_id).then(|| self.print_type_id_for_inferred_declaration(widened))
    }

    pub(in crate::declaration_emitter) fn widen_unique_symbol_value_type_for_dts(
        &self,
        type_id: tsz_solver::types::TypeId,
        _depth: usize,
    ) -> tsz_solver::types::TypeId {
        let Some(interner) = self.type_interner else {
            return type_id;
        };
        tsz_solver::visitor::widen_unique_symbol_value_type_for_dts(interner, type_id)
    }

    pub(in crate::declaration_emitter) fn rewrite_exported_import_equals_type_text(
        &self,
        type_text: String,
    ) -> String {
        let visible_aliases = self.visible_import_equals_type_alias_rewrites();
        let type_text = visible_aliases
            .into_iter()
            .fold(type_text, |text, (target, alias)| {
                Self::replace_qualified_type_reference_text(&text, &target, &alias)
            });

        let aliases = self.exported_import_equals_type_alias_rewrites();
        if aliases.is_empty() {
            return type_text;
        }

        aliases
            .into_iter()
            .fold(type_text, |text, (alias, target)| {
                Self::replace_qualified_type_reference_text(&text, &alias, &target)
            })
    }

    pub(in crate::declaration_emitter) fn rewrite_initializer_exported_import_equals_type_text(
        &self,
        initializer: NodeIndex,
        type_text: String,
    ) -> String {
        let type_text = self.rewrite_initializer_import_equals_type_text(initializer, type_text);
        self.rewrite_exported_import_equals_type_text(type_text)
    }

    pub(in crate::declaration_emitter) fn rewrite_initializer_import_equals_type_text(
        &self,
        initializer: NodeIndex,
        type_text: String,
    ) -> String {
        let Some((target, alias)) = self.initializer_import_equals_alias_rewrite(initializer)
        else {
            return type_text;
        };
        Self::replace_qualified_type_reference_prefix_text(&type_text, &target, &alias)
    }

    pub(in crate::declaration_emitter) fn initializer_import_equals_alias_rewrite(
        &self,
        initializer: NodeIndex,
    ) -> Option<(String, String)> {
        let initializer = self.skip_parenthesized_non_null_and_comma(initializer);
        let node = self.arena.get(initializer)?;
        match node.kind {
            k if k == syntax_kind_ext::NEW_EXPRESSION || k == syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.arena.get_call_expr(node)?;
                self.expression_import_equals_alias_rewrite(call.expression)
            }
            _ => self.expression_import_equals_alias_rewrite(initializer),
        }
    }

    pub(in crate::declaration_emitter) fn expression_import_equals_alias_rewrite(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<(String, String)> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let node = self.arena.get(expr_idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                self.import_equals_alias_target_text_for_identifier(expr_idx)
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(node)?;
                self.expression_import_equals_alias_rewrite(access.expression)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn import_equals_alias_target_text_for_identifier(
        &self,
        ident_idx: NodeIndex,
    ) -> Option<(String, String)> {
        let binder = self.binder?;
        let ident_node = self.arena.get(ident_idx)?;
        if ident_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let alias_name = self.get_identifier_text(ident_idx)?;
        let scope_id = binder.find_enclosing_scope(self.arena, ident_idx)?;
        let sym_id = self.resolve_name_in_scope_chain(binder, scope_id, &alias_name)?;
        let symbol = binder.symbols.get(sym_id)?;
        if symbol.flags & tsz_binder::symbol_flags::ALIAS == 0 {
            return None;
        }
        let import_idx = symbol.declarations.iter().copied().find(|&decl_idx| {
            self.arena
                .get(decl_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
        })?;
        let import_node = self.arena.get(import_idx)?;
        let import_decl = self.arena.get_import_decl(import_node)?;
        let target_text = self.entity_name_text(import_decl.module_specifier)?;
        if target_text == alias_name
            || self
                .arena
                .get(import_decl.module_specifier)
                .is_some_and(|node| node.kind == SyntaxKind::StringLiteral as u16)
        {
            return None;
        }
        Some((target_text, alias_name))
    }

    pub(in crate::declaration_emitter) fn visible_import_equals_type_alias_rewrites(
        &self,
    ) -> Vec<(String, String)> {
        let Some(source_file_idx) = self.current_source_file_idx else {
            return Vec::new();
        };
        let Some(source_file_node) = self.arena.get(source_file_idx) else {
            return Vec::new();
        };
        let Some(source_file) = self.arena.get_source_file(source_file_node) else {
            return Vec::new();
        };

        let current_namespace_path = self.current_namespace_symbol_path();
        let mut aliases = Vec::new();
        self.collect_visible_import_equals_type_aliases(
            &source_file.statements,
            &mut Vec::new(),
            &current_namespace_path,
            &mut aliases,
        );
        aliases.sort_by_key(|(target, _)| std::cmp::Reverse(target.len()));
        aliases.dedup();
        aliases
    }

    pub(in crate::declaration_emitter) fn current_namespace_symbol_path(&self) -> Vec<String> {
        let (Some(binder), Some(mut current)) = (self.binder, self.enclosing_namespace_symbol)
        else {
            return Vec::new();
        };

        let mut path = Vec::new();
        for _ in 0..20 {
            let Some(symbol) = binder.symbols.get(current) else {
                break;
            };
            if !symbol.escaped_name.starts_with("__") {
                path.push(symbol.escaped_name.clone());
            }
            if !symbol.parent.is_some() {
                break;
            }
            current = symbol.parent;
        }
        path.reverse();
        path
    }

    pub(in crate::declaration_emitter) fn collect_visible_import_equals_type_aliases(
        &self,
        statements: &NodeList,
        namespace_path: &mut Vec<String>,
        current_namespace_path: &[String],
        aliases: &mut Vec<(String, String)>,
    ) {
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                self.collect_visible_import_equals_type_aliases_in_module(
                    stmt_node,
                    namespace_path,
                    current_namespace_path,
                    aliases,
                );
                continue;
            }

            if stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                && namespace_path.as_slice() == current_namespace_path
            {
                self.collect_visible_import_equals_type_alias(stmt_idx, aliases);
                continue;
            }

            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_decl) = self.arena.get_export_decl(stmt_node)
                && let Some(clause_node) = self.arena.get(export_decl.export_clause)
            {
                if clause_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                    self.collect_visible_import_equals_type_aliases_in_module(
                        clause_node,
                        namespace_path,
                        current_namespace_path,
                        aliases,
                    );
                } else if clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    && namespace_path.as_slice() == current_namespace_path
                {
                    self.collect_visible_import_equals_type_alias(
                        export_decl.export_clause,
                        aliases,
                    );
                }
            }
        }
    }

    pub(in crate::declaration_emitter) fn collect_visible_import_equals_type_aliases_in_module(
        &self,
        module_node: &Node,
        namespace_path: &mut Vec<String>,
        current_namespace_path: &[String],
        aliases: &mut Vec<(String, String)>,
    ) {
        let Some(module) = self.arena.get_module(module_node) else {
            return;
        };
        let Some(module_name) = self.entity_name_text(module.name) else {
            return;
        };

        let old_len = namespace_path.len();
        namespace_path.extend(module_name.split('.').map(ToString::to_string));

        if current_namespace_path.starts_with(namespace_path.as_slice())
            && let Some(body_node) = self.arena.get(module.body)
        {
            if self.arena.get_module(body_node).is_some() {
                self.collect_visible_import_equals_type_aliases_in_module(
                    body_node,
                    namespace_path,
                    current_namespace_path,
                    aliases,
                );
            } else if let Some(block) = self.arena.get_module_block(body_node)
                && let Some(statements) = block.statements.as_ref()
            {
                self.collect_visible_import_equals_type_aliases(
                    statements,
                    namespace_path,
                    current_namespace_path,
                    aliases,
                );
            }
        }

        namespace_path.truncate(old_len);
    }

    pub(in crate::declaration_emitter) fn collect_visible_import_equals_type_alias(
        &self,
        import_idx: NodeIndex,
        aliases: &mut Vec<(String, String)>,
    ) {
        let Some(import_node) = self.arena.get(import_idx) else {
            return;
        };
        let Some(import_decl) = self.arena.get_import_decl(import_node) else {
            return;
        };
        let Some(alias_name) = self.get_identifier_text(import_decl.import_clause) else {
            return;
        };
        let Some(target_text) = self.entity_name_text(import_decl.module_specifier) else {
            return;
        };
        if target_text == alias_name
            || self
                .arena
                .get(import_decl.module_specifier)
                .is_some_and(|node| node.kind == SyntaxKind::StringLiteral as u16)
        {
            return;
        }

        aliases.push((target_text, alias_name));
    }

    pub(in crate::declaration_emitter) fn exported_import_equals_type_alias_rewrites(
        &self,
    ) -> Vec<(String, String)> {
        let Some(source_file_idx) = self.current_source_file_idx else {
            return Vec::new();
        };
        let Some(source_file_node) = self.arena.get(source_file_idx) else {
            return Vec::new();
        };
        let Some(source_file) = self.arena.get_source_file(source_file_node) else {
            return Vec::new();
        };

        let mut aliases = Vec::new();
        self.collect_exported_import_equals_type_aliases(
            &source_file.statements,
            &mut Vec::new(),
            &mut aliases,
        );
        aliases.sort_by_key(|(alias, _)| std::cmp::Reverse(alias.len()));
        aliases.dedup();
        aliases
    }

    pub(in crate::declaration_emitter) fn collect_exported_import_equals_type_aliases(
        &self,
        statements: &NodeList,
        namespace_path: &mut Vec<String>,
        aliases: &mut Vec<(String, String)>,
    ) {
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                self.collect_exported_import_equals_type_aliases_in_module(
                    stmt_node,
                    namespace_path,
                    aliases,
                );
                continue;
            }

            if stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                self.collect_exported_import_equals_type_alias(
                    stmt_idx,
                    namespace_path,
                    aliases,
                    false,
                );
                continue;
            }

            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_decl) = self.arena.get_export_decl(stmt_node)
                && let Some(clause_node) = self.arena.get(export_decl.export_clause)
            {
                if clause_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                    self.collect_exported_import_equals_type_aliases_in_module(
                        clause_node,
                        namespace_path,
                        aliases,
                    );
                } else if clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                    self.collect_exported_import_equals_type_alias(
                        export_decl.export_clause,
                        namespace_path,
                        aliases,
                        true,
                    );
                }
            }
        }
    }

    pub(in crate::declaration_emitter) fn collect_exported_import_equals_type_aliases_in_module(
        &self,
        module_node: &Node,
        namespace_path: &mut Vec<String>,
        aliases: &mut Vec<(String, String)>,
    ) {
        let Some(module) = self.arena.get_module(module_node) else {
            return;
        };
        let Some(module_name) = self.entity_name_text(module.name) else {
            return;
        };

        let old_len = namespace_path.len();
        namespace_path.extend(module_name.split('.').map(ToString::to_string));

        if let Some(body_node) = self.arena.get(module.body) {
            if self.arena.get_module(body_node).is_some() {
                self.collect_exported_import_equals_type_aliases_in_module(
                    body_node,
                    namespace_path,
                    aliases,
                );
            } else if let Some(block) = self.arena.get_module_block(body_node)
                && let Some(statements) = block.statements.as_ref()
            {
                self.collect_exported_import_equals_type_aliases(
                    statements,
                    namespace_path,
                    aliases,
                );
            }
        }

        namespace_path.truncate(old_len);
    }

    pub(in crate::declaration_emitter) fn collect_exported_import_equals_type_alias(
        &self,
        import_idx: NodeIndex,
        namespace_path: &[String],
        aliases: &mut Vec<(String, String)>,
        already_exported: bool,
    ) {
        let Some(import_node) = self.arena.get(import_idx) else {
            return;
        };
        let Some(import_decl) = self.arena.get_import_decl(import_node) else {
            return;
        };
        if !already_exported
            && !self
                .arena
                .has_modifier(&import_decl.modifiers, SyntaxKind::ExportKeyword)
        {
            return;
        }
        let Some(alias_name) = self.get_identifier_text(import_decl.import_clause) else {
            return;
        };
        let Some(target_text) = self.entity_name_text(import_decl.module_specifier) else {
            return;
        };
        if target_text == alias_name
            || self
                .arena
                .get(import_decl.module_specifier)
                .is_some_and(|node| node.kind == SyntaxKind::StringLiteral as u16)
        {
            return;
        }

        // Top-level exported import aliases (`export import xc = x.c;` at the
        // file root) are always in scope wherever the d.ts is consumed, and
        // tsc prefers the alias spelling over the qualified target. Only
        // namespace-local aliases need a target rewrite — when an outer scope
        // references them, the alias name is not in scope, so the printer's
        // qualified path (`m2.m3.c`) must canonicalize back to its target
        // (`x.c`). Skipping the top-level case prevents the rewrite from
        // clobbering a printer output of `xc` with the longer `x.c`.
        if namespace_path.is_empty() {
            return;
        }
        let alias_text = format!("{}.{}", namespace_path.join("."), alias_name);
        aliases.push((alias_text, target_text));
    }

    pub(in crate::declaration_emitter) fn entity_name_text(
        &self,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = self.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self.get_identifier_text(idx);
        }
        if let Some(qualified) = self.arena.get_qualified_name(node) {
            let left = self.entity_name_text(qualified.left)?;
            let right = self.entity_name_text(qualified.right)?;
            return Some(format!("{left}.{right}"));
        }
        if let Some(access) = self.arena.get_access_expr(node) {
            let left = self.entity_name_text(access.expression)?;
            let right = self.entity_name_text(access.name_or_argument)?;
            return Some(format!("{left}.{right}"));
        }
        None
    }

    pub(in crate::declaration_emitter) fn replace_qualified_type_reference_text(
        type_text: &str,
        from: &str,
        to: &str,
    ) -> String {
        let mut out = String::with_capacity(type_text.len());
        let mut search_start = 0;

        while let Some(relative_idx) = type_text[search_start..].find(from) {
            let start = search_start + relative_idx;
            let end = start + from.len();
            out.push_str(&type_text[search_start..start]);
            if Self::is_qualified_type_reference_boundary(type_text, start, end) {
                out.push_str(to);
            } else {
                out.push_str(from);
            }
            search_start = end;
        }

        out.push_str(&type_text[search_start..]);
        out
    }

    pub(in crate::declaration_emitter) fn replace_qualified_type_reference_prefix_text(
        type_text: &str,
        from: &str,
        to: &str,
    ) -> String {
        let mut out = String::with_capacity(type_text.len());
        let mut search_start = 0;

        while let Some(relative_idx) = type_text[search_start..].find(from) {
            let start = search_start + relative_idx;
            let end = start + from.len();
            out.push_str(&type_text[search_start..start]);
            let before = type_text[..start].chars().next_back();
            let after = type_text[end..].chars().next();
            let can_replace = !before.is_some_and(Self::is_qualified_type_reference_part)
                && (after == Some('.')
                    || !after.is_some_and(Self::is_qualified_type_reference_part));
            if can_replace {
                out.push_str(to);
            } else {
                out.push_str(from);
            }
            search_start = end;
        }

        out.push_str(&type_text[search_start..]);
        out
    }

    pub(in crate::declaration_emitter) fn is_qualified_type_reference_boundary(
        type_text: &str,
        start: usize,
        end: usize,
    ) -> bool {
        let before = type_text[..start].chars().next_back();
        let after = type_text[end..].chars().next();
        !before.is_some_and(Self::is_qualified_type_reference_part)
            && !after.is_some_and(Self::is_qualified_type_reference_part)
    }

    pub(in crate::declaration_emitter) const fn is_qualified_type_reference_part(ch: char) -> bool {
        ch == '.' || ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
    }

    pub(in crate::declaration_emitter) fn enum_value_index_access_alias_type_text(
        &self,
        type_text: &str,
    ) -> Option<String> {
        let mut inner = type_text.trim();
        let mut array_suffix = String::new();
        while let Some(next) = inner.strip_suffix("[]") {
            array_suffix.push_str("[]");
            inner = next.trim_end();
        }

        let (alias, key_alias) = inner.split_once("[keyof ")?;
        let alias = alias.trim();
        let key_alias = key_alias.strip_suffix(']')?.trim();
        if alias != key_alias || !Self::is_simple_identifier_text(alias) {
            return None;
        }

        let enum_name = self.typeof_enum_alias_target_name(alias)?;
        Some(format!("{enum_name}{array_suffix}"))
    }

    pub(in crate::declaration_emitter) fn typeof_enum_alias_target_name(
        &self,
        alias: &str,
    ) -> Option<String> {
        let alias_type_node = self.find_local_type_alias_type_node(alias)?;
        let alias_type = self.arena.get(alias_type_node)?;
        if alias_type.kind != syntax_kind_ext::TYPE_QUERY {
            return None;
        }
        let query = self.arena.get_type_query(alias_type)?;
        let enum_name = self.type_reference_name_text(query.expr_name)?;
        self.local_enum_declaration_exists(&enum_name)
            .then_some(enum_name)
    }

    pub(in crate::declaration_emitter) fn local_enum_declaration_exists(&self, name: &str) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(symbol) = binder
            .file_locals
            .get(name)
            .or_else(|| binder.current_scope.get(name))
        else {
            return false;
        };
        let Some(symbol_data) = binder.symbols.get(symbol) else {
            return false;
        };
        symbol_data.declarations.iter().copied().any(|decl_idx| {
            self.arena
                .get(decl_idx)
                .is_some_and(|node| self.arena.get_enum(node).is_some())
        })
    }
}
