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

        let Some(summary) = self.summarize_enclosing_class_initialization() else {
            return;
        };

        let Some(current_pos) = summary.member_position(current_prop_idx) else {
            return;
        };

        // Collect all `this.X` property accesses in the initializer
        let accesses = self.collect_this_property_accesses(initializer_idx);

        // When useDefineForClassFields is true (target >= ES2022), class field
        // initializers run via Object.defineProperty BEFORE the constructor body.
        // Parameter properties (public/private/protected/readonly constructor params)
        // are assigned in the constructor body, so they are uninitialized during
        // field definition. We need to check for this case.
        let use_define = self.ctx.compiler_options.target.supports_es2022();

        for (name, access_node_idx) in accesses {
            let own_property = summary.instance_property_named(&name);

            // If name wasn't found in class body members but matches a constructor
            // parameter property, report TS2729 when useDefineForClassFields is true.
            // Parameter properties are always assigned in the constructor body, which
            // runs after field definitions with useDefineForClassFields semantics.
            if let Some(target) = own_property {
                let should_error = target.position > current_pos
                    || target.is_abstract
                    || target.has_no_initializer;
                if should_error {
                    // If an ancestor class declares the same name with an
                    // initializer, base construction already ran and
                    // `this.x` sees the base value — not an error. Matches
                    // tsc's behavior for `class D extends C { old_x = this.x; x = 1 }`.
                    //
                    // Exception: when `useDefineForClassFields` is true and the
                    // current class redeclares the property WITHOUT an initializer,
                    // `Object.defineProperty` overwrites the base's value with
                    // `undefined`. The ancestor's initializer no longer applies.
                    let redeclared_without_init = use_define && target.has_no_initializer;
                    if !redeclared_without_init && self.ancestor_class_initializes_property(&name) {
                        continue;
                    }
                    self.error_at_node(
                        access_node_idx,
                        &format!("Property '{name}' is used before its initialization."),
                        diagnostic_codes::PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION,
                    );
                }
            } else if use_define && summary.parameter_property_names.contains(&name) {
                self.error_at_node(
                    access_node_idx,
                    &format!("Property '{name}' is used before its initialization."),
                    diagnostic_codes::PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION,
                );
            }
        }
    }

    /// Walk up the enclosing class's base chain and check whether any
    /// ancestor declares an instance property with the given name.
    /// If an ancestor initializes `name`, a `this.name` access in a child
    /// field initializer is safe — base construction ran first.
    fn ancestor_class_initializes_property(&self, name: &str) -> bool {
        use rustc_hash::FxHashSet;
        let Some(class_info) = self.ctx.enclosing_class.as_ref() else {
            return false;
        };
        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();
        let mut current = self.get_base_class_idx(class_info.class_idx);
        while let Some(class_idx) = current {
            if !visited.insert(class_idx) {
                break;
            }
            let Some(node) = self.ctx.arena.get(class_idx) else {
                break;
            };
            let Some(class) = self.ctx.arena.get_class(node) else {
                break;
            };
            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                    continue;
                }
                if self.is_static_property(member_idx) {
                    continue;
                }
                let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                    continue;
                };
                if let Some(prop_name) = self.get_property_name(prop.name)
                    && prop_name == name
                {
                    return true;
                }
            }
            current = self.get_base_class_idx(class_idx);
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

    /// Check for TS2729 in static block statements.
    ///
    /// Static blocks can reference static properties via `this.X` or `ClassName.X`.
    /// We report TS2729 when the referenced property is declared after the static block.
    pub(crate) fn check_static_block_initialization_order(&mut self, block_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        // Get class info to access member order
        let Some(class_info) = self.ctx.enclosing_class.clone() else {
            return;
        };

        // Find position of this static block in the member list
        let Some(block_pos) = class_info
            .member_nodes
            .iter()
            .position(|&idx| idx == block_idx)
        else {
            return;
        };

        // Get block statements
        let Some(node) = self.ctx.arena.get(block_idx) else {
            return;
        };
        let Some(block) = self.ctx.arena.get_block(node) else {
            return;
        };

        // Collect both `this.X` and `ClassName.X` accesses from all statements
        let class_name = class_info.name.clone();
        let stmt_indices: Vec<NodeIndex> = block.statements.nodes.clone();
        let mut accesses = Vec::new();
        for &stmt_idx in &stmt_indices {
            self.collect_static_block_accesses_recursive(stmt_idx, &class_name, &mut accesses);
        }

        // Check each access against static properties declared after the block
        for (member_name, access_node_idx) in accesses {
            for (target_pos, &target_idx) in class_info.member_nodes.iter().enumerate() {
                if let Some(found_name) = self.get_member_name(target_idx)
                    && found_name == member_name
                {
                    if self.is_static_property(target_idx) && target_pos > block_pos {
                        self.error_at_node(
                            access_node_idx,
                            &format!("Property '{member_name}' is used before its initialization."),
                            diagnostic_codes::PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION,
                        );
                    }
                    break;
                }
            }
        }
    }

    /// Recursive helper to collect both `this.X` and `ClassName.X` accesses
    /// in a static block context. Stops at ALL function/class boundaries
    /// (including arrow functions) since those accesses are deferred.
    fn collect_static_block_accesses_recursive(
        &self,
        node_idx: NodeIndex,
        class_name: &str,
        accesses: &mut Vec<(String, NodeIndex)>,
    ) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        // Stop at all function and class boundaries — accesses inside are deferred
        if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.kind == syntax_kind_ext::ARROW_FUNCTION
            || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || node.kind == syntax_kind_ext::CLASS_DECLARATION
            || node.kind == syntax_kind_ext::METHOD_DECLARATION
        {
            return;
        }

        // Check for property access: this.X or ClassName.X
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if let Some(access) = self.ctx.arena.get_access_expr(node)
                && let Some(expr_node) = self.ctx.arena.get(access.expression)
            {
                // Check `this.X` (in static blocks, `this` = class constructor)
                if expr_node.kind == SyntaxKind::ThisKeyword as u16 {
                    if let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        accesses.push((ident.escaped_text.clone(), access.name_or_argument));
                    }
                    return;
                }
                // Check `ClassName.X`
                if expr_node.kind == SyntaxKind::Identifier as u16
                    && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
                    && ident.escaped_text == class_name
                {
                    if let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
                        && let Some(prop_ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        accesses.push((prop_ident.escaped_text.clone(), access.name_or_argument));
                    }
                    return;
                }
                // Recurse into expression part for chained access
                self.collect_static_block_accesses_recursive(
                    access.expression,
                    class_name,
                    accesses,
                );
            }
            return;
        }

        // Recurse into children based on node type
        match node.kind {
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
                    self.collect_static_block_accesses_recursive(
                        expr_stmt.expression,
                        class_name,
                        accesses,
                    );
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(binary) = self.ctx.arena.get_binary_expr(node) {
                    self.collect_static_block_accesses_recursive(binary.left, class_name, accesses);
                    self.collect_static_block_accesses_recursive(
                        binary.right,
                        class_name,
                        accesses,
                    );
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    self.collect_static_block_accesses_recursive(
                        call.expression,
                        class_name,
                        accesses,
                    );
                    if let Some(ref args) = call.arguments {
                        for &arg in &args.nodes {
                            self.collect_static_block_accesses_recursive(arg, class_name, accesses);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.collect_static_block_accesses_recursive(
                        paren.expression,
                        class_name,
                        accesses,
                    );
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                    self.collect_static_block_accesses_recursive(
                        cond.condition,
                        class_name,
                        accesses,
                    );
                    self.collect_static_block_accesses_recursive(
                        cond.when_true,
                        class_name,
                        accesses,
                    );
                    self.collect_static_block_accesses_recursive(
                        cond.when_false,
                        class_name,
                        accesses,
                    );
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                // Variable statements: VARIABLE_STATEMENT → VARIABLE_DECLARATION_LIST → VARIABLE_DECLARATION
                if let Some(var_stmt) = self.ctx.arena.get_variable(node) {
                    for &list_idx in &var_stmt.declarations.nodes {
                        if let Some(list_node) = self.ctx.arena.get(list_idx)
                            && let Some(var_list) = self.ctx.arena.get_variable(list_node)
                        {
                            for &decl_idx in &var_list.declarations.nodes {
                                if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                                    && let Some(decl) =
                                        self.ctx.arena.get_variable_declaration(decl_node)
                                    && decl.initializer.is_some()
                                {
                                    self.collect_static_block_accesses_recursive(
                                        decl.initializer,
                                        class_name,
                                        accesses,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
                    self.collect_static_block_accesses_recursive(
                        if_stmt.expression,
                        class_name,
                        accesses,
                    );
                    self.collect_static_block_accesses_recursive(
                        if_stmt.then_statement,
                        class_name,
                        accesses,
                    );
                    if if_stmt.else_statement.is_some() {
                        self.collect_static_block_accesses_recursive(
                            if_stmt.else_statement,
                            class_name,
                            accesses,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.collect_static_block_accesses_recursive(
                            stmt_idx, class_name, accesses,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.ctx.arena.get_return_statement(node)
                    && ret.expression.is_some()
                {
                    self.collect_static_block_accesses_recursive(
                        ret.expression,
                        class_name,
                        accesses,
                    );
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
            {
                if let Some(unary) = self.ctx.arena.get_unary_expr(node) {
                    self.collect_static_block_accesses_recursive(
                        unary.operand,
                        class_name,
                        accesses,
                    );
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                if let Some(tmpl) = self.ctx.arena.get_template_expr(node) {
                    for &span_idx in &tmpl.template_spans.nodes {
                        if let Some(span_node) = self.ctx.arena.get(span_idx)
                            && let Some(span) = self.ctx.arena.get_template_span(span_node)
                        {
                            self.collect_static_block_accesses_recursive(
                                span.expression,
                                class_name,
                                accesses,
                            );
                        }
                    }
                }
            }
            _ => {
                // For unhandled node types, don't recurse to stay safe
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

        // Don't recurse into nested class declarations
        if node.kind == syntax_kind_ext::CLASS_DECLARATION {
            return;
        }

        // For class expressions, check heritage clause extends expressions
        // (e.g. `class extends D.B { ... }`) but don't recurse into the body
        if node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            if let Some(class) = self.ctx.arena.get_class_at(node_idx)
                && let Some(heritage_clauses) = &class.heritage_clauses
            {
                for &clause_idx in &heritage_clauses.nodes {
                    if let Some(clause) = self.ctx.arena.get_heritage_clause_at(clause_idx) {
                        // Only check extends, not implements
                        if clause.token == SyntaxKind::ExtendsKeyword as u16 {
                            for &type_idx in &clause.types.nodes {
                                self.collect_static_accesses_recursive(
                                    type_idx, class_name, accesses,
                                );
                            }
                        }
                    }
                }
            }
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
                                accesses.push((
                                    prop_ident.escaped_text.clone(),
                                    access.name_or_argument,
                                ));
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
            k if k == syntax_kind_ext::JSX_OPENING_ELEMENT => {
                // Visited only when JSX_OPENING_ELEMENT is the entry node.
                // When descending from JSX_ELEMENT below, that branch handles
                // the opening tag's `tag_name` and attributes directly — do
                // NOT also handle them here or they fire twice.
                if let Some(jsx_elem) = self.ctx.arena.get_jsx_opening(node)
                    && let Some(attrs_node) = self.ctx.arena.get(jsx_elem.attributes)
                    && let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node)
                {
                    for &attr_idx in &attrs.properties.nodes {
                        self.collect_static_accesses_recursive(attr_idx, class_name, accesses);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT => {
                // A self-closing element (`<C.z/>`) has no JSX_ELEMENT wrapper,
                // so this branch must walk both the tag name AND attributes.
                if let Some(jsx_elem) = self.ctx.arena.get_jsx_opening(node) {
                    self.collect_static_accesses_recursive(jsx_elem.tag_name, class_name, accesses);
                    if let Some(attrs_node) = self.ctx.arena.get(jsx_elem.attributes)
                        && let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node)
                    {
                        for &attr_idx in &attrs.properties.nodes {
                            self.collect_static_accesses_recursive(attr_idx, class_name, accesses);
                        }
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
                // Walk the opening tag (tag name + attributes) and the element
                // children. We deliberately do NOT walk the closing element's
                // tag name: tsc emits TS2729 once per JSX element use, anchored
                // at the opening tag. The closing tag is markup that must
                // syntactically match the opening; checking it again would
                // emit a duplicate diagnostic.
                if let Some(jsx_elem) = self.ctx.arena.get_jsx_element(node) {
                    if let Some(opening_node) = self.ctx.arena.get(jsx_elem.opening_element)
                        && let Some(opening) = self.ctx.arena.get_jsx_opening(opening_node)
                    {
                        self.collect_static_accesses_recursive(
                            opening.tag_name,
                            class_name,
                            accesses,
                        );
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
                    // Walk JSX element children — `{C.y}` expressions and nested
                    // elements both reference static members through here.
                    for &child_idx in &jsx_elem.children.nodes {
                        self.collect_static_accesses_recursive(child_idx, class_name, accesses);
                    }
                }
            }

            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                if let Some(obj) = self.ctx.arena.get_literal_expr(node) {
                    for &elem_idx in &obj.elements.nodes {
                        self.collect_static_accesses_recursive(elem_idx, class_name, accesses);
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.ctx.arena.get_property_assignment(node) {
                    self.collect_static_accesses_recursive(prop.name, class_name, accesses);
                    self.collect_static_accesses_recursive(prop.initializer, class_name, accesses);
                }
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                if let Some(computed) = self.ctx.arena.get_computed_property(node) {
                    self.collect_static_accesses_recursive(
                        computed.expression,
                        class_name,
                        accesses,
                    );
                }
            }
            k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                if let Some(spread) = self.ctx.arena.get_spread(node) {
                    self.collect_static_accesses_recursive(spread.expression, class_name, accesses);
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                if let Some(template) = self.ctx.arena.get_template_expr(node) {
                    for &span_idx in &template.template_spans.nodes {
                        self.collect_static_accesses_recursive(span_idx, class_name, accesses);
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_SPAN => {
                if let Some(span) = self.ctx.arena.get_template_span(node) {
                    self.collect_static_accesses_recursive(span.expression, class_name, accesses);
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(arr) = self.ctx.arena.get_literal_expr(node) {
                    for &elem_idx in &arr.elements.nodes {
                        self.collect_static_accesses_recursive(elem_idx, class_name, accesses);
                    }
                }
            }
            _ => {
                // For other expressions, we don't recurse further
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
                            accesses.push((ident.escaped_text.clone(), access.name_or_argument));
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
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                if let Some(obj) = self.ctx.arena.get_literal_expr(node) {
                    for &elem_idx in &obj.elements.nodes {
                        self.collect_this_accesses_recursive(elem_idx, accesses);
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.ctx.arena.get_property_assignment(node) {
                    self.collect_this_accesses_recursive(prop.name, accesses);
                    self.collect_this_accesses_recursive(prop.initializer, accesses);
                }
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                if let Some(shorthand) = self.ctx.arena.get_shorthand_property(node) {
                    self.collect_this_accesses_recursive(shorthand.name, accesses);
                }
            }
            k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                if let Some(spread) = self.ctx.arena.get_spread(node) {
                    self.collect_this_accesses_recursive(spread.expression, accesses);
                }
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                if let Some(computed) = self.ctx.arena.get_computed_property(node) {
                    self.collect_this_accesses_recursive(computed.expression, accesses);
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                if let Some(template) = self.ctx.arena.get_template_expr(node) {
                    for &span_idx in &template.template_spans.nodes {
                        self.collect_this_accesses_recursive(span_idx, accesses);
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_SPAN => {
                if let Some(span) = self.ctx.arena.get_template_span(node) {
                    self.collect_this_accesses_recursive(span.expression, accesses);
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(arr) = self.ctx.arena.get_literal_expr(node) {
                    for &elem_idx in &arr.elements.nodes {
                        self.collect_this_accesses_recursive(elem_idx, accesses);
                    }
                }
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                if let Some(spread) = self.ctx.arena.get_spread(node) {
                    self.collect_this_accesses_recursive(spread.expression, accesses);
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.ctx.arena.get_access_expr(node) {
                    self.collect_this_accesses_recursive(access.expression, accesses);
                    self.collect_this_accesses_recursive(access.name_or_argument, accesses);
                }
            }
            _ => {
                // For other expressions, we don't recurse further
            }
        }
    }
}
