use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn rewrite_recursive_static_class_expression_type(
        &self,
        prop_idx: NodeIndex,
        type_id: tsz_solver::types::TypeId,
    ) -> String {
        let printed = self.print_type_id(type_id);
        let Some(prop_node) = self.arena.get(prop_idx) else {
            return printed;
        };
        let Some(prop) = self.arena.get_property_decl(prop_node) else {
            return printed;
        };
        let Some(property_name) = self
            .arena
            .get_identifier_at(prop.name)
            .map(|ident| ident.escaped_text.clone())
        else {
            return printed;
        };
        if !self.property_initializer_is_recursive_class_expression(prop_idx, prop.initializer) {
            return printed;
        }
        let Some(interner) = self.type_interner else {
            return printed;
        };
        let Some(callable) = tsz_solver::type_queries::get_callable_shape(interner, type_id) else {
            return printed;
        };
        if !callable.properties.iter().any(|prop| {
            interner.resolve_atom(prop.name) == property_name
                && prop.type_id == tsz_solver::TypeId::ANY
        }) {
            return printed;
        }

        printed.replacen(
            &format!("{property_name}: any;"),
            &format!("{property_name}: /*elided*/ any;"),
            1,
        )
    }

    pub(in crate::declaration_emitter) fn property_initializer_is_recursive_class_expression(
        &self,
        prop_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) -> bool {
        let Some(class_expr) = self.arena.get_class_at(initializer_idx) else {
            return false;
        };
        let Some(enclosing_class_idx) = self
            .arena
            .get_extended(prop_idx)
            .map(|extended| extended.parent)
            .filter(|parent| {
                self.arena
                    .get(*parent)
                    .is_some_and(|node| node.kind == syntax_kind_ext::CLASS_DECLARATION)
            })
        else {
            return false;
        };
        let Some(enclosing_class_name) = self
            .arena
            .get_class_at(enclosing_class_idx)
            .and_then(|class| self.arena.get_identifier_at(class.name))
            .map(|ident| ident.escaped_text.clone())
        else {
            return false;
        };
        let Some(heritage_clauses) = class_expr.heritage_clauses.as_ref() else {
            return false;
        };

        heritage_clauses.nodes.iter().copied().any(|clause_idx| {
            self.arena
                .get_heritage_clause_at(clause_idx)
                .filter(|heritage| heritage.token == SyntaxKind::ExtendsKeyword as u16)
                .and_then(|heritage| heritage.types.nodes.first().copied())
                .map(|type_idx| {
                    self.arena
                        .get_expr_type_args_at(type_idx)
                        .map_or(type_idx, |expr_type_args| expr_type_args.expression)
                })
                .and_then(|expr_idx| self.arena.get_identifier_at(expr_idx))
                .is_some_and(|ident| ident.escaped_text == enclosing_class_name)
        })
    }
}
