use super::FlowAnalyzer;
use crate::query_boundaries::flow_analysis::is_unit_type;
use tsz_binder::{FlowNodeId, SymbolId};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> FlowAnalyzer<'a> {
    pub(super) fn flow_comparison_type(
        &self,
        other_node: NodeIndex,
        antecedent_id: FlowNodeId,
        allow_untyped_fallback: bool,
    ) -> Option<TypeId> {
        if let Some(node_types) = self.node_types
            && let Some(&initial_type) = node_types.get(&other_node.0)
        {
            let comparison_type = if antecedent_id.is_some() {
                self.get_flow_type(other_node, initial_type, antecedent_id)
            } else {
                initial_type
            };
            if comparison_type != TypeId::UNKNOWN || !allow_untyped_fallback {
                return Some(comparison_type);
            }
        }

        if let Some(literal_type) = self.literal_type_from_node_for_unknown_target(other_node) {
            return Some(literal_type);
        }

        if !allow_untyped_fallback {
            return None;
        }

        let other_node = self.skip_parenthesized(other_node);
        let sym_id = self.binder.resolve_identifier(self.arena, other_node)?;
        let sym_ref = tsz_solver::SymbolRef(sym_id.0);
        if let Some(env) = self.type_environment.as_ref() {
            let env = env.borrow();
            if let Some(ty) = env.get(sym_ref)
                && !matches!(ty, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
            {
                return Some(ty);
            }
        }
        self.annotation_comparison_type(sym_id).or_else(|| {
            self.resolve_symbol_to_lazy(sym_ref)
                .filter(|&ty| is_unit_type(self.interner, ty))
        })
    }

    pub(super) fn annotation_comparison_type(&self, sym_id: SymbolId) -> Option<TypeId> {
        let symbol = self.binder.get_symbol(sym_id)?;
        let mut decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol.declarations.first().copied()?
        };
        if let Some(decl_node) = self.arena.get(decl_idx)
            && decl_node.kind == SyntaxKind::Identifier as u16
            && let Some(ext) = self.arena.get_extended(decl_idx)
            && ext.parent.is_some()
        {
            decl_idx = ext.parent;
        }

        let decl_node = self.arena.get(decl_idx)?;
        let annotation = if decl_node.kind == syntax_kind_ext::PARAMETER {
            self.arena.get_parameter(decl_node)?.type_annotation
        } else if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            self.arena
                .get_variable_declaration(decl_node)?
                .type_annotation
        } else {
            return None;
        };
        let annotation_node = self.arena.get(annotation)?;
        match annotation_node.kind {
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                let op = self.arena.get_type_operator(annotation_node)?;
                if op.operator == SyntaxKind::UniqueKeyword as u16
                    && self.is_symbol_type_reference(op.type_node)
                {
                    let decl_sym = self.binder.get_node_symbol(decl_idx).unwrap_or(sym_id);
                    Some(
                        self.interner
                            .unique_symbol(tsz_solver::SymbolRef(decl_sym.0)),
                    )
                } else {
                    None
                }
            }
            k if k == SyntaxKind::ObjectKeyword as u16
                || k == syntax_kind_ext::TYPE_LITERAL
                || k == syntax_kind_ext::FUNCTION_TYPE
                || k == syntax_kind_ext::CONSTRUCTOR_TYPE =>
            {
                Some(TypeId::OBJECT)
            }
            _ => None,
        }
    }

    fn is_symbol_type_reference(&self, type_node: NodeIndex) -> bool {
        let Some(node) = self.arena.get(type_node) else {
            return false;
        };
        if node.kind == SyntaxKind::SymbolKeyword as u16 {
            return true;
        }
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.arena.get_type_ref(node) else {
            return false;
        };
        let Some(name_node) = self.arena.get(type_ref.type_name) else {
            return false;
        };
        self.arena
            .get_identifier(name_node)
            .is_some_and(|ident| ident.escaped_text == "symbol")
    }
}
