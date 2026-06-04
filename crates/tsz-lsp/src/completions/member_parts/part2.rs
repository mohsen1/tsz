impl<'a> Completions<'a> {
    fn add_property_completion_ex(
        &self,
        props: &mut FxHashMap<String, PropertyCompletion>,
        interner: &TypeInterner,
        name: String,
        type_id: TypeId,
        is_method: bool,
        is_optional: bool,
    ) {
        if let Some(existing) = props.get_mut(&name) {
            if existing.type_id != type_id {
                existing.type_id = interner.union(vec![existing.type_id, type_id]);
            }
            existing.is_method |= is_method;
        } else {
            props.insert(
                name,
                PropertyCompletion {
                    type_id,
                    is_method,
                    is_optional,
                },
            );
        }
    }

    /// Suggest properties for object literals based on contextual type.
    /// When typing inside `{ | }`, suggests properties from the expected type.
    pub(super) fn get_object_literal_completions(
        &self,
        node_idx: NodeIndex,
        offset: u32,
        type_cache: Option<&mut Option<TypeCache>>,
    ) -> Option<Vec<CompletionItem>> {
        let interner = self.interner?;

        // 1. Find the enclosing object literal
        let object_literal_idx = self.find_enclosing_object_literal(node_idx, offset)?;

        // 2. Determine the contextual type (expected type)
        let mut cache_ref = type_cache;
        let mut checker = self.make_checker(cache_ref.as_deref_mut())?;

        let context_type = self.get_contextual_type(object_literal_idx, &mut checker)?;

        // 3. Find properties already defined in this literal
        let existing_props = self.get_defined_properties(object_literal_idx);

        // 4. Collect properties from the expected type
        let mut items = Vec::new();
        let mut props: FxHashMap<String, PropertyCompletion> = FxHashMap::default();
        let mut visited = FxHashSet::default();

        self.collect_properties_for_type(
            context_type,
            interner,
            &mut checker,
            &mut visited,
            &mut props,
        );
        let in_string_property_name_context =
            self.is_string_property_name_completion_context(node_idx);

        for (name, info) in props {
            if !in_string_property_name_context && existing_props.contains(&name) {
                continue;
            }

            let kind = if info.is_method {
                CompletionItemKind::Method
            } else {
                CompletionItemKind::Property
            };

            let needs_quoted_label =
                !in_string_property_name_context && !Self::is_valid_unquoted_property_name(&name);
            let label = if needs_quoted_label {
                format!("\"{name}\"")
            } else {
                name.clone()
            };

            let mut item = CompletionItem::new(label, kind);
            item = item.with_detail(checker.format_type(info.type_id));
            if info.is_optional {
                item.sort_text = Some(sort_priority::OPTIONAL_MEMBER.to_string());
                item.kind_modifiers = Some("optional".to_string());
            } else {
                item.sort_text = Some(sort_priority::MEMBER.to_string());
            }

            // Add snippet insert text for method completions in object literals
            if info.is_method {
                item.insert_text = Some(format!("{name}($1)"));
                item.is_snippet = true;
            }

            items.push(item);
        }

        if let Some(cache) = cache_ref {
            *cache = Some(checker.extract_cache());
        }

        if items.is_empty() {
            None
        } else {
            items.sort_by(|a, b| a.label.cmp(&b.label));
            Some(items)
        }
    }

    pub(super) fn is_string_property_name_completion_context(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        let mut depth = 0usize;
        while current.is_some() && depth < 8 {
            let Some(node) = self.arena.get(current) else {
                break;
            };
            if node.kind == SyntaxKind::StringLiteral as u16
                || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            {
                let Some(ext) = self.arena.get_extended(current) else {
                    break;
                };
                let Some(parent) = self.arena.get(ext.parent) else {
                    break;
                };
                if parent.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    && let Some(prop) = self.arena.get_property_assignment(parent)
                    && prop.name == current
                {
                    return true;
                }
            }
            let Some(ext) = self.arena.get_extended(current) else {
                break;
            };
            if ext.parent == current {
                break;
            }
            current = ext.parent;
            depth += 1;
        }
        false
    }

    fn is_valid_unquoted_property_name(name: &str) -> bool {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
            return false;
        }
        chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    /// Find the enclosing object literal expression for a given node.
    fn find_enclosing_object_literal(&self, node_idx: NodeIndex, offset: u32) -> Option<NodeIndex> {
        let node = self.arena.get(node_idx)?;

        // Cursor is directly on the literal (e.g. empty {})
        if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return Some(node_idx);
        }

        // Cursor is on a child (identifier, property, etc.)
        let ext = self.arena.get_extended(node_idx)?;
        let parent = self.arena.get(ext.parent)?;

        if parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return Some(ext.parent);
        }

        // Cursor is deep (e.g. inside a property assignment value)
        // Handle { prop: | } or { prop }
        if parent.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
            let grand_ext = self.arena.get_extended(ext.parent)?;
            let grand_parent = self.arena.get(grand_ext.parent)?;
            if grand_parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return Some(grand_ext.parent);
            }
        }

        // Also check for shorthand property assignment
        if parent.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
            let grand_ext = self.arena.get_extended(ext.parent)?;
            let grand_parent = self.arena.get(grand_ext.parent)?;
            if grand_parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return Some(grand_ext.parent);
            }
        }

        // General fallback: walk ancestors and pick the nearest object literal.
        let mut current = node_idx;
        let mut depth = 0usize;
        while current.is_some() && depth < 64 {
            let Some(current_node) = self.arena.get(current) else {
                break;
            };
            if current_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return Some(current);
            }
            let Some(current_ext) = self.arena.get_extended(current) else {
                break;
            };
            if current_ext.parent == current {
                break;
            }
            current = current_ext.parent;
            depth += 1;
        }

        // Fallback: choose smallest object literal containing the cursor offset.
        let mut best = None;
        let mut best_len = u32::MAX;
        for (i, n) in self.arena.nodes.iter().enumerate() {
            if n.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                continue;
            }
            if n.pos <= offset && offset <= n.end {
                let len = n.end.saturating_sub(n.pos);
                if len < best_len {
                    best_len = len;
                    best = Some(NodeIndex(i as u32));
                }
            }
        }
        if best.is_some() {
            return best;
        }

        None
    }

    pub(super) fn find_enclosing_class_declaration(
        &self,
        node_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = node_idx;
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.is_class_like() {
                return Some(current);
            }
            // Stop at regular function boundaries — `function() {}` resets
            // `this` binding, so `this.` inside a function expression/declaration
            // doesn't refer to the enclosing class.
            // Arrow functions do NOT reset `this`, so we continue past them.
            if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            {
                return None;
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }
        None
    }

    /// Check if a class member node has the `static` modifier.
    fn has_static_modifier_node(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(member_idx) else {
            return false;
        };
        let modifiers = if node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
            self.arena
                .get_property_decl(node)
                .and_then(|d| d.modifiers.as_ref())
        } else if node.kind == syntax_kind_ext::METHOD_DECLARATION {
            self.arena
                .get_method_decl(node)
                .and_then(|d| d.modifiers.as_ref())
        } else {
            None
        };
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::StaticKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }

    fn node_type_detail(checker: &mut CheckerState, node_idx: NodeIndex) -> Option<String> {
        let type_id = checker.get_type_of_node(node_idx);
        let detail = checker.format_type(type_id);
        if detail.is_empty() {
            return None;
        }
        // Completion details use colon notation `(params): RetType`, not arrow `(params) => RetType`.
        Some(crate::hover::format::arrow_to_colon(&detail))
    }

    pub(super) fn class_extends_expression(&self, class_idx: NodeIndex) -> Option<NodeIndex> {
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;
        let clauses = class_data.heritage_clauses.as_ref()?;
        for &clause_idx in &clauses.nodes {
            let clause_node = self.arena.get(clause_idx)?;
            let heritage = self.arena.get_heritage_clause(clause_node)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            for &type_idx in &heritage.types.nodes {
                let type_node = self.arena.get(type_idx)?;
                if let Some(expr_with_type_args) = self.arena.get_expr_type_args(type_node) {
                    return Some(expr_with_type_args.expression);
                }
            }
        }
        None
    }

    pub(super) fn class_declared_member_names(&self, class_idx: NodeIndex) -> FxHashSet<String> {
        let mut names = FxHashSet::default();
        let Some(class_node) = self.arena.get(class_idx) else {
            return names;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return names;
        };

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let name_idx = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    self.arena.get_method_decl(member_node).map(|m| m.name)
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    self.arena.get_property_decl(member_node).map(|m| m.name)
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.arena.get_accessor(member_node).map(|m| m.name)
                }
                _ => None,
            };
            if let Some(name_idx) = name_idx
                && let Some(name) = self.arena.get_identifier_text(name_idx)
            {
                names.insert(name.to_string());
            }
        }

        names
    }

    /// Get the set of property names already defined in an object literal.
    fn get_defined_properties(&self, object_literal_idx: NodeIndex) -> FxHashSet<String> {
        let mut names = FxHashSet::default();
        let node = self
            .arena
            .get(object_literal_idx)
            .expect("object_literal_idx must be valid in arena");

        if let Some(lit) = self.arena.get_literal_expr(node) {
            for &prop_idx in &lit.elements.nodes {
                if let Some(name) = self.get_property_name(prop_idx) {
                    names.insert(name);
                }
            }
        }
        names
    }

    /// Extract the property name from a property assignment or shorthand.
    fn get_property_name(&self, prop_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(prop_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(node)?;
                self.arena
                    .get_identifier_text(prop.name)
                    .map(std::string::ToString::to_string)
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_shorthand_property(node)?;
                self.arena
                    .get_identifier_text(prop.name)
                    .map(std::string::ToString::to_string)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl(node)?;
                self.arena
                    .get_identifier_text(method.name)
                    .map(std::string::ToString::to_string)
            }
            _ => None,
        }
    }

    /// Walk up the AST to find the expected/contextual type for a node.
    pub(super) fn get_contextual_type(
        &self,
        node_idx: NodeIndex,
        checker: &mut CheckerState,
    ) -> Option<TypeId> {
        let ext = self.arena.get_extended(node_idx)?;
        let parent_idx = ext.parent;
        let parent = self.arena.get(parent_idx)?;

        match parent.kind {
            // const x: Type = { ... }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                let decl = self.arena.get_variable_declaration(parent)?;
                if decl.initializer == node_idx && decl.type_annotation.is_some() {
                    return Some(checker.get_type_of_node(decl.type_annotation));
                }
            }
            // { prop: { ... } } -> Recurse to parent object
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(parent)?;
                if prop.initializer == node_idx {
                    let grand_parent_ext = self.arena.get_extended(parent_idx)?;
                    let grand_parent_idx = grand_parent_ext.parent;

                    // Get context of the parent object
                    let parent_context = self.get_contextual_type(grand_parent_idx, checker)?;

                    // Look up this property in the parent context
                    let prop_name = self.arena.get_identifier_text(prop.name)?;
                    return self.lookup_property_type(parent_context, prop_name, checker);
                }
            }
            // return { ... }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let func_idx = self.find_enclosing_function(parent_idx)?;
                let func_node = self.arena.get(func_idx)?;

                // Check return type annotation
                if let Some(func) = self.arena.get_function(func_node)
                    && func.type_annotation.is_some()
                {
                    return Some(checker.get_type_of_node(func.type_annotation));
                }
            }
            // function call argument: foo({ ... })
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.arena.get_call_expr(parent)?;
                // Find which argument position this node is at
                let arg_index = call
                    .arguments
                    .as_ref()
                    .and_then(|args| args.nodes.iter().position(|&arg| arg == node_idx));

                if let Some(arg_idx) = arg_index {
                    // Get the function signature type
                    let func_type = checker.get_type_of_node(call.expression);
                    if let Some(param_type) =
                        self.get_parameter_type_at(func_type, arg_idx, checker)
                    {
                        return Some(param_type);
                    }

                    if let Some(sym_id) = self.resolve_member_target_symbol(call.expression) {
                        let symbol_type = checker.get_type_of_symbol(sym_id);
                        if let Some(param_type) =
                            self.get_parameter_type_at(symbol_type, arg_idx, checker)
                        {
                            return Some(param_type);
                        }
                    }
                }
            }
            // assignment expression: target = { ... }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let binary = self.arena.get_binary_expr(parent)?;
                if binary.right == node_idx
                    && binary.operator_token == SyntaxKind::EqualsToken as u16
                {
                    return Some(checker.get_type_of_node(binary.left));
                }
            }
            _ => {}
        }
        None
    }

    /// Find the type of a property from an object type.
    fn lookup_property_type(
        &self,
        type_id: TypeId,
        name: &str,
        checker: &mut CheckerState,
    ) -> Option<TypeId> {
        let mut props = FxHashMap::default();
        let mut visited = FxHashSet::default();
        let interner = self.interner?;

        self.collect_properties_for_type(type_id, interner, checker, &mut visited, &mut props);
        props.get(name).map(|p| p.type_id)
    }

    /// Find the enclosing function for a node (for return type lookup).
    fn find_enclosing_function(&self, start_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = start_idx;
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || node.kind == syntax_kind_ext::ARROW_FUNCTION
                || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }
        None
    }

    /// Get the type of the Nth parameter of a function type.
    fn get_parameter_type_at(
        &self,
        func_type: TypeId,
        param_index: usize,
        _checker: &mut CheckerState,
    ) -> Option<TypeId> {
        let interner = self.interner?;

        // Look up the callable signature
        if let Some(callable_id) = visitor::callable_shape_id(interner, func_type) {
            let callable = interner.callable_shape(callable_id);
            // Use the first call signature
            if let Some(first_sig) = callable.call_signatures.first()
                && param_index < first_sig.params.len()
            {
                return Some(first_sig.params[param_index].type_id);
            }
        }
        None
    }
}
