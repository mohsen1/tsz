//! Prior value-declaration detection and cached inferred variable types.
//!
//! Helpers on `CheckerState` that locate an already-computed type for a
//! variable declaration's symbol and decide whether a same-named prior
//! declaration establishes a value-typed binding (for `TS2403`/`TS2502`).
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn cached_inferred_variable_type(
        &self,
        decl_idx: NodeIndex,
        name_idx: NodeIndex,
    ) -> Option<TypeId> {
        let name_is_binding_pattern = self.ctx.arena.kind_at(name_idx).is_some_and(|kind| {
            kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                || kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
        });

        self.ctx
            .binder
            .get_node_symbol(decl_idx)
            .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id))
            .or_else(|| {
                self.ctx
                    .binder
                    .get_node_symbol(name_idx)
                    .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id))
            })
            .or_else(|| {
                name_is_binding_pattern
                    .then(|| self.ctx.node_types.get(&decl_idx.0).copied())
                    .flatten()
            })
            .or_else(|| {
                name_is_binding_pattern
                    .then(|| self.ctx.node_types.get(&name_idx.0).copied())
                    .flatten()
            })
            .filter(|&type_id| type_id != TypeId::ERROR)
    }

    pub(super) fn has_prior_value_declaration_for_symbol(&self, decl_idx: NodeIndex) -> bool {
        self.has_prior_value_declaration_for_symbol_impl(decl_idx, false)
    }

    // TS2502 variant: alias-style declarations (imports, namespace exports) do not
    // establish a value-typed binding in the redeclaring scope, so `typeof X` inside
    // a later same-named declaration is genuinely circular.  Use this variant only for
    // the circularity check; the general variant is used for symbol-type caching so
    // that module augmentations cannot overwrite a prior JS-export type.
    pub(super) fn has_prior_value_declaration_for_ts2502(&self, decl_idx: NodeIndex) -> bool {
        self.has_prior_value_declaration_for_symbol_impl(decl_idx, true)
    }

    fn has_prior_value_declaration_for_symbol_impl(
        &self,
        decl_idx: NodeIndex,
        exclude_aliases: bool,
    ) -> bool {
        let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx).or_else(|| {
            self.ctx
                .arena
                .get(decl_idx)
                .and_then(|node| self.ctx.arena.get_variable_declaration(node))
                .and_then(|decl| self.ctx.binder.get_node_symbol(decl.name))
        }) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let current_pos = self
            .ctx
            .arena
            .get(decl_idx)
            .map_or(u32::MAX, |node| node.pos);
        // Use source position to find prior declarations rather than
        // relying on declaration-list order. Hoisted `var` declarations
        // appear first in the list (before parameters) even though the
        // parameter appears earlier in source. Source-position ordering
        // correctly identifies the parameter as a prior declaration.
        //
        // Exclude block-scoped (let/const) declarations: when a `const`
        // precedes a `var` of the same name, they occupy different scoping
        // realms and the const should not be treated as a "prior value
        // declaration" for the var (that case is TS2451, not TS2403).
        symbol.declarations.iter().any(|&other| {
            if other == decl_idx || !other.is_some() {
                return false;
            }
            let has_earlier_pos = self
                .ctx
                .arena
                .get(other)
                .is_some_and(|node| node.pos < current_pos);
            if !has_earlier_pos {
                return false;
            }
            // Filter out block-scoped prior declarations (let/const/using).
            // These don't establish a prior value type for function-scoped vars.
            if let Some(other_node) = self.ctx.arena.get(other)
                && other_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                && let Some(other_ext) = self.ctx.arena.get_extended(other)
                && let Some(other_parent) = self.ctx.arena.get(other_ext.parent)
                && other_parent.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            {
                let flags = other_parent.flags as u32;
                use tsz_parser::parser::node_flags;
                if node_flags::is_block_scoped(flags) {
                    return false;
                }
            }
            // When checking for TS2502 circular references, alias-style prior
            // declarations (imports / UMD namespace exports) do not establish a
            // value-typed binding in the redeclaring scope, so `typeof X` inside
            // a later same-named `const X` declaration is genuinely circular.
            // For symbol-type caching we keep imports as valid prior declarations
            // so that module augmentations cannot overwrite a JS-export type.
            if exclude_aliases && let Some(other_node) = self.ctx.arena.get(other) {
                let kind = other_node.kind;
                if kind == syntax_kind_ext::NAMESPACE_IMPORT
                    || kind == syntax_kind_ext::IMPORT_CLAUSE
                    || kind == syntax_kind_ext::IMPORT_SPECIFIER
                    || kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    || kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                    || kind == syntax_kind_ext::NAMESPACE_EXPORT
                    || kind == syntax_kind_ext::EXPORT_SPECIFIER
                {
                    return false;
                }
                // The UMD `export as namespace foo` (and a few namespace-export
                // forms) record the export_clause identifier as the declaration
                // node; check the parent kind to filter that case as well.
                if kind == SyntaxKind::Identifier as u16
                    && let Some(other_ext) = self.ctx.arena.get_extended(other)
                    && let Some(parent_node) = self.ctx.arena.get(other_ext.parent)
                    && (parent_node.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                        || parent_node.kind == syntax_kind_ext::NAMESPACE_EXPORT
                        || parent_node.kind == syntax_kind_ext::IMPORT_CLAUSE
                        || parent_node.kind == syntax_kind_ext::NAMESPACE_IMPORT
                        || parent_node.kind == syntax_kind_ext::IMPORT_SPECIFIER
                        || parent_node.kind == syntax_kind_ext::EXPORT_SPECIFIER)
                {
                    return false;
                }
            }
            true
        })
    }
}
