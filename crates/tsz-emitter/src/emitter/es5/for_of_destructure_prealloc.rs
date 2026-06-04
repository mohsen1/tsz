//! ES5 assignment-target destructuring temp pre-pass.
//!
//! Extracted from `bindings_for_of.rs` / `helpers.rs` so the `emit.rs` and
//! `helpers.rs` monoliths stay under their §19 size ratchet. This module owns
//! temp pre-allocation that reserves hoisted assignment-destructuring temps in
//! source order before non-hoisted ES5 temps in the same scope consume names.

use super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{ForInOfData, Node};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    /// Pre-pass that allocates hoisted destructuring-assignment temps for ES5
    /// statement scopes in source order, before any statement is emitted.
    ///
    /// tsc assigns auto-generated temp names at print time in source order: the
    /// hoisted `var _a, _b, ...;` declaration prints at the top of the scope, so
    /// every hoisted assignment-destructuring temp claims a low number before
    /// non-hoisted temps from block-scoped declarations or for-of loop-control.
    /// tsz allocates names eagerly while emitting, so without this pre-pass those
    /// temps can interleave. Running the real destructuring lowering for each
    /// hoisted assignment target into a throwaway writer reserves temps in the
    /// exact order the later emit consumes them.
    ///
    /// Runs at a scope boundary where `hoisted_assignment_temps` is empty (source
    /// file / function body). The dry-run lowering allocates names through the
    /// normal `make_unique_name_hoisted_assignment` path, so they accumulate in
    /// `hoisted_assignment_temps`; we then move them into
    /// `preallocated_assignment_temps` so the real emit replays the same names in
    /// the same order while the loop-control (index/array) temps take the higher
    /// numbers.
    pub(in crate::emitter) fn prealloc_for_of_destructure_temps(
        &mut self,
        statements: &[NodeIndex],
    ) {
        if !self.ctx.target_es5 || self.ctx.options.downlevel_iteration {
            return;
        }
        for &stmt_idx in statements {
            self.visit_for_of_assignment_temp_prealloc(stmt_idx);
        }
        // The dry runs pushed every allocated destructuring temp onto the hoist
        // pool; hand them to the assignment-temp queue (in order) and clear the
        // pool so the real emit re-records them as it replays the names.
        let collected = std::mem::take(&mut self.hoisted_assignment_temps);
        for name in collected {
            self.preallocated_assignment_temps.push_back(name);
        }
    }

    fn visit_for_of_assignment_temp_prealloc(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::FOR_OF_STATEMENT {
            if let Some(for_in_of) = self.arena.get_for_in_of(node) {
                if !for_in_of.await_modifier {
                    self.prealloc_for_of_assignment_destructure_temps(for_in_of);
                }
                self.visit_for_of_assignment_temp_prealloc(for_in_of.statement);
            }
            return;
        }

        if node.kind == syntax_kind_ext::FOR_STATEMENT {
            if let Some(loop_data) = self.arena.get_loop(node) {
                self.prealloc_for_initializer_assignment_destructure_temps(loop_data.initializer);
                self.visit_for_of_assignment_temp_prealloc(loop_data.statement);
            }
            return;
        }

        // Descend into nested statement containers, but stop at function/class
        // boundaries — those introduce their own temp scope and hoist pool.
        match node.kind {
            k if k == syntax_kind_ext::BLOCK || k == syntax_kind_ext::CASE_BLOCK => {
                if let Some(block) = self.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.visit_for_of_assignment_temp_prealloc(stmt);
                    }
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.arena.get_if_statement(node) {
                    self.visit_for_of_assignment_temp_prealloc(if_stmt.then_statement);
                    self.visit_for_of_assignment_temp_prealloc(if_stmt.else_statement);
                }
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_stmt) = self.arena.get_try(node) {
                    self.visit_for_of_assignment_temp_prealloc(try_stmt.try_block);
                    self.visit_for_of_assignment_temp_prealloc(try_stmt.catch_clause);
                    self.visit_for_of_assignment_temp_prealloc(try_stmt.finally_block);
                }
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_clause) = self.arena.get_catch_clause(node) {
                    self.visit_for_of_assignment_temp_prealloc(catch_clause.block);
                }
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT =>
            {
                if let Some(loop_data) = self.arena.get_loop(node) {
                    self.visit_for_of_assignment_temp_prealloc(loop_data.statement);
                } else if let Some(for_in_of) = self.arena.get_for_in_of(node) {
                    self.visit_for_of_assignment_temp_prealloc(for_in_of.statement);
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(sw) = self.arena.get_switch(node) {
                    self.visit_for_of_assignment_temp_prealloc(sw.case_block);
                }
            }
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                if let Some(clause) = self.arena.get_case_clause(node) {
                    for &stmt in &clause.statements.nodes {
                        self.visit_for_of_assignment_temp_prealloc(stmt);
                    }
                }
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.arena.get_labeled_statement(node) {
                    self.visit_for_of_assignment_temp_prealloc(labeled.statement);
                }
            }
            k if k == syntax_kind_ext::WITH_STATEMENT => {
                if let Some(with_stmt) = self.arena.get_with_statement(node) {
                    self.visit_for_of_assignment_temp_prealloc(with_stmt.then_statement);
                }
            }
            _ => {}
        }
    }

    /// If `for_in_of` is an assignment-target destructuring for-of that takes the
    /// ES5 array-indexing path, run its destructuring lowering into a throwaway
    /// writer so the hoisted temps it allocates claim their numbers now.
    fn prealloc_for_of_assignment_destructure_temps(&mut self, for_in_of: &ForInOfData) {
        let Some(init_node) = self.arena.get(for_in_of.initializer) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            && init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        {
            return;
        }
        // Empty assignment patterns allocate no destructuring temp.
        if self
            .arena
            .get_literal_expr(init_node)
            .is_some_and(|lit| lit.elements.nodes.is_empty())
        {
            return;
        }

        // Swap in a scratch writer (no source map) so the destructuring text and
        // mappings produced by this dry run are discarded; only the temp counter
        // and the allocated hoist-pool names advance.
        let scratch = crate::output::source_writer::SourceWriter::new();
        let real_writer = std::mem::replace(&mut self.writer, scratch);

        self.emit_for_of_assignment_target_destructuring_es5(init_node, "_");

        self.writer = real_writer;
    }

    /// Reserve hoisted assignment-destructuring temps created by an ordinary
    /// `for` initializer expression. The block-scoped binding declaration in
    /// `for (let [x] = value; ...)` uses inline temps in the header, not hoisted
    /// assignment temps, so this only descends into declaration initializers and
    /// expression initializers.
    fn prealloc_for_initializer_assignment_destructure_temps(&mut self, initializer: NodeIndex) {
        if initializer.is_none() {
            return;
        }
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };

        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            if let Some(decl_list) = self.arena.get_variable(init_node) {
                for &decl_idx in &decl_list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                        continue;
                    };
                    self.prealloc_expression_assignment_destructure_temps(decl.initializer);
                }
            }
            return;
        }

        self.prealloc_expression_assignment_destructure_temps(initializer);
    }

    fn prealloc_expression_assignment_destructure_temps(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let Some(binary) = self.arena.get_binary_expr(node) else {
                return;
            };
            if binary.operator_token == SyntaxKind::CommaToken as u16 {
                self.prealloc_expression_assignment_destructure_temps(binary.left);
                self.prealloc_expression_assignment_destructure_temps(binary.right);
                return;
            }

            if binary.operator_token == SyntaxKind::EqualsToken as u16
                && let Some(left_node) = self.arena.get(binary.left)
                && Self::is_assignment_destructure_pattern(left_node)
            {
                self.prealloc_assignment_destructure_expression(left_node, binary.right);
            }
        }
    }

    const fn is_assignment_destructure_pattern(node: &Node) -> bool {
        matches!(
            node.kind,
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN
                || k == syntax_kind_ext::OBJECT_BINDING_PATTERN
        )
    }

    fn prealloc_assignment_destructure_expression(&mut self, left_node: &Node, right: NodeIndex) {
        let scratch = crate::output::source_writer::SourceWriter::new();
        let real_writer = std::mem::replace(&mut self.writer, scratch);
        let saved_pending_source_pos = self.pending_source_pos.take();
        let saved_pending_block_comment_space = self.pending_block_comment_space;
        let saved_comment_emit_idx = self.comment_emit_idx;

        self.pending_block_comment_space = false;
        self.emit_assignment_destructuring_es5(left_node, right);

        self.writer = real_writer;
        self.pending_source_pos = saved_pending_source_pos;
        self.pending_block_comment_space = saved_pending_block_comment_space;
        self.comment_emit_idx = saved_comment_emit_idx;
    }
}
