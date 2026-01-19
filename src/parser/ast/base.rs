//! Base types for AST nodes (legacy fat AST).
//!
//! Note: shared base types (`NodeIndex`, `NodeList`, `TextRange`) live in
//! `crate::parser::base` so the thin pipeline doesn't depend on the legacy AST.

pub use crate::parser::base::{NodeIndex, NodeList, TextRange};
use crate::scanner::SyntaxKind;
use serde::Serialize;

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
