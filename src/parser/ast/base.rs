//! Base types for AST nodes.

use crate::scanner::SyntaxKind;
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// A text range with start and end positions.
/// All positions are character indices (not byte indices).
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct TextRange {
    pub pos: u32, // Start position
    pub end: u32, // End position
}

#[wasm_bindgen]
impl TextRange {
    #[wasm_bindgen(constructor)]
    pub fn new(pos: u32, end: u32) -> TextRange {
        TextRange { pos, end }
    }
}

/// Index into the node arena. Used instead of pointers/references
/// for efficient serialization and memory management.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Hash)]
pub struct NodeIndex(pub u32);

impl NodeIndex {
    pub const NONE: NodeIndex = NodeIndex(u32::MAX);

    pub fn is_none(&self) -> bool {
        self.0 == u32::MAX
    }

    pub fn is_some(&self) -> bool {
        self.0 != u32::MAX
    }
}

/// A list of node indices, representing children or a node array.
#[derive(Clone, Debug, Default, Serialize)]
pub struct NodeList {
    pub nodes: Vec<NodeIndex>,
    pub pos: u32,
    pub end: u32,
    pub has_trailing_comma: bool,
}

impl NodeList {
    pub fn new() -> NodeList {
        NodeList {
            nodes: Vec::new(),
            pos: 0,
            end: 0,
            has_trailing_comma: false,
        }
    }

    pub fn with_capacity(capacity: usize) -> NodeList {
        NodeList {
            nodes: Vec::with_capacity(capacity),
            pos: 0,
            end: 0,
            has_trailing_comma: false,
        }
    }

    pub fn push(&mut self, node: NodeIndex) {
        self.nodes.push(node);
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Common fields present in all AST nodes.
/// Note: `kind` is stored as u16 to support both token kinds (from SyntaxKind enum)
/// and extended node kinds (from syntax_kind_ext constants).
#[derive(Clone, Debug, Serialize)]
pub struct NodeBase {
    pub kind: u16,            // SyntaxKind value (u16 to support extended kinds)
    pub flags: u32,           // NodeFlags
    pub modifier_flags: u32,  // ModifierFlags (cached)
    pub transform_flags: u32, // TransformFlags
    pub pos: u32,             // Start position (character index)
    pub end: u32,             // End position (character index)
    pub parent: NodeIndex,    // Parent node index
    pub id: u32,              // Unique node ID (assigned by parser)
}

impl Default for NodeBase {
    fn default() -> Self {
        NodeBase {
            kind: SyntaxKind::Unknown as u16,
            flags: 0,
            modifier_flags: 0,
            transform_flags: 0,
            pos: 0,
            end: 0,
            parent: NodeIndex::NONE,
            id: 0,
        }
    }
}

impl NodeBase {
    /// Create a new NodeBase with a SyntaxKind (token kind)
    pub fn new(kind: SyntaxKind, pos: u32, end: u32) -> NodeBase {
        NodeBase {
            kind: kind as u16,
            flags: 0,
            modifier_flags: 0,
            transform_flags: 0,
            pos,
            end,
            parent: NodeIndex::NONE,
            id: 0,
        }
    }

    /// Create a new NodeBase with an extended kind (for node types not in scanner)
    pub fn new_ext(kind: u16, pos: u32, end: u32) -> NodeBase {
        NodeBase {
            kind,
            flags: 0,
            modifier_flags: 0,
            transform_flags: 0,
            pos,
            end,
            parent: NodeIndex::NONE,
            id: 0,
        }
    }
}
