//! AST to IR conversion for the ES5 class transformer.
//!
//! Contains `AstToIr`, which converts AST statement and expression nodes
//! into IR nodes, avoiding `ASTRef` when possible.

use super::*;

/// Convert an AST node to IR, avoiding `ASTRef` when possible
pub struct AstToIr<'a> {
    arena: &'a NodeArena,
    /// Track if we're inside an arrow function that captures `this`
    this_captured: Cell<bool>,
    /// Transform directives from `LoweringPass`
    transforms: Option<TransformContext>,
    /// Current class alias to use for `this` substitution in static methods
    current_class_alias: Cell<Option<String>>,
    /// Whether we're inside a derived class (has extends clause) — needed for super lowering
    has_super: bool,
}

impl<'a> AstToIr<'a> {
    pub const fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            this_captured: Cell::new(false),
            transforms: None,
            current_class_alias: Cell::new(None),
            has_super: false,
        }
    }

    /// Set whether we're inside a derived class (for super lowering)
    pub const fn with_super(mut self, has_super: bool) -> Self {
        self.has_super = has_super;
        self
    }

    /// Set transform directives from `LoweringPass`
    pub fn with_transforms(mut self, transforms: TransformContext) -> Self {
        self.transforms = Some(transforms);
        self
    }

    /// Set the current class alias for `this` substitution
    pub fn with_class_alias(self, alias: Option<String>) -> Self {
        self.current_class_alias.set(alias);
        self
    }

    /// Set whether `this` should be captured as `_this`
    pub fn with_this_captured(self, captured: bool) -> Self {
        self.this_captured.set(captured);
        self
    }

    /// Convert a statement to IR
    pub fn convert_statement(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::ASTRef(idx);
        };

        match node.kind {
            k if k == syntax_kind_ext::BLOCK => self.convert_block(idx),
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                self.convert_expression_statement(idx)
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => self.convert_return_statement(idx),
            k if k == syntax_kind_ext::IF_STATEMENT => self.convert_if_statement(idx),
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => self.convert_variable_statement(idx),
            k if k == syntax_kind_ext::THROW_STATEMENT => self.convert_throw_statement(idx),
            k if k == syntax_kind_ext::TRY_STATEMENT => self.convert_try_statement(idx),
            k if k == syntax_kind_ext::FOR_STATEMENT => self.convert_for_statement(idx),
            k if k == syntax_kind_ext::WHILE_STATEMENT => self.convert_while_statement(idx),
            k if k == syntax_kind_ext::DO_STATEMENT => self.convert_do_while_statement(idx),
            k if k == syntax_kind_ext::SWITCH_STATEMENT => self.convert_switch_statement(idx),
            k if k == syntax_kind_ext::BREAK_STATEMENT => self.convert_break_statement(idx),
            k if k == syntax_kind_ext::CONTINUE_STATEMENT => self.convert_continue_statement(idx),
            k if k == syntax_kind_ext::LABELED_STATEMENT => self.convert_labeled_statement(idx),
            k if k == syntax_kind_ext::EMPTY_STATEMENT => IRNode::EmptyStatement,
            k if k == syntax_kind_ext::DEBUGGER_STATEMENT => {
                IRNode::ExpressionStatement(Box::new(IRNode::Identifier("debugger".to_string())))
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                self.convert_for_in_of_statement(idx)
            }
            _ => IRNode::ASTRef(idx), // Fallback for unsupported statements
        }
    }

    /// Convert an expression to IR
    pub fn convert_expression(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::ASTRef(idx);
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => self.convert_identifier(idx),
            k if k == SyntaxKind::NumericLiteral as u16 => self.convert_numeric_literal(idx),
            k if k == SyntaxKind::StringLiteral as u16 => self.convert_string_literal(idx),
            k if k == SyntaxKind::TrueKeyword as u16 => IRNode::BooleanLiteral(true),
            k if k == SyntaxKind::FalseKeyword as u16 => IRNode::BooleanLiteral(false),
            k if k == SyntaxKind::NullKeyword as u16 => IRNode::NullLiteral,
            k if k == SyntaxKind::UndefinedKeyword as u16 => IRNode::Undefined,
            k if k == SyntaxKind::ThisKeyword as u16 => {
                // If we have a class_alias set (static method context), use it instead of `this`
                if let Some(alias) = self.current_class_alias.take() {
                    self.current_class_alias.set(Some(alias.clone()));
                    IRNode::Identifier(alias)
                } else {
                    IRNode::This {
                        captured: self.this_captured.get(),
                    }
                }
            }
            k if k == SyntaxKind::SuperKeyword as u16 => IRNode::Super,
            k if k == syntax_kind_ext::CALL_EXPRESSION => self.convert_call_expression(idx),
            k if k == syntax_kind_ext::NEW_EXPRESSION => self.convert_new_expression(idx),
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.convert_property_access(idx)
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.convert_element_access(idx)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => self.convert_binary_expression(idx),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => self.convert_prefix_unary(idx),
            k if k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => self.convert_postfix_unary(idx),
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => self.convert_parenthesized(idx),
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => self.convert_conditional(idx),
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => self.convert_array_literal(idx),
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.convert_object_literal(idx)
            }
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => self.convert_function_expression(idx),
            k if k == syntax_kind_ext::ARROW_FUNCTION => self.convert_arrow_function(idx),
            k if k == syntax_kind_ext::SPREAD_ELEMENT => self.convert_spread_element(idx),
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                self.convert_template_literal(idx)
            }
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => self.convert_await_expression(idx),
            k if k == syntax_kind_ext::TYPE_ASSERTION || k == syntax_kind_ext::AS_EXPRESSION => {
                // Type assertions are stripped in ES5
                self.convert_type_assertion(idx)
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => self.convert_non_null(idx),
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                // QualifiedName (A.B) is used in import aliases: import X = A.B
                // Convert to PropertyAccess IR so source text isn't copied verbatim
                // (which would include trailing semicolons).
                if let Some(qn) = self.arena.get_qualified_name(node) {
                    IRNode::PropertyAccess {
                        object: Box::new(self.convert_expression(qn.left)),
                        property: self
                            .arena
                            .get(qn.right)
                            .and_then(|n| self.arena.get_identifier(n))
                            .map_or_else(String::new, |id| id.escaped_text.clone()),
                    }
                } else {
                    IRNode::ASTRef(idx)
                }
            }
            _ => IRNode::ASTRef(idx), // Fallback
        }
    }

    fn convert_block(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(block) = self.arena.get_block(node) {
            let stmts: Vec<IRNode> = block
                .statements
                .nodes
                .iter()
                .map(|&s| self.convert_statement(s))
                .collect();
            IRNode::Block(stmts)
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_expression_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
            if self.is_destructuring_assignment_expr(expr_stmt.expression) {
                return IRNode::ASTRef(idx);
            }
            IRNode::ExpressionStatement(Box::new(self.convert_expression(expr_stmt.expression)))
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_return_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(ret) = self.arena.get_return_statement(node) {
            let expr = if ret.expression.is_none() {
                None
            } else {
                Some(Box::new(self.convert_expression(ret.expression)))
            };
            IRNode::ReturnStatement(expr)
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_if_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(if_stmt) = self.arena.get_if_statement(node) {
            let else_branch = if if_stmt.else_statement.is_none() {
                None
            } else {
                Some(Box::new(self.convert_statement(if_stmt.else_statement)))
            };
            IRNode::IfStatement {
                condition: Box::new(self.convert_expression(if_stmt.expression)),
                then_branch: Box::new(self.convert_statement(if_stmt.then_statement)),
                else_branch,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_variable_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // VariableStatement uses VariableData which has declarations directly
        if let Some(var_data) = self.arena.get_variable(node) {
            // Collect all declaration indices, handling the case where
            // VariableData.declarations may contain VARIABLE_DECLARATION_LIST nodes
            let mut decl_indices = Vec::new();
            for &decl_idx in &var_data.declarations.nodes {
                if let Some(decl_node) = self.arena.get(decl_idx) {
                    use tsz_parser::parser::syntax_kind_ext;
                    // Check if this is a VARIABLE_DECLARATION_LIST (intermediate node)
                    if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                        // Get the VariableData for this list and collect its declarations
                        if let Some(list_var_data) = self.arena.get_variable(decl_node) {
                            for &actual_decl_idx in &list_var_data.declarations.nodes {
                                decl_indices.push(actual_decl_idx);
                            }
                        }
                    } else {
                        // Direct VARIABLE_DECLARATION node
                        decl_indices.push(decl_idx);
                    }
                }
            }

            let decls: Vec<IRNode> = decl_indices
                .iter()
                .filter_map(|&d| self.convert_variable_declaration(d))
                .collect();

            if decls.is_empty() {
                // If all declarations were filtered out (e.g., due to parsing issues),
                // fallback to source text
                return IRNode::ASTRef(idx);
            }
            if decls.len() == 1 {
                return decls
                    .into_iter()
                    .next()
                    .expect("decls has exactly 1 element, checked above");
            }
            return IRNode::VarDeclList(decls);
        }
        IRNode::ASTRef(idx)
    }

    fn convert_variable_declaration(&self, idx: NodeIndex) -> Option<IRNode> {
        let node = self.arena.get(idx)?;
        let var_decl = self.arena.get_variable_declaration(node)?;

        // Try to get identifier text, but handle binding patterns and other cases
        let name = if let Some(name) = get_identifier_text(self.arena, var_decl.name) {
            name
        } else if let Some(name_node) = self.arena.get(var_decl.name) {
            // Fallback: try to get text from source span if available
            // For binding patterns, return None and let caller handle via ASTRef
            if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                return None; // Handled via ASTRef
            }
            // Try getting identifier via IdentifierData
            if let Some(id_data) = self.arena.get_identifier(name_node) {
                id_data.escaped_text.clone()
            } else {
                return None;
            }
        } else {
            return None;
        };

        let initializer = if var_decl.initializer.is_none() {
            None
        } else {
            Some(Box::new(self.convert_expression(var_decl.initializer)))
        };
        Some(IRNode::VarDecl { name, initializer })
    }

    fn convert_throw_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // Throw uses ReturnData (same as return statement)
        if let Some(return_data) = self.arena.get_return_statement(node) {
            IRNode::ThrowStatement(Box::new(self.convert_expression(return_data.expression)))
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_try_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(try_data) = self.arena.get_try(node) {
            let try_block = Box::new(self.convert_statement(try_data.try_block));

            let catch_clause = if try_data.catch_clause.is_none() {
                None
            } else if let Some(catch_node) = self.arena.get(try_data.catch_clause)
                && let Some(catch) = self.arena.get_catch_clause(catch_node)
            {
                let param = if catch.variable_declaration.is_none() {
                    None
                } else {
                    get_identifier_text(self.arena, catch.variable_declaration)
                };
                let catch_block = self.arena.get(catch.block);
                let body = if let Some(block_node) = catch_block
                    && let Some(block) = self.arena.get_block(block_node)
                {
                    block
                        .statements
                        .nodes
                        .iter()
                        .map(|&s| self.convert_statement(s))
                        .collect()
                } else {
                    vec![]
                };
                Some(IRCatchClause { param, body })
            } else {
                None
            };

            let finally_block = if try_data.finally_block.is_none() {
                None
            } else {
                Some(Box::new(self.convert_statement(try_data.finally_block)))
            };

            IRNode::TryStatement {
                try_block,
                catch_clause,
                finally_block,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_for_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // For uses LoopData (same as while/do-while)
        if let Some(loop_data) = self.arena.get_loop(node) {
            let initializer = if loop_data.initializer.is_none() {
                None
            } else {
                Some(Box::new(self.convert_expression(loop_data.initializer)))
            };
            let condition = if loop_data.condition.is_none() {
                None
            } else {
                Some(Box::new(self.convert_expression(loop_data.condition)))
            };
            let incrementor = if loop_data.incrementor.is_none() {
                None
            } else {
                Some(Box::new(self.convert_expression(loop_data.incrementor)))
            };
            IRNode::ForStatement {
                initializer,
                condition,
                incrementor,
                body: Box::new(self.convert_statement(loop_data.statement)),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_while_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // While uses LoopData (same as for/do-while)
        if let Some(loop_data) = self.arena.get_loop(node) {
            IRNode::WhileStatement {
                condition: Box::new(self.convert_expression(loop_data.condition)),
                body: Box::new(self.convert_statement(loop_data.statement)),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_do_while_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // DoWhile uses LoopData (same as while/for loops)
        if let Some(loop_data) = self.arena.get_loop(node) {
            IRNode::DoWhileStatement {
                body: Box::new(self.convert_statement(loop_data.statement)),
                condition: Box::new(self.convert_expression(loop_data.condition)),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_switch_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(switch_data) = self.arena.get_switch(node) {
            // Case block uses BlockData where statements contains the case clauses
            let cases = if let Some(case_block_node) = self.arena.get(switch_data.case_block)
                && let Some(block_data) = self.arena.get_block(case_block_node)
            {
                block_data
                    .statements
                    .nodes
                    .iter()
                    .map(|&c| self.convert_switch_case(c))
                    .collect()
            } else {
                vec![]
            };
            IRNode::SwitchStatement {
                expression: Box::new(self.convert_expression(switch_data.expression)),
                cases,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_switch_case(&self, idx: NodeIndex) -> IRSwitchCase {
        let node = self.arena.get(idx).unwrap();
        // get_case_clause works for both CASE_CLAUSE and DEFAULT_CLAUSE
        // For DEFAULT_CLAUSE, expression is NONE
        if let Some(case_clause) = self.arena.get_case_clause(node) {
            let test = if case_clause.expression.is_none() {
                None // Default clause
            } else {
                Some(self.convert_expression(case_clause.expression))
            };
            IRSwitchCase {
                test,
                statements: case_clause
                    .statements
                    .nodes
                    .iter()
                    .map(|&s| self.convert_statement(s))
                    .collect(),
            }
        } else {
            IRSwitchCase {
                test: None,
                statements: vec![],
            }
        }
    }

    fn convert_break_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(jump_data) = self.arena.get_jump_data(node) {
            let label = if jump_data.label.is_none() {
                None
            } else {
                get_identifier_text(self.arena, jump_data.label)
            };
            IRNode::BreakStatement(label)
        } else {
            IRNode::BreakStatement(None)
        }
    }

    fn convert_continue_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(jump_data) = self.arena.get_jump_data(node) {
            let label = if jump_data.label.is_none() {
                None
            } else {
                get_identifier_text(self.arena, jump_data.label)
            };
            IRNode::ContinueStatement(label)
        } else {
            IRNode::ContinueStatement(None)
        }
    }

    fn convert_labeled_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(labeled) = self.arena.get_labeled_statement(node)
            && let Some(label) = get_identifier_text(self.arena, labeled.label)
        {
            return IRNode::LabeledStatement {
                label,
                statement: Box::new(self.convert_statement(labeled.statement)),
            };
        }
        IRNode::ASTRef(idx)
    }

    const fn convert_for_in_of_statement(&self, idx: NodeIndex) -> IRNode {
        // For-in/for-of need ES5 transformation - use ASTRef for now
        // A complete implementation would convert to a regular for loop
        IRNode::ASTRef(idx)
    }

    fn convert_identifier(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(ident) = self.arena.get_identifier(node) {
            IRNode::Identifier(ident.escaped_text.clone())
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_numeric_literal(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(lit) = self.arena.get_literal(node) {
            IRNode::NumericLiteral(lit.text.clone())
        } else {
            IRNode::ASTRef(idx)
        }
    }

    const fn convert_string_literal(&self, idx: NodeIndex) -> IRNode {
        // Use ASTRef to preserve original quote style from source text
        IRNode::ASTRef(idx)
    }

    fn convert_call_expression(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(call) = self.arena.get_call_expr(node) {
            let args: Vec<IRNode> = if let Some(ref args) = call.arguments {
                args.nodes
                    .iter()
                    .map(|&a| self.convert_expression(a))
                    .collect()
            } else {
                vec![]
            };

            // Check for super.method(args) or super[expr](args) → _super.prototype.method.call(this, args)
            if self.has_super
                && let Some(super_call) =
                    self.try_convert_super_method_call(call.expression, args.clone())
            {
                return super_call;
            }

            let callee = self.convert_expression(call.expression);
            IRNode::CallExpr {
                callee: Box::new(callee),
                arguments: args,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    /// Check if a call expression callee is super.method or super[expr] and transform to
    /// _super.prototype.method.call(this, args) or _super.prototype[expr].call(this, args)
    fn try_convert_super_method_call(
        &self,
        callee_idx: NodeIndex,
        args: Vec<IRNode>,
    ) -> Option<IRNode> {
        let callee_node = self.arena.get(callee_idx)?;

        // Check for super.method(args) → _super.prototype.method.call(this, args)
        if callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(callee_node)?;
            let obj_node = self.arena.get(access.expression)?;
            if obj_node.kind == SyntaxKind::SuperKeyword as u16 {
                let method_name = get_identifier_text(self.arena, access.name_or_argument)?;
                // Build: _super.prototype.method.call(this, args...)
                let super_proto_method = IRNode::PropertyAccess {
                    object: Box::new(IRNode::PropertyAccess {
                        object: Box::new(IRNode::id("_super")),
                        property: "prototype".to_string(),
                    }),
                    property: method_name,
                };
                let call_method = IRNode::PropertyAccess {
                    object: Box::new(super_proto_method),
                    property: "call".to_string(),
                };
                let mut call_args = vec![IRNode::This {
                    captured: self.this_captured.get(),
                }];
                call_args.extend(args);
                return Some(IRNode::CallExpr {
                    callee: Box::new(call_method),
                    arguments: call_args,
                });
            }
        }

        // Check for super[expr](args) → _super.prototype[expr].call(this, args)
        if callee_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(callee_node)?;
            let obj_node = self.arena.get(access.expression)?;
            if obj_node.kind == SyntaxKind::SuperKeyword as u16 {
                let index_expr = self.convert_expression(access.name_or_argument);
                // Build: _super.prototype[expr].call(this, args...)
                let super_proto = IRNode::PropertyAccess {
                    object: Box::new(IRNode::id("_super")),
                    property: "prototype".to_string(),
                };
                let super_proto_elem = IRNode::ElementAccess {
                    object: Box::new(super_proto),
                    index: Box::new(index_expr),
                };
                let call_method = IRNode::PropertyAccess {
                    object: Box::new(super_proto_elem),
                    property: "call".to_string(),
                };
                let mut call_args = vec![IRNode::This {
                    captured: self.this_captured.get(),
                }];
                call_args.extend(args);
                return Some(IRNode::CallExpr {
                    callee: Box::new(call_method),
                    arguments: call_args,
                });
            }
        }

        None
    }

    fn convert_new_expression(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // NewExpression uses CallExprData (same as CallExpression)
        if let Some(call_data) = self.arena.get_call_expr(node) {
            let callee = self.convert_expression(call_data.expression);
            let args = if let Some(ref args) = call_data.arguments {
                args.nodes
                    .iter()
                    .map(|&a| self.convert_expression(a))
                    .collect()
            } else {
                vec![]
            };
            IRNode::NewExpr {
                callee: Box::new(callee),
                arguments: args,
                explicit_arguments: call_data.arguments.is_some(),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_property_access(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // PropertyAccessExpression uses AccessExprData
        if let Some(access) = self.arena.get_access_expr(node) {
            // Check for super.property → _super.prototype.property
            if self.has_super
                && let Some(obj_node) = self.arena.get(access.expression)
                && obj_node.kind == SyntaxKind::SuperKeyword as u16
                && let Some(name) = get_identifier_text(self.arena, access.name_or_argument)
            {
                return IRNode::PropertyAccess {
                    object: Box::new(IRNode::PropertyAccess {
                        object: Box::new(IRNode::id("_super")),
                        property: "prototype".to_string(),
                    }),
                    property: name,
                };
            }

            let object = self.convert_expression(access.expression);
            if let Some(name) = get_identifier_text(self.arena, access.name_or_argument) {
                return IRNode::PropertyAccess {
                    object: Box::new(object),
                    property: name,
                };
            }
        }
        IRNode::ASTRef(idx)
    }

    fn convert_element_access(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // ElementAccessExpression uses AccessExprData
        if let Some(access) = self.arena.get_access_expr(node) {
            // Check for super[expr] → _super.prototype[expr]
            if self.has_super
                && let Some(obj_node) = self.arena.get(access.expression)
                && obj_node.kind == SyntaxKind::SuperKeyword as u16
            {
                let index = self.convert_expression(access.name_or_argument);
                return IRNode::ElementAccess {
                    object: Box::new(IRNode::PropertyAccess {
                        object: Box::new(IRNode::id("_super")),
                        property: "prototype".to_string(),
                    }),
                    index: Box::new(index),
                };
            }

            let object = self.convert_expression(access.expression);
            let index = self.convert_expression(access.name_or_argument);
            IRNode::ElementAccess {
                object: Box::new(object),
                index: Box::new(index),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_binary_expression(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(bin) = self.arena.get_binary_expr(node) {
            let left = self.convert_expression(bin.left);
            let right = self.convert_expression(bin.right);
            let op = self.get_binary_operator(bin.operator_token);

            // Handle logical operators specially
            if op == "||" {
                return IRNode::LogicalOr {
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
            if op == "&&" {
                return IRNode::LogicalAnd {
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }

            IRNode::BinaryExpr {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn get_binary_operator(&self, token: u16) -> String {
        match token {
            k if k == SyntaxKind::PlusToken as u16 => "+".to_string(),
            k if k == SyntaxKind::MinusToken as u16 => "-".to_string(),
            k if k == SyntaxKind::AsteriskToken as u16 => "*".to_string(),
            k if k == SyntaxKind::SlashToken as u16 => "/".to_string(),
            k if k == SyntaxKind::PercentToken as u16 => "%".to_string(),
            k if k == SyntaxKind::EqualsToken as u16 => "=".to_string(),
            k if k == SyntaxKind::PlusEqualsToken as u16 => "+=".to_string(),
            k if k == SyntaxKind::MinusEqualsToken as u16 => "-=".to_string(),
            k if k == SyntaxKind::AsteriskEqualsToken as u16 => "*=".to_string(),
            k if k == SyntaxKind::SlashEqualsToken as u16 => "/=".to_string(),
            k if k == SyntaxKind::EqualsEqualsToken as u16 => "==".to_string(),
            k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => "===".to_string(),
            k if k == SyntaxKind::ExclamationEqualsToken as u16 => "!=".to_string(),
            k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => "!==".to_string(),
            k if k == SyntaxKind::LessThanToken as u16 => "<".to_string(),
            k if k == SyntaxKind::LessThanEqualsToken as u16 => "<=".to_string(),
            k if k == SyntaxKind::GreaterThanToken as u16 => ">".to_string(),
            k if k == SyntaxKind::GreaterThanEqualsToken as u16 => ">=".to_string(),
            k if k == SyntaxKind::AmpersandAmpersandToken as u16 => "&&".to_string(),
            k if k == SyntaxKind::BarBarToken as u16 => "||".to_string(),
            k if k == SyntaxKind::AmpersandToken as u16 => "&".to_string(),
            k if k == SyntaxKind::BarToken as u16 => "|".to_string(),
            k if k == SyntaxKind::CaretToken as u16 => "^".to_string(),
            k if k == SyntaxKind::LessThanLessThanToken as u16 => "<<".to_string(),
            k if k == SyntaxKind::GreaterThanGreaterThanToken as u16 => ">>".to_string(),
            k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => {
                ">>>".to_string()
            }
            k if k == SyntaxKind::InKeyword as u16 => "in".to_string(),
            k if k == SyntaxKind::InstanceOfKeyword as u16 => "instanceof".to_string(),
            k if k == SyntaxKind::CommaToken as u16 => ",".to_string(),
            _ => "?".to_string(),
        }
    }

    fn convert_prefix_unary(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // PrefixUnaryExpression uses UnaryExprData
        if let Some(unary) = self.arena.get_unary_expr(node) {
            let operand = self.convert_expression(unary.operand);
            let op = self.get_prefix_operator(unary.operator);
            IRNode::PrefixUnaryExpr {
                operator: op,
                operand: Box::new(operand),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn get_prefix_operator(&self, token: u16) -> String {
        match token {
            k if k == SyntaxKind::PlusPlusToken as u16 => "++".to_string(),
            k if k == SyntaxKind::MinusMinusToken as u16 => "--".to_string(),
            k if k == SyntaxKind::ExclamationToken as u16 => "!".to_string(),
            k if k == SyntaxKind::TildeToken as u16 => "~".to_string(),
            k if k == SyntaxKind::PlusToken as u16 => "+".to_string(),
            k if k == SyntaxKind::MinusToken as u16 => "-".to_string(),
            k if k == SyntaxKind::TypeOfKeyword as u16 => "typeof ".to_string(),
            k if k == SyntaxKind::VoidKeyword as u16 => "void ".to_string(),
            k if k == SyntaxKind::DeleteKeyword as u16 => "delete ".to_string(),
            _ => "".to_string(),
        }
    }

    fn convert_postfix_unary(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // PostfixUnaryExpression uses UnaryExprData
        if let Some(unary) = self.arena.get_unary_expr(node) {
            let operand = self.convert_expression(unary.operand);
            let op = match unary.operator {
                k if k == SyntaxKind::PlusPlusToken as u16 => "++".to_string(),
                k if k == SyntaxKind::MinusMinusToken as u16 => "--".to_string(),
                _ => "".to_string(),
            };
            IRNode::PostfixUnaryExpr {
                operand: Box::new(operand),
                operator: op,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_parenthesized(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        if let Some(paren) = self.arena.get_parenthesized(node) {
            IRNode::Parenthesized(Box::new(self.convert_expression(paren.expression)))
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_conditional(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // ConditionalExpression uses ConditionalExprData
        if let Some(cond) = self.arena.get_conditional_expr(node) {
            IRNode::ConditionalExpr {
                condition: Box::new(self.convert_expression(cond.condition)),
                when_true: Box::new(self.convert_expression(cond.when_true)),
                when_false: Box::new(self.convert_expression(cond.when_false)),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_array_literal(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // Array and Object literals use LiteralExprData
        if let Some(arr) = self.arena.get_literal_expr(node) {
            let elements: Vec<IRNode> = arr
                .elements
                .nodes
                .iter()
                .map(|&e| self.convert_expression(e))
                .collect();
            IRNode::ArrayLiteral(elements)
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_object_literal(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // Array and Object literals use LiteralExprData (elements = properties)
        if let Some(obj) = self.arena.get_literal_expr(node) {
            let props: Vec<IRProperty> = obj
                .elements
                .nodes
                .iter()
                .filter_map(|&p| self.convert_object_property(p))
                .collect();
            IRNode::object(props)
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_object_property(&self, idx: NodeIndex) -> Option<IRProperty> {
        let node = self.arena.get(idx)?;

        if let Some(prop_assign) = self.arena.get_property_assignment(node) {
            let key = self.get_property_key(prop_assign.name)?;
            let value = self.convert_expression(prop_assign.initializer);
            Some(IRProperty {
                key,
                value,
                kind: IRPropertyKind::Init,
            })
        } else if let Some(shorthand) = self.arena.get_shorthand_property(node) {
            let name = get_identifier_text(self.arena, shorthand.name)?;
            Some(IRProperty {
                key: IRPropertyKey::Identifier(name.clone()),
                value: IRNode::Identifier(name),
                kind: IRPropertyKind::Init,
            })
        } else {
            None
        }
    }

    fn get_property_key(&self, idx: NodeIndex) -> Option<IRPropertyKey> {
        let node = self.arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            let name = get_identifier_text(self.arena, idx)?;
            Some(IRPropertyKey::Identifier(name))
        } else if node.kind == SyntaxKind::StringLiteral as u16 {
            self.arena
                .get_literal(node)
                .map(|lit| IRPropertyKey::StringLiteral(lit.text.clone()))
        } else if node.kind == SyntaxKind::NumericLiteral as u16 {
            self.arena
                .get_literal(node)
                .map(|lit| IRPropertyKey::NumericLiteral(lit.text.clone()))
        } else if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            self.arena.get_computed_property(node).map(|computed| {
                IRPropertyKey::Computed(Box::new(self.convert_expression(computed.expression)))
            })
        } else {
            None
        }
    }

    fn convert_function_expression(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // FunctionExpression uses FunctionData
        if let Some(func) = self.arena.get_function(node) {
            let name = if func.name.is_none() {
                None
            } else {
                get_identifier_text(self.arena, func.name)
            };
            let params = self.convert_parameters(&func.parameters);
            // Capture body source range for single-line detection
            let body_source_range = if !func.body.is_none() {
                self.arena
                    .get(func.body)
                    .map(|body_node| (body_node.pos, body_node.end))
            } else {
                None
            };
            let body = if func.body.is_none() {
                vec![]
            } else if let Some(body_node) = self.arena.get(func.body)
                && let Some(block) = self.arena.get_block(body_node)
            {
                block
                    .statements
                    .nodes
                    .iter()
                    .map(|&s| self.convert_statement(s))
                    .collect()
            } else {
                vec![]
            };
            IRNode::FunctionExpr {
                name,
                parameters: params,
                body,
                is_expression_body: false,
                body_source_range,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_arrow_function(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();

        // ArrowFunction uses FunctionData (has equals_greater_than_token set)
        if let Some(arrow) = self.arena.get_function(node) {
            // First check if there's a directive from LoweringPass
            let (captures_this, class_alias) = if let Some(ref transforms) = self.transforms {
                if let Some(crate::transform_context::TransformDirective::ES5ArrowFunction {
                    captures_this,
                    class_alias,
                    ..
                }) = transforms.get(idx)
                {
                    (
                        *captures_this,
                        class_alias.as_ref().map(std::string::ToString::to_string),
                    )
                } else {
                    // No directive, fall back to local analysis
                    (contains_this_reference(self.arena, idx), None)
                }
            } else {
                // No transforms available, fall back to local analysis
                (contains_this_reference(self.arena, idx), None)
            };

            // Save previous state and set captured flag if needed
            let prev_captured = self.this_captured.get();
            let prev_alias = self.current_class_alias.take();

            if captures_this {
                self.this_captured.set(true);
            }
            // Set the class_alias so `this` references in the body get converted
            self.current_class_alias.set(class_alias);

            let params = self.convert_parameters(&arrow.parameters);
            let (body, is_expression_body, body_source_range) =
                if let Some(body_node) = self.arena.get(arrow.body) {
                    if let Some(block) = self.arena.get_block(body_node) {
                        let stmts: Vec<IRNode> = block
                            .statements
                            .nodes
                            .iter()
                            .map(|&s| self.convert_statement(s))
                            .collect();
                        let range = Some((body_node.pos, body_node.end));
                        (stmts, false, range)
                    } else {
                        // Expression body
                        let expr = self.convert_expression(arrow.body);
                        (
                            vec![IRNode::ReturnStatement(Some(Box::new(expr)))],
                            true,
                            None,
                        )
                    }
                } else {
                    (vec![], false, None)
                };

            // Restore previous state
            self.this_captured.set(prev_captured);
            self.current_class_alias.set(prev_alias);

            // Arrow functions become regular functions in ES5

            // TypeScript's ES5 arrow transform:
            // - Convert arrow to plain function expression
            // - Containing function emits `var _this = this;` at body start
            // - Substitution of `this` -> `_this` is handled by IRNode::This { captured: true }
            //
            // Note: We no longer use IIFE wrappers like `(function (_this) { ... })(this)`
            // The `_this` capture should be hoisted to the containing function's body start.
            IRNode::FunctionExpr {
                name: None,
                parameters: params,
                body,
                is_expression_body,
                body_source_range,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_parameters(&self, params: &NodeList) -> Vec<IRParam> {
        params
            .nodes
            .iter()
            .filter_map(|&p| {
                let node = self.arena.get(p)?;
                let param = self.arena.get_parameter(node)?;
                let name = get_identifier_text(self.arena, param.name)?;
                let rest = param.dot_dot_dot_token;
                // Convert default value if present
                let default_value = (!param.initializer.is_none())
                    .then(|| Box::new(self.convert_expression(param.initializer)));
                Some(IRParam {
                    name,
                    rest,
                    default_value,
                })
            })
            .collect()
    }

    fn convert_spread_element(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // SpreadElement uses SpreadData
        if let Some(spread) = self.arena.get_spread(node) {
            IRNode::SpreadElement(Box::new(self.convert_expression(spread.expression)))
        } else {
            IRNode::ASTRef(idx)
        }
    }

    const fn convert_template_literal(&self, idx: NodeIndex) -> IRNode {
        // Template literals need string concatenation in ES5
        // For now, use ASTRef as a fallback
        IRNode::ASTRef(idx)
    }

    const fn convert_await_expression(&self, idx: NodeIndex) -> IRNode {
        // Await expressions are handled by the async transform
        IRNode::ASTRef(idx)
    }

    fn convert_type_assertion(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // Both TYPE_ASSERTION and AS_EXPRESSION use TypeAssertionData
        if let Some(assertion) = self.arena.get_type_assertion(node) {
            self.convert_expression(assertion.expression)
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_non_null(&self, idx: NodeIndex) -> IRNode {
        let node = self.arena.get(idx).unwrap();
        // NON_NULL_EXPRESSION uses UnaryExpressionData
        if let Some(unary) = self.arena.get_unary_expr_ex(node) {
            self.convert_expression(unary.expression)
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn is_destructuring_assignment_expr(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        let target_expr = if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            self.arena
                .get_parenthesized(expr_node)
                .map(|p| p.expression)
                .unwrap_or(expr_idx)
        } else {
            expr_idx
        };
        let Some(bin_node) = self.arena.get(target_expr) else {
            return false;
        };
        if bin_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(bin) = self.arena.get_binary_expr(bin_node) else {
            return false;
        };
        if bin.operator_token != SyntaxKind::EqualsToken as u16 {
            return false;
        }
        self.arena.get(bin.left).is_some_and(|left| {
            left.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || left.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        })
    }
}
