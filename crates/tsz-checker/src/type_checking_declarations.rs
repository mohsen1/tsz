//! Type checking: super expression validation, property initialization,
//! symbol helpers, ambient/namespace checks, interface merge compatibility.
//!
//! Split from `type_checking.rs` for maintainability.

use crate::query_boundaries::type_checking as query;
use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Super Expression Validation
    // =========================================================================

    // Check if a super expression is inside a nested function within a constructor.
    // Walks up the AST from the given node to determine if it's inside
    // a nested function (function expression, arrow function) within a constructor.
    //
    // ## Parameters:
    // 17. Property Initialization Checking (5 functions)

    /// Check for TS2729: Property is used before its initialization.
    ///
    /// This checks if a property initializer references another property via `this.X`
    /// where X is declared after the current property.
    ///
    /// ## Parameters
    /// - `current_prop_idx`: The current property node index
    /// - `initializer_idx`: The initializer expression node index
    pub(crate) fn check_property_initialization_order(
        &mut self,
        current_prop_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;

        // Get class info to access member order
        let Some(class_info) = self.ctx.enclosing_class.clone() else {
            return;
        };

        // Find the position of the current property in the member list
        let Some(current_pos) = class_info
            .member_nodes
            .iter()
            .position(|&idx| idx == current_prop_idx)
        else {
            return;
        };

        // Collect all `this.X` property accesses in the initializer
        let accesses = self.collect_this_property_accesses(initializer_idx);

        for (name, access_node_idx) in accesses {
            // Find if this name refers to another property in the class
            for (target_pos, &target_idx) in class_info.member_nodes.iter().enumerate() {
                if let Some(member_name) = self.get_member_name(target_idx)
                    && member_name == name
                {
                    // Check if target is an instance property (not static, not a method)
                    if self.is_instance_property(target_idx) {
                        // Report 2729 if:
                        // 1. Target is declared after current property, OR
                        // 2. Target is an abstract property (no initializer in this class)
                        let should_error = target_pos > current_pos
                            || self.is_abstract_property(target_idx)
                            || self.has_no_initializer(target_idx);
                        if should_error {
                            self.error_at_node(
                                access_node_idx,
                                &format!("Property '{name}' is used before its initialization."),
                                diagnostic_codes::PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION,
                            );
                        }
                    }
                    break;
                }
            }
        }
    }

    /// Check if a property declaration is abstract (has abstract modifier).
    ///
    /// ## Parameters
    /// - `member_idx`: The class member node index
    ///
    /// Returns true if the member is an abstract property declaration.
    pub(crate) fn is_abstract_property(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            return false;
        }

        if let Some(prop) = self.ctx.arena.get_property_decl(node) {
            return self.has_abstract_modifier(&prop.modifiers);
        }

        false
    }

    /// Check if a property declaration has no initializer.
    ///
    /// This is used to detect properties that are declared but not initialized,
    /// which should trigger TS2729 when referenced in other property initializers.
    pub(crate) fn has_no_initializer(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            return false;
        }

        if let Some(prop) = self.ctx.arena.get_property_decl(node) {
            return prop.initializer.is_none();
        }

        false
    }

    /// Check if a member is a static property (has static modifier).
    pub(crate) fn is_static_property(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            return false;
        }

        if let Some(prop) = self.ctx.arena.get_property_decl(node) {
            return self.has_static_modifier(&prop.modifiers);
        }

        false
    }

    /// Check for TS2729 in static property initializers.
    ///
    /// Static properties can reference other static properties defined after them,
    /// but we report TS2729 when they do (if the referenced property comes after).
    pub(crate) fn check_static_property_initialization_order(
        &mut self,
        current_prop_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;

        // Get class info to access member order
        let Some(class_info) = self.ctx.enclosing_class.clone() else {
            return;
        };

        // Find the position of the current property in the member list
        let Some(current_pos) = class_info
            .member_nodes
            .iter()
            .position(|&idx| idx == current_prop_idx)
        else {
            return;
        };

        // Collect all `ClassName.member` accesses in the initializer
        let class_name = &class_info.name;
        let accesses = self.collect_static_member_accesses(initializer_idx, class_name);

        for (member_name, access_node_idx) in accesses {
            // Find if this name refers to another static property in the class
            for (target_pos, &target_idx) in class_info.member_nodes.iter().enumerate() {
                if let Some(member_name_found) = self.get_member_name(target_idx)
                    && member_name_found == member_name
                {
                    // Check if target is a static property
                    if self.is_static_property(target_idx) {
                        // Report 2729 if target is declared after current property
                        if target_pos > current_pos {
                            self.error_at_node(
                                access_node_idx,
                                &format!(
                                    "Property '{member_name}' is used before its initialization."
                                ),
                                diagnostic_codes::PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION,
                            );
                        }
                    }
                    break;
                }
            }
        }
    }

    /// Collect `ClassName.member` accesses in an AST node.
    fn collect_static_member_accesses(
        &self,
        node_idx: NodeIndex,
        class_name: &str,
    ) -> Vec<(String, NodeIndex)> {
        let mut accesses = Vec::new();
        self.collect_static_accesses_recursive(node_idx, class_name, &mut accesses);
        accesses
    }

    /// Recursive helper to collect `ClassName.member` accesses (including in JSX).
    fn collect_static_accesses_recursive(
        &self,
        node_idx: NodeIndex,
        class_name: &str,
        accesses: &mut Vec<(String, NodeIndex)>,
    ) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        // Don't recurse into nested class definitions
        if node.kind == syntax_kind_ext::CLASS_DECLARATION
            || node.kind == syntax_kind_ext::CLASS_EXPRESSION
        {
            return;
        }

        // Check for property access `C.X`
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if let Some(access) = self.ctx.arena.get_access_expr(node) {
                // Check if the expression is an identifier matching the class name
                if let Some(expr_node) = self.ctx.arena.get(access.expression) {
                    if expr_node.kind == SyntaxKind::Identifier as u16 {
                        if let Some(ident) = self.ctx.arena.get_identifier(expr_node)
                            && ident.escaped_text == class_name
                        {
                            // Get the property name
                            if let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
                                && let Some(prop_ident) = self.ctx.arena.get_identifier(name_node)
                            {
                                accesses.push((prop_ident.escaped_text.clone(), node_idx));
                            }
                        }
                    } else {
                        // Recurse into the expression part
                        self.collect_static_accesses_recursive(
                            access.expression,
                            class_name,
                            accesses,
                        );
                    }
                }
            }
            return;
        }

        // For other nodes, recurse into children based on node type
        match node.kind {
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(binary) = self.ctx.arena.get_binary_expr(node) {
                    self.collect_static_accesses_recursive(binary.left, class_name, accesses);
                    self.collect_static_accesses_recursive(binary.right, class_name, accesses);
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    self.collect_static_accesses_recursive(call.expression, class_name, accesses);
                    if let Some(ref args) = call.arguments {
                        for &arg in &args.nodes {
                            self.collect_static_accesses_recursive(arg, class_name, accesses);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_EXPRESSION => {
                // For JSX expressions, recurse into the expression
                if let Some(jsx_expr) = self.ctx.arena.get_jsx_expression(node)
                    && jsx_expr.expression.is_some()
                {
                    self.collect_static_accesses_recursive(
                        jsx_expr.expression,
                        class_name,
                        accesses,
                    );
                }
            }
            k if k == syntax_kind_ext::JSX_OPENING_ELEMENT
                || k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT =>
            {
                // Check JSX element attributes
                if let Some(jsx_elem) = self.ctx.arena.get_jsx_opening(node)
                    && let Some(attrs_node) = self.ctx.arena.get(jsx_elem.attributes)
                    && let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node)
                {
                    for &attr_idx in &attrs.properties.nodes {
                        self.collect_static_accesses_recursive(attr_idx, class_name, accesses);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_ATTRIBUTE => {
                // Check JSX attribute initializer
                if let Some(attr) = self.ctx.arena.get_jsx_attribute(node)
                    && attr.initializer.is_some()
                {
                    self.collect_static_accesses_recursive(attr.initializer, class_name, accesses);
                }
            }
            k if k == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE => {
                // Check JSX spread attribute
                if let Some(spread) = self.ctx.arena.get_jsx_spread_attribute(node) {
                    self.collect_static_accesses_recursive(spread.expression, class_name, accesses);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.collect_static_accesses_recursive(paren.expression, class_name, accesses);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                    self.collect_static_accesses_recursive(cond.condition, class_name, accesses);
                    self.collect_static_accesses_recursive(cond.when_true, class_name, accesses);
                    self.collect_static_accesses_recursive(cond.when_false, class_name, accesses);
                }
            }
            k if k == syntax_kind_ext::JSX_ELEMENT => {
                // Check JSX element tag name for C.prop references
                if let Some(jsx_elem) = self.ctx.arena.get_jsx_element(node) {
                    if let Some(opening_node) = self.ctx.arena.get(jsx_elem.opening_element)
                        && let Some(opening) = self.ctx.arena.get_jsx_opening(opening_node)
                    {
                        // Recursively check tag name (might be C.x)
                        self.collect_static_accesses_recursive(
                            opening.tag_name,
                            class_name,
                            accesses,
                        );
                        // Also check attributes
                        if let Some(attrs_node) = self.ctx.arena.get(opening.attributes)
                            && let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node)
                        {
                            for &attr_idx in &attrs.properties.nodes {
                                self.collect_static_accesses_recursive(
                                    attr_idx, class_name, accesses,
                                );
                            }
                        }
                    }
                    if let Some(closing_node) = self.ctx.arena.get(jsx_elem.closing_element)
                        && let Some(closing) = self.ctx.arena.get_jsx_closing(closing_node)
                    {
                        // Also check closing tag name
                        self.collect_static_accesses_recursive(
                            closing.tag_name,
                            class_name,
                            accesses,
                        );
                    }
                }
            }

            _ => {
                // For other expressions, we don't recurse further to keep it simple
            }
        }
    }

    /// Collect all `this.propertyName` accesses in an expression.
    ///
    /// Stops at function boundaries where `this` context changes.
    ///
    /// ## Parameters
    /// - `node_idx`: The expression node index to search
    ///
    /// Returns a list of (`property_name`, `access_node`) tuples.
    pub(crate) fn collect_this_property_accesses(
        &self,
        node_idx: NodeIndex,
    ) -> Vec<(String, NodeIndex)> {
        let mut accesses = Vec::new();
        self.collect_this_accesses_recursive(node_idx, &mut accesses);
        accesses
    }

    /// Recursive helper to collect this.X accesses.
    ///
    /// Traverses the AST to find `this.property` expressions, stopping at
    /// function/class boundaries where `this` context changes (except arrow functions).
    ///
    /// ## Parameters
    /// - `node_idx`: The current node to examine
    /// - `accesses`: Accumulator for found accesses
    pub(crate) fn collect_this_accesses_recursive(
        &self,
        node_idx: NodeIndex,
        accesses: &mut Vec<(String, NodeIndex)>,
    ) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        // Stop at function boundaries where `this` context changes
        // (but not arrow functions, which preserve `this`)
        if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || node.kind == syntax_kind_ext::CLASS_DECLARATION
        {
            return;
        }

        // Property access uses AccessExprData with expression and name_or_argument
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if let Some(access) = self.ctx.arena.get_access_expr(node) {
                // Check if the expression is `this`
                if let Some(expr_node) = self.ctx.arena.get(access.expression) {
                    if expr_node.kind == SyntaxKind::ThisKeyword as u16 {
                        // Get the property name
                        if let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
                            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                        {
                            accesses.push((ident.escaped_text.clone(), node_idx));
                        }
                    } else {
                        // Recurse into the expression part
                        self.collect_this_accesses_recursive(access.expression, accesses);
                    }
                }
            }
            return;
        }

        // For other nodes, recurse into children based on node type
        match node.kind {
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(binary) = self.ctx.arena.get_binary_expr(node) {
                    self.collect_this_accesses_recursive(binary.left, accesses);
                    self.collect_this_accesses_recursive(binary.right, accesses);
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    self.collect_this_accesses_recursive(call.expression, accesses);
                    if let Some(ref args) = call.arguments {
                        for &arg in &args.nodes {
                            self.collect_this_accesses_recursive(arg, accesses);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.collect_this_accesses_recursive(paren.expression, accesses);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                    self.collect_this_accesses_recursive(cond.condition, accesses);
                    self.collect_this_accesses_recursive(cond.when_true, accesses);
                    self.collect_this_accesses_recursive(cond.when_false, accesses);
                }
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION => {
                // Arrow functions: while they preserve `this` context, property access
                // inside is deferred until the function is called. So we don't recurse
                // because the access doesn't happen during initialization.
                // (This matches TypeScript's behavior for error 2729)
            }
            _ => {
                // For other expressions, we don't recurse further to keep it simple
            }
        }
    }

    /// Check if a class member is an instance property (not static, not a method/accessor).
    ///
    /// ## Parameters
    /// - `member_idx`: The class member node index
    ///
    /// Returns true if the member is a non-static property declaration.
    pub(crate) fn is_instance_property(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            return false;
        }

        if let Some(prop) = self.ctx.arena.get_property_decl(node) {
            // Check if it has a static modifier
            return !self.has_static_modifier(&prop.modifiers);
        }

        false
    }

    // 18. AST Context Checking (4 functions)

    /// Get the name of a method declaration.
    ///
    /// Handles both identifier names and numeric literal names
    /// (for methods like `0()`, `1()`, etc.).
    ///
    /// ## Parameters
    /// - `member_idx`: The class member node index
    ///
    /// Returns the method name if found.
    pub(crate) fn get_method_name_from_node(&self, member_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(member_idx)?;

        if let Some(method) = self.ctx.arena.get_method_decl(node) {
            if let Some(name_node) = self.ctx.arena.get(method.name)
                && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            {
                return self.get_method_name_from_computed_property(name_node, method.name);
            }

            return self.get_property_name(method.name);
        }
        None
    }

    /// Get method name for computed method signatures.
    ///
    /// Computed identifiers are only overload-matchable when they are backed by
    /// `unique symbol` declarations, matching TypeScript's method implementation
    /// matching behavior.
    pub(crate) fn get_method_name_from_computed_property(
        &self,
        name_node: &tsz_parser::parser::node::Node,
        _name_idx: NodeIndex,
    ) -> Option<String> {
        let computed = self.ctx.arena.get_computed_property(name_node)?;

        if let Some(symbol_name) = self.get_symbol_property_name_from_expr(computed.expression) {
            return Some(symbol_name);
        }

        if let Some(expr_node) = self.ctx.arena.get(computed.expression) {
            if expr_node.kind == SyntaxKind::Identifier as u16
                && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
            {
                if self.identifier_refers_to_unique_symbol(computed.expression) {
                    return Some(ident.escaped_text.clone());
                }

                // Plain identifiers that are not unique symbols are not
                // overload-matchable — TSC skips TS2391 for these.
                return None;
            }

            if matches!(
                expr_node.kind,
                k if k == SyntaxKind::StringLiteral as u16
                    || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                    || k == SyntaxKind::NumericLiteral as u16
            ) && let Some(lit) = self.ctx.arena.get_literal(expr_node)
            {
                if expr_node.kind == SyntaxKind::NumericLiteral as u16 {
                    return tsz_solver::utils::canonicalize_numeric_name(&lit.text);
                }

                return Some(lit.text.clone());
            }
        }

        None
    }

    fn identifier_refers_to_unique_symbol(&self, name_node: NodeIndex) -> bool {
        let Some(sym_id) = self.resolve_symbol_id_from_identifier_node(name_node) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        symbol.declarations.iter().any(|decl_idx| {
            let Some(decl_node) = self.ctx.arena.get(*decl_idx) else {
                return false;
            };

            if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                return false;
            }

            let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                return false;
            };

            if var_decl.type_annotation.is_none() {
                return false;
            }

            self.is_unique_symbol_type_annotation(var_decl.type_annotation)
        })
    }

    fn resolve_symbol_id_from_identifier_node(&self, name_node: NodeIndex) -> Option<SymbolId> {
        if let Some(sym_id) = self.ctx.binder.get_node_symbol(name_node) {
            return Some(sym_id);
        }

        let ident = self.ctx.arena.get(name_node)?;
        let ident = self.ctx.arena.get_identifier(ident)?;

        self.ctx.binder.file_locals.get(&ident.escaped_text)
    }

    fn is_unique_symbol_type_annotation(&self, type_annotation: NodeIndex) -> bool {
        let Some(type_node) = self.ctx.arena.get(type_annotation) else {
            return false;
        };

        match type_node.kind {
            k if k == syntax_kind_ext::TYPE_OPERATOR => self
                .ctx
                .arena
                .get_type_operator(type_node)
                .is_some_and(|op| {
                    op.operator == SyntaxKind::UniqueKeyword as u16
                        && self.is_symbol_type_node(op.type_node)
                }),
            _ => false,
        }
    }

    fn is_symbol_type_node(&self, type_annotation: NodeIndex) -> bool {
        let Some(type_node) = self.ctx.arena.get(type_annotation) else {
            return false;
        };
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }

        let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) else {
            return false;
        };

        let Some(name_node) = self.ctx.arena.get(type_ref.type_name) else {
            return false;
        };

        self.ctx
            .arena
            .get_identifier(name_node)
            .is_some_and(|ident| ident.escaped_text == "symbol")
    }

    /// Get a method declaration name for diagnostics.
    ///
    /// This is display-oriented and preserves syntax details for computed/property
    /// names in error messages (e.g. `"foo"`, `["bar"]`).
    pub(crate) fn get_method_name_for_diagnostic(&self, member_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(member_idx)?;

        if let Some(method) = self.ctx.arena.get_method_decl(node) {
            let name_node = self.ctx.arena.get(method.name)?;

            if let Some(id) = self.ctx.arena.get_identifier(name_node) {
                return Some(id.escaped_text.clone());
            }

            if let Some(lit) = self.ctx.arena.get_literal(name_node) {
                return Some(match name_node.kind {
                    k if k == SyntaxKind::StringLiteral as u16
                        || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                    {
                        format!("\"{}\"", lit.text.clone())
                    }
                    k if k == SyntaxKind::NumericLiteral as u16 => {
                        if let Some(canonical) =
                            tsz_solver::utils::canonicalize_numeric_name(&lit.text)
                        {
                            canonical
                        } else {
                            lit.text.clone()
                        }
                    }
                    _ => lit.text.clone(),
                });
            }

            if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                && let Some(computed) = self.ctx.arena.get_computed_property(name_node)
            {
                if let Some(symbol_name) =
                    self.get_symbol_property_name_from_expr(computed.expression)
                {
                    return Some(format!("[{symbol_name}]"));
                }

                if let Some(expr_node) = self.ctx.arena.get(computed.expression) {
                    if let Some(id) = self.ctx.arena.get_identifier(expr_node) {
                        return Some(format!("[{}]", id.escaped_text));
                    }

                    if let Some(lit) = self.ctx.arena.get_literal(expr_node) {
                        return Some(match expr_node.kind {
                            kind if kind == SyntaxKind::StringLiteral as u16
                                || kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                            {
                                format!("[\"{}\"]", lit.text)
                            }
                            kind if kind == SyntaxKind::NumericLiteral as u16 => {
                                if let Some(canonical) =
                                    tsz_solver::utils::canonicalize_numeric_name(&lit.text)
                                {
                                    format!("[{canonical}]")
                                } else {
                                    format!("[{}]", lit.text)
                                }
                            }
                            _ => format!("[{}]", lit.text),
                        });
                    }
                }
            }
        }

        None
    }

    /// Check if a function is within a namespace or module context.
    ///
    /// Uses AST-based parent traversal to detect `ModuleDeclaration` in the parent chain.
    ///
    /// ## Parameters
    /// - `func_idx`: The function node index
    ///
    /// Returns true if the function is inside a namespace/module declaration.
    pub fn is_in_namespace_context(&self, func_idx: NodeIndex) -> bool {
        // Walk up the parent chain looking for ModuleDeclaration nodes
        let mut current = func_idx;

        while current.is_some() {
            if let Some(node) = self.ctx.arena.get(current) {
                // Check if this node is a ModuleDeclaration (namespace or module)
                if node.kind == syntax_kind_ext::MODULE_DECLARATION {
                    return true;
                }
            }

            // Move to the parent node
            if let Some(ext) = self.ctx.arena.get_extended(current) {
                current = ext.parent;
            } else {
                break;
            }
        }

        false
    }

    /// Check if a variable is declared in an ambient context (declare keyword).
    ///
    /// This uses proper AST-based detection by:
    /// 1. Checking the node's flags for the AMBIENT flag
    /// 2. Walking up the parent chain to find if enclosed in an ambient context
    /// 3. Checking modifiers on declaration nodes for `DeclareKeyword`
    ///
    /// ## Parameters
    /// - `var_idx`: The variable declaration node index
    ///
    /// Returns true if the declaration is in an ambient context.
    pub(crate) fn is_ambient_declaration(&self, var_idx: NodeIndex) -> bool {
        use tsz_parser::parser::node_flags;

        // Declarations inside .d.ts files are ambient by definition.
        if self.ctx.file_name.ends_with(".d.ts") {
            return true;
        }

        let mut current = var_idx;
        while current.is_some() {
            if let Some(node) = self.ctx.arena.get(current) {
                // Check if this node has the AMBIENT flag set
                if (node.flags as u32) & node_flags::AMBIENT != 0 {
                    return true;
                }

                // Check modifiers on various declaration types for DeclareKeyword
                // Variable statements
                if let Some(var_stmt) = self.ctx.arena.get_variable(node)
                    && self.has_declare_modifier(&var_stmt.modifiers)
                {
                    return true;
                }
                // Function declarations
                if let Some(func) = self.ctx.arena.get_function(node)
                    && self.has_declare_modifier(&func.modifiers)
                {
                    return true;
                }
                // Class declarations
                if let Some(class) = self.ctx.arena.get_class(node)
                    && self.has_declare_modifier(&class.modifiers)
                {
                    return true;
                }
                // Enum declarations
                if let Some(enum_decl) = self.ctx.arena.get_enum(node)
                    && self.has_declare_modifier(&enum_decl.modifiers)
                {
                    return true;
                }
                // Interface declarations (interfaces are implicitly ambient)
                if self.ctx.arena.get_interface(node).is_some() {
                    return true;
                }
                // Type alias declarations (type aliases are implicitly ambient)
                if self.ctx.arena.get_type_alias(node).is_some() {
                    return true;
                }
                // Module/namespace declarations
                if let Some(module) = self.ctx.arena.get_module(node)
                    && self.has_declare_modifier(&module.modifiers)
                {
                    return true;
                }
            }

            // Move to parent node
            if let Some(ext) = self.ctx.arena.get_extended(current) {
                if ext.parent.is_none() {
                    break;
                }
                current = ext.parent;
            } else {
                break;
            }
        }

        false
    }

    // 19. Type and Name Checking Utilities (8 functions)

    /// Check if a type name is a mapped type utility.
    ///
    /// Mapped type utilities are TypeScript built-in utility types
    /// that transform mapped types.
    ///
    /// ## Parameters
    /// - `name`: The type name to check
    ///
    /// Returns true if the name is a mapped type utility.
    pub(crate) fn is_mapped_type_utility(&self, name: &str) -> bool {
        matches!(
            name,
            "Partial"
                | "Required"
                | "Readonly"
                | "Record"
                | "Pick"
                | "Omit"
                | "Extract"
                | "Exclude"
                | "NonNullable"
                | "ThisType"
                | "Infer"
        )
    }

    /// Check if a type name is a known global type.
    ///
    /// Known global types include built-in JavaScript/TypeScript types
    /// like Object, Array, Promise, Map, etc.
    ///
    /// ## Parameters
    /// - `name`: The type name to check
    ///
    /// Returns true if the name is a known global type.
    pub(crate) fn is_known_global_type_name(&self, name: &str) -> bool {
        if self.ctx.is_known_global_type(name) {
            return true;
        }

        matches!(
            name,
            // Core built-in objects
            "Object"
                | "String"
                | "Number"
                | "Boolean"
                | "Symbol"
                | "Function"
                | "Date"
                | "RegExp"
                | "RegExpExecArray"
                | "RegExpMatchArray"
                // Arrays and collections
                | "Array"
                | "ReadonlyArray"
                | "ArrayLike"
                | "ArrayBuffer"
                | "SharedArrayBuffer"
                | "DataView"
                | "TypedArray"
                | "Int8Array"
                | "Uint8Array"
                | "Uint8ClampedArray"
                | "Int16Array"
                | "Uint16Array"
                | "Int32Array"
                | "Uint32Array"
                | "Float32Array"
                | "Float64Array"
                | "BigInt64Array"
                | "BigUint64Array"
                // ES2015+ collection types
                | "Map"
                | "Set"
                | "WeakMap"
                | "WeakSet"
                | "WeakRef"
                | "ReadonlyMap"
                | "ReadonlySet"
                // Promise types
                | "Promise"
                | "PromiseConstructor"
                | "PromiseConstructorLike"
                | "Awaited"
                // Iterator/Generator types
                | "Iterator"
                | "IteratorResult"
                | "IteratorYieldResult"
                | "IteratorReturnResult"
                | "IterableIterator"
                | "AsyncIterator"
                | "AsyncIterable"
                | "AsyncIterableIterator"
                | "Generator"
                | "GeneratorFunction"
                | "AsyncGenerator"
                | "AsyncGeneratorFunction"
                // Utility types
                | "Partial"
                | "Required"
                | "Readonly"
                | "Record"
                | "Pick"
                | "Omit"
                | "NonNullable"
                | "Extract"
                | "Exclude"
                | "ReturnType"
                | "Parameters"
                | "ConstructorParameters"
                | "InstanceType"
                | "ThisParameterType"
                | "OmitThisParameter"
                | "ThisType"
                | "Uppercase"
                | "Lowercase"
                | "Capitalize"
                | "Uncapitalize"
                | "NoInfer"
                // Object types
                | "PropertyKey"
                | "PropertyDescriptor"
                | "PropertyDescriptorMap"
                | "ObjectConstructor"
                | "FunctionConstructor"
                // Error types
                | "Error"
                | "ErrorConstructor"
                | "TypeError"
                | "RangeError"
                | "EvalError"
                | "URIError"
                | "ReferenceError"
                | "SyntaxError"
                | "AggregateError"
                // Math and JSON
                | "Math"
                | "JSON"
                // Proxy and Reflect
                | "Proxy"
                | "ProxyHandler"
                | "Reflect"
                // BigInt
                | "BigInt"
                | "BigIntConstructor"
                // ES2021+
                | "FinalizationRegistry"
                // DOM types (commonly used)
                | "Element"
                | "HTMLElement"
                | "Document"
                | "Window"
                | "Event"
                | "EventTarget"
                | "NodeList"
                | "NodeListOf"
                | "Console"
                | "Atomics"
                // Primitive types (lowercase)
                | "number"
                | "string"
                | "boolean"
                | "void"
                | "null"
                | "undefined"
                | "never"
                | "unknown"
                | "any"
                | "object"
                | "bigint"
                | "symbol"
        )
    }

    /// Check if a type is a constructor type.
    ///
    /// A constructor type has construct signatures (can be called with `new`).
    ///
    /// ## Parameters
    /// - `type_id`: The type ID to check
    ///
    /// Returns true if the type is a constructor type.
    /// Replace `Function` type members with a callable type for call resolution.
    ///
    /// When the callee type is exactly the Function type, returns `TypeId::ANY` directly.
    /// When the callee type is a union containing Function members, replaces those
    /// members with a synthetic function `(...args: any[]) => any` so that
    /// `resolve_union_call` in the solver can handle it.
    pub(crate) fn replace_function_type_for_call(
        &mut self,
        callee_type_orig: TypeId,
        callee_type_for_call: TypeId,
    ) -> TypeId {
        // Direct Function type - return ANY (which is callable)
        if self.is_global_function_type(callee_type_orig) {
            return TypeId::ANY;
        }

        // Check if callee_type_for_call is a union containing Function members
        if let Some(members_vec) = query::union_members(self.ctx.types, callee_type_for_call) {
            let members = members_vec;
            let orig_members = query::union_members(self.ctx.types, callee_type_orig);
            let factory = self.ctx.types.factory();

            let mut has_function = false;
            let mut new_members = Vec::new();

            for (i, &member) in members.iter().enumerate() {
                // Check if this member corresponds to a Function type in the original
                let is_func = if let Some(ref orig) = orig_members {
                    i < orig.len() && self.is_global_function_type(orig[i])
                } else {
                    false
                };

                if is_func {
                    has_function = true;
                    // Replace Function member with a synthetic callable returning any
                    // Use a simple function: (...args: any[]) => any
                    let rest_param = tsz_solver::ParamInfo {
                        name: Some(self.ctx.types.intern_string("args")),
                        type_id: TypeId::ANY,
                        optional: false,
                        rest: true,
                    };
                    let func_shape = tsz_solver::FunctionShape {
                        params: vec![rest_param],
                        this_type: None,
                        return_type: TypeId::ANY,
                        type_params: vec![],
                        type_predicate: None,
                        is_constructor: false,
                        is_method: false,
                    };
                    let func_type = factory.function(func_shape);
                    new_members.push(func_type);
                } else {
                    new_members.push(member);
                }
            }

            if has_function {
                return factory.union(new_members);
            }
        }

        callee_type_for_call
    }

    /// Check if a type is the global `Function` interface type from lib.d.ts.
    ///
    /// In TypeScript, the `Function` type is callable (returns `any`) even though
    /// the `Function` interface has no call signatures. This method identifies
    /// the Function type so the caller can handle it specially.
    pub(crate) fn is_global_function_type(&mut self, type_id: TypeId) -> bool {
        // Quick check for the intrinsic Function type
        if type_id == TypeId::FUNCTION {
            return true;
        }

        // Check if the type matches the global Function interface type.
        // The Function type annotation resolves to a Lazy(DefId) pointing to the
        // Function symbol. We look up the global Function symbol and compare.
        let lib_binders = self.get_lib_binders();
        if let Some(func_sym_id) = self
            .ctx
            .binder
            .get_global_type_with_libs("Function", &lib_binders)
        {
            let func_type = self.type_reference_symbol_type(func_sym_id);
            if type_id == func_type {
                return true;
            }
        }

        // Also check union members: if all callable members resolve through Function
        // (e.g., `Function | (() => void)` should still be callable)
        false
    }

    pub(crate) fn is_constructor_type(&self, type_id: TypeId) -> bool {
        // Any type is always considered a constructor type (TypeScript compatibility)
        if type_id == TypeId::ANY {
            return true;
        }

        // First check if it directly has construct signatures
        if query::has_construct_signatures(self.ctx.types, type_id) {
            return true;
        }

        // Check if type has a prototype property (functions with prototype are constructable)
        // This handles cases like `function Foo() {}` where `Foo.prototype` exists
        if self.type_has_prototype_property(type_id) {
            return true;
        }

        // For type parameters, check if the constraint is a constructor type
        // For intersection types, check if any member is a constructor type
        // For application types, check if the base type is a constructor type
        match query::classify_for_constructor_check(self.ctx.types, type_id) {
            query::ConstructorCheckKind::TypeParameter { constraint } => {
                if let Some(constraint) = constraint {
                    self.is_constructor_type(constraint)
                } else {
                    false
                }
            }
            query::ConstructorCheckKind::Intersection(members) => {
                members.iter().any(|&m| self.is_constructor_type(m))
            }
            query::ConstructorCheckKind::Union(members) => {
                // Union types are constructable if ALL members are constructable
                // This matches TypeScript's behavior where `type A | B` used in extends
                // requires both A and B to be constructors
                !members.is_empty() && members.iter().all(|&m| self.is_constructor_type(m))
            }
            query::ConstructorCheckKind::Application { base } => {
                // For type applications like Ctor<{}>, check if the base type is a constructor
                // This handles cases like:
                //   type Constructor<T> = new (...args: any[]) => T;
                //   function f<T extends Constructor<{}>>(x: T) {
                //     class C extends x {}  // x should be valid here
                //   }
                // Only check the base - don't recurse further to avoid infinite loops
                // Check if base is a Lazy type to a type alias with constructor type body
                if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(base)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    && let Some(decl_idx) = symbol.declarations.first().copied()
                    && let Some(decl_node) = self.ctx.arena.get(decl_idx)
                    && decl_node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    && let Some(alias) = self.ctx.arena.get_type_alias(decl_node)
                    && let Some(body_node) = self.ctx.arena.get(alias.type_node)
                {
                    // Constructor type syntax: new (...args) => T
                    if body_node.kind == tsz_parser::parser::syntax_kind_ext::CONSTRUCTOR_TYPE {
                        return true;
                    }
                }
                // Also check if base is directly a Callable with construct signatures
                query::has_construct_signatures(self.ctx.types, base)
            }
            // Lazy reference (DefId) - check if it's a class or interface
            // This handles cases like:
            // 1. `class C extends MyClass` where MyClass is a class
            // 2. `function f<T>(ctor: T)` then `class B extends ctor` where ctor has a constructor type
            // 3. `class C extends Object` where Object is declared as ObjectConstructor interface
            query::ConstructorCheckKind::Lazy(def_id) => {
                let symbol_id = match self.ctx.def_to_symbol_id(def_id) {
                    Some(id) => id,
                    None => return false,
                };
                if let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) {
                    // Check if this is a class symbol - classes are always constructors
                    if (symbol.flags & tsz_binder::symbol_flags::CLASS) != 0 {
                        return true;
                    }

                    // Check if this is an interface symbol with construct signatures
                    // This handles cases like ObjectConstructor, ArrayConstructor, etc.
                    // which are interfaces with `new()` signatures
                    if (symbol.flags & tsz_binder::symbol_flags::INTERFACE) != 0 {
                        // Check the cached type for interface - it should be Callable if it has construct signatures
                        if let Some(&cached_type) = self.ctx.symbol_types.get(&symbol_id) {
                            if cached_type != type_id {
                                // Interface type was already resolved - check if it has construct signatures
                                if query::has_construct_signatures(self.ctx.types, cached_type) {
                                    return true;
                                }
                            }
                        } else if !symbol.declarations.is_empty() {
                            // Interface not cached - check if it has construct signatures by examining declarations
                            // This handles lib.d.ts interfaces like ObjectConstructor that may not be resolved yet
                            // IMPORTANT: Use the correct arena for the symbol (may be different for lib types)
                            use tsz_lowering::TypeLowering;
                            let symbol_arena = self
                                .ctx
                                .binder
                                .symbol_arenas
                                .get(&symbol_id)
                                .map_or(self.ctx.arena, |arena| arena.as_ref());

                            let type_param_bindings = self.get_type_param_bindings();
                            let type_resolver = |node_idx: tsz_parser::parser::NodeIndex| {
                                self.resolve_type_symbol_for_lowering(node_idx)
                            };
                            let value_resolver = |node_idx: tsz_parser::parser::NodeIndex| {
                                self.resolve_value_symbol_for_lowering(node_idx)
                            };
                            let lowering = TypeLowering::with_resolvers(
                                symbol_arena,
                                self.ctx.types,
                                &type_resolver,
                                &value_resolver,
                            )
                            .with_type_param_bindings(type_param_bindings);
                            let interface_type =
                                lowering.lower_interface_declarations(&symbol.declarations);
                            if query::has_construct_signatures(self.ctx.types, interface_type) {
                                return true;
                            }
                        }
                    }

                    // For other symbols (variables, parameters, type aliases), check their cached type
                    // This handles cases like:
                    //   function f<T extends typeof A>(ctor: T) {
                    //     class B extends ctor {}  // ctor should be recognized as constructible
                    //   }
                    if let Some(&cached_type) = self.ctx.symbol_types.get(&symbol_id) {
                        // Recursively check if the resolved type is a constructor
                        // Avoid infinite recursion by checking if cached_type == type_id
                        if cached_type != type_id {
                            return self.is_constructor_type(cached_type);
                        }
                    }
                }
                // For other symbols (namespaces, enums, etc.) without cached types, they're not constructors
                false
            }
            // TypeQuery (typeof X) - similar to Ref but for typeof expressions
            // This handles cases like:
            //   class A {}
            //   function f<T extends typeof A>(ctor: T) {
            //     class B extends ctor {}  // ctor: T where T extends typeof A
            //   }
            query::ConstructorCheckKind::TypeQuery(symbol_ref) => {
                use tsz_binder::SymbolId;
                let symbol_id = SymbolId(symbol_ref.0);
                if let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) {
                    // Classes have constructor types
                    if (symbol.flags & tsz_binder::symbol_flags::CLASS) != 0 {
                        return true;
                    }

                    // Check cached type for variables/parameters with constructor types
                    if let Some(&cached_type) = self.ctx.symbol_types.get(&symbol_id) {
                        // Recursively check if the resolved type is a constructor
                        // Avoid infinite recursion by checking if cached_type == type_id
                        if cached_type != type_id {
                            return self.is_constructor_type(cached_type);
                        }
                    }
                }
                false
            }
            query::ConstructorCheckKind::Other => false,
        }
    }

    /// Check if an expression is a property access to a get accessor.
    ///
    /// Used to emit TS6234 instead of TS2349 when a getter is accidentally called:
    /// ```typescript
    /// class Test { get property(): number { return 1; } }
    /// x.property(); // TS6234: not callable because it's a get accessor
    /// ```
    pub(crate) fn is_get_accessor_call(&self, expr_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
            return false;
        };

        // Get the property name
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        let prop_name = &ident.escaped_text;

        // Check via symbol flags if the property is a getter
        if let Some(sym_id) = self
            .ctx
            .binder
            .node_symbols
            .get(&access.name_or_argument.0)
            .copied()
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.flags & tsz_binder::symbol_flags::GET_ACCESSOR) != 0
        {
            return true;
        }

        // Check if the object type is a class instance with a get accessor for this property
        if let Some(&obj_type) = self.ctx.node_types.get(&access.expression.0)
            && let Some(class_idx) = self.ctx.class_instance_type_to_decl.get(&obj_type).copied()
            && let Some(class) = self.ctx.arena.get_class_at(class_idx)
        {
            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                    && let Some(accessor) = self.ctx.arena.get_accessor(member_node)
                    && let Some(acc_ident) = self.ctx.arena.get_identifier_at(accessor.name)
                    && acc_ident.escaped_text == *prop_name
                {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a type has a 'prototype' property.
    ///
    /// Functions with a prototype property can be used as constructors.
    /// This handles cases like:
    /// ```typescript
    /// function Foo() {}
    /// new Foo(); // Valid if Foo.prototype exists
    /// ```
    pub(crate) fn type_has_prototype_property(&self, type_id: TypeId) -> bool {
        // Check callable shape for prototype property
        if let Some(shape) = query::callable_shape_for_type(self.ctx.types, type_id) {
            let prototype_atom = self.ctx.types.intern_string("prototype");
            return shape.properties.iter().any(|p| p.name == prototype_atom);
        }

        // Function types typically have prototype
        query::has_function_shape(self.ctx.types, type_id)
    }

    /// Check if a symbol is a class symbol.
    ///
    /// ## Parameters
    /// - `symbol_id`: The symbol ID to check
    ///
    /// Returns true if the symbol represents a class.
    pub(crate) fn is_class_symbol(&self, symbol_id: tsz_binder::SymbolId) -> bool {
        use tsz_binder::symbol_flags;
        if let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) {
            (symbol.flags & symbol_flags::CLASS) != 0
        } else {
            false
        }
    }

    /// Check if an expression is a numeric literal with value 0.
    ///
    /// ## Parameters
    /// - `expr_idx`: The expression node index
    ///
    /// Returns true if the expression is the literal 0.
    pub(crate) fn is_numeric_literal_zero(&self, expr_idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != SyntaxKind::NumericLiteral as u16 {
            return false;
        }
        let Some(lit) = self.ctx.arena.get_literal(node) else {
            return false;
        };
        lit.text == "0"
    }

    /// Check if an expression is a property or element access expression.
    ///
    /// ## Parameters
    /// - `expr_idx`: The expression node index
    ///
    /// Returns true if the expression is a property or element access.
    pub(crate) fn is_access_expression(&self, expr_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        matches!(
            node.kind,
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        )
    }

    /// Check if a statement is a `super()` call.
    ///
    /// ## Parameters
    /// - `stmt_idx`: The statement node index
    ///
    /// Returns true if the statement is an expression statement calling `super()`.
    pub(crate) fn is_super_call_statement(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return false;
        }
        let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) else {
            return false;
        };
        let Some(expr_node) = self.ctx.arena.get(expr_stmt.expression) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        };
        let Some(call) = self.ctx.arena.get_call_expr(expr_node) else {
            return false;
        };
        let Some(callee_node) = self.ctx.arena.get(call.expression) else {
            return false;
        };
        callee_node.kind == SyntaxKind::SuperKeyword as u16
    }

    /// Check if a parameter name is "this".
    ///
    /// ## Parameters
    /// - `name_idx`: The parameter name node index
    ///
    /// Returns true if the parameter name is "this".
    pub(crate) fn is_this_parameter_name(&self, name_idx: NodeIndex) -> bool {
        if let Some(name_node) = self.ctx.arena.get(name_idx) {
            if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                return true;
            }
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                return ident.escaped_text == "this";
            }
        }
        false
    }

    // 20. Declaration and Node Checking Utilities (6 functions)

    /// Check if a variable declaration is in a const declaration list.
    ///
    /// ## Parameters
    /// - `var_decl_idx`: The variable declaration node index
    ///
    /// Returns true if the variable is declared with `const`.
    pub(crate) fn is_const_variable_declaration(&self, var_decl_idx: NodeIndex) -> bool {
        use tsz_parser::parser::node_flags;

        let Some(ext) = self.ctx.arena.get_extended(var_decl_idx) else {
            return false;
        };
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return false;
        }
        (parent_node.flags as u32) & node_flags::CONST != 0
    }

    /// Check if an initializer expression is an `as const` assertion.
    /// For `let x = "div" as const`, the initializer is the `"div" as const` expression.
    /// TypeScript preserves literal types for `as const` even on mutable bindings.
    pub(crate) fn is_const_assertion_initializer(&self, init_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(init_idx) else {
            return false;
        };
        if let Some(assertion) = self.ctx.arena.get_type_assertion(node)
            && let Some(type_node) = self.ctx.arena.get(assertion.type_node)
        {
            return type_node.kind == tsz_scanner::SyntaxKind::ConstKeyword as u16;
        }
        false
    }

    /// Check if an initializer is a valid const initializer for ambient contexts.
    /// Valid initializers are string/numeric/bigint literals and enum references.
    pub(crate) fn is_valid_ambient_const_initializer(&self, init_idx: NodeIndex) -> bool {
        use tsz_binder::symbol_flags;

        let Some(node) = self.ctx.arena.get(init_idx) else {
            return false;
        };
        match node.kind {
            k if k == tsz_scanner::SyntaxKind::StringLiteral as u16
                || k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == tsz_scanner::SyntaxKind::NumericLiteral as u16
                || k == tsz_scanner::SyntaxKind::BigIntLiteral as u16 =>
            {
                true
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr(node)
                    && unary.operator == tsz_scanner::SyntaxKind::MinusToken as u16
                    && let Some(operand) = self.ctx.arena.get(unary.operand)
                {
                    return operand.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16
                        || operand.kind == tsz_scanner::SyntaxKind::BigIntLiteral as u16;
                }
                false
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                let Some(access) = self.ctx.arena.get_access_expr(node) else {
                    return false;
                };
                let Some(sym_id) = self.resolve_identifier_symbol(access.expression) else {
                    return false;
                };
                let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                    return false;
                };
                if symbol.flags & symbol_flags::ENUM == 0 {
                    return false;
                }
                if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    return true;
                }
                let Some(arg_node) = self.ctx.arena.get(access.name_or_argument) else {
                    return false;
                };
                arg_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
                    || arg_node.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16
                    || arg_node.kind
                        == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16
            }
            _ => false,
        }
    }

    /// Check if a class declaration has the declare modifier (is ambient).
    ///
    /// ## Parameters
    /// - `decl_idx`: The declaration node index
    ///
    /// Returns true if the class is an ambient declaration.
    pub(crate) fn is_ambient_class_declaration(&self, decl_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CLASS_DECLARATION {
            return false;
        }
        let Some(class) = self.ctx.arena.get_class(node) else {
            return false;
        };
        // Check for explicit `declare` modifier
        if self.has_declare_modifier(&class.modifiers) {
            return true;
        }
        // Check if the class is inside a `declare namespace`/`declare module`
        // by walking up the parent chain to find an ambient module declaration
        let mut current = decl_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent)
                && parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = self.ctx.arena.get_module(parent_node)
                && self.has_declare_modifier(&module.modifiers)
            {
                return true;
            }
            current = parent;
        }
        false
    }

    /// Check if a declaration is inside a `declare namespace` or `declare module` context.
    /// This is different from `is_ambient_declaration` which also treats interfaces and type
    /// aliases as implicitly ambient.
    pub(crate) fn is_in_declare_namespace_or_module(&self, decl_idx: NodeIndex) -> bool {
        let mut current = decl_idx;
        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = self.ctx.arena.get_module(parent_node)
                && self.has_declare_modifier(&module.modifiers)
            {
                return true;
            }
            if parent_node.kind == syntax_kind_ext::SOURCE_FILE {
                return false;
            }
            current = parent;
        }
    }

    /// Check if any declaration node is exported (has export keyword).
    /// Handles all declaration kinds: function, class, interface, enum, type alias,
    /// module/namespace, and variable declarations.
    /// The parser wraps `export <decl>` as `ExportDeclaration → <inner decl>`, so
    /// we check both the node's own modifiers and whether its parent is `ExportDeclaration`.
    pub(crate) fn is_declaration_exported(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        decl_idx: NodeIndex,
    ) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(node) = arena.get(decl_idx) else {
            return false;
        };

        // Helper: check if this node's direct parent is an ExportDeclaration wrapper.
        let parent_is_export_decl = || {
            arena
                .get_extended(decl_idx)
                .and_then(|ext| arena.get(ext.parent))
                .is_some_and(|parent| parent.kind == syntax_kind_ext::EXPORT_DECLARATION)
        };

        let has_export = |modifiers: &Option<tsz_parser::parser::NodeList>| {
            if let Some(list) = modifiers {
                list.nodes.iter().any(|&idx| {
                    arena
                        .get(idx)
                        .is_some_and(|n| n.kind == SyntaxKind::ExportKeyword as u16)
                })
            } else {
                false
            }
        };

        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                arena
                    .get_function(node)
                    .is_some_and(|func| has_export(&func.modifiers))
                    || parent_is_export_decl()
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                arena
                    .get_class(node)
                    .is_some_and(|class| has_export(&class.modifiers))
                    || parent_is_export_decl()
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                arena
                    .get_interface(node)
                    .is_some_and(|iface| has_export(&iface.modifiers))
                    || parent_is_export_decl()
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                arena
                    .get_enum(node)
                    .is_some_and(|enm| has_export(&enm.modifiers))
                    || parent_is_export_decl()
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                arena
                    .get_type_alias(node)
                    .is_some_and(|alias| has_export(&alias.modifiers))
                    || parent_is_export_decl()
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                arena
                    .get_module(node)
                    .is_some_and(|module| has_export(&module.modifiers))
                    || parent_is_export_decl()
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                // Variable declaration exports are on the statement
                if let Some(ext) = arena.get_extended(decl_idx)
                    && let Some(list_node) = arena.get(ext.parent)
                    && list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                    && let Some(list_ext) = arena.get_extended(ext.parent)
                    && let Some(stmt_node) = arena.get(list_ext.parent)
                    && stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                    && let Some(stmt) = arena.get_variable(stmt_node)
                {
                    return has_export(&stmt.modifiers)
                        || arena
                            .get_extended(list_ext.parent)
                            .and_then(|e| arena.get(e.parent))
                            .is_some_and(|p| p.kind == syntax_kind_ext::EXPORT_DECLARATION);
                }
                false
            }
            _ => false,
        }
    }

    /// Check if a function declaration has the declare modifier (is ambient).
    pub(crate) fn is_ambient_function_declaration(&self, decl_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
            return false;
        }
        let Some(function) = self.ctx.arena.get_function(node) else {
            return false;
        };
        if self.has_declare_modifier(&function.modifiers) {
            return true;
        }

        let mut current = decl_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent)
                && parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = self.ctx.arena.get_module(parent_node)
                && self.has_declare_modifier(&module.modifiers)
            {
                return true;
            }
            current = parent;
        }
        false
    }

    /// Check whether a namespace declaration is instantiated (has runtime value declarations).
    pub(crate) fn is_namespace_declaration_instantiated(&self, namespace_idx: NodeIndex) -> bool {
        let Some(namespace_node) = self.ctx.arena.get(namespace_idx) else {
            return false;
        };
        if namespace_node.kind != syntax_kind_ext::MODULE_DECLARATION {
            return false;
        }
        let Some(module_decl) = self.ctx.arena.get_module(namespace_node) else {
            return false;
        };
        self.module_body_has_runtime_members(module_decl.body)
    }

    fn module_body_has_runtime_members(&self, body_idx: NodeIndex) -> bool {
        if body_idx.is_none() {
            return false;
        }
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };

        if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            return self.is_namespace_declaration_instantiated(body_idx);
        }

        if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            return false;
        }

        let Some(module_block) = self.ctx.arena.get_module_block(body_node) else {
            return false;
        };
        let Some(statements) = &module_block.statements else {
            return false;
        };

        for &statement_idx in &statements.nodes {
            let Some(statement_node) = self.ctx.arena.get(statement_idx) else {
                continue;
            };
            match statement_node.kind {
                k if k == syntax_kind_ext::VARIABLE_STATEMENT
                    || k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::ENUM_DECLARATION
                    || k == syntax_kind_ext::EXPRESSION_STATEMENT
                    || k == syntax_kind_ext::EXPORT_ASSIGNMENT =>
                {
                    return true;
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    if self.is_namespace_declaration_instantiated(statement_idx) {
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    /// Check if a method declaration has a body (is an implementation, not just a signature).
    ///
    /// ## Parameters
    /// - `decl_idx`: The method declaration node index
    ///
    /// Returns true if the method has a body.
    pub(crate) fn method_has_body(&self, decl_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::METHOD_DECLARATION {
            return false;
        }
        let Some(method) = self.ctx.arena.get_method_decl(node) else {
            return false;
        };
        method.body.is_some()
    }

    /// Get the name node of a declaration for error reporting.
    ///
    /// ## Parameters
    /// - `decl_idx`: The declaration node index
    ///
    /// Returns the name node if the declaration has one.
    pub(crate) fn get_declaration_name_node(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(decl_idx)?;

        match node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let var_decl = self.ctx.arena.get_variable_declaration(node)?;
                Some(var_decl.name)
            }
            syntax_kind_ext::FUNCTION_DECLARATION => {
                let func = self.ctx.arena.get_function(node)?;
                Some(func.name)
            }
            syntax_kind_ext::CLASS_DECLARATION => {
                let class = self.ctx.arena.get_class(node)?;
                Some(class.name)
            }
            syntax_kind_ext::INTERFACE_DECLARATION => {
                let interface = self.ctx.arena.get_interface(node)?;
                Some(interface.name)
            }
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let type_alias = self.ctx.arena.get_type_alias(node)?;
                Some(type_alias.name)
            }
            syntax_kind_ext::ENUM_DECLARATION => {
                let enum_decl = self.ctx.arena.get_enum(node)?;
                Some(enum_decl.name)
            }
            syntax_kind_ext::IMPORT_CLAUSE => {
                let clause = self.ctx.arena.get_import_clause(node)?;
                Some(clause.name)
            }
            syntax_kind_ext::NAMESPACE_IMPORT => {
                let named = self.ctx.arena.get_named_imports(node)?;
                Some(named.name)
            }
            syntax_kind_ext::IMPORT_SPECIFIER | syntax_kind_ext::EXPORT_SPECIFIER => {
                let spec = self.ctx.arena.get_specifier(node)?;
                if spec.name.is_some() {
                    Some(spec.name)
                } else {
                    Some(spec.property_name)
                }
            }
            syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                let import = self.ctx.arena.get_import_decl(node)?;
                Some(import.import_clause)
            }
            _ => None,
        }
    }

    // Interface merge compatibility, name matching, property name utilities,
    // and node containment are in `type_checking_declarations_utils.rs`.
}
