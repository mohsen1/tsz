//! Import candidate collection, auto-import module specifier resolution, and related utilities.

use std::cmp::Ordering;
use std::path::{Component, Path, PathBuf};

use rustc_hash::FxHashSet;

use crate::code_actions::{ImportCandidate, ImportCandidateKind};
use crate::completions::{CompletionItem, CompletionItemKind, sort_priority};
use crate::diagnostics::LspDiagnostic;
use crate::document_symbols::SymbolKind;
use crate::utils::find_node_at_offset;
use tsz_common::position::{Location, Position, Range};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::{NodeArena, NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

use super::project::{ExportMatch, ImportKind, ImportTarget, Project, ProjectFile};

impl Project {
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
            if diag.code != Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME) {
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
        if !self.auto_imports_allowed_for_file(from_file.file_name()) {
            return;
        }

        // Try optimized path for named exports using symbol index
        let candidate_files = self.symbol_index.get_files_with_symbol(missing_name);
        let use_optimized = !candidate_files.is_empty();

        let files_to_check: Vec<String> = if use_optimized {
            // Check both: files that directly define the symbol + all files (for wildcard re-exports)
            // We need to check all files because wildcard re-exports (export * from './mod')
            // aren't tracked in the SymbolIndex
            self.files.keys().cloned().collect()
        } else {
            // Fallback to checking all files for default/namespace exports
            // (where import name can be different from export name)
            self.files.keys().cloned().collect()
        };

        for file_name in files_to_check {
            if file_name == from_file.file_name() {
                continue;
            }

            for (module_specifier, export_match) in
                self.matching_exports_in_ambient_modules(&file_name, missing_name)
            {
                if self.is_auto_import_candidate_excluded(&file_name, &module_specifier) {
                    continue;
                }

                let candidate = ImportCandidate {
                    module_specifier,
                    local_name: missing_name.to_string(),
                    kind: export_match.kind.clone(),
                    is_type_only: export_match.is_type_only,
                };

                let kind_key = match &candidate.kind {
                    ImportCandidateKind::Named { export_name } => format!("named:{export_name}"),
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

            let module_specifiers =
                self.auto_import_module_specifiers_from_files(from_file.file_name(), &file_name);
            if module_specifiers.is_empty() {
                continue;
            }

            let mut visited = FxHashSet::default();
            let matches = self.matching_exports_in_file(&file_name, missing_name, &mut visited);
            if matches.is_empty() {
                continue;
            }

            let Some(module_specifier) = module_specifiers.into_iter().find(|module_specifier| {
                !self.is_auto_import_candidate_excluded(&file_name, module_specifier)
            }) else {
                continue;
            };

            for export_match in &matches {
                let candidate = ImportCandidate {
                    module_specifier: module_specifier.clone(),
                    local_name: missing_name.to_string(),
                    kind: export_match.kind.clone(),
                    is_type_only: export_match.is_type_only,
                };

                let kind_key = match &candidate.kind {
                    ImportCandidateKind::Named { export_name } => format!("named:{export_name}"),
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

    /// Collect import candidates for symbols matching a prefix.
    ///
    /// This is used for auto-completion when the user has typed a partial
    /// identifier (e.g., "use" should match "useEffect", "useState", etc.).
    pub(crate) fn collect_import_candidates_for_prefix(
        &self,
        from_file: &ProjectFile,
        prefix: &str,
        existing: &FxHashSet<String>,
        output: &mut Vec<ImportCandidate>,
        seen: &mut FxHashSet<(String, String, String, bool)>,
    ) {
        if !self.auto_imports_allowed_for_file(from_file.file_name()) {
            return;
        }

        // Get all symbols that match the prefix using the sorted symbol index
        let matching_symbols = self.symbol_index.get_symbols_with_prefix(prefix);

        for symbol_name in matching_symbols {
            // Skip if the symbol already exists in the current file (local definition or imported)
            if existing.contains(&symbol_name) {
                continue;
            }

            // Check ALL files for this symbol (including wildcard re-exports)
            let files_to_check: Vec<String> = self.files.keys().cloned().collect();

            for file_name in files_to_check {
                if file_name == from_file.file_name() {
                    continue;
                }

                for (module_specifier, export_match) in
                    self.matching_exports_in_ambient_modules(&file_name, &symbol_name)
                {
                    if self.is_auto_import_candidate_excluded(&file_name, &module_specifier) {
                        continue;
                    }

                    let candidate = ImportCandidate {
                        module_specifier,
                        local_name: symbol_name.clone(),
                        kind: export_match.kind.clone(),
                        is_type_only: export_match.is_type_only,
                    };

                    let kind_key = match &candidate.kind {
                        ImportCandidateKind::Named { export_name } => {
                            format!("named:{export_name}")
                        }
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

                let module_specifiers = self
                    .auto_import_module_specifiers_from_files(from_file.file_name(), &file_name);
                if module_specifiers.is_empty() {
                    continue;
                }

                let mut visited = FxHashSet::default();
                let matches = self.matching_exports_in_file(&file_name, &symbol_name, &mut visited);
                if matches.is_empty() {
                    continue;
                }

                let Some(module_specifier) =
                    module_specifiers.into_iter().find(|module_specifier| {
                        !self.is_auto_import_candidate_excluded(&file_name, module_specifier)
                    })
                else {
                    continue;
                };

                for export_match in &matches {
                    let candidate = ImportCandidate {
                        module_specifier: module_specifier.clone(),
                        local_name: symbol_name.clone(),
                        kind: export_match.kind.clone(),
                        is_type_only: export_match.is_type_only,
                    };

                    let kind_key = match &candidate.kind {
                        ImportCandidateKind::Named { export_name } => {
                            format!("named:{export_name}")
                        }
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
    }

    fn auto_import_path_is_excluded(&self, path: &str) -> bool {
        if self.auto_import_file_exclude_matchers.is_empty() {
            return false;
        }

        let normalized = path.trim().replace('\\', "/");
        if normalized.is_empty() {
            return false;
        }

        let trimmed = normalized.trim_start_matches('/');
        self.auto_import_file_exclude_matchers
            .iter()
            .any(|matcher| {
                matcher.is_match(&normalized)
                    || (!trimmed.is_empty() && matcher.is_match(trimmed))
                    || normalized
                        .strip_prefix('/')
                        .is_some_and(|stripped| matcher.is_match(stripped))
            })
    }

    fn auto_import_specifier_is_excluded(&self, module_specifier: &str) -> bool {
        self.auto_import_specifier_exclude_matchers
            .iter()
            .any(|matcher| matcher.is_match(module_specifier))
    }

    fn is_auto_import_candidate_excluded(&self, target_file: &str, module_specifier: &str) -> bool {
        if self.auto_import_specifier_is_excluded(module_specifier) {
            return true;
        }

        if self.auto_import_path_is_excluded(target_file) {
            return true;
        }

        if module_specifier.starts_with('.') {
            return false;
        }

        if self.auto_import_path_is_excluded(module_specifier) {
            return true;
        }

        let synthetic_node_modules_path = format!("/node_modules/{module_specifier}");
        self.auto_import_path_is_excluded(&synthetic_node_modules_path)
            || self
                .auto_import_path_is_excluded(synthetic_node_modules_path.trim_start_matches('/'))
    }

    pub(crate) fn completion_from_import_candidate(
        &self,
        candidate: &ImportCandidate,
    ) -> CompletionItem {
        let detail = self.auto_import_detail(candidate);
        let documentation = self.auto_import_documentation(candidate);
        let completion_kind = self.auto_import_completion_kind(candidate);

        let mut item = CompletionItem::new(candidate.local_name.clone(), completion_kind)
            .with_detail(detail)
            .with_sort_text(sort_priority::AUTO_IMPORT)
            .with_has_action()
            .with_source(candidate.module_specifier.clone())
            .with_source_display(candidate.module_specifier.clone())
            .with_kind_modifiers("export".to_string());
        if let Some(doc) = documentation {
            item = item.with_documentation(doc);
        }
        item
    }

    fn auto_import_completion_kind(&self, candidate: &ImportCandidate) -> CompletionItemKind {
        match self.symbol_index.get_definition_kind(&candidate.local_name) {
            Some(SymbolKind::Class) => CompletionItemKind::Class,
            Some(SymbolKind::Method) => CompletionItemKind::Method,
            Some(SymbolKind::Property) | Some(SymbolKind::Field) | Some(SymbolKind::Key) => {
                CompletionItemKind::Property
            }
            Some(SymbolKind::Constructor) => CompletionItemKind::Constructor,
            Some(SymbolKind::Enum) => CompletionItemKind::Enum,
            Some(SymbolKind::Interface) => CompletionItemKind::Interface,
            Some(SymbolKind::Function) | Some(SymbolKind::Event) | Some(SymbolKind::Operator) => {
                CompletionItemKind::Function
            }
            Some(SymbolKind::Module) | Some(SymbolKind::Namespace) | Some(SymbolKind::Package) => {
                CompletionItemKind::Module
            }
            Some(SymbolKind::TypeParameter) => CompletionItemKind::TypeParameter,
            Some(SymbolKind::Struct) => CompletionItemKind::TypeAlias,
            _ => CompletionItemKind::Variable,
        }
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
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
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

    fn matching_exports_in_ambient_modules(
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
            .map(std::string::ToString::to_string)
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
        while current.is_some() {
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

        while current.is_some() {
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
        let import_decl = arena.get_import_decl_at(import_decl_idx)?;
        let module_specifier = arena
            .get_literal_text(import_decl.module_specifier)?
            .to_string();

        let kind = if let Some(spec_idx) = import_specifier {
            let spec = arena.get_specifier_at(spec_idx)?;
            let export_ident = if spec.property_name.is_some() {
                spec.property_name
            } else {
                spec.name
            };
            let export_name = arena.get_identifier_text(export_ident)?.to_string();
            ImportKind::Named(export_name)
        } else if let Some(clause_idx) = import_clause {
            let clause = arena.get_import_clause_at(clause_idx)?;

            if clause.name == node_idx {
                ImportKind::Default
            } else if clause.named_bindings == node_idx || import_decl.module_specifier == node_idx
            {
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

    pub(crate) fn resolve_module_specifier(
        &self,
        from_file: &str,
        module_specifier: &str,
    ) -> Option<String> {
        let candidates = self.module_specifier_candidates(from_file, module_specifier);
        candidates
            .into_iter()
            .find(|candidate| self.files.contains_key(candidate))
    }

    fn auto_import_module_specifiers_from_files(
        &self,
        from_file: &str,
        target_file: &str,
    ) -> Vec<String> {
        let target_in_node_modules = target_file.replace('\\', "/").contains("/node_modules/");
        if let Some(package_specifier) = self.package_specifier_from_node_modules(target_file) {
            return vec![package_specifier];
        }

        let Some(relative) = self.relative_module_specifier_from_files(from_file, target_file)
        else {
            return Vec::new();
        };

        let root_dirs_relative =
            self.root_dirs_relative_specifier_from_files(from_file, target_file);
        let path_mappings = self.path_mapping_specifiers_from_files(from_file, target_file);
        let package_imports = self.package_import_specifiers_from_files(from_file, target_file);
        let pref = self.import_module_specifier_preference.as_deref();
        let mut candidates = Vec::new();

        if pref == Some("non-relative") {
            candidates.extend(path_mappings);
            candidates.extend(package_imports);
            candidates.push(relative);
            if let Some(root_dirs_relative) = root_dirs_relative {
                candidates.push(root_dirs_relative);
            }
        } else {
            candidates.push(relative);
            if let Some(root_dirs_relative) = root_dirs_relative {
                candidates.push(root_dirs_relative);
            }
            candidates.extend(path_mappings);
            candidates.extend(package_imports);
        }

        let mut seen = FxHashSet::default();
        candidates.retain(|spec| seen.insert(spec.clone()));
        if target_in_node_modules {
            candidates.retain(|spec| !spec.replace('\\', "/").contains("node_modules/"));
        }

        if pref.is_none() || pref == Some("shortest") {
            candidates.sort_by(compare_module_specifier_candidates);
        } else if pref == Some("non-relative") {
            candidates.sort_by(|a, b| {
                let a_relative = a.starts_with('.');
                let b_relative = b.starts_with('.');
                a_relative
                    .cmp(&b_relative)
                    .then_with(|| compare_module_specifier_candidates(a, b))
            });
        }

        candidates
    }

    fn path_mapping_specifiers_from_files(
        &self,
        from_file: &str,
        target_file: &str,
    ) -> Vec<String> {
        let Some((config_dir, compiler_options)) =
            self.nearest_compiler_options_for_file(from_file)
        else {
            return Vec::new();
        };

        let Some(paths) = compiler_options
            .get("paths")
            .and_then(serde_json::Value::as_object)
        else {
            return Vec::new();
        };

        let base_url = compiler_options
            .get("baseUrl")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(".");
        let base_dir = normalize_path(&config_dir.join(base_url));
        let target_file = path_to_string(&strip_js_ts_extension(&normalize_path(Path::new(
            target_file,
        ))))
        .replace('\\', "/");

        let mut specifiers = Vec::new();
        for (alias_pattern, mapped_targets) in paths {
            let Some(mapped_targets) = mapped_targets.as_array() else {
                continue;
            };
            for mapped_target in mapped_targets {
                let Some(mapped_target) = mapped_target.as_str() else {
                    continue;
                };
                let mapped_target = mapped_target.replace('\\', "/");
                let mapped_target = if let Some(rest) = mapped_target.strip_prefix("${configDir}/")
                {
                    path_to_string(&normalize_path(&config_dir.join(rest))).replace('\\', "/")
                } else {
                    path_to_string(&normalize_path(&base_dir.join(&mapped_target)))
                        .replace('\\', "/")
                };
                let mapped_target =
                    path_to_string(&strip_js_ts_extension(Path::new(&mapped_target)))
                        .replace('\\', "/");

                let Some(capture) = wildcard_capture_case_insensitive(&mapped_target, &target_file)
                else {
                    continue;
                };
                let Some(specifier) = apply_wildcard_capture(alias_pattern, &capture) else {
                    continue;
                };
                specifiers.push(normalize_path_mapping_specifier(&specifier));
            }
        }

        let mut seen = FxHashSet::default();
        specifiers.retain(|specifier| seen.insert(specifier.clone()));
        specifiers
    }

    fn root_dirs_relative_specifier_from_files(
        &self,
        from_file: &str,
        target_file: &str,
    ) -> Option<String> {
        let (config_dir, compiler_options) = self.nearest_compiler_options_for_file(from_file)?;
        let root_dirs = compiler_options
            .get("rootDirs")
            .and_then(serde_json::Value::as_array)?;
        if root_dirs.is_empty() {
            return None;
        }

        let roots: Vec<PathBuf> = root_dirs
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(|root| normalize_path(&config_dir.join(root)))
            .collect();
        if roots.is_empty() {
            return None;
        }

        let from_path = strip_ts_extension(&normalize_path(Path::new(from_file)));
        let target_path = strip_ts_extension(&normalize_path(Path::new(target_file)));
        let style = self.relative_import_style(from_file);
        let mut best_spec: Option<String> = None;

        for from_root in &roots {
            let Ok(from_rel) = from_path.strip_prefix(from_root) else {
                continue;
            };
            let from_rel_dir = from_rel.parent().unwrap_or_else(|| Path::new(""));
            for target_root in &roots {
                let Ok(target_rel) = target_path.strip_prefix(target_root) else {
                    continue;
                };

                let relative = relative_path(from_rel_dir, target_rel);
                let mut spec = path_to_string(&relative).replace('\\', "/");
                if spec.is_empty() {
                    continue;
                }
                if !spec.starts_with('.') {
                    spec = format!("./{spec}");
                }

                // Preserve existing extension style behavior for relative imports.
                match style {
                    RelativeImportStyle::Minimal => {}
                    RelativeImportStyle::Ts => {
                        if let Some(ext) = ts_source_extension(target_file) {
                            spec.push_str(ext);
                        }
                    }
                    RelativeImportStyle::Js => spec.push_str(".js"),
                }

                if let Some(current_best) = best_spec.as_ref() {
                    if compare_module_specifier_candidates(&spec, current_best) == Ordering::Less {
                        best_spec = Some(spec);
                    }
                } else {
                    best_spec = Some(spec);
                }
            }
        }

        best_spec
    }

    fn nearest_compiler_options_for_file(
        &self,
        from_file: &str,
    ) -> Option<(PathBuf, serde_json::Map<String, serde_json::Value>)> {
        let mut current = Path::new(from_file).parent();
        while let Some(dir) = current {
            for config_name in ["tsconfig.json", "jsconfig.json"] {
                let config_path = normalize_path(&dir.join(config_name));
                let config_key = path_to_string(&config_path).replace('\\', "/");
                let config_text = self
                    .files
                    .get(&config_key)
                    .map(|f| f.source_text().to_string())
                    .or_else(|| std::fs::read_to_string(&config_key).ok());
                let Some(config_text) = config_text else {
                    continue;
                };
                let Some(config_json) = parse_typescript_config_json(&config_text) else {
                    continue;
                };
                let Some(compiler_options) = config_json
                    .get("compilerOptions")
                    .and_then(serde_json::Value::as_object)
                    .cloned()
                else {
                    continue;
                };
                return Some((normalize_path(dir), compiler_options));
            }
            current = dir.parent();
        }
        None
    }

    fn auto_imports_allowed_for_file(&self, from_file: &str) -> bool {
        let Some((_, compiler_options)) = self.nearest_compiler_options_for_file(from_file) else {
            if let Some(allow) = self.auto_imports_allowed_from_fourslash_directives(from_file) {
                return allow;
            }
            return self.auto_imports_allowed_without_tsconfig;
        };

        let module_none = compiler_options
            .get("module")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|module| module.eq_ignore_ascii_case("none"));
        if !module_none {
            return true;
        }

        compiler_options
            .get("target")
            .and_then(serde_json::Value::as_str)
            .is_some_and(target_supports_import_syntax)
    }

    fn auto_imports_allowed_from_fourslash_directives(&self, from_file: &str) -> Option<bool> {
        self.files
            .get(from_file)
            .and_then(|file| Self::fourslash_auto_import_directive_result(file.source_text()))
            .or_else(|| {
                self.files.values().find_map(|file| {
                    (file.file_name != from_file)
                        .then(|| Self::fourslash_auto_import_directive_result(file.source_text()))
                        .flatten()
                })
            })
    }

    fn fourslash_auto_import_directive_result(source_text: &str) -> Option<bool> {
        let mut saw_module = false;
        let mut module_none = false;
        let mut saw_target = false;
        let mut target_supports_imports = false;

        for line in source_text.lines().take(64) {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("// @module:") {
                saw_module = true;
                module_none = rest.split(',').map(str::trim).any(|value| {
                    value.eq_ignore_ascii_case("none") || value.parse::<i64>().ok() == Some(0)
                });
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("// @target:") {
                saw_target = true;
                target_supports_imports = rest
                    .split(',')
                    .map(str::trim)
                    .any(target_supports_import_syntax);
            }
        }

        if saw_module && module_none {
            return Some(saw_target && target_supports_imports);
        }

        None
    }

    fn relative_module_specifier_from_files(
        &self,
        from_file: &str,
        target_file: &str,
    ) -> Option<String> {
        let style = self.relative_import_style(from_file);
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
            spec = format!("./{spec}");
        }

        match style {
            RelativeImportStyle::Minimal => {}
            RelativeImportStyle::Ts => {
                if let Some(ext) = ts_source_extension(target_file) {
                    spec.push_str(ext);
                }
            }
            RelativeImportStyle::Js => {
                spec.push_str(".js");
            }
        }

        Some(spec)
    }

    fn package_import_specifiers_from_files(
        &self,
        from_file: &str,
        target_file: &str,
    ) -> Vec<String> {
        let additional_targets = self.package_import_target_alternatives(from_file, target_file);
        let mut current = Path::new(from_file).parent();
        while let Some(dir) = current {
            let package_json_path = normalize_path(&dir.join("package.json"));
            let package_json_key = path_to_string(&package_json_path).replace('\\', "/");
            let Some(package_json_text) = self
                .files
                .get(&package_json_key)
                .map(|f| f.source_text().to_string())
                .or_else(|| std::fs::read_to_string(&package_json_key).ok())
            else {
                current = dir.parent();
                continue;
            };

            let package_dir = path_to_string(dir).replace('\\', "/");
            return package_import_specifiers_for_target(
                &package_json_text,
                &package_dir,
                target_file,
                self.allow_importing_ts_extensions,
                &additional_targets,
            );
        }

        Vec::new()
    }

    fn package_import_target_alternatives(
        &self,
        from_file: &str,
        target_file: &str,
    ) -> Vec<String> {
        let mut current = Path::new(from_file).parent();
        while let Some(dir) = current {
            let tsconfig_path = normalize_path(&dir.join("tsconfig.json"));
            let tsconfig_key = path_to_string(&tsconfig_path).replace('\\', "/");
            let Some(tsconfig_text) = self
                .files
                .get(&tsconfig_key)
                .map(|f| f.source_text().to_string())
                .or_else(|| std::fs::read_to_string(&tsconfig_key).ok())
            else {
                current = dir.parent();
                continue;
            };

            let Some(tsconfig) = parse_typescript_config_json(&tsconfig_text) else {
                return Vec::new();
            };
            let Some(compiler_options) = tsconfig
                .get("compilerOptions")
                .and_then(serde_json::Value::as_object)
            else {
                return Vec::new();
            };

            let root_dir = compiler_options
                .get("rootDir")
                .and_then(serde_json::Value::as_str);
            let out_dir = compiler_options
                .get("outDir")
                .and_then(serde_json::Value::as_str);
            let declaration_dir = compiler_options
                .get("declarationDir")
                .and_then(serde_json::Value::as_str);

            let Some(root_dir) = root_dir else {
                return Vec::new();
            };

            let config_dir = normalize_path(dir);
            let root_dir = normalize_path(&config_dir.join(root_dir));
            let target_path = strip_js_ts_extension(&normalize_path(Path::new(target_file)));
            let Ok(relative) = target_path.strip_prefix(&root_dir) else {
                return Vec::new();
            };

            let mut alternatives = Vec::new();
            if let Some(out_dir) = out_dir {
                let out_dir = normalize_path(&config_dir.join(out_dir));
                alternatives.push(path_to_string(&out_dir.join(relative)).replace('\\', "/"));
            }
            if let Some(declaration_dir) = declaration_dir {
                let declaration_dir = normalize_path(&config_dir.join(declaration_dir));
                alternatives
                    .push(path_to_string(&declaration_dir.join(relative)).replace('\\', "/"));
            }

            return alternatives;
        }

        Vec::new()
    }

    fn relative_import_style(&self, from_file: &str) -> RelativeImportStyle {
        if self.import_module_specifier_ending.as_deref() == Some("js") {
            return RelativeImportStyle::Ts;
        }

        if from_file.ends_with(".mts") {
            return RelativeImportStyle::Minimal;
        }

        let Some(file) = self.files.get(from_file) else {
            return RelativeImportStyle::Minimal;
        };
        let arena = file.arena();
        let Some(source_file) = arena.get_source_file_at(file.root()) else {
            return RelativeImportStyle::Minimal;
        };

        let mut saw_ts = false;
        let mut saw_js = false;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }
            let Some(import_decl) = arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_text) = arena.get_literal_text(import_decl.module_specifier) else {
                continue;
            };
            if !module_text.starts_with('.') {
                continue;
            }

            if has_ts_extension(module_text) {
                saw_ts = true;
            } else if has_js_extension(module_text) {
                saw_js = true;
            }
        }

        if saw_js {
            RelativeImportStyle::Js
        } else if saw_ts {
            RelativeImportStyle::Ts
        } else {
            RelativeImportStyle::Minimal
        }
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
                    candidates.push(format!("{module_specifier}.{ext}"));
                }
            }
        }

        candidates
    }

    fn package_specifier_from_node_modules(&self, target_file: &str) -> Option<String> {
        let normalized = target_file.replace('\\', "/");
        let marker = "/node_modules/";
        let marker_idx = normalized.find(marker)?;
        let package_path = &normalized[marker_idx + marker.len()..];
        if package_path.is_empty() {
            return None;
        }

        let (package_root, _package_suffix) = split_node_modules_package_path(package_path)?;
        let package_root = normalize_node_modules_package_specifier(&package_root);
        let package_prefix = &normalized[..marker_idx + marker.len()];
        let package_json_path = format!("{package_prefix}{package_root}/package.json");
        let package_json = self
            .files
            .get(&package_json_path)
            .map(|f| f.source_text().to_string())
            .or_else(|| std::fs::read_to_string(&package_json_path).ok())
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());

        if package_json
            .as_ref()
            .and_then(|json| json.get("exports"))
            .is_some()
        {
            return self.package_specifier_from_package_exports(
                &normalized,
                &package_root,
                package_prefix,
                &package_json_path,
            );
        }

        let runtime_spec = package_runtime_specifier_from_target_path(package_path);
        if let Some(package_json) = package_json.as_ref()
            && let Some(specifier) = package_main_module_specifier_for_target(
                package_json,
                &package_root,
                &runtime_spec,
                target_file,
            )
        {
            return Some(specifier);
        }

        let spec = normalize_node_modules_package_specifier(&runtime_spec);
        if spec.is_empty() { None } else { Some(spec) }
    }

    fn package_specifier_from_package_exports(
        &self,
        normalized_target: &str,
        package_root: &str,
        package_prefix: &str,
        package_json_path: &str,
    ) -> Option<String> {
        let package_json_text = if let Some(file) = self.files.get(package_json_path) {
            Some(file.source_text().to_string())
        } else {
            std::fs::read_to_string(package_json_path).ok()
        }?;

        let package_json = serde_json::from_str::<serde_json::Value>(&package_json_text).ok()?;
        let exports_value = package_json.get("exports")?;
        if let Some(exports_target) = exports_value.as_str() {
            let package_dir = format!("{package_prefix}{package_root}");
            let package_dir_prefix = format!("{package_dir}/");
            let target_relative = normalized_target.strip_prefix(&package_dir_prefix)?;
            let target_relative =
                path_to_string(&strip_js_ts_extension(Path::new(target_relative)))
                    .replace('\\', "/");
            let target_pattern = path_to_string(&strip_js_ts_extension(Path::new(exports_target)))
                .replace('\\', "/");
            let target_pattern = target_pattern.strip_prefix("./").unwrap_or(&target_pattern);
            if wildcard_capture_case_insensitive(target_pattern, &target_relative).is_some() {
                return Some(package_root.to_string());
            }
            return None;
        }
        let exports_object = exports_value.as_object()?;

        let package_dir = format!("{package_prefix}{package_root}");
        let package_dir_prefix = format!("{package_dir}/");
        let target_relative = normalized_target.strip_prefix(&package_dir_prefix)?;
        let target_relative =
            path_to_string(&strip_js_ts_extension(Path::new(target_relative))).replace('\\', "/");

        for (export_key, export_target) in exports_object {
            let key_pattern = if export_key == "." {
                ""
            } else if let Some(rest) = export_key.strip_prefix("./") {
                rest
            } else {
                continue;
            };

            let (type_targets, default_targets) = collect_exports_targets(export_target);
            let should_append_js = key_pattern.contains('*')
                && !has_source_extension(key_pattern)
                && default_targets
                    .iter()
                    .any(|target| !has_source_extension(target));

            for target_pattern in type_targets.iter().chain(default_targets.iter()) {
                let target_pattern = target_pattern.replace('\\', "/");
                let target_pattern = target_pattern.strip_prefix("./").unwrap_or(&target_pattern);
                let target_pattern =
                    path_to_string(&strip_js_ts_extension(Path::new(target_pattern)))
                        .replace('\\', "/");

                let Some(capture) =
                    wildcard_capture_case_insensitive(&target_pattern, &target_relative)
                else {
                    continue;
                };

                if export_key == "." {
                    return Some(package_root.to_string());
                }

                let mut subpath = apply_wildcard_capture(key_pattern, &capture)?;
                if should_append_js && !has_source_extension(&subpath) {
                    subpath.push_str(".js");
                }
                if subpath.is_empty() {
                    return Some(package_root.to_string());
                }
                return Some(format!("{package_root}/{subpath}"));
            }
        }

        None
    }
}

const TS_EXTENSION_CANDIDATES: [&str; 7] = ["ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts"];
const TS_EXTENSION_SUFFIXES: [&str; 7] =
    [".d.ts", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RelativeImportStyle {
    Minimal,
    Ts,
    Js,
}

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
fn split_node_modules_package_path(package_path: &str) -> Option<(String, String)> {
    let mut segments = package_path.split('/');
    let first = segments.next()?;
    if first.is_empty() {
        return None;
    }

    if first.starts_with('@') {
        let second = segments.next()?;
        let package_root = format!("{first}/{second}");
        let suffix = segments.collect::<Vec<_>>().join("/");
        Some((package_root, suffix))
    } else {
        let suffix = segments.collect::<Vec<_>>().join("/");
        Some((first.to_string(), suffix))
    }
}

fn normalize_node_modules_package_specifier(package_specifier: &str) -> String {
    let mut normalized = package_specifier.replace('\\', "/");
    if let Some(stripped) = normalized.strip_suffix("/index")
        && !stripped.is_empty()
    {
        normalized = stripped.to_string();
    }

    if let Some(stripped) = normalized.strip_prefix("@types/") {
        let mut parts = stripped.splitn(2, '/');
        let package_name = parts.next().unwrap_or_default();
        let rest = parts.next();

        let package_name = if let Some((scope, name)) = package_name.split_once("__") {
            format!("@{scope}/{name}")
        } else {
            package_name.to_string()
        };

        return match rest {
            Some(rest) if !rest.is_empty() && rest != "index" => {
                format!("{package_name}/{rest}")
            }
            _ => package_name,
        };
    }

    normalized
}

fn normalize_path_mapping_specifier(specifier: &str) -> String {
    specifier
        .strip_suffix("/index")
        .unwrap_or(specifier)
        .to_string()
}

fn package_runtime_specifier_from_target_path(package_path: &str) -> String {
    let normalized = package_path.replace('\\', "/");

    if let Some(base) = normalized.strip_suffix(".d.mts") {
        return format!("{base}.mjs");
    }
    if let Some(base) = normalized.strip_suffix(".d.cts") {
        return format!("{base}.cjs");
    }
    if let Some(base) = normalized.strip_suffix(".d.ts") {
        return base.to_string();
    }

    normalized
}

fn is_declaration_source_path(path: &str) -> bool {
    path.ends_with(".d.ts") || path.ends_with(".d.mts") || path.ends_with(".d.cts")
}

fn normalize_package_entry_for_match(path: &str) -> String {
    let path = path.replace('\\', "/");
    let path = path.strip_prefix("./").unwrap_or(&path);
    let stripped = path_to_string(&strip_js_ts_extension(Path::new(path))).replace('\\', "/");
    stripped
        .strip_suffix("/index")
        .unwrap_or(&stripped)
        .to_string()
}

fn package_main_module_specifier_for_target(
    package_json: &serde_json::Value,
    package_root: &str,
    runtime_package_spec: &str,
    target_file: &str,
) -> Option<String> {
    // Declaration files frequently model multiple runtime entrypoints; avoid
    // collapsing them to package root/main aliases.
    if is_declaration_source_path(target_file) {
        return None;
    }

    let package_prefix = format!("{package_root}/");
    let runtime_subpath = runtime_package_spec.strip_prefix(&package_prefix)?;
    let runtime_normalized = normalize_package_entry_for_match(runtime_subpath);
    if runtime_normalized.is_empty() {
        return None;
    }

    let package_type_module = package_json
        .get("type")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value == "module");

    for entry_field in ["module", "main"] {
        let Some(entry) = package_json
            .get(entry_field)
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        let entry_normalized = normalize_package_entry_for_match(entry);
        if entry_normalized.is_empty() || entry_normalized != runtime_normalized {
            continue;
        }

        if package_type_module {
            return Some(format!("{package_root}/{entry_normalized}"));
        }

        return Some(package_root.to_string());
    }

    None
}

fn has_ts_extension(module_text: &str) -> bool {
    module_text.ends_with(".ts")
        || module_text.ends_with(".tsx")
        || module_text.ends_with(".mts")
        || module_text.ends_with(".cts")
}

fn has_js_extension(module_text: &str) -> bool {
    module_text.ends_with(".js")
        || module_text.ends_with(".jsx")
        || module_text.ends_with(".mjs")
        || module_text.ends_with(".cjs")
}

fn ts_source_extension(target_file: &str) -> Option<&'static str> {
    if target_file.ends_with(".tsx") {
        Some(".tsx")
    } else if target_file.ends_with(".ts") && !target_file.ends_with(".d.ts") {
        Some(".ts")
    } else if target_file.ends_with(".mts") && !target_file.ends_with(".d.mts") {
        Some(".mts")
    } else if target_file.ends_with(".cts") && !target_file.ends_with(".d.cts") {
        Some(".cts")
    } else {
        None
    }
}

fn target_supports_import_syntax(target: &str) -> bool {
    let target = target.trim();
    if let Ok(numeric_target) = target.parse::<i64>() {
        return numeric_target >= 2;
    }

    target.eq_ignore_ascii_case("es6")
        || target.eq_ignore_ascii_case("es2015")
        || target.eq_ignore_ascii_case("es2016")
        || target.eq_ignore_ascii_case("es2017")
        || target.eq_ignore_ascii_case("es2018")
        || target.eq_ignore_ascii_case("es2019")
        || target.eq_ignore_ascii_case("es2020")
        || target.eq_ignore_ascii_case("es2021")
        || target.eq_ignore_ascii_case("es2022")
        || target.eq_ignore_ascii_case("es2023")
        || target.eq_ignore_ascii_case("es2024")
        || target.eq_ignore_ascii_case("esnext")
        || target.eq_ignore_ascii_case("latest")
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

fn parse_typescript_config_json(text: &str) -> Option<serde_json::Value> {
    serde_json::from_str(text)
        .ok()
        .or_else(|| json5::from_str::<serde_json::Value>(text).ok())
}

fn compare_module_specifier_candidates(a: &String, b: &String) -> Ordering {
    let a_segments = a.matches('/').count();
    let b_segments = b.matches('/').count();
    let candidate_rank = |candidate: &str| -> u8 {
        if candidate.starts_with("./") {
            0
        } else if !candidate.starts_with('.') {
            1
        } else if candidate.starts_with("../") {
            2
        } else {
            3
        }
    };
    let a_rank = candidate_rank(a);
    let b_rank = candidate_rank(b);
    a_segments
        .cmp(&b_segments)
        .then_with(|| a_rank.cmp(&b_rank))
        .then_with(|| a.len().cmp(&b.len()))
        .then_with(|| a.cmp(b))
}

fn package_import_specifiers_for_target(
    package_json_text: &str,
    package_dir: &str,
    target_file: &str,
    allow_importing_ts_extensions: bool,
    additional_targets: &[String],
) -> Vec<String> {
    let Some(package_json) = serde_json::from_str::<serde_json::Value>(package_json_text).ok()
    else {
        return Vec::new();
    };

    let Some(imports) = package_json
        .get("imports")
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };
    let package_type_module = package_json
        .get("type")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|v| v == "module");

    let package_dir = normalize_path(Path::new(package_dir));
    let target_path = strip_js_ts_extension(Path::new(target_file));
    let target_normalized = path_to_string(&target_path).replace('\\', "/");

    let mut specs = Vec::new();

    for (specifier_pattern, target_mapping) in imports {
        if !specifier_pattern.starts_with('#') {
            continue;
        }

        let target_patterns = collect_import_targets(target_mapping);
        for target_pattern in target_patterns {
            let target_pattern = target_pattern.replace('\\', "/");
            if !target_pattern.starts_with("./") {
                continue;
            }

            let resolved = normalize_path(&package_dir.join(&target_pattern));
            let resolved_stripped =
                path_to_string(&strip_js_ts_extension(&resolved)).replace('\\', "/");

            let direct_capture =
                wildcard_capture_case_insensitive(&resolved_stripped, &target_normalized);
            let additional_capture = additional_targets.iter().find_map(|candidate| {
                wildcard_capture_case_insensitive(&resolved_stripped, candidate)
            });
            let matched_via_additional_target =
                direct_capture.is_none() && additional_capture.is_some();
            let capture = direct_capture.or(additional_capture);
            let Some(capture) = capture else {
                continue;
            };

            let Some(mut specifier) = apply_wildcard_capture(specifier_pattern, &capture) else {
                continue;
            };

            if specifier_pattern.contains('*')
                && !specifier_pattern.ends_with(".js")
                && !specifier_pattern.ends_with(".ts")
                && !has_source_extension(&target_pattern)
            {
                let prefer_ts_extension = allow_importing_ts_extensions
                    && !matched_via_additional_target
                    || specifier_pattern.contains('/')
                    || (package_type_module && resolved_stripped.contains("/src/"));
                if prefer_ts_extension {
                    if let Some(ext) = ts_source_extension(target_file) {
                        specifier.push_str(ext);
                    } else {
                        specifier.push_str(".js");
                    }
                } else {
                    specifier.push_str(".js");
                }
            }

            specs.push(specifier);
        }
    }

    let mut seen = FxHashSet::default();
    specs.retain(|spec| seen.insert(spec.clone()));
    specs.sort_by(compare_module_specifier_candidates);
    specs
}

fn collect_import_targets(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(text) => vec![text.to_string()],
        serde_json::Value::Array(items) => items.iter().flat_map(collect_import_targets).collect(),
        serde_json::Value::Object(map) => map.values().flat_map(collect_import_targets).collect(),
        _ => Vec::new(),
    }
}

fn collect_exports_targets(value: &serde_json::Value) -> (Vec<String>, Vec<String>) {
    let mut types = Vec::new();
    let mut defaults = Vec::new();
    collect_exports_targets_inner(value, false, &mut types, &mut defaults);
    (types, defaults)
}

fn collect_exports_targets_inner(
    value: &serde_json::Value,
    is_types_branch: bool,
    types: &mut Vec<String>,
    defaults: &mut Vec<String>,
) {
    match value {
        serde_json::Value::String(text) => {
            if is_types_branch {
                types.push(text.to_string());
            } else {
                defaults.push(text.to_string());
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_exports_targets_inner(item, is_types_branch, types, defaults);
            }
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                collect_exports_targets_inner(
                    item,
                    is_types_branch || key == "types",
                    types,
                    defaults,
                );
            }
        }
        _ => {}
    }
}

fn apply_wildcard_capture(specifier_pattern: &str, capture: &str) -> Option<String> {
    if let Some((prefix, suffix)) = specifier_pattern.split_once('*') {
        let mut spec = String::with_capacity(prefix.len() + capture.len() + suffix.len());
        spec.push_str(prefix);
        spec.push_str(capture);
        spec.push_str(suffix);
        return Some(spec);
    }

    if capture.is_empty() {
        return Some(specifier_pattern.to_string());
    }

    None
}

fn wildcard_capture_case_insensitive(pattern: &str, target: &str) -> Option<String> {
    fn capture(pattern: &str, target: &str) -> Option<String> {
        let pattern_lower = pattern.to_ascii_lowercase();
        let target_lower = target.to_ascii_lowercase();
        if let Some((prefix, suffix)) = pattern_lower.split_once('*') {
            if !target_lower.starts_with(prefix) || !target_lower.ends_with(suffix) {
                return None;
            }
            let start = prefix.len();
            let end = target_lower.len().saturating_sub(suffix.len());
            return Some(target[start..end].to_string());
        }
        (pattern_lower == target_lower).then_some(String::new())
    }

    let pattern = pattern.replace('\\', "/");
    let target = target.replace('\\', "/");

    capture(&pattern, &target)
        .or_else(|| pattern.strip_prefix('/').and_then(|p| capture(p, &target)))
        .or_else(|| target.strip_prefix('/').and_then(|t| capture(&pattern, t)))
        .or_else(|| {
            pattern
                .strip_prefix('/')
                .zip(target.strip_prefix('/'))
                .and_then(|(p, t)| capture(p, t))
        })
}

fn strip_js_ts_extension(path: &Path) -> PathBuf {
    const SOURCE_SUFFIXES: [&str; 11] = [
        ".d.ts", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs",
    ];
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.to_path_buf();
    };

    for suffix in SOURCE_SUFFIXES {
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

fn has_source_extension(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.ends_with(".d.ts")
        || normalized.ends_with(".d.mts")
        || normalized.ends_with(".d.cts")
        || normalized.ends_with(".ts")
        || normalized.ends_with(".tsx")
        || normalized.ends_with(".mts")
        || normalized.ends_with(".cts")
        || normalized.ends_with(".js")
        || normalized.ends_with(".jsx")
        || normalized.ends_with(".mjs")
        || normalized.ends_with(".cjs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_specifier_prefers_package_root_for_commonjs_main_module_entrypoint() {
        let mut project = Project::new();
        project.set_file(
            "/node_modules/pkg/package.json".to_string(),
            r#"{
  "name": "pkg",
  "version": "1.0.0",
  "main": "lib",
  "module": "lib"
}"#
            .to_string(),
        );
        project.set_file(
            "/node_modules/pkg/lib/index.js".to_string(),
            "export function foo() {}".to_string(),
        );

        assert_eq!(
            project.package_specifier_from_node_modules("/node_modules/pkg/lib/index.js"),
            Some("pkg".to_string())
        );
    }

    #[test]
    fn package_specifier_uses_subpath_for_type_module_main_entrypoint() {
        let mut project = Project::new();
        project.set_file(
            "/node_modules/pkg/package.json".to_string(),
            r#"{
  "name": "pkg",
  "version": "1.0.0",
  "main": "lib",
  "type": "module"
}"#
            .to_string(),
        );
        project.set_file(
            "/node_modules/pkg/lib/index.js".to_string(),
            "export function foo() {}".to_string(),
        );

        assert_eq!(
            project.package_specifier_from_node_modules("/node_modules/pkg/lib/index.js"),
            Some("pkg/lib".to_string())
        );
    }

    #[test]
    fn package_specifier_maps_dmts_to_mjs_without_collapsing_to_package_root() {
        let mut project = Project::new();
        project.set_file(
            "/node_modules/pkg/package.json".to_string(),
            r#"{
  "name": "pkg",
  "version": "1.0.0",
  "main": "lib"
}"#
            .to_string(),
        );
        project.set_file(
            "/node_modules/pkg/lib/index.d.mts".to_string(),
            "export declare function foo(): any;".to_string(),
        );

        assert_eq!(
            project.package_specifier_from_node_modules("/node_modules/pkg/lib/index.d.mts"),
            Some("pkg/lib/index.mjs".to_string())
        );
    }

    #[test]
    fn package_specifier_maps_dcts_to_cjs_when_no_package_json_exists() {
        let mut project = Project::new();
        project.set_file(
            "/node_modules/lit/index.d.cts".to_string(),
            "export declare function customElement(name: string): any;".to_string(),
        );

        assert_eq!(
            project.package_specifier_from_node_modules("/node_modules/lit/index.d.cts"),
            Some("lit/index.cjs".to_string())
        );
    }

    #[test]
    fn package_specifier_collapses_extensionless_root_index_to_package_name() {
        let mut project = Project::new();
        project.set_file(
            "/node_modules/bar/index.d.ts".to_string(),
            "export declare const fromBar: number;".to_string(),
        );

        assert_eq!(
            project.package_specifier_from_node_modules("/node_modules/bar/index.d.ts"),
            Some("bar".to_string())
        );
    }

    #[test]
    fn root_dirs_prefers_shortest_relative_specifier_across_roots() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "commonjs",
    "rootDirs": [".", "./some/other/root"]
  }
}"#
            .to_string(),
        );

        assert_eq!(
            project
                .root_dirs_relative_specifier_from_files("/index.ts", "/some/other/root/types.ts"),
            Some("./types".to_string())
        );

        assert_eq!(
            project
                .auto_import_module_specifiers_from_files("/index.ts", "/some/other/root/types.ts"),
            vec!["./types".to_string(), "./some/other/root/types".to_string()]
        );
    }

    #[test]
    fn path_mapping_collapses_index_suffix_for_barrel_target() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "commonjs",
    "paths": {
      "~/*": ["src/*"]
    }
  }
}"#
            .to_string(),
        );
        project.set_file("/src/dirA/thing1A.ts".to_string(), "Thing".to_string());
        project.set_file(
            "/src/dirB/index.ts".to_string(),
            "export * from \"./thing1B\";".to_string(),
        );

        assert_eq!(
            project
                .path_mapping_specifiers_from_files("/src/dirA/thing1A.ts", "/src/dirB/index.ts"),
            vec!["~/dirB".to_string()]
        );
    }

    #[test]
    fn package_imports_from_outdir_mapping_prefer_js_even_with_allow_ts_extensions() {
        let specs = package_import_specifiers_for_target(
            r##"{
  "type": "module",
  "imports": {
    "#*": {
      "types": "./types/*",
      "default": "./dist/*"
    }
  }
}"##,
            "/",
            "/src/add.ts",
            true,
            &["/dist/add".to_string(), "/types/add".to_string()],
        );

        assert_eq!(specs, vec!["#add.js".to_string()]);
    }

    #[test]
    fn auto_import_prefix_candidates_include_barrel_and_direct_path_variants() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "commonjs",
    "paths": {
      "~/*": ["src/*"]
    }
  }
}"#
            .to_string(),
        );
        project.set_file("/src/dirA/thing1A.ts".to_string(), "Thing".to_string());
        project.set_file(
            "/src/dirB/index.ts".to_string(),
            "export * from \"./thing1B\";\nexport * from \"./thing2B\";\n".to_string(),
        );
        project.set_file(
            "/src/dirB/thing1B.ts".to_string(),
            "export class Thing1B {}".to_string(),
        );
        project.set_file(
            "/src/dirB/thing2B.ts".to_string(),
            "export class Thing2B {}".to_string(),
        );

        let mut thing2_specs: Vec<String> = project
            .get_import_candidates_for_prefix("/src/dirA/thing1A.ts", "Thing")
            .into_iter()
            .filter(|candidate| candidate.local_name == "Thing2B")
            .map(|candidate| candidate.module_specifier)
            .collect();
        thing2_specs.sort();
        thing2_specs.dedup();

        assert_eq!(
            thing2_specs,
            vec!["~/dirB".to_string(), "~/dirB/thing2B".to_string()]
        );
    }

    #[test]
    fn jsconfig_paths_mapping_outranks_relative_for_shortest_preference() {
        let mut project = Project::new();
        project.set_file(
            "/package1/jsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "checkJs": true,
    "paths": {
      "package1/*": ["./*"],
      "package2/*": ["../package2/*"]
    },
    "baseUrl": "."
  }
}"#
            .to_string(),
        );
        project.set_file("/package1/file1.js".to_string(), "bar".to_string());
        project.set_file(
            "/package2/file1.js".to_string(),
            "export const bar = 0;".to_string(),
        );

        assert_eq!(
            project.auto_import_module_specifiers_from_files(
                "/package1/file1.js",
                "/package2/file1.js"
            ),
            vec![
                "package2/file1".to_string(),
                "../package2/file1.js".to_string()
            ]
        );
    }

    #[test]
    fn jsconfig_jsonc_unquoted_keys_are_supported_for_paths_mapping() {
        let mut project = Project::new();
        project.set_file(
            "/package1/jsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    checkJs: true,
    "paths": {
      "package1/*": ["./*"],
      "package2/*": ["../package2/*"]
    },
    "baseUrl": "."
  }
}"#
            .to_string(),
        );
        project.set_file("/package1/file1.js".to_string(), "bar".to_string());
        project.set_file(
            "/package2/file1.js".to_string(),
            "export const bar = 0;".to_string(),
        );

        assert_eq!(
            project.auto_import_module_specifiers_from_files(
                "/package1/file1.js",
                "/package2/file1.js"
            ),
            vec![
                "package2/file1".to_string(),
                "../package2/file1.js".to_string()
            ]
        );
    }

    #[test]
    fn shortest_prefers_relative_over_paths_when_depth_matches() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "preserve",
    "paths": {
      "@app/*": ["./src/*"]
    }
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/src/utils.ts".to_string(),
            "export function add(a: number, b: number) {}".to_string(),
        );
        project.set_file("/src/index.ts".to_string(), "ad".to_string());

        assert_eq!(
            project.auto_import_module_specifiers_from_files("/src/index.ts", "/src/utils.ts"),
            vec!["./utils".to_string(), "@app/utils".to_string()]
        );
    }

    #[test]
    fn shortest_keeps_path_mapping_ahead_of_parent_relative_specifier() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "paths": {
      "@root/*": ["${configDir}/src/*"]
    }
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/src/one.ts".to_string(),
            "export const one = 1;".to_string(),
        );
        project.set_file("/src/foo/two.ts".to_string(), "one".to_string());

        assert_eq!(
            project.auto_import_module_specifiers_from_files("/src/foo/two.ts", "/src/one.ts"),
            vec!["@root/one".to_string(), "../one".to_string()]
        );
    }

    #[test]
    fn auto_imports_disabled_for_module_none_es5() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "none",
    "target": "es5"
  }
}"#
            .to_string(),
        );

        assert!(!project.auto_imports_allowed_for_file("/index.ts"));
    }

    #[test]
    fn auto_imports_enabled_for_module_none_es2015() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "none",
    "target": "es2015"
  }
}"#
            .to_string(),
        );

        assert!(project.auto_imports_allowed_for_file("/index.ts"));
    }

    #[test]
    fn auto_imports_disabled_from_fourslash_directives_for_module_none_es5() {
        let mut project = Project::new();
        project.set_file(
            "/index.ts".to_string(),
            "// @module: none\n// @target: es5\nx".to_string(),
        );

        assert!(!project.auto_imports_allowed_for_file("/index.ts"));
    }

    #[test]
    fn auto_imports_enabled_from_fourslash_directives_for_module_none_es2015() {
        let mut project = Project::new();
        project.set_file(
            "/index.ts".to_string(),
            "// @module: none\n// @target: es2015\nx".to_string(),
        );

        assert!(project.auto_imports_allowed_for_file("/index.ts"));
    }

    #[test]
    fn auto_imports_disabled_from_fourslash_directives_in_sibling_file() {
        let mut project = Project::new();
        project.set_file(
            "/fourslash.ts".to_string(),
            "// @module: none\n// @target: es5\n".to_string(),
        );
        project.set_file("/index.ts".to_string(), "x".to_string());

        assert!(!project.auto_imports_allowed_for_file("/index.ts"));
    }

    #[test]
    fn auto_import_candidates_include_ambient_module_exports() {
        let mut project = Project::new();
        project.set_file(
            "/node_modules/lib/index.d.ts".to_string(),
            "declare module \"ambient\" { export const x: number; }\ndeclare module \"ambient/utils\" { export const x: number; }\n".to_string(),
        );
        project.set_file("/index.ts".to_string(), "x".to_string());

        let mut specs: Vec<String> = project
            .get_import_candidates_for_prefix("/index.ts", "x")
            .into_iter()
            .map(|candidate| candidate.module_specifier)
            .collect();
        specs.sort();
        specs.dedup();

        assert_eq!(
            specs,
            vec!["ambient".to_string(), "ambient/utils".to_string()]
        );
    }

    #[test]
    fn auto_import_candidates_include_commonjs_exports_from_js_files() {
        let mut project = Project::new();
        project.set_file(
            "/tsconfig.json".to_string(),
            r#"{
  "compilerOptions": {
    "module": "node18",
    "allowJs": true,
    "checkJs": true
  }
}"#
            .to_string(),
        );
        project.set_file(
            "/matrix.js".to_string(),
            "exports.variants = [];".to_string(),
        );
        project.set_file("/main.js".to_string(), "variants".to_string());

        let specs: Vec<String> = project
            .get_import_candidates_for_prefix("/main.js", "variants")
            .into_iter()
            .filter(|candidate| candidate.local_name == "variants")
            .map(|candidate| candidate.module_specifier)
            .collect();

        assert!(
            specs.iter().any(|spec| spec == "./matrix.js"),
            "expected './matrix.js' auto-import candidate, got {specs:?}"
        );
    }

    #[test]
    fn mts_auto_import_sources_stay_extensionless_even_with_js_imports() {
        let mut project = Project::new();
        project.set_file(
            "/mod.ts".to_string(),
            "export interface I {}\nexport class C {}\n".to_string(),
        );
        project.set_file(
            "/a.mts".to_string(),
            "import type { I } from \"./mod.js\";\nconst x: I = new C();\n".to_string(),
        );

        let specifiers = project.auto_import_module_specifiers_from_files("/a.mts", "/mod.ts");
        assert_eq!(specifiers, vec!["./mod".to_string()]);
    }

    #[test]
    fn ambient_module_auto_import_candidates_respect_specifier_exclude_regexes() {
        let mut project = Project::new();
        project.set_auto_import_specifier_exclude_regexes(vec!["utils".to_string()]);
        project.set_file(
            "/node_modules/lib/index.d.ts".to_string(),
            "declare module \"ambient\" { export const x: number; }\ndeclare module \"ambient/utils\" { export const x: number; }\n".to_string(),
        );
        project.set_file("/index.ts".to_string(), "x".to_string());

        let mut specs: Vec<String> = project
            .get_import_candidates_for_prefix("/index.ts", "x")
            .into_iter()
            .map(|candidate| candidate.module_specifier)
            .collect();
        specs.sort();
        specs.dedup();

        assert_eq!(specs, vec!["ambient".to_string()]);
    }
}
