//! Declaration emitter - parameter and heritage clause emission.

use super::super::DeclarationEmitter;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(crate) fn emit_parameters(&mut self, params: &NodeList) {
        self.emit_parameters_with_body(params, NodeIndex::NONE);
    }

    pub(crate) fn emit_parameters_with_body(&mut self, params: &NodeList, body_idx: NodeIndex) {
        // Find the index of the last required parameter (no ?, no initializer, no rest).
        // Parameters with initializers before the last required param cannot use `?` syntax;
        // instead they emit `param: Type | undefined` (matching tsc behavior).
        let last_required_idx = params
            .nodes
            .iter()
            .rposition(|&idx| {
                self.arena
                    .get(idx)
                    .and_then(|n| self.arena.get_parameter(n))
                    .is_some_and(|p| {
                        !p.question_token && p.initializer.is_none() && !p.dot_dot_dot_token
                    })
            })
            .unwrap_or(0);

        let mut first = true;
        let mut previous_param_end = 0;
        for (i, &param_idx) in params.nodes.iter().enumerate() {
            if !first {
                self.write(", ");
            }
            first = false;

            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                let jsdoc_param = if self.source_is_js_file {
                    self.jsdoc_param_decl_for_parameter(param_idx, i)
                } else {
                    None
                };
                let jsdoc_satisfies_param =
                    if self.source_is_js_file && self.use_jsdoc_satisfies_parameter_fallback {
                        self.jsdoc_satisfies_param_decl_for_parameter(param_idx, i)
                    } else {
                        None
                    };
                let effective_jsdoc_param = jsdoc_param.as_ref().or(jsdoc_satisfies_param.as_ref());
                let is_parameter_property = self.in_constructor_params
                    && self.parameter_has_property_modifier(&param.modifiers);

                // For public parameter properties, tsc appends `| undefined` to the
                // constructor parameter type as well as the property declaration.
                // For private/protected parameter properties, the type is hidden on
                // the property (`private x?;`) so no `| undefined` is added to the
                // constructor parameter.
                let is_private_param_property = is_parameter_property
                    && param.modifiers.as_ref().is_some_and(|mods| {
                        mods.nodes.iter().any(|&mod_idx| {
                            self.arena
                                .get(mod_idx)
                                .is_some_and(|n| n.kind == SyntaxKind::PrivateKeyword as u16)
                        })
                    });

                // Inline JSDoc comment before parameter (e.g. /** comment */ a: string)
                let comment_pos = self
                    .arena
                    .get(param.name)
                    .map_or(param_node.pos, |name_node| name_node.pos);
                if self.strip_internal && self.in_constructor_params {
                    self.emit_strip_internal_constructor_parameter_comment(
                        comment_pos,
                        previous_param_end,
                        params.nodes.len() == 1,
                    );
                } else {
                    self.emit_inline_parameter_comment(comment_pos);
                }

                // Modifiers (public, private, etc for constructor parameters)
                self.emit_member_modifiers(&param.modifiers);

                // Rest parameter
                if param.dot_dot_dot_token || jsdoc_param.as_ref().is_some_and(|decl| decl.rest) {
                    self.write("...");
                }

                // Name
                self.emit_node(param.name);

                // A parameter with an initializer that appears before the last required
                // parameter is NOT optional — you can't omit it. Instead, its type
                // gets `| undefined` appended. Explicitly optional (?) params always use `?`.
                let has_initializer_before_required =
                    param.initializer.is_some() && !param.question_token && i < last_required_idx;

                if param.question_token
                    || effective_jsdoc_param
                        .as_ref()
                        .is_some_and(|decl| decl.optional && !decl.rest)
                    || (param.initializer.is_some() && !has_initializer_before_required)
                {
                    self.write("?");
                }

                // Type
                if param.type_annotation.is_some() {
                    self.write(": ");
                    let before_type = self.writer.len();
                    if let Some(rescued) = self.rescued_asserts_parameter_type_text(param_idx) {
                        self.write(&rescued);
                    } else if let Some(type_text) =
                        self.preferred_annotation_name_text(param.type_annotation)
                    {
                        self.write(&type_text);
                    } else if self.normalize_string_literal_type_quotes
                        && let Some(type_text) =
                            self.emit_type_node_text_normalized(param.type_annotation)
                    {
                        self.write(&type_text);
                    } else {
                        self.emit_type(param.type_annotation);
                    }
                    // For non-private parameter properties with `?`, tsc appends
                    // `| undefined` to both the property declaration and the constructor
                    // parameter type. For private params, the type is hidden so skip.
                    if is_parameter_property
                        && !is_private_param_property
                        && param.question_token
                        && !self
                            .type_annotation_semantically_includes_undefined(param.type_annotation)
                    {
                        let output = self.writer.get_output();
                        let type_text = &output[before_type..];
                        if !output.ends_with("| undefined")
                            && !Self::type_text_has_undefined_branch(type_text)
                        {
                            self.write(" | undefined");
                        }
                    }
                } else if let Some(type_text) =
                    self.jsdoc_object_binding_param_type_literal(param_idx, i)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else if let Some(jsdoc_param) = effective_jsdoc_param
                    && !Self::jsdoc_type_needs_checker_resolution(&jsdoc_param.type_text)
                {
                    self.write(": ");
                    let type_text =
                        self.jsdoc_type_text_for_declaration_emit(&jsdoc_param.type_text);
                    self.write(&type_text);
                    if jsdoc_param.optional && !Self::type_text_has_undefined_branch(&type_text) {
                        self.write(" | undefined");
                    }
                } else if let Some(jsdoc_param) = effective_jsdoc_param
                    && Self::jsdoc_type_needs_checker_resolution(&jsdoc_param.type_text)
                    && let Some(converted) =
                        Self::convert_jsdoc_function_type(&jsdoc_param.type_text)
                {
                    self.write(": ");
                    self.write(&converted);
                } else if let Some(type_text) = self.binding_pattern_parameter_type_text(
                    param_idx,
                    param.name,
                    param.initializer,
                ) {
                    self.write(": ");
                    self.write(&type_text);
                } else if param.initializer.is_some()
                    && let Some(type_text) =
                        self.conditional_boolean_undefined_default_type_text(param.initializer)
                {
                    self.write(": ");
                    self.write(type_text);
                } else if param.initializer.is_some()
                    && let Some(type_text) =
                        self.widened_inferred_expression_type_text(param.initializer)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else if let Some(type_id) = self.parameter_type_for_emit(param_idx, param.name) {
                    // Inferred type from type cache
                    self.write(": ");
                    self.write(&self.print_type_id(type_id));
                } else if param.initializer.is_some()
                    && let Some(type_text) =
                        self.allowlisted_initializer_type_text(param.initializer)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else if param.dot_dot_dot_token {
                    // Rest parameters without explicit type → any[]
                    self.write(": any[]");
                } else if !self.source_is_declaration_file {
                    // Empty object binding pattern `{}` without a type annotation
                    // gets type `{}` (not `any`), matching tsc behavior.
                    let is_empty_object_binding = self.arena.get(param.name).is_some_and(|n| {
                        n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            && self
                                .arena
                                .get_binding_pattern(n)
                                .is_none_or(|bp| bp.elements.nodes.is_empty())
                    });
                    let is_empty_array_binding = self.arena.get(param.name).is_some_and(|n| {
                        n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            && self
                                .arena
                                .get_binding_pattern(n)
                                .is_none_or(|bp| bp.elements.nodes.is_empty())
                    });
                    if is_empty_object_binding {
                        self.write(": {}");
                    } else if is_empty_array_binding {
                        // Empty array binding pattern `[]` desugars to an
                        // iterator-protocol consumption; tsc widens to
                        // `Iterable<any, void, undefined>` (the 3-arg shape
                        // matches the lib `Iterable<T, TReturn, TNext>`).
                        // Matches emptyArrayBindingPatternParameter02.
                        self.write(": Iterable<any, void, undefined>");
                    } else if let Some(synth) = self.synthesize_destructured_param_type(param.name)
                    {
                        // Untyped destructured parameter: tsc synthesizes a tuple
                        // matching the binding-pattern shape rather than collapsing
                        // to `any`. `function bar([x, z, ...w]) {}` →
                        // `bar([x, z, ...w]: [any, any, ...any[]])`.
                        self.write(": ");
                        self.write(&synth);
                    } else {
                        // In declaration emit from source, parameters without
                        // explicit type annotations default to `any` (matching tsc)
                        self.write(": any");
                    }
                }

                // When strictNullChecks is true and a parameter has an
                // initializer before the last required parameter, tsc appends
                // `| undefined` — but only when the type doesn't already
                // include undefined (to avoid `T | undefined | undefined`).
                if self.strict_null_checks
                    && has_initializer_before_required
                    && !self.type_annotation_semantically_includes_undefined(param.type_annotation)
                {
                    let output = self.writer.get_output();
                    if !output.ends_with("| undefined") {
                        self.write(" | undefined");
                    }
                }
                previous_param_end = self.parameter_semantic_end(param_node.end, param);
            }
        }

        if self.should_emit_js_arguments_rest_param(params, body_idx) {
            if !first {
                self.write(", ");
            }
            self.write("...args: any[]");
        }
    }

    /// Synthesize a tuple-shaped declaration-emit type for an untyped
    /// destructured parameter. tsc's declaration emitter walks the binding
    /// pattern shape and emits a corresponding tuple/object literal type
    /// (e.g. `[x, z, ...w]` → `[any, any, ...any[]]`,
    /// `[x, [y]]` → `[any, [any]]`, `{ x }` → `{ x: any }`).
    ///
    /// Returns `None` for empty patterns and patterns with computed,
    /// rename-aliased, or initialized properties — those are handled by
    /// other branches in `emit_parameters_with_body` or fall through to
    /// the default `any` behavior.
    fn parameter_type_for_emit(
        &self,
        param_idx: NodeIndex,
        param_name: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let name_node = self.arena.get(param_name)?;
        let is_binding_pattern = name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            || name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN;

        if is_binding_pattern {
            if let Some(type_id) = self.parameter_type_from_enclosing_signature(param_idx)
                && !type_id.is_any_unknown_or_error()
            {
                return Some(type_id);
            }

            // Binding-pattern parameters often have a widened local node_type on the
            // pattern itself, while the declaration-grade parameter type may still
            // be cached on the parameter/name symbol.
            return self
                .get_symbol_cached_type(param_idx)
                .or_else(|| self.get_symbol_cached_type(param_name))
                .filter(|type_id| !type_id.is_any_unknown_or_error());
        }

        self.get_node_type_or_names(&[param_idx, param_name])
    }

    fn parameter_type_from_enclosing_signature(
        &self,
        param_idx: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let interner = self.type_interner?;
        let ext = self.arena.get_extended(param_idx)?;
        let parent_idx = ext.parent;
        let parent_node = self.arena.get(parent_idx)?;
        let func = self.arena.get_function(parent_node)?;
        let param_position = func
            .parameters
            .nodes
            .iter()
            .position(|&candidate| candidate == param_idx)?;
        let func_type = self
            .get_node_type_or_names(&[parent_idx, func.name])
            .or_else(|| self.get_type_via_symbol_for_func(parent_idx, func.name))?;
        let callable = tsz_solver::type_queries::get_callable_shape_for_type(interner, func_type)?;
        callable
            .call_signatures
            .first()?
            .params
            .get(param_position)
            .map(|param| param.type_id)
    }

    fn binding_pattern_parameter_type_text(
        &self,
        param_idx: NodeIndex,
        param_name: NodeIndex,
        param_initializer: NodeIndex,
    ) -> Option<String> {
        let name_node = self.arena.get(param_name)?;
        let is_binding_pattern = name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            || name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN;
        if !is_binding_pattern {
            return None;
        }

        if param_initializer.is_some()
            && (self.binding_pattern_initializer_is_unannotated_var(param_initializer)
                || (self
                    .get_node_type_or_names(&[param_initializer])
                    .is_none_or(|type_id| type_id.is_any_unknown_or_error())
                    && self
                        .allowlisted_initializer_type_text(param_initializer)
                        .is_none()))
        {
            return Some("any".to_string());
        }

        let source_type = self
            .parameter_type_from_enclosing_signature(param_idx)
            .or_else(|| {
                self.get_symbol_cached_type(param_idx)
                    .or_else(|| self.get_symbol_cached_type(param_name))
                    .filter(|type_id| !type_id.is_any_unknown_or_error())
            });

        self.binding_pattern_type_text(param_name, source_type, param_initializer)
            .or_else(|| self.synthesize_destructured_param_type(param_name))
    }

    fn binding_pattern_initializer_is_unannotated_var(&self, initializer: NodeIndex) -> bool {
        let initializer = self.skip_parenthesized_non_null_and_comma(initializer);
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(sym_id) = self.value_reference_symbol(initializer) else {
            return false;
        };
        binder
            .symbols
            .get(sym_id)
            .and_then(|symbol| symbol.declarations.first().copied())
            .and_then(|decl_idx| self.arena.get(decl_idx))
            .and_then(|decl_node| self.arena.get_variable_declaration(decl_node))
            .is_some_and(|decl| decl.type_annotation.is_none() && decl.initializer.is_none())
    }

    fn binding_pattern_type_text(
        &self,
        pattern_idx: NodeIndex,
        source_type: Option<tsz_solver::types::TypeId>,
        initializer: NodeIndex,
    ) -> Option<String> {
        let node = self.arena.get(pattern_idx)?;
        if node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            let pat = self.arena.get_binding_pattern(node)?;
            if pat.elements.nodes.is_empty() {
                return None;
            }

            let tuple_elements = self.type_interner.and_then(|interner| {
                source_type.and_then(|type_id| {
                    tsz_solver::type_queries::get_tuple_elements(interner, type_id)
                })
            });
            let array_element_type = self.type_interner.and_then(|interner| {
                source_type.and_then(|type_id| {
                    tsz_solver::type_queries::get_array_element_type(interner, type_id).or_else(
                        || {
                            tsz_solver::type_queries::get_tuple_element_type_union(
                                interner, type_id,
                            )
                        },
                    )
                })
            });
            let last_present_index = pat
                .elements
                .nodes
                .iter()
                .rposition(|&elem_idx| {
                    self.arena.get(elem_idx).is_some_and(|elem_node| {
                        elem_node.kind != syntax_kind_ext::OMITTED_EXPRESSION
                    })
                })
                .unwrap_or(0);
            let array_init = self
                .arena
                .get(initializer)
                .filter(|n| n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
                .and_then(|n| self.arena.get_literal_expr(n));
            let literal_len = array_init.map_or(0, |lit| lit.elements.nodes.len());
            let use_literal_slots = array_init.is_some()
                && tuple_elements.as_ref().is_none_or(|elements| {
                    elements.iter().any(|element| element.rest)
                        || elements.len() < pat.elements.nodes.len()
                        || elements.len() < literal_len
                });

            let mut parts = Vec::new();
            let mut source_index = 0usize;
            for (pattern_index, &element_idx) in pat.elements.nodes.iter().enumerate() {
                if element_idx.is_none() {
                    let literal_slot = array_init
                        .and_then(|lit| lit.elements.nodes.get(source_index).copied())
                        .unwrap_or(NodeIndex::NONE);
                    let mut slot_text = tuple_elements
                        .as_deref()
                        .and_then(|elements| elements.get(source_index))
                        .map(|element| self.print_type_id(element.type_id))
                        .or_else(|| {
                            (!use_literal_slots)
                                .then(|| array_element_type.map(|t| self.print_type_id(t)))
                                .flatten()
                        })
                        .or_else(|| self.type_text_from_initializer(literal_slot))
                        .unwrap_or_else(|| "any".to_string());
                    if !use_literal_slots && pattern_index > last_present_index {
                        slot_text.push('?');
                    }
                    parts.push(slot_text);
                    source_index += 1;
                    continue;
                }
                if let Some(element_node) = self.arena.get(element_idx)
                    && element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION
                {
                    let literal_slot = array_init
                        .and_then(|lit| lit.elements.nodes.get(source_index).copied())
                        .unwrap_or(NodeIndex::NONE);
                    let mut slot_text = tuple_elements
                        .as_deref()
                        .and_then(|elements| elements.get(source_index))
                        .map(|element| self.print_type_id(element.type_id))
                        .or_else(|| {
                            (!use_literal_slots)
                                .then(|| array_element_type.map(|t| self.print_type_id(t)))
                                .flatten()
                        })
                        .or_else(|| self.type_text_from_initializer(literal_slot))
                        .unwrap_or_else(|| "any".to_string());
                    if !use_literal_slots && pattern_index > last_present_index {
                        slot_text.push('?');
                    }
                    parts.push(slot_text);
                    source_index += 1;
                    continue;
                }

                let Some(element_node) = self.arena.get(element_idx) else {
                    parts.push("any".to_string());
                    source_index += 1;
                    continue;
                };
                let Some(element) = self.arena.get_binding_element(element_node) else {
                    parts.push("any".to_string());
                    source_index += 1;
                    continue;
                };

                if element.dot_dot_dot_token {
                    if use_literal_slots && let Some(lit) = array_init {
                        for &literal_idx in lit.elements.nodes.iter().skip(source_index) {
                            parts.push(
                                self.type_text_from_initializer(literal_idx)
                                    .unwrap_or_else(|| "any".to_string()),
                            );
                        }
                        source_index = lit.elements.nodes.len();
                    } else if let Some(elements) = tuple_elements.as_deref() {
                        for tuple_element in elements.iter().skip(source_index) {
                            if tuple_element.rest {
                                let rest_type = self
                                    .type_interner
                                    .and_then(|interner| {
                                        tsz_solver::type_queries::get_array_element_type(
                                            interner,
                                            tuple_element.type_id,
                                        )
                                        .or(Some(tuple_element.type_id))
                                    })
                                    .map(|type_id| self.print_type_id(type_id))
                                    .unwrap_or_else(|| "any".to_string());
                                parts.push(format!("...{rest_type}[]"));
                            } else {
                                let mut slot_text = self.print_type_id(tuple_element.type_id);
                                if tuple_element.optional {
                                    slot_text.push('?');
                                }
                                parts.push(slot_text);
                            }
                        }
                    } else {
                        let rest_type = array_element_type
                            .map(|type_id| self.print_type_id(type_id))
                            .unwrap_or_else(|| "any".to_string());
                        parts.push(format!("...{rest_type}[]"));
                    }
                    break;
                }

                let slot_source_type = self.array_binding_element_type(
                    tuple_elements.as_deref(),
                    source_index,
                    array_element_type,
                );
                let literal_slot = array_init
                    .and_then(|lit| lit.elements.nodes.get(source_index).copied())
                    .unwrap_or(NodeIndex::NONE);
                let slot_initializer = if element.initializer.is_some() {
                    element.initializer
                } else {
                    literal_slot
                };
                let slot_node = self.arena.get(element.name)?;
                let mut slot_text = if slot_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                    || slot_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                {
                    self.binding_pattern_type_text(element.name, slot_source_type, slot_initializer)
                        .or_else(|| slot_source_type.map(|type_id| self.print_type_id(type_id)))
                        .or_else(|| self.type_text_from_initializer(slot_initializer))
                        .unwrap_or_else(|| "any".to_string())
                } else {
                    slot_source_type
                        .map(|type_id| self.print_type_id(type_id))
                        .or_else(|| {
                            self.get_symbol_cached_type(element.name)
                                .map(|type_id| self.print_type_id(type_id))
                        })
                        .or_else(|| self.type_text_from_initializer(slot_initializer))
                        .unwrap_or_else(|| "any".to_string())
                };
                if tuple_elements
                    .as_deref()
                    .and_then(|elements| elements.get(source_index))
                    .is_some_and(|element| element.optional)
                {
                    slot_text.push('?');
                }
                parts.push(slot_text);
                source_index += 1;
            }

            if use_literal_slots && let Some(lit) = array_init {
                for &literal_idx in lit.elements.nodes.iter().skip(source_index) {
                    parts.push(
                        self.type_text_from_initializer(literal_idx)
                            .unwrap_or_else(|| "any".to_string()),
                    );
                }
            }

            return Some(format!("[{}]", parts.join(", ")));
        }

        if node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return None;
        }
        let pat = self.arena.get_binding_pattern(node)?;
        if pat.elements.nodes.is_empty() {
            return None;
        }

        let mut members = Vec::new();
        for &elem_idx in &pat.elements.nodes {
            if elem_idx.is_none() {
                return None;
            }
            let elem_node = self.arena.get(elem_idx)?;
            let elem = self.arena.get_binding_element(elem_node)?;
            if elem.dot_dot_dot_token {
                let rest_name = self
                    .arena
                    .get(elem.name)
                    .and_then(|n| self.arena.get_identifier(n))
                    .map(|id| id.escaped_text.as_str())
                    .unwrap_or("rest");
                members.push(format!("...{rest_name}: any;"));
                continue;
            }
            let prop_name_idx = if elem.property_name.is_some() {
                elem.property_name
            } else {
                elem.name
            };
            let mut member_text = self.binding_pattern_member_name_text(prop_name_idx)?;
            if elem.initializer.is_some() {
                member_text.push_str("?: ");
            } else {
                member_text.push_str(": ");
            }
            let prop_source_type = self.object_binding_element_type(source_type, elem);
            let prop_initializer = elem.initializer;
            let value_node = self.arena.get(elem.name)?;
            let value_type = if value_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                || value_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            {
                self.binding_pattern_type_text(elem.name, prop_source_type, prop_initializer)
                    .or_else(|| prop_source_type.map(|type_id| self.print_type_id(type_id)))
                    .or_else(|| self.type_text_from_initializer(prop_initializer))
                    .unwrap_or_else(|| String::from("any"))
            } else {
                prop_source_type
                    .map(|type_id| self.print_type_id(type_id))
                    .or_else(|| {
                        self.get_symbol_cached_type(elem.name)
                            .map(|type_id| self.print_type_id(type_id))
                    })
                    .or_else(|| self.type_text_from_initializer(prop_initializer))
                    .unwrap_or_else(|| String::from("any"))
            };
            member_text.push_str(&value_type);
            member_text.push(';');
            members.push(member_text);
        }
        let member_indent = "    ".repeat((self.indent_level + 1) as usize);
        let closing_indent = "    ".repeat(self.indent_level as usize);
        let lines: Vec<String> = members
            .into_iter()
            .map(|member| format!("{member_indent}{member}"))
            .collect();
        Some(format!("{{\n{}\n{closing_indent}}}", lines.join("\n")))
    }

    fn binding_pattern_member_name_text(&self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            return self
                .arena
                .get_identifier(name_node)
                .map(|ident| ident.escaped_text.to_string());
        }

        let property_name = self.destructuring_property_lookup_text(name_idx)?;
        if Self::is_simple_identifier_text(&property_name) || property_name.parse::<f64>().is_ok() {
            Some(property_name)
        } else {
            Some(format!(
                "\"{}\"",
                property_name.replace('\\', "\\\\").replace('"', "\\\"")
            ))
        }
    }

    fn type_text_from_initializer(&self, initializer: NodeIndex) -> Option<String> {
        initializer
            .is_some()
            .then(|| self.allowlisted_initializer_type_text(initializer))
            .flatten()
    }

    fn synthesize_destructured_param_type(&self, pattern_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(pattern_idx)?;
        if node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            let pat = self.arena.get_binding_pattern(node)?;
            if pat.elements.nodes.is_empty() {
                return None;
            }
            let last_present_index = pat
                .elements
                .nodes
                .iter()
                .rposition(|&elem_idx| {
                    self.arena.get(elem_idx).is_some_and(|elem_node| {
                        elem_node.kind != syntax_kind_ext::OMITTED_EXPRESSION
                    })
                })
                .unwrap_or(0);
            let mut out = String::from("[");
            let mut first = true;
            for (index, &elem_idx) in pat.elements.nodes.iter().enumerate() {
                if !first {
                    out.push_str(", ");
                }
                first = false;
                if elem_idx.is_none() {
                    out.push_str("any");
                    if index > last_present_index {
                        out.push('?');
                    }
                    continue;
                }
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    out.push_str("any");
                    if index > last_present_index {
                        out.push('?');
                    }
                    continue;
                };
                if elem_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                    out.push_str("any");
                    if index > last_present_index {
                        out.push('?');
                    }
                    continue;
                }
                let Some(elem) = self.arena.get_binding_element(elem_node) else {
                    out.push_str("any");
                    continue;
                };
                if elem.dot_dot_dot_token {
                    out.push_str("...any[]");
                    continue;
                }
                let inner = self
                    .synthesize_destructured_param_type(elem.name)
                    .or_else(|| {
                        elem.initializer
                            .is_some()
                            .then(|| self.allowlisted_initializer_type_text(elem.initializer))
                            .flatten()
                    })
                    .unwrap_or_else(|| String::from("any"));
                out.push_str(&inner);
            }
            out.push(']');
            Some(out)
        } else if node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            let pat = self.arena.get_binding_pattern(node)?;
            if pat.elements.nodes.is_empty() {
                return None;
            }
            let mut out = String::from("{ ");
            for (i, &elem_idx) in pat.elements.nodes.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                if elem_idx.is_none() {
                    return None;
                }
                let elem_node = self.arena.get(elem_idx)?;
                let elem = self.arena.get_binding_element(elem_node)?;
                if elem.dot_dot_dot_token {
                    // Rest in object pattern is `...<name>: any` in tsc; preserve
                    // the source binding name so the synthesized shape matches
                    // the user's destructuring (e.g. `{ ...remaining }`).
                    let rest_name = self
                        .arena
                        .get(elem.name)
                        .and_then(|n| self.arena.get_identifier(n))
                        .map(|id| id.escaped_text.as_str())
                        .unwrap_or("rest");
                    out.push_str(&format!("...{rest_name}: any;"));
                    continue;
                }
                // Property name: prefer the explicit `property_name` (`{ a: b }`),
                // fall back to the binding name when shorthand (`{ a }`).
                let prop_name_idx = if elem.property_name.is_some() {
                    elem.property_name
                } else {
                    elem.name
                };
                let name_node = self.arena.get(prop_name_idx)?;
                if name_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
                    // Computed property names, string/numeric literal keys, etc.
                    // are not supported by this synthesizer.
                    return None;
                }
                let ident = self.arena.get_identifier(name_node)?;
                out.push_str(&ident.escaped_text);
                if elem.initializer.is_some() {
                    out.push_str("?: ");
                } else {
                    out.push_str(": ");
                }
                let value_type = self
                    .synthesize_destructured_param_type(elem.name)
                    .or_else(|| {
                        elem.initializer
                            .is_some()
                            .then(|| self.allowlisted_initializer_type_text(elem.initializer))
                            .flatten()
                    })
                    .unwrap_or_else(|| String::from("any"));
                out.push_str(&value_type);
                out.push(';');
            }
            out.push_str(" }");
            Some(out)
        } else {
            None
        }
    }

    pub(crate) fn parameter_has_property_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&mod_idx| {
                self.arena.get(mod_idx).is_some_and(|mod_node| {
                    let kind = mod_node.kind;
                    kind == SyntaxKind::PublicKeyword as u16
                        || kind == SyntaxKind::PrivateKeyword as u16
                        || kind == SyntaxKind::ProtectedKeyword as u16
                        || kind == SyntaxKind::ReadonlyKeyword as u16
                        || kind == SyntaxKind::OverrideKeyword as u16
                })
            })
        })
    }

    /// Emit parameters without type annotations (used for private accessors)
    pub(crate) fn emit_parameters_without_types(&mut self, params: &NodeList, omit_types: bool) {
        if !omit_types {
            self.emit_parameters(params);
            return;
        }

        let mut first = true;
        for &param_idx in &params.nodes {
            if !first {
                self.write(", ");
            }
            first = false;

            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                // Rest parameter
                if param.dot_dot_dot_token {
                    self.write("...");
                }

                // Name only (no type)
                self.emit_node(param.name);

                // Optional marker still included
                if param.question_token {
                    self.write("?");
                }
            }
        }
    }

    fn should_emit_js_arguments_rest_param(&self, params: &NodeList, body_idx: NodeIndex) -> bool {
        if !self.source_is_js_file || body_idx.is_none() {
            return false;
        }

        let has_rest_param = params.nodes.iter().any(|&param_idx| {
            self.arena
                .get(param_idx)
                .and_then(|param_node| self.arena.get_parameter(param_node))
                .is_some_and(|param| param.dot_dot_dot_token)
        });
        if has_rest_param {
            return false;
        }

        tsz_parser::syntax::transform_utils::contains_arguments_reference(self.arena, body_idx)
    }

    pub(crate) fn emit_type_parameters(&mut self, type_params: &NodeList) {
        self.write("<");
        let mut first = true;
        for &param_idx in &type_params.nodes {
            if !first {
                self.write(", ");
            }
            first = false;

            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_type_parameter(param_node)
            {
                // Inline JSDoc comment before type parameter
                self.emit_inline_parameter_comment(param_node.pos);

                // Emit type-parameter modifiers as parsed. `public` is not a
                // valid variance modifier, but tsc preserves it in declaration
                // emit when recovering from invalid type-parameter syntax.
                if let Some(ref mods) = param.modifiers {
                    for &mod_idx in &mods.nodes {
                        if let Some(mod_node) = self.arena.get(mod_idx) {
                            match mod_node.kind {
                                k if k == SyntaxKind::InKeyword as u16 => self.write("in "),
                                k if k == SyntaxKind::OutKeyword as u16 => self.write("out "),
                                k if k == SyntaxKind::ConstKeyword as u16 => self.write("const "),
                                k if k == SyntaxKind::PublicKeyword as u16 => {
                                    self.write("public ");
                                }
                                _ => {}
                            }
                        }
                    }
                }

                // When the parser recovered from a missing identifier (the
                // user's "name" token was a reserved word and unusable, e.g.
                // `<in in>`), the synthesized name has an empty atom and
                // zero-width source range.  tsc renders this case as a
                // phantom comma after the modifier (`<in , >`), reflecting
                // its own recovery shape (one modifier-only param + one
                // empty trailing slot).  We mirror that rendering only when
                // (a) at least one modifier was emitted, and (b) the name
                // is genuinely synthesized — never for user-chosen names.
                let name_is_synthesized = self
                    .arena
                    .get(param.name)
                    .and_then(|n| self.arena.get_identifier(n))
                    .is_some_and(|id| id.escaped_text.is_empty());
                let has_modifier = param
                    .modifiers
                    .as_ref()
                    .is_some_and(|m| !m.nodes.is_empty());
                if name_is_synthesized && has_modifier {
                    self.write(", ");
                } else {
                    self.emit_node(param.name);
                }

                if param.constraint.is_some() {
                    self.write(" extends ");
                    self.emit_type(param.constraint);
                }

                if param.default.is_some() {
                    self.write(" = ");
                    self.emit_type(param.default);
                }
            }
        }
        self.write(">");
    }

    pub(crate) fn emit_heritage_clauses(&mut self, clauses: &NodeList) {
        self.emit_heritage_clauses_inner(clauses, false, None, None);
    }

    pub(crate) fn emit_class_heritage_clauses(
        &mut self,
        clauses: &NodeList,
        extends_alias: Option<&str>,
        jsdoc_extends_type: Option<&str>,
    ) {
        self.emit_heritage_clauses_inner(clauses, false, extends_alias, jsdoc_extends_type);
    }

    pub(crate) fn emit_interface_heritage_clauses(&mut self, clauses: &NodeList) {
        self.emit_heritage_clauses_inner(clauses, true, None, None);
    }

    fn emit_heritage_clauses_inner(
        &mut self,
        clauses: &NodeList,
        is_interface: bool,
        extends_alias: Option<&str>,
        jsdoc_extends_type: Option<&str>,
    ) {
        for &clause_idx in &clauses.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            let keyword = match heritage.token {
                k if k == SyntaxKind::ExtendsKeyword as u16 => "extends",
                k if k == SyntaxKind::ImplementsKeyword as u16 => "implements",
                _ => continue,
            };

            // For interfaces, filter out heritage types with non-entity-name
            // expressions (e.g. `typeof X`, parenthesized expressions).
            // tsc strips these in declaration emit.
            let valid_types: Vec<_> = heritage
                .types
                .nodes
                .iter()
                .copied()
                .filter(|&type_idx| !is_interface || self.is_entity_name_heritage(type_idx))
                .filter(|&type_idx| {
                    !(self.source_is_js_file
                        && heritage.token == SyntaxKind::ExtendsKeyword as u16
                        && self.heritage_type_is_null(type_idx))
                })
                .collect();

            if valid_types.is_empty() {
                continue;
            }

            self.write(" ");
            self.write(keyword);
            self.write(" ");

            if heritage.token == SyntaxKind::ExtendsKeyword as u16
                && let Some(alias_name) = extends_alias
            {
                self.write(alias_name);
                if let Some(&type_idx) = valid_types.first()
                    && let Some(type_node) = self.arena.get(type_idx)
                    && let Some(expr) = self.arena.get_expr_type_args(type_node)
                    && let Some(ref type_args) = expr.type_arguments
                    && !type_args.nodes.is_empty()
                {
                    self.emit_type_arguments(type_args);
                }
                continue;
            }

            if heritage.token == SyntaxKind::ExtendsKeyword as u16
                && let Some(type_text) = jsdoc_extends_type
            {
                self.write(type_text);
                continue;
            }

            let mut first = true;
            for &type_idx in &valid_types {
                if !first {
                    self.write(", ");
                }
                first = false;
                if heritage.token == SyntaxKind::ExtendsKeyword as u16
                    && self.source_is_js_file
                    && self.heritage_type_is_bare_array(type_idx)
                {
                    self.write("Array<any>");
                    continue;
                }
                self.emit_type(type_idx);
            }
        }
    }

    pub(in crate::declaration_emitter) fn heritage_clauses_extend_bare_array(
        &self,
        clauses: Option<&NodeList>,
    ) -> bool {
        let Some(clauses) = clauses else {
            return false;
        };
        clauses.nodes.iter().copied().any(|clause_idx| {
            let Some(heritage) = self.arena.get_heritage_clause_at(clause_idx) else {
                return false;
            };
            heritage.token == SyntaxKind::ExtendsKeyword as u16
                && heritage
                    .types
                    .nodes
                    .iter()
                    .copied()
                    .any(|type_idx| self.heritage_type_is_bare_array(type_idx))
        })
    }

    pub(in crate::declaration_emitter) fn heritage_type_is_null(
        &self,
        type_idx: NodeIndex,
    ) -> bool {
        self.arena
            .get(type_idx)
            .and_then(|node| self.get_source_slice_no_semi(node.pos, node.end))
            .is_some_and(|text| text.trim() == "null")
    }

    fn heritage_type_is_bare_array(&self, type_idx: NodeIndex) -> bool {
        let Some(type_node) = self.arena.get(type_idx) else {
            return false;
        };
        if self.get_identifier_text(type_idx).as_deref() == Some("Array")
            && let Some(identifier) = self.arena.get_identifier(type_node)
        {
            return identifier
                .type_arguments
                .as_ref()
                .is_none_or(|args| args.nodes.is_empty());
        }

        let Some(expr) = self.arena.get_expr_type_args(type_node) else {
            return false;
        };
        if expr
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return false;
        }
        self.get_identifier_text(expr.expression).as_deref() == Some("Array")
    }

    /// Check if a heritage type expression is an entity name (identifier or
    /// property access chain). Non-entity-name expressions like `typeof X` or
    /// parenthesized expressions are invalid in interface `extends` clauses
    /// and should be stripped in .d.ts output.
    pub(crate) fn is_entity_name_heritage(&self, type_idx: NodeIndex) -> bool {
        let Some(type_node) = self.arena.get(type_idx) else {
            return false;
        };
        // Heritage types may be wrapped in ExpressionWithTypeArguments (when
        // type args are present, e.g. `extends Foo<T>`), or may be bare
        // identifiers / property access chains (e.g. `extends A, B`).
        if let Some(eta) = self.arena.get_expr_type_args(type_node) {
            self.is_entity_name_expr(eta.expression)
        } else {
            self.is_entity_name_expr(type_idx)
        }
    }

    fn is_entity_name_expr(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind == SyntaxKind::Identifier as u16
            || expr_node.kind == SyntaxKind::NullKeyword as u16
        {
            return true;
        }
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(expr_node)
        {
            return self.is_entity_name_expr(access.expression);
        }
        false
    }

    /// Pre-scan a module body to determine if it has a "scope marker" —
    /// either an explicit `export {}` statement or a mix of exported and
    /// non-exported members. When true, `export` keywords should be preserved
    /// on individual members inside the ambient module.
    ///
    /// When `non_ambient` is true (non-ambient namespaces), only namespace
    /// declarations count as visible non-exported members. Other non-exported
    /// declarations (classes, interfaces, variables, etc.) are not emitted
    /// in the .d.ts output and should not trigger the scope marker.
    pub(crate) fn module_body_has_scope_marker(
        &self,
        stmts: &tsz_parser::parser::NodeList,
        non_ambient: bool,
    ) -> bool {
        let mut has_exported = false;
        let mut has_non_exported = false;

        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            match stmt_node.kind {
                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                    if let Some(export) = self.arena.get_export_decl(stmt_node) {
                        // `export {}` — explicit scope marker
                        if let Some(clause_node) = self.arena.get(export.export_clause)
                            && clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
                            && let Some(named) = self.arena.get_named_imports(clause_node)
                            && named.elements.nodes.is_empty()
                        {
                            return true;
                        }
                        // `export *` or `export * from "mod"` — scope marker
                        // (export_clause is None for bare `export *`)
                        if !export.export_clause.is_some()
                            || self
                                .arena
                                .get(export.export_clause)
                                .is_some_and(|n| n.kind == syntax_kind_ext::NAMESPACE_EXPORT)
                        {
                            return true;
                        }
                        // Check if export_clause wraps a declaration (e.g., `export class Foo`)
                        // — these count as exported members, not scope markers
                        if let Some(clause_node) = self.arena.get(export.export_clause) {
                            let ck = clause_node.kind;
                            if ck == syntax_kind_ext::CLASS_DECLARATION
                                || ck == syntax_kind_ext::FUNCTION_DECLARATION
                                || ck == syntax_kind_ext::INTERFACE_DECLARATION
                                || ck == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                || ck == syntax_kind_ext::ENUM_DECLARATION
                                || ck == syntax_kind_ext::VARIABLE_STATEMENT
                                || ck == syntax_kind_ext::MODULE_DECLARATION
                                || ck == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                            {
                                has_exported = true;
                                if non_ambient
                                    && ck == syntax_kind_ext::CLASS_DECLARATION
                                    && let Some(class) = self.arena.get_class(clause_node)
                                    && class
                                        .heritage_clauses
                                        .as_ref()
                                        .and_then(|heritage| {
                                            self.non_nameable_extends_heritage_type(heritage)
                                        })
                                        .is_some()
                                {
                                    has_non_exported = true;
                                }
                            } else {
                                // Named exports like `export { a, b }` — scope marker
                                return true;
                            }
                        }
                    }
                }
                k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    // `export = value` or `export default` — scope marker
                    return true;
                }
                _ => {
                    if self.stmt_has_export_modifier(stmt_node) {
                        has_exported = true;
                        if non_ambient
                            && stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION
                            && let Some(class) = self.arena.get_class(stmt_node)
                            && class
                                .heritage_clauses
                                .as_ref()
                                .and_then(|heritage| {
                                    self.non_nameable_extends_heritage_type(heritage)
                                })
                                .is_some()
                        {
                            has_non_exported = true;
                        }
                    } else {
                        // Skip ImportDeclaration and ImportEqualsDeclaration
                        // as they don't count as non-exported members
                        if stmt_node.kind == syntax_kind_ext::IMPORT_DECLARATION
                            || stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        {
                            continue;
                        }
                        // In non-ambient namespaces, non-exported declarations
                        // are only emitted in .d.ts if they are referenced by
                        // exported members (via used_symbols). Namespace
                        // declarations are only emitted when they are
                        // referenced by the exported API surface or by an
                        // exported import alias. Matching that elision here
                        // prevents hidden namespaces from triggering a false
                        // mixed-export scope-marker.
                        if non_ambient {
                            let counts_as_non_exported = if stmt_node.kind
                                == syntax_kind_ext::MODULE_DECLARATION
                            {
                                self.arena.get_module(stmt_node).is_none_or(|m| {
                                    !self.is_module_body_effectively_empty(m.body)
                                        && (self.is_ns_member_used_by_exports(stmt_idx)
                                            || self
                                                .is_empty_namespace_referenced_by_export_import_alias(
                                                    stmt_idx,
                                                ))
                                })
                            } else {
                                self.is_ns_member_used_by_exports(stmt_idx)
                            };
                            if counts_as_non_exported {
                                has_non_exported = true;
                            }
                        } else {
                            has_non_exported = true;
                        }
                    }
                }
            }

            // For non-ambient namespaces only: a mix of exported and
            // non-exported members means a scope marker is needed so the
            // DTS consumer can tell the two apart.  For ambient contexts
            // (declare module / declare namespace) the only trigger is an
            // *explicit* export statement (`export {}`, `export *`, etc.)
            // which is already handled by the early-return paths above.
            if non_ambient && has_exported && has_non_exported {
                return true;
            }
        }

        false
    }

    pub(in crate::declaration_emitter) fn module_body_has_exported_member(
        &self,
        stmts: &tsz_parser::parser::NodeList,
    ) -> bool {
        stmts.nodes.iter().copied().any(|stmt_idx| {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                return false;
            };
            if self.stmt_has_export_modifier(stmt_node) {
                return true;
            }
            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                || stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                || stmt_node.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
            {
                return true;
            }
            false
        })
    }

    pub(crate) fn emit_member_modifiers(&mut self, modifiers: &Option<NodeList>) {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    match mod_node.kind {
                        // In constructor parameters, strip accessibility and readonly modifiers
                        k if k == SyntaxKind::PublicKeyword as u16 => {
                            // In .d.ts files, `public` is the default and is omitted by tsc.
                            // Only emit it for constructor parameter properties
                            // (which is handled separately and already skips it).
                        }
                        k if k == SyntaxKind::PrivateKeyword as u16
                            && !self.in_constructor_params =>
                        {
                            self.write("private ");
                        }
                        k if k == SyntaxKind::ProtectedKeyword as u16
                            && !self.in_constructor_params =>
                        {
                            self.write("protected ");
                        }
                        k if k == SyntaxKind::ReadonlyKeyword as u16
                            && !self.in_constructor_params =>
                        {
                            self.write("readonly ");
                        }
                        k if k == SyntaxKind::StaticKeyword as u16 => self.write("static "),
                        k if k == SyntaxKind::AbstractKeyword as u16 => self.write("abstract "),
                        k if k == SyntaxKind::OverrideKeyword as u16 => {
                            // tsc strips `override` in .d.ts output — it is not
                            // part of the declaration surface.
                        }
                        k if k == SyntaxKind::AsyncKeyword as u16 => {
                            // tsc strips `async` in .d.ts — the return type already
                            // encodes Promise<T>, so the modifier is redundant.
                        }
                        k if k == SyntaxKind::AccessorKeyword as u16 => self.write("accessor "),
                        k if k == SyntaxKind::InKeyword as u16 => self.write("in "),
                        k if k == SyntaxKind::OutKeyword as u16 => self.write("out "),
                        k if k == SyntaxKind::DeclareKeyword as u16 => {
                            // tsc strips `declare` from class members in .d.ts — it is
                            // only meaningful at the top-level statement level
                            // (`declare class`, `declare function`, etc.).
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
