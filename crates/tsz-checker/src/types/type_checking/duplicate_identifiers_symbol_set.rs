//! Symbol-id collection for the per-file duplicate-identifier pass.
//!
//! Split out of `duplicate_identifiers.rs` so the main pass stays under the
//! checker size ceiling. The collection itself is on the hot path: it runs once
//! per file and, when libs are loaded, has to skip the ~2000+ merged lib symbols
//! that share the scope tables with user code.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::{ContainerKind, SymbolId};

impl<'a> CheckerState<'a> {
    /// Collect the user-code symbol ids whose declarations the duplicate-identifier
    /// pass must examine for the current file.
    ///
    /// When libs are loaded the scope tables also hold the merged lib symbols;
    /// processing all of them is a 40-50ms-per-file bottleneck, so we pre-build a
    /// set of user-code symbols from `node_symbols` and intersect it with the
    /// non-class scope symbols (preserving the Class-scope exclusion). The result
    /// is a subset of the user symbols, so it is pre-sized to that bound and never
    /// rehashes on growth (#11617).
    pub(super) fn collect_duplicate_check_symbol_ids(&self, has_libs: bool) -> FxHashSet<SymbolId> {
        if !has_libs {
            // No libs: scope tables / file_locals are already all user symbols.
            let mut result = FxHashSet::default();
            self.extend_scope_symbol_ids(&mut result, |_| true);
            return result;
        }

        let user_syms: FxHashSet<SymbolId> =
            self.ctx.binder.node_symbols.values().copied().collect();
        let mut result = FxHashSet::with_capacity_and_hasher(user_syms.len(), Default::default());
        self.extend_scope_symbol_ids(&mut result, |id| user_syms.contains(&id));
        result
    }

    /// Insert every non-class scope symbol (or file-local symbol when no scopes
    /// exist) that passes `keep` into `result`. Shared by the lib / no-lib paths
    /// of [`collect_duplicate_check_symbol_ids`] so the Class-scope exclusion and
    /// the scope/file-local fallback live in exactly one place.
    fn extend_scope_symbol_ids(
        &self,
        result: &mut FxHashSet<SymbolId>,
        keep: impl Fn(SymbolId) -> bool,
    ) {
        if !self.ctx.binder.scopes.is_empty() {
            for scope in self.ctx.binder.scopes.iter() {
                if scope.kind == ContainerKind::Class {
                    continue;
                }
                for (_, &id) in scope.table.iter() {
                    if keep(id) {
                        result.insert(id);
                    }
                }
            }
        } else {
            for (_, &id) in self.ctx.binder.file_locals.iter() {
                if keep(id) {
                    result.insert(id);
                }
            }
        }
    }
}
