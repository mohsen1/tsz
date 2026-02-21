//! Property initialization order checking (TS2729) and property attribute detection.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

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
}
