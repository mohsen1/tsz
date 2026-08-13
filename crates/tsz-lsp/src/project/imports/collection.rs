//! Export-match collection: walking file ASTs to find matching export declarations.
//!
//! Entry-point orchestration (deciding which files to check and pushing
//! candidates through the sink) stays in `mod.rs`; this module owns the
//! per-file AST traversal helpers that `mod.rs` calls.

use std::path::Path;

use rustc_hash::FxHashSet;

use crate::code_actions::{ImportCandidate, ImportCandidateKind};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::{NodeArena, NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

use super::super::import_collect::{
    AutoImportCandidateContext, ImportCandidateCollectionMode, ImportCandidateSink,
};
use super::super::{ExportMatch, Project, ProjectFile};

impl Project {
    /// Iterate `files_to_check` and push every matching export into `sink`.
    ///
    /// Returns `true` when at least one candidate was added (used by the
    /// caller to decide whether a full-project fallback scan is needed).
    pub(super) fn collect_import_candidates_for_symbol_from_files(
        &self,
        files_to_check: Vec<String>,
        symbol_name: &str,
        mode: ImportCandidateCollectionMode,
        context: &mut AutoImportCandidateContext<'_>,
        sink: &mut ImportCandidateSink<'_>,
    ) -> bool {
        let before_len = sink.len();

        for file_name in files_to_check {
            if file_name == context.request_file_name() {
                continue;
            }

            self.collect_ambient_import_candidates_for_symbol(
                &file_name,
                symbol_name,
                context,
                sink,
            );

            if context.is_regular_file_excluded(&file_name) {
                continue;
            }

            if self.is_shadowed_by_ambiguous_package_import(context.request_file_name(), &file_name)
            {
                continue;
            }

            if !context.has_module_specifiers_for(self, &file_name) {
                continue;
            }

            let mut visited = FxHashSet::default();
            let matches = self.matching_exports_in_file(&file_name, symbol_name, &mut visited);
            if matches.is_empty() && !mode.include_namespace_default {
                continue;
            }

            let Some(module_specifier) = context.first_allowed_module_specifier(self, &file_name)
            else {
                continue;
            };
            let relative_fallback =
                context.ambiguous_relative_fallback_specifier(self, &file_name, &module_specifier);

            for export_match in &matches {
                let primary_is_new = sink.push(ImportCandidate {
                    module_specifier: module_specifier.clone(),
                    local_name: symbol_name.to_string(),
                    kind: export_match.kind.clone(),
                    is_type_only: export_match.is_type_only,
                });
                // Only the file that first claims the (shared) primary
                // specifier contributes the relative fallback. A later file
                // resolving to the same ambiguous bare specifier (e.g. both
                // `browser.ts` and `node.ts` matching `#is-browser`) is
                // already covered by the earlier file's fix and must not add
                // its own distinct fallback specifier.
                if primary_is_new && let Some(fallback) = relative_fallback.as_ref() {
                    sink.push(ImportCandidate {
                        module_specifier: fallback.clone(),
                        local_name: symbol_name.to_string(),
                        kind: export_match.kind.clone(),
                        is_type_only: export_match.is_type_only,
                    });
                }
            }

            if mode.include_namespace_default
                && let Some(is_type_only) = self.export_star_as_default_is_type_only(&file_name)
            {
                sink.push(ImportCandidate {
                    module_specifier,
                    local_name: symbol_name.to_string(),
                    kind: ImportCandidateKind::Default,
                    is_type_only,
                });
            }
        }

        sink.len() > before_len
    }

    pub(super) fn collect_ambient_import_candidates_for_symbol(
        &self,
        file_name: &str,
        symbol_name: &str,
        context: &mut AutoImportCandidateContext<'_>,
        sink: &mut ImportCandidateSink<'_>,
    ) {
        for (module_specifier, export_match) in
            self.matching_exports_in_ambient_modules(file_name, symbol_name)
        {
            if context.is_ambient_module_candidate_excluded(self, &module_specifier) {
                continue;
            }

            sink.push(ImportCandidate {
                module_specifier,
                local_name: symbol_name.to_string(),
                kind: export_match.kind.clone(),
                is_type_only: export_match.is_type_only,
            });
        }
    }

    /// Returns files that are likely to export `symbol_name`, based on the
    /// symbol index. Falls back to all files when the index is empty or has
    /// only self-references.
    pub(super) fn files_to_check_for_symbol(
        &self,
        symbol_name: &str,
        from_file_name: &str,
        all_files: &[String],
        wildcard_reexport_files: &[String],
    ) -> Vec<String> {
        let candidate_files = self.symbol_index.get_files_with_symbol(symbol_name);
        let has_external_candidates = candidate_files
            .iter()
            .any(|file_name| file_name != from_file_name);
        if candidate_files.is_empty() || !has_external_candidates {
            return all_files.to_vec();
        }

        let mut seen = FxHashSet::default();
        let mut files_to_check = Vec::new();

        for file_name in candidate_files
            .into_iter()
            .chain(wildcard_reexport_files.iter().cloned())
        {
            if seen.insert(file_name.clone()) {
                files_to_check.push(file_name);
            }
        }

        files_to_check
    }

    pub(super) fn file_has_wildcard_reexport(&self, file_name: &str) -> bool {
        self.files
            .get(file_name)
            .is_some_and(|f| f.has_wildcard_reexport)
    }

    /// Returns names re-exported by `file_name` whose text starts with `prefix`.
    pub(super) fn reexported_names_with_prefix(
        &self,
        file_name: &str,
        prefix: &str,
    ) -> Vec<String> {
        let Some(file) = self.files.get(file_name) else {
            return Vec::new();
        };
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return Vec::new();
        };

        let mut names = FxHashSet::default();

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };

            if stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                let Some(export_assign) = arena.get_export_assignment(stmt_node) else {
                    continue;
                };
                if export_assign.is_export_equals {
                    if let Some(expr_text) = arena.get_identifier_text(export_assign.expression)
                        && expr_text.starts_with(prefix)
                    {
                        names.insert(expr_text.to_string());
                    }
                } else if "default".starts_with(prefix) {
                    names.insert("default".to_string());
                }
                continue;
            }

            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = arena.get_export_decl(stmt_node) else {
                continue;
            };

            if export.is_default_export && "default".starts_with(prefix) {
                names.insert("default".to_string());
            }

            let clause_idx = export.export_clause;
            if !clause_idx.is_some() {
                continue;
            }
            let Some(clause_node) = arena.get(clause_idx) else {
                continue;
            };

            if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS {
                let Some(named) = arena.get_named_imports(clause_node) else {
                    continue;
                };
                for &spec_idx in &named.elements.nodes {
                    let Some(spec) = arena.get_specifier_at(spec_idx) else {
                        continue;
                    };
                    let export_ident = if spec.name.is_some() {
                        spec.name
                    } else {
                        spec.property_name
                    };
                    let Some(export_text) = arena.get_identifier_text(export_ident) else {
                        continue;
                    };
                    if export_text.starts_with(prefix) {
                        names.insert(export_text.to_string());
                    }
                }
                continue;
            }

            if clause_node.kind == SyntaxKind::Identifier as u16
                && let Some(export_text) = arena.get_identifier_text(clause_idx)
                && export_text.starts_with(prefix)
            {
                names.insert(export_text.to_string());
            }
        }

        let mut out: Vec<String> = names.into_iter().collect();
        out.sort();
        out
    }

    /// Walk `file_name`'s AST and collect all exports matching `export_name`,
    /// following wildcard re-exports recursively (via `visited` guard).
    pub(super) fn matching_exports_in_file(
        &self,
        file_name: &str,
        export_name: &str,
        visited: &mut FxHashSet<String>,
    ) -> Vec<ExportMatch> {
        if !visited.insert(file_name.to_string()) {
            return Vec::new();
        }

        let Some(file) = self.files.get(file_name) else {
            return Vec::new();
        };
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return Vec::new();
        };

        let mut matches = Vec::new();

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION
                && Self::is_supported_direct_export_declaration_kind(stmt_node.kind)
                && Self::statement_has_export_modifier(arena, stmt_node)
            {
                if !Self::statement_text_contains_name(file.source_text(), stmt_node, export_name) {
                    continue;
                }
                if export_name == "default"
                    && Self::statement_has_default_modifier(arena, stmt_node)
                {
                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Default,
                        is_type_only: Self::statement_is_type_only(stmt_node.kind),
                    });
                    continue;
                }
                if file.declaration_has_name(stmt_idx, export_name) {
                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Named {
                            export_name: export_name.to_string(),
                        },
                        is_type_only: Self::statement_is_type_only(stmt_node.kind),
                    });
                }
                continue;
            }
            if stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                let Some(export_assign) = arena.get_export_assignment(stmt_node) else {
                    continue;
                };
                if export_assign.is_export_equals
                    && let Some(expr_text) = arena.get_identifier_text(export_assign.expression)
                    && expr_text == export_name
                {
                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Default,
                        is_type_only: false,
                    });
                }
                continue;
            }
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }

            let Some(export) = arena.get_export_decl(stmt_node) else {
                continue;
            };

            if export.is_default_export {
                matches.push(ExportMatch {
                    kind: ImportCandidateKind::Default,
                    is_type_only: export.is_type_only,
                });
                continue;
            }

            if export.module_specifier.is_none() {
                if export.export_clause.is_none() {
                    continue;
                }

                let Some(clause_node) = arena.get(export.export_clause) else {
                    continue;
                };
                if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS {
                    let Some(named) = arena.get_named_imports(clause_node) else {
                        continue;
                    };
                    for &spec_idx in &named.elements.nodes {
                        let Some(spec) = arena.get_specifier_at(spec_idx) else {
                            continue;
                        };

                        let export_ident = if spec.name.is_some() {
                            spec.name
                        } else {
                            spec.property_name
                        };
                        let Some(export_text) = arena.get_identifier_text(export_ident) else {
                            continue;
                        };
                        if export_text == "default" {
                            matches.push(ExportMatch {
                                kind: ImportCandidateKind::Default,
                                is_type_only: export.is_type_only || spec.is_type_only,
                            });
                        }
                        if export_text != export_name {
                            continue;
                        }

                        let is_type_only = export.is_type_only || spec.is_type_only;
                        matches.push(ExportMatch {
                            kind: ImportCandidateKind::Named {
                                export_name: export_text.to_string(),
                            },
                            is_type_only,
                        });
                        if is_type_only && Self::file_has_type_namespace_import(file, export_text) {
                            matches.push(ExportMatch {
                                kind: ImportCandidateKind::Named {
                                    export_name: export_text.to_string(),
                                },
                                is_type_only: false,
                            });
                        }
                    }
                } else if file.declaration_has_name(export.export_clause, export_name) {
                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Named {
                            export_name: export_name.to_string(),
                        },
                        is_type_only: export.is_type_only,
                    });
                }

                continue;
            }

            let module_specifier = match arena.get_literal_text(export.module_specifier) {
                Some(text) => text,
                None => continue,
            };
            if export.export_clause.is_none() {
                if export_name == "default" {
                    continue;
                }

                let has_named_export = if let Some(resolved) =
                    self.resolve_module_specifier(file.file_name(), module_specifier)
                {
                    self.file_exports_named(&resolved, export_name, visited)
                } else {
                    self.ambient_module_exports_named(module_specifier, export_name)
                };

                if has_named_export {
                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Named {
                            export_name: export_name.to_string(),
                        },
                        is_type_only: export.is_type_only,
                    });
                }

                continue;
            }

            let Some(clause_node) = arena.get(export.export_clause) else {
                continue;
            };
            if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS {
                let Some(named) = arena.get_named_imports(clause_node) else {
                    continue;
                };
                for &spec_idx in &named.elements.nodes {
                    let Some(spec) = arena.get_specifier_at(spec_idx) else {
                        continue;
                    };

                    let export_ident = if spec.name.is_some() {
                        spec.name
                    } else {
                        spec.property_name
                    };
                    let Some(export_text) = arena.get_identifier_text(export_ident) else {
                        continue;
                    };
                    if export_text == "default" {
                        matches.push(ExportMatch {
                            kind: ImportCandidateKind::Default,
                            is_type_only: export.is_type_only || spec.is_type_only,
                        });
                    }
                    if export_text != export_name {
                        continue;
                    }

                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Named {
                            export_name: export_text.to_string(),
                        },
                        is_type_only: export.is_type_only || spec.is_type_only,
                    });
                }
            } else if clause_node.kind == SyntaxKind::Identifier as u16
                && let Some(export_text) = arena.get_identifier_text(export.export_clause)
            {
                if export_text == "default" {
                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Default,
                        is_type_only: export.is_type_only,
                    });
                }
                if export_text == export_name {
                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Named {
                            export_name: export_text.to_string(),
                        },
                        is_type_only: export.is_type_only,
                    });
                }
            }
        }

        if matches.is_empty()
            && export_name != "default"
            && Self::is_js_like_file(file_name)
            && Self::has_commonjs_named_export(file, export_name)
        {
            matches.push(ExportMatch {
                kind: ImportCandidateKind::Named {
                    export_name: export_name.to_string(),
                },
                is_type_only: false,
            });
        }

        matches
    }

    fn ambient_module_exports_named(&self, module_specifier: &str, export_name: &str) -> bool {
        self.files.keys().any(|file_name| {
            self.matching_exports_in_ambient_modules(file_name, export_name)
                .iter()
                .any(|(ambient_module, export_match)| {
                    ambient_module == module_specifier
                        && matches!(export_match.kind, ImportCandidateKind::Named { .. })
                })
        })
    }

    fn statement_modifiers<'a>(
        arena: &'a NodeArena,
        stmt_node: &'a tsz_parser::parser::node::Node,
    ) -> Option<&'a tsz_parser::parser::base::NodeList> {
        match stmt_node.kind {
            syntax_kind_ext::FUNCTION_DECLARATION => arena
                .get_function(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            syntax_kind_ext::CLASS_DECLARATION => arena
                .get_class(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            syntax_kind_ext::INTERFACE_DECLARATION => arena
                .get_interface(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => arena
                .get_type_alias(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            syntax_kind_ext::ENUM_DECLARATION => arena
                .get_enum(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            syntax_kind_ext::VARIABLE_STATEMENT => arena
                .get_variable(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            syntax_kind_ext::MODULE_DECLARATION => arena
                .get_module(stmt_node)
                .and_then(|data| data.modifiers.as_ref()),
            _ => None,
        }
    }

    pub(super) const fn is_supported_direct_export_declaration_kind(kind: u16) -> bool {
        kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::INTERFACE_DECLARATION
            || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
            || kind == syntax_kind_ext::ENUM_DECLARATION
            || kind == syntax_kind_ext::VARIABLE_STATEMENT
            || kind == syntax_kind_ext::MODULE_DECLARATION
    }

    fn statement_text_contains_name(
        source_text: &str,
        stmt_node: &tsz_parser::parser::node::Node,
        name: &str,
    ) -> bool {
        if name.is_empty() {
            return false;
        }
        let len = source_text.len();
        let start = (stmt_node.pos as usize).min(len);
        let end = (stmt_node.end as usize).min(len);
        if end <= start {
            return false;
        }
        source_text[start..end].contains(name)
    }

    fn statement_has_export_modifier(
        arena: &NodeArena,
        stmt_node: &tsz_parser::parser::node::Node,
    ) -> bool {
        let modifiers = Self::statement_modifiers(arena, stmt_node);
        arena.has_modifier_ref(modifiers, SyntaxKind::ExportKeyword)
    }

    fn statement_has_default_modifier(
        arena: &NodeArena,
        stmt_node: &tsz_parser::parser::node::Node,
    ) -> bool {
        let modifiers = Self::statement_modifiers(arena, stmt_node);
        arena.has_modifier_ref(modifiers, SyntaxKind::DefaultKeyword)
    }

    pub(super) const fn statement_is_type_only(kind: u16) -> bool {
        kind == syntax_kind_ext::INTERFACE_DECLARATION
            || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
    }

    fn file_has_type_namespace_import(file: &ProjectFile, namespace_name: &str) -> bool {
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return false;
        };

        source_file.statements.nodes.iter().any(|&stmt_idx| {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                return false;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                return false;
            }
            let Some(import_decl) = arena.get_import_decl(stmt_node) else {
                return false;
            };
            let Some(import_clause_node) = arena.get(import_decl.import_clause) else {
                return false;
            };
            let Some(import_clause) = arena.get_import_clause(import_clause_node) else {
                return false;
            };
            if !import_clause.is_type_only || !import_clause.named_bindings.is_some() {
                return false;
            }
            let Some(named_bindings_node) = arena.get(import_clause.named_bindings) else {
                return false;
            };
            if named_bindings_node.kind != syntax_kind_ext::NAMESPACE_IMPORT {
                return false;
            }
            let Some(namespace_import) = arena.get_named_imports(named_bindings_node) else {
                return false;
            };
            arena
                .get_identifier_text(namespace_import.name)
                .is_some_and(|name| name == namespace_name)
        })
    }

    fn has_commonjs_named_export(file: &ProjectFile, export_name: &str) -> bool {
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return false;
        };

        source_file.statements.nodes.iter().any(|&stmt_idx| {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                return false;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                return false;
            }
            let Some(stmt_data) = arena.get_expression_statement(stmt_node) else {
                return false;
            };
            let Some(expr_node) = arena.get(stmt_data.expression) else {
                return false;
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                return false;
            }
            let Some(binary) = arena.get_binary_expr(expr_node) else {
                return false;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                return false;
            }

            Self::is_commonjs_export_assignment(arena, binary.left, export_name)
        })
    }

    fn is_commonjs_export_assignment(
        arena: &NodeArena,
        left_idx: NodeIndex,
        export_name: &str,
    ) -> bool {
        let Some(left_node) = arena.get(left_idx) else {
            return false;
        };
        if left_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = arena.get_access_expr(left_node) else {
            return false;
        };
        let Some(member_name) = arena.get_identifier_text(access.name_or_argument) else {
            return false;
        };
        member_name == export_name && Self::is_commonjs_exports_target(arena, access.expression)
    }

    fn is_commonjs_exports_target(arena: &NodeArena, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = arena.get(expr_idx) else {
            return false;
        };

        if expr_node.kind == SyntaxKind::Identifier as u16 {
            return arena.get_identifier_text(expr_idx) == Some("exports");
        }

        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = arena.get_access_expr(expr_node) else {
            return false;
        };
        let Some(name) = arena.get_identifier_text(access.name_or_argument) else {
            return false;
        };

        if name == "exports" {
            let Some(base_node) = arena.get(access.expression) else {
                return false;
            };
            if base_node.kind == SyntaxKind::Identifier as u16
                && arena.get_identifier_text(access.expression) == Some("module")
            {
                return true;
            }
        }

        Self::is_commonjs_exports_target(arena, access.expression)
    }

    fn is_js_like_file(file_name: &str) -> bool {
        matches!(
            Path::new(file_name)
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_ascii_lowercase())
                .as_deref(),
            Some("js" | "jsx" | "mjs" | "cjs")
        )
    }

    /// Collect exports from all `declare module "…"` blocks inside `file_name`
    /// that match `export_name`.
    pub(super) fn matching_exports_in_ambient_modules(
        &self,
        file_name: &str,
        export_name: &str,
    ) -> Vec<(String, ExportMatch)> {
        let Some(file) = self.files.get(file_name) else {
            return Vec::new();
        };
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return Vec::new();
        };

        let mut matches = Vec::new();

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
            let Some(module_specifier) = arena.get_literal_text(module_decl.name) else {
                continue;
            };
            let Some(module_body_node) = arena.get(module_decl.body) else {
                continue;
            };
            if module_body_node.kind != syntax_kind_ext::MODULE_BLOCK {
                continue;
            }
            let Some(module_block) = arena.get_module_block(module_body_node) else {
                continue;
            };
            let Some(statements) = module_block.statements.as_ref() else {
                continue;
            };

            for &module_stmt_idx in &statements.nodes {
                let Some(module_stmt_node) = arena.get(module_stmt_idx) else {
                    continue;
                };
                if module_stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                    if !Self::is_supported_direct_export_declaration_kind(module_stmt_node.kind) {
                        continue;
                    }
                    if !Self::statement_has_export_modifier(arena, module_stmt_node) {
                        continue;
                    }
                    if !Self::statement_text_contains_name(
                        file.source_text(),
                        module_stmt_node,
                        export_name,
                    ) {
                        continue;
                    }
                    if file.declaration_has_name(module_stmt_idx, export_name) {
                        matches.push((
                            module_specifier.to_string(),
                            ExportMatch {
                                kind: ImportCandidateKind::Named {
                                    export_name: export_name.to_string(),
                                },
                                is_type_only: Self::statement_is_type_only(module_stmt_node.kind),
                            },
                        ));
                    }
                    continue;
                }
                let Some(export) = arena.get_export_decl(module_stmt_node) else {
                    continue;
                };
                if export.module_specifier.is_some() {
                    continue;
                }
                if export.is_default_export {
                    matches.push((
                        module_specifier.to_string(),
                        ExportMatch {
                            kind: ImportCandidateKind::Default,
                            is_type_only: export.is_type_only,
                        },
                    ));
                }
                if file.declaration_has_name(export.export_clause, export_name) {
                    matches.push((
                        module_specifier.to_string(),
                        ExportMatch {
                            kind: ImportCandidateKind::Named {
                                export_name: export_name.to_string(),
                            },
                            is_type_only: export.is_type_only,
                        },
                    ));
                }
            }
        }

        matches
    }

    pub(super) fn file_exports_named(
        &self,
        file_name: &str,
        export_name: &str,
        visited: &mut FxHashSet<String>,
    ) -> bool {
        self.matching_exports_in_file(file_name, export_name, visited)
            .iter()
            .any(|export_match| matches!(export_match.kind, ImportCandidateKind::Named { .. }))
    }

    pub(super) fn export_star_as_default_is_type_only(&self, file_name: &str) -> Option<bool> {
        let file = self.files.get(file_name)?;
        let arena = file.arena();
        let source_file = arena.get_source_file_at(file.root())?;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export.module_specifier.is_none() || export.export_clause.is_none() {
                continue;
            }
            let clause_node = arena.get(export.export_clause)?;
            let export_text = if clause_node.kind == SyntaxKind::Identifier as u16 {
                arena.get_identifier_text(export.export_clause)
            } else if clause_node.kind == SyntaxKind::StringLiteral as u16 {
                arena.get_literal_text(export.export_clause)
            } else {
                None
            };
            if export_text == Some("default") {
                return Some(export.is_type_only);
            }
        }

        None
    }
}
