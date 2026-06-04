impl<'a> GoToDefinition<'a> {
    /// Get the definition location(s) for the symbol at the given position.
    ///
    /// Returns a list of locations because a symbol can have multiple declarations
    /// (e.g., function overloads, merged declarations).
    ///
    /// Returns None if no symbol is found at the position.
    pub fn get_definition(&self, root: NodeIndex, position: Position) -> Option<Vec<Location>> {
        self.get_definition_internal(root, position, None, None)
    }

    pub fn get_definition_with_scope_cache(
        &self,
        root: NodeIndex,
        position: Position,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        self.get_definition_internal(root, position, Some(scope_cache), scope_stats)
    }

    fn get_definition_internal(
        &self,
        root: NodeIndex,
        position: Position,
        scope_cache: Option<&mut ScopeCache>,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        // 1. Convert position to byte offset
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        // 2. Find the most specific node at this offset (or nearest preceding symbol node)
        let mut node_idx = find_node_at_or_before_offset(self.arena, offset, self.source_text);
        if node_idx.is_none()
            && let Some(adjusted) =
                find_symbol_query_node_at_or_before(self.arena, self.source_text, offset)
        {
            node_idx = adjusted;
        }

        if node_idx.is_none() {
            return None;
        }

        // Only accept the backtracked node if it ends at or very near the cursor.
        // This prevents jumping from e.g. a semicolon after `1;` back to an
        // identifier like `x` that is several tokens earlier.
        if !is_symbol_query_node(self.arena, node_idx)
            && (is_comment_context(self.source_text, offset)
                || should_backtrack_to_previous_symbol(self.source_text, offset))
            && let Some(adjusted) =
                find_symbol_query_node_at_or_before(self.arena, self.source_text, offset)
            && let Some(adj_node) = self.arena.get(adjusted)
            && (adj_node.end >= offset || offset.saturating_sub(adj_node.end) <= 1)
        {
            node_idx = adjusted;
        }

        if !is_symbol_query_node(self.arena, node_idx) {
            return None;
        }

        // 2a. Skip keyword literals and built-in identifiers with no user definition
        if self.is_builtin_node(node_idx) {
            return None;
        }

        // 3. Resolve the node to a symbol via scope walking
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id_opt = if let Some(scope_cache) = scope_cache {
            walker.resolve_node_cached(root, node_idx, scope_cache, scope_stats)
        } else {
            walker.resolve_node(root, node_idx)
        };

        // 4. If primary resolution succeeded, use the symbol
        //    But skip class/interface members resolved via scope chain for bare identifiers
        //    (they require `this.` qualification and shouldn't resolve as lexical names).
        //    Also skip for `super.member` — let the member access fallback handle it
        //    to resolve to the base class, not the overriding derived class.
        if !self.is_super_member_access(node_idx)
            && let Some(symbol_id) = symbol_id_opt
            && !self.is_bare_class_member_reference(node_idx, symbol_id)
            && let Some(locations) = self.locations_from_symbol(symbol_id)
        {
            return Some(locations);
        }

        // 5. Fallback: try member access resolution (obj.method, Class.staticProp, super.method)
        if let Some(locations) = self.try_member_access_fallback(root, node_idx) {
            return Some(locations);
        }

        // 6. Fallback: try file_locals lookup by identifier text
        if let Some(locations) = self.try_file_locals_fallback(node_idx) {
            return Some(locations);
        }

        None
    }

    /// Get the definition location for a specific node (by `NodeIndex`).
    ///
    /// This is useful when you already have the node index from another operation.
    pub fn get_definition_for_node(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
    ) -> Option<Vec<Location>> {
        self.get_definition_for_node_internal(root, node_idx, None, None)
    }

    pub fn get_definition_for_node_with_scope_cache(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        self.get_definition_for_node_internal(root, node_idx, Some(scope_cache), scope_stats)
    }

    fn get_definition_for_node_internal(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
        scope_cache: Option<&mut ScopeCache>,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        if node_idx.is_none() {
            return None;
        }

        // Skip keyword literals and built-in identifiers
        if self.is_builtin_node(node_idx) {
            return None;
        }

        // Resolve the node to a symbol
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id_opt = if let Some(scope_cache) = scope_cache {
            walker.resolve_node_cached(root, node_idx, scope_cache, scope_stats)
        } else {
            walker.resolve_node(root, node_idx)
        };

        // If primary resolution succeeded, use the symbol
        if let Some(symbol_id) = symbol_id_opt
            && let Some(locations) = self.locations_from_symbol(symbol_id)
        {
            return Some(locations);
        }

        // Fallback: try file_locals
        if let Some(locations) = self.try_file_locals_fallback(node_idx) {
            return Some(locations);
        }

        None
    }

    /// Convert a symbol's declarations into validated Location objects.
    ///
    /// This validates that declaration positions are within the source text bounds
    /// to prevent crashes when declarations point to other files or invalid positions.
    fn locations_from_symbol(&self, symbol_id: SymbolId) -> Option<Vec<Location>> {
        let symbol = self.binder.symbols.get(symbol_id)?;
        let source_len = self.source_text.len() as u32;

        let locations: Vec<Location> = symbol
            .declarations
            .iter()
            .filter_map(|&decl_idx| {
                let decl_node = self.arena.get(decl_idx)?;

                // Validate that positions are within the current file's bounds.
                // Declarations from other files (cross-file references, built-ins)
                // will have node indices that either don't exist in this arena or
                // have positions outside this file's text range.
                if decl_node.pos > source_len || decl_node.end > source_len {
                    return None;
                }
                if decl_node.end < decl_node.pos {
                    return None;
                }
                // Skip zero-width declarations - these are synthetic/placeholder
                // declarations for built-in globals (undefined, null, etc.)
                if decl_node.pos == decl_node.end {
                    return None;
                }

                let start_pos = self
                    .line_map
                    .offset_to_position(decl_node.pos, self.source_text);
                let end_pos = self
                    .line_map
                    .offset_to_position(decl_node.end, self.source_text);

                // Validate computed positions are within the line map bounds
                let line_count = self.line_map.line_count() as u32;
                if start_pos.line >= line_count || end_pos.line >= line_count {
                    return None;
                }

                Some(Location {
                    file_path: self.file_name.clone(),
                    range: Range::new(start_pos, end_pos),
                })
            })
            .collect();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    /// Try to resolve a node's identifier text via the binder's `file_locals` table.
    ///
    /// This serves as a fallback when the scope-based resolution fails (e.g., for
    /// shorthand properties, certain export patterns, etc.)
    fn try_file_locals_fallback(&self, node_idx: NodeIndex) -> Option<Vec<Location>> {
        let node = self.arena.get(node_idx)?;
        let pos = node.pos as usize;
        let end = node.end as usize;
        if end > self.source_text.len() || pos > end {
            return None;
        }

        let text = &self.source_text[pos..end];

        // Skip if this is a built-in global - no definition in user source
        if is_builtin_global(text) {
            return None;
        }

        // Try looking up in file_locals
        let symbol_id = self.binder.file_locals.get(text)?;

        // Skip class/interface members — they aren't lexically scoped and require
        // `this.` qualification. Without this guard, `value` inside a method body
        // would incorrectly resolve to a class property named `value`.
        if let Some(symbol) = self.binder.symbols.get(symbol_id) {
            const CLASS_MEMBER_FLAGS: u32 = symbol_flags::PROPERTY
                | symbol_flags::METHOD
                | symbol_flags::GET_ACCESSOR
                | symbol_flags::SET_ACCESSOR;
            if symbol.flags & CLASS_MEMBER_FLAGS != 0
                && symbol.flags
                    & (symbol_flags::FUNCTION_SCOPED_VARIABLE
                        | symbol_flags::BLOCK_SCOPED_VARIABLE
                        | symbol_flags::FUNCTION
                        | symbol_flags::CLASS
                        | symbol_flags::INTERFACE
                        | symbol_flags::TYPE_ALIAS)
                    == 0
            {
                return None;
            }
        }

        self.locations_from_symbol(symbol_id)
    }

    /// Try to resolve a member access expression (e.g., obj.method, Class.staticProp).
    /// Returns the symbol ID of the member if found.
    fn try_resolve_member_access(&self, root: NodeIndex, node_idx: NodeIndex) -> Option<SymbolId> {
        // Check if the node is the right-hand side of a property access expression
        let ext = self.arena.get_extended(node_idx)?;
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return None;
        }
        let parent_node = self.arena.get(parent_idx)?;
        if parent_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.arena.get_access_expr(parent_node)?;
        // Make sure we're on the name side (right of dot), not the expression side
        if access.name_or_argument != node_idx {
            return None;
        }

        // Get the member name text
        let node = self.arena.get(node_idx)?;
        let member_name = &self.source_text[node.pos as usize..node.end as usize];

        // Resolve the expression (left side) to a symbol.
        // Handle `super` keyword by finding the base class.
        let is_super = self
            .arena
            .get(access.expression)
            .map(|n| n.kind == tsz_scanner::SyntaxKind::SuperKeyword as u16)
            .unwrap_or(false);
        let expr_symbol_id = if is_super {
            self.resolve_super_base_class(access.expression)
        } else {
            let mut walker = ScopeWalker::new(self.arena, self.binder);
            walker.resolve_node(root, access.expression)
        }?;
        let expr_symbol = self.binder.symbols.get(expr_symbol_id)?;

        // Look up in members table (instance members)
        if let Some(ref members) = expr_symbol.members
            && let Some(member_id) = members.get(member_name)
        {
            return Some(member_id);
        }

        // Look up in exports table (static members, namespace exports)
        if let Some(ref exports) = expr_symbol.exports
            && let Some(member_id) = exports.get(member_name)
        {
            return Some(member_id);
        }

        // For instances: resolve the variable's type by checking its declarations.
        // Handles multiple patterns:
        //   1. `var x = new Foo()` → look at the class from the new expression
        //   2. `const x: Foo = ...` → look at the type annotation
        //   3. `function f(x: Foo) { x.member }` → look at the parameter type
        for &decl_idx in &expr_symbol.declarations {
            if let Some(decl_node) = self.arena.get(decl_idx) {
                // Handle variable declarations
                if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                    && let Some(var_data) = self.arena.get_variable_declaration(decl_node)
                {
                    // Try type annotation first (e.g., `const x: MyInterface = ...`)
                    if let Some(member_id) = self.resolve_member_from_type_annotation(
                        root,
                        var_data.type_annotation,
                        member_name,
                    ) {
                        return Some(member_id);
                    }

                    // Fallback: check if the initializer is `new ClassName()`
                    if var_data.initializer.is_some()
                        && let Some(init_node) = self.arena.get(var_data.initializer)
                        && init_node.kind == syntax_kind_ext::NEW_EXPRESSION
                        && let Some(new_data) = self.arena.get_call_expr(init_node)
                    {
                        let mut walker2 = ScopeWalker::new(self.arena, self.binder);
                        if let Some(class_symbol_id) =
                            walker2.resolve_node(root, new_data.expression)
                            && let Some(member_id) = self.resolve_member_in_class_chain(
                                root,
                                class_symbol_id,
                                member_name,
                                0,
                            )
                        {
                            return Some(member_id);
                        }
                    }
                }

                // Handle parameter declarations (e.g., `function f(x: Foo) { x.member }`)
                if decl_node.kind == syntax_kind_ext::PARAMETER
                    && let Some(param_data) = self.arena.get_parameter(decl_node)
                    && let Some(member_id) = self.resolve_member_from_type_annotation(
                        root,
                        param_data.type_annotation,
                        member_name,
                    )
                {
                    return Some(member_id);
                }
            }
        }

        None
    }

    /// Resolve a member name from a type annotation node.
    ///
    /// Given a type annotation like `Foo` in `const x: Foo`, resolves `Foo` to its
    /// symbol and looks up the member name in its members or exports tables.
    /// Also walks interface/class declaration AST nodes to find member declarations
    /// that may not be stored in the binder's `members` table.
    fn resolve_member_from_type_annotation(
        &self,
        root: NodeIndex,
        type_annotation: NodeIndex,
        member_name: &str,
    ) -> Option<SymbolId> {
        if !type_annotation.is_some() {
            return None;
        }
        let type_node = self.arena.get(type_annotation)?;

        // If the annotation is a TypeReference (e.g., `Foo` or `Foo<T>`),
        // extract the type name and resolve it
        let type_name_idx = if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let type_ref = self.arena.get_type_ref(type_node)?;
            type_ref.type_name
        } else if type_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            // Plain identifier used as type (rare but possible)
            type_annotation
        } else {
            return None;
        };

        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let type_symbol_id = walker.resolve_node(root, type_name_idx)?;

        // Use the class chain resolver which handles direct members, exports,
        // AST member walking, and extends-chain inheritance.
        self.resolve_member_in_class_chain(root, type_symbol_id, member_name, 0)
    }

    /// Walk a declaration node (interface/class) to find a named member.
    fn find_member_in_declaration(
        &self,
        decl_idx: NodeIndex,
        member_name: &str,
    ) -> Option<SymbolId> {
        use tsz_scanner::SyntaxKind;

        let decl_node = self.arena.get(decl_idx)?;

        // Interface declaration: walk member signatures
        if decl_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
            let iface = self.arena.get_interface(decl_node)?;
            for &member_idx in &iface.members.nodes {
                let member_node = self.arena.get(member_idx)?;
                let name_idx = match member_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_SIGNATURE => {
                        self.arena.get_signature(member_node).map(|s| s.name)
                    }
                    k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                        self.arena.get_signature(member_node).map(|s| s.name)
                    }
                    _ => None,
                };
                if let Some(name_idx) = name_idx
                    && name_idx.is_some()
                    && let Some(name_node) = self.arena.get(name_idx)
                    && name_node.kind == SyntaxKind::Identifier as u16
                {
                    let pos = name_node.pos as usize;
                    let end = name_node.end as usize;
                    if end <= self.source_text.len() && pos < end {
                        let raw_text = &self.source_text[pos..end];
                        let text = raw_text.trim_end_matches(|c: char| {
                            !c.is_alphanumeric() && c != '_' && c != '$'
                        });
                        if text == member_name {
                            if let Some(sym_id) = self.binder.get_node_symbol(name_idx) {
                                return Some(sym_id);
                            }
                            if let Some(sym_id) = self.binder.get_node_symbol(member_idx) {
                                return Some(sym_id);
                            }
                        }
                    }
                }
            }
        }

        // Class declaration: walk class members (backup for non-binder-tracked members)
        if decl_node.is_class_like() {
            let class = self.arena.get_class(decl_node)?;
            for &member_idx in &class.members.nodes {
                let member_node = self.arena.get(member_idx)?;
                let name_idx = if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                    self.arena.get_property_decl(member_node).map(|p| p.name)
                } else if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                    self.arena.get_method_decl(member_node).map(|m| m.name)
                } else {
                    None
                };
                if let Some(name_idx) = name_idx
                    && name_idx.is_some()
                    && let Some(name_node) = self.arena.get(name_idx)
                    && name_node.kind == SyntaxKind::Identifier as u16
                {
                    let pos = name_node.pos as usize;
                    let end = name_node.end as usize;
                    if end <= self.source_text.len() && pos < end {
                        let raw_text = &self.source_text[pos..end];
                        let text = raw_text.trim_end_matches(|c: char| {
                            !c.is_alphanumeric() && c != '_' && c != '$'
                        });
                        if text == member_name {
                            if let Some(sym_id) = self.binder.get_node_symbol(name_idx) {
                                return Some(sym_id);
                            }
                            if let Some(sym_id) = self.binder.get_node_symbol(member_idx) {
                                return Some(sym_id);
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Resolve a member by walking the class inheritance chain via `extends` clauses.
    /// Returns the SymbolId of the member if found in any ancestor class.
    fn resolve_member_in_class_chain(
        &self,
        root: NodeIndex,
        class_symbol_id: tsz_binder::SymbolId,
        member_name: &str,
        depth: u8,
    ) -> Option<tsz_binder::SymbolId> {
        if depth > 10 {
            return None; // guard against circular inheritance
        }
        let class_symbol = self.binder.symbols.get(class_symbol_id)?;

        // Check direct members first
        if let Some(ref members) = class_symbol.members
            && let Some(member_id) = members.get(member_name)
        {
            return Some(member_id);
        }

        // Check exports (for static members)
        if let Some(ref exports) = class_symbol.exports
            && let Some(member_id) = exports.get(member_name)
        {
            return Some(member_id);
        }

        // Walk class declarations to find the member in the AST
        for &decl_idx in &class_symbol.declarations {
            if let Some(member_id) = self.find_member_in_declaration(decl_idx, member_name) {
                return Some(member_id);
            }
        }

        // Walk up the extends chain (classes and interfaces)
        for &decl_idx in &class_symbol.declarations {
            let decl_node = self.arena.get(decl_idx)?;
            // Get heritage clauses from class or interface declarations
            let heritage_clauses = if decl_node.is_class_like() {
                self.arena
                    .get_class(decl_node)
                    .and_then(|c| c.heritage_clauses.as_ref())
            } else if decl_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                self.arena
                    .get_interface(decl_node)
                    .and_then(|i| i.heritage_clauses.as_ref())
            } else {
                continue;
            };
            let Some(heritage_clauses) = heritage_clauses else {
                continue;
            };
            for &clause_idx in &heritage_clauses.nodes {
                let clause_node = self.arena.get(clause_idx)?;
                let heritage = self.arena.get_heritage(clause_node)?;
                // Only follow 'extends', not 'implements'
                if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                for &type_idx in &heritage.types.nodes {
                    let type_node = self.arena.get(type_idx)?;
                    // ExpressionWithTypeArguments — get the expression
                    let expr_idx =
                        if type_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS {
                            self.arena
                                .get_expr_type_args(type_node)
                                .map(|e| e.expression)
                        } else {
                            Some(type_idx)
                        };
                    if let Some(expr_idx) = expr_idx {
                        // Try binder's direct identifier resolution first (faster),
                        // then fall back to scope walker for qualified names.
                        let base_symbol_id = self
                            .binder
                            .resolve_identifier(self.arena, expr_idx)
                            .or_else(|| {
                                let mut walker = ScopeWalker::new(self.arena, self.binder);
                                walker.resolve_node(root, expr_idx)
                            });
                        if let Some(base_symbol_id) = base_symbol_id
                            && let Some(member_id) = self.resolve_member_in_class_chain(
                                root,
                                base_symbol_id,
                                member_name,
                                depth + 1,
                            )
                        {
                            return Some(member_id);
                        }
                    }
                }
            }
        }

        None
    }

    /// Fallback for member access in `get_definition_internal` (returns Location objects).
    fn try_member_access_fallback(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
    ) -> Option<Vec<Location>> {
        // First try symbol-based resolution
        if let Some(member_symbol_id) = self.try_resolve_member_access(root, node_idx)
            && let Some(locations) = self.locations_from_symbol(member_symbol_id)
        {
            return Some(locations);
        }
        // Then try direct AST-based resolution (for interface members without SymbolIds)
        self.try_resolve_member_access_from_ast(root, node_idx)
    }

    /// Resolve member access directly to AST locations without requiring SymbolId.
    /// This handles interface members that the binder doesn't track in `node_symbols`.
    fn try_resolve_member_access_from_ast(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
    ) -> Option<Vec<Location>> {
        let ext = self.arena.get_extended(node_idx)?;
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return None;
        }
        let parent_node = self.arena.get(parent_idx)?;
        if parent_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.arena.get_access_expr(parent_node)?;
        if access.name_or_argument != node_idx {
            return None;
        }

        let node = self.arena.get(node_idx)?;
        let member_name = &self.source_text[node.pos as usize..node.end as usize];

        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let expr_symbol_id = walker.resolve_node(root, access.expression)?;
        let expr_symbol = self.binder.symbols.get(expr_symbol_id)?;

        // For instances: resolve the variable's type annotation
        for &decl_idx in &expr_symbol.declarations {
            if let Some(decl_node) = self.arena.get(decl_idx) {
                let type_annotation = if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                    self.arena
                        .get_variable_declaration(decl_node)
                        .map(|v| v.type_annotation)
                } else if decl_node.kind == syntax_kind_ext::PARAMETER {
                    self.arena
                        .get_parameter(decl_node)
                        .map(|p| p.type_annotation)
                } else {
                    None
                };

                if let Some(type_ann) = type_annotation
                    && type_ann.is_some()
                    && let Some(loc) =
                        self.find_member_location_from_type(root, type_ann, member_name)
                {
                    return Some(vec![loc]);
                }
            }
        }

        None
    }

    /// Find a member's location directly from a type annotation, returning an AST-based Location.
    fn find_member_location_from_type(
        &self,
        root: NodeIndex,
        type_annotation: NodeIndex,
        member_name: &str,
    ) -> Option<Location> {
        let type_node = self.arena.get(type_annotation)?;
        let type_name_idx = if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let type_ref = self.arena.get_type_ref(type_node)?;
            type_ref.type_name
        } else if type_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            type_annotation
        } else {
            return None;
        };

        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let type_symbol_id = walker.resolve_node(root, type_name_idx)?;

        self.find_member_location_in_chain(root, type_symbol_id, member_name, 0)
    }

    /// Find a member's AST location by walking the class/interface inheritance chain.
    fn find_member_location_in_chain(
        &self,
        root: NodeIndex,
        symbol_id: tsz_binder::SymbolId,
        member_name: &str,
        depth: u8,
    ) -> Option<Location> {
        if depth > 10 {
            return None;
        }
        let symbol = self.binder.symbols.get(symbol_id)?;

        // Check direct declarations
        for &decl_idx in &symbol.declarations {
            if let Some(loc) = self.find_member_location_in_declaration(decl_idx, member_name) {
                return Some(loc);
            }
        }

        // Walk extends chain
        for &decl_idx in &symbol.declarations {
            let decl_node = self.arena.get(decl_idx)?;
            let heritage_clauses = if decl_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                self.arena
                    .get_interface(decl_node)
                    .and_then(|i| i.heritage_clauses.as_ref())
            } else if decl_node.is_class_like() {
                self.arena
                    .get_class(decl_node)
                    .and_then(|c| c.heritage_clauses.as_ref())
            } else {
                continue;
            };
            let Some(heritage_clauses) = heritage_clauses else {
                continue;
            };
            for &clause_idx in &heritage_clauses.nodes {
                let clause_node = self.arena.get(clause_idx)?;
                let heritage = self.arena.get_heritage(clause_node)?;
                if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                for &type_idx in &heritage.types.nodes {
                    let type_node = self.arena.get(type_idx)?;
                    let expr_idx =
                        if type_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS {
                            self.arena
                                .get_expr_type_args(type_node)
                                .map(|e| e.expression)
                        } else {
                            Some(type_idx)
                        };
                    if let Some(expr_idx) = expr_idx {
                        let base_symbol_id = self
                            .binder
                            .resolve_identifier(self.arena, expr_idx)
                            .or_else(|| {
                                let mut w = ScopeWalker::new(self.arena, self.binder);
                                w.resolve_node(root, expr_idx)
                            });
                        if let Some(base_symbol_id) = base_symbol_id
                            && let Some(loc) = self.find_member_location_in_chain(
                                root,
                                base_symbol_id,
                                member_name,
                                depth + 1,
                            )
                        {
                            return Some(loc);
                        }
                    }
                }
            }
        }

        None
    }

    /// Find the AST location of a named member within an interface/class declaration.
    /// Returns a Location directly from the member's name node, bypassing the symbol table.
    fn find_member_location_in_declaration(
        &self,
        decl_idx: NodeIndex,
        member_name: &str,
    ) -> Option<Location> {
        use tsz_scanner::SyntaxKind;

        let decl_node = self.arena.get(decl_idx)?;

        let members_list = if decl_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
            Some(self.arena.get_interface(decl_node)?.members.nodes.clone())
        } else if decl_node.is_class_like() {
            Some(self.arena.get_class(decl_node)?.members.nodes.clone())
        } else {
            None
        }?;

        for &member_idx in &members_list {
            let member_node = self.arena.get(member_idx)?;
            let name_idx = match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_SIGNATURE
                    || k == syntax_kind_ext::METHOD_SIGNATURE =>
                {
                    self.arena.get_signature(member_node).map(|s| s.name)
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    self.arena.get_property_decl(member_node).map(|p| p.name)
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    self.arena.get_method_decl(member_node).map(|m| m.name)
                }
                _ => None,
            };

            if let Some(name_idx) = name_idx
                && name_idx.is_some()
                && let Some(name_node) = self.arena.get(name_idx)
                && name_node.kind == SyntaxKind::Identifier as u16
            {
                let pos = name_node.pos as usize;
                let end = name_node.end as usize;
                if end <= self.source_text.len() && pos < end {
                    // Extract just the identifier text (the name node span may
                    // include trailing punctuation like `:` or `?` in signatures)
                    let raw_text = &self.source_text[pos..end];
                    let ident_text = raw_text
                        .trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '$');
                    if ident_text == member_name {
                        let start_pos = self
                            .line_map
                            .offset_to_position(member_node.pos, self.source_text);
                        let end_pos = self
                            .line_map
                            .offset_to_position(member_node.end, self.source_text);
                        return Some(Location {
                            file_path: self.file_name.clone(),
                            range: Range::new(start_pos, end_pos),
                        });
                    }
                }
            }
        }

        None
    }

    /// Check if a resolved symbol is a class/interface member being referenced as a bare
    /// identifier (not through `this.member` or `obj.member`). Class members require
    /// `this.` qualification and shouldn't resolve as lexical names.
    /// Check if the node is the member name in a `super.member` property access.
    fn is_super_member_access(&self, node_idx: NodeIndex) -> bool {
        let Some(ext) = self.arena.get_extended(node_idx) else {
            return false;
        };
        let Some(parent) = self.arena.get(ext.parent) else {
            return false;
        };
        if parent.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.arena.get_access_expr(parent) else {
            return false;
        };
        if access.name_or_argument != node_idx {
            return false;
        }
        self.arena
            .get(access.expression)
            .map(|n| n.kind == tsz_scanner::SyntaxKind::SuperKeyword as u16)
            .unwrap_or(false)
    }

    /// Resolve `super` to the base class symbol by walking up to the enclosing class
    /// and finding its extends clause target.
    fn resolve_super_base_class(&self, super_idx: NodeIndex) -> Option<tsz_binder::SymbolId> {
        let mut current = super_idx;
        loop {
            let ext = self.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            let parent = self.arena.get(ext.parent)?;
            if parent.is_class_like() {
                let class = self.arena.get_class(parent)?;
                let heritage = class.heritage_clauses.as_ref()?;
                for &clause_idx in &heritage.nodes {
                    let clause_node = self.arena.get(clause_idx)?;
                    let hd = self.arena.get_heritage(clause_node)?;
                    if hd.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                        continue;
                    }
                    if let Some(&type_idx) = hd.types.nodes.first() {
                        let type_node = self.arena.get(type_idx)?;
                        let expr_idx =
                            if type_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS {
                                self.arena
                                    .get_expr_type_args(type_node)
                                    .map(|e| e.expression)?
                            } else {
                                type_idx
                            };
                        return self.binder.resolve_identifier(self.arena, expr_idx);
                    }
                }
                return None;
            }
            current = ext.parent;
        }
    }

    fn is_bare_class_member_reference(&self, node_idx: NodeIndex, symbol_id: SymbolId) -> bool {
        // If this node IS the declaration name itself, it's not a "bare reference"
        if self.binder.node_symbols.contains_key(&node_idx.0) {
            return false;
        }

        let Some(symbol) = self.binder.symbols.get(symbol_id) else {
            return false;
        };

        // Only applies to property/method/accessor symbols
        const CLASS_MEMBER_FLAGS: u32 = symbol_flags::PROPERTY
            | symbol_flags::METHOD
            | symbol_flags::GET_ACCESSOR
            | symbol_flags::SET_ACCESSOR;
        if symbol.flags & CLASS_MEMBER_FLAGS == 0 {
            return false;
        }
        // If symbol also has variable/function flags, it's a merged declaration
        if symbol.flags
            & (symbol_flags::FUNCTION_SCOPED_VARIABLE
                | symbol_flags::BLOCK_SCOPED_VARIABLE
                | symbol_flags::FUNCTION
                | symbol_flags::CLASS
                | symbol_flags::INTERFACE
                | symbol_flags::TYPE_ALIAS)
            != 0
        {
            return false;
        }

        // Check if the identifier node is the right-hand side of a property access.
        // If it is (e.g., `this.value`), this is a legitimate member reference.
        if let Some(ext) = self.arena.get_extended(node_idx)
            && ext.parent.is_some()
            && let Some(parent_node) = self.arena.get(ext.parent)
            && parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(parent_node)
            && access.name_or_argument == node_idx
        {
            return false; // Part of `obj.member` — not a bare reference
        }

        true // Bare identifier referencing a class member
    }

    /// Check if a node is a built-in keyword literal or built-in identifier
    /// that has no user-navigable definition (e.g., null, true, false, undefined, arguments).
    fn is_builtin_node(&self, node_idx: NodeIndex) -> bool {
        if let Some(node) = self.arena.get(node_idx) {
            use tsz_scanner::SyntaxKind;
            let kind = node.kind;
            // Keyword literals never have user-navigable definitions
            if kind == SyntaxKind::NullKeyword as u16
                || kind == SyntaxKind::TrueKeyword as u16
                || kind == SyntaxKind::FalseKeyword as u16
                || kind == SyntaxKind::VoidKeyword as u16
            {
                return true;
            }
            // Check identifier text against built-in globals without definitions
            if kind == SyntaxKind::Identifier as u16 {
                let pos = node.pos as usize;
                let end = node.end as usize;
                if end <= self.source_text.len() && pos <= end {
                    let text = &self.source_text[pos..end];
                    if text == "undefined" || text == "arguments" {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Get rich definition info including metadata for tsserver protocol.
    pub fn get_definition_info(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Option<Vec<DefinitionInfo>> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        let mut node_idx = find_node_at_or_before_offset(self.arena, offset, self.source_text);
        if node_idx.is_none()
            && let Some(adjusted) =
                find_symbol_query_node_at_or_before(self.arena, self.source_text, offset)
        {
            node_idx = adjusted;
        }

        if node_idx.is_none() {
            return None;
        }

        if !is_symbol_query_node(self.arena, node_idx)
            && (is_comment_context(self.source_text, offset)
                || should_backtrack_to_previous_symbol(self.source_text, offset))
            && let Some(adjusted) =
                find_symbol_query_node_at_or_before(self.arena, self.source_text, offset)
            && let Some(adj_node) = self.arena.get(adjusted)
            && (adj_node.end >= offset || offset.saturating_sub(adj_node.end) <= 1)
        {
            node_idx = adjusted;
        }

        if !is_symbol_query_node(self.arena, node_idx) {
            return None;
        }

        // Check for keyword navigation (return/await/yield/case/switch)
        if let Some(infos) = self.try_keyword_navigation(node_idx, offset) {
            return Some(infos);
        }

        // Skip keyword literals and built-in identifiers
        if self.is_builtin_node(node_idx) {
            return None;
        }

        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id_opt = walker.resolve_node(root, node_idx);

        if let Some(symbol_id) = symbol_id_opt
            && let Some(infos) = self.definition_infos_from_symbol(symbol_id)
        {
            return Some(infos);
        }

        // Fallback: try member access resolution
        if let Some(member_symbol_id) = self.try_resolve_member_access(root, node_idx)
            && let Some(infos) = self.definition_infos_from_symbol(member_symbol_id)
        {
            return Some(infos);
        }

        // Fallback: try file_locals
        if let Some(infos) = self.try_file_locals_fallback_info(node_idx) {
            return Some(infos);
        }

        None
    }

    /// Convert a symbol's declarations into rich `DefinitionInfo` objects.
    pub fn definition_infos_from_symbol(&self, symbol_id: SymbolId) -> Option<Vec<DefinitionInfo>> {
        let symbol = self.binder.symbols.get(symbol_id)?;
        let source_len = self.source_text.len() as u32;

        let infos: Vec<DefinitionInfo> = symbol
            .declarations
            .iter()
            .filter_map(|&decl_idx| {
                let decl_node = self.arena.get(decl_idx)?;

                if decl_node.pos > source_len || decl_node.end > source_len {
                    return None;
                }
                if decl_node.end < decl_node.pos {
                    return None;
                }
                // Skip zero-width declarations (synthetic builtins)
                if decl_node.pos == decl_node.end {
                    return None;
                }

                // Get the name node span (text span) vs full declaration span (context span)
                let (name_range, context_range) =
                    self.compute_name_and_context_spans(decl_idx, decl_node);

                let line_count = self.line_map.line_count() as u32;
                if name_range.start.line >= line_count || name_range.end.line >= line_count {
                    return None;
                }

                // Determine kind, name, and other metadata
                let kind = self.get_declaration_kind(decl_idx, symbol.flags);
                let name = symbol.escaped_name.clone();
                let (container_name, container_kind) = self.get_container_info(symbol_id);
                let is_local = if kind == "parameter" {
                    false
                } else if self.is_class_or_interface_member(decl_idx) {
                    true
                } else {
                    !self.is_top_level_declaration(decl_idx)
                };
                let is_ambient = self.is_ambient_declaration(decl_idx);

                Some(DefinitionInfo {
                    location: Location {
                        file_path: self.file_name.clone(),
                        range: name_range,
                    },
                    context_span: Some(context_range),
                    name,
                    kind,
                    container_name,
                    container_kind,
                    is_local,
                    is_ambient,
                })
            })
            .collect();

        if infos.is_empty() {
            return None;
        }

        // For function/method overloads, return only the implementation (the one with a body)
        if infos.len() > 1
            && self.has_function_overloads(symbol)
            && let Some(impl_info) = self.find_implementation_info(&infos, symbol)
        {
            return Some(vec![impl_info]);
        }

        Some(infos)
    }

    /// Check if a symbol has function/method overloads (multiple function declarations).
    fn has_function_overloads(&self, symbol: &tsz_binder::Symbol) -> bool {
        if symbol.declarations.len() <= 1 {
            return false;
        }
        let mut func_count = 0;
        for &decl_idx in &symbol.declarations {
            if let Some(node) = self.arena.get(decl_idx) {
                match node.kind {
                    syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::METHOD_DECLARATION
                    | syntax_kind_ext::CONSTRUCTOR => {
                        func_count += 1;
                    }
                    _ => {}
                }
            }
        }
        func_count > 1
    }

    /// Find the implementation definition info (the overload with a function body).
    fn find_implementation_info(
        &self,
        infos: &[DefinitionInfo],
        symbol: &tsz_binder::Symbol,
    ) -> Option<DefinitionInfo> {
        for (i, &decl_idx) in symbol.declarations.iter().enumerate() {
            if let Some(node) = self.arena.get(decl_idx) {
                let has_body = match node.kind {
                    syntax_kind_ext::FUNCTION_DECLARATION => self
                        .arena
                        .get_function(node)
                        .is_some_and(|f| f.body.is_some()),
                    syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(node)
                        .is_some_and(|m| m.body.is_some()),
                    syntax_kind_ext::CONSTRUCTOR => self
                        .arena
                        .get_constructor(node)
                        .is_some_and(|c| c.body.is_some()),
                    _ => false,
                };
                if has_body && let Some(info) = infos.get(i) {
                    return Some(info.clone());
                }
            }
        }
        None
    }

    /// Try `file_locals` fallback but return `DefinitionInfo`.
    fn try_file_locals_fallback_info(&self, node_idx: NodeIndex) -> Option<Vec<DefinitionInfo>> {
        let node = self.arena.get(node_idx)?;
        let pos = node.pos as usize;
        let end = node.end as usize;
        if end > self.source_text.len() || pos > end {
            return None;
        }

        let text = &self.source_text[pos..end];
        if is_builtin_global(text) {
            return None;
        }

        let symbol_id = self.binder.file_locals.get(text)?;
        self.definition_infos_from_symbol(symbol_id)
    }

    /// Compute the name span and context span for a declaration node.
    /// Returns (`name_range` for the identifier, full declaration range for context).
    fn compute_name_and_context_spans(
        &self,
        decl_idx: NodeIndex,
        decl_node: &tsz_parser::parser::node::Node,
    ) -> (Range, Range) {
        // For the context span, we may need to go up to the parent node.
        // For VariableDeclaration, the context is the VariableStatement
        // (which includes `declare var ... ;`).
        let context_node_span = self.get_context_span_node(decl_idx, decl_node);

        let context_start = self
            .line_map
            .offset_to_position(context_node_span.0, self.source_text);
        let context_end = self
            .line_map
            .offset_to_position(context_node_span.1, self.source_text);
        let context_range = Range::new(context_start, context_end);

        // Try to find the identifier name node within the declaration
        if let Some(name_idx) = self.get_declaration_name_idx(decl_idx)
            && name_idx.is_some()
            && let Some(name_node) = self.arena.get(name_idx)
        {
            let name_start = self
                .line_map
                .offset_to_position(name_node.pos, self.source_text);
            let name_end = self
                .line_map
                .offset_to_position(name_node.end, self.source_text);
            return (Range::new(name_start, name_end), context_range);
        }

        // If we can't find a name node, use the declaration span for both
        (context_range, context_range)
    }

    /// Get the span for the context (the full declaration statement).
    /// For `VariableDeclaration`, walk up to `VariableStatement`.
    /// For other declarations, use the declaration node itself.
    /// Returns the span with leading trivia stripped (using getStart semantics).
    fn get_context_span_node(
        &self,
        decl_idx: NodeIndex,
        decl_node: &tsz_parser::parser::node::Node,
    ) -> (u32, u32) {
        let source_bytes = self.source_text.as_bytes();
        let source_len = self.source_text.len() as u32;

        // Strip leading whitespace/newlines from a position
        let skip_leading = |pos: u32, end: u32| -> u32 {
            let limit = end.min(source_len) as usize;
            let mut i = pos as usize;
            while i < limit {
                match source_bytes[i] {
                    b' ' | b'\t' | b'\n' | b'\r' => i += 1,
                    _ => break,
                }
            }
            i as u32
        };

        // Strip trailing whitespace/newlines from an end position
        let skip_trailing = |pos: u32, end: u32| -> u32 {
            let start = pos as usize;
            let mut i = end.min(source_len) as usize;
            while i > start {
                match source_bytes[i - 1] {
                    b' ' | b'\t' | b'\n' | b'\r' => i -= 1,
                    _ => break,
                }
            }
            i as u32
        };

        // Find the position right after the last significant token in the range.
        // This handles cases where the parser's node `end` extends into the next
        // statement by finding the last ; or } in the range.
        let find_real_end = |pos: u32, end: u32| -> u32 {
            let start = pos as usize;
            let e = end.min(source_len) as usize;
            // Scan backwards for the last ; or } (statement-ending tokens)
            for i in (start..e).rev() {
                match source_bytes[i] {
                    b';' | b'}' => return (i + 1) as u32,
                    _ => {}
                }
            }
            // Fall back to stripping trailing whitespace
            skip_trailing(pos, end)
        };

        // For declarations that end with a body (class, enum, function, etc.),
        // find the closing } and use that as the end (don't include trailing ;).
        let find_brace_end = |pos: u32, end: u32| -> u32 {
            let start = pos as usize;
            let e = end.min(source_len) as usize;
            // Scan backwards for the closing }
            for i in (start..e).rev() {
                if source_bytes[i] == b'}' {
                    return (i + 1) as u32;
                }
            }
            // Fall back to find_real_end
            find_real_end(pos, end)
        };

        // Clean span: strip leading trivia, find real end
        let clean = |pos: u32, end: u32| -> (u32, u32) {
            let s = skip_leading(pos, end);
            let e = find_real_end(s, end);
            (s, e)
        };

        // Clean span for brace-terminated declarations (class, enum, etc.):
        // strip leading trivia, find closing }
        let clean_brace = |pos: u32, end: u32| -> (u32, u32) {
            let s = skip_leading(pos, end);
            let e = find_brace_end(s, end);
            (s, e)
        };

        match decl_node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                // Walk up: VariableDeclaration -> VariableDeclarationList -> VariableStatement
                if let Some(ext) = self.arena.get_extended(decl_idx) {
                    let parent_idx = ext.parent;
                    if parent_idx.is_some() {
                        // Check if parent is a CatchClause - no contextSpan for catch vars
                        if let Some(parent_node) = self.arena.get(parent_idx)
                            && parent_node.kind == syntax_kind_ext::CATCH_CLAUSE
                        {
                            return (decl_node.pos, decl_node.end);
                        }
                        if let Some(parent_ext) = self.arena.get_extended(parent_idx) {
                            let grandparent_idx = parent_ext.parent;
                            if grandparent_idx.is_some()
                                && let Some(gp_node) = self.arena.get(grandparent_idx)
                                && gp_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                            {
                                return clean(gp_node.pos, gp_node.end);
                            }
                        }
                        if let Some(parent_node) = self.arena.get(parent_idx) {
                            return clean(parent_node.pos, parent_node.end);
                        }
                    }
                }
                (decl_node.pos, decl_node.end)
            }
            syntax_kind_ext::FUNCTION_DECLARATION
            | syntax_kind_ext::CLASS_DECLARATION
            | syntax_kind_ext::INTERFACE_DECLARATION
            | syntax_kind_ext::TYPE_ALIAS_DECLARATION
            | syntax_kind_ext::ENUM_DECLARATION
            | syntax_kind_ext::MODULE_DECLARATION => {
                // Check for modifiers (declare, export, async, abstract, etc.)
                // that extend the span before the declaration keyword.
                let modifiers = match decl_node.kind {
                    syntax_kind_ext::FUNCTION_DECLARATION => self
                        .arena
                        .get_function(decl_node)
                        .and_then(|f| f.modifiers.as_ref()),
                    syntax_kind_ext::CLASS_DECLARATION => self
                        .arena
                        .get_class(decl_node)
                        .and_then(|c| c.modifiers.as_ref()),
                    syntax_kind_ext::INTERFACE_DECLARATION => self
                        .arena
                        .get_interface(decl_node)
                        .and_then(|i| i.modifiers.as_ref()),
                    syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                        .arena
                        .get_type_alias(decl_node)
                        .and_then(|t| t.modifiers.as_ref()),
                    syntax_kind_ext::ENUM_DECLARATION => self
                        .arena
                        .get_enum(decl_node)
                        .and_then(|e| e.modifiers.as_ref()),
                    syntax_kind_ext::MODULE_DECLARATION => self
                        .arena
                        .get_module(decl_node)
                        .and_then(|m| m.modifiers.as_ref()),
                    _ => None,
                };

                // Find the earliest modifier position to include keywords like `declare`, `export`
                let start_pos = if let Some(mods) = modifiers {
                    let mut earliest = decl_node.pos;
                    for &mod_idx in &mods.nodes {
                        if let Some(mod_node) = self.arena.get(mod_idx)
                            && mod_node.pos < earliest
                        {
                            earliest = mod_node.pos;
                        }
                    }
                    earliest
                } else {
                    decl_node.pos
                };

                // Type aliases end with ; (e.g., `type T = ...;`), other declarations
                // end with } and should NOT include trailing ;
                if decl_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                    clean(start_pos, decl_node.end)
                } else {
                    clean_brace(start_pos, decl_node.end)
                }
            }
            syntax_kind_ext::METHOD_SIGNATURE
            | syntax_kind_ext::PROPERTY_SIGNATURE
            | syntax_kind_ext::METHOD_DECLARATION
            | syntax_kind_ext::PROPERTY_DECLARATION
            | syntax_kind_ext::CONSTRUCT_SIGNATURE
            | syntax_kind_ext::CONSTRUCTOR => {
                // For member declarations, include modifiers (public, static, etc.)
                let modifiers = match decl_node.kind {
                    syntax_kind_ext::METHOD_SIGNATURE | syntax_kind_ext::PROPERTY_SIGNATURE => self
                        .arena
                        .get_signature(decl_node)
                        .and_then(|s| s.modifiers.as_ref()),
                    syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(decl_node)
                        .and_then(|m| m.modifiers.as_ref()),
                    syntax_kind_ext::PROPERTY_DECLARATION => self
                        .arena
                        .get_property_decl(decl_node)
                        .and_then(|p| p.modifiers.as_ref()),
                    syntax_kind_ext::CONSTRUCTOR => self
                        .arena
                        .get_constructor(decl_node)
                        .and_then(|c| c.modifiers.as_ref()),
                    _ => None,
                };

                let start_pos = if let Some(mods) = modifiers {
                    let mut earliest = decl_node.pos;
                    for &mod_idx in &mods.nodes {
                        if let Some(mod_node) = self.arena.get(mod_idx)
                            && mod_node.pos < earliest
                        {
                            earliest = mod_node.pos;
                        }
                    }
                    earliest
                } else {
                    decl_node.pos
                };

                clean(start_pos, decl_node.end)
            }
            _ => (decl_node.pos, decl_node.end),
        }
    }
}
