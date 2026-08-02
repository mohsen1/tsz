//! Declaration merging across the bodies of a merged namespace.
//!
//! `tsc` gives every namespace body its own `locals` table while the merged
//! namespace symbol owns one shared `exports` table. A declaration written
//! without `export` therefore becomes a local of the body that wrote it and never
//! joins an exported declaration of the same name contributed by a *different*
//! body — `A.Point` resolves through `exports` and sees only what was exported.
//!
//! Within a single body the two declarations *do* land on one symbol; that is what
//! `tsc` reports TS2395 against, so the split here is deliberately scoped to the
//! cross-body case.

use std::sync::Arc;

use crate::state::BinderState;
use crate::{ContainerKind, SymbolId, symbol_flags};
use tsz_common::interner::AstAtom;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;

/// Declaration kinds that, when written without `export` in one body of a merged
/// namespace, become locals of that body instead of joining an exported member of
/// the same name contributed by another body.
///
/// `FUNCTION_SCOPED_VARIABLE` is covered by the older same-body-inclusive branch
/// in [`BinderState::try_declare_namespace_body_local`] and is deliberately absent
/// here so that branch keeps its existing behaviour.
const NAMESPACE_BODY_LOCAL_SHADOW_FLAGS: u32 = symbol_flags::INTERFACE
    | symbol_flags::CLASS
    | symbol_flags::FUNCTION
    | symbol_flags::TYPE_ALIAS
    | symbol_flags::ENUM
    | symbol_flags::MODULE
    | symbol_flags::BLOCK_SCOPED_VARIABLE;

/// Bound on the parent walk in [`node_is_within`]; deep enough for any real
/// nesting depth, and it stops a malformed chain from spinning.
const MAX_PARENT_WALK: usize = 512;

impl BinderState {
    /// Allocate a body-local symbol instead of merging into an exported member
    /// contributed by another body of the same namespace, returning the new
    /// `SymbolId` when the split applies.
    ///
    /// Returns `None` when the declaration should merge as before.
    pub(crate) fn try_declare_namespace_body_local(
        &mut self,
        arena: &NodeArena,
        name: &str,
        flags: u32,
        declaration: NodeIndex,
        is_exported: bool,
        existing_id: SymbolId,
        name_atom_key: Option<(usize, AstAtom)>,
    ) -> Option<SymbolId> {
        let scope = self.current_persistent_scope()?;
        if scope.kind != ContainerKind::Module {
            return None;
        }
        let container_node = scope.container_node;
        if !self.symbols.get(existing_id).is_some_and(|s| s.is_exported) || is_exported {
            return None;
        }

        // The `var` rule predates this module and applies within a single body too.
        let is_variable = (flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0;
        if !is_variable
            && !self.declaration_shadows_other_body_export(
                arena,
                declaration,
                flags,
                container_node,
                existing_id,
            )
        {
            return None;
        }

        let owned_name = name.to_string();
        let sym_id = self.symbols.alloc(flags, owned_name.clone());
        let container_sym = self.current_container_symbol();
        if let Some(sym) = self.symbols.get_mut(sym_id) {
            let span = Self::declaration_span(arena, declaration);
            sym.add_declaration(declaration, span);
            if (flags & symbol_flags::VALUE) != 0 {
                sym.set_value_declaration(declaration, span);
            }
            sym.is_exported = false;
            if let Some(parent_id) = container_sym {
                sym.parent = parent_id;
            }
        }
        Arc::make_mut(&mut self.node_symbols).insert(declaration.0, sym_id);
        self.declare_in_persistent_scope_with_atom(owned_name, name_atom_key, sym_id);
        Some(sym_id)
    }

    /// The cross-body test for every declaration kind other than `var`.
    fn declaration_shadows_other_body_export(
        &self,
        arena: &NodeArena,
        declaration: NodeIndex,
        flags: u32,
        container_node: NodeIndex,
        existing_id: SymbolId,
    ) -> bool {
        (flags & NAMESPACE_BODY_LOCAL_SHADOW_FLAGS) != 0
            && !declaration_is_exported_in_module_body(arena, declaration)
            && !self.module_body_is_export_context(arena, container_node)
            && self.existing_declarations_are_in_another_module_body(
                arena,
                existing_id,
                container_node,
            )
    }

    /// True when every declaration already recorded on `existing_id` sits outside
    /// the namespace body currently being bound.
    ///
    /// Declarations from another file (cross-file namespace merging) walk to the
    /// root of their own arena without meeting this body's container node, which
    /// is the answer we want: they are another body's contribution just as much as
    /// a second `namespace A { ... }` in the same file is.
    fn existing_declarations_are_in_another_module_body(
        &self,
        arena: &NodeArena,
        existing_id: SymbolId,
        container_node: NodeIndex,
    ) -> bool {
        if container_node.is_none() {
            return false;
        }
        let Some(existing) = self.symbols.get(existing_id) else {
            return false;
        };
        if existing.declarations.is_empty() {
            return false;
        }
        !existing
            .declarations
            .iter()
            .any(|&declaration| node_is_within(arena, declaration, container_node))
    }

    /// Whether the namespace body being bound is an *export context*, in which
    /// unmarked declarations are implicitly exported and so must still merge.
    ///
    /// `tsc`'s `setExportContextFlag` marks an ambient container as an export
    /// context, which is why `declare namespace JSX { interface IntrinsicElements
    /// {} }` in one file merges with the same shape inside `declare global` in
    /// another even though neither interface says `export`.
    fn module_body_is_export_context(&self, arena: &NodeArena, container_node: NodeIndex) -> bool {
        if self.in_global_augmentation {
            return true;
        }
        let mut current = container_node;
        for _ in 0..MAX_PARENT_WALK {
            if current.is_none() {
                return false;
            }
            if let Some(node) = arena.get(current) {
                if node.kind == syntax_kind_ext::SOURCE_FILE {
                    return false;
                }
                if node.kind == syntax_kind_ext::MODULE_DECLARATION
                    && arena.get_module(node).is_some_and(|module| {
                        Self::has_declare_modifier(arena, module.modifiers.as_ref())
                    })
                {
                    return true;
                }
            }
            let Some(extended) = arena.get_extended(current) else {
                return false;
            };
            if extended.parent == current {
                return false;
            }
            current = extended.parent;
        }
        false
    }
}

/// Whether `declaration` carries `export` where it sits in a namespace body.
///
/// Inside a module block, `export interface I {}` parses as an export declaration
/// *wrapping* the inner declaration, and the inner node's own modifier list stays
/// empty — so the `is_exported` flag threaded into `declare_symbol` reads `false`
/// for exactly the declarations that *are* exported. `populate_module_exports` and
/// the checker's `is_declaration_exported` both consult the wrapper; this does the
/// same, so the binder agrees with them.
fn declaration_is_exported_in_module_body(arena: &NodeArena, declaration: NodeIndex) -> bool {
    arena
        .get_extended(declaration)
        .and_then(|ext| arena.get(ext.parent))
        .is_some_and(|parent| parent.kind == syntax_kind_ext::EXPORT_DECLARATION)
}

/// Walk `node`'s parent chain looking for `ancestor`.
fn node_is_within(arena: &NodeArena, node: NodeIndex, ancestor: NodeIndex) -> bool {
    let mut current = node;
    for _ in 0..MAX_PARENT_WALK {
        if current.is_none() {
            return false;
        }
        if current == ancestor {
            return true;
        }
        let Some(extended) = arena.get_extended(current) else {
            return false;
        };
        if extended.parent == current {
            return false;
        }
        current = extended.parent;
    }
    false
}
