//! Language Service API
//!
//! Provides TypeScript-compatible language service functionality for IDE features.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::binder::BinderState;
use crate::lsp::completions::{CompletionItemKind, Completions};
use crate::lsp::definition::GoToDefinition;
use crate::lsp::hover::HoverProvider;
use crate::lsp::position::{LineMap, Position};
use crate::lsp::references::FindReferences;
use crate::parser::{NodeArena, NodeIndex, ParserState};
use crate::solver::TypeInterner;

/// Language Service for a single file
///
/// Provides IDE-like features for a TypeScript/JavaScript file.
#[wasm_bindgen]
pub struct TsLanguageService {
    file_name: String,
    source_text: String,
    arena: NodeArena,
    binder: BinderState,
    line_map: LineMap,
    root_idx: NodeIndex,
    interner: TypeInterner,
}

#[wasm_bindgen]
impl TsLanguageService {
    /// Create a new language service for a file
    #[wasm_bindgen(constructor)]
    pub fn new(file_name: String, source_text: String) -> TsLanguageService {
        // Parse
        let mut parser = ParserState::new(file_name.clone(), source_text.clone());
        let root_idx = parser.parse_source_file();
        let arena = parser.into_arena();

        // Build line map
        let line_map = LineMap::build(&source_text);

        // Bind
        let mut binder = BinderState::new();
        binder.bind_source_file(&arena, root_idx);

        TsLanguageService {
            file_name,
            source_text,
            arena,
            binder,
            line_map,
            root_idx,
            interner: TypeInterner::new(),
        }
    }

    /// Get completions at a position
    ///
    /// # Arguments
    /// * `line` - 0-based line number
    /// * `character` - 0-based character offset
    ///
    /// # Returns
    /// JSON array of completion items
    #[wasm_bindgen(js_name = getCompletionsAtPosition)]
    pub fn get_completions_at_position(&self, line: u32, character: u32) -> String {
        let position = Position { line, character };

        let completions =
            Completions::new(&self.arena, &self.binder, &self.line_map, &self.source_text);

        let items = match completions.get_completions(self.root_idx, position) {
            Some(items) => items,
            None => return "[]".to_string(),
        };

        // Convert to JSON-serializable format
        let result: Vec<CompletionItemJson> = items
            .into_iter()
            .map(|item| CompletionItemJson {
                label: item.label,
                kind: match item.kind {
                    CompletionItemKind::Variable => 6,
                    CompletionItemKind::Function => 3,
                    CompletionItemKind::Class => 7,
                    CompletionItemKind::Method => 2,
                    CompletionItemKind::Parameter => 6,
                    CompletionItemKind::Property => 10,
                    CompletionItemKind::Keyword => 14,
                },
                detail: item.detail,
                documentation: item.documentation,
            })
            .collect();

        serde_json::to_string(&result).unwrap_or_else(|_| "[]".to_string())
    }

    /// Get hover information at a position
    ///
    /// # Arguments
    /// * `line` - 0-based line number
    /// * `character` - 0-based character offset
    ///
    /// # Returns
    /// JSON with hover contents and range
    #[wasm_bindgen(js_name = getQuickInfoAtPosition)]
    pub fn get_quick_info_at_position(&self, line: u32, character: u32) -> String {
        let position = Position { line, character };

        let hover = HoverProvider::new(
            &self.arena,
            &self.binder,
            &self.line_map,
            &self.interner,
            &self.source_text,
            self.file_name.clone(),
        );

        let mut type_cache = None;
        match hover.get_hover(self.root_idx, position, &mut type_cache) {
            Some(info) => {
                let result = QuickInfoJson {
                    display_parts: info
                        .contents
                        .iter()
                        .map(|s| DisplayPart {
                            text: s.clone(),
                            kind: "text".to_string(),
                        })
                        .collect(),
                    documentation: Vec::new(),
                    text_span: info.range.map(|r| TextSpanJson {
                        start: self
                            .line_map
                            .position_to_offset(
                                Position {
                                    line: r.start.line,
                                    character: r.start.character,
                                },
                                &self.source_text,
                            )
                            .unwrap_or(0) as u32,
                        length: 0, // TODO: calculate actual length
                    }),
                };
                serde_json::to_string(&result).unwrap_or_else(|_| "null".to_string())
            }
            None => "null".to_string(),
        }
    }

    /// Get definition location at a position
    ///
    /// # Arguments
    /// * `line` - 0-based line number
    /// * `character` - 0-based character offset
    ///
    /// # Returns
    /// JSON array of definition locations
    #[wasm_bindgen(js_name = getDefinitionAtPosition)]
    pub fn get_definition_at_position(&self, line: u32, character: u32) -> String {
        let position = Position { line, character };

        let goto_def = GoToDefinition::new(
            &self.arena,
            &self.binder,
            &self.line_map,
            self.file_name.clone(),
            &self.source_text,
        );

        match goto_def.get_definition(self.root_idx, position) {
            Some(locations) => {
                let result: Vec<DefinitionInfoJson> = locations
                    .into_iter()
                    .map(|loc| DefinitionInfoJson {
                        file_name: loc.file_path,
                        text_span: TextSpanJson {
                            start: self
                                .line_map
                                .position_to_offset(loc.range.start, &self.source_text)
                                .unwrap_or(0) as u32,
                            length: (self
                                .line_map
                                .position_to_offset(loc.range.end, &self.source_text)
                                .unwrap_or(0)
                                - self
                                    .line_map
                                    .position_to_offset(loc.range.start, &self.source_text)
                                    .unwrap_or(0)) as u32,
                        },
                    })
                    .collect();
                serde_json::to_string(&result).unwrap_or_else(|_| "[]".to_string())
            }
            None => "[]".to_string(),
        }
    }

    /// Get references at a position
    ///
    /// # Arguments
    /// * `line` - 0-based line number
    /// * `character` - 0-based character offset
    ///
    /// # Returns
    /// JSON array of reference locations
    #[wasm_bindgen(js_name = getReferencesAtPosition)]
    pub fn get_references_at_position(&self, line: u32, character: u32) -> String {
        let position = Position { line, character };

        let find_refs = FindReferences::new(
            &self.arena,
            &self.binder,
            &self.line_map,
            self.file_name.clone(),
            &self.source_text,
        );

        match find_refs.find_references(self.root_idx, position) {
            Some(locations) => {
                let result: Vec<ReferenceEntryJson> = locations
                    .into_iter()
                    .map(|loc| ReferenceEntryJson {
                        file_name: loc.file_path,
                        text_span: TextSpanJson {
                            start: self
                                .line_map
                                .position_to_offset(loc.range.start, &self.source_text)
                                .unwrap_or(0) as u32,
                            length: (self
                                .line_map
                                .position_to_offset(loc.range.end, &self.source_text)
                                .unwrap_or(0)
                                - self
                                    .line_map
                                    .position_to_offset(loc.range.start, &self.source_text)
                                    .unwrap_or(0)) as u32,
                        },
                        is_write_access: false,
                        is_definition: false,
                    })
                    .collect();
                serde_json::to_string(&result).unwrap_or_else(|_| "[]".to_string())
            }
            None => "[]".to_string(),
        }
    }

    /// Update the source text and re-parse
    #[wasm_bindgen(js_name = updateSource)]
    pub fn update_source(&mut self, source_text: String) {
        self.source_text = source_text.clone();

        // Re-parse
        let mut parser = ParserState::new(self.file_name.clone(), source_text.clone());
        self.root_idx = parser.parse_source_file();
        self.arena = parser.into_arena();

        // Update line map
        self.line_map = LineMap::build(&self.source_text);

        // Re-bind
        self.binder = BinderState::new();
        self.binder.bind_source_file(&self.arena, self.root_idx);
    }

    /// Get the file name
    #[wasm_bindgen(getter, js_name = fileName)]
    pub fn file_name(&self) -> String {
        self.file_name.clone()
    }

    /// Dispose resources
    #[wasm_bindgen]
    pub fn dispose(&mut self) {
        // Clear internal state
        self.source_text.clear();
    }
}

// JSON serialization types

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompletionItemJson {
    label: String,
    kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    documentation: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuickInfoJson {
    display_parts: Vec<DisplayPart>,
    documentation: Vec<DisplayPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text_span: Option<TextSpanJson>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DisplayPart {
    text: String,
    kind: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextSpanJson {
    start: u32,
    length: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DefinitionInfoJson {
    file_name: String,
    text_span: TextSpanJson,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReferenceEntryJson {
    file_name: String,
    text_span: TextSpanJson,
    is_write_access: bool,
    is_definition: bool,
}

/// Create a language service for a file
#[wasm_bindgen(js_name = createLanguageService)]
pub fn create_language_service(file_name: &str, source_text: &str) -> TsLanguageService {
    TsLanguageService::new(file_name.to_string(), source_text.to_string())
}
