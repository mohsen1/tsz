use super::FlowAnalyzer;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> FlowAnalyzer<'a> {
    pub(crate) fn array_to_enum_member_literal_type(
        &self,
        initializer: NodeIndex,
        property_name_node: NodeIndex,
    ) -> Option<TypeId> {
        let property_name = self.property_name_text(property_name_node)?;
        let initializer = self.skip_parens_and_assertions(initializer);
        let node = self.arena.get(initializer)?;
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(node)?;
        if !self.call_expression_is_array_to_enum(call.expression) {
            return None;
        }

        let first_arg = call.arguments.as_ref()?.nodes.first().copied()?;
        let first_arg = self.skip_parens_and_assertions(first_arg);
        let arg_node = self.arena.get(first_arg)?;
        if arg_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }

        let array = self.arena.get_literal_expr(arg_node)?;
        for &element in &array.elements.nodes {
            let element = self.skip_parens_and_assertions(element);
            let element_node = self.arena.get(element)?;
            if (element_node.kind == SyntaxKind::StringLiteral as u16
                || element_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16)
                && let Some(lit) = self.arena.get_literal(element_node)
                && lit.text == property_name
            {
                return Some(self.interner.literal_string(&lit.text));
            }
        }

        None
    }

    fn call_expression_is_array_to_enum(&self, callee: NodeIndex) -> bool {
        let callee = self.skip_parens_and_assertions(callee);
        let Some(node) = self.arena.get(callee) else {
            return false;
        };

        if let Some(ident) = self.arena.get_identifier(node) {
            return ident.escaped_text == "arrayToEnum";
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(node)
            && !access.question_dot_token
            && let Some(name_node) = self.arena.get(access.name_or_argument)
            && let Some(ident) = self.arena.get_identifier(name_node)
        {
            return ident.escaped_text == "arrayToEnum";
        }

        false
    }

    fn property_name_text(&self, name: NodeIndex) -> Option<String> {
        let name = self.skip_parens_and_assertions(name);
        let node = self.arena.get(name)?;
        if let Some(ident) = self.arena.get_identifier(node) {
            return Some(ident.escaped_text.clone());
        }
        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            || node.kind == SyntaxKind::NumericLiteral as u16
        {
            return self.arena.get_literal(node).map(|lit| lit.text.clone());
        }
        None
    }
}
