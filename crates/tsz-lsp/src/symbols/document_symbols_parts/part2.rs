impl<'a> DocumentSymbolProvider<'a> {
    /// Post-process: scan top-level expression statements for
    /// `identifier = { … }` and attach the RHS object literal's
    /// members as children of the matching var / const entry. Skips
    /// owners that already have children (from an initializer or an
    /// expando promotion).
    /// Detect CommonJS chained `exports.X = exports.Y = … = value`
    /// assignments and emit a nested nav tree (X → Y → …). tsc models
    /// these as declaration merging for the CommonJS module
    /// namespace. Handles only simple `exports.<name>` LHS forms.
    fn apply_commonjs_exports_chain(
        &self,
        statements: &[NodeIndex],
        symbols: &mut Vec<DocumentSymbolEntry>,
    ) {
        // Walk an assignment, collecting (name, stmt_idx) in order.
        // Returns None if the chain breaks (non-exports LHS or wrong
        // shape). `value_idx` is the innermost RHS for span purposes.
        fn walk(
            provider: &DocumentSymbolProvider,
            expr_idx: NodeIndex,
            out: &mut Vec<String>,
        ) -> bool {
            let Some(expr) = provider.arena.get(expr_idx) else {
                return false;
            };
            if expr.kind != syntax_kind_ext::BINARY_EXPRESSION {
                return true; // non-assignment terminator — OK (end of chain)
            }
            let Some(bin) = provider.arena.get_binary_expr(expr) else {
                return false;
            };
            if bin.operator_token != SyntaxKind::EqualsToken as u16 {
                return false;
            }
            // LHS must be exports.<name>
            let Some(lhs) = provider.arena.get(bin.left) else {
                return false;
            };
            if lhs.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                return false;
            }
            let Some(access) = provider.arena.get_access_expr(lhs) else {
                return false;
            };
            let Some(root) = provider.arena.get(access.expression) else {
                return false;
            };
            if root.kind != SyntaxKind::Identifier as u16 {
                return false;
            }
            if provider.get_name(access.expression).as_deref() != Some("exports") {
                return false;
            }
            let Some(name) = provider.get_name(access.name_or_argument) else {
                return false;
            };
            out.push(name);
            walk(provider, bin.right, out)
        }

        for &stmt_idx in statements {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(exp_stmt) = self.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let mut names: Vec<String> = Vec::new();
            if !walk(self, exp_stmt.expression, &mut names) || names.is_empty() {
                continue;
            }
            // Build nested chain: names[0] is outermost, names[n-1]
            // innermost. tsc renders them all as `const`.
            let range = node_range(self.arena, self.line_map, self.source_text, stmt_idx);
            let mut inner: Option<DocumentSymbolEntry> = None;
            for name in names.iter().rev() {
                let mut children = Vec::new();
                if let Some(child) = inner.take() {
                    children.push(child);
                }
                inner = Some(DocumentSymbolEntry {
                    name: name.clone(),
                    detail: None,
                    kind: SymbolKind::Constant,
                    kind_modifiers: String::new(),
                    range,
                    selection_range: range,
                    container_name: None,
                    children,
                });
            }
            if let Some(top) = inner {
                symbols.push(top);
            }
        }
    }

    /// Walk top-level expression statements for named class / function
    /// expressions at any nesting depth (most commonly inside call
    /// arguments like `console.log(class Foo {})`). Each named class /
    /// function expression becomes a top-level nav entry matching
    /// tsc's behavior in `navigationBarAnonymousClassAndFunctionExpressions2`.
    fn apply_nested_named_expressions(
        &self,
        statements: &[NodeIndex],
        symbols: &mut Vec<DocumentSymbolEntry>,
    ) {
        fn walk(
            provider: &DocumentSymbolProvider,
            expr_idx: NodeIndex,
            out: &mut Vec<DocumentSymbolEntry>,
        ) {
            if expr_idx.is_none() {
                return;
            }
            let Some(node) = provider.arena.get(expr_idx) else {
                return;
            };
            match node.kind {
                k if k == syntax_kind_ext::CLASS_EXPRESSION => {
                    // Only named class expressions surface; anonymous
                    // ones are skipped (expected behavior per tsc).
                    if let Some(class) = provider.arena.get_class(node)
                        && class.name.is_some()
                        && let Some(name) = provider.get_name(class.name)
                    {
                        let range = node_range(
                            provider.arena,
                            provider.line_map,
                            provider.source_text,
                            expr_idx,
                        );
                        let selection_range = node_range(
                            provider.arena,
                            provider.line_map,
                            provider.source_text,
                            class.name,
                        );
                        let mut children = Vec::new();
                        for &member in &class.members.nodes {
                            children.extend(provider.collect_symbols(member, Some(&name)));
                        }
                        out.push(DocumentSymbolEntry {
                            name,
                            detail: None,
                            kind: SymbolKind::Class,
                            kind_modifiers: String::new(),
                            range,
                            selection_range,
                            container_name: None,
                            children,
                        });
                    }
                }
                k if k == syntax_kind_ext::CALL_EXPRESSION => {
                    let Some(call) = provider.arena.get_call_expr(node) else {
                        return;
                    };
                    walk(provider, call.expression, out);
                    if let Some(args) = call.arguments.as_ref() {
                        for &arg in &args.nodes {
                            walk(provider, arg, out);
                        }
                    }
                }
                _ => {}
            }
        }
        let mut new_entries = Vec::new();
        for &stmt_idx in statements {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(exp_stmt) = self.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            walk(self, exp_stmt.expression, &mut new_entries);
        }
        symbols.extend(new_entries);
    }

    fn apply_identifier_object_assignments(
        &self,
        statements: &[NodeIndex],
        symbols: &mut Vec<DocumentSymbolEntry>,
    ) {
        // Collect top-level assignments `x = { foo: function() {…}, … }`
        // where x is a previously-declared (empty) var. tsc surfaces
        // each function-valued property of the RHS object as a TOP-LEVEL
        // nav entry (the binding expression's `parent` is the
        // ExpressionStatement, which is a direct child of the source
        // file), not as children of `x`. Non-function-valued properties
        // are dropped.
        let mut new_entries: Vec<DocumentSymbolEntry> = Vec::new();
        for &stmt_idx in statements {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(exp_stmt) = self.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(expr_node) = self.arena.get(exp_stmt.expression) else {
                continue;
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(bin) = self.arena.get_binary_expr(expr_node) else {
                continue;
            };
            if bin.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }
            let Some(lhs) = self.arena.get(bin.left) else {
                continue;
            };
            if lhs.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let Some(owner) = self.get_name(bin.left) else {
                continue;
            };
            let Some(rhs_node) = self.arena.get(bin.right) else {
                continue;
            };
            if rhs_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                continue;
            }
            // Only process when the owner is a previously-declared var /
            // const with no initializer-driven children yet. (If the var
            // already has children from its initializer, we'd be
            // duplicating.)
            let owner_exists = symbols.iter().any(|s| {
                s.name == owner
                    && matches!(s.kind, SymbolKind::Variable | SymbolKind::Constant)
                    && s.children.is_empty()
            });
            if !owner_exists {
                continue;
            }
            let Some(obj) = self.arena.get_literal_expr(rhs_node) else {
                continue;
            };
            for &prop_idx in &obj.elements.nodes {
                let Some(prop_node) = self.arena.get(prop_idx) else {
                    continue;
                };
                if prop_node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
                    continue;
                }
                let Some(prop) = self.arena.get_property_assignment(prop_node) else {
                    continue;
                };
                let Some(name) = self.get_name(prop.name) else {
                    continue;
                };
                let Some(init) = self.arena.get(prop.initializer) else {
                    continue;
                };
                if init.kind != syntax_kind_ext::FUNCTION_EXPRESSION
                    && init.kind != syntax_kind_ext::ARROW_FUNCTION
                {
                    continue;
                }
                let body = self
                    .arena
                    .get_function(init)
                    .map_or(NodeIndex::NONE, |f| f.body);
                let children = self.collect_children_from_block(body, Some(&name));
                let range = node_range(self.arena, self.line_map, self.source_text, prop_idx);
                let selection_range =
                    node_range(self.arena, self.line_map, self.source_text, prop.name);
                new_entries.push(DocumentSymbolEntry {
                    name,
                    detail: None,
                    kind: SymbolKind::Method,
                    kind_modifiers: String::new(),
                    range,
                    selection_range,
                    container_name: None,
                    children,
                });
            }
        }
        symbols.extend(new_entries);
    }

    fn collect_returned_object_members(
        &self,
        block_idx: NodeIndex,
        container_name: Option<&str>,
    ) -> Vec<DocumentSymbolEntry> {
        if block_idx.is_none() {
            return Vec::new();
        }
        let Some(block_node) = self.arena.get(block_idx) else {
            return Vec::new();
        };
        if block_node.kind != syntax_kind_ext::BLOCK {
            return Vec::new();
        }
        let Some(block) = self.arena.get_block(block_node) else {
            return Vec::new();
        };
        for &stmt in &block.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                continue;
            }
            let Some(ret) = self.arena.get_return_statement(stmt_node) else {
                continue;
            };
            let expr_idx = ret.expression;
            if expr_idx.is_none() {
                continue;
            }
            let Some(expr_node) = self.arena.get(expr_idx) else {
                continue;
            };
            if expr_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return self.collect_object_literal_members(expr_idx, container_name);
            }
        }
        Vec::new()
    }

    /// Helper to collect children from a block (e.g. inside function).
    /// Only collects nested functions/classes for the outline.
    fn collect_children_from_block(
        &self,
        block_idx: NodeIndex,
        container_name: Option<&str>,
    ) -> Vec<DocumentSymbolEntry> {
        let mut symbols = Vec::new();
        if block_idx.is_none() {
            return symbols;
        }

        if let Some(node) = self.arena.get(block_idx)
            && node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.arena.get_block(node)
        {
            for &stmt in &block.statements.nodes {
                // tsc's `addChildrenRecursively` walks every statement
                // inside a block and treats function/class/interface/
                // enum/type-alias/module declarations AND variable
                // statements as nav nodes. Surfacing vars matches tests
                // like `navigationBarItemsFunctions` which expect
                // `function baz() { var v = 10 }` → baz has child v.
                if let Some(stmt_node) = self.arena.get(stmt)
                    && matches!(
                        stmt_node.kind,
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION
                            || k == syntax_kind_ext::CLASS_DECLARATION
                            || k == syntax_kind_ext::INTERFACE_DECLARATION
                            || k == syntax_kind_ext::ENUM_DECLARATION
                            || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                            || k == syntax_kind_ext::MODULE_DECLARATION
                            || k == syntax_kind_ext::VARIABLE_STATEMENT
                    )
                {
                    symbols.extend(self.collect_symbols(stmt, container_name));
                }
            }
        }
        symbols
    }
}
