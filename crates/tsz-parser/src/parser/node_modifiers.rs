//! Modifier query utilities for `NodeArena`.
//!
//! Provides shared helpers for checking whether a node's modifier list
//! contains a specific `SyntaxKind`. This consolidates the `has_*_modifier`
//! pattern that was previously duplicated across binder, checker, emitter,
//! and lowering crates.

use super::base::{NodeIndex, NodeList};
use super::node::NodeArena;
use tsz_scanner::SyntaxKind;

impl NodeArena {
    /// Check whether a modifier list contains a modifier of the given kind.
    ///
    /// This is the single source of truth for the "scan modifiers for a kind"
    /// pattern used across all pipeline stages.
    #[inline]
    pub fn has_modifier(&self, modifiers: &Option<NodeList>, kind: SyntaxKind) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.get(mod_idx)
                    && mod_node.kind == kind as u16
                {
                    return true;
                }
            }
        }
        false
    }

    /// Like [`has_modifier`](Self::has_modifier) but accepts `Option<&NodeList>`.
    #[inline]
    pub fn has_modifier_ref(&self, modifiers: Option<&NodeList>, kind: SyntaxKind) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.get(mod_idx)
                    && mod_node.kind == kind as u16
                {
                    return true;
                }
            }
        }
        false
    }

    /// Find the first modifier of the given kind, returning its `NodeIndex`.
    #[inline]
    pub fn find_modifier(
        &self,
        modifiers: &Option<NodeList>,
        kind: SyntaxKind,
    ) -> Option<NodeIndex> {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.get(mod_idx)
                    && mod_node.kind == kind as u16
                {
                    return Some(mod_idx);
                }
            }
        }
        None
    }
}
