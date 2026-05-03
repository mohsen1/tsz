//! AST-based validity helpers for index-signature parameter type annotations.
//!
//! Extracted from `index_signature_checks.rs` to keep the parent module
//! under the 2000-LOC ceiling. Methods live on `CheckerState` so they can
//! reuse the type-position symbol resolver and the checker's type-parameter
//! stack.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    /// Check if a type node represents a valid index signature parameter type.
    /// Valid types: string, number, symbol keywords, template literal types,
    /// type aliases that resolve to these, unions whose members are all valid,
    /// and non-generic intersections where some member is valid.
    pub(crate) fn is_valid_index_sig_param_type(
        &self,
        type_node_kind: u16,
        type_annotation_idx: NodeIndex,
    ) -> bool {
        use crate::symbol_resolver::TypeSymbolResolution;
        use tsz_scanner::SyntaxKind;

        match type_node_kind {
            k if k == SyntaxKind::StringKeyword as u16 => true,
            k if k == SyntaxKind::NumberKeyword as u16 => true,
            k if k == SyntaxKind::SymbolKeyword as u16 => true,
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => true,
            k if k == syntax_kind_ext::UNION_TYPE => self
                .ctx
                .arena
                .get(type_annotation_idx)
                .and_then(|n| self.ctx.arena.get_composite_type(n))
                .is_some_and(|composite| {
                    composite.types.nodes.iter().all(|&m| {
                        self.ctx
                            .arena
                            .get(m)
                            .is_some_and(|mn| self.is_valid_index_sig_param_type(mn.kind, m))
                    })
                }),
            k if k == syntax_kind_ext::INTERSECTION_TYPE => self
                .ctx
                .arena
                .get(type_annotation_idx)
                .and_then(|n| self.ctx.arena.get_composite_type(n))
                .is_some_and(|composite| {
                    composite.types.nodes.iter().any(|&m| {
                        self.ctx
                            .arena
                            .get(m)
                            .is_some_and(|mn| self.is_valid_index_sig_param_type(mn.kind, m))
                    })
                }),
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let Some(type_node) = self.ctx.arena.get(type_annotation_idx) else {
                    return false;
                };
                let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) else {
                    return false;
                };
                if let Some(name_node) = self.ctx.arena.get(type_ref.type_name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                {
                    let name = ident.escaped_text.as_str();
                    if matches!(name, "string" | "number" | "symbol") {
                        return true;
                    }
                }
                if let TypeSymbolResolution::Type(sym_id) =
                    self.resolve_identifier_symbol_in_type_position(type_ref.type_name)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    && (symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0
                    && let Some(&decl_idx) = symbol.declarations.first()
                    && let Some(decl_node) = self.ctx.arena.get(decl_idx)
                    && let Some(type_alias) = self.ctx.arena.get_type_alias(decl_node)
                    && let Some(alias_type_node) = self.ctx.arena.get(type_alias.type_node)
                {
                    return self
                        .is_valid_index_sig_param_type(alias_type_node.kind, type_alias.type_node);
                }
                false
            }
            _ => false,
        }
    }

    /// Check if the type annotation of an index signature parameter is a type
    /// parameter or a literal type (triggers TS1337 instead of TS1268).
    pub(crate) fn is_type_param_or_literal_in_index_sig(
        &self,
        type_node_kind: u16,
        type_annotation_idx: NodeIndex,
    ) -> bool {
        use crate::symbol_resolver::TypeSymbolResolution;
        use tsz_scanner::SyntaxKind;

        if type_node_kind == syntax_kind_ext::LITERAL_TYPE
            || type_node_kind == SyntaxKind::StringLiteral as u16
            || type_node_kind == SyntaxKind::NumericLiteral as u16
            || type_node_kind == SyntaxKind::TrueKeyword as u16
            || type_node_kind == SyntaxKind::FalseKeyword as u16
        {
            return true;
        }

        if type_node_kind == syntax_kind_ext::UNION_TYPE
            || type_node_kind == syntax_kind_ext::INTERSECTION_TYPE
        {
            return self
                .ctx
                .arena
                .get(type_annotation_idx)
                .and_then(|n| self.ctx.arena.get_composite_type(n))
                .is_some_and(|composite| {
                    composite.types.nodes.iter().any(|&m| {
                        self.ctx.arena.get(m).is_some_and(|mn| {
                            self.is_type_param_or_literal_in_index_sig(mn.kind, m)
                        })
                    })
                });
        }

        if type_node_kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_node) = self.ctx.arena.get(type_annotation_idx)
            && let Some(type_ref) = self.ctx.arena.get_type_ref(type_node)
        {
            if let TypeSymbolResolution::Type(sym_id) =
                self.resolve_identifier_symbol_in_type_position(type_ref.type_name)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                if (symbol.flags & tsz_binder::symbol_flags::TYPE_PARAMETER) != 0 {
                    return true;
                }
                if (symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0
                    && let Some(&decl_idx) = symbol.declarations.first()
                    && let Some(decl_node) = self.ctx.arena.get(decl_idx)
                    && let Some(type_alias) = self.ctx.arena.get_type_alias(decl_node)
                    && let Some(alias_type_node) = self.ctx.arena.get(type_alias.type_node)
                {
                    return self.is_type_param_or_literal_in_index_sig(
                        alias_type_node.kind,
                        type_alias.type_node,
                    );
                }
            }
            // Fallback: checker's type parameter stack (covers type params from
            // type aliases/generics not registered in the binder symbol table).
            if let Some(name_node) = self.ctx.arena.get(type_ref.type_name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                && self
                    .lookup_type_parameter(ident.escaped_text.as_str())
                    .is_some()
            {
                return true;
            }
        }

        false
    }
}
