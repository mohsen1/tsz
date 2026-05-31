//! Cached entry points for resolving a named export through a module's
//! `export =` target.
//!
//! The heavy resolution walk itself lives in
//! `resolve_named_export_via_export_equals_tracked_uncached` (in `module.rs`).
//! This module owns only the thin caching/cycle-guard wrappers so the deep
//! cross-file `export=` / re-export chains that dominate large-project type
//! checking are not re-walked at every nesting hop.

use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;

impl<'a> CheckerState<'a> {
    /// Resolve a named export through an `export =` target's members, e.g.
    /// `declare module "m" { namespace e { interface X {} } export = e }` where
    /// `import { X } from "m"` resolves via the export-assignment target.
    pub(crate) fn resolve_named_export_via_export_equals(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let mut visited = AliasCycleTracker::new();
        self.resolve_named_export_via_export_equals_tracked(
            module_specifier,
            export_name,
            &mut visited,
        )
    }

    /// Cycle-aware variant of [`resolve_named_export_via_export_equals`] that
    /// shares the caller's `visited_aliases` set with
    /// [`Self::resolve_alias_symbol`], preserving cycle tracking across the
    /// mutual-recursion boundary. Callers already inside alias resolution must
    /// use this variant.
    pub(crate) fn resolve_named_export_via_export_equals_tracked(
        &self,
        module_specifier: &str,
        export_name: &str,
        visited_aliases: &mut AliasCycleTracker,
    ) -> Option<tsz_binder::SymbolId> {
        let cache_key = (
            self.ctx.current_file_idx,
            module_specifier.to_string(),
            export_name.to_string(),
        );
        // `Some` is a context-independent true positive (always cacheable). A
        // `None` is only *written* when the walk observed no cycle collision
        // (write gate below), so every stored entry is cycle-independent and
        // safe to return at any depth. The collision counter generalizes the
        // old `visited_aliases.len() == 0` top-of-chain gate to any acyclic
        // sub-walk.
        let cache_enabled =
            !crate::types_domain::queries::lib_aliases::alias_resolution_cache_disabled();
        if cache_enabled
            && let Some(cached) = self
                .ctx
                .export_equals_named_cache
                .borrow()
                .get(&cache_key)
                .copied()
        {
            return cached;
        }

        // Re-entrancy guard: a re-entrant lookup of the same
        // `(file, module, export)` key is an `export=` cycle the symbol-keyed
        // `AliasCycleTracker` cannot see (it hops across specifiers/export
        // names, not just symbols). Break it like an alias cycle (`None`) and
        // record the collision so neither cache stores the truncation. Skipped
        // when the cache is disabled, preserving legacy recursive behavior.
        if cache_enabled {
            let inserted = self
                .ctx
                .export_equals_in_progress
                .borrow_mut()
                .insert(cache_key.clone());
            if !inserted {
                visited_aliases.record_cycle_collision();
                return None;
            }
        }

        let collisions_before = visited_aliases.collision_count();
        let resolved = stacker::maybe_grow(256 * 1024, 2 * 1024 * 1024, || {
            self.resolve_named_export_via_export_equals_tracked_uncached(
                module_specifier,
                export_name,
                visited_aliases,
            )
        });
        let cycle_independent = visited_aliases.collision_count() == collisions_before;

        if cache_enabled {
            self.ctx
                .export_equals_in_progress
                .borrow_mut()
                .remove(&cache_key);
            if resolved.is_some() || cycle_independent {
                self.ctx
                    .export_equals_named_cache
                    .borrow_mut()
                    .insert(cache_key, resolved);
            }
        }
        resolved
    }
}
