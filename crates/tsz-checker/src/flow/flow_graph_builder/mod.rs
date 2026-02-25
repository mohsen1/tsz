//! Flow Graph Builder for Control Flow Analysis.
//!
//! This module provides the `FlowGraph` side-table and `FlowGraphBuilder` for
//! constructing control flow graphs from Node AST post-binding.
//!
//! The `FlowGraph` is a side-table that tracks:
//! - Flow nodes for each control flow point (conditions, branches, loops)
//! - Mapping from AST nodes to their corresponding flow nodes
//! - Antecedent relationships between flow nodes
//!
//! This enables type narrowing analysis without mutating AST nodes.

mod core;
pub(crate) mod expressions;

pub use self::core::{FlowGraph, FlowGraphBuilder};
