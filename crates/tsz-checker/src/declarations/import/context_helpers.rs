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
        let parent_idx = self.ctx.arena.parent_of(node_idx);
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
    ///
    /// A class `static { }` block is function-like for this purpose: tsc's
    /// binder gives it its own `container` cursor, same as a function body,
    /// and it never resolves a position-invalid `import`/`import =` module
    /// specifier inside one (#16450).
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
                    || k == syntax_kind_ext::SET_ACCESSOR
                    || k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION =>
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

    /// Whether a module element that has *already* drawn a placement diagnostic
    /// still resolves its module specifier.
    ///
    /// Only meaningful for a node in a non-module-element context — i.e. one for
    /// which [`Self::is_in_non_module_element_context`] is true. A declaration in a
    /// valid context (`SourceFile`, or the `ModuleBlock` of an ambient module)
    /// always resolves and must not consult this.
    ///
    /// tsc's `checkExportDeclaration` reports the placement diagnostic and then
    /// `return`s, so `resolveExternalModuleName` — the only TS2307/TS2305 site — is
    /// never reached. That return is reached only when a *declaration scope*
    /// encloses the declaration: a function-like body, or a namespace/ambient-module
    /// body it does not directly belong to. A container that opens no declaration
    /// scope — a bare block, an `if`/loop/`try` body, a labeled statement, a
    /// `switch` clause — leaves the declaration in the source file's own scope, and
    /// resolution still runs there.
    ///
    /// The walk therefore stops at the first scope-opening ancestor and answers
    /// from its kind, rather than testing for any single container shape.
    ///
    /// One measured exception is deliberately not encoded: a block inside
    /// `declare global { }` keeps resolving, because a global augmentation re-opens
    /// the global scope rather than introducing one. tsz reports no diagnostic at
    /// all for that shape today, so the branch would be unreachable and untestable;
    /// it is recorded here instead of written as dead code.
    pub(crate) fn position_invalid_module_element_resolves_specifier(
        &self,
        node_idx: NodeIndex,
    ) -> bool {
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
                // Function-like ancestors are the same set `is_inside_function_body`
                // walks, including a class `static { }` block (#16450).
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::CONSTRUCTOR
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
                    || k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION =>
                {
                    return false;
                }
                // Reaching a `ModuleBlock` from a position-invalid node means the
                // node is nested *inside* a namespace/module body rather than being
                // one of its own elements, so the body is a scope it sits within.
                k if k == syntax_kind_ext::MODULE_BLOCK => return false,
                k if k == syntax_kind_ext::SOURCE_FILE => return true,
                _ => continue,
            }
        }
        true
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
            if node.kind == syntax_kind_ext::MODULE_DECLARATION && node.is_global_augmentation() {
                return true;
            }
        }
        false
    }

    /// Returns `true` when `decl_idx` is (or is the name identifier of) an
    /// `export as namespace X;` declaration. These attach a global namespace
    /// alias to the containing module and do not introduce a local binding, so
    /// the TS2440 import/local-declaration conflict must ignore them.
    pub(crate) fn decl_is_namespace_export_declaration(&self, decl_idx: NodeIndex) -> bool {
        if let Some(node) = self.ctx.arena.get(decl_idx)
            && node.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
        {
            return true;
        }
        if let Some(ext) = self.ctx.arena.get_extended(decl_idx)
            && let Some(parent) = self.ctx.arena.get(ext.parent)
            && parent.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
        {
            return true;
        }
        false
    }
}
