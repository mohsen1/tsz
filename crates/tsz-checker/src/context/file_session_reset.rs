//! T2.1 file-session reset boundary.
//!
//! Per `PERFORMANCE_PLAN.md` §6 step 5 ("Introduce `FileSession` reset
//! boundaries after fields are classified"), this module implements the
//! `reset_for_next_file()` helper on `CheckerContext` that clears the
//! file-local state most likely to leak across file boundaries.
//!
//! Scope of this first pass: the critical subset shown in plan §6's
//! illustrative impl — file-keyed diagnostic buffers, node-keyed
//! request/class caches, resolution-stack debug invariants, and the
//! speculative depth counters that gate recursion. Many other
//! `FileLocalReset` manifest entries are caches keyed by stable solver
//! values (e.g. `SymbolId`) and are safe to leave populated across
//! files — clearing them would just force a re-fetch with no
//! correctness gain. Those entries will be drained in follow-up PRs
//! only if attribution data shows the cold-start cost matters.
//!
//! This helper is **not yet called from anywhere** — it exists as the
//! boundary API so that the future T2.1.B "sequential session-reuse
//! path behind a flag" PR can wire it into the per-file loop without
//! also designing the reset semantics at the same time.

use super::CheckerContext;

impl CheckerContext<'_> {
    /// Reset file-local state so the same `CheckerContext` can be
    /// reused for the next file in a sequential session-reuse path.
    ///
    /// Clears or resets the fields the plan §6 marks as having the
    /// highest cross-file leak risk:
    ///
    /// - **Diagnostic buffers** (`DiagnosticsOnly` class): diagnostics
    ///   collected during this file's check would otherwise spill into
    ///   the next file's diagnostic stream.
    /// - **Position-keyed `emitted_diagnostics`** set: positions are
    ///   file-local indices, so retaining them would suppress a
    ///   genuine duplicate in the next file.
    /// - **`NodeIndex`-keyed caches** (`request_node_types`,
    ///   `class_instance_type_cache`, `class_constructor_type_cache`):
    ///   raw `NodeIndex` collides across files; carrying entries
    ///   would return one file's type for another file's node.
    /// - **Resolution stacks** (`node_resolution_stack`,
    ///   `import_resolution_stack`): non-empty stacks would create
    ///   false-recursion diagnostics in the next file. Symbol-
    ///   resolution stack/set are `debug_assert!`'d empty rather
    ///   than force-cleared, because a non-empty state at file
    ///   boundary indicates a programming error in the prior file's
    ///   check, not a value worth silently discarding.
    /// - **Implicit-any closure sets**: keyed by node id, would
    ///   suppress or replay errors in the wrong file.
    /// - **Class-checking sets** (`checking_classes`,
    ///   `checked_classes`): retain state would cause false
    ///   "already checked" decisions in the next file.
    /// - **Pending-circular-return sites**: contains `NodeIndex`
    ///   values that collide across files.
    /// - **No-overload call nodes**: keyed by node id; retaining
    ///   would mis-flag calls in the next file.
    /// - **Depth counters** (`call_depth`, `circ_ref_depth`,
    ///   `overlap_depth`, `recursion_depth`, `instantiation_depth`):
    ///   non-zero depth at file boundary would suppress legitimate
    ///   recursion in the next file or trip TS2589-like behaviour.
    /// - **Module thread-local memoisations** in
    ///   `types::utilities::{cycle_guard, enum_utils, const_enum_eval}`:
    ///   each is keyed by `NodeIndex` and must be cleared when
    ///   reusing a worker across files.
    ///
    /// Fields not cleared in this pass (and why):
    ///
    /// - `SymbolId`-keyed caches (`symbol_types`,
    ///   `symbol_instance_types`, `lib_delegation_cache`, etc.):
    ///   stable symbol identity makes these correct to retain.
    /// - `Atom`/string-keyed lib caches: stable across compilations.
    /// - The bulk of the 119 `FileLocalReset` manifest entries:
    ///   purely-keyed caches whose retained entries are
    ///   correctness-neutral and merely costs a re-fetch. They will
    ///   be added here only if attribution data shows cold-start
    ///   cost matters.
    ///
    /// # Speculation
    ///
    /// This helper does **not** handle speculative rollback. The
    /// `SpeculationScoped` lifetime class is rolled back by its own
    /// save/restore mechanism scoped to overload/generic checking;
    /// this reset is for *successful* file completion only.
    pub fn reset_for_next_file(&mut self) {
        // Attribution counter: increments only on the sequential session-
        // reuse path (T2.1.B). Zero on the default construction-per-file
        // path, so reuse vs construct is observable from a single counter.
        // The helper gates on `enabled_fast()` once before the
        // `counters()` `OnceLock` deref, so disabled runs pay only one
        // relaxed atomic load + branch.
        tsz_common::perf_counters::record_file_session_reset();

        // Diagnostic buffers.
        self.diagnostics.clear();
        self.emitted_diagnostics.clear();

        // Node-keyed caches.
        self.request_node_types.clear();
        self.class_instance_type_cache.clear();
        self.class_constructor_type_cache.clear();

        // Resolution stacks (force-clear the import stack; symbol-
        // resolution stack/set are asserted empty as an invariant).
        self.node_resolution_stack.clear();
        self.import_resolution_stack.clear();

        // Implicit-any tracking sets.
        self.implicit_any_checked_closures.clear();
        self.implicit_any_contextual_closures.clear();
        self.deferred_implicit_any_closures.clear();
        self.speculative_implicit_any_closures.clear();

        // Class checking state.
        self.checking_classes.clear();
        self.checked_classes.clear();

        // Pending-circular-return sites + no-overload call nodes.
        self.pending_circular_return_sites.clear();
        self.no_overload_call_nodes.clear();

        // Depth counters: reset to their base depth and clear the
        // `exceeded` flag.
        self.call_depth.borrow_mut().reset();
        self.circ_ref_depth.borrow_mut().reset();
        self.overlap_depth.borrow_mut().reset();
        self.recursion_depth.borrow_mut().reset();
        self.instantiation_depth.set(0);

        // Module-scoped thread-local memoisations that key by file-
        // local `NodeIndex`.
        crate::types_domain::utilities::cycle_guard::clear_visited_sets();
        crate::types_domain::utilities::enum_utils::clear_enum_eval_memo();
        crate::types_domain::utilities::const_enum_eval::clear_const_eval_memo();

        // Invariants: these stacks must be empty at the file
        // boundary. A non-empty state indicates a logic bug in the
        // prior file's check (missing pop, early return inside a
        // resolution scope). Force-clearing would mask that bug.
        debug_assert!(
            self.symbol_resolution_stack.is_empty(),
            "symbol_resolution_stack non-empty at file boundary",
        );
        debug_assert!(
            self.symbol_resolution_set.is_empty(),
            "symbol_resolution_set non-empty at file boundary",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CheckerOptions;
    use tsz_binder::BinderState;
    use tsz_parser::parser::NodeArena;
    use tsz_solver::TypeInterner;

    fn fresh_ctx<'a>(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a TypeInterner,
    ) -> CheckerContext<'a> {
        CheckerContext::new(
            arena,
            binder,
            types,
            "test.ts".to_string(),
            CheckerOptions::default(),
        )
    }

    #[test]
    fn reset_clears_diagnostic_buffers_and_node_keyed_caches() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut ctx = fresh_ctx(&arena, &binder, &types);

        // Populate via direct field access (we control the test).
        ctx.diagnostics.push(crate::diagnostics::Diagnostic::error(
            "test.ts".to_string(),
            0,
            1,
            "test".to_string(),
            0,
        ));
        ctx.emitted_diagnostics.insert((0, 1));
        ctx.instantiation_depth.set(7);

        assert_eq!(ctx.diagnostics.len(), 1);
        assert_eq!(ctx.emitted_diagnostics.len(), 1);
        assert_eq!(ctx.instantiation_depth.get(), 7);

        ctx.reset_for_next_file();

        assert!(ctx.diagnostics.is_empty());
        assert!(ctx.emitted_diagnostics.is_empty());
        assert_eq!(ctx.instantiation_depth.get(), 0);
    }

    #[test]
    fn reset_is_idempotent() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut ctx = fresh_ctx(&arena, &binder, &types);

        ctx.reset_for_next_file();
        ctx.reset_for_next_file();

        assert!(ctx.diagnostics.is_empty());
        assert_eq!(ctx.instantiation_depth.get(), 0);
    }

    #[test]
    fn reset_clears_all_recursion_depth_counters() {
        // The reset helper resets five depth counters: four
        // `RefCell<DepthCounter>` (call/circ_ref/overlap/recursion) plus
        // one `Cell<u32>` (instantiation). The original "diagnostic
        // buffers" test only exercises `instantiation_depth`. This test
        // locks the semantics of the four RefCell-backed counters,
        // including the sticky `exceeded` flag that a careless future
        // refactor (e.g. clearing only `depth` and forgetting `exceeded`)
        // would silently break — and a non-cleared `exceeded` would
        // suppress legitimate TS2589-style depth errors in the next
        // file checked on the reused context.
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut ctx = fresh_ctx(&arena, &binder, &types);

        // Drive each counter past zero and set the sticky exceeded flag.
        for depth_cell in [
            &ctx.call_depth,
            &ctx.circ_ref_depth,
            &ctx.overlap_depth,
            &ctx.recursion_depth,
        ] {
            let mut d = depth_cell.borrow_mut();
            assert!(d.enter(), "enter should succeed under max_depth");
            assert!(d.enter(), "second enter should succeed");
            d.mark_exceeded();
            assert_eq!(d.depth(), 2);
            assert!(d.is_exceeded());
        }
        ctx.instantiation_depth.set(11);

        ctx.reset_for_next_file();

        for depth_cell in [
            &ctx.call_depth,
            &ctx.circ_ref_depth,
            &ctx.overlap_depth,
            &ctx.recursion_depth,
        ] {
            let d = depth_cell.borrow();
            assert_eq!(d.depth(), 0, "depth not cleared on reset");
            assert!(
                !d.is_exceeded(),
                "exceeded flag not cleared on reset — would silently \
                 suppress real depth errors in the next file",
            );
        }
        assert_eq!(ctx.instantiation_depth.get(), 0);
    }
}
