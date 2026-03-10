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
            if !export_decl.export_clause.is_none() || !export_decl.module_specifier.is_some() {
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

        // Track: name -> first module specifier that claimed it
        let mut claimed: rustc_hash::FxHashMap<String, String> = rustc_hash::FxHashMap::default();

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
                if let Some(first_module) = claimed.get(name) {
                    // Only report when the collision comes from different source modules.
                    // Re-exporting from the same module twice (e.g., export type * from "./a";
                    // export * from "./a") is not a collision.
                    if first_module != module_spec {
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

            // Claim names for the first module that provides them
            for name in names {
                if name != "default" {
                    claimed.entry(name).or_insert_with(|| module_spec.clone());
                }
            }
        }
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
            for (name, _) in exports.iter() {
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
