//! Statement validation helpers used by statement callbacks.

use crate::state::CheckerState;
use crate::statements::StatementCheckCallbacks;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_declare_modifiers_in_ambient_body(&mut self, body_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            return;
        }

        let Some(block) = self.ctx.arena.get_module_block(body_node) else {
            return;
        };

        let Some(ref statements) = block.statements else {
            return;
        };

        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            // Check different declaration types for 'declare' modifier
            let modifiers = match stmt_node.kind {
                syntax_kind_ext::FUNCTION_DECLARATION => {
                    self.ctx.arena.get_function(stmt_node).map(|f| &f.modifiers)
                }
                syntax_kind_ext::VARIABLE_STATEMENT => {
                    self.ctx.arena.get_variable(stmt_node).map(|v| &v.modifiers)
                }
                syntax_kind_ext::CLASS_DECLARATION => {
                    self.ctx.arena.get_class(stmt_node).map(|c| &c.modifiers)
                }
                syntax_kind_ext::INTERFACE_DECLARATION => self
                    .ctx
                    .arena
                    .get_interface(stmt_node)
                    .map(|i| &i.modifiers),
                syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                    .ctx
                    .arena
                    .get_type_alias(stmt_node)
                    .map(|t| &t.modifiers),
                syntax_kind_ext::ENUM_DECLARATION => {
                    self.ctx.arena.get_enum(stmt_node).map(|e| &e.modifiers)
                }
                syntax_kind_ext::MODULE_DECLARATION => {
                    self.ctx.arena.get_module(stmt_node).map(|m| &m.modifiers)
                }
                _ => None,
            };

            if let Some(mods) = modifiers {
                if let Some(declare_mod) = self.get_declare_modifier(mods) {
                    self.error_at_node(
                        declare_mod,
                        "A 'declare' modifier cannot be used in an already ambient context.",
                        diagnostic_codes::A_DECLARE_MODIFIER_CANNOT_BE_USED_IN_AN_ALREADY_AMBIENT_CONTEXT,
                    );
                }
            }
        }
    }

    /// TS1039: Check for variable initializers in ambient contexts.
    /// This is checked even when we skip full type checking of ambient module bodies.
    pub(crate) fn check_initializers_in_ambient_body(&mut self, body_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_parser::parser::syntax_kind_ext;

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            return;
        }

        let Some(block) = self.ctx.arena.get_module_block(body_node) else {
            return;
        };

        let Some(ref statements) = block.statements else {
            return;
        };

        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            // Get the actual variable statement - it might be wrapped in an export declaration
            // For example: export var x = 1; is parsed as EXPORT_DECLARATION with export_clause pointing to VARIABLE_STATEMENT
            let var_stmt_node = if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                if let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node) {
                    if export_decl.export_clause.is_none() {
                        continue;
                    }
                    let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause) else {
                        continue;
                    };
                    clause_node
                } else {
                    continue;
                }
            } else if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                stmt_node
            } else {
                continue;
            };

            // Check variable statements for initializers
            if var_stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                if let Some(var_stmt) = self.ctx.arena.get_variable(var_stmt_node) {
                    // var_stmt.declarations.nodes contains VariableDeclarationList nodes
                    // We need to get each list and then iterate its declarations
                    for &list_idx in &var_stmt.declarations.nodes {
                        if let Some(list_node) = self.ctx.arena.get(list_idx)
                            && let Some(decl_list) = self.ctx.arena.get_variable(list_node)
                        {
                            use tsz_parser::parser::node_flags;
                            let is_const = (list_node.flags & node_flags::CONST as u16) != 0;

                            for &decl_idx in &decl_list.declarations.nodes {
                                if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                                    && let Some(var_decl) =
                                        self.ctx.arena.get_variable_declaration(decl_node)
                                    && !var_decl.initializer.is_none()
                                {
                                    if is_const && var_decl.type_annotation.is_none() {
                                        // const without type annotation: only string/numeric literals allowed
                                        // TS1254 if initializer is not a valid literal
                                        if !self.is_valid_const_initializer(var_decl.initializer) {
                                            self.error_at_node(
                                                var_decl.initializer,
                                                diagnostic_messages::A_CONST_INITIALIZER_IN_AN_AMBIENT_CONTEXT_MUST_BE_A_STRING_OR_NUMERIC_LITERAL_OR,
                                                diagnostic_codes::A_CONST_INITIALIZER_IN_AN_AMBIENT_CONTEXT_MUST_BE_A_STRING_OR_NUMERIC_LITERAL_OR,
                                            );
                                        }
                                        // else: valid literal initializer, no error
                                    } else {
                                        // Non-const or const with type annotation: TS1039
                                        self.error_at_node(
                                            var_decl.initializer,
                                            diagnostic_messages::INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                                            diagnostic_codes::INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Recursively check nested modules/namespaces
            if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                if let Some(module) = self.ctx.arena.get_module(stmt_node) {
                    if !module.body.is_none() {
                        self.check_initializers_in_ambient_body(module.body);
                    }
                }
            }
        }
    }

    /// Check if a node is a valid const initializer in an ambient context.
    /// Valid initializers are string literals, numeric literals, or negative numeric literals.
    fn is_valid_const_initializer(&self, init_idx: NodeIndex) -> bool {
        self.is_valid_ambient_const_initializer(init_idx)
    }

    /// Check a break statement for validity.
    /// Check a with statement and emit TS2410.
    /// The 'with' statement is not supported in TypeScript.
    pub(crate) fn check_with_statement(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        self.error_at_node(
            stmt_idx,
            diagnostic_messages::THE_WITH_STATEMENT_IS_NOT_SUPPORTED_ALL_SYMBOLS_IN_A_WITH_BLOCK_WILL_HAVE_TYPE_A,
            diagnostic_codes::THE_WITH_STATEMENT_IS_NOT_SUPPORTED_ALL_SYMBOLS_IN_A_WITH_BLOCK_WILL_HAVE_TYPE_A,
        );

        if self.ctx.in_async_context() {
            self.error_at_node(
                stmt_idx,
                diagnostic_messages::WITH_STATEMENTS_ARE_NOT_ALLOWED_IN_AN_ASYNC_FUNCTION_BLOCK,
                diagnostic_codes::WITH_STATEMENTS_ARE_NOT_ALLOWED_IN_AN_ASYNC_FUNCTION_BLOCK,
            );
        }

        if self.is_with_statement_in_strict_mode_context(stmt_idx) {
            self.error_at_node(
                stmt_idx,
                diagnostic_messages::WITH_STATEMENTS_ARE_NOT_ALLOWED_IN_STRICT_MODE,
                diagnostic_codes::WITH_STATEMENTS_ARE_NOT_ALLOWED_IN_STRICT_MODE,
            );
        }
    }

    fn is_with_statement_in_strict_mode_context(&self, stmt_idx: NodeIndex) -> bool {
        if self.ctx.compiler_options.always_strict {
            return true;
        }

        let mut current = stmt_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                || parent_node.kind == syntax_kind_ext::METHOD_DECLARATION
                || parent_node.kind == syntax_kind_ext::GET_ACCESSOR
                || parent_node.kind == syntax_kind_ext::SET_ACCESSOR
                || parent_node.kind == syntax_kind_ext::CONSTRUCTOR
                || parent_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
            {
                return true;
            }

            current = parent_idx;
        }

        false
    }

    /// TS1105: A 'break' statement can only be used within an enclosing iteration statement or switch statement.
    /// TS1107: Jump target cannot cross function boundary.
    /// TS1116: A 'break' statement can only jump to a label of an enclosing statement.
    pub(crate) fn check_break_statement(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        // Get the label if any
        let label_name = self
            .ctx
            .arena
            .get(stmt_idx)
            .and_then(|node| self.ctx.arena.get_jump_data(node))
            .and_then(|jump_data| {
                if jump_data.label.is_none() {
                    None
                } else {
                    self.get_node_text(jump_data.label)
                }
            });

        if let Some(label) = label_name {
            // Labeled break - look up the label
            if let Some(label_info) = self.find_label(&label) {
                // Check if the label crosses a function boundary
                if label_info.function_depth < self.ctx.function_depth {
                    self.error_at_node(
                        stmt_idx,
                        "Jump target cannot cross function boundary.",
                        diagnostic_codes::JUMP_TARGET_CANNOT_CROSS_FUNCTION_BOUNDARY,
                    );
                }
                // Otherwise, labeled break is valid (can target any label, not just iteration)
            } else {
                // Label not found - emit TS1116
                self.error_at_node(
                    stmt_idx,
                    "A 'break' statement can only jump to a label of an enclosing statement.",
                    diagnostic_codes::A_BREAK_STATEMENT_CAN_ONLY_JUMP_TO_A_LABEL_OF_AN_ENCLOSING_STATEMENT,
                );
            }
        } else {
            // Unlabeled break - must be inside iteration or switch
            if self.ctx.iteration_depth == 0 && self.ctx.switch_depth == 0 {
                // Check if we're inside a function that's inside a loop
                // If so, emit TS1107 (crossing function boundary) instead of TS1105
                if self.ctx.function_depth > 0 && self.ctx.had_outer_loop {
                    self.error_at_node(
                        stmt_idx,
                        "Jump target cannot cross function boundary.",
                        diagnostic_codes::JUMP_TARGET_CANNOT_CROSS_FUNCTION_BOUNDARY,
                    );
                } else {
                    self.error_at_node(
                        stmt_idx,
                        "A 'break' statement can only be used within an enclosing iteration or switch statement.",
                        diagnostic_codes::A_BREAK_STATEMENT_CAN_ONLY_BE_USED_WITHIN_AN_ENCLOSING_ITERATION_OR_SWITCH_STATE,
                    );
                }
            }
        }
    }

    /// Check a continue statement for validity.
    /// TS1104: A 'continue' statement can only be used within an enclosing iteration statement.
    /// TS1107: Jump target cannot cross function boundary.
    /// TS1116: A 'continue' statement can only jump to a label of an enclosing iteration statement.
    pub(crate) fn check_continue_statement(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        // Get the label if any
        let label_name = self
            .ctx
            .arena
            .get(stmt_idx)
            .and_then(|node| self.ctx.arena.get_jump_data(node))
            .and_then(|jump_data| {
                if jump_data.label.is_none() {
                    None
                } else {
                    self.get_node_text(jump_data.label)
                }
            });

        if let Some(label) = label_name {
            // Labeled continue - look up the label
            if let Some(label_info) = self.find_label(&label) {
                // Check if the label crosses a function boundary
                if label_info.function_depth < self.ctx.function_depth {
                    self.error_at_node(
                        stmt_idx,
                        "Jump target cannot cross function boundary.",
                        diagnostic_codes::JUMP_TARGET_CANNOT_CROSS_FUNCTION_BOUNDARY,
                    );
                } else if !label_info.is_iteration {
                    // Continue can only target iteration labels (label found but not on loop) - TS1115
                    self.error_at_node(
                        stmt_idx,
                        "A 'continue' statement can only target a label of an enclosing iteration statement.",
                        diagnostic_codes::A_CONTINUE_STATEMENT_CAN_ONLY_JUMP_TO_A_LABEL_OF_AN_ENCLOSING_ITERATION_STATEMEN,
                    );
                }
                // Otherwise, labeled continue to iteration label is valid
            } else {
                // Label not found - emit TS1115 (same as when label exists but not on iteration)
                self.error_at_node(
                    stmt_idx,
                    "A 'continue' statement can only target a label of an enclosing iteration statement.",
                    diagnostic_codes::A_CONTINUE_STATEMENT_CAN_ONLY_JUMP_TO_A_LABEL_OF_AN_ENCLOSING_ITERATION_STATEMEN,
                );
            }
        } else {
            // Unlabeled continue - must be inside iteration
            if self.ctx.iteration_depth == 0 {
                // Check if we're inside a function that's inside a loop
                // If so, emit TS1107 (crossing function boundary) instead of TS1104
                if self.ctx.function_depth > 0 && self.ctx.had_outer_loop {
                    self.error_at_node(
                        stmt_idx,
                        "Jump target cannot cross function boundary.",
                        diagnostic_codes::JUMP_TARGET_CANNOT_CROSS_FUNCTION_BOUNDARY,
                    );
                } else {
                    self.error_at_node(
                        stmt_idx,
                        "A 'continue' statement can only be used within an enclosing iteration statement.",
                        diagnostic_codes::A_CONTINUE_STATEMENT_CAN_ONLY_BE_USED_WITHIN_AN_ENCLOSING_ITERATION_STATEMENT,
                    );
                }
            }
        }
    }

    /// Find a label in the label stack by name.
    fn find_label(&self, name: &str) -> Option<&crate::context::LabelInfo> {
        self.ctx
            .label_stack
            .iter()
            .rev()
            .find(|info| info.name == name)
    }
}
