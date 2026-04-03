//! Wildcard re-export collision detection (TS2308).

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// Check for conflicting wildcard re-exports in a file.
    ///
    /// When a file has multiple `export * from` / `export type * from` declarations
    /// that re-export the same name from different modules, emit TS2308:
    /// "Module {0} has already exported a member named '{1}'."
    ///
    /// Suppresses the error when both paths resolve to the same original binding
    /// (e.g., `export * from "./b"` and `export * from "./c"` where `b.ts` itself
    /// re-exports from `c.ts`).
    ///
    /// Example:
    /// ```ts
    /// export type * from "./a"; // exports A, B
    /// export * from "./b";      // exports B, C  → TS2308 for 'B'
    /// ```
    pub(crate) fn check_wildcard_reexport_collisions(&mut self, statements: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;

        // Collect wildcard re-export statements: (stmt_idx, module_specifier_text)
        let mut wildcards: Vec<(NodeIndex, String)> = Vec::new();

        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            let Some(export_decl) = self.ctx.arena.get_export_decl(node) else {
                continue;
            };
            // Wildcard re-export: no export clause + has module specifier
            if export_decl.export_clause.is_some() || !export_decl.module_specifier.is_some() {
                continue;
            }
            let Some(spec_node) = self.ctx.arena.get(export_decl.module_specifier) else {
                continue;
            };
            let Some(lit) = self.ctx.arena.get_literal(spec_node) else {
                continue;
            };
            wildcards.push((stmt_idx, lit.text.clone()));
        }

        if wildcards.len() < 2 {
            return;
        }

        // Track: name -> (first_module_specifier, origin_file_idx)
        let mut claimed: rustc_hash::FxHashMap<String, (String, usize)> =
            rustc_hash::FxHashMap::default();

        for (stmt_idx, module_spec) in &wildcards {
            let Some(source_idx) = self.ctx.resolve_import_target(module_spec) else {
                continue;
            };

            let mut visited = rustc_hash::FxHashSet::default();
            let names = self.collect_exported_names_for_collision_check(source_idx, &mut visited);

            for name in &names {
                if name == "default" {
                    continue; // export * doesn't forward default
                }
                if let Some((first_module, first_origin)) = claimed.get(name) {
                    // Only report when the collision comes from different source modules.
                    // Re-exporting from the same module twice (e.g., export type * from "./a";
                    // export * from "./a") is not a collision.
                    if first_module != module_spec {
                        // Trace the current path's origin — if both paths resolve to
                        // the same original file, the re-exports point at the same
                        // binding and tsc suppresses TS2308 ("same value" rule).
                        let mut cur_visited = rustc_hash::FxHashSet::default();
                        let cur_origin =
                            self.trace_export_origin(source_idx, name, &mut cur_visited);

                        if cur_origin == Some(*first_origin) {
                            continue;
                        }

                        let msg = format!(
                            "Module \"{first_module}\" has already exported a member named '{name}'. Consider explicitly re-exporting to resolve the ambiguity."
                        );
                        self.error_at_node(
                            *stmt_idx,
                            &msg,
                            diagnostic_codes::MODULE_HAS_ALREADY_EXPORTED_A_MEMBER_NAMED_CONSIDER_EXPLICITLY_RE_EXPORTING_TO_R,
                        );
                    }
                }
            }

            // Claim names for the first module that provides them, recording origin
            for name in names {
                if name != "default" {
                    claimed.entry(name.clone()).or_insert_with(|| {
                        let mut v = rustc_hash::FxHashSet::default();
                        let origin = self
                            .trace_export_origin(source_idx, &name, &mut v)
                            .unwrap_or(source_idx);
                        (module_spec.clone(), origin)
                    });
                }
            }
        }
    }

    /// Trace an exported name to its ultimate origin file, following alias chains.
    ///
    /// Unlike `resolve_export_in_file` (which returns the first hit from
    /// `module_exports`), this follows ALIAS symbols through their `import_module`
    /// chain so that `export { X } from './other'` resolves to `./other`'s origin.
    fn trace_export_origin(
        &self,
        file_idx: usize,
        export_name: &str,
        visited: &mut rustc_hash::FxHashSet<usize>,
    ) -> Option<usize> {
        if !visited.insert(file_idx) {
            return None; // Cycle detection
        }

        let target_binder = self.ctx.get_binder_for_file(file_idx)?;
        let target_arena = self.ctx.get_arena_for_file(file_idx as u32);
        let target_file_name = target_arena.source_files.first()?.file_name.clone();

        // Check direct exports (module_exports)
        if let Some(exports) = target_binder.module_exports.get(&target_file_name)
            && let Some(sym_id) = exports.get(export_name)
        {
            // If this is an alias, follow it to the source module
            if let Some(sym) = target_binder.symbols.get(sym_id)
                && (sym.flags & tsz_binder::symbol_flags::ALIAS) != 0
                && let Some(ref import_module) = sym.import_module
            {
                let import_name = sym.import_name.as_deref().unwrap_or(export_name);
                if let Some(source_idx) = self
                    .ctx
                    .resolve_import_target_from_file(file_idx, import_module)
                    && let Some(origin) = self.trace_export_origin(source_idx, import_name, visited)
                {
                    return Some(origin);
                }
            }
            // Not an alias, or couldn't follow — this file is the origin
            return Some(file_idx);
        }

        // Check named re-exports
        if let Some(reexports) = target_binder.reexports.get(&target_file_name)
            && let Some((source_module, original_name)) = reexports.get(export_name)
        {
            let name = original_name.as_deref().unwrap_or(export_name);
            if let Some(source_idx) = self
                .ctx
                .resolve_import_target_from_file(file_idx, source_module)
                && let Some(origin) = self.trace_export_origin(source_idx, name, visited)
            {
                return Some(origin);
            }
        }

        // Check wildcard re-exports
        if let Some(source_modules) = target_binder.wildcard_reexports.get(&target_file_name) {
            let source_modules = source_modules.clone();
            for source_module in &source_modules {
                if let Some(source_idx) = self
                    .ctx
                    .resolve_import_target_from_file(file_idx, source_module)
                    && let Some(origin) = self.trace_export_origin(source_idx, export_name, visited)
                {
                    return Some(origin);
                }
            }
        }

        // Check file_locals
        if target_binder.file_locals.get(export_name).is_some() {
            return Some(file_idx);
        }

        None
    }

    /// Collect all exported names from a file for wildcard re-export collision detection.
    /// Follows re-export chains recursively.
    fn collect_exported_names_for_collision_check(
        &self,
        file_idx: usize,
        visited: &mut rustc_hash::FxHashSet<usize>,
    ) -> rustc_hash::FxHashSet<String> {
        let mut names = rustc_hash::FxHashSet::default();

        if !visited.insert(file_idx) {
            return names;
        }

        let Some(binder) = self.ctx.get_binder_for_file(file_idx) else {
            return names;
        };
        let Some(file_name) = self
            .ctx
            .get_arena_for_file(file_idx as u32)
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
        else {
            return names;
        };

        // Direct exports from module_exports (populated during binding, pre-merge)
        if let Some(exports) = binder.module_exports.get(&file_name) {
            for (name, &sym_id) in exports.iter() {
                // Skip lib/global symbols merged from lib.d.ts.
                if binder.lib_symbol_ids.contains(&sym_id)
                    || binder
                        .symbols
                        .get(sym_id)
                        .is_some_and(|s| s.decl_file_idx == u32::MAX)
                {
                    continue;
                }
                names.insert(name.to_string());
            }
        }

        // Named re-exports (export { X } from './module')
        if let Some(reexports) = binder.reexports.get(&file_name) {
            for (name, _) in reexports.iter() {
                names.insert(name.to_string());
            }
        }

        // Wildcard re-exports (recursive — includes both value and type-only)
        if let Some(source_modules) = binder.wildcard_reexports.get(&file_name) {
            let source_modules = source_modules.clone();
            for source_module in &source_modules {
                if let Some(source_idx) = self
                    .ctx
                    .resolve_import_target_from_file(file_idx, source_module)
                {
                    let sub_names =
                        self.collect_exported_names_for_collision_check(source_idx, visited);
                    names.extend(sub_names);
                }
            }
        }

        names
    }
}
