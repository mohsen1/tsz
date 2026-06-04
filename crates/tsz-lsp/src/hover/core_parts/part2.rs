impl<'a> HoverProvider<'a> {
    /// Get the tsserver-compatible kind string for the symbol.
    fn get_tsserver_kind(&self, symbol: &tsz_binder::Symbol, decl_node_idx: NodeIndex) -> String {
        use tsz_binder::symbol_flags;
        let f = symbol.flags;

        if f & symbol_flags::ALIAS != 0 {
            return "alias".to_string();
        }
        if f & symbol_flags::FUNCTION != 0 {
            return "function".to_string();
        }
        if f & symbol_flags::CLASS != 0 {
            return "class".to_string();
        }
        if f & symbol_flags::INTERFACE != 0 {
            return "interface".to_string();
        }
        if f & symbol_flags::ENUM != 0 {
            return "enum".to_string();
        }
        if f & symbol_flags::TYPE_ALIAS != 0 {
            return "type".to_string();
        }
        if f & symbol_flags::ENUM_MEMBER != 0 {
            return "enum member".to_string();
        }
        if f & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE) != 0 {
            return "module".to_string();
        }
        if f & symbol_flags::METHOD != 0 {
            return "method".to_string();
        }
        if f & symbol_flags::CONSTRUCTOR != 0 {
            return "constructor".to_string();
        }
        if f & symbol_flags::PROPERTY != 0 {
            return "property".to_string();
        }
        if f & symbol_flags::TYPE_PARAMETER != 0 {
            return "type parameter".to_string();
        }
        if f & (symbol_flags::GET_ACCESSOR | symbol_flags::SET_ACCESSOR) != 0 {
            // Use declaration node kind to distinguish when both flags are set
            if decl_node_idx.is_some()
                && let Some(decl_node) = self.arena.get(decl_node_idx)
                && decl_node.kind == tsz_parser::syntax_kind_ext::SET_ACCESSOR
            {
                return "setter".to_string();
            }
            return "getter".to_string();
        }
        if f & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            return self.get_variable_keyword(decl_node_idx).to_string();
        }
        if f & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            if self.is_parameter_declaration(decl_node_idx) {
                return "parameter".to_string();
            }
            return "var".to_string();
        }
        "var".to_string()
    }

    /// Get comma-separated kind modifiers string for tsserver.
    fn get_kind_modifiers(&self, symbol: &tsz_binder::Symbol, decl_node_idx: NodeIndex) -> String {
        use tsz_binder::symbol_flags as sf;
        use tsz_parser::modifier_flags as mf;

        let mut modifiers = Vec::with_capacity(8);

        if symbol.is_exported || symbol.flags & sf::EXPORT_VALUE != 0 {
            modifiers.push("export");
        }
        if symbol.flags & sf::ABSTRACT != 0 {
            modifiers.push("abstract");
        }
        if symbol.flags & sf::STATIC != 0 {
            modifiers.push("static");
        }
        if symbol.flags & sf::PRIVATE != 0 {
            modifiers.push("private");
        }
        if symbol.flags & sf::PROTECTED != 0 {
            modifiers.push("protected");
        }

        if decl_node_idx.is_some()
            && let Some(ext) = self.arena.get_extended(decl_node_idx)
        {
            let mflags = ext.modifier_flags;
            if mflags & mf::AMBIENT != 0 {
                modifiers.push("declare");
            }
            if mflags & mf::ASYNC != 0 {
                modifiers.push("async");
            }
            if mflags & mf::READONLY != 0 {
                modifiers.push("readonly");
            }
            if !modifiers.contains(&"export") && mflags & mf::EXPORT != 0 {
                modifiers.push("export");
            }
            if !modifiers.contains(&"abstract") && mflags & mf::ABSTRACT != 0 {
                modifiers.push("abstract");
            }
        }

        modifiers.join(",")
    }

    /// Determine the variable keyword (const, let, or var) from the declaration node.
    fn get_variable_keyword(&self, decl_node_idx: NodeIndex) -> &'static str {
        use tsz_parser::parser::flags::node_flags;
        use tsz_parser::syntax_kind_ext;

        if decl_node_idx.is_none() {
            return "let";
        }

        let node = match self.arena.get(decl_node_idx) {
            Some(n) => n,
            None => return "let",
        };

        let list_idx = if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            if let Some(ext) = self.arena.get_extended(decl_node_idx) {
                ext.parent
            } else {
                return "let";
            }
        } else if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            decl_node_idx
        } else {
            let flags = node.flags as u32;
            if flags & node_flags::CONST != 0 {
                return "const";
            }
            if flags & node_flags::LET != 0 {
                return "let";
            }
            return "var";
        };

        if let Some(list_node) = self.arena.get(list_idx) {
            let flags = list_node.flags as u32;
            if flags & node_flags::CONST != 0 {
                return "const";
            }
            if flags & node_flags::LET != 0 {
                return "let";
            }
        }

        "let"
    }

    /// Check if a variable declaration is local (inside a function/method body).
    /// TypeScript uses `(local var)`, `(local const)`, `(local let)` for variables
    /// declared inside function bodies, as opposed to module-level declarations.
    fn is_local_variable(&self, decl_node_idx: NodeIndex) -> bool {
        use tsz_parser::syntax_kind_ext;

        if decl_node_idx.is_none() {
            return false;
        }

        // Walk up the parent chain looking for a function-like container
        let mut current = decl_node_idx;
        loop {
            let ext = match self.arena.get_extended(current) {
                Some(e) => e,
                None => return false,
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let parent_node = match self.arena.get(parent_idx) {
                Some(n) => n,
                None => return false,
            };
            match parent_node.kind {
                syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::CONSTRUCTOR
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR => {
                    return true;
                }
                syntax_kind_ext::SOURCE_FILE
                | syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::MODULE_BLOCK => {
                    return false;
                }
                _ => {
                    current = parent_idx;
                }
            }
        }
    }

    /// Check if a declaration node is a parameter.
    fn is_parameter_declaration(&self, decl_node_idx: NodeIndex) -> bool {
        use tsz_parser::syntax_kind_ext;

        if decl_node_idx.is_none() {
            return false;
        }
        if let Some(node) = self.arena.get(decl_node_idx) {
            return node.kind == syntax_kind_ext::PARAMETER;
        }
        false
    }

    /// Pick the declaration that is an ancestor of the hovered node.
    /// When a symbol has multiple declarations (e.g., getter + setter),
    /// this ensures we display the correct kind.
    fn find_best_declaration(
        &self,
        symbol: &tsz_binder::Symbol,
        hovered_node: NodeIndex,
    ) -> NodeIndex {
        if symbol.declarations.len() > 1 {
            for &decl in &symbol.declarations {
                if self.node_is_descendant_of(hovered_node, decl) {
                    return decl;
                }
            }
        }
        symbol.primary_declaration().unwrap_or(NodeIndex::NONE)
    }

    /// Check if `child` is a descendant of `ancestor` in the AST.
    fn node_is_descendant_of(&self, child: NodeIndex, ancestor: NodeIndex) -> bool {
        if ancestor.is_none() || child.is_none() {
            return false;
        }
        let ancestor_node = match self.arena.get(ancestor) {
            Some(n) => n,
            None => return false,
        };
        let child_node = match self.arena.get(child) {
            Some(n) => n,
            None => return false,
        };
        child_node.pos >= ancestor_node.pos && child_node.end <= ancestor_node.end
    }

    /// Get the parent symbol name (for enum members, properties, methods).
    fn get_parent_name(&self, decl_node_idx: NodeIndex) -> Option<String> {
        if decl_node_idx.is_none() {
            return None;
        }
        let ext = self.arena.get_extended(decl_node_idx)?;
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return None;
        }
        let parent_node = self.arena.get(parent_idx)?;
        if let Some(data) = self.arena.get_identifier(parent_node) {
            return Some(self.arena.resolve_identifier_text(data).to_string());
        }
        if let Some(data) = self.arena.get_class(parent_node)
            && let Some(name_node) = self.arena.get(data.name)
            && let Some(id) = self.arena.get_identifier(name_node)
        {
            return Some(self.arena.resolve_identifier_text(id).to_string());
        }
        if let Some(data) = self.arena.get_enum(parent_node)
            && let Some(name_node) = self.arena.get(data.name)
            && let Some(id) = self.arena.get_identifier(name_node)
        {
            return Some(self.arena.resolve_identifier_text(id).to_string());
        }
        if let Some(data) = self.arena.get_interface(parent_node)
            && let Some(name_node) = self.arena.get(data.name)
            && let Some(id) = self.arena.get_identifier(name_node)
        {
            return Some(self.arena.resolve_identifier_text(id).to_string());
        }
        None
    }

    fn namespace_container_name(&self, decl_node_idx: NodeIndex) -> Option<String> {
        use tsz_parser::syntax_kind_ext;

        if !decl_node_idx.is_some() {
            return None;
        }

        let mut names = Vec::new();
        let mut current = decl_node_idx;
        while current.is_some() {
            let parent_idx = self.arena.get_extended(current)?.parent;
            if !parent_idx.is_some() {
                break;
            }
            let parent_node = self.arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module_data) = self.arena.get_module(parent_node)
                && let Some(name_node) = self.arena.get(module_data.name)
                && let Some(name_ident) = self.arena.get_identifier(name_node)
            {
                names.push(self.arena.resolve_identifier_text(name_ident).to_string());
            }
            current = parent_idx;
        }

        if names.is_empty() {
            None
        } else {
            names.reverse();
            Some(names.join("."))
        }
    }

    /// Extract plain documentation text from `JSDoc` (without markdown formatting).
    fn extract_plain_documentation(&self, doc: &str) -> String {
        if doc.is_empty() {
            return String::new();
        }
        let parsed = parse_jsdoc(doc);
        let mut parts = Vec::new();
        if let Some(summary) = parsed.summary.as_ref()
            && !summary.is_empty()
        {
            parts.push(inline_links::expand_to_plain_text(summary));
        }
        // Include relevant tags in plain documentation
        for tag in &parsed.tags {
            match tag.name.as_str() {
                "example" => {
                    if tag.text.is_empty() {
                        parts.push("@example".to_string());
                    } else {
                        parts.push(format!(
                            "@example {}",
                            inline_links::expand_to_plain_text(&tag.text)
                        ));
                    }
                }
                "returns" | "return" if !tag.text.is_empty() => {
                    parts.push(format!(
                        "@returns {}",
                        inline_links::expand_to_plain_text(&tag.text)
                    ));
                }
                "deprecated" => {
                    if tag.text.is_empty() {
                        parts.push("@deprecated".to_string());
                    } else {
                        parts.push(format!(
                            "@deprecated {}",
                            inline_links::expand_to_plain_text(&tag.text)
                        ));
                    }
                }
                "see" if !tag.text.is_empty() => {
                    parts.push(format!(
                        "@see {}",
                        inline_links::expand_to_plain_text(&tag.text)
                    ));
                }
                _ => {}
            }
        }
        if parts.is_empty() {
            inline_links::expand_to_plain_text(doc)
        } else {
            parts.join("\n\n")
        }
    }

    fn format_jsdoc_for_hover(
        &self,
        doc: &str,
        root: NodeIndex,
        anchor: NodeIndex,
    ) -> Option<String> {
        if doc.is_empty() {
            return None;
        }

        let resolve = |name: &str| self.resolve_jsdoc_link_uri(root, anchor, name);
        let parsed = parse_jsdoc(doc);
        if parsed.is_empty() {
            return Some(inline_links::expand_to_markdown_with_resolver(doc, resolve));
        }

        let mut sections = Vec::new();
        if let Some(summary) = parsed.summary.as_ref()
            && !summary.is_empty()
        {
            sections.push(inline_links::expand_to_markdown_with_resolver(
                summary, resolve,
            ));
        }

        if !parsed.params.is_empty() {
            let mut names: Vec<&String> = parsed.params.keys().collect();
            names.sort();
            let mut lines = Vec::new();
            lines.push("Parameters:".to_string());
            for name in names {
                let desc = parsed.params.get(name).map_or("", |s| s.as_str());
                let name_code = format_inline_code(name);
                if desc.is_empty() {
                    lines.push(format!("- {name_code}"));
                } else {
                    lines.push(format!(
                        "- {name_code} {}",
                        inline_links::expand_to_markdown_with_resolver(desc, resolve)
                    ));
                }
            }
            sections.push(lines.join("\n"));
        }

        // Include relevant JSDoc tags
        for tag in &parsed.tags {
            match tag.name.as_str() {
                "returns" if !tag.text.is_empty() => {
                    sections.push(format!(
                        "Returns: {}",
                        inline_links::expand_to_markdown_with_resolver(&tag.text, resolve)
                    ));
                }
                "example" => {
                    // Fenced code blocks render `tag.text` verbatim, so they
                    // do not need inline escaping. Re-fence with enough
                    // backticks to outlast any fence in the example.
                    if tag.text.is_empty() {
                        sections.push("Example:".to_string());
                    } else {
                        let fence: String = "`".repeat(pick_example_fence_length(&tag.text));
                        sections.push(format!("Example:\n{fence}\n{}\n{fence}", tag.text));
                    }
                }
                "deprecated" => {
                    if tag.text.is_empty() {
                        sections.push("**@deprecated**".to_string());
                    } else {
                        sections.push(format!(
                            "**@deprecated** {}",
                            inline_links::expand_to_markdown_with_resolver(&tag.text, resolve)
                        ));
                    }
                }
                "see" if !tag.text.is_empty() => {
                    sections.push(format!(
                        "See: {}",
                        inline_links::expand_to_markdown_with_resolver(&tag.text, resolve)
                    ));
                }
                "throws" | "exception" if !tag.text.is_empty() => {
                    sections.push(format!(
                        "Throws: {}",
                        inline_links::expand_to_markdown_with_resolver(&tag.text, resolve)
                    ));
                }
                "since" if !tag.text.is_empty() => {
                    sections.push(format!(
                        "Since: {}",
                        inline_links::expand_to_markdown_with_resolver(&tag.text, resolve)
                    ));
                }
                _ => {}
            }
        }

        let formatted = sections.join("\n\n");
        if formatted.is_empty() {
            Some(inline_links::expand_to_markdown_with_resolver(doc, resolve))
        } else {
            Some(formatted)
        }
    }

    fn resolve_jsdoc_link_uri(
        &self,
        root: NodeIndex,
        anchor: NodeIndex,
        name: &str,
    ) -> Option<String> {
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id = walker.resolve_name_at(root, anchor, name)?;
        let symbol = self.binder.symbols.get(symbol_id)?;
        let decl_idx = symbol.primary_declaration()?;
        if !self.declaration_belongs_to_current_arena(symbol_id, decl_idx) {
            return None;
        }
        let decl_node = self.arena.get(decl_idx)?;
        let source_len = self.source_text.len() as u32;
        if decl_node.pos > source_len
            || decl_node.end > source_len
            || decl_node.pos == decl_node.end
        {
            return None;
        }

        let pos = self
            .line_map
            .offset_to_position(decl_node.pos, self.source_text);
        Some(format!(
            "{}#L{},{}",
            self.markdown_file_uri(),
            pos.line.saturating_add(1),
            pos.character.saturating_add(1)
        ))
    }

    fn declaration_belongs_to_current_arena(
        &self,
        symbol_id: tsz_binder::SymbolId,
        decl_idx: NodeIndex,
    ) -> bool {
        self.binder
            .declaration_arenas
            .get(&(symbol_id, decl_idx))
            .is_none_or(|arenas| {
                arenas
                    .iter()
                    .any(|arena| std::ptr::eq(std::sync::Arc::as_ptr(arena), self.arena))
            })
    }

    fn markdown_file_uri(&self) -> String {
        if self.file_name.starts_with("file://") {
            self.file_name.clone()
        } else {
            format!("file://{}", self.file_name)
        }
    }
}
