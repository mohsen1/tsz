//! Relation-input ref/heritage closure resolution (`ensure_refs_resolved`).
//!
//! Before a relation runs, its inputs' `Lazy`/`Ref`/type-query references must
//! be resolved into the type environment. This walks the transitive closure of
//! a relation input and resolves every reference it reaches. Split out of
//! `assignability_checker` so each file stays within the checker LOC budget.
//!
//! Two complementary levers keep this off the relation-heavy hot path:
//!
//! - **On-demand forcing (#12101):** a force-eligible simple lib interface's
//!   tail (members, heritage bases) is left `Lazy` and materialized on demand
//!   at the consuming `resolve_lazy` miss, so its body is not pushed back onto
//!   the worklist — dropping the eager DOM/webworker heritage pre-walk.
//! - **Lib-pure closure reuse (#13936):** a closure that completes without
//!   exhausting either fuel budget and touches only builtin-lib defs is
//!   recorded in [`crate::context::CheckerContext::refs_resolved`]; later
//!   traversals skip re-descending into it. Builtin-lib types resolve
//!   identically in every arena/requester context, so "resolved once" is
//!   "resolved for everyone"; user types can resolve per requester under
//!   cross-arena/`CommonJS` delegation, so any closure touching a user def
//!   keeps re-walking. See `RefsResolutionCache` for the soundness contract.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Ensure all Lazy/Ref types in a type are resolved into the type environment.
    pub(crate) fn ensure_refs_resolved(&mut self, type_id: TypeId) {
        use crate::state_checking::lazy_lib_member::on_demand_forcing_disabled;
        use crate::state_domain::type_environment::lazy::{
            enter_refs_resolution_scope, exit_refs_resolution_scope,
            global_resolution_fuel_exhausted, increment_global_resolution_fuel,
            increment_refs_resolution_fuel, refs_resolution_fuel_exhausted,
        };

        if self.ctx.refs_resolved.contains_entry_or_closure(type_id) {
            return;
        }

        // Default: on-demand forcing. The legacy eager transitive pre-walk is
        // only used when the kill-switch is set, for byte-parity comparison.
        let transitive = on_demand_forcing_disabled();

        let is_outermost = enter_refs_resolution_scope();

        let mut visited_types = FxHashSet::default();
        let mut visited_def_ids = FxHashSet::default();
        let mut worklist = vec![type_id];

        // Whether the traversal both (a) reached only builtin-lib entities and
        // (b) fully walked them — no tail was deferred by on-demand forcing.
        // Only a closure that is *both* lib-pure and fully walked is safe to
        // record for reuse: lib types resolve identically in every
        // arena/requester context (#13936 / #12144), and "fully walked" means
        // every transitively reachable ref is already in the environment, so a
        // later skip-descent cannot under-resolve a consumer (e.g. JSX
        // excess-property checking). When on-demand forcing (#14016) defers a
        // force-eligible lib tail, the closure is *not* fully walked, so it is
        // not recorded — skip-descent then never fires for it, leaving that
        // path byte-identical.
        let mut closure_lib_pure = true;
        let mut closure_fully_walked = true;

        while let Some(current) = worklist.pop() {
            if refs_resolution_fuel_exhausted() {
                break;
            }

            if !visited_types.insert(current) {
                continue;
            }

            // A prior traversal already fully walked this lib-pure type's
            // closure into the environment; re-walking it would only repeat
            // idempotent cache hits over the shared DOM/lib heritage graph that
            // dominates relation-heavy projects. Stop descending.
            if current != type_id && self.ctx.refs_resolved.closure_resolved(current) {
                continue;
            }

            let type_queries = self.ctx.collect_type_queries_cached(current);
            if !type_queries.is_empty() {
                // A `typeof X` query references a user value symbol → not lib-pure.
                closure_lib_pure = false;
            }
            for symbol_ref in type_queries.iter().copied() {
                let sym_id = tsz_binder::SymbolId(symbol_ref.0);
                let _ = self.get_type_of_symbol(sym_id);
                // Populate type_env with the VALUE type (constructor for classes) so that
                // TypeEvaluator::visit_type_query can resolve via TypeEnvironment::resolve_ref.
                // Without this, resolve_ref returns None and the fallback resolve_lazy returns
                // the INSTANCE type for classes, causing false TS2345 on `typeof ClassName` args.
                if let Some(value_type) = self.ctx.symbol_types.get(&sym_id)
                    && let Ok(mut env) = self.ctx.type_env.try_borrow_mut()
                {
                    env.insert(tsz_solver::SymbolRef(sym_id.0), value_type);
                }
            }

            for &def_id in self.ctx.collect_lazy_def_ids_cached(current).iter() {
                if refs_resolution_fuel_exhausted() {
                    break;
                }
                if !visited_def_ids.insert(def_id) {
                    continue;
                }
                // A non-lib def taints the closure (its resolution can be
                // requester/arena-specific). Once tainted the flag never flips
                // back, so skip the lookup entirely after the first taint.
                if closure_lib_pure && !self.ctx.definition_store.def_is_lib_resident(def_id) {
                    closure_lib_pure = false;
                }
                increment_refs_resolution_fuel();
                increment_global_resolution_fuel();
                let at_fuel_limit = global_resolution_fuel_exhausted();
                // Always call resolve_and_insert_def_type even when global fuel is
                // exhausted: the call is typically a fast cache hit for lib types that
                // were computed during type-environment building, and the resolver needs
                // the TypeEnvironment entry to evaluate a Lazy(def_id) during
                // assignability checks.  Without this, exhausted-fuel calls silently
                // leave subsequent DOM/lib type refs unresolvable, causing the relation
                // checker to treat unresolved Lazy types as compatible (issue #12144).
                // When at the fuel limit we still resolve the direct def_id but skip
                // adding its result to the worklist so transitive work stays bounded.
                //
                // On-demand forcing (#12101): when `def_id` is a force-eligible simple
                // lib interface, its referenced tail (members, heritage bases) is made
                // of lib refs that `CheckerContext::force_def_on_miss` materializes on
                // demand at the consuming `resolve_lazy` miss, so its body is NOT
                // pushed back onto the worklist — this is what drops the eager
                // DOM/webworker heritage-graph pre-walk. For every other def
                // (cross-file class/namespace, user types, generic/augmented lib
                // interfaces) the transitive push is preserved so its tail is
                // materialized exactly as the legacy eager path did, keeping
                // byte-parity. With the kill-switch set, `transitive` is always true
                // (legacy eager pre-walk).
                let push_tail = transitive || !self.force_eligible_lib_def(def_id);
                if let Some(result) = self.resolve_and_insert_def_type(def_id)
                    && result != TypeId::ERROR
                    && result != TypeId::ANY
                    && !at_fuel_limit
                {
                    if push_tail {
                        worklist.push(result);
                    } else {
                        // On-demand forcing deferred this force-eligible lib
                        // tail, so the closure is not fully walked here and must
                        // not be recorded for skip-descent reuse.
                        closure_fully_walked = false;
                    }
                }
                if at_fuel_limit {
                    break;
                }
            }
        }
        self.ctx.refs_resolved.mark_entered(type_id);

        // Record the closure for reuse only when it is lib-pure, fully walked
        // (no on-demand-deferred tail), and neither fuel budget was exhausted.
        // Those three conditions make `visited_types` an exactly-resolved,
        // context-independent closure, so a later skip-descent over it is
        // byte-identical. Any other case records nothing, leaving the
        // fuel-limited, user-type, and on-demand-deferred paths unchanged.
        if closure_lib_pure
            && closure_fully_walked
            && !refs_resolution_fuel_exhausted()
            && !global_resolution_fuel_exhausted()
        {
            self.ctx
                .refs_resolved
                .record_closures(visited_types.iter().copied());
        }

        if is_outermost {
            exit_refs_resolution_scope();
        }
    }
}
