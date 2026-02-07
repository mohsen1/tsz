//! TypeScript SourceFile API
//!
//! Provides the `TsSourceFile` struct which implements TypeScript's SourceFile interface.

use std::sync::Arc;
use wasm_bindgen::prelude::*;

use tsz::parser::syntax_kind_ext;
use tsz::parser::{NodeArena, NodeIndex, ParserState};

use super::enums::{ScriptKind, ScriptTarget};

/// TypeScript SourceFile - represents a parsed source file
///
/// Provides access to:
/// - File metadata (name, text, language version)
/// - AST root and statements
/// - Node traversal methods
#[wasm_bindgen]
pub struct TsSourceFile {
    /// File name
    file_name: String,
    /// Source text
    text: String,
    /// Language version
    language_version: ScriptTarget,
    /// Script kind (TS, TSX, JS, etc.)
    script_kind: ScriptKind,
    /// Is declaration file
    is_declaration_file: bool,
    /// Parsed AST arena
    arena: Option<Arc<NodeArena>>,
    /// Root node index
    root_idx: Option<NodeIndex>,
}

#[wasm_bindgen]
impl TsSourceFile {
    /// Create a new source file by parsing the given text
    #[wasm_bindgen(constructor)]
    pub fn new(file_name: String, source_text: String) -> TsSourceFile {
        let script_kind = get_script_kind_from_file_name(&file_name);
        let is_declaration_file = file_name.ends_with(".d.ts");

        TsSourceFile {
            file_name,
            text: source_text,
            language_version: ScriptTarget::ESNext,
            script_kind,
            is_declaration_file,
            arena: None,
            root_idx: None,
        }
    }

    /// Parse the source file (lazy)
    fn ensure_parsed(&mut self) {
        if self.arena.is_some() {
            return;
        }

        let mut parser = ParserState::new(self.file_name.clone(), self.text.clone());
        let root_idx = parser.parse_source_file();

        self.arena = Some(Arc::new(parser.into_arena()));
        self.root_idx = Some(root_idx);
    }

    /// Get the file name
    #[wasm_bindgen(getter, js_name = fileName)]
    pub fn file_name(&self) -> String {
        self.file_name.clone()
    }

    /// Get the source text
    #[wasm_bindgen(getter)]
    pub fn text(&self) -> String {
        self.text.clone()
    }

    /// Get the language version
    #[wasm_bindgen(getter, js_name = languageVersion)]
    pub fn language_version(&self) -> ScriptTarget {
        self.language_version
    }

    /// Get the script kind
    #[wasm_bindgen(getter, js_name = scriptKind)]
    pub fn script_kind(&self) -> ScriptKind {
        self.script_kind
    }

    /// Check if this is a declaration file
    #[wasm_bindgen(getter, js_name = isDeclarationFile)]
    pub fn is_declaration_file(&self) -> bool {
        self.is_declaration_file
    }

    /// Get the end position (length of text)
    #[wasm_bindgen(getter)]
    pub fn end(&self) -> u32 {
        self.text.len() as u32
    }

    /// Get the start position (always 0)
    #[wasm_bindgen(getter)]
    pub fn pos(&self) -> u32 {
        0
    }

    /// Get the kind (always SourceFile)
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> u16 {
        syntax_kind_ext::SOURCE_FILE
    }

    /// Get the root node handle
    #[wasm_bindgen(js_name = getRootHandle)]
    pub fn get_root_handle(&mut self) -> u32 {
        self.ensure_parsed();
        self.root_idx.map(|idx| idx.0).unwrap_or(u32::MAX)
    }

    /// Get statement handles (children of source file)
    #[wasm_bindgen(js_name = getStatementHandles)]
    pub fn get_statement_handles(&mut self) -> Vec<u32> {
        self.ensure_parsed();

        let Some(arena) = &self.arena else {
            return Vec::new();
        };
        let Some(root_idx) = self.root_idx else {
            return Vec::new();
        };

        // Get statements from the source file node
        if let Some(sf) = arena.get_source_file_at(root_idx) {
            sf.statements.nodes.iter().map(|idx| idx.0).collect()
        } else {
            Vec::new()
        }
    }

    /// Get node kind by handle
    #[wasm_bindgen(js_name = getNodeKind)]
    pub fn get_node_kind(&self, handle: u32) -> u16 {
        let Some(arena) = &self.arena else {
            return 0;
        };
        arena.get(NodeIndex(handle)).map(|n| n.kind).unwrap_or(0)
    }

    /// Get node start position
    #[wasm_bindgen(js_name = getNodePos)]
    pub fn get_node_pos(&self, handle: u32) -> u32 {
        let Some(arena) = &self.arena else {
            return 0;
        };
        arena.get(NodeIndex(handle)).map(|n| n.pos).unwrap_or(0)
    }

    /// Get node end position
    #[wasm_bindgen(js_name = getNodeEnd)]
    pub fn get_node_end(&self, handle: u32) -> u32 {
        let Some(arena) = &self.arena else {
            return 0;
        };
        arena.get(NodeIndex(handle)).map(|n| n.end).unwrap_or(0)
    }

    /// Get node flags
    #[wasm_bindgen(js_name = getNodeFlags)]
    pub fn get_node_flags(&self, handle: u32) -> u16 {
        let Some(arena) = &self.arena else {
            return 0;
        };
        arena.get(NodeIndex(handle)).map(|n| n.flags).unwrap_or(0)
    }

    /// Get node text (substring from source)
    #[wasm_bindgen(js_name = getNodeText)]
    pub fn get_node_text(&self, handle: u32) -> String {
        let Some(arena) = &self.arena else {
            return String::new();
        };
        let Some(node) = arena.get(NodeIndex(handle)) else {
            return String::new();
        };

        let start = node.pos as usize;
        let end = node.end as usize;

        if start <= end && end <= self.text.len() {
            self.text[start..end].to_string()
        } else {
            String::new()
        }
    }

    /// Get parent node handle
    #[wasm_bindgen(js_name = getParentHandle)]
    pub fn get_parent_handle(&self, handle: u32) -> u32 {
        let Some(arena) = &self.arena else {
            return u32::MAX;
        };
        arena
            .get_extended(NodeIndex(handle))
            .map(|ext| ext.parent.0)
            .unwrap_or(u32::MAX)
    }

    /// Get children of a node
    #[wasm_bindgen(js_name = getChildHandles)]
    pub fn get_child_handles(&self, handle: u32) -> Vec<u32> {
        let Some(arena) = &self.arena else {
            return Vec::new();
        };

        // Use the comprehensive get_node_children from ast module
        super::ast::get_node_children(arena, NodeIndex(handle))
            .into_iter()
            .map(|idx| idx.0)
            .collect()
    }

    /// Get identifier text for an identifier node
    #[wasm_bindgen(js_name = getIdentifierText)]
    pub fn get_identifier_text(&self, handle: u32) -> Option<String> {
        let arena = self.arena.as_ref()?;
        let node = arena.get(NodeIndex(handle))?;
        let ident = arena.get_identifier(node)?;
        Some(ident.escaped_text.clone())
    }

    /// Check if a node is a specific kind
    #[wasm_bindgen(js_name = isKind)]
    pub fn is_kind(&self, handle: u32, kind: u16) -> bool {
        self.get_node_kind(handle) == kind
    }

    /// Iterate over children (returns child handles as JSON array)
    #[wasm_bindgen(js_name = forEachChild)]
    pub fn for_each_child(&self, handle: u32) -> JsValue {
        let children = self.get_child_handles(handle);
        serde_wasm_bindgen::to_value(&children).unwrap_or(JsValue::NULL)
    }
}

/// Get script kind from file extension
fn get_script_kind_from_file_name(file_name: &str) -> ScriptKind {
    let lower = file_name.to_lowercase();
    if lower.ends_with(".tsx") {
        ScriptKind::TSX
    } else if lower.ends_with(".jsx") {
        ScriptKind::JSX
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") || lower.ends_with(".cjs") {
        ScriptKind::JS
    } else if lower.ends_with(".json") {
        ScriptKind::JSON
    } else {
        ScriptKind::TS
    }
}

/// Create a source file (factory function)
#[wasm_bindgen(js_name = createTsSourceFile)]
pub fn create_ts_source_file(
    file_name: String,
    source_text: String,
    language_version: ScriptTarget,
) -> TsSourceFile {
    let mut sf = TsSourceFile::new(file_name, source_text);
    sf.language_version = language_version;
    sf
}
