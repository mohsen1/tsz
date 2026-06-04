impl<'a> GoToDefinition<'a> {
    /// Get the name node index from a declaration node.
    fn get_declaration_name_idx(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(decl_idx)?;
        match node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let var_decl = self.arena.get_variable_declaration(node)?;
                Some(var_decl.name)
            }
            syntax_kind_ext::FUNCTION_DECLARATION => {
                let func = self.arena.get_function(node)?;
                Some(func.name)
            }
            syntax_kind_ext::CLASS_DECLARATION | syntax_kind_ext::CLASS_EXPRESSION => {
                let class = self.arena.get_class(node)?;
                Some(class.name)
            }
            syntax_kind_ext::INTERFACE_DECLARATION => {
                let iface = self.arena.get_interface(node)?;
                Some(iface.name)
            }
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let type_alias = self.arena.get_type_alias(node)?;
                Some(type_alias.name)
            }
            syntax_kind_ext::ENUM_DECLARATION => {
                let enum_decl = self.arena.get_enum(node)?;
                Some(enum_decl.name)
            }
            syntax_kind_ext::MODULE_DECLARATION => {
                let module = self.arena.get_module(node)?;
                Some(module.name)
            }
            syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl(node)?;
                Some(method.name)
            }
            syntax_kind_ext::PROPERTY_DECLARATION => {
                let prop = self.arena.get_property_decl(node)?;
                Some(prop.name)
            }
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.arena.get_accessor(node)?;
                Some(accessor.name)
            }
            syntax_kind_ext::ENUM_MEMBER => {
                let member = self.arena.get_enum_member(node)?;
                Some(member.name)
            }
            syntax_kind_ext::PARAMETER => {
                let param = self.arena.get_parameter(node)?;
                Some(param.name)
            }
            syntax_kind_ext::IMPORT_SPECIFIER => {
                let spec = self.arena.get_specifier(node)?;
                Some(spec.name)
            }
            syntax_kind_ext::METHOD_SIGNATURE | syntax_kind_ext::PROPERTY_SIGNATURE => {
                let sig = self.arena.get_signature(node)?;
                Some(sig.name)
            }
            syntax_kind_ext::CONSTRUCT_SIGNATURE | syntax_kind_ext::CALL_SIGNATURE => {
                // These don't have meaningful names
                None
            }
            _ => None,
        }
    }

    /// Get the declaration kind string for a specific declaration,
    /// using node info to distinguish const/let/var when needed.
    fn get_declaration_kind(&self, decl_idx: NodeIndex, flags: u32) -> String {
        use tsz_parser::parser::flags::node_flags;

        // Check if the declaration node is a parameter
        if let Some(decl_node) = self.arena.get(decl_idx)
            && decl_node.kind == syntax_kind_ext::PARAMETER
        {
            return "parameter".to_string();
        }

        // For block-scoped variables, check if const
        if flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            // Walk up to VariableDeclarationList to check CONST flag
            if let Some(ext) = self.arena.get_extended(decl_idx) {
                let parent_idx = ext.parent; // VariableDeclarationList
                if parent_idx.is_some()
                    && let Some(parent_node) = self.arena.get(parent_idx)
                    && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                    && parent_node.flags as u32 & node_flags::CONST != 0
                {
                    return "const".to_string();
                }
            }
            return "let".to_string();
        }

        // For function-scoped variables, also check for parameter
        if flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            // Check if this specific declaration is a parameter
            if let Some(node) = self.arena.get(decl_idx)
                && node.kind == syntax_kind_ext::PARAMETER
            {
                return "parameter".to_string();
            }
            // Check if var is inside a function body -> "local var"
            if self.is_inside_function_body(decl_idx) {
                return "local var".to_string();
            }
            return "var".to_string();
        }

        let base_kind = self.symbol_flags_to_kind_string(flags);
        // For functions inside function bodies, use "local function"
        if base_kind == "function" && self.is_inside_function_body(decl_idx) {
            return "local function".to_string();
        }
        // For classes inside function bodies, use "local class"
        if base_kind == "class" && self.is_inside_function_body(decl_idx) {
            return "local class".to_string();
        }
        base_kind
    }

    /// Convert symbol flags to a tsserver-compatible kind string.
    pub fn symbol_flags_to_kind_string(&self, flags: u32) -> String {
        if flags & symbol_flags::FUNCTION != 0 {
            "function".to_string()
        } else if flags & symbol_flags::CLASS != 0 {
            "class".to_string()
        } else if flags & symbol_flags::INTERFACE != 0 {
            "interface".to_string()
        } else if flags & symbol_flags::TYPE_ALIAS != 0 {
            "type".to_string()
        } else if flags & symbol_flags::ENUM != 0 {
            "enum".to_string()
        } else if flags & symbol_flags::ENUM_MEMBER != 0 {
            "enum member".to_string()
        } else if flags & symbol_flags::MODULE != 0 {
            "module".to_string()
        } else if flags & symbol_flags::METHOD != 0 {
            "method".to_string()
        } else if flags & symbol_flags::PROPERTY != 0 {
            "property".to_string()
        } else if flags & symbol_flags::CONSTRUCTOR != 0 {
            "constructor".to_string()
        } else if flags & symbol_flags::GET_ACCESSOR != 0 {
            "getter".to_string()
        } else if flags & symbol_flags::SET_ACCESSOR != 0 {
            "setter".to_string()
        } else if flags & symbol_flags::TYPE_PARAMETER != 0 {
            "type parameter".to_string()
        } else if flags & symbol_flags::ALIAS != 0 {
            "alias".to_string()
        } else if flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            // Could be let or const
            "let".to_string()
        } else if flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            "var".to_string()
        } else {
            "".to_string()
        }
    }

    /// Get the container name and kind for a symbol.
    fn get_container_info(&self, symbol_id: SymbolId) -> (String, String) {
        let symbol = match self.binder.symbols.get(symbol_id) {
            Some(s) => s,
            None => return (String::new(), String::new()),
        };

        // First try symbol.parent (set by binder for enums, lib types)
        if symbol.parent.is_some()
            && let Some(parent_symbol) = self.binder.symbols.get(symbol.parent)
        {
            let parent_kind = self.symbol_flags_to_kind_string(parent_symbol.flags);
            return (parent_symbol.escaped_name.clone(), parent_kind);
        }

        // Fallback: walk AST from first declaration to find containing class/interface/enum
        if let Some(&decl_idx) = symbol.declarations.first() {
            return self.get_container_from_ast(decl_idx);
        }

        (String::new(), String::new())
    }

    /// Get identifier text from a `NodeIndex` using `source_text`.
    fn get_node_text(&self, idx: NodeIndex) -> String {
        if idx.is_none() {
            return String::new();
        }
        if let Some(node) = self.arena.get(idx) {
            let pos = node.pos as usize;
            let end = node.end as usize;
            if pos < end && end <= self.source_text.len() {
                return self.source_text[pos..end].to_string();
            }
        }
        String::new()
    }

    /// Walk up the AST from a declaration node to find the containing class/interface/enum.
    fn get_container_from_ast(&self, decl_idx: NodeIndex) -> (String, String) {
        let mut current = decl_idx;
        for _ in 0..20 {
            if let Some(ext) = self.arena.get_extended(current) {
                let parent = ext.parent;
                if parent.is_none() {
                    break;
                }
                if let Some(parent_node) = self.arena.get(parent) {
                    match parent_node.kind {
                        k if k == syntax_kind_ext::CLASS_DECLARATION
                            || k == syntax_kind_ext::CLASS_EXPRESSION =>
                        {
                            let name = self
                                .arena
                                .get_class(parent_node)
                                .map(|c| self.get_node_text(c.name))
                                .unwrap_or_default();
                            return (name, "class".to_string());
                        }
                        k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                            let name = self
                                .arena
                                .get_interface(parent_node)
                                .map(|i| self.get_node_text(i.name))
                                .unwrap_or_default();
                            return (name, "interface".to_string());
                        }
                        k if k == syntax_kind_ext::ENUM_DECLARATION => {
                            let name = self
                                .arena
                                .get_enum(parent_node)
                                .map(|e| self.get_node_text(e.name))
                                .unwrap_or_default();
                            return (name, "enum".to_string());
                        }
                        k if k == syntax_kind_ext::MODULE_DECLARATION => {
                            let name = self
                                .arena
                                .get_module(parent_node)
                                .map(|m| self.get_node_text(m.name))
                                .unwrap_or_default();
                            return (name, "module".to_string());
                        }
                        _ => {}
                    }
                }
                current = parent;
            } else {
                break;
            }
        }
        (String::new(), String::new())
    }

    /// Check if a declaration is ambient (has `declare` modifier).
    /// Walks up the parent chain to find a node with `declare` in its modifiers.
    fn is_ambient_declaration(&self, decl_idx: NodeIndex) -> bool {
        self.arena.is_in_ambient_context(decl_idx)
    }

    /// Check if a declaration is at the top level of the source file.
    /// Top-level declarations have isLocal = false.
    /// Handle keyword navigation: clicking on `return`, `await`, `yield`, `case`, `default`,
    /// or `switch` navigates to the containing function/switch declaration.
    fn try_keyword_navigation(
        &self,
        node_idx: NodeIndex,
        offset: u32,
    ) -> Option<Vec<DefinitionInfo>> {
        let node = self.arena.get(node_idx)?;
        let node_start = node.pos;
        let keyword_offset = offset - node_start;

        // Check if cursor is on the keyword portion of the node
        let (keyword_len, target_kind) = match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => (6, "function"), // "return"
            syntax_kind_ext::AWAIT_EXPRESSION | syntax_kind_ext::YIELD_EXPRESSION => {
                (5, "function")
            } // "await", "yield"
            syntax_kind_ext::SWITCH_STATEMENT => (6, "switch"),   // "switch"
            syntax_kind_ext::CASE_CLAUSE => (4, "switch"),        // "case"
            syntax_kind_ext::DEFAULT_CLAUSE => (7, "switch"),     // "default"
            _ => return None,
        };

        // Only navigate if cursor is within the keyword text
        if keyword_offset >= keyword_len {
            return None;
        }

        // Walk up to find the containing function or switch
        let target_idx = self.find_containing_declaration(node_idx, target_kind)?;
        let target_node = self.arena.get(target_idx)?;

        // Build a DefinitionInfo pointing to the containing declaration
        let (name_range, context_range) =
            self.compute_name_and_context_spans(target_idx, target_node);

        let line_count = self.line_map.line_count() as u32;
        if name_range.start.line >= line_count {
            return None;
        }

        // Get the name from the target
        let name = if let Some(name_idx) = self.get_declaration_name_idx(target_idx) {
            if name_idx.is_some() {
                self.get_node_text(name_idx)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let kind = match target_node.kind {
            syntax_kind_ext::METHOD_DECLARATION => "method".to_string(),
            syntax_kind_ext::CONSTRUCTOR => "constructor".to_string(),
            syntax_kind_ext::GET_ACCESSOR => "getter".to_string(),
            syntax_kind_ext::SET_ACCESSOR => "setter".to_string(),
            syntax_kind_ext::SWITCH_STATEMENT => "var".to_string(),
            _ => "function".to_string(),
        };

        Some(vec![DefinitionInfo {
            location: Location {
                file_path: self.file_name.clone(),
                range: name_range,
            },
            context_span: Some(context_range),
            name,
            kind,
            container_name: String::new(),
            container_kind: String::new(),
            is_local: false,
            is_ambient: false,
        }])
    }

    /// Find the containing function or switch declaration, walking up the AST.
    fn find_containing_declaration(
        &self,
        start_idx: NodeIndex,
        target_kind: &str,
    ) -> Option<NodeIndex> {
        let mut current = start_idx;
        for _ in 0..30 {
            let ext = self.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            if let Some(parent_node) = self.arena.get(parent) {
                match target_kind {
                    "function" => match parent_node.kind {
                        syntax_kind_ext::FUNCTION_DECLARATION
                        | syntax_kind_ext::FUNCTION_EXPRESSION
                        | syntax_kind_ext::ARROW_FUNCTION
                        | syntax_kind_ext::METHOD_DECLARATION
                        | syntax_kind_ext::CONSTRUCTOR
                        | syntax_kind_ext::GET_ACCESSOR
                        | syntax_kind_ext::SET_ACCESSOR => return Some(parent),
                        _ => {}
                    },
                    "switch" if parent_node.kind == syntax_kind_ext::SWITCH_STATEMENT => {
                        return Some(parent);
                    }
                    _ => {}
                }
            }
            current = parent;
        }
        None
    }

    /// Check if a declaration is inside a function body (not at module/source level).
    fn is_inside_function_body(&self, decl_idx: NodeIndex) -> bool {
        let mut current = decl_idx;
        for _ in 0..30 {
            if let Some(ext) = self.arena.get_extended(current) {
                let parent = ext.parent;
                if parent.is_none() {
                    return false;
                }
                if let Some(parent_node) = self.arena.get(parent) {
                    match parent_node.kind {
                        syntax_kind_ext::SOURCE_FILE => return false,
                        syntax_kind_ext::FUNCTION_DECLARATION
                        | syntax_kind_ext::FUNCTION_EXPRESSION
                        | syntax_kind_ext::ARROW_FUNCTION
                        | syntax_kind_ext::METHOD_DECLARATION
                        | syntax_kind_ext::CONSTRUCTOR
                        | syntax_kind_ext::GET_ACCESSOR
                        | syntax_kind_ext::SET_ACCESSOR => return true,
                        _ => {
                            current = parent;
                        }
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        false
    }

    /// Check if a declaration is a member of a class or interface.
    fn is_class_or_interface_member(&self, decl_idx: NodeIndex) -> bool {
        if let Some(ext) = self.arena.get_extended(decl_idx) {
            let parent = ext.parent;
            if parent.is_some()
                && let Some(parent_node) = self.arena.get(parent)
            {
                let k = parent_node.kind;
                return k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION
                    || k == syntax_kind_ext::INTERFACE_DECLARATION;
            }
        }
        false
    }

    fn is_top_level_declaration(&self, decl_idx: NodeIndex) -> bool {
        let mut current = decl_idx;
        // Walk up through the parent chain looking for source file
        for _ in 0..20 {
            if let Some(ext) = self.arena.get_extended(current) {
                let parent = ext.parent;
                if parent.is_none() {
                    return true; // Reached root
                }
                if let Some(parent_node) = self.arena.get(parent) {
                    match parent_node.kind {
                        syntax_kind_ext::SOURCE_FILE => return true,
                        // Transparent containers - keep walking up
                        syntax_kind_ext::VARIABLE_DECLARATION_LIST
                        | syntax_kind_ext::VARIABLE_STATEMENT
                        | syntax_kind_ext::CLASS_DECLARATION
                        | syntax_kind_ext::CLASS_EXPRESSION
                        | syntax_kind_ext::INTERFACE_DECLARATION
                        | syntax_kind_ext::ENUM_DECLARATION
                        | syntax_kind_ext::MODULE_DECLARATION
                        | syntax_kind_ext::MODULE_BLOCK => {
                            current = parent;
                            continue;
                        }
                        // If we hit a function/method/class body, it's local
                        _ => return false,
                    }
                }
                current = parent;
            } else {
                break;
            }
        }
        false
    }
}
