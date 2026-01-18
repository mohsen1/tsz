```rust
// src/parser/flags.rs

//! Flags and attributes for nodes in the arena.

use std::fmt;

/// Represents the kind of a node in the AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Root,
    Element,
    Text,
    Comment,
    /// Specific flags for ThinNode (e.g., HasChildren)
    NodeFlag,
}

/// A reference to a node within the arena, encoded as an offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeRef {
    pub(crate) index: u32,
    pub(crate) generation: u32,
}

impl NodeRef {
    pub fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Returns the offset of the node in the arena storage.
    pub fn offset(&self) -> usize {
        self.index as usize
    }
}

/// A lightweight reference to a node that stores a flag and a reference.
#[derive(Clone, Copy)]
pub struct ThinNode {
    /// The kind of node or flags associated with it.
    pub kind: NodeKind,
    /// Reference to the main data in the arena.
    pub reference: NodeRef,
}

impl ThinNode {
    /// Creates a new ThinNode.
    pub fn new(kind: NodeKind, reference: NodeRef) -> Self {
        Self { kind, reference }
    }

    /// Returns the offset of the node in the arena.
    /// Used by tests to verify memory layout correctness without asserting ownership.
    pub fn get_arena_offset(&self) -> usize {
        self.reference.offset()
    }

    /// Returns the generation of the reference.
    pub fn generation(&self) -> u32 {
        self.reference.generation
    }
}

impl fmt::Debug for ThinNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ThinNode")
            .field("kind", &self.kind)
            .field("ref_offset", &self.reference.offset())
            .field("ref_gen", &self.reference.generation)
            .finish()
    }
}
```
