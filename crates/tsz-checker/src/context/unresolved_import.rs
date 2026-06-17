//! Context-level detection of references to *unresolved imported aliases*.
//!
//! When an `import { X } from "missing"` cannot resolve its module (its
//! `TS2307` was already reported), `tsc` types `X` — and anything reached
//! through it — as the permissive `error` type. Checker passes that would
//! otherwise cascade spurious structural diagnostics (`TS2536`/`TS2574`/…) off
//! the resulting poisoned shape consult these helpers to honor that contagion.
//!
//! Lives on `CheckerContext` (not `CheckerState`) so both the full state
//! surface and the type-node grammar checker — which holds only a
//! `CheckerContext` — share a single detector.

use tsz_solver::TypeId;

use crate::context::CheckerContext;

impl<'a> CheckerContext<'a> {
    /// Whether `type_id` (recursively) references an *unresolved imported alias*:
    /// a `Lazy(DefId)`/`UnresolvedTypeName` whose backing symbol is an `import`
    /// from a module that failed to resolve. Covers the
    /// `import { X } from "missing"` alias case via the symbol's `import_module`;
    /// the entity-name `import X = E.Member` variant is handled by the
    /// `CheckerState` resolver path and not needed here.
    pub fn type_references_unresolved_import(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::collect_all_types(self.types, type_id)
            .into_iter()
            .any(|ty| {
                crate::query_boundaries::common::lazy_def_id(self.types, ty)
                    .and_then(|def_id| self.def_to_symbol_id(def_id))
                    .is_some_and(|sym_id| self.is_unresolved_import_alias_symbol(sym_id))
                    || crate::query_boundaries::spread::unresolved_type_name_atom(self.types, ty)
                        .is_some()
            })
    }

    /// Whether `sym_id` is an `import` alias whose module cannot be resolved
    /// through any known channel (`module_exports`, ambient/shorthand modules,
    /// CLI-resolved modules, or package resolution). The module-spec import case
    /// of `CheckerState::is_unresolved_import_symbol_id`, lifted to the context
    /// so it is reachable without the full state surface.
    fn is_unresolved_import_alias_symbol(&self, sym_id: tsz_binder::SymbolId) -> bool {
        use tsz_binder::symbol_flags;
        let Some(symbol) = self.binder.get_symbol(sym_id) else {
            return false;
        };
        if !symbol.has_any_flags(symbol_flags::ALIAS) {
            return false;
        }
        let Some(module_name) = symbol.import_module() else {
            return false;
        };
        if self.module_exports_contains_module(self.binder, module_name) {
            return false;
        }
        if self.context_has_ambient_module(module_name) {
            // A shorthand ambient module (no exports) is treated as unresolved
            // (`any`); a declared ambient module with a body is resolved.
            return self.binder.shorthand_ambient_modules.contains(module_name)
                && !self.declared_modules_contains(self.binder, module_name);
        }
        if let Some(ref resolved) = self.resolved_modules
            && resolved.contains(module_name)
        {
            return false;
        }
        if self.resolve_import_target(module_name).is_some() {
            return false;
        }
        true
    }

    /// Whether `module_name` is declared as an ambient module in the current
    /// binder, the project-wide declared-module index, or any sibling binder.
    fn context_has_ambient_module(&self, module_name: &str) -> bool {
        let binder_ambient = |binder: &tsz_binder::BinderState| {
            binder.declared_modules.contains(module_name)
                || binder.shorthand_ambient_modules.contains(module_name)
        };
        if binder_ambient(self.binder) {
            return true;
        }
        if let Some(declared) = &self.global_declared_modules {
            let normalized = module_name.trim().trim_matches('"').trim_matches('\'');
            if declared.exact.contains(normalized) {
                return true;
            }
            return declared.matches_wildcard(module_name);
        }
        if let Some(binders) = &self.all_binders {
            return binders.iter().any(|binder| binder_ambient(binder));
        }
        false
    }
}
