//! Helpers for detecting local exports renamed away from an imported name.

use crate::state::CheckerState;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;

/// Result of scanning a statement list for direct/renamed exports of a single
/// name. Tracked separately so callers can aggregate across multiple
/// ambient-module blocks and arenas before deciding whether to emit TS2460.
#[derive(Default, Clone)]
struct RenameScan {
    direct_export: bool,
    renamed_export: Option<String>,
}

impl<'a> CheckerState<'a> {
    /// Check if a declaration's name matches the expected string.
    pub(super) fn declaration_name_matches_string(
        arena: &NodeArena,
        decl_idx: NodeIndex,
        expected_name: &str,
    ) -> bool {
        let Some(node) = arena.get(decl_idx) else {
            return false;
        };

        let name_node_idx = match node.kind {
            syntax_kind_ext::VARIABLE_STATEMENT => {
                let Some(var_stmt) = arena.get_variable_at(decl_idx) else {
                    return false;
                };
                for &list_idx in &var_stmt.declarations.nodes {
                    let Some(list_node) = arena.get(list_idx) else {
                        continue;
                    };
                    let Some(list) = arena.get_variable(list_node) else {
                        continue;
                    };
                    for &decl_idx in &list.declarations.nodes {
                        if Self::declaration_name_matches_string(arena, decl_idx, expected_name) {
                            return true;
                        }
                    }
                }
                return false;
            }
            syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(var_decl) = arena.get_variable_declaration(node) {
                    var_decl.name
                } else {
                    return false;
                }
            }
            syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = arena.get_function(node) {
                    func.name
                } else {
                    return false;
                }
            }
            syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = arena.get_class(node) {
                    class.name
                } else {
                    return false;
                }
            }
            syntax_kind_ext::INTERFACE_DECLARATION => {
                if let Some(interface) = arena.get_interface(node) {
                    interface.name
                } else {
                    return false;
                }
            }
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(type_alias) = arena.get_type_alias(node) {
                    type_alias.name
                } else {
                    return false;
                }
            }
            syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = arena.get_enum(node) {
                    enum_decl.name
                } else {
                    return false;
                }
            }
            _ => return false,
        };

        let Some(name_node) = arena.get(name_node_idx) else {
            return false;
        };

        let Some(ident) = arena.get_identifier(name_node) else {
            return false;
        };

        arena.resolve_identifier_text(ident) == expected_name
    }

    pub(super) fn local_named_export_alias_for_import(
        &self,
        arena: &NodeArena,
        import_name: &str,
    ) -> Option<String> {
        let source_file = arena.source_files.first()?;
        let scan = Self::scan_statements_for_renamed_export(
            arena,
            &source_file.statements.nodes,
            import_name,
            /*implicit_export_of_declarations=*/ false,
        );
        if scan.direct_export {
            None
        } else {
            scan.renamed_export
        }
    }

    /// Walk an explicit statement list looking for a direct or renamed export
    /// of `import_name`.
    ///
    /// `implicit_export_of_declarations` controls whether *unmodified*
    /// declarations count as direct exports. Set `true` for an ambient module
    /// body that has no external module indicator (no `import` / `export *` /
    /// `export = ` / `export { ... }` / namespace-export-declaration), so that
    /// tsc's "all declarations are implicitly exported" semantics are mirrored.
    ///
    /// Shared between the file-module path (top-level source-file statements)
    /// and the ambient-module path (statements inside a `declare module "X"
    /// { ... }` block) so both surfaces enforce identical rename detection.
    fn scan_statements_for_renamed_export(
        arena: &NodeArena,
        statements: &[NodeIndex],
        import_name: &str,
        implicit_export_of_declarations: bool,
    ) -> RenameScan {
        let mut out = RenameScan::default();

        for &stmt_idx in statements {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            let has_export_modifier = arena
                .get_declaration_modifiers(stmt_node)
                .is_some_and(|mods| arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword));
            let name_matches = Self::declaration_name_matches_string(arena, stmt_idx, import_name);
            // Direct export of `import_name` either via explicit `export` modifier
            // or via the ambient-module implicit-export rule (when the enclosing
            // body has no external module indicator).
            if name_matches && (has_export_modifier || implicit_export_of_declarations) {
                out.direct_export = true;
                continue;
            }
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export_decl.module_specifier.is_some() || export_decl.export_clause.is_none() {
                continue;
            }
            // Skip type-only export declarations (`export type { ... }`).
            // These don't create value exports, so they shouldn't trigger TS2460.
            let decl_is_type_only = export_decl.is_type_only;
            let Some(clause_node) = arena.get(export_decl.export_clause) else {
                continue;
            };
            if arena.get_named_imports(clause_node).is_none() {
                if !decl_is_type_only
                    && Self::declaration_name_matches_string(
                        arena,
                        export_decl.export_clause,
                        import_name,
                    )
                {
                    out.direct_export = true;
                }
                continue;
            }
            let Some(named_exports) = arena.get_named_imports(clause_node) else {
                continue;
            };

            for &spec_idx in &named_exports.elements.nodes {
                let Some(spec_node) = arena.get(spec_idx) else {
                    continue;
                };
                let Some(specifier) = arena.get_specifier(spec_node) else {
                    continue;
                };
                // Skip type-only specifiers (`export { type X as Y }`).
                if decl_is_type_only || specifier.is_type_only {
                    continue;
                }

                let original_name_idx = if specifier.property_name.is_none() {
                    specifier.name
                } else {
                    specifier.property_name
                };
                let exported_name_idx = if specifier.name.is_none() {
                    original_name_idx
                } else {
                    specifier.name
                };

                let Some(original_name_node) = arena.get(original_name_idx) else {
                    continue;
                };
                let Some(original_ident) = arena.get_identifier(original_name_node) else {
                    continue;
                };
                if arena.resolve_identifier_text(original_ident) != import_name {
                    continue;
                }

                let Some(exported_name_node) = arena.get(exported_name_idx) else {
                    continue;
                };
                let Some(exported_ident) = arena.get_identifier(exported_name_node) else {
                    continue;
                };
                let exported_name = arena.resolve_identifier_text(exported_ident);

                if exported_name == import_name {
                    out.direct_export = true;
                } else if out.renamed_export.is_none() {
                    out.renamed_export = Some(exported_name.to_string());
                }
            }
        }

        out
    }

    /// Detect whether an ambient-module body acts as an "external module"
    /// (has at least one import/export/export-assignment statement). When
    /// this is true, declarations need explicit `export` modifiers to be
    /// part of the module's public surface; otherwise tsc treats every
    /// declaration as implicitly exported.
    fn ambient_module_body_has_external_indicator(arena: &NodeArena, stmts: &[NodeIndex]) -> bool {
        for &stmt_idx in stmts {
            let Some(node) = arena.get(stmt_idx) else {
                continue;
            };
            match node.kind {
                syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::EXPORT_ASSIGNMENT
                | syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION => return true,
                _ => {}
            }
        }
        false
    }

    /// Aggregate direct/renamed export information across every
    /// `declare module "<module_name>" { ... }` block in `arena`. The
    /// aggregate matters because the same ambient module is often augmented
    /// in multiple blocks: a rename in one block must not produce TS2460
    /// when another block directly exports the same name (via explicit
    /// `export` modifier or via the implicit-export rule for bodies without
    /// an external module indicator).
    fn scan_ambient_module_for_renamed_export(
        arena: &NodeArena,
        module_name: &str,
        import_name: &str,
    ) -> RenameScan {
        let normalized = module_name.trim_matches('"').trim_matches('\'');
        let mut aggregate = RenameScan::default();
        let Some(source_file) = arena.source_files.first() else {
            return aggregate;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            let Some(module_decl) = arena.get_module(stmt_node) else {
                continue;
            };
            let Some(name_node) = arena.get(module_decl.name) else {
                continue;
            };
            // Ambient modules have a string-literal name; namespace declarations
            // (identifier names) are not module imports.
            let Some(lit) = arena.get_literal(name_node) else {
                continue;
            };
            if lit.text != module_name && lit.text != normalized {
                continue;
            }
            let Some(body_node) = arena.get(module_decl.body) else {
                continue;
            };
            let Some(block) = arena.get_module_block(body_node) else {
                continue;
            };
            let Some(stmts) = block.statements.as_ref() else {
                continue;
            };
            let has_indicator =
                Self::ambient_module_body_has_external_indicator(arena, &stmts.nodes);
            let scan = Self::scan_statements_for_renamed_export(
                arena,
                &stmts.nodes,
                import_name,
                /*implicit_export_of_declarations=*/ !has_indicator,
            );
            if scan.direct_export {
                aggregate.direct_export = true;
            }
            if aggregate.renamed_export.is_none() {
                aggregate.renamed_export = scan.renamed_export;
            }
        }
        aggregate
    }

    pub(super) fn local_named_export_alias_for_module(
        &self,
        module_name: &str,
        import_name: &str,
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
    ) -> Option<String> {
        let target_idx = if let Some(mode) = resolution_mode {
            self.ctx.resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_name,
                Some(mode),
            )
        } else {
            self.ctx.resolve_import_target(module_name)
        };

        // File-module path: when the specifier resolves to a concrete file,
        // scan that file's top-level statements first (same behaviour as
        // before this fix — preserves the wildcard-reexport suppression).
        if let Some(target_idx) = target_idx {
            let arena = self.ctx.get_arena_for_file(target_idx as u32);
            if let Some(renamed) = self.local_named_export_alias_for_import(arena, import_name) {
                // When the target module also re-exports `import_name` via
                // `export * from "..."`, the original name is a valid export
                // alongside the renamed alias. Suppress TS2460 in that case —
                // both names are valid import targets, matching tsc behaviour.
                if !self.is_exported_via_wildcard_reexport(target_idx, import_name) {
                    return Some(renamed);
                }
            }
        }

        // Ambient-module path: aggregate direct + renamed scans across every
        // `declare module "<module_name>" { ... }` block in every loaded
        // arena. A direct export in any block trumps a rename anywhere else,
        // matching tsc's behaviour for augmented ambient modules.
        let mut aggregate = RenameScan::default();
        if let Some(target_idx) = target_idx {
            let arena = self.ctx.get_arena_for_file(target_idx as u32);
            let scan =
                Self::scan_ambient_module_for_renamed_export(arena, module_name, import_name);
            if scan.direct_export {
                aggregate.direct_export = true;
            }
            if aggregate.renamed_export.is_none() {
                aggregate.renamed_export = scan.renamed_export;
            }
        }
        if let Some(all_arenas) = self.ctx.all_arenas.as_ref() {
            for arena in all_arenas.iter() {
                if aggregate.direct_export {
                    // Direct export wins — short-circuit further scans.
                    break;
                }
                let scan = Self::scan_ambient_module_for_renamed_export(
                    arena.as_ref(),
                    module_name,
                    import_name,
                );
                if scan.direct_export {
                    aggregate.direct_export = true;
                }
                if aggregate.renamed_export.is_none() {
                    aggregate.renamed_export = scan.renamed_export;
                }
            }
        }

        if aggregate.direct_export {
            None
        } else {
            aggregate.renamed_export
        }
    }

    /// Returns true when any `export * from "..."` in the file at `file_idx` directly
    /// exports `export_name` (checked one level deep, i.e. the immediate star sources).
    pub(super) fn is_exported_via_wildcard_reexport(
        &self,
        file_idx: usize,
        export_name: &str,
    ) -> bool {
        let Some(file_binder) = self.ctx.get_binder_for_file(file_idx) else {
            return false;
        };
        let file_arena = self.ctx.get_arena_for_file(file_idx as u32);
        let Some(file_name) = file_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.as_str())
        else {
            return false;
        };
        let Some(wildcard_sources) = self.ctx.wildcard_reexports_for_file(file_binder, file_name)
        else {
            return false;
        };
        for source_module in wildcard_sources {
            let Some(source_idx) = self
                .ctx
                .resolve_import_target_from_file(file_idx, source_module)
            else {
                continue;
            };
            let Some(source_binder) = self.ctx.get_binder_for_file(source_idx) else {
                continue;
            };
            let source_arena = self.ctx.get_arena_for_file(source_idx as u32);
            let Some(source_file_name) = source_arena
                .source_files
                .first()
                .map(|sf| sf.file_name.as_str())
            else {
                continue;
            };
            if let Some(exports) = self
                .ctx
                .module_exports_for_module(source_binder, source_file_name)
                && exports.has(export_name)
            {
                return true;
            }
        }
        false
    }
}
