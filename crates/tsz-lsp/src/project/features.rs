//! LSP feature dispatch methods for `Project`.
//!
//! Each method looks up the target file, delegates to the appropriate LSP
//! provider, and records performance timing.  This keeps the core `Project`
//! struct (file management, dependencies, configuration) in `mod.rs` while
//! the feature-facing surface lives here.

use rustc_hash::{FxHashMap, FxHashSet};
use web_time::Instant;

use super::{Project, ProjectRequestKind};
use crate::code_actions::{CodeAction, CodeActionContext, CodeActionKind, CodeActionProvider};
use crate::code_lens::CodeLens;
use crate::completions::CompletionItem;
use crate::definition::GoToDefinition;
use crate::diagnostics::LspDiagnostic;
use crate::hover::HoverInfo;
use crate::resolver::ScopeCacheStats;
use crate::signature_help::SignatureHelp;
use crate::utils::find_node_at_offset;
use crate::workspace_symbols::{SymbolInformation, WorkspaceSymbolsProvider};
use tsz_common::position::{Location, Position, Range};
use tsz_scanner::SyntaxKind;

impl Project {
    /// Go to definition within a single file.
    pub fn get_definition(&mut self, file_name: &str, position: Position) -> Option<Vec<Location>> {
        let start = Instant::now();
        let mut scope_stats = ScopeCacheStats::default();
        let result = (|| {
            {
                let file = self.files.get(file_name)?;
                if let Some(definitions) = self.definition_from_import(file, position) {
                    return Some(definitions);
                }
            }

            let file = self.files.get_mut(file_name)?;
            let arena = file.parser.get_arena();
            let binder = &file.binder;
            let line_map = &file.line_map;
            let source_text = file.parser.get_source_text();
            let file_name = file.file_name.clone();
            let root = file.root;
            let goto_def = GoToDefinition::new(arena, binder, line_map, file_name, source_text);
            goto_def.get_definition_with_scope_cache(
                root,
                position,
                &mut file.scope_cache,
                Some(&mut scope_stats),
            )
        })();

        self.performance
            .record(ProjectRequestKind::Definition, start.elapsed(), scope_stats);

        result
    }

    /// Hover within a single file.
    pub fn get_hover(&mut self, file_name: &str, position: Position) -> Option<HoverInfo> {
        let start = Instant::now();
        let mut scope_stats = ScopeCacheStats::default();
        let result = self
            .files
            .get_mut(file_name)?
            .get_hover_with_stats(position, Some(&mut scope_stats));

        self.performance
            .record(ProjectRequestKind::Hover, start.elapsed(), scope_stats);

        result
    }

    /// Signature help within a single file.
    pub fn get_signature_help(
        &mut self,
        file_name: &str,
        position: Position,
    ) -> Option<SignatureHelp> {
        let start = Instant::now();
        let mut scope_stats = ScopeCacheStats::default();
        let result = self
            .files
            .get_mut(file_name)?
            .get_signature_help_with_stats(position, Some(&mut scope_stats));

        self.performance.record(
            ProjectRequestKind::SignatureHelp,
            start.elapsed(),
            scope_stats,
        );

        result
    }

    /// Completions within a single file.
    pub fn get_completions(
        &mut self,
        file_name: &str,
        position: Position,
    ) -> Option<Vec<CompletionItem>> {
        let start = Instant::now();
        let mut scope_stats = ScopeCacheStats::default();
        let mut completions = {
            let file = self.files.get_mut(file_name)?;
            file.get_completions_with_stats(position, Some(&mut scope_stats))
                .unwrap_or_default()
        };

        let existing_file_symbols: FxHashSet<String> = self
            .files
            .get(file_name)
            .map(|file| {
                file.binder
                    .file_locals
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect()
            })
            .unwrap_or_default();

        let (missing_name, skip_auto_import) = {
            let file = self.files.get(file_name)?;
            if let Some((node_idx, name)) = self.identifier_at_position(file, position) {
                let skip = self.is_member_access_node(file.arena(), node_idx);
                (Some(name), skip)
            } else {
                let offset = file
                    .line_map()
                    .position_to_offset(position, file.source_text())
                    .unwrap_or(0) as usize;
                let mut node_idx = find_node_at_offset(file.arena(), offset as u32);
                if node_idx.is_none() && offset > 0 {
                    node_idx = find_node_at_offset(file.arena(), (offset - 1) as u32);
                }
                let in_string_literal = file.arena().get(node_idx).is_some_and(|node| {
                    node.kind == SyntaxKind::StringLiteral as u16
                        || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                        || node.kind == SyntaxKind::TemplateHead as u16
                        || node.kind == SyntaxKind::TemplateMiddle as u16
                        || node.kind == SyntaxKind::TemplateTail as u16
                });
                let source = file.source_text().as_bytes();
                let mut idx = offset.min(source.len());
                while idx > 0 && source[idx - 1].is_ascii_whitespace() {
                    idx -= 1;
                }
                let skip = in_string_literal || (idx > 0 && source[idx - 1] == b'.');
                (None, skip)
            }
        };

        if !skip_auto_import {
            let file = self.files.get(file_name)?;
            let mut candidates = Vec::new();
            let mut seen = FxHashSet::default();
            let prefix = missing_name.unwrap_or_default();

            // Use prefix matching for better completion UX
            // If the missing_name is not in existing completions, try to find symbols
            // that start with this prefix (e.g., "use" → "useEffect", "useState")
            self.collect_import_candidates_for_prefix(
                file,
                &prefix,
                &existing_file_symbols,
                &mut candidates,
                &mut seen,
            );
            candidates.sort_by(|a, b| {
                let a_segments = a.module_specifier.matches('/').count();
                let b_segments = b.module_specifier.matches('/').count();
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
                let index_penalty = |candidate: &str| -> u8 {
                    if candidate == "." || candidate == ".." || candidate.ends_with("/index") {
                        1
                    } else {
                        0
                    }
                };
                a.local_name
                    .cmp(&b.local_name)
                    .then_with(|| a_segments.cmp(&b_segments))
                    .then_with(|| {
                        candidate_rank(&a.module_specifier)
                            .cmp(&candidate_rank(&b.module_specifier))
                    })
                    .then_with(|| {
                        index_penalty(&a.module_specifier).cmp(&index_penalty(&b.module_specifier))
                    })
                    .then_with(|| a.module_specifier.len().cmp(&b.module_specifier.len()))
                    .then_with(|| a.module_specifier.cmp(&b.module_specifier))
            });

            // Create CodeActionProvider for generating import edits
            use crate::code_actions::CodeActionProvider;
            let code_action_provider = CodeActionProvider::new(
                file.arena(),
                &file.binder,
                &file.line_map,
                file.file_name.clone(),
                file.source_text(),
            );

            for candidate in candidates {
                if existing_file_symbols.contains(&candidate.local_name) {
                    continue;
                }

                // Generate additional text edits for auto-import
                if let Some(edits) = code_action_provider.build_auto_import_edit_for_completion(
                    file.root(),
                    &candidate,
                    position,
                ) {
                    let mut item = self.completion_from_import_candidate(&candidate);
                    item = item.with_additional_edits(edits);
                    completions.push(item);
                }
            }
        }

        let result = if completions.is_empty() {
            None
        } else {
            Some(completions)
        };

        self.performance.record(
            ProjectRequestKind::Completions,
            start.elapsed(),
            scope_stats,
        );

        result
    }

    /// Diagnostics within a single file.
    pub fn get_diagnostics(&mut self, file_name: &str) -> Option<Vec<LspDiagnostic>> {
        let start = Instant::now();
        let scope_stats = ScopeCacheStats::default();
        let result = {
            let file = self.files.get_mut(file_name)?;
            Some(file.get_diagnostics())
        };

        self.performance.record(
            ProjectRequestKind::Diagnostics,
            start.elapsed(),
            scope_stats,
        );

        result
    }

    /// Resolve import candidates for missing-name diagnostics in a file.
    pub fn get_import_candidates_for_diagnostics(
        &self,
        file_name: &str,
        diagnostics: &[LspDiagnostic],
    ) -> Vec<crate::code_actions::ImportCandidate> {
        let Some(file) = self.files.get(file_name) else {
            return Vec::new();
        };
        self.import_candidates_for_diagnostics(file, diagnostics)
    }

    /// Resolve auto-import candidates by symbol prefix.
    pub fn get_import_candidates_for_prefix(
        &self,
        file_name: &str,
        prefix: &str,
    ) -> Vec<crate::code_actions::ImportCandidate> {
        let Some(file) = self.files.get(file_name) else {
            return Vec::new();
        };

        let mut output = Vec::new();
        let mut seen = FxHashSet::default();
        let existing = FxHashSet::default();
        self.collect_import_candidates_for_prefix(file, prefix, &existing, &mut output, &mut seen);
        output
    }

    /// Get diagnostics for all files that have stale (dirty) diagnostics.
    ///
    /// This method should be called after `update_file()` to provide diagnostics
    /// for all files that were affected by the change (transitively).
    ///
    /// Returns a map of `file_name` -> diagnostics for all files with dirty flags.
    pub fn get_stale_diagnostics(&mut self) -> FxHashMap<String, Vec<LspDiagnostic>> {
        let mut result = FxHashMap::default();

        // Collect all file names first to avoid borrow issues
        let file_names: Vec<String> = self.files.keys().cloned().collect();

        for file_name in file_names {
            if let Some(file) = self.files.get(&file_name)
                && file.diagnostics_dirty
                && let Some(diagnostics) = self.get_diagnostics(&file_name)
            {
                result.insert(file_name, diagnostics);
            }
        }

        result
    }

    /// Get code lenses for a file (project-aware).
    pub fn get_code_lenses(&self, file_name: &str) -> Option<Vec<CodeLens>> {
        let file = self.files.get(file_name)?;

        use crate::code_lens::CodeLensProvider;
        let provider = CodeLensProvider::new(
            file.arena(),
            file.binder(),
            file.line_map(),
            file.file_name().to_string(),
            file.source_text(),
        );

        Some(provider.provide_code_lenses(file.root()))
    }

    /// Resolve a code lens by computing its command (project-aware).
    ///
    /// This uses project-wide `find_references` for accurate reference counts.
    pub fn resolve_code_lens(&mut self, file_name: &str, lens: &CodeLens) -> Option<CodeLens> {
        let _file = self.files.get(file_name)?;
        let data = lens.data.as_ref()?;

        match data.kind {
            crate::code_lens::CodeLensKind::References => {
                // Use project-wide find_references for accurate counts
                let position = data.position;
                let references = self.find_references(file_name, position)?;

                // Count references (subtract 1 if declaration is included)
                let ref_count = if references.is_empty() {
                    0
                } else {
                    // Check if any reference is at the same position as the declaration
                    let has_decl_reference = references
                        .iter()
                        .any(|r| r.range.start == position && r.range.end == position);
                    references.len() - usize::from(has_decl_reference)
                };

                let title = if ref_count == 1 {
                    "1 reference".to_string()
                } else {
                    format!("{ref_count} references")
                };

                let command = crate::code_lens::CodeLensCommand {
                    title,
                    command: "editor.action.showReferences".to_string(),
                    arguments: Some(vec![
                        serde_json::json!(data.file_path),
                        serde_json::json!({
                            "line": data.position.line,
                            "character": data.position.character
                        }),
                        serde_json::json!(
                            references
                                .into_iter()
                                .map(|loc| serde_json::json!({
                                    "uri": loc.file_path,
                                    "range": loc.range
                                }))
                                .collect::<Vec<_>>()
                        ),
                    ]),
                };

                Some(CodeLens {
                    range: lens.range,
                    command: Some(command),
                    data: None,
                })
            }
            crate::code_lens::CodeLensKind::Implementations => {
                // Use project-wide get_implementations
                let position = data.position;
                let implementations = self.get_implementations(file_name, position)?;

                let count = implementations.len();
                let title = if count == 1 {
                    "1 implementation".to_string()
                } else {
                    format!("{count} implementations")
                };

                let command = crate::code_lens::CodeLensCommand {
                    title,
                    command: "editor.action.goToImplementation".to_string(),
                    arguments: Some(vec![
                        serde_json::json!(data.file_path),
                        serde_json::json!({
                            "line": data.position.line,
                            "character": data.position.character
                        }),
                    ]),
                };

                Some(CodeLens {
                    range: lens.range,
                    command: Some(command),
                    data: None,
                })
            }
            _ => Some(lens.clone()),
        }
    }

    /// Code actions for a file (project-aware).
    pub fn get_code_actions(
        &self,
        file_name: &str,
        range: Range,
        diagnostics: Vec<LspDiagnostic>,
        only: Option<Vec<CodeActionKind>>,
    ) -> Option<Vec<CodeAction>> {
        let file = self.files.get(file_name)?;
        let import_candidates = self.import_candidates_for_diagnostics(file, &diagnostics);

        let provider = CodeActionProvider::new(
            file.arena(),
            file.binder(),
            file.line_map(),
            file.file_name().to_string(),
            file.source_text(),
        );

        let actions = provider.provide_code_actions(
            file.root(),
            range,
            CodeActionContext {
                diagnostics,
                only,
                import_candidates,
            },
        );

        if actions.is_empty() {
            None
        } else {
            Some(actions)
        }
    }

    /// Search for symbols across the entire project.
    ///
    /// This implements the LSP `workspace/symbol` request (Cmd+T / Ctrl+T in most editors).
    /// Returns symbols matching the given query string, sorted by relevance:
    /// 1. Exact matches (case-insensitive)
    /// 2. Prefix matches
    /// 3. Substring matches
    ///
    /// At most 100 results are returned.
    ///
    /// # Arguments
    /// * `query` - The search query string. An empty query returns no results.
    ///
    /// # Returns
    /// A vector of `SymbolInformation` for matching symbols, sorted by relevance.
    pub fn get_workspace_symbols(&self, query: &str) -> Vec<SymbolInformation> {
        let provider = WorkspaceSymbolsProvider::new(&self.symbol_index);
        provider.find_symbols(query)
    }

    /// Prepare type hierarchy for a symbol (project-aware).
    pub fn prepare_type_hierarchy(
        &self,
        file_name: &str,
        position: Position,
    ) -> Option<crate::type_hierarchy::TypeHierarchyItem> {
        let file = self.files.get(file_name)?;

        use crate::type_hierarchy::TypeHierarchyProvider;
        let provider = TypeHierarchyProvider::new(
            file.arena(),
            file.binder(),
            file.line_map(),
            file.file_name().to_string(),
            file.source_text(),
        );

        provider.prepare(file.root(), position)
    }

    /// Get supertypes for a symbol (file-local only for now).
    ///
    /// TODO: Extend to search across all files using `SymbolIndex`.
    pub fn supertypes(
        &self,
        file_name: &str,
        position: Position,
    ) -> Vec<crate::type_hierarchy::TypeHierarchyItem> {
        let file = match self.files.get(file_name) {
            Some(f) => f,
            None => return Vec::new(),
        };

        use crate::type_hierarchy::TypeHierarchyProvider;
        let provider = TypeHierarchyProvider::new(
            file.arena(),
            file.binder(),
            file.line_map(),
            file.file_name().to_string(),
            file.source_text(),
        );

        provider.supertypes(file.root(), position)
    }

    /// Get subtypes for a symbol (file-local only for now).
    ///
    /// TODO: Extend to search across all files using `SymbolIndex` heritage clauses.
    pub fn subtypes(
        &self,
        file_name: &str,
        position: Position,
    ) -> Vec<crate::type_hierarchy::TypeHierarchyItem> {
        let file = match self.files.get(file_name) {
            Some(f) => f,
            None => return Vec::new(),
        };

        use crate::type_hierarchy::TypeHierarchyProvider;
        let provider = TypeHierarchyProvider::new(
            file.arena(),
            file.binder(),
            file.line_map(),
            file.file_name().to_string(),
            file.source_text(),
        );

        provider.subtypes(file.root(), position)
    }
}
