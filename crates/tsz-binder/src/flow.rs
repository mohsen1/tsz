//! Control flow graph types and arena for the binder.
//!
//! Provides `FlowNode`, `FlowNodeId`, `FlowNodeArena`, and `flow_flags`.

use serde::{Deserialize, Serialize};
use tsz_parser::NodeIndex;

// =============================================================================
// Control Flow Graph
// =============================================================================

/// Flags for flow nodes describing their type and properties.
/// Matches TypeScript's `FlowFlags` in src/compiler/types.ts
pub mod flow_flags {
    pub const UNREACHABLE: u32 = 1 << 0; // Unreachable code
    pub const START: u32 = 1 << 1; // Start of flow graph
    pub const BRANCH_LABEL: u32 = 1 << 2; // Branch label
    pub const LOOP_LABEL: u32 = 1 << 3; // Loop label
    pub const ASSIGNMENT: u32 = 1 << 4; // Assignment
    pub const TRUE_CONDITION: u32 = 1 << 5; // True condition
    pub const FALSE_CONDITION: u32 = 1 << 6; // False condition
    pub const SWITCH_CLAUSE: u32 = 1 << 7; // Switch clause
    pub const ARRAY_MUTATION: u32 = 1 << 8; // Array mutation
    pub const CALL: u32 = 1 << 9; // Call expression
    pub const REDUCE_LABEL: u32 = 1 << 10; // Reduce label
    pub const REFERENCED: u32 = 1 << 11; // Referenced
    pub const AWAIT_POINT: u32 = 1 << 12; // Await expression (suspension point)
    pub const YIELD_POINT: u32 = 1 << 13; // Yield expression (generator suspension point)

    // Composite flags
    pub const LABEL: u32 = BRANCH_LABEL | LOOP_LABEL;
    pub const CONDITION: u32 = TRUE_CONDITION | FALSE_CONDITION;
}

/// Unique identifier for a flow node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlowNodeId(pub u32);

impl FlowNodeId {
    pub const NONE: Self = Self(u32::MAX);

    #[must_use]
    pub const fn is_none(&self) -> bool {
        self.0 == u32::MAX
    }

    #[must_use]
    pub const fn is_some(&self) -> bool {
        self.0 != u32::MAX
    }
}

/// A node in the control flow graph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlowNode {
    /// Flow node flags
    pub flags: u32,
    /// Flow node ID
    pub id: FlowNodeId,
    /// Antecedent flow node(s) - predecessors in the control flow
    pub antecedent: Vec<FlowNodeId>,
    /// Associated AST node (for assignments, conditions, etc.)
    pub node: NodeIndex,
}

impl FlowNode {
    #[must_use]
    pub const fn new(id: FlowNodeId, flags: u32) -> Self {
        Self {
            flags,
            id,
            antecedent: Vec::new(),
            node: NodeIndex::NONE,
        }
    }

    #[must_use]
    pub const fn has_flags(&self, flags: u32) -> bool {
        (self.flags & flags) == flags
    }

    #[must_use]
    pub const fn has_any_flags(&self, flags: u32) -> bool {
        (self.flags & flags) != 0
    }
}

/// Arena for flow nodes.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FlowNodeArena {
    nodes: Vec<FlowNode>,
}

impl FlowNodeArena {
    #[must_use]
    pub const fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// Allocate a new flow node.
    ///
    /// # Panics
    ///
    /// Panics if the number of flow nodes would overflow a `u32` when converted
    /// from arena length.
    pub fn alloc(&mut self, flags: u32) -> FlowNodeId {
        let id = FlowNodeId(
            u32::try_from(self.nodes.len()).expect("flow node arena length exceeds u32"),
        );
        self.nodes.push(FlowNode::new(id, flags));
        id
    }

    /// Get a flow node by ID.
    #[must_use]
    pub fn get(&self, id: FlowNodeId) -> Option<&FlowNode> {
        if id.is_none() {
            None
        } else {
            self.nodes.get(id.0 as usize)
        }
    }

    /// Get a mutable flow node by ID.
    pub fn get_mut(&mut self, id: FlowNodeId) -> Option<&mut FlowNode> {
        if id.is_none() {
            None
        } else {
            self.nodes.get_mut(id.0 as usize)
        }
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
    }

    /// Find the unreachable flow node in the arena.
    /// This is used when reconstructing a `BinderState` from serialized flow data.
    ///
    /// # Panics
    ///
    /// Panics if a matching flow node index cannot be represented as `u32` while
    /// constructing the returned `FlowNodeId`. This implies arena state is
    /// inconsistent with the internal `u32` IDs.
    #[must_use]
    pub fn find_unreachable(&self) -> Option<FlowNodeId> {
        for (idx, node) in self.nodes.iter().enumerate() {
            if node.has_any_flags(flow_flags::UNREACHABLE) {
                return Some(FlowNodeId(
                    u32::try_from(idx).expect("flow node index exceeds u32"),
                ));
            }
        }
        None
    }
}
