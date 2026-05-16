//! JSX union-props attribute collection helpers.

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn collect_jsx_union_resolution_attr_value_type(
        &mut self,
        value_idx: NodeIndex,
        allow_function_types: bool,
    ) -> Option<TypeId> {
        let Some(value_node) = self.ctx.arena.get(value_idx) else {
            return Some(TypeId::ANY);
        };
        if !allow_function_types
            && matches!(
                value_node.kind,
                syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
            )
        {
            return None;
        }

        let prev = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let ty = self.compute_type_of_node(value_idx);
        self.ctx.preserve_literal_types = prev;
        Some(ty)
    }

    pub(crate) fn collect_jsx_union_resolution_spread_attrs(
        &mut self,
        expr_idx: NodeIndex,
        provided: &mut Vec<(String, Option<TypeId>)>,
    ) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(obj_lit) = self.ctx.arena.get_literal_expr(expr_node) else {
            return false;
        };

        for &elem_idx in &obj_lit.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
                        continue;
                    };
                    let Some(name) = self.get_property_name(prop.name) else {
                        return false;
                    };
                    let ty =
                        self.collect_jsx_union_resolution_attr_value_type(prop.initializer, true);
                    provided.push((name, ty));
                }
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.ctx.arena.get_shorthand_property(elem_node) else {
                        continue;
                    };
                    let Some(name) = self.get_property_name(prop.name) else {
                        return false;
                    };
                    let ty = self.collect_jsx_union_resolution_attr_value_type(prop.name, true);
                    provided.push((name, ty));
                }
                syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR => {
                    let name = match elem_node.kind {
                        syntax_kind_ext::METHOD_DECLARATION => self
                            .ctx
                            .arena
                            .get_method_decl(elem_node)
                            .and_then(|method| self.get_property_name(method.name)),
                        syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => self
                            .ctx
                            .arena
                            .get_accessor(elem_node)
                            .and_then(|accessor| self.get_property_name(accessor.name)),
                        _ => None,
                    };
                    let Some(name) = name else {
                        return false;
                    };
                    let ty = self.collect_jsx_union_resolution_attr_value_type(elem_idx, true);
                    provided.push((name, ty));
                }
                syntax_kind_ext::SPREAD_ASSIGNMENT | syntax_kind_ext::SPREAD_ELEMENT => {
                    let Some(spread) = self.ctx.arena.get_spread(elem_node) else {
                        return false;
                    };
                    if !self.collect_jsx_union_resolution_spread_attrs(spread.expression, provided)
                    {
                        return false;
                    }
                }
                _ => return false,
            }
        }

        true
    }
}
