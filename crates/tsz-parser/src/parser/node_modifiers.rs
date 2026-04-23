//! Modifier query utilities for `NodeArena`.
//!
//! Provides shared helpers for checking whether a node's modifier list
//! contains a specific `SyntaxKind`. This consolidates the `has_*_modifier`
//! pattern that was previously duplicated across binder, checker, emitter,
//! and lowering crates.

use super::base::{NodeIndex, NodeList};
use super::node::NodeArena;
use tsz_common::Visibility;
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

    /// Check whether a modifier list contains `declare`.
    ///
    /// Shortcut for `has_modifier(modifiers, SyntaxKind::DeclareKeyword)`,
    /// the most common single-kind query across the emitter lowering and
    /// declaration-file pipelines (ambient-namespace detection, CommonJS
    /// lowering, const-enum gating).
    #[inline]
    #[must_use]
    pub fn is_declare(&self, modifiers: &Option<NodeList>) -> bool {
        self.has_modifier(modifiers, SyntaxKind::DeclareKeyword)
    }

    /// Like [`is_declare`](Self::is_declare) but accepts `Option<&NodeList>`.
    #[inline]
    #[must_use]
    pub fn is_declare_ref(&self, modifiers: Option<&NodeList>) -> bool {
        self.has_modifier_ref(modifiers, SyntaxKind::DeclareKeyword)
    }

    /// Extract the visibility level from a modifier list.
    ///
    /// Scans for `private` or `protected` keywords; returns `Public` if neither is found.
    /// This is the single source of truth for the modifier→Visibility mapping,
    /// consolidating duplicate implementations across checker and lowering crates.
    #[inline]
    pub fn get_visibility_from_modifiers(&self, modifiers: &Option<NodeList>) -> Visibility {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::PrivateKeyword as u16 {
                        return Visibility::Private;
                    }
                    if mod_node.kind == SyntaxKind::ProtectedKeyword as u16 {
                        return Visibility::Protected;
                    }
                }
            }
        }
        Visibility::Public
    }
}
