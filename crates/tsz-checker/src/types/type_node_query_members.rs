//! Type query member helpers.

use super::type_node::TypeNodeChecker;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    pub(crate) fn value_property_type_query(&self, expr_name: NodeIndex) -> Option<TypeId> {
        let expr_name = self.ctx.arena.skip_parenthesized_and_assertions(expr_name);
        let node = self.ctx.arena.get(expr_name)?;
        let (base, property_name_node) = if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
            let access = self.ctx.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            (access.expression, access.name_or_argument)
        } else if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qualified = self.ctx.arena.get_qualified_name(node)?;
            (qualified.left, qualified.right)
        } else {
            return None;
        };

        let property_name = self.property_name_text(property_name_node)?;
        let base_type = self.value_type_for_type_query_member_base(base)?;
        let evaluated_base =
            crate::query_boundaries::state::type_environment::evaluate_type_with_cache(
                self.ctx.types,
                &*self.ctx,
                base_type,
                std::iter::empty(),
                false,
                self.ctx.is_declaration_file() || self.ctx.emit_declarations(),
            )
            .result;
        let base_type = if evaluated_base != TypeId::ERROR {
            evaluated_base
        } else {
            base_type
        };
        match crate::query_boundaries::property_access::resolve_property_access(
            self.ctx.types,
            base_type,
            &property_name,
        ) {
            tsz_solver::operations::property::PropertyAccessResult::Success { type_id, .. }
            | tsz_solver::operations::property::PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: Some(type_id),
                ..
            } if type_id != TypeId::ANY && type_id != TypeId::ERROR => Some(type_id),
            _ => None,
        }
    }

    pub(crate) fn value_type_for_type_query_member_base(
        &self,
        expr_name: NodeIndex,
    ) -> Option<TypeId> {
        let expr_name = self.ctx.arena.skip_parenthesized_and_assertions(expr_name);
        let node = self.ctx.arena.get(expr_name)?;
        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let ident = self.ctx.arena.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            if name == "default" {
                return None;
            }
            let sym_id = self
                .ctx
                .binder
                .resolve_identifier(self.ctx.arena, expr_name)?;
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if symbol.flags & tsz_binder::symbol_flags::VALUE == 0 {
                return None;
            }
            let mut type_id = {
                let env = self.ctx.type_environment.borrow();
                tsz_solver::TypeResolver::resolve_type_query(
                    &*env,
                    tsz_solver::SymbolRef(sym_id.0),
                    self.ctx.types,
                )
            }
            .or_else(|| self.ctx.symbol_types.get(&sym_id).copied())?;
            if self
                .symbol_is_bare_const_object_literal(sym_id)
                .unwrap_or(false)
            {
                type_id = self.widen_mutable_object_literal_property_types(type_id);
            }
            return (type_id != TypeId::ANY && type_id != TypeId::ERROR).then_some(type_id);
        }

        self.value_property_type_query(expr_name)
    }

    fn symbol_is_bare_const_object_literal(&self, sym_id: tsz_binder::SymbolId) -> Option<bool> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
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
            return Some(false);
        }

        let decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let assertion_expr = self.ctx.arena.skip_parenthesized(decl.initializer);
        if self
            .ctx
            .arena
            .get(assertion_expr)
            .and_then(|node| self.ctx.arena.get_type_assertion(node))
            .and_then(|assertion| self.ctx.arena.get(assertion.type_node))
            .is_some_and(|type_node| type_node.kind == SyntaxKind::ConstKeyword as u16)
        {
            return Some(false);
        }
        if self.ctx.arena.get(assertion_expr).is_some_and(|node| {
            matches!(
                node.kind,
                syntax_kind_ext::SATISFIES_EXPRESSION
                    | syntax_kind_ext::AS_EXPRESSION
                    | syntax_kind_ext::TYPE_ASSERTION
            )
        }) {
            return Some(false);
        }

        let initializer = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(decl.initializer);
        Some(
            self.ctx
                .arena
                .get(initializer)
                .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION),
        )
    }

    fn widen_mutable_object_literal_property_types(&self, type_id: TypeId) -> TypeId {
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
        else {
            return type_id;
        };

        let mut widened_shape = shape.as_ref().clone();
        let mut changed = false;
        for prop in &mut widened_shape.properties {
            let widened_read =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, prop.type_id);
            let widened_write = crate::query_boundaries::common::widen_literal_type(
                self.ctx.types,
                prop.write_type,
            );
            if widened_read != prop.type_id || widened_write != prop.write_type {
                changed = true;
            }
            prop.type_id = widened_read;
            prop.write_type = widened_write;
        }

        if changed {
            self.ctx.types.factory().object_with_index(widened_shape)
        } else {
            type_id
        }
    }
}
