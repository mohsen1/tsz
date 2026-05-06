use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::UsageAnalyzer;

impl UsageAnalyzer<'_> {
    pub(super) fn initializer_preserves_value_reference(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => self
                .value_reference_symbol(expr_idx)
                .is_some_and(|sym_id| self.symbol_needs_typeof(sym_id)),
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let Some(access) = self.arena.get_access_expr(expr_node) else {
                    return false;
                };
                self.value_reference_symbol(access.name_or_argument)
                    .is_some_and(|sym_id| self.symbol_needs_typeof(sym_id))
                    || self
                        .value_reference_symbol(expr_idx)
                        .is_some_and(|sym_id| self.symbol_needs_typeof(sym_id))
                    || self
                        .entity_access_root_symbol(expr_idx)
                        .is_some_and(|sym_id| self.is_namespace_import_alias_symbol(sym_id))
            }
            _ => false,
        }
    }

    fn symbol_needs_typeof(&self, sym_id: SymbolId) -> bool {
        let Some(source_symbol) = self.binder.symbols.get(sym_id) else {
            return false;
        };
        let resolved_sym_id = if source_symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
            && source_symbol.import_module.is_some()
        {
            self.resolve_import_alias_target_symbol(sym_id)
                .unwrap_or(sym_id)
        } else {
            sym_id
        };
        let Some(symbol) = self.binder.symbols.get(resolved_sym_id) else {
            return false;
        };

        (symbol.has_any_flags(
            tsz_binder::symbol_flags::FUNCTION
                | tsz_binder::symbol_flags::CLASS
                | tsz_binder::symbol_flags::ENUM
                | tsz_binder::symbol_flags::VALUE_MODULE
                | tsz_binder::symbol_flags::METHOD,
        ) || self.is_namespace_import_alias_symbol(sym_id)
            || self.is_namespace_import_alias_symbol(resolved_sym_id))
            && !symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER)
    }

    fn is_namespace_import_alias_symbol(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.binder.symbols.get(sym_id) else {
            return false;
        };

        symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
            && symbol.import_module.is_some()
            && (symbol.import_name.is_none() || symbol.import_name.as_deref() == Some("*"))
    }

    fn entity_access_root_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let mut current = expr_idx;
        for _ in 0..32 {
            let node = self.arena.get(current)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                return self.value_reference_symbol(current);
            }
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = self.arena.get_access_expr(node)?;
                current = access.expression;
                continue;
            }
            return None;
        }
        None
    }

    pub(super) fn value_reference_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let expr_node = self.arena.get(expr_idx)?;

        if expr_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(&sym_id) = self.binder.node_symbols.get(&expr_idx.0) {
                return Some(sym_id);
            }

            let ident = self.arena.get_identifier(expr_node)?;
            return self
                .import_name_map
                .get(&ident.escaped_text)
                .copied()
                .or_else(|| self.binder.file_locals.get(&ident.escaped_text));
        }

        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(expr_node)?;
            return self
                .binder
                .node_symbols
                .get(&expr_idx.0)
                .copied()
                .or_else(|| {
                    self.binder
                        .node_symbols
                        .get(&access.name_or_argument.0)
                        .copied()
                });
        }

        self.binder.get_node_symbol(expr_idx)
    }
}
