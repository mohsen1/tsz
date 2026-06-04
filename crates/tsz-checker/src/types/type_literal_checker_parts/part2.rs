impl<'a> CheckerState<'a> {
    // =========================================================================
    // Type Node Resolution in Type Literals
    // =========================================================================

    /// Get type from a type literal node (anonymous object types).
    ///
    /// Type literals represent inline object types like `{ x: string; y: number }` or
    /// callable types with call/construct signatures. This function parses the type
    /// literal and creates the appropriate type representation.
    ///
    /// ## Type Literal Members:
    /// - **Property Signatures**: Named properties with types (`{ x: string }`)
    /// - **Method Signatures**: Function-typed methods (`{ method(): void }`)
    /// - **Call Signatures**: Callable objects (`{ (): string }`)
    /// - **Construct Signatures**: Constructor functions (`{ new(): T }`)
    /// - **Index Signatures**: Dynamic property access (`{ [key: string]: T }`)
    ///
    /// ## Modifiers:
    /// - `?`: Optional property (can be undefined)
    /// - `readonly`: Read-only property (cannot be assigned to)
    ///
    /// ## Type Resolution:
    /// - Property types are resolved via `get_type_from_type_node_in_type_literal`
    /// - Type parameters are pushed/popped for each member
    /// - Index signatures are tracked by key type (string or number)
    ///
    /// ## Result Type:
    /// - **Callable**: If has call/construct signatures
    /// - **`ObjectWithIndex`**: If has index signatures
    /// - **Object**: Plain object type otherwise
    pub(crate) fn get_type_from_type_literal(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext::{
            CALL_SIGNATURE, CONSTRUCT_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE,
        };
        use tsz_solver::{
            CallSignature, CallableShape, FunctionShape, IndexSignature, ObjectShape, PropertyInfo,
        };
        let factory = self.ctx.types.factory();

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(data) = self.ctx.arena.get_type_literal(node) else {
            return TypeId::ERROR; // Missing type literal data - propagate error
        };
        let owner_name = self.enclosing_type_literal_owner_name(idx);

        struct AccessorMemberInfo {
            name_idx: NodeIndex,
            type_annotation: NodeIndex,
            resolved_type: TypeId,
            circular_self_reference: bool,
        }

        struct AccessorAggregate {
            getter: Option<AccessorMemberInfo>,
            setter: Option<AccessorMemberInfo>,
            declaration_order: u32,
        }

        let mut properties = Vec::new();
        let mut accessors: FxHashMap<Atom, AccessorAggregate> = FxHashMap::default();
        let mut call_signatures = Vec::new();
        let mut construct_signatures = Vec::new();
        let mut string_index = None;
        let mut number_index = None;
        let mut extra_number_indices = Vec::new();
        let mut has_abstract_construct_sig = false;
        let mut has_late_bound_members = false;
        // Global member counter for preserving source declaration order across
        // both properties and methods. Using properties.len() would give methods
        // higher declaration_order than all properties since methods are merged
        // after the loop, breaking tsc's interleaved display order.
        let mut member_order: u32 = 0;
        struct OverloadEntry {
            signature: CallSignature,
            optional: bool,
            readonly: bool,
            is_symbol_named: bool,
        }
        struct OverloadOrderKey {
            name: Atom,
            decl_order: u32,
            is_string_named: bool,
            single_quoted_name: bool,
        }
        let mut method_overloads: FxHashMap<Atom, Vec<OverloadEntry>> = FxHashMap::default();
        let mut method_overload_order: Vec<OverloadOrderKey> = Vec::new();

        for &member_idx in &data.members.nodes {
            let Some(member) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if let Some(sig) = self.ctx.arena.get_signature(member) {
                match member.kind {
                    CALL_SIGNATURE => {
                        if let Some(ref _params) = sig.parameters {}
                        let (type_params, type_param_updates) =
                            self.push_type_parameters(&sig.type_parameters);
                        // Check for unused type parameters (TS6133)
                        self.check_unused_type_params(&sig.type_parameters, member_idx);
                        let (params, this_type) =
                            self.extract_params_from_signature_in_type_literal(sig);
                        let (return_type, type_predicate) = self
                            .return_type_and_predicate_in_type_literal(
                                sig.type_annotation,
                                &params,
                            );
                        call_signatures.push(CallSignature {
                            type_params,
                            params,
                            this_type,
                            return_type,
                            type_predicate,
                            is_method: false,
                        });
                        self.pop_type_parameters(type_param_updates);
                    }
                    CONSTRUCT_SIGNATURE => {
                        if let Some(ref _params) = sig.parameters {}
                        if self.has_abstract_modifier(&sig.modifiers) {
                            has_abstract_construct_sig = true;
                        }
                        let (type_params, type_param_updates) =
                            self.push_type_parameters(&sig.type_parameters);
                        // Check for unused type parameters (TS6133)
                        self.check_unused_type_params(&sig.type_parameters, member_idx);
                        let (params, this_type) =
                            self.extract_params_from_signature_in_type_literal(sig);
                        let (return_type, type_predicate) = self
                            .return_type_and_predicate_in_type_literal(
                                sig.type_annotation,
                                &params,
                            );
                        construct_signatures.push(CallSignature {
                            type_params,
                            params,
                            this_type,
                            return_type,
                            type_predicate,
                            is_method: false,
                        });
                        self.pop_type_parameters(type_param_updates);
                    }
                    METHOD_SIGNATURE | PROPERTY_SIGNATURE => {
                        let Some(name) = self.get_property_name_resolved(sig.name) else {
                            if self
                                .ctx
                                .arena
                                .get(sig.name)
                                .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                            {
                                has_late_bound_members = true;
                            }
                            continue;
                        };
                        let name_atom = self.ctx.types.intern_string(&name);
                        let is_symbol_named = self.is_symbol_property_name(sig.name);
                        let (is_string_named, single_quoted_name) =
                            self.ctx.arena.string_property_name_flags(sig.name);

                        if member.kind == METHOD_SIGNATURE {
                            if let Some(ref _params) = sig.parameters {}
                            let (type_params, type_param_updates) =
                                self.push_type_parameters(&sig.type_parameters);
                            let (params, this_type) =
                                self.extract_params_from_signature_in_type_literal(sig);
                            let (return_type, type_predicate) = self
                                .return_type_and_predicate_in_type_literal(
                                    sig.type_annotation,
                                    &params,
                                );
                            let call_sig = CallSignature {
                                type_params,
                                params,
                                this_type,
                                return_type,
                                type_predicate,
                                is_method: true,
                            };
                            self.pop_type_parameters(type_param_updates);
                            let optional = sig.question_token;
                            let readonly = self.has_readonly_modifier(&sig.modifiers);
                            let entry = method_overloads.entry(name_atom).or_default();
                            if entry.is_empty() {
                                member_order += 1;
                                method_overload_order.push(OverloadOrderKey {
                                    name: name_atom,
                                    decl_order: member_order,
                                    is_string_named,
                                    single_quoted_name,
                                });
                            }
                            entry.push(OverloadEntry {
                                signature: call_sig,
                                optional,
                                readonly,
                                is_symbol_named,
                            });
                        } else {
                            let circular_self_reference = sig.type_annotation.is_some()
                                && owner_name.as_deref().is_some_and(|owner_name| {
                                    self.indexed_access_references_owner_property(
                                        sig.type_annotation,
                                        owner_name,
                                        &name,
                                    )
                                });
                            let type_id = if circular_self_reference {
                                let message = format!(
                                    "'{name}' is referenced directly or indirectly in its own type annotation."
                                );
                                self.error_at_node(sig.name, &message, 2502);
                                TypeId::ANY
                            } else if sig.type_annotation.is_some() {
                                self.get_type_from_type_node_in_type_literal(sig.type_annotation)
                            } else {
                                TypeId::ANY
                            };
                            let write_type =
                                if self.ctx.compiler_options.exact_optional_property_types
                                    && sig.question_token
                                    && sig.type_annotation.is_some()
                                    && !type_node_includes_explicit_undefined(
                                        self.ctx.arena,
                                        sig.type_annotation,
                                    )
                                {
                                    crate::query_boundaries::common::remove_undefined(
                                        self.ctx.types.as_type_database(),
                                        type_id,
                                    )
                                } else {
                                    type_id
                                };
                            member_order += 1;
                            properties.push(PropertyInfo {
                                name: name_atom,
                                type_id,
                                write_type,
                                optional: sig.question_token,
                                readonly: self.has_readonly_modifier(&sig.modifiers),
                                is_method: false,
                                is_class_prototype: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                                declaration_order: member_order,
                                is_string_named,
                                is_symbol_named,
                                single_quoted_name,
                            });
                        }
                    }
                    _ => {}
                }
                continue;
            }

            if let Some(index_sig) = self.ctx.arena.get_index_signature(member) {
                let param_idx = index_sig
                    .parameters
                    .nodes
                    .first()
                    .copied()
                    .unwrap_or(NodeIndex::NONE);
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                let key_type = if param_data.type_annotation.is_some() {
                    self.get_type_from_type_node_in_type_literal(param_data.type_annotation)
                } else {
                    // Missing annotation defaults to ANY (TS7011 reported separately)
                    TypeId::ANY
                };

                // TS1337 / TS1268: Validate index signature parameter type.
                // Suppress when the parameter already has grammar errors (rest/optional) — matches tsc.
                let has_param_grammar_error =
                    param_data.dot_dot_dot_token || param_data.question_token;
                let is_valid_index_type = if !has_param_grammar_error
                    && param_data.type_annotation.is_some()
                {
                    let (is_generic_or_literal, is_valid) =
                        self.classify_index_sig_param_type(key_type, param_data.type_annotation);
                    if !is_valid {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        if is_generic_or_literal {
                            self.error_at_node(
                                param_idx,
                                diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_CANNOT_BE_A_LITERAL_TYPE_OR_GENERIC_TYPE_CONSI,
                                diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_CANNOT_BE_A_LITERAL_TYPE_OR_GENERIC_TYPE_CONSI,
                            );
                        } else {
                            self.error_at_node(
                                param_idx,
                                diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                                diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                            );
                        }
                    }
                    is_valid
                } else {
                    false
                };

                // TS2693: Check if parameter name without type annotation
                // refers to a type (e.g., `[K]: number` where `K` is a type alias).
                if !has_param_grammar_error
                    && param_data.type_annotation.is_none()
                    && let Some(name_node) = self.ctx.arena.get(param_data.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && let Some(sym_id) = self
                        .ctx
                        .binder
                        .resolve_identifier(self.ctx.arena, param_data.name)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                {
                    let name = &ident.escaped_text;
                    // Check if this identifier resolves to a type symbol
                    let has_type = symbol.has_any_flags(
                        tsz_binder::symbol_flags::TYPE
                            | tsz_binder::symbol_flags::TYPE_ALIAS
                            | tsz_binder::symbol_flags::INTERFACE,
                    );
                    let has_value = symbol.has_any_flags(tsz_binder::symbol_flags::VALUE);
                    if has_type && !has_value {
                        // The identifier refers to a type-only symbol
                        // Emit TS2693: Type only used as value
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };
                        let message = format_message(
                            diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                            &[name],
                        );
                        self.ctx.error(
                            name_node.pos,
                            name_node.end - name_node.pos,
                            message,
                            diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                        );
                    }
                }

                let value_type = if index_sig.type_annotation.is_some() {
                    self.get_type_from_type_node_in_type_literal(index_sig.type_annotation)
                } else {
                    // Missing annotation defaults to ANY (TS7011 reported separately)
                    TypeId::ANY
                };
                let readonly = self.has_readonly_modifier(&index_sig.modifiers);
                let param_name = self
                    .ctx
                    .arena
                    .get(param_data.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .map(|name_ident| self.ctx.types.intern_string(&name_ident.escaped_text));
                let info = IndexSignature {
                    key_type,
                    value_type,
                    readonly,
                    param_name,
                };
                if is_valid_index_type {
                    if key_type == TypeId::NUMBER {
                        if number_index.is_none() {
                            number_index = Some(info);
                        } else {
                            extra_number_indices.push(info);
                        }
                    } else {
                        match string_index.as_mut() {
                            None => string_index = Some(info),
                            Some(existing) => {
                                super::interface_type::merge_string_index_by_union(
                                    existing, info, factory,
                                );
                            }
                        }
                    }
                }
                continue;
            }

            // Handle accessor declarations (get/set) in type literals
            if (member.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR
                || member.kind == tsz_parser::parser::syntax_kind_ext::SET_ACCESSOR)
                && let Some(accessor) = self.ctx.arena.get_accessor(member)
                && let Some(name) = self.get_property_name_resolved(accessor.name)
            {
                let name_atom = self.ctx.types.intern_string(&name);
                let is_new_accessor = !accessors.contains_key(&name_atom);
                if is_new_accessor {
                    member_order += 1;
                }
                let current_order = member_order;
                let entry = accessors.entry(name_atom).or_insert(AccessorAggregate {
                    getter: None,
                    setter: None,
                    declaration_order: current_order,
                });

                if member.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR {
                    let circular_self_reference = accessor.type_annotation.is_some()
                        && owner_name.as_deref().is_some_and(|owner_name| {
                            self.type_literal_accessor_circular_reference(
                                accessor.type_annotation,
                                accessor.name,
                                owner_name,
                            )
                        });
                    let resolved_type =
                        if accessor.type_annotation.is_some() && !circular_self_reference {
                            self.get_type_from_type_node_in_type_literal(accessor.type_annotation)
                        } else {
                            TypeId::ANY
                        };
                    entry.getter = Some(AccessorMemberInfo {
                        name_idx: accessor.name,
                        type_annotation: accessor.type_annotation,
                        resolved_type,
                        circular_self_reference,
                    });
                } else {
                    let mut type_annotation = NodeIndex::NONE;
                    let mut circular_self_reference = false;
                    let mut resolved_type = TypeId::UNKNOWN;
                    if let Some(&param_idx) = accessor.parameters.nodes.first()
                        && let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                    {
                        type_annotation = param.type_annotation;
                        circular_self_reference = param.type_annotation.is_some()
                            && owner_name.as_deref().is_some_and(|owner_name| {
                                self.type_literal_accessor_circular_reference(
                                    param.type_annotation,
                                    accessor.name,
                                    owner_name,
                                )
                            });
                        if param.type_annotation.is_some() && !circular_self_reference {
                            resolved_type =
                                self.get_type_from_type_node_in_type_literal(param.type_annotation);
                        }
                    }
                    entry.setter = Some(AccessorMemberInfo {
                        name_idx: accessor.name,
                        type_annotation,
                        resolved_type,
                        circular_self_reference,
                    });
                }
            } else if member.is_accessor()
                && let Some(accessor) = self.ctx.arena.get_accessor(member)
                && self
                    .ctx
                    .arena
                    .get(accessor.name)
                    .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
            {
                has_late_bound_members = true;
            }
        }

        // Convert accessors to properties (getter-only implies readonly)
        for (name, accessor) in accessors {
            let getter_requires_ts2502 = accessor.getter.as_ref().is_some_and(|getter| {
                getter.circular_self_reference
                    && accessor.setter.as_ref().is_none_or(|setter| {
                        setter.type_annotation.is_none() || setter.circular_self_reference
                    })
            });
            let setter_requires_ts2502 = accessor.setter.as_ref().is_some_and(|setter| {
                setter.circular_self_reference
                    && accessor.getter.as_ref().is_none_or(|getter| {
                        getter.type_annotation.is_none() || getter.circular_self_reference
                    })
            });

            let getter_type = accessor.getter.as_ref().map(|getter| {
                if getter_requires_ts2502 {
                    let name = self.ctx.types.resolve_atom_ref(name).to_string();
                    let message = format!(
                        "'{name}' is referenced directly or indirectly in its own type annotation."
                    );
                    self.error_at_node(getter.name_idx, &message, 2502);
                    TypeId::ANY
                } else if getter.circular_self_reference {
                    accessor
                        .setter
                        .as_ref()
                        .map_or(TypeId::ANY, |setter| setter.resolved_type)
                } else {
                    getter.resolved_type
                }
            });
            let setter_type = accessor.setter.as_ref().map(|setter| {
                if setter_requires_ts2502 {
                    let name = self.ctx.types.resolve_atom_ref(name).to_string();
                    let message = format!(
                        "'{name}' is referenced directly or indirectly in its own type annotation."
                    );
                    self.error_at_node(setter.name_idx, &message, 2502);
                    TypeId::ANY
                } else if setter.circular_self_reference {
                    accessor
                        .getter
                        .as_ref()
                        .map_or(TypeId::UNKNOWN, |getter| getter.resolved_type)
                } else {
                    setter.resolved_type
                }
            });

            let read_type = getter_type.or(setter_type).unwrap_or(TypeId::UNKNOWN);
            let write_type = setter_type.or(getter_type).unwrap_or(read_type);
            let readonly = getter_type.is_some() && setter_type.is_none();
            let primary_name_idx = accessor
                .getter
                .as_ref()
                .or(accessor.setter.as_ref())
                .map(|member| member.name_idx);
            let is_symbol_named =
                primary_name_idx.is_some_and(|name_idx| self.is_symbol_property_name(name_idx));
            let (is_string_named, single_quoted_name) = primary_name_idx
                .map(|name_idx| self.ctx.arena.string_property_name_flags(name_idx))
                .unwrap_or((false, false));
            properties.push(PropertyInfo {
                name,
                type_id: read_type,
                write_type,
                optional: false,
                readonly,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: accessor.declaration_order,
                is_string_named,
                is_symbol_named,
                single_quoted_name,
            });
        }

        // Merge overloaded method signatures into properties.
        // Single-signature methods become Function types; multi-signature become Callable types.
        for key in method_overload_order {
            if let Some(sigs) = method_overloads.remove(&key.name) {
                let optional = sigs.iter().all(|entry| entry.optional);
                let readonly = sigs.iter().any(|entry| entry.readonly);
                let is_symbol_named = sigs.iter().any(|entry| entry.is_symbol_named);
                let method_type = if sigs.len() == 1 {
                    let sig = sigs
                        .into_iter()
                        .next()
                        .expect("sigs.len() == 1 guard ensures at least one element")
                        .signature;
                    factory.function(FunctionShape {
                        type_params: sig.type_params,
                        params: sig.params,
                        this_type: sig.this_type,
                        return_type: sig.return_type,
                        type_predicate: sig.type_predicate,
                        is_constructor: false,
                        is_method: true,
                    })
                } else {
                    let merged_sigs: Vec<CallSignature> =
                        sigs.into_iter().map(|entry| entry.signature).collect();
                    factory.callable(CallableShape {
                        call_signatures: merged_sigs,
                        construct_signatures: Vec::new(),
                        properties: Vec::new(),
                        string_index: None,
                        number_index: None,
                        symbol: None,
                        is_abstract: false,
                    })
                };
                properties.push(PropertyInfo {
                    name: key.name,
                    type_id: method_type,
                    write_type: method_type,
                    optional,
                    readonly,
                    is_method: true,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: key.decl_order,
                    is_string_named: key.is_string_named,
                    is_symbol_named,
                    single_quoted_name: key.single_quoted_name,
                });
            }
        }

        if !call_signatures.is_empty() || !construct_signatures.is_empty() {
            let mut result = factory.callable(CallableShape {
                call_signatures,
                construct_signatures,
                properties,
                string_index,
                number_index,
                symbol: None,
                is_abstract: has_abstract_construct_sig,
            });
            for idx in extra_number_indices {
                let member = factory.object_with_index(ObjectShape {
                    number_index: Some(idx),
                    ..ObjectShape::default()
                });
                result = self.ctx.types.intersect_types_raw2(result, member);
            }
            return result;
        }

        if string_index.is_some() || number_index.is_some() {
            let mut shape = ObjectShape {
                properties,
                string_index,
                number_index,
                ..ObjectShape::default()
            };
            if has_late_bound_members {
                shape.mark_has_late_bound_members();
            }
            let mut result = factory.object_with_index(shape);
            for idx in extra_number_indices {
                let member = factory.object_with_index(ObjectShape {
                    number_index: Some(idx),
                    ..ObjectShape::default()
                });
                result = self.ctx.types.intersect_types_raw2(result, member);
            }
            return result;
        }

        if has_late_bound_members {
            factory.object_with_late_bound_members(properties, None)
        } else {
            factory.object_with_symbol(properties, None)
        }
    }
}
