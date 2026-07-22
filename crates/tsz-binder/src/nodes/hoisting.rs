//! Variable/function declaration hoisting, split out of `binding.rs` to keep it
//! under the size ratchet. Included as a sibling `impl BinderState` block.

use crate::state::BinderState;
use crate::symbol_flags;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::{NodeIndex, NodeList};

impl BinderState {
    /// Collect hoisted declarations from statements.
    pub(crate) fn collect_hoisted_declarations(
        &mut self,
        arena: &NodeArena,
        statements: &NodeList,
    ) {
        self.collect_hoisted_declarations_impl(arena, statements, false);
    }

    /// Internal implementation with block tracking.
    fn collect_hoisted_declarations_impl(
        &mut self,
        arena: &NodeArena,
        statements: &NodeList,
        in_block: bool,
    ) {
        for &stmt_idx in &statements.nodes {
            if let Some(node) = arena.get(stmt_idx) {
                match node.kind {
                    k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                        if let Some(var_stmt) = arena.get_variable(node) {
                            // VariableStatement stores declaration_list as first element
                            if let Some(&decl_list_idx) = var_stmt.declarations.nodes.first() {
                                self.collect_hoisted_var_decl(arena, decl_list_idx);
                            }
                        }
                    }
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        // Function declarations inside blocks are block-scoped when:
                        // - The file is an external module (ES6 modules), or
                        // - The scope is in strict mode ("use strict" or --alwaysStrict), or
                        // - The target is ES2015 or later.
                        // In non-strict, non-module ES3/ES5 scripts, they hoist (Annex B behavior).
                        let is_es6_or_later = self.options.target as u32
                            >= tsz_common::common::ScriptTarget::ES2015 as u32;
                        let block_scoped = in_block
                            && (self.is_external_module || self.is_strict_scope || is_es6_or_later);
                        if !block_scoped {
                            self.hoisted_functions.push(stmt_idx);
                        }
                    }
                    k if k == syntax_kind_ext::BLOCK => {
                        // Always recurse into blocks for var hoisting (var is always
                        // function-scoped regardless of target).
                        // Pass in_block=true to prevent function hoisting from blocks.
                        if let Some(block) = arena.get_block(node) {
                            self.collect_hoisted_declarations_impl(arena, &block.statements, true);
                        }
                    }
                    k if k == syntax_kind_ext::IF_STATEMENT => {
                        if let Some(if_stmt) = arena.get_if_statement(node) {
                            self.collect_hoisted_from_node(arena, if_stmt.then_statement);
                            if if_stmt.else_statement.is_some() {
                                self.collect_hoisted_from_node(arena, if_stmt.else_statement);
                            }
                        }
                    }
                    k if k == syntax_kind_ext::WHILE_STATEMENT
                        || k == syntax_kind_ext::DO_STATEMENT =>
                    {
                        if let Some(loop_data) = arena.get_loop(node) {
                            self.collect_hoisted_from_node(arena, loop_data.statement);
                        }
                    }
                    k if k == syntax_kind_ext::FOR_STATEMENT => {
                        if let Some(loop_data) = arena.get_loop(node) {
                            // Hoist var declarations from initializer (e.g., `for (var i = 0; ...)`)
                            let init = loop_data.initializer;
                            if init.is_some()
                                && let Some(init_node) = arena.get(init)
                                && init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                            {
                                self.collect_hoisted_var_decl(arena, init);
                            }
                            // Hoist from the loop body
                            self.collect_hoisted_from_node(arena, loop_data.statement);
                        }
                    }
                    k if k == syntax_kind_ext::FOR_IN_STATEMENT
                        || k == syntax_kind_ext::FOR_OF_STATEMENT =>
                    {
                        if let Some(for_data) = arena.get_for_in_of(node) {
                            // Hoist var declarations from the initializer (e.g., `for (var x in obj)`)
                            let init = for_data.initializer;
                            if let Some(init_node) = arena.get(init)
                                && init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                            {
                                self.collect_hoisted_var_decl(arena, init);
                            }
                            // Hoist from the loop body
                            self.collect_hoisted_from_node(arena, for_data.statement);
                        }
                    }
                    k if k == syntax_kind_ext::TRY_STATEMENT => {
                        if let Some(try_data) = arena.get_try(node) {
                            // Hoist from try block
                            self.collect_hoisted_from_node(arena, try_data.try_block);
                            // Hoist from catch clause's block
                            if try_data.catch_clause.is_some()
                                && let Some(catch_data) =
                                    arena.get_catch_clause_at(try_data.catch_clause)
                            {
                                self.collect_hoisted_from_node(arena, catch_data.block);
                            }
                            // Hoist from finally block
                            if try_data.finally_block.is_some() {
                                self.collect_hoisted_from_node(arena, try_data.finally_block);
                            }
                        }
                    }
                    k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                        if let Some(switch_data) = arena.get_switch(node) {
                            // The case_block is treated as a block - get its children (case/default clauses)
                            if let Some(block_data) = arena.get_block_at(switch_data.case_block) {
                                // Each child is a case/default clause with statements
                                for &clause_idx in &block_data.statements.nodes {
                                    if let Some(clause_data) = arena.get_case_clause_at(clause_idx)
                                    {
                                        self.collect_hoisted_declarations(
                                            arena,
                                            &clause_data.statements,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    k if k == syntax_kind_ext::LABELED_STATEMENT => {
                        if let Some(label_data) = arena.get_labeled_statement(node) {
                            self.collect_hoisted_from_node(arena, label_data.statement);
                        }
                    }
                    // `var` inside a `with` body is function-scoped and hoisted by
                    // tsc; without this arm the name is never collected, so a later
                    // reference / `export { name }` resolves to nothing -> spurious
                    // TS2304. Mirrors the IF/LABELED sibling arms.
                    k if k == syntax_kind_ext::WITH_STATEMENT => {
                        if let Some(with_stmt) = arena.get_with_statement(node) {
                            self.collect_hoisted_from_node(arena, with_stmt.then_statement);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub(crate) fn collect_hoisted_var_decl(&mut self, arena: &NodeArena, decl_list_idx: NodeIndex) {
        if let Some(node) = arena.get(decl_list_idx)
            && let Some(list) = arena.get_variable(node)
        {
            // Check if this is a var declaration (not let/const)
            let is_var = !node_flags::is_let_or_const(u32::from(node.flags));
            if is_var {
                for &decl_idx in &list.declarations.nodes {
                    if let Some(decl) = arena.get_variable_declaration_at(decl_idx) {
                        if let Some(name) = Self::get_identifier_name(arena, decl.name) {
                            self.hoisted_vars.push((name.to_string(), decl_idx));
                        } else {
                            let mut names = Vec::new();
                            Self::collect_binding_identifiers(arena, decl.name, &mut names);
                            for ident_idx in names {
                                if let Some(name) = Self::get_identifier_name(arena, ident_idx) {
                                    self.hoisted_vars.push((name.to_string(), ident_idx));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn collect_hoisted_from_node(&mut self, arena: &NodeArena, idx: NodeIndex) {
        if let Some(node) = arena.get(idx) {
            if node.kind == syntax_kind_ext::BLOCK {
                // Always recurse into blocks for var hoisting (var is always
                // function-scoped regardless of target).
                // Function declarations directly in a function body are at the
                // function scope; only nested blocks make them block-scoped.
                if let Some(block) = arena.get_block(node) {
                    let is_function_body = arena
                        .get_extended(idx)
                        .and_then(|ext| arena.get(ext.parent))
                        .is_some_and(|parent_node| {
                            matches!(
                                parent_node.kind,
                                syntax_kind_ext::FUNCTION_DECLARATION
                                    | syntax_kind_ext::FUNCTION_EXPRESSION
                                    | syntax_kind_ext::ARROW_FUNCTION
                                    | syntax_kind_ext::METHOD_DECLARATION
                                    | syntax_kind_ext::CONSTRUCTOR
                                    | syntax_kind_ext::GET_ACCESSOR
                                    | syntax_kind_ext::SET_ACCESSOR
                            )
                        });
                    self.collect_hoisted_declarations_impl(
                        arena,
                        &block.statements,
                        !is_function_body,
                    );
                }
            } else if node.kind == syntax_kind_ext::MODULE_BLOCK {
                // Namespace bodies are function scopes: top-level `function`
                // declarations in the body are not block-scoped.
                if let Some(block) = arena.get_module_block(node)
                    && let Some(ref statements) = block.statements
                {
                    self.collect_hoisted_declarations_impl(arena, statements, false);
                }
            } else {
                // Handle single statement (not wrapped in a block)
                // e.g., `if (x) var y = 1;` or `while (x) var i = 0;`
                // These are at the same scope level, not in a block.
                let mut stmts = tsz_parser::NodeList::new();
                stmts.nodes.push(idx);
                self.collect_hoisted_declarations(arena, &stmts);
            }
        }
    }

    /// Process hoisted function declarations.
    pub(crate) fn process_hoisted_functions(&mut self, arena: &NodeArena) {
        let functions = std::mem::take(&mut self.hoisted_functions);
        for func_idx in functions {
            if let Some(node) = arena.get(func_idx)
                && let Some(func) = arena.get_function(node)
                && let Some(name) = Self::get_identifier_name(arena, func.name)
            {
                let is_exported = Self::has_export_modifier(arena, func.modifiers.as_ref());
                let sym_id =
                    self.declare_symbol(arena, name, symbol_flags::FUNCTION, func_idx, is_exported);

                // Also add to persistent scope
                self.declare_in_persistent_scope(name.to_string(), sym_id);
            }
        }
    }

    /// Process hoisted var declarations.
    /// Var declarations are hoisted to the top of their function/global scope.
    pub(crate) fn process_hoisted_vars(&mut self, arena: &NodeArena) {
        let hoisted_vars = std::mem::take(&mut self.hoisted_vars);
        for (name, decl_idx) in hoisted_vars {
            // Declare the var symbol with FUNCTION_SCOPED_VARIABLE flag
            // This makes it accessible before its actual declaration point
            let is_exported = Self::is_node_exported(arena, decl_idx);
            let sym_id = self.declare_symbol(
                arena,
                &name,
                symbol_flags::FUNCTION_SCOPED_VARIABLE,
                decl_idx,
                is_exported,
            );

            // Also add to persistent scope
            self.declare_in_persistent_scope(name, sym_id);
        }
    }
}
