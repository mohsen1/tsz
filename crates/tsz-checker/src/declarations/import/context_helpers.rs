//! AST context-checking utilities for import/export validation.
//!
//! Functions that walk the parent chain to determine context
//! (namespace, function body, module augmentation, etc.).

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Check if a statement has an export modifier.
    pub(crate) fn has_export_modifier(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        let Some(mods) = self.get_declaration_modifiers(node) else {
            return false;
        };

        self.ctx
            .arena
            .has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
    }

    /// Check whether a node is nested inside a namespace declaration.
    /// String-literal ambient modules (`declare module "x"`) are excluded.
    pub(crate) fn is_inside_namespace_declaration(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;

        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if current.is_none() {
                break;
            }

            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }

            let Some(module_decl) = self.ctx.arena.get_module(node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(module_decl.name) else {
                continue;
            };

            if name_node.kind != SyntaxKind::StringLiteral as u16 {
                return true;
            }
        }

        false
    }

    /// Check if a node is NOT in a valid module-element context (`SourceFile` or `ModuleBlock`).
    /// Returns true when the node is inside a block, function body, or other non-module context.
    pub(crate) fn is_in_non_module_element_context(&self, node_idx: NodeIndex) -> bool {
        let parent_idx = self.ctx.arena.get_extended(node_idx).map(|ext| ext.parent);
        let parent_kind = parent_idx
            .and_then(|p| self.ctx.arena.get(p))
            .map(|p| p.kind);

        // For import-equals inside `export import X = N;`, the direct parent is
        // EXPORT_DECLARATION. Look through it to the grandparent.
        let effective_parent_kind = if matches!(parent_kind, Some(k) if k == syntax_kind_ext::EXPORT_DECLARATION)
        {
            parent_idx
                .and_then(|p| self.ctx.arena.get_extended(p))
                .and_then(|ext| self.ctx.arena.get(ext.parent))
                .map(|p| p.kind)
        } else {
            parent_kind
        };

        match effective_parent_kind {
            Some(k) if k == syntax_kind_ext::SOURCE_FILE || k == syntax_kind_ext::MODULE_BLOCK => {
                false
            }
            None => false, // Top-level
            _ => true,
        }
    }

    /// Check if a node is inside a function/method body.
    /// Walks up the parent chain to find a function-like ancestor.
    pub(crate) fn is_inside_function_body(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if current.is_none() {
                break;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            match node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::CONSTRUCTOR
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR =>
                {
                    return true;
                }
                k if k == syntax_kind_ext::SOURCE_FILE || k == syntax_kind_ext::MODULE_BLOCK => {
                    return false;
                }
                _ => continue,
            }
        }
        false
    }

    /// Check if a node is inside a module augmentation
    /// (`declare module "string" { ... }`).  Module augmentations have a
    /// `MODULE_DECLARATION` ancestor whose name is a string literal.
    pub(crate) fn is_inside_module_augmentation(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if current.is_none() {
                break;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(mod_data) = self.ctx.arena.get_module_at(current)
                && let Some(name_node) = self.ctx.arena.get(mod_data.name)
                && name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
            {
                return true;
            }
        }
        false
    }

    /// Check if a node is inside a `declare global { ... }` augmentation block.
    pub(crate) fn is_inside_global_augmentation(&self, node_idx: NodeIndex) -> bool {
        use tsz_parser::parser::flags::node_flags;

        let mut current = node_idx;
        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if current.is_none() {
                break;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::MODULE_DECLARATION
                && (node.flags as u32) & node_flags::GLOBAL_AUGMENTATION != 0
            {
                return true;
            }
        }
        false
    }
}
