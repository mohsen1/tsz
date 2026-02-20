//! Super expression validation (calls, property access, derived class requirements).

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

// =============================================================================
// Super Expression Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Super Context Detection
    // =========================================================================

    /// Check if super is in a nested function inside a constructor.
    ///
    /// Returns true if super is in a nested function inside a constructor.
    pub(crate) fn is_super_in_nested_function(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        let mut function_depth = 0;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            // Count function/arrow function boundaries
            if parent_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || parent_node.kind == syntax_kind_ext::ARROW_FUNCTION
            {
                function_depth += 1;
            }

            // Check if we've reached the constructor
            if parent_node.kind == syntax_kind_ext::CONSTRUCTOR {
                return function_depth > 0;
            }

            // Check if we've left the class
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                break;
            }

            current = parent_idx;
        }

        false
    }

    /// Check if a node is inside a class method body.
    ///
    /// Returns true if inside a method body (both static and non-static).
    /// `super` property access is valid in both static and instance methods
    /// of a derived class.
    pub(crate) fn is_in_class_method_body(&self, idx: NodeIndex) -> bool {
        let mut current = idx;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            // Arrow functions capture the class context, so skip them when checking
            if parent_node.kind == syntax_kind_ext::ARROW_FUNCTION {
                current = parent_idx;
                continue;
            }

            // Regular functions create a new scope - super is not valid inside them
            // TS2660: 'super' can only be referenced in members of derived classes
            if parent_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            {
                return false;
            }

            if parent_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                // `super` property access is valid in both static and non-static methods
                return true;
            }

            // Check if we've left the class
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                break;
            }

            current = parent_idx;
        }

        false
    }

    /// Check if a node is inside a class accessor body (getter/setter).
    ///
    /// Returns true if inside an accessor body.
    /// IMPORTANT: Skips arrow function boundaries since they capture the class context.
    pub(crate) fn is_in_class_accessor_body(&self, idx: NodeIndex) -> bool {
        let mut current = idx;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            // Arrow functions capture the class context, so skip them when checking
            if parent_node.kind == syntax_kind_ext::ARROW_FUNCTION {
                current = parent_idx;
                continue;
            }

            // Regular functions create a new scope - super is not valid inside them
            if parent_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            {
                return false;
            }

            if parent_node.kind == syntax_kind_ext::GET_ACCESSOR
                || parent_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                return true;
            }

            // Check if we've left the class
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                break;
            }

            current = parent_idx;
        }

        false
    }

    /// Check if a node is inside a class property initializer.
    ///
    /// Returns true if inside a property declaration (class field).
    /// IMPORTANT: Skips arrow function boundaries since they capture the class context.
    pub(crate) fn is_in_class_property_initializer(&self, idx: NodeIndex) -> bool {
        let mut current = idx;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            // Arrow functions capture the class context, so skip them when checking
            if parent_node.kind == syntax_kind_ext::ARROW_FUNCTION {
                current = parent_idx;
                continue;
            }

            // Regular functions create a new scope - super is not valid inside them
            if parent_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            {
                return false;
            }

            if parent_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                return true;
            }

            // Check if we've left the class
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                break;
            }

            current = parent_idx;
        }

        false
    }

    /// Check if a node is inside a class static block.
    ///
    /// Returns true if inside a `static { ... }` block.
    /// Super property access is valid in static blocks of derived classes.
    pub(crate) fn is_in_class_static_block(&self, idx: NodeIndex) -> bool {
        let mut current = idx;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            if parent_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                return true;
            }

            // Check if we've left the class
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                break;
            }

            current = parent_idx;
        }

        false
    }

    // =========================================================================
    // Super Expression Validation
    // =========================================================================

    /// Check if super is in a constructor.
    ///
    /// Returns true if super is inside a constructor declaration.
    fn is_in_constructor(&self, idx: NodeIndex) -> bool {
        let mut current = idx;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            // Arrow functions capture the class context
            if parent_node.kind == syntax_kind_ext::ARROW_FUNCTION {
                current = parent_idx;
                continue;
            }

            // Found the constructor
            if parent_node.kind == syntax_kind_ext::CONSTRUCTOR {
                return true;
            }

            // Found a class - stop searching
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                break;
            }

            current = parent_idx;
        }

        false
    }

    fn is_super_property_before_super_call_in_constructor(&self, idx: NodeIndex) -> bool {
        let mut current = idx;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            if parent_node.kind == syntax_kind_ext::CONSTRUCTOR {
                let Some(ctor) = self.ctx.arena.get_constructor(parent_node) else {
                    return false;
                };
                if ctor.body.is_none() {
                    return false;
                }

                let Some(body_node) = self.ctx.arena.get(ctor.body) else {
                    return false;
                };
                let Some(block) = self.ctx.arena.get_block(body_node) else {
                    return false;
                };

                let Some(super_expr_node) = self.ctx.arena.get(idx) else {
                    return false;
                };
                let first_super_pos = block
                    .statements
                    .nodes
                    .iter()
                    .copied()
                    .find(|&stmt| self.is_super_call_statement(stmt))
                    .and_then(|stmt| self.ctx.arena.get(stmt).map(|n| n.pos))
                    .or_else(|| {
                        // Fallback for constructors where first super() is nested in control flow.
                        let body_idx = ctor.body;
                        let mut first_pos: Option<u32> = None;
                        for i in 0..self.ctx.arena.len() {
                            let node_idx = NodeIndex(i as u32);
                            if !self.is_descendant_of_node(node_idx, body_idx)
                                && node_idx != body_idx
                            {
                                continue;
                            }
                            let Some(node) = self.ctx.arena.get(node_idx) else {
                                continue;
                            };
                            if node.kind != SyntaxKind::SuperKeyword as u16 {
                                continue;
                            }
                            let Some(ext) = self.ctx.arena.get_extended(node_idx) else {
                                continue;
                            };
                            let Some(parent) = self.ctx.arena.get(ext.parent) else {
                                continue;
                            };
                            if parent.kind != syntax_kind_ext::CALL_EXPRESSION {
                                continue;
                            }
                            let Some(call) = self.ctx.arena.get_call_expr(parent) else {
                                continue;
                            };
                            if call.expression != node_idx {
                                continue;
                            }
                            if first_pos.is_none_or(|p| node.pos < p) {
                                first_pos = Some(node.pos);
                            }
                        }
                        first_pos
                    });
                let Some(first_super_pos) = first_super_pos else {
                    return false;
                };

                return super_expr_node.pos < first_super_pos;
            }

            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                break;
            }

            current = parent_idx;
        }

        false
    }

    fn is_in_constructor_parameter_context(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        let mut saw_parameter = false;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            if parent_node.kind == syntax_kind_ext::PARAMETER {
                saw_parameter = true;
            }

            if parent_node.kind == syntax_kind_ext::CONSTRUCTOR {
                return saw_parameter;
            }

            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                break;
            }

            current = parent_idx;
        }

        false
    }

    fn is_in_object_literal_member(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            ARROW_FUNCTION, CLASS_DECLARATION, CLASS_EXPRESSION, FUNCTION_DECLARATION,
            FUNCTION_EXPRESSION, GET_ACCESSOR, METHOD_DECLARATION, OBJECT_LITERAL_EXPRESSION,
            SET_ACCESSOR,
        };
        let mut current = idx;
        let mut saw_object_member = false;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            if parent_node.kind == METHOD_DECLARATION
                || parent_node.kind == GET_ACCESSOR
                || parent_node.kind == SET_ACCESSOR
            {
                saw_object_member = true;
            }

            if parent_node.kind == OBJECT_LITERAL_EXPRESSION {
                return saw_object_member;
            }

            if parent_node.kind == FUNCTION_DECLARATION
                || parent_node.kind == FUNCTION_EXPRESSION
                || parent_node.kind == ARROW_FUNCTION
            {
                return false;
            }

            if parent_node.kind == CLASS_DECLARATION || parent_node.kind == CLASS_EXPRESSION {
                return false;
            }

            current = parent_idx;
        }

        false
    }

    fn is_super_call_root_level_statement_in_constructor(&self, idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(idx) else {
            return false;
        };
        let call_idx = ext.parent;
        let Some(call_node) = self.ctx.arena.get(call_idx) else {
            return false;
        };
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }

        let Some(call_data) = self.ctx.arena.get_call_expr(call_node) else {
            return false;
        };
        if call_data.expression != idx {
            return false;
        }

        let Some(call_ext) = self.ctx.arena.get_extended(call_idx) else {
            return false;
        };
        let expr_stmt_idx = call_ext.parent;
        let Some(expr_stmt_node) = self.ctx.arena.get(expr_stmt_idx) else {
            return false;
        };
        if expr_stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return false;
        }

        let Some(expr_stmt_data) = self.ctx.arena.get_expression_statement(expr_stmt_node) else {
            return false;
        };
        if expr_stmt_data.expression != call_idx {
            return false;
        }

        let Some(stmt_ext) = self.ctx.arena.get_extended(expr_stmt_idx) else {
            return false;
        };
        let block_idx = stmt_ext.parent;
        let Some(block_node) = self.ctx.arena.get(block_idx) else {
            return false;
        };
        if block_node.kind != syntax_kind_ext::BLOCK {
            return false;
        }

        let Some(block_ext) = self.ctx.arena.get_extended(block_idx) else {
            return false;
        };
        let ctor_idx = block_ext.parent;
        let Some(ctor_node) = self.ctx.arena.get(ctor_idx) else {
            return false;
        };
        ctor_node.kind == syntax_kind_ext::CONSTRUCTOR
    }

    fn is_super_call_first_statement_in_constructor(&self, idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(idx) else {
            return false;
        };
        let call_idx = ext.parent;
        let Some(call_node) = self.ctx.arena.get(call_idx) else {
            return false;
        };
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }

        let Some(call_data) = self.ctx.arena.get_call_expr(call_node) else {
            return false;
        };
        if call_data.expression != idx {
            return false;
        }

        let Some(call_ext) = self.ctx.arena.get_extended(call_idx) else {
            return false;
        };
        let expr_stmt_idx = call_ext.parent;
        let Some(expr_stmt_node) = self.ctx.arena.get(expr_stmt_idx) else {
            return false;
        };
        if expr_stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return false;
        }

        let Some(stmt_ext) = self.ctx.arena.get_extended(expr_stmt_idx) else {
            return false;
        };
        let block_idx = stmt_ext.parent;
        let Some(block_node) = self.ctx.arena.get(block_idx) else {
            return false;
        };
        let Some(block) = self.ctx.arena.get_block(block_node) else {
            return false;
        };

        block
            .statements
            .nodes
            .first()
            .is_some_and(|&first| first == expr_stmt_idx)
    }

    fn enclosing_constructor_node(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return None;
            }
            let parent_node = self.ctx.arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::CONSTRUCTOR {
                return Some(parent_idx);
            }
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                return None;
            }
            current = parent_idx;
        }
        None
    }

    fn is_directly_in_constructor_body(&self, idx: NodeIndex, ctor_idx: NodeIndex) -> bool {
        let mut current = idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            if parent_idx == ctor_idx {
                return true;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || parent_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || parent_node.kind == syntax_kind_ext::METHOD_DECLARATION
                || parent_node.kind == syntax_kind_ext::GET_ACCESSOR
                || parent_node.kind == syntax_kind_ext::SET_ACCESSOR
                || (parent_node.kind == syntax_kind_ext::CONSTRUCTOR && parent_idx != ctor_idx)
            {
                return false;
            }

            current = parent_idx;
        }
        false
    }

    fn is_descendant_of_node(&self, node_idx: NodeIndex, ancestor_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            if parent_idx == ancestor_idx {
                return true;
            }
            current = parent_idx;
        }
        false
    }

    fn constructor_has_pre_super_this_or_super_property_reference(
        &self,
        ctor_idx: NodeIndex,
    ) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(ctor_node) = self.ctx.arena.get(ctor_idx) else {
            return false;
        };
        let Some(ctor) = self.ctx.arena.get_constructor(ctor_node) else {
            return false;
        };
        if ctor.body.is_none() {
            return false;
        }

        let Some(body_node) = self.ctx.arena.get(ctor.body) else {
            return false;
        };
        let Some(block) = self.ctx.arena.get_block(body_node) else {
            return false;
        };

        let Some(first_super_stmt) = block
            .statements
            .nodes
            .iter()
            .copied()
            .find(|&stmt| self.is_super_call_statement(stmt))
        else {
            return false;
        };

        let Some(first_super_stmt_index) = block
            .statements
            .nodes
            .iter()
            .position(|&stmt| stmt == first_super_stmt)
        else {
            return false;
        };

        let pre_super_statements = &block.statements.nodes[..first_super_stmt_index];
        if pre_super_statements.is_empty() {
            return false;
        }

        for i in 0..self.ctx.arena.len() {
            let node_idx = NodeIndex(i as u32);
            let Some(node) = self.ctx.arena.get(node_idx) else {
                continue;
            };

            if !self.is_directly_in_constructor_body(node_idx, ctor_idx) {
                continue;
            }

            let in_pre_super_statement = pre_super_statements.iter().any(|&stmt_idx| {
                self.is_descendant_of_node(node_idx, stmt_idx) || node_idx == stmt_idx
            });

            if !in_pre_super_statement {
                continue;
            }

            if node.kind == SyntaxKind::ThisKeyword as u16
                && self.is_this_before_super_in_derived_constructor(node_idx)
            {
                return true;
            }

            if node.kind == SyntaxKind::SuperKeyword as u16 {
                let Some(ext) = self.ctx.arena.get_extended(node_idx) else {
                    continue;
                };
                let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
                    continue;
                };

                let is_super_property = parent_node.kind
                    == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION;

                if is_super_property {
                    return true;
                }
            }
        }

        false
    }

    fn is_additional_super_call_after_first_root_level_super_statement(
        &self,
        idx: NodeIndex,
        ctor_idx: NodeIndex,
    ) -> bool {
        let Some(ctor_node) = self.ctx.arena.get(ctor_idx) else {
            return false;
        };
        let Some(ctor) = self.ctx.arena.get_constructor(ctor_node) else {
            return false;
        };
        if ctor.body.is_none() {
            return false;
        }

        let Some(body_node) = self.ctx.arena.get(ctor.body) else {
            return false;
        };
        let Some(block) = self.ctx.arena.get_block(body_node) else {
            return false;
        };

        let Some(first_super_stmt) = block
            .statements
            .nodes
            .iter()
            .copied()
            .find(|&stmt| self.is_super_call_statement(stmt))
        else {
            return false;
        };

        let Some(first_super_stmt_node) = self.ctx.arena.get(first_super_stmt) else {
            return false;
        };
        let Some(super_node) = self.ctx.arena.get(idx) else {
            return false;
        };

        super_node.pos > first_super_stmt_node.pos
            && !self.is_descendant_of_node(idx, first_super_stmt)
            && idx != first_super_stmt
    }

    /// Find the enclosing class by walking up the parent chain.
    ///
    /// This is more reliable than relying on `enclosing_class` which may not be set
    /// during type computation (before class declarations are checked).
    /// This function correctly handles arrow functions which capture the class context.
    fn find_enclosing_class(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        // Track if we entered through a computed property name.
        // super in a computed property key `[super.foo()]() {}` of an inner class
        // refers to the OUTER class, so we skip the inner class.
        let mut in_computed_property = false;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            if parent_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                in_computed_property = true;
            }

            // Reset computed property tracking when passing through an object literal.
            // Computed property names in object literals don't affect which class `super` refers to.
            // Without this, `class C extends B { m() { var o = { [super.x]() {} }; } }` would
            // incorrectly skip class C thinking the computed property is a class member.
            if parent_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                in_computed_property = false;
            }

            // Arrow functions capture the class context, so skip them
            if parent_node.kind == syntax_kind_ext::ARROW_FUNCTION {
                current = parent_idx;
                continue;
            }

            // Found the enclosing class
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                if in_computed_property {
                    // super in computed property name of this class's member
                    // refers to the outer class, not this one — keep walking
                    in_computed_property = false;
                    current = parent_idx;
                    continue;
                }
                return Some(parent_idx);
            }

            current = parent_idx;
        }

        None
    }

    /// Check a super expression for proper usage.
    ///
    /// Validates that super expressions are used correctly:
    /// - TS17011: super cannot be in static property initializers
    /// - TS2335: super can only be used in derived classes
    /// - TS2337: `super()` calls must be in constructors
    /// - TS2336: super property access must be in valid contexts
    pub(crate) fn check_super_expression(&mut self, idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // TS2466: 'super' cannot be referenced in a computed property name.
        // Check this first — it takes priority over TS2337/TS2660/etc.
        if self.is_super_in_computed_property_name(idx) {
            self.error_at_node(
                idx,
                diagnostic_messages::SUPER_CANNOT_BE_REFERENCED_IN_A_COMPUTED_PROPERTY_NAME,
                diagnostic_codes::SUPER_CANNOT_BE_REFERENCED_IN_A_COMPUTED_PROPERTY_NAME,
            );
            return;
        }

        // Detect if this is a super() call early (needed for error selection)
        let parent_info = self
            .ctx
            .arena
            .get_extended(idx)
            .and_then(|ext| self.ctx.arena.get(ext.parent).map(|n| (ext.parent, n)));

        let is_super_call = parent_info.as_ref().is_some_and(|(_, parent_node)| {
            parent_node.kind == syntax_kind_ext::CALL_EXPRESSION
                && self
                    .ctx
                    .arena
                    .get_call_expr(parent_node)
                    .is_some_and(|call| call.expression == idx)
        });

        let is_super_new = parent_info.as_ref().is_some_and(|(_, parent_node)| {
            parent_node.kind == syntax_kind_ext::NEW_EXPRESSION
                && self
                    .ctx
                    .arena
                    .get_call_expr(parent_node)
                    .is_some_and(|new_expr| new_expr.expression == idx)
        });

        let is_super_property_access = parent_info.as_ref().is_some_and(|(_, parent_node)| {
            parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        });

        let in_constructor_parameter_context = self.is_in_constructor_parameter_context(idx);
        let in_static_property_initializer = self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|info| info.in_static_property_initializer);

        if in_static_property_initializer {
            self.error_at_node(
                idx,
                diagnostic_messages::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_A_PROPERTY_OF_SUPER_IN_THE_CONSTRUCTOR_OF,
                diagnostic_codes::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_A_PROPERTY_OF_SUPER_IN_THE_CONSTRUCTOR_OF,
            );
        }

        if !is_super_call && !is_super_property_access {
            if is_super_new && self.is_in_constructor(idx) {
                self.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_A_PROPERTY_OF_SUPER_IN_THE_CONSTRUCTOR_OF,
                    diagnostic_codes::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_A_PROPERTY_OF_SUPER_IN_THE_CONSTRUCTOR_OF,
                );
                return;
            }

            self.error_at_node(
                idx,
                diagnostic_messages::SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS,
                diagnostic_codes::SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS,
            );
            if in_constructor_parameter_context {
                self.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_CANNOT_BE_REFERENCED_IN_CONSTRUCTOR_ARGUMENTS,
                    diagnostic_codes::SUPER_CANNOT_BE_REFERENCED_IN_CONSTRUCTOR_ARGUMENTS,
                );
                self.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_A_PROPERTY_OF_SUPER_IN_THE_CONSTRUCTOR_OF,
                    diagnostic_codes::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_A_PROPERTY_OF_SUPER_IN_THE_CONSTRUCTOR_OF,
                );
            }
            return;
        }

        if is_super_call
            && self.is_in_constructor(idx)
            && let Some(ref mut class_info) = self.ctx.enclosing_class
        {
            class_info.has_super_call_in_current_constructor = true;
        }

        // Find the enclosing class by walking up the parent chain
        // This works even during type computation when `enclosing_class` is not yet set
        let class_idx = match self.find_enclosing_class(idx) {
            Some(idx) => idx,
            None => {
                if is_super_property_access {
                    if self.is_in_object_literal_member(idx) {
                        if self.ctx.compiler_options.target.is_es5() {
                            self.error_at_node(
                                idx,
                                diagnostic_messages::SUPER_IS_ONLY_ALLOWED_IN_MEMBERS_OF_OBJECT_LITERAL_EXPRESSIONS_WHEN_OPTION_TARGE,
                                diagnostic_codes::SUPER_IS_ONLY_ALLOWED_IN_MEMBERS_OF_OBJECT_LITERAL_EXPRESSIONS_WHEN_OPTION_TARGE,
                            );
                        }
                        return;
                    }

                    self.error_at_node(
                        idx,
                        diagnostic_messages::SUPER_CAN_ONLY_BE_REFERENCED_IN_MEMBERS_OF_DERIVED_CLASSES_OR_OBJECT_LITERAL_EXP,
                        diagnostic_codes::SUPER_CAN_ONLY_BE_REFERENCED_IN_MEMBERS_OF_DERIVED_CLASSES_OR_OBJECT_LITERAL_EXP,
                    );
                    return;
                }

                // Emit TS2337 for super() calls, TS2335 for super property access
                // This matches TypeScript's behavior when super is used outside a class
                if is_super_call {
                    self.error_at_node(
                        idx,
                        diagnostic_messages::SUPER_CALLS_ARE_NOT_PERMITTED_OUTSIDE_CONSTRUCTORS_OR_IN_NESTED_FUNCTIONS_INSIDE,
                        diagnostic_codes::SUPER_CALLS_ARE_NOT_PERMITTED_OUTSIDE_CONSTRUCTORS_OR_IN_NESTED_FUNCTIONS_INSIDE,
                    );
                } else {
                    self.error_at_node(
                        idx,
                        diagnostic_messages::SUPER_CAN_ONLY_BE_REFERENCED_IN_A_DERIVED_CLASS,
                        diagnostic_codes::SUPER_CAN_ONLY_BE_REFERENCED_IN_A_DERIVED_CLASS,
                    );
                }
                return;
            }
        };

        // Check if the class has an extends clause (is a derived class)
        // We check for the existence of an extends heritage clause, not whether the
        // base class symbol resolves. This matches TypeScript's behavior where
        // `class B extends A {}` is always a derived class even if `A` can't be resolved.
        let class_data = self
            .ctx
            .arena
            .get(class_idx)
            .and_then(|node| self.ctx.arena.get_class(node))
            .cloned();

        let has_base_class = class_data
            .as_ref()
            .is_some_and(|class| self.class_has_base(class));

        let extends_null = class_data
            .as_ref()
            .is_some_and(|class| self.class_extends_null(class));

        let requires_super_call = class_data
            .as_ref()
            .is_some_and(|class| self.class_requires_super_call(class));

        let has_position_sensitive_members = class_data
            .as_ref()
            .is_some_and(|class| self.class_has_super_call_position_sensitive_members(class));

        // TS2335: super can only be referenced in a derived class
        if !has_base_class {
            self.error_at_node(
                idx,
                diagnostic_messages::SUPER_CAN_ONLY_BE_REFERENCED_IN_A_DERIVED_CLASS,
                diagnostic_codes::SUPER_CAN_ONLY_BE_REFERENCED_IN_A_DERIVED_CLASS,
            );
            return;
        }

        if is_super_call && extends_null {
            self.error_at_node(
                idx,
                diagnostic_messages::A_CONSTRUCTOR_CANNOT_CONTAIN_A_SUPER_CALL_WHEN_ITS_CLASS_EXTENDS_NULL,
                diagnostic_codes::A_CONSTRUCTOR_CANNOT_CONTAIN_A_SUPER_CALL_WHEN_ITS_CLASS_EXTENDS_NULL,
            );
            return;
        }

        if is_super_call
            && requires_super_call
            && has_position_sensitive_members
            && self.is_in_constructor(idx)
            && !self.is_super_in_nested_function(idx)
        {
            if let Some(ctor_idx) = self.enclosing_constructor_node(idx)
                && self
                    .is_additional_super_call_after_first_root_level_super_statement(idx, ctor_idx)
            {
                return;
            }

            let diagnostic_node = self.enclosing_constructor_node(idx).unwrap_or(idx);

            if !self.is_super_call_root_level_statement_in_constructor(idx) {
                self.error_at_node(
                    diagnostic_node,
                    diagnostic_messages::A_SUPER_CALL_MUST_BE_A_ROOT_LEVEL_STATEMENT_WITHIN_A_CONSTRUCTOR_OF_A_DERIVED_CL,
                    diagnostic_codes::A_SUPER_CALL_MUST_BE_A_ROOT_LEVEL_STATEMENT_WITHIN_A_CONSTRUCTOR_OF_A_DERIVED_CL,
                );
                return;
            }

            if !self.is_super_call_first_statement_in_constructor(idx) {
                let should_emit_ts2376 =
                    self.enclosing_constructor_node(idx)
                        .is_some_and(|ctor_idx| {
                            self.constructor_has_pre_super_this_or_super_property_reference(
                                ctor_idx,
                            )
                        });

                if should_emit_ts2376 {
                    self.error_at_node(
                        diagnostic_node,
                        diagnostic_messages::A_SUPER_CALL_MUST_BE_THE_FIRST_STATEMENT_IN_THE_CONSTRUCTOR_TO_REFER_TO_SUPER_OR,
                        diagnostic_codes::A_SUPER_CALL_MUST_BE_THE_FIRST_STATEMENT_IN_THE_CONSTRUCTOR_TO_REFER_TO_SUPER_OR,
                    );
                }
                return;
            }
        }

        // TS2336/TS17011: super property access in constructor parameters is not allowed.
        if is_super_property_access && in_constructor_parameter_context {
            self.error_at_node(
                idx,
                diagnostic_messages::SUPER_CANNOT_BE_REFERENCED_IN_CONSTRUCTOR_ARGUMENTS,
                diagnostic_codes::SUPER_CANNOT_BE_REFERENCED_IN_CONSTRUCTOR_ARGUMENTS,
            );
            self.error_at_node(
                idx,
                diagnostic_messages::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_A_PROPERTY_OF_SUPER_IN_THE_CONSTRUCTOR_OF,
                diagnostic_codes::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_A_PROPERTY_OF_SUPER_IN_THE_CONSTRUCTOR_OF,
            );
            return;
        }

        // TS2337: Super calls are not permitted outside constructors
        if is_super_call {
            if !self.is_in_constructor(idx) {
                self.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_CALLS_ARE_NOT_PERMITTED_OUTSIDE_CONSTRUCTORS_OR_IN_NESTED_FUNCTIONS_INSIDE,
                    diagnostic_codes::SUPER_CALLS_ARE_NOT_PERMITTED_OUTSIDE_CONSTRUCTORS_OR_IN_NESTED_FUNCTIONS_INSIDE,
                );
                return;
            }

            // Check for nested function inside constructor
            if self.is_super_in_nested_function(idx) {
                self.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_CALLS_ARE_NOT_PERMITTED_OUTSIDE_CONSTRUCTORS_OR_IN_NESTED_FUNCTIONS_INSIDE,
                    diagnostic_codes::SUPER_CALLS_ARE_NOT_PERMITTED_OUTSIDE_CONSTRUCTORS_OR_IN_NESTED_FUNCTIONS_INSIDE,
                );
                return;
            }
        }

        // TS17011: super property access before super() call in derived constructors.
        if is_super_property_access
            && self.is_in_constructor(idx)
            && self.is_super_property_before_super_call_in_constructor(idx)
        {
            self.error_at_node(
                idx,
                diagnostic_messages::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_A_PROPERTY_OF_SUPER_IN_THE_CONSTRUCTOR_OF,
                diagnostic_codes::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_A_PROPERTY_OF_SUPER_IN_THE_CONSTRUCTOR_OF,
            );
            return;
        }

        // TS2336/TS2660: super property access must be in constructor, method, accessor, or property initializer
        // Note: Arrow functions capture the class context, so super inside arrow functions is valid
        // if the arrow itself is in a valid context (checked by the helper functions)
        if is_super_property_access {
            let in_valid_context = self.is_in_constructor(idx)
                || self.is_in_class_method_body(idx)
                || self.is_in_class_accessor_body(idx)
                || self.is_in_class_property_initializer(idx)
                || self.is_in_class_static_block(idx);

            if !in_valid_context {
                // TS2660: Super can only be referenced in members of derived classes or object literal expressions
                // This is emitted when super is used in contexts that break the "member" requirement,
                // such as inside nested regular functions (not arrow functions)
                self.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_CAN_ONLY_BE_REFERENCED_IN_MEMBERS_OF_DERIVED_CLASSES_OR_OBJECT_LITERAL_EXP,
                    diagnostic_codes::SUPER_CAN_ONLY_BE_REFERENCED_IN_MEMBERS_OF_DERIVED_CLASSES_OR_OBJECT_LITERAL_EXP,
                );
            }
        }
    }
}
