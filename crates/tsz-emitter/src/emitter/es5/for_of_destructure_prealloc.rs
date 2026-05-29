//! ES5 for-of assignment-target destructuring temp pre-pass.
//!
//! Extracted from `bindings_for_of.rs` / `helpers.rs` so the `emit.rs` and
//! `helpers.rs` monoliths stay under their §19 size ratchet. Behavior is
//! unchanged: this module owns the temp pre-allocation that reserves the
//! hoisted destructuring temps for assignment-target `for-of` loops before any
//! for-of loop-control (index/array) temp.

use super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::ForInOfData;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    /// Pre-pass that allocates the hoisted destructuring-assignment temps for all
    /// ES5 array-indexing assignment-target `for-of` statements in `statements`,
    /// in source order, before any loop is emitted.
    ///
    /// tsc assigns auto-generated temp names at print time in source order: the
    /// hoisted `var _a, _b, ...;` declaration prints at the top of the scope, so
    /// every assignment-target for-of destructuring temp claims a low number
    /// before any for-of loop-control (index/array) temp. tsz allocates names
    /// eagerly while emitting, so without this pre-pass the destructuring and
    /// loop-control temps interleave. Running the real destructuring lowering for
    /// each such for-of into a throwaway writer reserves those temps in the exact
    /// order the later emit consumes them.
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
            k if k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::FOR_IN_STATEMENT
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
}
