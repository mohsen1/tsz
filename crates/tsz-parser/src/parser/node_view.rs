//! `NodeView` ergonomic wrapper, `NodeInfo` struct, and `NodeAccess` trait.
//!
//! Extracted from `node_access.rs` to keep that file focused on typed data
//! accessors. This module provides higher-level abstractions for reading nodes:
//! - `NodeView`: a borrowed "fat pointer" pairing a Node reference with its arena
//! - `NodeInfo`: a detached snapshot of a node's essential metadata
//! - `NodeAccess`: trait for unified access across arena implementations

use super::base::NodeIndex;
use super::node::{
    BinaryExprData, BlockData, CallExprData, ClassData, ExtendedNodeInfo, FunctionData,
    IdentifierData, LiteralData, Node, NodeArena, SourceFileData,
};

// =============================================================================
// Node View - Ergonomic wrapper for reading Nodes
// =============================================================================

/// A view into a node that provides convenient access to both the Node
/// header and its type-specific data. This avoids the need to pass the arena
/// around when working with node data.
#[derive(Clone, Copy)]
pub struct NodeView<'a> {
    pub node: &'a Node,
    pub arena: &'a NodeArena,
    pub index: NodeIndex,
}

impl<'a> NodeView<'a> {
    /// Create a new `NodeView`.
    #[inline]
    #[must_use]
    pub fn new(arena: &'a NodeArena, index: NodeIndex) -> Option<Self> {
        arena.get(index).map(|node| NodeView { node, arena, index })
    }

    /// Get the `SyntaxKind`.
    #[inline]
    #[must_use]
    pub const fn kind(&self) -> u16 {
        self.node.kind
    }

    /// Get the start position.
    #[inline]
    #[must_use]
    pub const fn pos(&self) -> u32 {
        self.node.pos
    }

    /// Get the end position.
    #[inline]
    #[must_use]
    pub const fn end(&self) -> u32 {
        self.node.end
    }

    /// Get the flags.
    #[inline]
    #[must_use]
    pub const fn flags(&self) -> u16 {
        self.node.flags
    }

    /// Check if this node has associated data.
    #[inline]
    #[must_use]
    pub const fn has_data(&self) -> bool {
        self.node.has_data()
    }

    /// Get extended node info (`parent`, `id`, modifier/transform flags).
    #[inline]
    #[must_use]
    pub fn extended(&self) -> Option<&'a ExtendedNodeInfo> {
        self.arena.get_extended(self.index)
    }

    /// Get parent node index.
    #[inline]
    #[must_use]
    pub fn parent(&self) -> NodeIndex {
        self.extended().map_or(NodeIndex::NONE, |e| e.parent)
    }

    /// Get node id.
    #[inline]
    #[must_use]
    pub fn id(&self) -> u32 {
        self.extended().map_or(0, |e| e.id)
    }

    /// Get a child node as a `NodeView`.
    #[inline]
    #[must_use]
    pub fn child(&self, index: NodeIndex) -> Option<Self> {
        NodeView::new(self.arena, index)
    }

    // Typed data accessors - return Option<&T> based on node kind

    /// Get identifier data (for `Identifier`, `PrivateIdentifier` nodes).
    #[inline]
    #[must_use]
    pub fn as_identifier(&self) -> Option<&'a IdentifierData> {
        self.arena.get_identifier(self.node)
    }

    /// Get literal data (for `StringLiteral`, `NumericLiteral`, etc.).
    #[inline]
    #[must_use]
    pub fn as_literal(&self) -> Option<&'a LiteralData> {
        self.arena.get_literal(self.node)
    }

    /// Get binary expression data
    #[inline]
    #[must_use]
    pub fn as_binary_expr(&self) -> Option<&'a BinaryExprData> {
        self.arena.get_binary_expr(self.node)
    }

    /// Get call expression data
    #[inline]
    #[must_use]
    pub fn as_call_expr(&self) -> Option<&'a CallExprData> {
        self.arena.get_call_expr(self.node)
    }

    /// Get function data
    #[inline]
    #[must_use]
    pub fn as_function(&self) -> Option<&'a FunctionData> {
        self.arena.get_function(self.node)
    }

    /// Get class data
    #[inline]
    #[must_use]
    pub fn as_class(&self) -> Option<&'a ClassData> {
        self.arena.get_class(self.node)
    }

    /// Get block data
    #[inline]
    #[must_use]
    pub fn as_block(&self) -> Option<&'a BlockData> {
        self.arena.get_block(self.node)
    }

    /// Get source file data
    #[inline]
    #[must_use]
    pub fn as_source_file(&self) -> Option<&'a SourceFileData> {
        self.arena.get_source_file(self.node)
    }
}

// =============================================================================
// Node Access Trait - Unified Interface for Arena Types
// =============================================================================

/// Common node information that both arena types can provide.
/// This struct contains the essential fields needed by most consumers.
#[derive(Clone, Debug)]
pub struct NodeInfo {
    pub kind: u16,
    pub flags: u32,
    pub modifier_flags: u32,
    pub pos: u32,
    pub end: u32,
    pub parent: NodeIndex,
    pub id: u32,
}

impl NodeInfo {
    /// Create from a Node and its extended info
    #[must_use]
    pub fn from_thin(node: &Node, ext: &ExtendedNodeInfo) -> Self {
        Self {
            kind: node.kind,
            flags: u32::from(node.flags),
            modifier_flags: ext.modifier_flags,
            pos: node.pos,
            end: node.end,
            parent: ext.parent,
            id: ext.id,
        }
    }
}

/// Trait for unified access to AST nodes across different arena implementations.
/// This allows consumers (binder, checker, emitter) to work with either
/// different arena implementations without code changes.
pub trait NodeAccess {
    /// Get basic node information by index
    fn node_info(&self, index: NodeIndex) -> Option<NodeInfo>;

    /// Get the syntax kind of a node
    fn kind(&self, index: NodeIndex) -> Option<u16>;

    /// Get the source position range
    fn pos_end(&self, index: NodeIndex) -> Option<(u32, u32)>;

    /// Check if a node exists
    fn exists(&self, index: NodeIndex) -> bool {
        index.is_some() && self.kind(index).is_some()
    }

    /// Get identifier text (if this is an identifier node)
    fn get_identifier_text(&self, index: NodeIndex) -> Option<&str>;

    /// Get literal value text (if this is a literal node)
    fn get_literal_text(&self, index: NodeIndex) -> Option<&str>;

    /// Get children of a node (for traversal)
    fn get_children(&self, index: NodeIndex) -> Vec<NodeIndex>;
}

/// Implementation of `NodeAccess` for `NodeArena`
impl NodeAccess for NodeArena {
    fn node_info(&self, index: NodeIndex) -> Option<NodeInfo> {
        if index.is_none() {
            return None;
        }
        let node = self.nodes.get(index.0 as usize)?;
        let ext = self.extended_info.get(index.0 as usize)?;
        Some(NodeInfo::from_thin(node, ext))
    }

    fn kind(&self, index: NodeIndex) -> Option<u16> {
        if index.is_none() {
            return None;
        }
        self.nodes.get(index.0 as usize).map(|n| n.kind)
    }

    fn pos_end(&self, index: NodeIndex) -> Option<(u32, u32)> {
        if index.is_none() {
            return None;
        }
        self.nodes.get(index.0 as usize).map(|n| (n.pos, n.end))
    }

    fn get_identifier_text(&self, index: NodeIndex) -> Option<&str> {
        let node = self.get(index)?;
        let data = self.get_identifier(node)?;
        // Use atom for O(1) lookup if available, otherwise fall back to escaped_text
        Some(self.resolve_identifier_text(data))
    }

    fn get_literal_text(&self, index: NodeIndex) -> Option<&str> {
        let node = self.get(index)?;
        let data = self.get_literal(node)?;
        Some(&data.text)
    }

    fn get_children(&self, index: NodeIndex) -> Vec<NodeIndex> {
        if index.is_none() {
            return Vec::new();
        }

        let Some(node) = self.nodes.get(index.0 as usize) else {
            return Vec::new();
        };

        let mut children = Vec::new();

        if self.collect_name_children(node, &mut children)
            || self.collect_expression_children(node, &mut children)
            || self.collect_statement_children(node, &mut children)
            || self.collect_declaration_children(node, &mut children)
            || self.collect_import_export_children(node, &mut children)
            || self.collect_type_children(node, &mut children)
            || self.collect_member_children(node, &mut children)
            || self.collect_pattern_children(node, &mut children)
            || self.collect_jsx_children(node, &mut children)
            || self.collect_signature_children(node, &mut children)
            || self.collect_source_children(node, &mut children)
        {
            return children;
        }

        children
    }
}
