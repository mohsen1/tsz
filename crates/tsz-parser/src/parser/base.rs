//! Shared parser base types used by both Node and (legacy) AST.
//!
//! These types are part of the "thin pipeline" surface area and should remain
//! available even if the legacy fat AST is disabled.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// A text range with start and end positions.
/// All positions are character indices (not byte indices).
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct TextRange {
    pub pos: u32,
    pub end: u32,
}

impl<'de> serde::Deserialize<'de> for TextRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Helper {
            pos: u32,
            end: u32,
        }

        let helper = Helper::deserialize(deserializer)?;
        Ok(Self {
            pos: helper.pos,
            end: helper.end,
        })
    }
}

#[wasm_bindgen]
impl TextRange {
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new(pos: u32, end: u32) -> Self {
        Self { pos, end }
    }
}

/// Index into an arena. Used instead of pointers/references for serialization-friendly graphs.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, Hash)]
pub struct NodeIndex(pub u32);

impl NodeIndex {
    pub const NONE: Self = Self(u32::MAX);

    #[inline]
    #[must_use]
    pub fn is_none(&self) -> bool {
        self.0 == u32::MAX
    }

    #[inline]
    #[must_use]
    pub fn is_some(&self) -> bool {
        self.0 != u32::MAX
    }
}

/// A list of node indices, representing children or a node array.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NodeList {
    pub nodes: Vec<NodeIndex>,
    pub pos: u32,
    pub end: u32,
    pub has_trailing_comma: bool,
}

impl NodeList {
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            pos: 0,
            end: 0,
            has_trailing_comma: false,
        }
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(capacity),
            pos: 0,
            end: 0,
            has_trailing_comma: false,
        }
    }

    pub fn push(&mut self, node: NodeIndex) {
        self.nodes.push(node);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}
