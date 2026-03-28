//! IIFE argument-based parameter inference helpers.

use crate::query_boundaries::checkers::call::{
    array_element_type_for_type, tuple_elements_for_type,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn expanded_iife_argument_types(&mut self, args: &[NodeIndex]) -> Vec<TypeId> {
        let mut expanded = Vec::new();
        for &arg_idx in args {
            let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                continue;
            };
            if arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                && let Some(spread) = self.ctx.arena.get_spread(arg_node)
            {
                let spread_type = self.get_type_of_node(spread.expression);
                let spread_type = self.resolve_type_for_property_access(spread_type);
                let spread_type = self.resolve_lazy_type(spread_type);

                if let Some(elements) = tuple_elements_for_type(self.ctx.types, spread_type) {
                    for elem in &elements {
                        if elem.rest {
                            if let Some(inner) =
                                array_element_type_for_type(self.ctx.types, elem.type_id)
                            {
                                expanded.push(inner);
                            } else if let Some(nested) =
                                tuple_elements_for_type(self.ctx.types, elem.type_id)
                            {
                                for nested_elem in &nested {
                                    if nested_elem.rest {
                                        if let Some(inner) = array_element_type_for_type(
                                            self.ctx.types,
                                            nested_elem.type_id,
                                        ) {
                                            expanded.push(inner);
                                        }
                                    } else {
                                        expanded.push(if nested_elem.optional {
                                            self.ctx
                                                .types
                                                .factory()
                                                .union2(nested_elem.type_id, TypeId::UNDEFINED)
                                        } else {
                                            nested_elem.type_id
                                        });
                                    }
                                }
                            }
                        } else {
                            expanded.push(if elem.optional {
                                self.ctx
                                    .types
                                    .factory()
                                    .union2(elem.type_id, TypeId::UNDEFINED)
                            } else {
                                elem.type_id
                            });
                        }
                    }
                    continue;
                }

                if let Some(elem_type) = array_element_type_for_type(self.ctx.types, spread_type) {
                    expanded.push(elem_type);
                    continue;
                }
            }

            expanded.push(self.get_type_of_node(arg_idx));
        }
        expanded
    }

    pub(super) fn infer_iife_parameter_type_from_arguments(
        &mut self,
        func_idx: NodeIndex,
        param_index: usize,
        is_rest: bool,
        is_optional: bool,
    ) -> Option<TypeId> {
        use tsz_parser::parser::syntax_kind_ext::{
            CALL_EXPRESSION, NEW_EXPRESSION, PARENTHESIZED_EXPRESSION,
        };

        if !self.ctx.arena.is_immediately_invoked(func_idx) {
            return None;
        }

        let mut callee = func_idx;
        for _ in 0..100 {
            let ext = self.ctx.arena.get_extended(callee)?;
            if ext.parent.is_none() {
                return None;
            }
            let parent = ext.parent;
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == PARENTHESIZED_EXPRESSION {
                callee = parent;
                continue;
            }
            if parent_node.kind != CALL_EXPRESSION && parent_node.kind != NEW_EXPRESSION {
                return None;
            }
            let call = self.ctx.arena.get_call_expr(parent_node)?;
            if call.expression != callee {
                return None;
            }

            let args = call.arguments.as_ref().map(|a| &a.nodes);
            if is_rest {
                let Some(args) = args else {
                    return Some(self.ctx.types.array(TypeId::NEVER));
                };
                let expanded = self.expanded_iife_argument_types(args);
                if param_index >= expanded.len() {
                    return Some(self.ctx.types.array(TypeId::NEVER));
                }
                let tail = &expanded[param_index..];
                let elem = if tail.is_empty() {
                    TypeId::NEVER
                } else if tail.len() == 1 {
                    tail[0]
                } else {
                    self.ctx.types.factory().union(tail.to_vec())
                };
                return Some(self.ctx.types.array(elem));
            }

            let Some(args) = args else {
                return is_optional.then_some(TypeId::UNDEFINED);
            };
            let expanded = self.expanded_iife_argument_types(args);
            if let Some(&arg_type) = expanded.get(param_index) {
                return Some(arg_type);
            }
            return is_optional.then_some(TypeId::UNDEFINED);
        }
        None
    }
}
