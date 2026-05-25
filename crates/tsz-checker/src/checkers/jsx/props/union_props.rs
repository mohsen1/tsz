//! JSX union prop compatibility and display helpers.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn report_jsx_body_children_excess_property(
        &mut self,
        tag_name_idx: NodeIndex,
        display_target: &str,
        provided_attrs: &[(String, TypeId)],
    ) {
        let mut ordered_attrs: Vec<(String, TypeId)> = Vec::with_capacity(provided_attrs.len());
        if let Some((_, children_type)) = provided_attrs.iter().find(|(name, _)| name == "children")
        {
            ordered_attrs.push(("children".to_string(), *children_type));
        }
        ordered_attrs.extend(
            provided_attrs
                .iter()
                .filter(|(name, _)| name != "children")
                .cloned(),
        );

        let properties: Vec<tsz_solver::PropertyInfo> = ordered_attrs
            .iter()
            .map(|(name, type_id)| {
                let name_atom = self.ctx.types.intern_string(name);
                let display_type = if name == "children" {
                    self.jsx_children_display_type(*type_id)
                } else {
                    *type_id
                };
                tsz_solver::PropertyInfo {
                    name: name_atom,
                    type_id: display_type,
                    write_type: display_type,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: tsz_solver::Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                    is_string_named: false,
                    is_symbol_named: false,
                    single_quoted_name: false,
                }
            })
            .collect();
        let source_type = self.format_type(self.ctx.types.factory().object(properties));
        let base = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_type, display_target],
        );
        let message =
            format!("{base}\n  Property 'children' does not exist on type '{display_target}'.");
        self.error_at_node(
            tag_name_idx,
            &message,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );
    }

    pub(crate) fn check_jsx_union_props(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
        display_target: &str,
        tag_name_idx: NodeIndex,
        children_ctx: Option<crate::checkers_domain::JsxChildrenContext>,
    ) {
        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        let attr_nodes = &attrs.properties.nodes;
        let mut provided_attrs: Vec<(String, TypeId, bool)> = Vec::new();
        let mut has_spread = false;

        for &attr_idx in attr_nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };

            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                    continue;
                };
                let Some(attr_name) = self.get_jsx_attribute_name(name_node) else {
                    continue;
                };

                if attr_name == "key" || attr_name == "ref" {
                    continue;
                }

                if attr_data.initializer.is_some() {
                    let value_idx =
                        if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                            if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                                self.ctx
                                    .arena
                                    .get_jsx_expression(init_node)
                                    .map(|e| e.expression)
                                    .unwrap_or(attr_data.initializer)
                            } else {
                                attr_data.initializer
                            }
                        } else {
                            attr_data.initializer
                        };
                    if let Some(value_node) = self.ctx.arena.get(value_idx)
                        && (value_node.kind == syntax_kind_ext::ARROW_FUNCTION
                            || value_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
                    {
                        return;
                    }
                }

                let is_shorthand = attr_data.initializer.is_none();
                let attr_type = if is_shorthand {
                    TypeId::BOOLEAN_TRUE
                } else if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                    let value_idx = if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                        self.ctx
                            .arena
                            .get_jsx_expression(init_node)
                            .map(|e| e.expression)
                            .unwrap_or(attr_data.initializer)
                    } else {
                        attr_data.initializer
                    };
                    let prev = self.ctx.preserve_literal_types;
                    self.ctx.preserve_literal_types = true;
                    let t = self.compute_type_of_node(value_idx);
                    self.ctx.preserve_literal_types = prev;
                    t
                } else {
                    TypeId::ANY
                };

                provided_attrs.push((attr_name, attr_type, is_shorthand));
            } else if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                has_spread = true;
            }
        }

        if let Some(children) = children_ctx {
            provided_attrs.push((
                self.get_jsx_children_prop_name(),
                children.synthesized_type,
                false,
            ));
        }

        if has_spread {
            return;
        }

        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, props_type)
        else {
            return;
        };
        let provided_names: rustc_hash::FxHashSet<&str> = provided_attrs
            .iter()
            .map(|(name, _, _)| name.as_str())
            .collect();

        let mut any_member_compatible = false;
        for &member in &members {
            let member_resolved = self.resolve_type_for_property_access(member);
            let all_attrs_compatible = provided_attrs.iter().all(|(name, attr_type, _)| {
                use crate::query_boundaries::common::PropertyAccessResult;
                match self.resolve_property_access_with_env(member_resolved, name) {
                    PropertyAccessResult::Success { type_id, .. } => {
                        let expected = crate::query_boundaries::common::remove_undefined(
                            self.ctx.types,
                            type_id,
                        );
                        if *attr_type == TypeId::ANY || *attr_type == TypeId::ERROR {
                            return true;
                        }
                        self.diagnostic_relation_boolean_guard(*attr_type, expected)
                    }
                    _ => true,
                }
            });

            if !all_attrs_compatible {
                continue;
            }

            let all_required_present = if let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    member_resolved,
                ) {
                shape.properties.iter().all(|prop| {
                    if prop.optional {
                        return true;
                    }
                    let prop_name = self.ctx.types.resolve_atom(prop.name);
                    provided_names.contains(prop_name.as_str())
                })
            } else {
                true
            };

            if all_required_present {
                any_member_compatible = true;
                break;
            }
        }

        if any_member_compatible {
            return;
        }

        let any_attr_has_type_params = provided_attrs.iter().any(|(_, attr_type, _)| {
            crate::query_boundaries::common::contains_type_parameters(self.ctx.types, *attr_type)
        });
        if any_attr_has_type_params {
            return;
        }

        let properties: Vec<tsz_solver::PropertyInfo> = provided_attrs
            .iter()
            .map(|(name, type_id, is_shorthand)| {
                let name_atom = self.ctx.types.intern_string(name);
                let display_type = if *is_shorthand && *type_id == TypeId::BOOLEAN_TRUE {
                    TypeId::BOOLEAN
                } else if name == "children" {
                    self.jsx_children_display_type(*type_id)
                } else {
                    *type_id
                };
                tsz_solver::PropertyInfo {
                    name: name_atom,
                    type_id: display_type,
                    write_type: display_type,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: tsz_solver::Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                    is_string_named: false,
                    is_symbol_named: false,
                    single_quoted_name: false,
                }
            })
            .collect();
        let attrs_type = self.ctx.types.factory().object(properties);
        if self.diagnostic_relation_boolean_guard(attrs_type, props_type) {
            return;
        }
        self.report_jsx_synthesized_props_assignability_error(
            attrs_type,
            display_target,
            tag_name_idx,
        );
    }

    pub(super) fn jsx_props_type_is_library_managed_attributes_application(
        &mut self,
        type_id: TypeId,
    ) -> bool {
        self.jsx_library_managed_attributes_application_args(type_id)
            .is_some()
    }

    pub(super) fn jsx_library_managed_attributes_application_args(
        &mut self,
        type_id: TypeId,
    ) -> Option<Vec<TypeId>> {
        let (base, args) = crate::query_boundaries::state::type_environment::application_info(
            self.ctx.types,
            type_id,
        )?;
        let sym_id = self.ctx.resolve_type_to_symbol_id(base)?;
        self.get_symbol_globally(sym_id)
            .is_some_and(|symbol| symbol.escaped_name == "LibraryManagedAttributes")
            .then_some(args)
    }

    pub(super) fn jsx_library_managed_attributes_application_display(
        &mut self,
        type_id: TypeId,
    ) -> Option<String> {
        let args = self.jsx_library_managed_attributes_application_args(type_id)?;
        if args.len() != 2 {
            return None;
        }
        Some(format!(
            "LibraryManagedAttributes<{}, {}>",
            self.format_type(args[0]),
            self.jsx_library_managed_structural_props_display(args[1])
                .unwrap_or_else(|| self.format_type(args[1]))
        ))
    }
}
