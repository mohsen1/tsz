//! Type-level abstract member checking for expression-based heritage (TS2515/TS2654).
//!
//! When a class extends an expression (e.g., `extends MixedBase` where `MixedBase`
//! is a const holding a mixin call result), the base class is not a direct AST class
//! declaration. This module resolves the instance type via the solver and walks
//! contributing class declarations to find abstract members that must be implemented.

use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Type-level fallback for TS2515/TS2654 checking when the base class is not
    /// a direct class declaration.
    pub(crate) fn check_abstract_members_from_type(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
        heritage_expr_idx: Option<NodeIndex>,
        heritage_type_idx: Option<NodeIndex>,
        base_class_name: &str,
    ) {
        let Some(h_expr_idx) = heritage_expr_idx else {
            return;
        };

        let type_arguments = heritage_type_idx.and_then(|tidx| {
            self.ctx
                .arena
                .get(tidx)
                .and_then(|n| self.ctx.arena.get_expr_type_args(n))
                .and_then(|e| e.type_arguments.as_ref())
        });

        let Some(instance_type) =
            self.base_instance_type_from_expression(h_expr_idx, type_arguments)
        else {
            return;
        };

        // Collect implemented members from the derived class
        let mut implemented_members = rustc_hash::FxHashSet::default();
        for &member_idx in &class_data.members.nodes {
            if let Some(name) = self.get_member_name(member_idx)
                && !self.member_is_abstract(member_idx)
            {
                implemented_members.insert(name);
            }
        }

        // Find abstract members from the instance type
        let missing_members =
            self.find_abstract_members_in_type(instance_type, &implemented_members);

        if missing_members.is_empty() {
            return;
        }

        let is_ambient = self.has_declare_modifier(&class_data.modifiers);
        if is_ambient {
            return;
        }

        let derived_class_name = if class_data.name.is_some() {
            if let Some(name_node) = self.ctx.arena.get(class_data.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                ident.escaped_text.clone()
            } else {
                String::from("<anonymous>")
            }
        } else {
            String::from("<anonymous>")
        };

        // Determine the base class display name from the instance type.
        let type_base_name = self
            .intersection_instance_display_name(h_expr_idx, type_arguments)
            .or_else(|| self.collect_class_names_from_instance_type(instance_type))
            .unwrap_or_else(|| {
                if base_class_name.is_empty() {
                    self.format_type(instance_type)
                } else {
                    base_class_name.to_string()
                }
            });

        let is_class_expression = self
            .ctx
            .arena
            .get(class_idx)
            .is_some_and(|n| n.kind == syntax_kind_ext::CLASS_EXPRESSION);

        let error_node = if is_class_expression {
            class_idx
        } else if class_data.name.is_some() {
            class_data.name
        } else {
            class_idx
        };

        if missing_members.len() == 1 {
            if is_class_expression {
                self.error_at_node(
                    error_node,
                    &format!(
                        "Non-abstract class expression does not implement inherited abstract member '{}' from class '{}'.",
                        missing_members[0], type_base_name
                    ),
                    2653,
                );
            } else {
                self.error_at_node(
                    error_node,
                    &format!(
                        "Non-abstract class '{}' does not implement inherited abstract member {} from class '{}'.",
                        derived_class_name, missing_members[0], type_base_name
                    ),
                    diagnostic_codes::NON_ABSTRACT_CLASS_DOES_NOT_IMPLEMENT_INHERITED_ABSTRACT_MEMBER_FROM_CLASS,
                );
            }
        } else {
            let missing_list = missing_members
                .iter()
                .map(|s| format!("'{s}'"))
                .collect::<Vec<_>>()
                .join(", ");

            if is_class_expression {
                self.error_at_node(
                    error_node,
                    &format!(
                        "Non-abstract class expression is missing implementations for the following members of '{type_base_name}': {missing_list}."
                    ),
                    2656,
                );
            } else {
                self.error_at_node(
                    error_node,
                    &format!(
                        "Non-abstract class '{derived_class_name}' is missing implementations for the following members of '{type_base_name}': {missing_list}."
                    ),
                    diagnostic_codes::NON_ABSTRACT_CLASS_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBERS_OF,
                );
            }
        }
    }

    /// Walk an instance type (possibly an intersection) and find abstract members
    /// from contributing class declarations that are not in `implemented_members`.
    fn find_abstract_members_in_type(
        &self,
        instance_type: TypeId,
        implemented_members: &rustc_hash::FxHashSet<String>,
    ) -> Vec<String> {
        let mut missing = Vec::new();
        let type_ids_to_check =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, instance_type)
                .unwrap_or_else(|| vec![instance_type]);

        for type_id in type_ids_to_check {
            let Some(shape) = tsz_solver::type_queries::get_object_shape(self.ctx.types, type_id)
            else {
                continue;
            };

            // Strategy 1: class symbol on the shape itself
            if let Some(class_sym_id) = shape.symbol
                && let Some(symbol) = self.ctx.binder.get_symbol(class_sym_id)
            {
                for &decl_idx in &symbol.declarations {
                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(class_decl) = self.ctx.arena.get_class(decl_node) else {
                        continue;
                    };
                    for &member_idx in &class_decl.members.nodes {
                        if self.member_is_abstract(member_idx)
                            && let Some(name) = self.get_member_name(member_idx)
                            && !implemented_members.contains(&name)
                            && !missing.contains(&name)
                        {
                            missing.push(name);
                        }
                    }
                }
            }

            // Strategy 2: check individual properties via parent_id
            // Handles merged/flattened types where shape.symbol is None
            for prop in &shape.properties {
                let Some(parent_sym_id) = prop.parent_id else {
                    continue;
                };
                let name = self.ctx.types.resolve_atom(prop.name);
                if implemented_members.contains(&name) || missing.contains(&name) {
                    continue;
                }
                if self.is_property_abstract_via_parent(parent_sym_id, &name) {
                    missing.push(name);
                }
            }
        }

        missing
    }

    /// Collect class/interface names from an instance type by inspecting its
    /// structure. For intersections, walks each member; for objects, collects
    /// names from properties' `parent_id` symbols.
    fn collect_class_names_from_instance_type(&self, instance_type: TypeId) -> Option<String> {
        let mut names: Vec<String> = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();

        if let Some(members) =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, instance_type)
        {
            for &member_id in members.iter() {
                if let Some(shape) =
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, member_id)
                    && let Some(sym_id) = shape.symbol
                    && seen.insert(sym_id)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    && !symbol.escaped_name.is_empty()
                    && symbol.escaped_name != "__type"
                {
                    names.push(symbol.escaped_name.clone());
                }
            }
        } else if let Some(shape) =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, instance_type)
        {
            if let Some(sym_id) = shape.symbol
                && seen.insert(sym_id)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && !symbol.escaped_name.is_empty()
                && symbol.escaped_name != "__type"
            {
                names.push(symbol.escaped_name.clone());
            }
            for prop in &shape.properties {
                if let Some(parent_sym_id) = prop.parent_id
                    && seen.insert(parent_sym_id)
                    && let Some(symbol) = self.ctx.binder.get_symbol(parent_sym_id)
                    && !symbol.escaped_name.is_empty()
                    && symbol.escaped_name != "__type"
                {
                    names.push(symbol.escaped_name.clone());
                }
            }
        }

        if names.is_empty() {
            None
        } else {
            Some(names.join(" & "))
        }
    }

    /// Check if a property is abstract by tracing through its parent symbol.
    fn is_property_abstract_via_parent(
        &self,
        parent_sym_id: tsz_binder::SymbolId,
        member_name: &str,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(parent_sym_id) else {
            return false;
        };

        if let Some(ref members_table) = symbol.members
            && let Some(member_sym_id) = members_table.get(member_name)
            && let Some(member_sym) = self.ctx.binder.get_symbol(member_sym_id)
        {
            for &decl_idx in &member_sym.declarations {
                if self.member_is_abstract(decl_idx) {
                    return true;
                }
            }
        }

        // Fallback: check class declarations directly
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(class_decl) = self.ctx.arena.get_class(decl_node) else {
                continue;
            };
            for &member_idx in &class_decl.members.nodes {
                if let Some(name) = self.get_member_name(member_idx)
                    && name == member_name
                    && self.member_is_abstract(member_idx)
                {
                    return true;
                }
            }
        }

        false
    }
}
