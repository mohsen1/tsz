//! Merged value/type-alias helpers for `typeof` type queries.

use super::type_node::TypeNodeChecker;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{ObjectShape, PropertyInfo, TupleElement, TypeId};

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    pub(crate) fn property_name_text(&self, name: tsz_parser::parser::NodeIndex) -> Option<String> {
        let name = self.ctx.arena.skip_parenthesized_and_assertions(name);
        let node = self.ctx.arena.get(name)?;
        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            return Some(ident.escaped_text.clone());
        }
        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            || node.kind == SyntaxKind::NumericLiteral as u16
        {
            return self.ctx.arena.get_literal(node).map(|lit| lit.text.clone());
        }
        None
    }

    pub(crate) fn literal_type_from_const_member_initializer(
        &self,
        initializer: tsz_parser::parser::NodeIndex,
    ) -> Option<TypeId> {
        let initializer = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(initializer);
        let node = self.ctx.arena.get(initializer)?;
        let factory = self.ctx.types.factory();
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                self.ctx
                    .arena
                    .get_literal(node)
                    .map(|lit| factory.literal_string(&lit.text))
            }
            k if k == SyntaxKind::NumericLiteral as u16 => self
                .ctx
                .arena
                .get_literal(node)
                .and_then(|lit| {
                    lit.value
                        .or_else(|| tsz_common::numeric::parse_numeric_literal_value(&lit.text))
                })
                .map(|value| factory.literal_number(value)),
            k if k == SyntaxKind::TrueKeyword as u16 => Some(factory.literal_boolean(true)),
            k if k == SyntaxKind::FalseKeyword as u16 => Some(factory.literal_boolean(false)),
            k if k == SyntaxKind::NullKeyword as u16 => Some(TypeId::NULL),
            k if k == SyntaxKind::UndefinedKeyword as u16 => Some(TypeId::UNDEFINED),
            _ => None,
        }
    }

    pub(crate) fn compute_safe_merged_value_type_for_type_query(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut decl = symbol.value_declaration;
        if let Some(decl_node) = self.ctx.arena.get(decl)
            && decl_node.kind == SyntaxKind::Identifier as u16
        {
            decl = self.ctx.arena.get_extended(decl)?.parent;
        }

        let decl_node = self.ctx.arena.get(decl)?;
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        if var_decl.type_annotation.is_some() || var_decl.initializer.is_none() {
            return None;
        }

        let initializer = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(var_decl.initializer);
        let init_node = self.ctx.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let literal = self.ctx.arena.get_literal_expr(init_node)?;
        let mut props = Vec::new();
        for &element in &literal.elements.nodes {
            let element_node = self.ctx.arena.get(element)?;
            if element_node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
                return None;
            }
            let prop = self.ctx.arena.get_property_assignment(element_node)?;
            let name = self.property_name_text(prop.name)?;
            let type_id = self.literal_type_from_const_member_initializer(prop.initializer)?;
            let mut info = PropertyInfo::new(self.ctx.types.intern_string(&name), type_id);
            info.write_type = type_id;
            info.readonly = true;
            info.declaration_order = props.len() as u32;
            props.push(info);
        }

        Some(self.ctx.types.factory().object_with_index(ObjectShape {
            properties: props,
            ..ObjectShape::default()
        }))
    }

    pub(crate) fn const_asserted_array_tuple_type_query(
        &self,
        expr_name: tsz_parser::parser::NodeIndex,
    ) -> Option<TypeId> {
        let expr_name = self.ctx.arena.skip_parenthesized_and_assertions(expr_name);
        let node = self.ctx.arena.get(expr_name)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expr_name)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE) {
            return None;
        }

        let mut decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol.primary_declaration()?
        };
        let mut decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind == SyntaxKind::Identifier as u16 {
            decl_idx = self.ctx.arena.get_extended(decl_idx)?.parent;
            decl_node = self.ctx.arena.get(decl_idx)?;
        }
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION
            || !self.ctx.arena.is_const_variable_declaration(decl_idx)
        {
            return None;
        }

        let decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let assertion_expr = self.ctx.arena.skip_parenthesized(decl.initializer);
        let initializer_is_const_assertion = self
            .ctx
            .arena
            .get(assertion_expr)
            .and_then(|node| self.ctx.arena.get_type_assertion(node))
            .and_then(|assertion| self.ctx.arena.get(assertion.type_node))
            .is_some_and(|type_node| type_node.kind == SyntaxKind::ConstKeyword as u16);
        if !initializer_is_const_assertion {
            return None;
        }

        let initializer = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(decl.initializer);
        let init_node = self.ctx.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }

        let array = self.ctx.arena.get_literal_expr(init_node)?;
        let mut elements = Vec::with_capacity(array.elements.nodes.len());
        for &element in &array.elements.nodes {
            if element.is_none() {
                return None;
            }
            let element_type = self.literal_type_from_const_member_initializer(element)?;
            elements.push(TupleElement {
                type_id: element_type,
                name: None,
                optional: false,
                rest: false,
            });
        }

        Some(self.ctx.types.factory().tuple(elements))
    }
}
