impl<'a> CheckerState<'a> {
    pub(crate) fn check_accessor_declaration_with_request(
        &mut self,
        member_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        use crate::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(accessor) = self.ctx.arena.get_accessor(node) else {
            return;
        };

        self.check_modifier_combinations(&accessor.modifiers);

        // Error 1183: An implementation cannot be declared in ambient contexts
        // Check if we're in a declared class and the accessor has a body.
        // TSC anchors the error at the body node (the `{`).
        if accessor.body.is_some()
            && let Some(ref class_info) = self.ctx.enclosing_class
            && class_info.is_declared
        {
            self.error_at_node(
                accessor.body,
                "An implementation cannot be declared in ambient contexts.",
                diagnostic_codes::AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS,
            );
        }

        // Error 1318: An abstract accessor cannot have an implementation
        // Abstract accessors must not have a body
        if accessor.body.is_some() && self.has_abstract_modifier(&accessor.modifiers) {
            self.error_at_node(
                member_idx,
                "An abstract accessor cannot have an implementation.",
                diagnostic_codes::METHOD_CANNOT_HAVE_AN_IMPLEMENTATION_BECAUSE_IT_IS_MARKED_ABSTRACT,
            );
        }

        let is_getter = node.kind == syntax_kind_ext::GET_ACCESSOR;

        // TS2808: A get accessor must be at least as accessible as the setter
        if is_getter {
            self.check_getter_setter_accessibility(accessor);
        }

        let has_type_annotation = is_getter && accessor.type_annotation.is_some();
        let mut return_type = if is_getter {
            if has_type_annotation {
                // Check for TS2502 using AST inspection first
                if self.is_accessor_circular_reference(
                    accessor.type_annotation,
                    accessor.name,
                    member_idx,
                ) {
                    let name = self
                        .get_property_name(accessor.name)
                        .unwrap_or_else(|| "unknown".to_string());
                    let message = format!(
                        "'{name}' is referenced directly or indirectly in its own type annotation."
                    );
                    self.error_at_node(accessor.name, &message, 2502);
                    // Use ANY to prevent further errors
                    TypeId::ANY
                } else {
                    self.get_type_from_type_node(accessor.type_annotation)
                }
            } else {
                TypeId::VOID // Default to void for getters without type annotation
            }
        } else {
            TypeId::VOID
        };

        let contextual_setter_param_types = if node.kind == syntax_kind_ext::SET_ACCESSOR {
            self.contextual_setter_parameter_types_for_class_accessor(accessor)
        } else {
            None
        };
        self.cache_parameter_types(
            &accessor.parameters.nodes,
            contextual_setter_param_types.as_deref(),
        );
        if let Some(contextual_types) = contextual_setter_param_types.as_ref() {
            for (&param_idx, contextual_type) in accessor
                .parameters
                .nodes
                .iter()
                .zip(contextual_types.iter().copied())
            {
                let Some(contextual_type) = contextual_type else {
                    continue;
                };
                self.ctx.node_types.insert(param_idx.0, contextual_type);
                if let Some(param) = self.ctx.arena.get_parameter_at(param_idx) {
                    self.ctx.node_types.insert(param.name.0, contextual_type);
                }
            }
        }

        // Check that parameter default values are assignable to declared types (TS2322)
        self.check_parameter_initializers(&accessor.parameters.nodes);

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors, not in accessors
        self.check_parameter_properties(&accessor.parameters.nodes);

        // TSC suppresses TS7006/TS7010 for private accessors in ambient (declare) classes
        let skip_implicit_any_accessor = self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|c| c.is_declared)
            && self.has_private_modifier(&accessor.modifiers);

        // Check getter parameters for TS7006 here.
        // Setter parameters are checked in check_setter_parameter() below, which also
        // validates other setter constraints (no initializer, no rest parameter).
        if is_getter && !skip_implicit_any_accessor {
            for (pi, &param_idx) in accessor.parameters.nodes.iter().enumerate() {
                if let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                {
                    let has_jsdoc = self.param_has_inline_jsdoc_type(param_idx);
                    self.maybe_report_implicit_any_parameter(param, has_jsdoc, pi);
                }
            }
        }

        // For setters, check parameter constraints (1052, 1053)
        if node.kind == syntax_kind_ext::SET_ACCESSOR {
            // TS2808: A get accessor must be at least as accessible as the setter
            // tsc emits this on BOTH the getter and setter declarations.
            self.check_setter_getter_accessibility(accessor);

            // Check if a paired getter exists — if so, setter parameter type is
            // inferred from the getter return type (contextually typed, no TS7006)
            let has_paired_getter = self.setter_has_paired_getter(member_idx, accessor);
            // Get accessor-level JSDoc to suppress TS7006 for @param annotations
            let accessor_jsdoc = self.get_jsdoc_for_function(member_idx);
            let accessor_name = if accessor.name.is_some() {
                Some(accessor.name)
            } else {
                None
            };
            self.check_setter_parameter(
                &accessor.parameters.nodes,
                has_paired_getter || skip_implicit_any_accessor,
                accessor_jsdoc.as_deref(),
                accessor_name,
            );
        }

        // Check accessor body
        if accessor.body.is_some() {
            if is_getter && !has_type_annotation {
                // Use full body-based inference for getter checking so nested returns
                // and implicit fallthrough are represented (e.g. `T | void`), which
                // aligns noImplicitReturns diagnostics with TSC behavior.
                return_type = self.infer_return_type_from_body(member_idx, accessor.body, None);
                // Cache the inferred return type so the declaration emitter can look it up
                self.ctx.node_types.insert(member_idx.0, return_type);
            }

            // TS7010 (implicit any return) is only emitted for ambient accessors,
            // matching TypeScript's behavior
            // Async getters infer Promise<void>, not 'any', so they should NOT trigger TS7010
            if is_getter {
                let is_ambient_class = self
                    .ctx
                    .enclosing_class
                    .as_ref()
                    .is_some_and(|c| c.is_declared);
                let is_ambient_file = self.ctx.is_declaration_file();
                let is_async = self.has_async_modifier(&accessor.modifiers);

                if (is_ambient_class || is_ambient_file) && !is_async && !skip_implicit_any_accessor
                {
                    let accessor_name = self.get_property_name(accessor.name);
                    self.maybe_report_implicit_any_return(
                        accessor_name,
                        Some(accessor.name),
                        return_type,
                        has_type_annotation,
                        false,
                        member_idx,
                    );
                }
            }

            // When the return type was purely inferred from the body (no annotation),
            // push ANY so check_return_statement skips the circular assignability check.
            let effective_return_type = if has_type_annotation {
                return_type
            } else {
                TypeId::ANY
            };
            self.push_return_type(effective_return_type);

            let body_request = request.read().contextual_opt(None);
            self.clear_type_cache_recursive(accessor.body);
            self.check_statement_with_request(accessor.body, &body_request);
            if is_getter {
                // Check if this is an async getter
                let is_async = self.has_async_modifier(&accessor.modifiers);
                // For async getters, extract the inner type from Promise<T>
                let mut check_return_type = self.return_type_for_implicit_return_check(
                    return_type,
                    is_async,
                    false, // getters cannot be generators
                );
                if is_async
                    && check_return_type == return_type
                    && has_type_annotation
                    && self.return_type_annotation_is_exactly_promise(accessor.type_annotation)
                {
                    check_return_type = TypeId::VOID;
                }
                let requires_return = self.requires_return_value(check_return_type);
                let has_return = self.body_has_return_with_value(accessor.body);
                let falls_through = self.function_body_falls_through(accessor.body);

                // TS2378: A 'get' accessor must return a value (regardless of type annotation)
                // Get accessors ALWAYS require a return value, even without type annotation
                if !has_return && falls_through {
                    // Use TS2378 for getters without return statements
                    self.error_at_node(
                        accessor.name,
                        "A 'get' accessor must return a value.",
                        diagnostic_codes::A_GET_ACCESSOR_MUST_RETURN_A_VALUE,
                    );
                } else if has_type_annotation && requires_return && falls_through {
                    // TS2366: always emit when return type doesn't include undefined
                    use crate::diagnostics::diagnostic_messages;
                    self.error_at_node(
                        accessor.type_annotation,
                        diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                        diagnostic_codes::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                    );
                } else if self.ctx.no_implicit_returns()
                    && has_return
                    && falls_through
                    && !self.should_skip_no_implicit_return_check(
                        check_return_type,
                        has_type_annotation,
                        false, // accessors cannot be generators
                    )
                {
                    // TS7030: noImplicitReturns - not all code paths return a value
                    // TSC points TS7030 to: return type annotation > accessor name > node itself
                    use crate::diagnostics::diagnostic_messages;
                    let error_node = if accessor.type_annotation.is_some() {
                        accessor.type_annotation
                    } else if accessor.name.is_some() {
                        accessor.name
                    } else {
                        member_idx
                    };
                    self.error_at_node(
                        error_node,
                        diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                        diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                    );
                }
            }

            self.pop_return_type();
        }

        if self.has_static_modifier(&accessor.modifiers) {
            self.check_static_member_for_class_type_param_refs(member_idx);
        }
    }

    /// Check if a setter has a paired getter with the same name in the class.
    ///
    /// TSC infers setter parameter types from the getter return type, so a setter
    /// with a paired getter has contextually typed parameters (no TS7006).
    fn setter_has_paired_getter(
        &self,
        _setter_idx: NodeIndex,
        setter_accessor: &tsz_parser::parser::node::AccessorData,
    ) -> bool {
        self.paired_getter_member_for_setter(setter_accessor)
            .is_some()
    }

    fn check_getter_setter_accessibility(
        &mut self,
        getter: &tsz_parser::parser::node::AccessorData,
    ) {
        let getter_name = match self.get_property_name(getter.name) {
            Some(n) => n,
            None => return,
        };

        let should_error = {
            let Some(ref class_info) = self.ctx.enclosing_class else {
                return;
            };
            let mut should_error = false;
            for &member_idx in &class_info.member_nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::SET_ACCESSOR {
                    continue;
                }
                let Some(setter) = self.ctx.arena.get_accessor(member_node) else {
                    continue;
                };
                let Some(setter_name) = self.get_property_name(setter.name) else {
                    continue;
                };
                if setter_name != getter_name {
                    continue;
                }

                let getter_level = self.accessibility_level(&getter.modifiers);
                let setter_level = self.accessibility_level(&setter.modifiers);
                should_error = getter_level < setter_level;
                break;
            }
            should_error
        };

        if should_error {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                getter.name,
                diagnostic_messages::A_GET_ACCESSOR_MUST_BE_AT_LEAST_AS_ACCESSIBLE_AS_THE_SETTER,
                diagnostic_codes::A_GET_ACCESSOR_MUST_BE_AT_LEAST_AS_ACCESSIBLE_AS_THE_SETTER,
            );
        }
    }

    fn accessibility_level(&self, modifiers: &Option<tsz_parser::parser::NodeList>) -> u8 {
        if self.has_private_modifier(modifiers) {
            1
        } else if self.has_protected_modifier(modifiers) {
            2
        } else {
            3 // public (explicit or implicit)
        }
    }

    fn check_setter_getter_accessibility(
        &mut self,
        setter: &tsz_parser::parser::node::AccessorData,
    ) {
        let setter_name = match self.get_property_name(setter.name) {
            Some(n) => n,
            None => return,
        };

        let should_error = {
            let Some(ref class_info) = self.ctx.enclosing_class else {
                return;
            };
            let mut should_error = false;
            for &member_idx in &class_info.member_nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::GET_ACCESSOR {
                    continue;
                }
                let Some(getter) = self.ctx.arena.get_accessor(member_node) else {
                    continue;
                };
                let Some(getter_name) = self.get_property_name(getter.name) else {
                    continue;
                };
                if getter_name != setter_name {
                    continue;
                }

                let getter_level = self.accessibility_level(&getter.modifiers);
                let setter_level = self.accessibility_level(&setter.modifiers);
                should_error = getter_level < setter_level;
                break;
            }
            should_error
        };

        if should_error {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                setter.name,
                diagnostic_messages::A_GET_ACCESSOR_MUST_BE_AT_LEAST_AS_ACCESSIBLE_AS_THE_SETTER,
                diagnostic_codes::A_GET_ACCESSOR_MUST_BE_AT_LEAST_AS_ACCESSIBLE_AS_THE_SETTER,
            );
        }
    }

    /// Resolve the symbol of a computed property name's inner expression.
    /// Returns the SymbolId if the name is a computed property with an identifier
    /// that resolves to a known symbol.
    pub(crate) fn resolve_computed_name_symbol(
        &self,
        name_idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let computed = self.ctx.arena.get_computed_property(name_node)?;
        self.ctx
            .binder
            .resolve_identifier(self.ctx.arena, computed.expression)
    }
}
