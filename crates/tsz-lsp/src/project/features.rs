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
use crate::completions::{CompletionItem, CompletionItemData};
use crate::diagnostics::LspDiagnostic;
use crate::editor_decorations::code_lens::CodeLens;
use crate::hover::HoverInfo;
use crate::navigation::definition::GoToDefinition;
use crate::resolver::ScopeCacheStats;
use crate::signature_help::SignatureHelp;
use crate::symbols::workspace_symbols::{SymbolInformation, WorkspaceSymbolsProvider};
use crate::utils::find_node_at_offset;
use tsz_common::position::{Location, Position, Range};
use tsz_scanner::SyntaxKind;

impl Project {
    /// Go to type definition within a single file.
    pub fn get_type_definition(
        &self,
        file_name: &str,
        position: Position,
    ) -> Option<Vec<Location>> {
        let file = self.files.get(file_name)?;

        use crate::navigation::type_definition::TypeDefinitionProvider;
        let provider = TypeDefinitionProvider::new(
            file.arena(),
            file.binder(),
            file.line_map(),
            file.file_name().to_string(),
            file.source_text(),
        );

        provider.get_type_definition(file.root(), position)
    }

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
            // Attach resolve data to each item so completionItem/resolve
            // can look up documentation using the original file and position.
            let resolve_data = CompletionItemData {
                file_name: file_name.to_string(),
                position,
            };
            for item in &mut completions {
                if item.data.is_none() {
                    item.data = Some(resolve_data.clone());
                }
            }
            Some(completions)
        };

        self.performance.record(
            ProjectRequestKind::Completions,
            start.elapsed(),
            scope_stats,
        );

        result
    }

    /// Resolve a completion item by filling in documentation and detail.
    ///
    /// This implements the LSP `completionItem/resolve` request. The initial
    /// completion response returns items without heavy fields (documentation, detail).
    /// When the user focuses on an item, the editor sends a resolve request and
    /// this method computes hover info to fill in the missing data.
    ///
    /// Returns `(detail, documentation)` — both optional.
    pub fn resolve_completion(
        &self,
        file_name: &str,
        label: &str,
    ) -> Option<(Option<String>, Option<String>)> {
        let file = self.files.get(file_name)?;
        let arena = file.arena();
        let binder = file.binder();
        let source_text = file.source_text();
        let root = file.root();

        // Look up the symbol by name in the binder's file locals
        let sym_id = binder.file_locals.get(label);

        if let Some(sid) = sym_id
            && let Some(symbol) = binder.get_symbol(sid)
        {
            // Extract JSDoc documentation from the declaration
            let documentation = symbol
                .declarations
                .first()
                .map(|decl_idx| crate::jsdoc::jsdoc_for_node(arena, root, *decl_idx, source_text));
            let documentation = documentation.filter(|s| !s.is_empty());

            return Some((None, documentation));
        }

        None
    }

    /// Position-aware resolve using `CompletionItemData`.
    ///
    /// Falls back to label-based lookup but can use the original position
    /// to provide richer type information via hover.
    pub fn resolve_completion_with_data(
        &mut self,
        data: &CompletionItemData,
        label: &str,
    ) -> Option<(Option<String>, Option<String>)> {
        // First try label-based lookup for documentation
        let label_result = self.resolve_completion(&data.file_name, label);

        // Also try hover at the original position for type detail
        let hover_detail = self
            .get_hover(&data.file_name, data.position)
            .and_then(|info| {
                if info.display_string.is_empty() {
                    None
                } else {
                    Some(info.display_string)
                }
            });

        match (label_result, hover_detail) {
            (Some((_, doc)), Some(detail)) => Some((Some(detail), doc)),
            (Some(result), None) => Some(result),
            (None, Some(detail)) => Some((Some(detail), None)),
            (None, None) => None,
        }
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

        use crate::editor_decorations::code_lens::CodeLensProvider;
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
        self.files.get(file_name)?;
        let data = lens.data.as_ref()?;

        match data.kind {
            crate::editor_decorations::code_lens::CodeLensKind::References => {
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

                let command = crate::editor_decorations::code_lens::CodeLensCommand {
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
            crate::editor_decorations::code_lens::CodeLensKind::Implementations => {
                // Use project-wide get_implementations
                let position = data.position;
                let implementations = self.get_implementations(file_name, position)?;

                let count = implementations.len();
                let title = if count == 1 {
                    "1 implementation".to_string()
                } else {
                    format!("{count} implementations")
                };

                let command = crate::editor_decorations::code_lens::CodeLensCommand {
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
    ) -> Option<crate::hierarchy::type_hierarchy::TypeHierarchyItem> {
        let file = self.files.get(file_name)?;

        use crate::hierarchy::type_hierarchy::TypeHierarchyProvider;
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
    ) -> Vec<crate::hierarchy::type_hierarchy::TypeHierarchyItem> {
        let file = match self.files.get(file_name) {
            Some(f) => f,
            None => return Vec::new(),
        };

        use crate::hierarchy::type_hierarchy::TypeHierarchyProvider;
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
    ) -> Vec<crate::hierarchy::type_hierarchy::TypeHierarchyItem> {
        let file = match self.files.get(file_name) {
            Some(f) => f,
            None => return Vec::new(),
        };

        use crate::hierarchy::type_hierarchy::TypeHierarchyProvider;
        let provider = TypeHierarchyProvider::new(
            file.arena(),
            file.binder(),
            file.line_map(),
            file.file_name().to_string(),
            file.source_text(),
        );

        provider.subtypes(file.root(), position)
    }

    // ── Provider-based features (document-local, no type checking) ──────

    /// Get document symbols for a file.
    pub fn get_document_symbols(
        &self,
        file_name: &str,
    ) -> Option<Vec<crate::symbols::DocumentSymbol>> {
        let file = self.files.get(file_name)?;
        let provider = crate::symbols::DocumentSymbolProvider::new(
            file.arena(),
            file.line_map(),
            file.source_text(),
        );
        Some(provider.get_document_symbols(file.root()))
    }

    /// Get folding ranges for a file.
    pub fn get_folding_ranges(
        &self,
        file_name: &str,
    ) -> Option<Vec<crate::editor_ranges::folding::FoldingRange>> {
        let file = self.files.get(file_name)?;
        let provider = crate::editor_ranges::folding::FoldingRangeProvider::new(
            file.arena(),
            file.line_map(),
            file.source_text(),
        );
        Some(provider.get_folding_ranges(file.root()))
    }

    /// Get selection ranges for given positions in a file.
    pub fn get_selection_ranges(
        &self,
        file_name: &str,
        positions: &[Position],
    ) -> Option<Vec<Option<crate::editor_ranges::selection_range::SelectionRange>>> {
        let file = self.files.get(file_name)?;
        let provider = crate::editor_ranges::selection_range::SelectionRangeProvider::new(
            file.arena(),
            file.line_map(),
            file.source_text(),
        );
        Some(provider.get_selection_ranges(positions))
    }

    /// Get semantic tokens for a file (encoded as delta array).
    pub fn get_semantic_tokens_full(&self, file_name: &str) -> Option<Vec<u32>> {
        let file = self.files.get(file_name)?;
        let mut provider = crate::highlighting::semantic_tokens::SemanticTokensProvider::new(
            file.arena(),
            file.binder(),
            file.line_map(),
            file.source_text(),
        );
        Some(provider.get_semantic_tokens(file.root()))
    }

    /// Get semantic tokens for a specific range in a file (encoded as delta array).
    pub fn get_semantic_tokens_range(&self, file_name: &str, range: Range) -> Option<Vec<u32>> {
        let file = self.files.get(file_name)?;
        let mut provider = crate::highlighting::semantic_tokens::SemanticTokensProvider::new(
            file.arena(),
            file.binder(),
            file.line_map(),
            file.source_text(),
        );
        Some(provider.get_semantic_tokens_range(file.root(), &range))
    }

    /// Get document highlights for a position in a file.
    pub fn get_document_highlighting(
        &self,
        file_name: &str,
        position: Position,
    ) -> Option<Vec<crate::highlighting::DocumentHighlight>> {
        let file = self.files.get(file_name)?;
        let provider = crate::highlighting::DocumentHighlightProvider::new(
            file.arena(),
            file.binder(),
            file.line_map(),
            file.source_text(),
        );
        provider.get_document_highlights(file.root(), position)
    }

    /// Get inlay hints for a range in a file.
    pub fn get_inlay_hints(
        &self,
        file_name: &str,
        range: Range,
    ) -> Option<Vec<crate::editor_decorations::inlay_hints::InlayHint>> {
        let file = self.files.get(file_name)?;
        let provider = crate::editor_decorations::inlay_hints::InlayHintsProvider {
            arena: file.arena(),
            binder: file.binder(),
            line_map: file.line_map(),
            source: file.source_text(),
            interner: &file.type_interner,
            file_name: file.file_name().to_string(),
        };
        Some(provider.provide_inlay_hints(file.root(), range))
    }

    /// Prepare call hierarchy at a position.
    pub fn prepare_call_hierarchy(
        &self,
        file_name: &str,
        position: Position,
    ) -> Option<crate::hierarchy::call_hierarchy::CallHierarchyItem> {
        let file = self.files.get(file_name)?;
        let provider = crate::hierarchy::call_hierarchy::CallHierarchyProvider::new(
            file.arena(),
            file.binder(),
            file.line_map(),
            file.file_name().to_string(),
            file.source_text(),
        );
        provider.prepare(file.root(), position)
    }

    /// Get incoming calls at a position.
    pub fn get_incoming_calls(
        &self,
        file_name: &str,
        position: Position,
    ) -> Vec<crate::hierarchy::call_hierarchy::CallHierarchyIncomingCall> {
        let file = match self.files.get(file_name) {
            Some(f) => f,
            None => return Vec::new(),
        };
        let provider = crate::hierarchy::call_hierarchy::CallHierarchyProvider::new(
            file.arena(),
            file.binder(),
            file.line_map(),
            file.file_name().to_string(),
            file.source_text(),
        );
        provider.incoming_calls(file.root(), position)
    }

    /// Get outgoing calls at a position.
    pub fn get_outgoing_calls(
        &self,
        file_name: &str,
        position: Position,
    ) -> Vec<crate::hierarchy::call_hierarchy::CallHierarchyOutgoingCall> {
        let file = match self.files.get(file_name) {
            Some(f) => f,
            None => return Vec::new(),
        };
        let provider = crate::hierarchy::call_hierarchy::CallHierarchyProvider::new(
            file.arena(),
            file.binder(),
            file.line_map(),
            file.file_name().to_string(),
            file.source_text(),
        );
        provider.outgoing_calls(file.root(), position)
    }

    /// Get document colors for a file (hex color literals in strings).
    pub fn get_document_colors(
        &self,
        file_name: &str,
    ) -> Option<Vec<crate::editor_decorations::document_color::ColorInformation>> {
        let file = self.files.get(file_name)?;
        let provider = crate::editor_decorations::document_color::DocumentColorProvider::new(
            file.arena(),
            file.line_map(),
            file.source_text(),
        );
        Some(provider.provide_document_colors(file.root()))
    }

    /// Get document links for a file.
    pub fn get_document_links(
        &self,
        file_name: &str,
    ) -> Option<Vec<crate::document_links::DocumentLink>> {
        let file = self.files.get(file_name)?;
        let provider = crate::document_links::DocumentLinkProvider::new(
            file.arena(),
            file.line_map(),
            file.source_text(),
        );
        Some(provider.provide_document_links(file.root()))
    }

    /// Get linked editing ranges for JSX tags.
    pub fn get_linked_editing_ranges(
        &self,
        file_name: &str,
        position: Position,
    ) -> Option<crate::rename::linked_editing::LinkedEditingRanges> {
        let file = self.files.get(file_name)?;
        let provider = crate::rename::linked_editing::LinkedEditingProvider::new(
            file.arena(),
            file.line_map(),
            file.source_text(),
        );
        provider.provide_linked_editing_ranges(file.root(), position)
    }

    /// Compute workspace edits for file renames.
    ///
    /// When a file is renamed/moved, this finds all import specifiers across the
    /// project that referenced the old path and produces text edits to update them.
    pub fn get_file_rename_edits(
        &self,
        old_path: &str,
        new_path: &str,
    ) -> FxHashMap<String, Vec<crate::rename::TextEdit>> {
        let mut workspace_edits: FxHashMap<String, Vec<crate::rename::TextEdit>> =
            FxHashMap::default();

        // Normalize paths by stripping common extensions for comparison
        let strip_ext = |p: &str| -> String {
            let p = p
                .strip_suffix(".ts")
                .or_else(|| p.strip_suffix(".tsx"))
                .or_else(|| p.strip_suffix(".js"))
                .or_else(|| p.strip_suffix(".jsx"))
                .or_else(|| p.strip_suffix(".mts"))
                .or_else(|| p.strip_suffix(".cts"))
                .unwrap_or(p);
            p.to_string()
        };

        let old_base = strip_ext(old_path);
        let new_base = strip_ext(new_path);

        for (file_name, file) in &self.files {
            let provider = crate::rename::file_rename::FileRenameProvider::new(
                file.arena(),
                file.line_map(),
                file.source_text(),
            );
            let locations = provider.find_import_specifier_nodes(file.root());

            for loc in locations {
                // Check if this import specifier references the old file
                // Resolve relative specifier to absolute path
                let resolved = self.resolve_specifier(file_name, &loc.current_specifier);
                let resolved_base = strip_ext(&resolved);

                if resolved_base == old_base {
                    // Compute the new relative specifier
                    let new_specifier = self.compute_relative_specifier(file_name, &new_base);

                    // The range includes quotes, so build the edit with quotes
                    let quote = if loc.current_specifier.contains('\'') {
                        '\''
                    } else {
                        '"'
                    };
                    // Adjust range to only replace the content inside quotes
                    let inner_range = tsz_common::position::Range::new(
                        tsz_common::position::Position::new(
                            loc.range.start.line,
                            loc.range.start.character + 1,
                        ),
                        tsz_common::position::Position::new(
                            loc.range.end.line,
                            loc.range.end.character.saturating_sub(1),
                        ),
                    );
                    let _ = quote; // suppress unused warning, quote is in original text
                    workspace_edits.entry(file_name.clone()).or_default().push(
                        crate::rename::TextEdit {
                            range: inner_range,
                            new_text: new_specifier,
                        },
                    );
                }
            }
        }

        workspace_edits
    }

    /// Resolve a module specifier relative to the importing file.
    fn resolve_specifier(&self, from_file: &str, specifier: &str) -> String {
        if !specifier.starts_with('.') {
            // Bare specifier (e.g., "react") - return as-is
            return specifier.to_string();
        }
        // Resolve relative to the directory of from_file
        let dir = if let Some(idx) = from_file.rfind('/') {
            &from_file[..idx]
        } else {
            "."
        };

        let mut parts: Vec<&str> = dir.split('/').collect();
        for segment in specifier.split('/') {
            match segment {
                "." => {}
                ".." => {
                    parts.pop();
                }
                s => parts.push(s),
            }
        }
        parts.join("/")
    }

    /// Compute a relative module specifier from one file to another.
    fn compute_relative_specifier(&self, from_file: &str, to_path: &str) -> String {
        let from_dir = if let Some(idx) = from_file.rfind('/') {
            &from_file[..idx]
        } else {
            "."
        };

        // Split into path components
        let from_parts: Vec<&str> = from_dir.split('/').collect();
        let to_parts: Vec<&str> = to_path.split('/').collect();

        // Find common prefix length
        let common = from_parts
            .iter()
            .zip(to_parts.iter())
            .take_while(|(a, b)| a == b)
            .count();

        let ups = from_parts.len() - common;
        let mut result = String::new();
        if ups == 0 {
            result.push_str("./");
        } else {
            for _ in 0..ups {
                result.push_str("../");
            }
        }

        let remaining: Vec<&str> = to_parts[common..].to_vec();
        result.push_str(&remaining.join("/"));

        result
    }

    /// Format a document using the built-in formatter.
    pub fn format_document(
        &self,
        file_name: &str,
        options: &crate::formatting::FormattingOptions,
    ) -> Option<Result<Vec<crate::formatting::TextEdit>, String>> {
        let file = self.files.get(file_name)?;
        Some(
            crate::formatting::DocumentFormattingProvider::format_document(
                file_name,
                file.source_text(),
                options,
            ),
        )
    }
}
