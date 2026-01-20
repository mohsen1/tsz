//! Shared parser base types used by both Node and (legacy) AST.
//!
//! These types are part of the "thin pipeline" surface area and should remain
//! available even if the legacy fat AST is disabled.

use serde::Serialize;
use wasm_bindgen::prelude::*;

/// A text range with start and end positions.
/// All positions are character indices (not byte indices).
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct TextRange {
    pub pos: u32,
    pub end: u32,
}

#[wasm_bindgen]
impl TextRange {
    #[wasm_bindgen(constructor)]
    pub fn new(pos: u32, end: u32) -> TextRange {
        TextRange { pos, end }
    }
}

/// Index into an arena. Used instead of pointers/references for serialization-friendly graphs.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Hash)]
pub struct NodeIndex(pub u32);

impl NodeIndex {
    pub const NONE: NodeIndex = NodeIndex(u32::MAX);

    #[inline]
    pub fn is_none(&self) -> bool {
        self.0 == u32::MAX
    }

    #[inline]
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
