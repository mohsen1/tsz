//! Project operations: references, rename, imports, and module resolution.

use std::cmp::Ordering;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use rustc_hash::FxHashSet;

use crate::lsp::code_actions::{ImportCandidate, ImportCandidateKind};
use crate::lsp::completions::{CompletionItem, CompletionItemKind};
use crate::lsp::diagnostics::LspDiagnostic;
use crate::lsp::position::{Location, Position, Range};
use crate::lsp::references::FindReferences;
use crate::lsp::rename::{RenameProvider, TextEdit, WorkspaceEdit};
use crate::lsp::resolver::ScopeCacheStats;
use crate::lsp::utils::find_node_at_offset;
use crate::parser::node::NodeAccess;
use crate::parser::{NodeIndex, node::NodeArena, syntax_kind_ext};
use crate::scanner::SyntaxKind;

use super::project::{
    ExportMatch, ImportKind, ImportSpecifierTarget, ImportTarget, NamespaceReexportTarget, Project,
    ProjectFile, ProjectRequestKind,
};

impl Project {
    fn collect_file_references(
        file: &mut ProjectFile,
        node_idx: NodeIndex,
        scope_stats: Option<&mut ScopeCacheStats>,
        output: &mut Vec<Location>,
    ) {
        if node_idx.is_none() {
            return;
        }

        let find_refs = FindReferences::new(
            file.parser.get_arena(),
            &file.binder,
            &file.line_map,
            file.file_name.clone(),
            file.parser.get_source_text(),
        );

        if let Some(mut refs) = find_refs.find_references_for_node_with_scope_cache(
            file.root(),
            node_idx,
            &mut file.scope_cache,
            scope_stats,
        ) {
            output.append(&mut refs);
        }
    }

    fn collect_file_rename_edits(
        file: &mut ProjectFile,
        node_idx: NodeIndex,
        new_name: &str,
        output: &mut WorkspaceEdit,
    ) {
        let mut locations = Vec::new();
        Self::collect_file_references(file, node_idx, None, &mut locations);
        for location in locations {
            output.add_edit(
                location.file_path,
                TextEdit::new(location.range, new_name.to_string()),
            );
        }
    }

    fn dedup_workspace_edit(workspace_edit: &mut WorkspaceEdit) {
        for edits in workspace_edit.changes.values_mut() {
            let mut seen = FxHashSet::default();
            edits.retain(|edit| {
                let key = (
                    edit.range.start.line,
                    edit.range.start.character,
                    edit.range.end.line,
                    edit.range.end.character,
                );
                seen.insert(key)
            });
        }
    }

    fn import_binding_nodes(
        &self,
        file: &ProjectFile,
        target_file: &str,
        export_name: &str,
    ) -> Vec<NodeIndex> {
        let mut bindings = Vec::new();
        let arena = file.arena();

        let Some(root_node) = arena.get(file.root()) else {
            return bindings;
        };
        let Some(source_file) = arena.get_source_file(root_node) else {
            return bindings;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION
                && stmt_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                continue;
            }

            let Some(import) = arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_specifier) = arena.get_literal_text(import.module_specifier) else {
                continue;
            };
            let Some(resolved) = self.resolve_module_specifier(file.file_name(), module_specifier)
            else {
                continue;
            };
            if resolved != target_file {
                continue;
            }

            if import.import_clause.is_none() {
                continue;
            }

            let Some(clause_node) = arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = arena.get_import_clause(clause_node) else {
                continue;
            };

            if export_name == "default" && !clause.name.is_none() {
                bindings.push(clause.name);
            }

            if clause.named_bindings.is_none() {
                continue;
            }

            let Some(bindings_node) = arena.get(clause.named_bindings) else {
                continue;
            };
            let Some(named) = arena.get_named_imports(bindings_node) else {
                continue;
            };

            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = arena.get_specifier(spec_node) else {
                    continue;
                };

                let export_ident = if !spec.property_name.is_none() {
                    spec.property_name
                } else {
                    spec.name
                };
                let Some(imported_name) = arena.get_identifier_text(export_ident) else {
                    continue;
                };
                if imported_name != export_name {
                    continue;
                }

                bindings.push(spec_idx);
            }
        }

        bindings
    }

    fn import_specifier_targets_for_export(
        &self,
        file: &ProjectFile,
        target_file: &str,
        export_name: &str,
    ) -> Vec<ImportSpecifierTarget> {
        let mut targets = Vec::new();
        let arena = file.arena();

        let Some(root_node) = arena.get(file.root()) else {
            return targets;
        };
        let Some(source_file) = arena.get_source_file(root_node) else {
            return targets;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION
                && stmt_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                continue;
            }

            let Some(import) = arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_specifier) = arena.get_literal_text(import.module_specifier) else {
                continue;
            };
            let Some(resolved) = self.resolve_module_specifier(file.file_name(), module_specifier)
            else {
                continue;
            };
            if resolved != target_file {
                continue;
            }

            if import.import_clause.is_none() {
                continue;
            }

            let Some(clause_node) = arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.named_bindings.is_none() {
                continue;
            }

            let Some(bindings_node) = arena.get(clause.named_bindings) else {
                continue;
            };
            if bindings_node.kind == SyntaxKind::Identifier as u16 {
                continue;
            }

            let Some(named) = arena.get_named_imports(bindings_node) else {
                continue;
            };

            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = arena.get_specifier(spec_node) else {
                    continue;
                };

                let export_ident = if !spec.property_name.is_none() {
                    spec.property_name
                } else {
                    spec.name
                };
                let Some(export_text) = arena.get_identifier_text(export_ident) else {
                    continue;
                };
                if export_text != export_name {
                    continue;
                }

                let local_ident = if !spec.name.is_none() {
                    spec.name
                } else {
                    spec.property_name
                };
                let property_name = if !spec.property_name.is_none() {
                    Some(spec.property_name)
                } else {
                    None
                };

                targets.push(ImportSpecifierTarget {
                    local_ident,
                    property_name,
                });
            }
        }

        targets
    }

    fn named_import_local_names(
        &self,
        file: &ProjectFile,
        target_file: &str,
        export_name: &str,
    ) -> Vec<String> {
        let mut locals = Vec::new();
        let arena = file.arena();

        let Some(root_node) = arena.get(file.root()) else {
            return locals;
        };
        let Some(source_file) = arena.get_source_file(root_node) else {
            return locals;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION
                && stmt_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                continue;
            }

            let Some(import) = arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_specifier) = arena.get_literal_text(import.module_specifier) else {
                continue;
            };
            let Some(resolved) = self.resolve_module_specifier(file.file_name(), module_specifier)
            else {
                continue;
            };
            if resolved != target_file {
                continue;
            }

            if import.import_clause.is_none() {
                continue;
            }

            let Some(clause_node) = arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.named_bindings.is_none() {
                continue;
            }

            let Some(bindings_node) = arena.get(clause.named_bindings) else {
                continue;
            };
            if bindings_node.kind == SyntaxKind::Identifier as u16 {
                continue;
            }

            let Some(named) = arena.get_named_imports(bindings_node) else {
                continue;
            };

            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = arena.get_specifier(spec_node) else {
                    continue;
                };

                let export_ident = if !spec.property_name.is_none() {
                    spec.property_name
                } else {
                    spec.name
                };
                let Some(export_text) = arena.get_identifier_text(export_ident) else {
                    continue;
                };
                if export_text != export_name {
                    continue;
                }

                let local_ident = if !spec.name.is_none() {
                    spec.name
                } else {
                    spec.property_name
                };
                let Some(local_text) = arena.get_identifier_text(local_ident) else {
                    continue;
                };
                locals.push(local_text.to_string());
            }
        }

        locals
    }

    fn reexport_targets_for(
        &self,
        source_file: &str,
        export_name: &str,
        refs: &mut Vec<Location>,
    ) -> (Vec<(String, String)>, Vec<NamespaceReexportTarget>) {
        let mut targets = Vec::new();
        let mut namespace_targets = Vec::new();

        for (file_name, file) in &self.files {
            let arena = file.arena();
            let Some(root_node) = arena.get(file.root()) else {
                continue;
            };
            let Some(source_file_node) = arena.get_source_file(root_node) else {
                continue;
            };

            for &stmt_idx in &source_file_node.statements.nodes {
                let Some(stmt_node) = arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                    continue;
                }

                let Some(export) = arena.get_export_decl(stmt_node) else {
                    continue;
                };
                if export.module_specifier.is_none() {
                    continue;
                }

                let Some(module_specifier) = arena.get_literal_text(export.module_specifier) else {
                    continue;
                };
                let Some(resolved) =
                    self.resolve_module_specifier(file.file_name(), module_specifier)
                else {
                    continue;
                };
                if resolved != source_file {
                    continue;
                }

                if export.export_clause.is_none() {
                    if export_name != "default" {
                        targets.push((file_name.clone(), export_name.to_string()));
                    }
                    continue;
                }

                let Some(clause_node) = arena.get(export.export_clause) else {
                    continue;
                };
                if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                    if clause_node.kind == SyntaxKind::Identifier as u16
                        && let Some(ns_name) = arena.get_identifier_text(export.export_clause)
                    {
                        namespace_targets.push(NamespaceReexportTarget {
                            file: file_name.clone(),
                            namespace: ns_name.to_string(),
                            member: export_name.to_string(),
                        });
                    }
                    continue;
                }

                let Some(named) = arena.get_named_imports(clause_node) else {
                    continue;
                };
                for &spec_idx in &named.elements.nodes {
                    let Some(spec_node) = arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(spec) = arena.get_specifier(spec_node) else {
                        continue;
                    };

                    let import_ident = if !spec.property_name.is_none() {
                        spec.property_name
                    } else {
                        spec.name
                    };
                    let Some(import_text) = arena.get_identifier_text(import_ident) else {
                        continue;
                    };
                    if import_text != export_name {
                        continue;
                    }

                    if let Some(location) = file.node_location(import_ident) {
                        refs.push(location);
                    }

                    let export_ident = if !spec.name.is_none() {
                        spec.name
                    } else {
                        spec.property_name
                    };
                    if let Some(export_text) = arena.get_identifier_text(export_ident) {
                        targets.push((file_name.clone(), export_text.to_string()));
                    }
                }
            }
        }

        (targets, namespace_targets)
    }

    fn namespace_import_names(&self, file: &ProjectFile, target_file: &str) -> Vec<String> {
        let mut names = Vec::new();
        let arena = file.arena();

        let Some(root_node) = arena.get(file.root()) else {
            return names;
        };
        let Some(source_file) = arena.get_source_file(root_node) else {
            return names;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION
                && stmt_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                continue;
            }

            let Some(import) = arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_specifier) = arena.get_literal_text(import.module_specifier) else {
                continue;
            };
            let Some(resolved) = self.resolve_module_specifier(file.file_name(), module_specifier)
            else {
                continue;
            };
            if resolved != target_file {
                continue;
            }

            if import.import_clause.is_none() {
                continue;
            }

            let Some(clause_node) = arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.named_bindings.is_none() {
                continue;
            }

            let Some(bindings_node) = arena.get(clause.named_bindings) else {
                continue;
            };
            if bindings_node.kind != syntax_kind_ext::NAMESPACE_IMPORT {
                continue;
            }

            let Some(bindings) = arena.get_named_imports(bindings_node) else {
                continue;
            };
            if let Some(name) = arena.get_identifier_text(bindings.name) {
                names.push(name.to_string());
            }
        }

        names
    }

    fn collect_namespace_member_locations(
        &self,
        file: &ProjectFile,
        namespace_name: &str,
        export_name: &str,
        output: &mut Vec<Location>,
    ) {
        let arena = file.arena();
        let expected_symbol = file.binder().file_locals.get(namespace_name);

        for node in arena.nodes.iter() {
            if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                continue;
            }

            let Some(access) = arena.get_access_expr(node) else {
                continue;
            };
            let expr_idx = access.expression;
            let Some(expr_node) = arena.get(expr_idx) else {
                continue;
            };
            if expr_node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }

            let Some(expr_text) = arena.get_identifier_text(expr_idx) else {
                continue;
            };
            if expr_text != namespace_name {
                continue;
            }

            if let Some(sym_id) = expected_symbol
                && file.binder().resolve_identifier(arena, expr_idx) != Some(sym_id)
            {
                continue;
            }

            let member_idx = access.name_or_argument;
            let matches = if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                arena.get_identifier_text(member_idx) == Some(export_name)
            } else {
                arena.get_literal_text(member_idx) == Some(export_name)
            };

            if !matches {
                continue;
            }

            if let Some(location) = file.node_location(member_idx) {
                output.push(location);
            }
        }
    }

    /// Find references within a single file.
    pub fn find_references(
        &mut self,
        file_name: &str,
        position: Position,
    ) -> Option<Vec<Location>> {
        let start = Instant::now();
        let mut scope_stats = ScopeCacheStats::default();
        let result = (|| {
            let (node_idx, symbol_id, local_name) = {
                let file = self.files.get_mut(file_name)?;
                let offset = file
                    .line_map
                    .position_to_offset(position, file.parser.get_source_text())?;
                let node_idx = find_node_at_offset(file.parser.get_arena(), offset);
                if node_idx.is_none() {
                    return None;
                }

                let finder = FindReferences::new(
                    file.parser.get_arena(),
                    &file.binder,
                    &file.line_map,
                    file.file_name.clone(),
                    file.parser.get_source_text(),
                );
                let symbol_id = finder.resolve_symbol_for_node_with_scope_cache(
                    file.root(),
                    node_idx,
                    &mut file.scope_cache,
                    Some(&mut scope_stats),
                )?;
                let symbol = file.binder().symbols.get(symbol_id)?;
                let local_name = symbol.escaped_name.clone();
                (node_idx, symbol_id, local_name)
            };

            let mut locations = Vec::new();
            {
                let file = self.files.get_mut(file_name)?;
                Self::collect_file_references(
                    file,
                    node_idx,
                    Some(&mut scope_stats),
                    &mut locations,
                );
            }

            let (import_targets, export_names, source_file_name) = {
                let file = self.files.get(file_name)?;
                let import_targets = file.import_targets_for_local(&local_name);
                let export_names = if import_targets.is_empty() {
                    file.exported_names_for_symbol(symbol_id)
                } else {
                    Vec::new()
                };
                (import_targets, export_names, file.file_name().to_string())
            };

            let mut cross_targets: Vec<(String, String)> = Vec::new();
            if !import_targets.is_empty() {
                for target in import_targets {
                    let Some(resolved) =
                        self.resolve_module_specifier(&source_file_name, &target.module_specifier)
                    else {
                        continue;
                    };
                    match target.kind {
                        ImportKind::Named(name) => cross_targets.push((resolved, name)),
                        ImportKind::Default => {
                            cross_targets.push((resolved, "default".to_string()))
                        }
                        ImportKind::Namespace => {}
                    }
                }
            } else {
                for export_name in export_names {
                    cross_targets.push((source_file_name.clone(), export_name));
                }
            }

            let mut expanded_targets = Vec::new();
            let mut pending = cross_targets;
            let mut seen_targets: FxHashSet<(String, String)> = FxHashSet::default();
            let mut namespace_targets = Vec::new();

            while let Some((def_file, export_name)) = pending.pop() {
                if !seen_targets.insert((def_file.clone(), export_name.clone())) {
                    continue;
                }
                expanded_targets.push((def_file.clone(), export_name.clone()));

                let mut reexport_refs = Vec::new();
                let (reexports, reexport_namespaces) =
                    self.reexport_targets_for(&def_file, &export_name, &mut reexport_refs);
                locations.extend(reexport_refs);
                pending.extend(reexports);
                namespace_targets.extend(reexport_namespaces);
            }

            let file_names: Vec<String> = self.files.keys().cloned().collect();

            for (def_file, export_name) in expanded_targets {
                let export_nodes = {
                    let target_file = self.files.get(&def_file);
                    target_file
                        .map(|file| file.export_nodes(&export_name))
                        .unwrap_or_default()
                };
                if !export_nodes.is_empty()
                    && let Some(target_file) = self.files.get_mut(&def_file)
                {
                    for node in export_nodes {
                        Self::collect_file_references(
                            target_file,
                            node,
                            Some(&mut scope_stats),
                            &mut locations,
                        );
                    }
                }

                for other_name in &file_names {
                    if other_name == &def_file {
                        continue;
                    }

                    let binding_nodes = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| self.import_binding_nodes(file, &def_file, &export_name))
                            .unwrap_or_default()
                    };
                    if !binding_nodes.is_empty()
                        && let Some(other_file) = self.files.get_mut(other_name)
                    {
                        for node in binding_nodes {
                            Self::collect_file_references(
                                other_file,
                                node,
                                Some(&mut scope_stats),
                                &mut locations,
                            );
                        }
                    }

                    let namespace_names = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| self.namespace_import_names(file, &def_file))
                            .unwrap_or_default()
                    };
                    if !namespace_names.is_empty()
                        && let Some(other_file) = self.files.get(other_name)
                    {
                        for namespace_name in namespace_names {
                            self.collect_namespace_member_locations(
                                other_file,
                                &namespace_name,
                                &export_name,
                                &mut locations,
                            );
                        }
                    }
                }
            }

            let mut seen_namespace_targets: FxHashSet<(String, String, String)> =
                FxHashSet::default();
            for target in namespace_targets {
                if !seen_namespace_targets.insert((
                    target.file.clone(),
                    target.namespace.clone(),
                    target.member.clone(),
                )) {
                    continue;
                }

                for other_name in &file_names {
                    if other_name == &target.file {
                        continue;
                    }

                    let local_names = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| {
                                self.named_import_local_names(file, &target.file, &target.namespace)
                            })
                            .unwrap_or_default()
                    };
                    if local_names.is_empty() {
                        continue;
                    }

                    if let Some(other_file) = self.files.get(other_name) {
                        for local_name in local_names {
                            self.collect_namespace_member_locations(
                                other_file,
                                &local_name,
                                &target.member,
                                &mut locations,
                            );
                        }
                    }
                }
            }

            if locations.is_empty() {
                return None;
            }

            locations.sort_by(|a, b| {
                let file_cmp = a.file_path.cmp(&b.file_path);
                if file_cmp != Ordering::Equal {
                    return file_cmp;
                }
                let start_cmp = (a.range.start.line, a.range.start.character)
                    .cmp(&(b.range.start.line, b.range.start.character));
                if start_cmp != Ordering::Equal {
                    return start_cmp;
                }
                (a.range.end.line, a.range.end.character)
                    .cmp(&(b.range.end.line, b.range.end.character))
            });
            locations.dedup_by(|a, b| a.file_path == b.file_path && a.range == b.range);

            Some(locations)
        })();

        self.performance
            .record(ProjectRequestKind::References, start.elapsed(), scope_stats);

        result
    }

    /// Rename a symbol across files in the project.
    pub fn get_rename_edits(
        &mut self,
        file_name: &str,
        position: Position,
        new_name: String,
    ) -> Result<WorkspaceEdit, String> {
        let start = Instant::now();
        let mut scope_stats = ScopeCacheStats::default();
        let result = (|| {
            let normalized_name = {
                let file = self
                    .files
                    .get(file_name)
                    .ok_or_else(|| "You cannot rename this element.".to_string())?;
                let provider = RenameProvider::new(
                    file.parser.get_arena(),
                    &file.binder,
                    &file.line_map,
                    file.file_name.clone(),
                    file.parser.get_source_text(),
                );
                provider.normalize_rename_at_position(position, &new_name)?
            };

            let (symbol_id, local_name, import_targets, export_names, source_file_name) = {
                let file = self
                    .files
                    .get_mut(file_name)
                    .ok_or_else(|| "You cannot rename this element.".to_string())?;
                let offset = file
                    .line_map
                    .position_to_offset(position, file.source_text())
                    .ok_or_else(|| "Could not find symbol to rename".to_string())?;
                let node_idx = find_node_at_offset(file.arena(), offset);
                if node_idx.is_none() {
                    return Err("Could not find symbol to rename".to_string());
                }

                let finder = FindReferences::new(
                    file.parser.get_arena(),
                    &file.binder,
                    &file.line_map,
                    file.file_name.clone(),
                    file.parser.get_source_text(),
                );
                let symbol_id = finder
                    .resolve_symbol_for_node_with_scope_cache(
                        file.root(),
                        node_idx,
                        &mut file.scope_cache,
                        Some(&mut scope_stats),
                    )
                    .ok_or_else(|| "Could not find symbol to rename".to_string())?;
                let symbol = file
                    .binder()
                    .symbols
                    .get(symbol_id)
                    .ok_or_else(|| "Could not find symbol to rename".to_string())?;
                let local_name = symbol.escaped_name.clone();
                let import_targets = file.import_targets_for_local(&local_name);
                let export_names = file.exported_names_for_symbol(symbol_id);

                (
                    symbol_id,
                    local_name,
                    import_targets,
                    export_names,
                    file.file_name().to_string(),
                )
            };

            let mut workspace_edit = {
                let file = self
                    .files
                    .get_mut(file_name)
                    .ok_or_else(|| "You cannot rename this element.".to_string())?;
                let provider = RenameProvider::new(
                    file.parser.get_arena(),
                    &file.binder,
                    &file.line_map,
                    file.file_name.clone(),
                    file.parser.get_source_text(),
                );
                provider.provide_rename_edits_for_symbol(
                    file.root(),
                    symbol_id,
                    normalized_name.clone(),
                )?
            };

            let mut cross_targets = Vec::new();

            if !import_targets.is_empty() {
                for target in import_targets {
                    let Some(resolved) =
                        self.resolve_module_specifier(&source_file_name, &target.module_specifier)
                    else {
                        continue;
                    };

                    match target.kind {
                        ImportKind::Named(name) => {
                            if name == local_name {
                                cross_targets.push((resolved, name));
                            }
                        }
                        ImportKind::Default => {
                            cross_targets.push((resolved, "default".to_string()));
                        }
                        ImportKind::Namespace => {}
                    }
                }
            }

            let mut export_names: Vec<String> = export_names
                .into_iter()
                .filter(|name| name == &local_name)
                .collect();
            export_names.sort();
            export_names.dedup();

            for export_name in export_names {
                cross_targets.push((source_file_name.clone(), export_name));
            }

            if cross_targets.is_empty() {
                Self::dedup_workspace_edit(&mut workspace_edit);
                return Ok(workspace_edit);
            }

            let file_names: Vec<String> = self.files.keys().cloned().collect();
            let mut pending = cross_targets;
            let mut seen_targets: FxHashSet<(String, String)> = FxHashSet::default();
            let mut namespace_targets = Vec::new();

            while let Some((def_file, export_name)) = pending.pop() {
                if !seen_targets.insert((def_file.clone(), export_name.clone())) {
                    continue;
                }

                if def_file != file_name {
                    let export_nodes = {
                        let target_file = self.files.get(&def_file);
                        target_file
                            .map(|file| file.export_nodes(&export_name))
                            .unwrap_or_default()
                    };
                    if !export_nodes.is_empty()
                        && let Some(target_file) = self.files.get_mut(&def_file)
                    {
                        for node in export_nodes {
                            Self::collect_file_rename_edits(
                                target_file,
                                node,
                                &normalized_name,
                                &mut workspace_edit,
                            );
                        }
                    }
                }

                let mut reexport_refs = Vec::new();
                let (reexports, reexport_namespaces) =
                    self.reexport_targets_for(&def_file, &export_name, &mut reexport_refs);
                for location in reexport_refs {
                    workspace_edit.add_edit(
                        location.file_path,
                        TextEdit::new(location.range, normalized_name.clone()),
                    );
                }

                for (reexport_file, reexport_name) in reexports {
                    if reexport_name == export_name {
                        pending.push((reexport_file, reexport_name));
                    }
                }

                namespace_targets.extend(reexport_namespaces);

                for other_name in &file_names {
                    if other_name == &def_file {
                        continue;
                    }

                    let import_targets = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| {
                                self.import_specifier_targets_for_export(
                                    file,
                                    &def_file,
                                    &export_name,
                                )
                            })
                            .unwrap_or_default()
                    };
                    if !import_targets.is_empty()
                        && let Some(other_file) = self.files.get_mut(other_name)
                    {
                        for target in import_targets {
                            if let Some(property_name) = target.property_name {
                                if let Some(location) = other_file.node_location(property_name) {
                                    workspace_edit.add_edit(
                                        location.file_path,
                                        TextEdit::new(location.range, normalized_name.clone()),
                                    );
                                }
                            } else {
                                if other_name == file_name {
                                    continue;
                                }
                                Self::collect_file_rename_edits(
                                    other_file,
                                    target.local_ident,
                                    &normalized_name,
                                    &mut workspace_edit,
                                );
                            }
                        }
                    }

                    let namespace_names = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| self.namespace_import_names(file, &def_file))
                            .unwrap_or_default()
                    };
                    if !namespace_names.is_empty()
                        && let Some(other_file) = self.files.get(other_name)
                    {
                        let mut locations = Vec::new();
                        for namespace_name in namespace_names {
                            self.collect_namespace_member_locations(
                                other_file,
                                &namespace_name,
                                &export_name,
                                &mut locations,
                            );
                        }
                        for location in locations {
                            workspace_edit.add_edit(
                                location.file_path,
                                TextEdit::new(location.range, normalized_name.clone()),
                            );
                        }
                    }
                }
            }

            let mut seen_namespace_targets: FxHashSet<(String, String, String)> =
                FxHashSet::default();
            for target in namespace_targets {
                if !seen_namespace_targets.insert((
                    target.file.clone(),
                    target.namespace.clone(),
                    target.member.clone(),
                )) {
                    continue;
                }

                for other_name in &file_names {
                    if other_name == &target.file {
                        continue;
                    }

                    let local_names = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| {
                                self.named_import_local_names(file, &target.file, &target.namespace)
                            })
                            .unwrap_or_default()
                    };
                    if local_names.is_empty() {
                        continue;
                    }

                    if let Some(other_file) = self.files.get(other_name) {
                        let mut locations = Vec::new();
                        for local_name in local_names {
                            self.collect_namespace_member_locations(
                                other_file,
                                &local_name,
                                &target.member,
                                &mut locations,
                            );
                        }
                        for location in locations {
                            workspace_edit.add_edit(
                                location.file_path,
                                TextEdit::new(location.range, normalized_name.clone()),
                            );
                        }
                    }
                }
            }

            Self::dedup_workspace_edit(&mut workspace_edit);
            Ok(workspace_edit)
        })();

        self.performance
            .record(ProjectRequestKind::Rename, start.elapsed(), scope_stats);

        result
    }

    pub(crate) fn definition_from_import(
        &self,
        file: &ProjectFile,
        position: Position,
    ) -> Option<Vec<Location>> {
        let target = self.import_target_at_position(file, position)?;
        let resolved = self.resolve_module_specifier(file.file_name(), &target.module_specifier)?;
        let target_file = self.files.get(&resolved)?;

        match target.kind {
            ImportKind::Namespace => {
                let location = target_file.node_location(target_file.root())?;
                Some(vec![location])
            }
            ImportKind::Default => {
                let locations = target_file.export_locations("default");
                if locations.is_empty() {
                    None
                } else {
                    Some(locations)
                }
            }
            ImportKind::Named(name) => {
                let locations = target_file.export_locations(&name);
                if locations.is_empty() {
                    None
                } else {
                    Some(locations)
                }
            }
        }
    }

    pub(crate) fn import_candidates_for_diagnostics(
        &self,
        file: &ProjectFile,
        diagnostics: &[LspDiagnostic],
    ) -> Vec<ImportCandidate> {
        let mut candidates = Vec::new();
        let mut seen = FxHashSet::default();

        for diag in diagnostics {
            if diag.code
                != Some(crate::checker::types::diagnostics::diagnostic_codes::CANNOT_FIND_NAME)
            {
                continue;
            }

            let Some(missing_name) = self.identifier_at_range(file, diag.range) else {
                continue;
            };

            self.collect_import_candidates_for_name(
                file,
                &missing_name,
                &mut candidates,
                &mut seen,
            );
        }

        candidates
    }

    pub(crate) fn collect_import_candidates_for_name(
        &self,
        from_file: &ProjectFile,
        missing_name: &str,
        output: &mut Vec<ImportCandidate>,
        seen: &mut FxHashSet<(String, String, String, bool)>,
    ) {
        for file_name in self.files.keys() {
            if file_name == from_file.file_name() {
                continue;
            }

            let Some(module_specifier) =
                self.module_specifier_from_files(from_file.file_name(), file_name)
            else {
                continue;
            };

            let mut visited = FxHashSet::default();
            let matches = self.matching_exports_in_file(file_name, missing_name, &mut visited);

            for export_match in matches {
                let candidate = ImportCandidate {
                    module_specifier: module_specifier.clone(),
                    local_name: missing_name.to_string(),
                    kind: export_match.kind,
                    is_type_only: export_match.is_type_only,
                };

                let kind_key = match &candidate.kind {
                    ImportCandidateKind::Named { export_name } => format!("named:{}", export_name),
                    ImportCandidateKind::Default => "default".to_string(),
                    ImportCandidateKind::Namespace => "namespace".to_string(),
                };

                if seen.insert((
                    candidate.module_specifier.clone(),
                    candidate.local_name.clone(),
                    kind_key,
                    candidate.is_type_only,
                )) {
                    output.push(candidate);
                }
            }
        }
    }

    pub(crate) fn completion_from_import_candidate(
        &self,
        candidate: &ImportCandidate,
    ) -> CompletionItem {
        let detail = self.auto_import_detail(candidate);
        let documentation = self.auto_import_documentation(candidate);

        let mut item =
            CompletionItem::new(candidate.local_name.clone(), CompletionItemKind::Variable);
        item = item.with_detail(detail);
        if let Some(doc) = documentation {
            item = item.with_documentation(doc);
        }
        item
    }

    fn auto_import_detail(&self, candidate: &ImportCandidate) -> String {
        let prefix = if candidate.is_type_only {
            "auto-import type"
        } else {
            "auto-import"
        };

        match candidate.kind {
            ImportCandidateKind::Named { .. } => {
                format!("{} from {}", prefix, candidate.module_specifier)
            }
            ImportCandidateKind::Default => {
                format!("{} default from {}", prefix, candidate.module_specifier)
            }
            ImportCandidateKind::Namespace => {
                format!("{} namespace from {}", prefix, candidate.module_specifier)
            }
        }
    }

    fn auto_import_documentation(&self, candidate: &ImportCandidate) -> Option<String> {
        let import_kw = if candidate.is_type_only {
            "import type"
        } else {
            "import"
        };

        let snippet = match &candidate.kind {
            ImportCandidateKind::Named { export_name } => {
                format!(
                    "{} {{ {} }} from \"{}\";",
                    import_kw, export_name, candidate.module_specifier
                )
            }
            ImportCandidateKind::Default => {
                format!(
                    "{} {} from \"{}\";",
                    import_kw, candidate.local_name, candidate.module_specifier
                )
            }
            ImportCandidateKind::Namespace => {
                format!(
                    "{} * as {} from \"{}\";",
                    import_kw, candidate.local_name, candidate.module_specifier
                )
            }
        };

        Some(snippet)
    }

    fn matching_exports_in_file(
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
        let Some(root_node) = arena.get(file.root()) else {
            return Vec::new();
        };
        let Some(source_file) = arena.get_source_file(root_node) else {
            return Vec::new();
        };

        let mut matches = Vec::new();

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
                        let Some(spec_node) = arena.get(spec_idx) else {
                            continue;
                        };
                        let Some(spec) = arena.get_specifier(spec_node) else {
                            continue;
                        };

                        let export_ident = if !spec.name.is_none() {
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
            let resolved = match self.resolve_module_specifier(file.file_name(), module_specifier) {
                Some(path) => path,
                None => continue,
            };

            if export.export_clause.is_none() {
                if export_name == "default" {
                    continue;
                }

                if self.file_exports_named(&resolved, export_name, visited) {
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
                    let Some(spec_node) = arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(spec) = arena.get_specifier(spec_node) else {
                        continue;
                    };

                    let export_ident = if !spec.name.is_none() {
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
                && export_text == export_name
            {
                matches.push(ExportMatch {
                    kind: ImportCandidateKind::Named {
                        export_name: export_text.to_string(),
                    },
                    is_type_only: export.is_type_only,
                });
            }
        }

        matches
    }

    fn file_exports_named(
        &self,
        file_name: &str,
        export_name: &str,
        visited: &mut FxHashSet<String>,
    ) -> bool {
        self.matching_exports_in_file(file_name, export_name, visited)
            .iter()
            .any(|export_match| matches!(export_match.kind, ImportCandidateKind::Named { .. }))
    }

    fn identifier_at_range(&self, file: &ProjectFile, range: Range) -> Option<String> {
        let offset = file
            .line_map()
            .position_to_offset(range.start, file.source_text())?;
        let node_idx = find_node_at_offset(file.arena(), offset);
        if node_idx.is_none() {
            return None;
        }

        let node = file.arena().get(node_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        file.arena()
            .get_identifier_text(node_idx)
            .map(|text| text.to_string())
    }

    pub(crate) fn identifier_at_position(
        &self,
        file: &ProjectFile,
        position: Position,
    ) -> Option<(NodeIndex, String)> {
        let offset = file
            .line_map()
            .position_to_offset(position, file.source_text())?;
        let mut node_idx = find_node_at_offset(file.arena(), offset);
        if node_idx.is_none() && offset > 0 {
            node_idx = find_node_at_offset(file.arena(), offset - 1);
        }
        if node_idx.is_none() {
            return None;
        }

        let node = file.arena().get(node_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let text = file.arena().get_identifier_text(node_idx)?.to_string();
        Some((node_idx, text))
    }

    pub(crate) fn is_member_access_node(&self, arena: &NodeArena, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        while !current.is_none() {
            let Some(node) = arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::QUALIFIED_NAME
            {
                return true;
            }

            let Some(ext) = arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
        }

        false
    }

    fn import_target_at_position(
        &self,
        file: &ProjectFile,
        position: Position,
    ) -> Option<ImportTarget> {
        let offset = file
            .line_map()
            .position_to_offset(position, file.source_text())?;
        let node_idx = find_node_at_offset(file.arena(), offset);
        if node_idx.is_none() {
            return None;
        }
        self.import_target_from_node(file, node_idx)
    }

    fn import_target_from_node(
        &self,
        file: &ProjectFile,
        node_idx: NodeIndex,
    ) -> Option<ImportTarget> {
        let arena = file.arena();
        let mut current = node_idx;
        let mut import_specifier = None;
        let mut import_clause = None;
        let mut import_decl = None;

        while !current.is_none() {
            let node = arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::IMPORT_SPECIFIER => {
                    import_specifier = Some(current);
                }
                k if k == syntax_kind_ext::IMPORT_CLAUSE => {
                    import_clause = Some(current);
                }
                k if k == syntax_kind_ext::IMPORT_DECLARATION
                    || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION =>
                {
                    import_decl = Some(current);
                    break;
                }
                _ => {}
            }
            current = arena.get_extended(current)?.parent;
        }

        let import_decl_idx = import_decl?;
        let import_decl_node = arena.get(import_decl_idx)?;
        let import_decl = arena.get_import_decl(import_decl_node)?;
        let module_specifier = arena
            .get_literal_text(import_decl.module_specifier)?
            .to_string();

        let kind = if let Some(spec_idx) = import_specifier {
            let spec_node = arena.get(spec_idx)?;
            let spec = arena.get_specifier(spec_node)?;
            let export_ident = if !spec.property_name.is_none() {
                spec.property_name
            } else {
                spec.name
            };
            let export_name = arena.get_identifier_text(export_ident)?.to_string();
            ImportKind::Named(export_name)
        } else if let Some(clause_idx) = import_clause {
            let clause_node = arena.get(clause_idx)?;
            let clause = arena.get_import_clause(clause_node)?;

            if clause.name == node_idx {
                ImportKind::Default
            } else if clause.named_bindings == node_idx {
                ImportKind::Namespace
            } else if import_decl.module_specifier == node_idx {
                ImportKind::Namespace
            } else {
                return None;
            }
        } else if import_decl.module_specifier == node_idx {
            ImportKind::Namespace
        } else {
            return None;
        };

        Some(ImportTarget {
            module_specifier,
            kind,
        })
    }

    fn resolve_module_specifier(&self, from_file: &str, module_specifier: &str) -> Option<String> {
        let candidates = self.module_specifier_candidates(from_file, module_specifier);
        candidates
            .into_iter()
            .find(|candidate| self.files.contains_key(candidate))
    }

    fn module_specifier_from_files(&self, from_file: &str, target_file: &str) -> Option<String> {
        let from_dir = Path::new(from_file)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        let target_path = strip_ts_extension(Path::new(target_file));
        let relative = relative_path(from_dir, &target_path);

        let mut spec = path_to_string(&relative).replace('\\', "/");
        if spec.is_empty() {
            return None;
        }
        if !spec.starts_with('.') {
            spec = format!("./{}", spec);
        }
        Some(spec)
    }

    fn module_specifier_candidates(&self, from_file: &str, module_specifier: &str) -> Vec<String> {
        let mut candidates = Vec::new();

        if module_specifier.starts_with('.') {
            let base_dir = Path::new(from_file)
                .parent()
                .unwrap_or_else(|| Path::new(""));
            let joined = normalize_path(&base_dir.join(module_specifier));

            if joined.extension().is_some() {
                candidates.push(path_to_string(&joined));
            } else {
                for ext in TS_EXTENSION_CANDIDATES {
                    candidates.push(path_to_string(&joined.with_extension(ext)));
                }
                for ext in TS_EXTENSION_CANDIDATES {
                    candidates.push(path_to_string(&joined.join("index").with_extension(ext)));
                }
            }
        } else {
            candidates.push(module_specifier.to_string());
            if Path::new(module_specifier).extension().is_none() {
                for ext in TS_EXTENSION_CANDIDATES {
                    candidates.push(format!("{}.{}", module_specifier, ext));
                }
            }
        }

        candidates
    }
}

const TS_EXTENSION_CANDIDATES: [&str; 7] = ["ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts"];
const TS_EXTENSION_SUFFIXES: [&str; 7] =
    [".d.ts", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts"];

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::RootDir | Component::Normal(_) | Component::Prefix(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }

    normalized
}

fn strip_ts_extension(path: &Path) -> PathBuf {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.to_path_buf();
    };

    for suffix in TS_EXTENSION_SUFFIXES {
        if let Some(base_name) = file_name.strip_suffix(suffix) {
            if base_name.is_empty() {
                return path.to_path_buf();
            }
            let mut base = PathBuf::new();
            if let Some(parent) = path.parent() {
                base.push(parent);
            }
            base.push(base_name);
            return base;
        }
    }

    path.to_path_buf()
}

fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from_components: Vec<_> = from
        .components()
        .filter(|c| *c != Component::CurDir)
        .collect();
    let to_components: Vec<_> = to
        .components()
        .filter(|c| *c != Component::CurDir)
        .collect();

    let mut common = 0;
    while common < from_components.len()
        && common < to_components.len()
        && from_components[common] == to_components[common]
    {
        common += 1;
    }

    let mut result = PathBuf::new();
    for _ in common..from_components.len() {
        result.push("..");
    }
    for component in &to_components[common..] {
        result.push(component.as_os_str());
    }

    if result.as_os_str().is_empty() {
        result.push(".");
    }

    result
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}
