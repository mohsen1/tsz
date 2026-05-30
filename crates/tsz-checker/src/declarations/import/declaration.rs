//! Import declaration validation (`import { X } from "y"`), re-export chain
//! cycle detection, and import resolution helpers.
//!
//! Import-equals validation (`import X = require("y")` / `import X = Namespace`)
//! lives in the sibling `equals` module.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

pub(crate) use super::declaration_helpers::{should_rewrite_module_specifier, ts_extension_suffix};

impl<'a> CheckerState<'a> {
    pub(crate) fn source_file_has_syntactic_module_indicator(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            match stmt.kind {
                syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                | syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    return true;
                }
                _ => {}
            }
        }

        false
    }

    pub(crate) fn source_file_has_top_level_global_augmentation(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            if !stmt.is_global_augmentation() {
                continue;
            }
            let Some(module) = arena.get_module(stmt) else {
                continue;
            };
            let Some(name_node) = arena.get(module.name) else {
                continue;
            };
            if name_node.kind == SyntaxKind::GlobalKeyword as u16
                || arena
                    .get_identifier(name_node)
                    .is_some_and(|ident| ident.escaped_text == "global")
            {
                return true;
            }
        }

        false
    }

    /// Check if a source file contains a module augmentation (not global augmentation).
    /// A module augmentation is a `declare module "X" { ... }` statement that extends
    /// an existing module's type definitions. Files with only module augmentations
    /// (and no regular exports) should not trigger TS2307 because they serve a valid
    /// purpose in the type system.
    pub(crate) fn source_file_has_module_augmentation(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            // Must NOT be a global augmentation
            if stmt.is_global_augmentation() {
                continue;
            }
            let Some(module) = arena.get_module(stmt) else {
                continue;
            };
            let Some(name_node) = arena.get(module.name) else {
                continue;
            };
            // Module augmentation has a string literal name (not "global" keyword)
            if name_node.kind == SyntaxKind::StringLiteral as u16 {
                return true;
            }
        }

        false
    }

    // =========================================================================
    // Import Declaration Validation
    // =========================================================================

    pub(crate) fn maybe_emit_imported_global_augmentation_errors(&mut self, target_idx: usize) {
        let arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return;
        };
        if self.source_file_has_syntactic_module_indicator(arena, source_file) {
            return;
        }
        // Collect positions first to avoid borrowing arena and self simultaneously
        let mut error_positions: Vec<(u32, u32)> = Vec::new();
        let file_name = source_file.file_name.clone();

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            if !stmt.is_global_augmentation() {
                continue;
            }
            let Some(module) = arena.get_module(stmt) else {
                continue;
            };
            let Some(name_node) = arena.get(module.name) else {
                continue;
            };
            let is_global = name_node.kind == SyntaxKind::GlobalKeyword as u16
                || arena
                    .get_identifier(name_node)
                    .is_some_and(|ident| ident.escaped_text == "global");
            if !is_global {
                continue;
            }

            error_positions.push((name_node.pos, name_node.end.saturating_sub(name_node.pos)));
        }

        for (start, length) in error_positions {
            self.error_at_position_in_file(
                file_name.clone(),
                start,
                length,
                diagnostic_messages::AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_CAN_ONLY_BE_DIRECTLY_NESTED_IN_EXTERNAL_MODUL,
                diagnostic_codes::AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_CAN_ONLY_BE_DIRECTLY_NESTED_IN_EXTERNAL_MODUL,
            );
        }
    }

    /// TS1214: Check import binding names for strict-mode reserved words.
    /// Import declarations make the file a module (always strict mode), so TS1214 applies.
    /// Matches tsc's binder: `checkContextualIdentifier` is guarded by
    /// `!file.parseDiagnostics.length`, so strict-mode checks are skipped
    /// entirely when the file has any parser errors.
    pub(crate) fn check_import_binding_reserved_words(&mut self, import_clause_idx: NodeIndex) {
        // Skip when there are parser errors (matches tsc binder behavior)
        if self.ctx.has_parse_errors {
            return;
        }

        use crate::state_checking::is_strict_mode_reserved_name;

        let Some(clause_node) = self.ctx.arena.get(import_clause_idx) else {
            return;
        };
        let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
            return;
        };

        // Check default import name: `import package from "./mod"`
        if clause.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(clause.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && is_strict_mode_reserved_name(&ident.escaped_text)
        {
            self.emit_module_strict_mode_reserved_word_error(clause.name, &ident.escaped_text);
        }

        // Check named bindings (namespace import or named imports)
        if clause.named_bindings.is_none() {
            return;
        }
        let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings) else {
            return;
        };

        if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
            // `import * as package from "./mod"` — check the alias name
            if let Some(ns_data) = self.ctx.arena.get_named_imports(bindings_node)
                && ns_data.name.is_some()
                && let Some(name_node) = self.ctx.arena.get(ns_data.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                && is_strict_mode_reserved_name(&ident.escaped_text)
            {
                self.emit_module_strict_mode_reserved_word_error(ns_data.name, &ident.escaped_text);
            }
        } else if bindings_node.kind == syntax_kind_ext::NAMED_IMPORTS {
            // `import { foo as package } from "./mod"` — check each specifier's local name
            if let Some(named_data) = self.ctx.arena.get_named_imports(bindings_node) {
                let elements: Vec<_> = named_data.elements.nodes.to_vec();
                for elem_idx in elements {
                    let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                        continue;
                    };
                    let Some(spec) = self.ctx.arena.get_specifier(elem_node) else {
                        continue;
                    };
                    // The local binding name is `spec.name`
                    let name_to_check = spec.name;
                    if let Some(name_node) = self.ctx.arena.get(name_to_check)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                        && is_strict_mode_reserved_name(&ident.escaped_text)
                    {
                        self.emit_module_strict_mode_reserved_word_error(
                            name_to_check,
                            &ident.escaped_text,
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::declaration_resolution::path_has_node_modules_segment;
    use super::ts_extension_suffix;
    use crate::context::{CheckerOptions, ScriptTarget};
    use crate::module_resolution::build_module_resolution_maps;
    use crate::state::CheckerState;
    use std::sync::Arc;
    use tsz_binder::BinderState;
    use tsz_common::common::ModuleKind;
    use tsz_parser::parser::ParserState;
    use tsz_solver::construction::TypeInterner;

    #[test]
    fn ts_extension_detects_ts() {
        assert_eq!(ts_extension_suffix("./foo.ts"), Some(".ts"));
    }

    #[test]
    fn ts_extension_detects_tsx() {
        assert_eq!(ts_extension_suffix("./foo.tsx"), Some(".tsx"));
    }

    #[test]
    fn ts_extension_detects_mts() {
        assert_eq!(ts_extension_suffix("./foo.mts"), Some(".mts"));
    }

    #[test]
    fn ts_extension_detects_cts() {
        assert_eq!(ts_extension_suffix("./foo.cts"), Some(".cts"));
    }

    #[test]
    fn import_external_library_check_uses_node_modules_path_segment() {
        assert!(path_has_node_modules_segment(
            "/repo/node_modules/pkg/index.d.ts"
        ));
        assert!(path_has_node_modules_segment(
            r"C:\repo\node_modules\pkg\index.d.ts"
        ));
        assert!(path_has_node_modules_segment(
            "/repo/packages/app/node_modules/pkg/index.d.ts"
        ));

        assert!(!path_has_node_modules_segment(
            "/repo/fixtures/node_modules_pkg/index.d.ts"
        ));
        assert!(!path_has_node_modules_segment(
            "/repo/fixtures/not_node_modules/index.d.ts"
        ));
    }

    #[test]
    fn ts_extension_ignores_dts() {
        assert_eq!(ts_extension_suffix("./foo.d.ts"), None);
    }

    #[test]
    fn ts_extension_ignores_d_mts() {
        assert_eq!(ts_extension_suffix("./foo.d.mts"), None);
    }

    #[test]
    fn ts_extension_ignores_d_cts() {
        assert_eq!(ts_extension_suffix("./foo.d.cts"), None);
    }

    #[test]
    fn ts_extension_ignores_js() {
        assert_eq!(ts_extension_suffix("./foo.js"), None);
    }

    #[test]
    fn ts_extension_ignores_no_ext() {
        assert_eq!(ts_extension_suffix("./foo"), None);
    }

    #[test]
    fn ts_extension_ignores_json() {
        assert_eq!(ts_extension_suffix("./data.json"), None);
    }

    fn import_binding_is_type_only_for_named_files(
        files: &[(&str, &str)],
        entry_file: &str,
        module_name: &str,
        import_name: &str,
    ) -> bool {
        let mut arenas = Vec::with_capacity(files.len());
        let mut binders = Vec::with_capacity(files.len());
        let mut roots = Vec::with_capacity(files.len());
        let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

        for (name, source) in files {
            let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
            let root = parser.parse_source_file();
            let mut binder = BinderState::new();
            binder.bind_source_file(parser.get_arena(), root);
            arenas.push(Arc::new(parser.get_arena().clone()));
            binders.push(Arc::new(binder));
            roots.push(root);
        }

        let entry_idx = file_names
            .iter()
            .position(|name| name == entry_file)
            .expect("entry file should exist");
        let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);

        let all_arenas = Arc::new(arenas);
        let all_binders = Arc::new(binders);
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            all_arenas[entry_idx].as_ref(),
            all_binders[entry_idx].as_ref(),
            &types,
            file_names[entry_idx].clone(),
            CheckerOptions {
                allow_js: true,
                check_js: true,
                target: ScriptTarget::ES2015,
                module: ModuleKind::ES2020,
                ..CheckerOptions::default()
            },
        );

        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(entry_idx);
        checker.ctx.set_lib_contexts(Vec::new());
        checker
            .ctx
            .set_resolved_module_paths(Arc::new(resolved_module_paths));
        checker.ctx.set_resolved_modules(resolved_modules);

        checker.check_source_file(roots[entry_idx]);
        checker.is_import_specifier_type_only(module_name, import_name)
            || checker.is_export_type_only_across_binders(module_name, import_name)
            || (import_name == "default" && checker.is_module_export_equals_type_only(module_name))
    }

    #[test]
    fn import_binding_is_type_only_detects_exported_interface() {
        assert!(import_binding_is_type_only_for_named_files(
            &[
                (
                    "mod.d.ts",
                    r#"
export interface WriteFileOptions {}
export function writeFile(path: string, data: any, options: WriteFileOptions, callback: (err: Error) => void): void;
                    "#,
                ),
                (
                    "index.js",
                    r#"
import { writeFile, WriteFileOptions } from "./mod";
writeFile("", "", /** @type {WriteFileOptions} */ ({}), () => {});
                    "#,
                ),
            ],
            "index.js",
            "./mod",
            "WriteFileOptions",
        ));
    }

    #[test]
    fn import_binding_is_type_only_detects_default_interface_export() {
        assert!(import_binding_is_type_only_for_named_files(
            &[
                (
                    "dep.d.ts",
                    r#"
export default interface TruffleContract {
  foo: number;
}
                    "#,
                ),
                (
                    "caller.js",
                    r#"
import TruffleContract from "./dep";
                    "#,
                ),
            ],
            "caller.js",
            "./dep",
            "default",
        ));
    }
}
