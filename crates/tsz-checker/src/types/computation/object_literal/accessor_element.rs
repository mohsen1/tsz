//! Accessor element handling for object literal type computation.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{PropertyInfo, TypeId, Visibility};

pub(super) struct ObjectLiteralAccessorContext<'b> {
    pub(super) elem_idx: NodeIndex,
    pub(super) obj_getter_names: &'b FxHashSet<String>,
    pub(super) contextual_type: Option<TypeId>,
    pub(super) marker_this_type: Option<TypeId>,
    pub(super) skip_duplicate_check: bool,
    pub(super) partial_initializer_stack_index: Option<usize>,
}

pub(super) struct ObjectLiteralAccessorState<'b> {
    pub(super) properties: &'b mut FxHashMap<Atom, PropertyInfo>,
    pub(super) setter_names: &'b mut FxHashSet<Atom>,
    pub(super) getter_names: &'b mut FxHashSet<Atom>,
    pub(super) explicit_property_names: &'b mut FxHashSet<Atom>,
    pub(super) prop_order: &'b mut u32,
    pub(super) number_index_types: &'b mut Vec<TypeId>,
    pub(super) string_index_types: &'b mut Vec<TypeId>,
    pub(super) symbol_index_types: &'b mut Vec<TypeId>,
}

impl<'a> CheckerState<'a> {
    pub(super) fn process_object_literal_accessor_element(
        &mut self,
        context: ObjectLiteralAccessorContext<'_>,
        state: ObjectLiteralAccessorState<'_>,
    ) -> bool {
        let ObjectLiteralAccessorContext {
            elem_idx,
            obj_getter_names,
            contextual_type,
            marker_this_type,
            skip_duplicate_check,
            partial_initializer_stack_index,
        } = context;
        let properties = state.properties;
        let setter_names = state.setter_names;
        let getter_names = state.getter_names;
        let explicit_property_names = state.explicit_property_names;
        let prop_order = state.prop_order;
        let number_index_types = state.number_index_types;
        let string_index_types = state.string_index_types;
        let symbol_index_types = state.symbol_index_types;

        let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
            return false;
        };
        let Some(accessor) = self.ctx.arena.get_accessor(elem_node) else {
            return false;
        };
        // Always type-check computed property name expressions for accessors,
        // even when the identifier can be resolved as a literal name.
        // E.g., `{ get [e]() {} }` needs TS2304 for undeclared `e`.
        // We call get_type_of_node directly (not check_computed_property_name)
        // to avoid triggering TS2467 for type parameters in nested object literals.
        if let Some(prop_name_node) = self.ctx.arena.get(accessor.name)
            && prop_name_node.kind == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.ctx.arena.get_computed_property(prop_name_node)
        {
            self.get_type_of_node(computed.expression);
        }
        // Missing body for accessors in object literals is a grammar error.
        // tsc does NOT emit TS1005 here; it defers to TS2378/TS1049
        // ("A 'get' accessor must have a body"). We skip TS1005 to avoid
        // false positives that incorrectly suppress TS5107 deprecation
        // warnings in the driver's grammar-error priority logic.

        // For setters, check implicit any on parameters (error 7006) and on
        // the property name itself (error 7032).
        // When a paired getter exists, the setter parameter type is inferred
        // from the getter return type (contextually typed, suppress TS7006/7032).
        if elem_node.kind == syntax_kind_ext::SET_ACCESSOR {
            let name_opt = self.get_property_name(accessor.name).or_else(|| {
                let prop_name_type = self.get_type_of_node(accessor.name);
                crate::query_boundaries::type_computation::access::literal_property_name(
                    self.ctx.types,
                    prop_name_type,
                )
                .map(|atom| self.ctx.types.resolve_atom(atom))
            });
            let has_paired_getter = name_opt
                .as_ref()
                .is_some_and(|name| obj_getter_names.contains(name));
            // Check if accessor JSDoc has @param type annotations
            let accessor_jsdoc = self.get_jsdoc_for_function(elem_idx);
            let mut first_param_lacks_annotation = false;
            for (pi, &param_idx) in accessor.parameters.nodes.iter().enumerate() {
                if let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                {
                    let has_jsdoc = has_paired_getter
                        || self.param_has_inline_jsdoc_type(param_idx)
                        || if let Some(ref jsdoc) = accessor_jsdoc {
                            let pname = self.parameter_name_for_error(param.name);
                            Self::jsdoc_has_param_type(jsdoc, &pname)
                        } else {
                            false
                        };
                    if param.type_annotation.is_none() && !has_jsdoc {
                        first_param_lacks_annotation = true;
                    }
                    self.maybe_report_implicit_any_parameter(param, has_jsdoc, pi);
                }
            }
            // TS7032: emit on property name when the setter has no parameter type
            // annotation and no paired getter (TSC checks this at accessor symbol
            // resolution time; we emit it here during object literal checking).
            if first_param_lacks_annotation
                && !has_paired_getter
                && self.ctx.no_implicit_any()
                && let Some(prop_name) = name_opt.as_deref()
            {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                            accessor.name,
                            diagnostic_codes::PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_ITS_SET_ACCESSOR_LACKS_A_PARAMETER_TYPE,
                            &[prop_name],
                        );
            }
        }

        let name_opt = if self.object_literal_computed_key_is_wide_symbol(accessor.name) {
            None
        } else {
            self.get_property_name_resolved(accessor.name)
        };
        if let Some(name) = name_opt.clone() {
            // For non-contextual object literals, TypeScript treats `this` inside
            // accessors as the object literal under construction. Provide a
            // lightweight synthetic receiver so property access checks (TS2339)
            // run during accessor body checking.
            let mut pushed_synthetic_this = false;
            if marker_this_type.is_none() {
                let mut this_props: Vec<PropertyInfo> = properties.values().cloned().collect();
                let name_atom = self.ctx.types.intern_string(&name);
                if !this_props.iter().any(|p| p.name == name_atom) {
                    // Getter-only accessors are readonly in the object type
                    let is_getter_only = elem_node.kind == syntax_kind_ext::GET_ACCESSOR
                        && !setter_names.contains(&name_atom);
                    this_props.push(PropertyInfo {
                        name: name_atom,
                        type_id: TypeId::ANY,
                        write_type: TypeId::ANY,
                        optional: false,
                        readonly: is_getter_only,
                        is_method: false,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: None,
                        declaration_order: 0,
                        is_string_named: false,
                        is_symbol_named: false,
                        single_quoted_name: false,
                    });
                }
                self.ctx
                    .this_type_stack
                    .push(self.ctx.types.factory().object(this_props));
                pushed_synthetic_this = true;
            }

            // For getter, infer return type; for setter, use the parameter type
            let accessor_type = if elem_node.kind == syntax_kind_ext::GET_ACCESSOR {
                // Check getter body/parameters via function checking, but object
                // property read type is the getter's return type (not a function type).
                self.get_type_of_function(elem_idx);
                if accessor.type_annotation.is_none() {
                    // When a contextual type exists (e.g., `T extends { [k: string]: Types }`),
                    // pass the contextual property type as the return context so that
                    // literal types from `as const` are preserved instead of widened.
                    // Without this, `get x() { return 'boolean' as const }` widens
                    // to `string` because infer_getter_return_type passes None.
                    let return_context = contextual_type.and_then(|ctx| {
                        self.contextual_object_property_type_for_lookup(ctx, &name)
                    });
                    // Clear `preserve_literal_types` around the body walk so the
                    // getter's literal-widening decision is independent of the
                    // outer `return_expression_type` scope. When the enclosing
                    // expression is itself a function's return statement
                    // (`function f() { return { get x() { return 1 } } }`), the
                    // outer scope sets the flag to preserve the obj literal's
                    // property literals, but the getter body must make its own
                    // widening decision — otherwise `readonly x: 1` leaks out
                    // where tsc emits `readonly x: number`. Mirrors the
                    // nested-function clearing already in `return_expression_type`.
                    let prev_preserve_literals = self.ctx.preserve_literal_types;
                    self.ctx.preserve_literal_types = false;
                    let result = self.infer_return_type_from_body(
                        tsz_parser::parser::NodeIndex::NONE,
                        accessor.body,
                        return_context,
                    );
                    self.ctx.preserve_literal_types = prev_preserve_literals;
                    result
                } else {
                    self.get_type_from_type_node(accessor.type_annotation)
                }
            } else {
                // Setter: type-check the function body to track variable usage
                // (especially for noUnusedParameters/noUnusedLocals checking),
                // but use the parameter type annotation for the property type
                self.get_type_of_function(elem_idx);

                // Extract setter write type from first parameter.
                // When no type annotation, fall back to the paired getter's
                // return type (mirroring tsc's inference behavior).
                accessor
                    .parameters
                    .nodes
                    .first()
                    .and_then(|&param_idx| {
                        let param = self.ctx.arena.get_parameter_at(param_idx)?;
                        if param.type_annotation.is_none() {
                            None
                        } else {
                            Some(self.get_type_from_type_node(param.type_annotation))
                        }
                    })
                    .or_else(|| {
                        // No annotation — infer from paired getter's type
                        let setter_name = name_opt.clone()?;
                        let name_atom = self.ctx.types.intern_string(&setter_name);
                        properties.get(&name_atom).map(|p| p.type_id)
                    })
                    .unwrap_or(TypeId::ANY)
            };

            if pushed_synthetic_this {
                self.ctx.this_type_stack.pop();
            }

            if elem_node.kind == syntax_kind_ext::GET_ACCESSOR {
                if accessor.type_annotation.is_none() {
                    use crate::diagnostics::diagnostic_codes;
                    let self_refs = self.collect_property_name_references(accessor.body, &name);
                    if !self_refs.is_empty() {
                        self.error_at_node_msg(
                                    accessor.name,
                                    diagnostic_codes::IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                                    &[&name],
                                );
                    }
                }

                self.maybe_report_implicit_any_return(
                    Some(name.clone()),
                    Some(accessor.name),
                    accessor_type,
                    accessor.type_annotation.is_some(),
                    false,
                    elem_idx,
                );
            }

            // TS2378: A 'get' accessor must return a value.
            // Check if the getter has a body but no return statement with a value.
            if elem_node.kind == syntax_kind_ext::GET_ACCESSOR && accessor.body.is_some() {
                let has_return = self.body_has_return_with_value(accessor.body);
                let falls_through = self.function_body_falls_through(accessor.body);

                if !has_return && falls_through {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        accessor.name,
                        "A 'get' accessor must return a value.",
                        diagnostic_codes::A_GET_ACCESSOR_MUST_RETURN_A_VALUE,
                    );
                }
            }
            let name_atom = self.ctx.types.intern_string(&name);

            // Check for duplicate property - but allow getter+setter pairs
            // A getter and setter with the same name is valid, not a duplicate
            let is_getter = elem_node.kind == syntax_kind_ext::GET_ACCESSOR;
            let is_complementary_pair = if is_getter {
                setter_names.contains(&name_atom) && !getter_names.contains(&name_atom)
            } else {
                getter_names.contains(&name_atom) && !setter_names.contains(&name_atom)
            };
            // Duplicate properties are an error in object literals.
            // TS1118 for duplicate get/set accessors, TS1117 for other duplicates.
            // Skip for computed property names — tsc only checks static names.
            if !skip_duplicate_check
                && explicit_property_names.contains(&name_atom)
                && !is_complementary_pair
                && !self.ctx.has_parse_errors
                && (!self.is_js_file() || self.ctx.js_strict_mode_diagnostics_enabled())
            {
                let is_duplicate_accessor = (is_getter && getter_names.contains(&name_atom))
                    || (!is_getter && setter_names.contains(&name_atom));
                if is_duplicate_accessor {
                    self.error_at_node(
                                accessor.name,
                                "An object literal cannot have multiple get/set accessors with the same name.",
                                diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_GET_SET_ACCESSORS_WITH_THE_SAME_NAME,
                            );
                } else {
                    let message = format_message(
                                diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                &[&name],
                            );
                    self.error_at_node(
                                accessor.name,
                                &message,
                                diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                            );
                }
            }
            explicit_property_names.insert(name_atom);

            if is_getter {
                getter_names.insert(name_atom);
            } else {
                setter_names.insert(name_atom);
            }

            let (acc_str_named, acc_sym_named, acc_single_quoted) =
                self.object_literal_member_naming_flags(accessor.name);
            // Merge getter/setter into a single property with separate
            // read (type_id) and write (write_type) types.
            if let Some(existing) = properties.get(&name_atom) {
                let existing_order = existing.declaration_order;
                let (read_type, write_type) = if is_getter {
                    // Getter arriving after setter
                    (accessor_type, existing.write_type)
                } else {
                    // Setter arriving after getter
                    (existing.type_id, accessor_type)
                };
                // Both getter and setter exist → not readonly
                let prop_info = PropertyInfo {
                    name: name_atom,
                    type_id: read_type,
                    write_type,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: existing_order,
                    is_string_named: acc_str_named || existing.is_string_named,
                    is_symbol_named: acc_sym_named || existing.is_symbol_named,
                    single_quoted_name: acc_single_quoted || existing.single_quoted_name,
                };
                properties.insert(name_atom, prop_info.clone());
                self.record_partial_object_literal_property(
                    partial_initializer_stack_index,
                    &prop_info,
                );
            } else {
                // Single accessor so far: getter-only is readonly.
                // Set-only: read type is `undefined`.
                let readonly = is_getter;
                let (read_type, write_type) = if is_getter {
                    (accessor_type, accessor_type)
                } else {
                    (TypeId::UNDEFINED, accessor_type)
                };
                let order = *prop_order;
                *prop_order += 1;
                let prop_info = PropertyInfo {
                    name: name_atom,
                    type_id: read_type,
                    write_type,
                    optional: false,
                    readonly,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: order,
                    is_string_named: acc_str_named,
                    is_symbol_named: acc_sym_named,
                    single_quoted_name: acc_single_quoted,
                };
                properties.insert(name_atom, prop_info.clone());
                self.record_partial_object_literal_property(
                    partial_initializer_stack_index,
                    &prop_info,
                );
            }
        } else {
            // Computed accessor name - still type-check the expression and body
            self.check_computed_property_name(accessor.name);

            let mut prop_name_type = TypeId::ANY;
            if let Some(prop_name_node) = self.ctx.arena.get(accessor.name)
                && prop_name_node.kind
                    == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                && let Some(computed) = self.ctx.arena.get_computed_property(prop_name_node)
            {
                prop_name_type = self.get_type_of_node(computed.expression);
                if let Some(atom) =
                    crate::query_boundaries::type_computation::access::literal_property_name(
                        self.ctx.types,
                        prop_name_type,
                    )
                {
                    let is_getter =
                        elem_node.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR;
                    let is_complementary_pair = if is_getter {
                        setter_names.contains(&atom) && !getter_names.contains(&atom)
                    } else {
                        getter_names.contains(&atom) && !setter_names.contains(&atom)
                    };
                    if !skip_duplicate_check
                        && explicit_property_names.contains(&atom)
                        && !is_complementary_pair
                        && !self.ctx.has_parse_errors
                        && (!self.is_js_file() || self.ctx.js_strict_mode_diagnostics_enabled())
                    {
                        let name = self.ctx.types.resolve_atom(atom);
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        let message = crate::diagnostics::format_message(
                                            diagnostic_messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                            &[&name],
                                        );
                        self.error_at_node(
                                            accessor.name,
                                            &message,
                                            diagnostic_codes::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                                        );
                    }
                    explicit_property_names.insert(atom);
                }
            }

            let accessor_type = if elem_node.kind == syntax_kind_ext::GET_ACCESSOR {
                self.get_type_of_function(elem_idx);

                // TS2378: A 'get' accessor must return a value.
                if accessor.body.is_some() {
                    let has_return = self.body_has_return_with_value(accessor.body);
                    let falls_through = self.function_body_falls_through(accessor.body);
                    if !has_return && falls_through {
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node(
                            accessor.name,
                            "A 'get' accessor must return a value.",
                            diagnostic_codes::A_GET_ACCESSOR_MUST_RETURN_A_VALUE,
                        );
                    }
                }

                if accessor.type_annotation.is_none() {
                    self.infer_getter_return_type(accessor.body)
                } else {
                    self.get_type_from_type_node(accessor.type_annotation)
                }
            } else {
                self.get_type_of_function(elem_idx);
                accessor
                    .parameters
                    .nodes
                    .first()
                    .and_then(|&param_idx| {
                        let param = self.ctx.arena.get_parameter_at(param_idx)?;
                        if param.type_annotation.is_none() {
                            None
                        } else {
                            Some(self.get_type_from_type_node(param.type_annotation))
                        }
                    })
                    .unwrap_or(TypeId::ANY)
            };

            self.route_computed_member_value_to_index_signature(
                prop_name_type,
                accessor_type,
                number_index_types,
                string_index_types,
                symbol_index_types,
            );
        }
        true
    }
}
