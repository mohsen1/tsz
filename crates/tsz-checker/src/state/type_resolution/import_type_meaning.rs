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
        // A JS module has no `export =`; `module.exports = class {}` is how it
        // supplies a type for a bare `import('./m')`.
        if self.commonjs_whole_module_export_assigns_a_class(
            module_name,
            Some(self.ctx.current_file_idx),
        ) {
            return true;
        }

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

    /// Whether a bare `typeof import("mod")` names a VALUE.
    ///
    /// A module with no `export =` always has runtime value — the module
    /// object itself is the value, regardless of what it exports — so this
    /// only ever returns `false` when the module's `export =` target is
    /// itself type-only (an interface, a type alias, or an uninstantiated
    /// namespace). `tsc` reports TS1339 in that case.
    /// `import("mod").Member` is unaffected: it is not a bare typeof import.
    pub(crate) fn bare_typeof_import_names_a_value(
        &self,
        module_name: &str,
        resolution_mode_override: Option<crate::context::ResolutionModeOverride>,
    ) -> bool {
        use tsz_binder::symbol_flags;

        let target_idx = self
            .ctx
            .resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_name,
                resolution_mode_override,
            )
            .or_else(|| self.ctx.resolve_import_target(module_name));

        if let Some(target_idx) = target_idx {
            // File-backed `export =`: reuse the same companion-aware
            // type-only determination the sibling TS1340 check uses via
            // `is_module_export_equals_type_only`. The `export =` table
            // entry is a proxy symbol whose own flags do not reliably carry
            // the target's value-ness (e.g. `export class C {} export = C;`
            // records the proxy as `TYPE_ALIAS`-flagged); the real answer
            // comes from that helper's sibling value-declaration lookup.
            //
            // That helper only recognizes `INTERFACE`/`TYPE_ALIAS` targets
            // as type-only, so an `export =`-ed namespace whose members are
            // all types (a `NAMESPACE_MODULE` that never instantiates a
            // runtime object) slips through it uncaught; check that
            // separately here.
            //
            // Must go through `module_exports_for_module` (not index
            // `binder.module_exports` directly): a full project build
            // aggregates exports into `ctx.program_module_exports` and
            // leaves each per-file binder's own `module_exports` map
            // unpopulated, so a direct index only ever succeeds in
            // single/few-file harnesses that skip that aggregation step.
            let namespace_is_uninstantiated =
                self.ctx
                    .get_binder_for_file(target_idx)
                    .and_then(|binder| {
                        let file_name = self
                            .ctx
                            .get_arena_for_file(target_idx as u32)
                            .source_files
                            .first()?
                            .file_name
                            .as_str();
                        let eq_sym_id = self
                            .ctx
                            .module_exports_for_module(binder, file_name)
                            .and_then(|exports| exports.get("export="))?;
                        Some((binder, eq_sym_id))
                    })
                    .is_some_and(|(binder, eq_sym_id)| {
                        binder.get_symbol(eq_sym_id).is_some_and(|eq_sym| {
                            eq_sym.has_any_flags(symbol_flags::NAMESPACE_MODULE)
                        }) && self.is_module_uninstantiated_in_binder(binder, eq_sym_id)
                    });

            return !namespace_is_uninstantiated
                && !self.is_module_export_equals_type_only(module_name);
        }

        // Ambient `declare module "mod" { ... export = X }`: no target file
        // to resolve through, so fall back to a direct flag check on the
        // `export =` symbol — the same simpler check the sibling
        // `bare_import_type_names_a_type` ambient branch uses.
        const VALUE_PROVIDING: u32 = symbol_flags::VARIABLE
            | symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::ENUM
            | symbol_flags::ENUM_MEMBER
            | symbol_flags::VALUE_MODULE;

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

        let Some(sym_id) = ambient_export_equals_sym else {
            // No `export =`: an ordinary module object always has value.
            return true;
        };

        let symbol_is_value = |checker: &Self, sym_id: tsz_binder::SymbolId| {
            checker
                .ctx
                .binder
                .get_symbol_with_libs(sym_id, &lib_binders)
                .is_some_and(|sym| !sym.is_type_only && sym.has_any_flags(VALUE_PROVIDING))
        };

        if symbol_is_value(self, sym_id) {
            return true;
        }

        let mut visited = AliasCycleTracker::new();
        self.resolve_alias_symbol(sym_id, &mut visited)
            .is_some_and(|resolved| symbol_is_value(self, resolved))
    }
}
