//! Whether a bare `import("mod")` names a type.
//!
//! Split out of `import_type.rs` so the JSDoc import-type resolver can ask the
//! same question. The two resolvers reach it by entirely separate paths:
//! `let a: import("./m")` goes through `import_type.rs`, while
//! `/** @param {import("./m")} a */` goes through the JSDoc comment scan.

use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;

impl<'a> CheckerState<'a> {
    /// Whether a bare `import("mod")` used in a type position names a TYPE.
    ///
    /// A bare import type resolves to the module's `export =` type. The module's
    /// own namespace is not a type, so a module exporting only values — or one
    /// with named type exports and no `export =` — cannot be used this way, and
    /// `tsc` reports TS1340. `import("mod").Member` is unaffected: it is not a
    /// bare import type.
    pub(crate) fn bare_import_type_names_a_type(
        &self,
        module_name: &str,
        resolution_mode_override: Option<crate::context::ResolutionModeOverride>,
    ) -> bool {
        use tsz_binder::symbol_flags;

        /// Symbol kinds whose `export =` gives the module a type meaning.
        /// Classes and enums qualify: they declare a type as well as a value.
        const TYPE_PROVIDING: u32 = symbol_flags::INTERFACE
            | symbol_flags::TYPE_ALIAS
            | symbol_flags::CLASS
            | symbol_flags::ENUM;

        let lib_binders = self.get_lib_binders();
        let ambient_export_equals_sym = self
            .ctx
            .binder
            .module_exports
            .get(module_name)
            .and_then(|exports| exports.get("export="))
            .or_else(|| {
                self.ctx
                    .global_module_exports_index
                    .as_ref()
                    .and_then(|idx| idx.get(module_name))
                    .and_then(|inner| inner.get("export="))
                    .and_then(|entries| entries.first().map(|&(_file_idx, sym_id)| sym_id))
            });
        let file_export_equals = self
            .ctx
            .resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_name,
                resolution_mode_override,
            )
            .and_then(|target_idx| {
                self.ctx
                    .get_binder_for_file(target_idx)
                    .map(|binder| (target_idx, binder))
            })
            .and_then(|(target_idx, binder)| {
                let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
                let file_name = target_arena.source_files.first()?.file_name.as_str();
                let sym_id = binder
                    .module_exports
                    .get(file_name)
                    .and_then(|exports| exports.get("export="))?;
                Some(
                    binder
                        .get_symbol(sym_id)
                        .is_some_and(|sym| sym.is_type_only || sym.has_any_flags(TYPE_PROVIDING)),
                )
            });

        // The module is usable as a bare import type only when its `export =`
        // target actually supplies a TYPE. A class or enum does — it carries a
        // type meaning alongside its value meaning, so `class Conn {}
        // export = Conn` is a valid bare import type. A plain `var` or
        // `function` export does not, and tsc reports TS1340 for it.
        file_export_equals.unwrap_or(false)
            || self.is_module_export_equals_type_only(module_name)
            || ambient_export_equals_sym.is_some_and(|sym_id| {
                let symbol_is_type = |checker: &Self, sym_id: tsz_binder::SymbolId| {
                    checker
                        .ctx
                        .binder
                        .get_symbol_with_libs(sym_id, &lib_binders)
                        .is_some_and(|sym| sym.is_type_only || sym.has_any_flags(TYPE_PROVIDING))
                };

                if symbol_is_type(self, sym_id) {
                    return true;
                }

                let mut visited = AliasCycleTracker::new();
                self.resolve_alias_symbol(sym_id, &mut visited)
                    .is_some_and(|resolved| symbol_is_type(self, resolved))
            })
    }
}
