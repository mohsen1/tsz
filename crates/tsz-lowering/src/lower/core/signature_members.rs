use super::*;

impl<'a> TypeLowering<'a> {
    /// Lower a function-like declaration (Method, Constructor, Function) to a `TypeId`.
    ///
    /// This is used for overload compatibility checking where we need the structural type
    /// of a specific declaration node, which might not be cached in the `node_types` map.
    ///
    /// # Arguments
    /// * `node_idx` - The declaration node index
    /// * `return_type_override` - Optional return type to use instead of the annotation.
    ///   (Useful for implementation signatures where return type is inferred from body)
    ///
    /// # Returns
    /// The `TypeId` of the function shape, or `TypeId::ERROR` if lowering fails.
    pub fn lower_signature_from_declaration(
        &self,
        node_idx: NodeIndex,
        return_type_override: Option<TypeId>,
    ) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.arena.get(node_idx) else {
            return TypeId::ERROR;
        };

        match node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let Some(method) = self.arena.get_method_decl(node) else {
                    return TypeId::ERROR;
                };

                let (type_params, (params, this_type, return_type, type_predicate)) = self
                    .with_type_params(&method.type_parameters, || {
                        let (params, this_type) = self.lower_params_with_this(&method.parameters);

                        let (return_type, type_predicate) =
                            if let Some(override_type) = return_type_override {
                                (override_type, None)
                            } else {
                                self.lower_return_type(method.type_annotation, &params)
                            };

                        (params, this_type, return_type, type_predicate)
                    });

                self.interner.function(tsz_solver::FunctionShape {
                    type_params,
                    params,
                    this_type,
                    return_type,
                    type_predicate,
                    is_constructor: false,
                    is_method: true, // Methods are bivariant
                })
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                let Some(ctor) = self.arena.get_constructor(node) else {
                    return TypeId::ERROR;
                };

                let (params, this_type) = self.lower_params_with_this(&ctor.parameters);

                // Constructors return the instance type (or void/any implicitly)
                // For overload checking, we usually compare the function shapes.
                let return_type = return_type_override.unwrap_or(TypeId::VOID);

                self.interner.function(tsz_solver::FunctionShape {
                    type_params: Vec::new(), // Constructors don't have own type params
                    params,
                    this_type,
                    return_type,
                    type_predicate: None,
                    is_constructor: true,
                    is_method: false,
                })
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                let Some(func) = self.arena.get_function(node) else {
                    return TypeId::ERROR;
                };

                let (type_params, (params, this_type, return_type, type_predicate)) = self
                    .with_type_params(&func.type_parameters, || {
                        let (params, this_type) = self.lower_params_with_this(&func.parameters);

                        let (return_type, type_predicate) =
                            if let Some(override_type) = return_type_override {
                                (override_type, None)
                            } else {
                                self.lower_return_type(func.type_annotation, &params)
                            };

                        (params, this_type, return_type, type_predicate)
                    });

                self.interner.function(tsz_solver::FunctionShape {
                    type_params,
                    params,
                    this_type,
                    return_type,
                    type_predicate,
                    is_constructor: false,
                    is_method: false, // Functions are contravariant (strict)
                })
            }
            _ => TypeId::ERROR,
        }
    }

    pub(super) fn collect_interface_members(&self, members: &NodeList, parts: &mut InterfaceParts) {
        for &idx in &members.nodes {
            let Some(member) = self.arena.get(idx) else {
                continue;
            };

            if let Some(sig) = self.arena.get_signature(member) {
                match member.kind {
                    k if k == syntax_kind_ext::CALL_SIGNATURE => {
                        parts.call_signatures.push(self.lower_call_signature(sig));
                    }
                    k if k == syntax_kind_ext::CONSTRUCT_SIGNATURE => {
                        parts
                            .construct_signatures
                            .push(self.lower_call_signature(sig));
                    }
                    k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                        if let Some(name) = self.lower_signature_name(sig.name) {
                            let is_symbol_named =
                                self.lower_signature_name_is_symbol_named(sig.name);
                            let (is_string_named, single_quoted_name) =
                                self.arena.string_property_name_flags(sig.name);
                            let mut signature = self.lower_call_signature(sig);
                            signature.is_method = true;
                            let readonly = self.arena.has_modifier(
                                &sig.modifiers,
                                tsz_scanner::SyntaxKind::ReadonlyKeyword,
                            );
                            parts.merge_method(
                                name,
                                signature,
                                sig.question_token,
                                readonly,
                                is_symbol_named,
                                is_string_named,
                                single_quoted_name,
                            );
                        } else if self.is_unresolved_computed_property_name(sig.name) {
                            parts.has_late_bound_members = true;
                        }
                    }
                    _ => {
                        if let Some(prop) = self.lower_type_element(idx) {
                            parts.merge_property(prop);
                        } else if self.is_unresolved_computed_property_name(sig.name) {
                            parts.has_late_bound_members = true;
                        }
                    }
                }
                continue;
            }

            if let Some(index_sig) = self.arena.get_index_signature(member)
                && let Some(index_info) = self.lower_index_signature(index_sig)
            {
                parts.merge_index_signature(index_info);
                continue;
            }

            // Handle accessor declarations (get/set) in interfaces and type literals
            if member.is_accessor()
                && let Some(accessor) = self.arena.get_accessor(member)
                && let Some(name) = self.lower_signature_name(accessor.name)
            {
                let is_symbol_named = self.lower_signature_name_is_symbol_named(accessor.name);
                let (is_string_named, single_quoted_name) =
                    self.arena.string_property_name_flags(accessor.name);
                let is_getter = member.kind == syntax_kind_ext::GET_ACCESSOR;
                if is_getter {
                    let getter_type = self.lower_type(accessor.type_annotation);
                    let order = parts.next_declaration_order();
                    // Merge with existing accessor entry or create new one
                    match parts.properties.entry(name) {
                        indexmap::map::Entry::Occupied(mut entry) => {
                            // Update existing property with getter type as read type
                            if let PropertyMerge::Property(prop) = entry.get_mut() {
                                prop.type_id = getter_type;
                                // Getter-only means readonly; if a setter was already
                                // merged, its branch will have set readonly=false already
                                // and we preserve that (both accessor present = not readonly).
                            }
                        }
                        indexmap::map::Entry::Vacant(entry) => {
                            entry.insert(PropertyMerge::Property(PropertyInfo {
                                name,
                                type_id: getter_type,
                                write_type: getter_type,
                                optional: false,
                                readonly: true,
                                is_method: false,
                                is_class_prototype: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                                declaration_order: order,
                                is_string_named,
                                is_symbol_named,
                                single_quoted_name,
                            }));
                        }
                    }
                } else {
                    // Set accessor - extract parameter type
                    let setter_type = accessor
                        .parameters
                        .nodes
                        .first()
                        .and_then(|&param_idx| self.arena.get(param_idx))
                        .and_then(|param_node| self.arena.get_parameter(param_node))
                        .map_or(TypeId::UNKNOWN, |param| {
                            self.lower_type(param.type_annotation)
                        });
                    let order = parts.next_declaration_order();
                    match parts.properties.entry(name) {
                        indexmap::map::Entry::Occupied(mut entry) => {
                            // Update existing property with setter type as write type
                            if let PropertyMerge::Property(prop) = entry.get_mut() {
                                prop.write_type = setter_type;
                                prop.readonly = false;
                            }
                        }
                        indexmap::map::Entry::Vacant(entry) => {
                            entry.insert(PropertyMerge::Property(PropertyInfo {
                                name,
                                type_id: setter_type,
                                write_type: setter_type,
                                optional: false,
                                readonly: false,
                                is_method: false,
                                is_class_prototype: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                                declaration_order: order,
                                is_string_named,
                                is_symbol_named,
                                single_quoted_name,
                            }));
                        }
                    }
                }
            } else if member.is_accessor()
                && let Some(accessor) = self.arena.get_accessor(member)
                && self.is_unresolved_computed_property_name(accessor.name)
            {
                parts.has_late_bound_members = true;
            }
        }
    }

    /// Assign `declaration_order` values by iterating declarations in FORWARD order.
    /// This gives earlier declarations lower order numbers, matching tsc's property
    /// enumeration for diagnostics like TS2740 "missing properties: length, pop, ...".
    pub(super) fn assign_forward_declaration_order(
        &self,
        parts: &mut InterfaceParts,
        declarations: impl Iterator<Item = NodeIndex>,
    ) {
        let mut counter: u32 = 0;
        for decl_idx in declarations {
            let Some(node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.arena.get_interface(node) else {
                continue;
            };
            for &idx in &interface.members.nodes {
                if let Some(name) = self.get_interface_member_name(idx) {
                    parts.declaration_orders.entry(name).or_insert_with(|| {
                        counter += 1;
                        counter
                    });
                }
            }
        }
    }

    /// Cross-file variant of `assign_forward_declaration_order`.
    pub(super) fn assign_forward_declaration_order_cross_file(
        &self,
        parts: &mut InterfaceParts,
        declarations: &[(NodeIndex, &NodeArena)],
    ) {
        let mut counter: u32 = 0;
        for &(decl_idx, decl_arena) in declarations {
            let Some(node) = decl_arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = decl_arena.get_interface(node) else {
                continue;
            };
            let lowerer = self.with_arena(decl_arena);
            for &idx in &interface.members.nodes {
                if let Some(name) = lowerer.get_interface_member_name(idx) {
                    parts.declaration_orders.entry(name).or_insert_with(|| {
                        counter += 1;
                        counter
                    });
                }
            }
        }
    }

    /// Extract the property/method name from an interface member node.
    fn get_interface_member_name(&self, idx: NodeIndex) -> Option<Atom> {
        let member = self.arena.get(idx)?;
        if let Some(sig) = self.arena.get_signature(member) {
            return self.lower_signature_name(sig.name);
        }
        if (member.kind == syntax_kind_ext::GET_ACCESSOR
            || member.kind == syntax_kind_ext::SET_ACCESSOR)
            && let Some(accessor) = self.arena.get_accessor(member)
        {
            return self.lower_signature_name(accessor.name);
        }
        None
    }

    pub(super) fn finish_interface_parts(
        &self,
        mut parts: InterfaceParts,
        symbol_id: Option<tsz_binder::SymbolId>,
    ) -> TypeId {
        // When an interface (or merged interface group) carries multiple string-keyed
        // index signatures with distinct key patterns (e.g. `[k: \`data-${string}\`]`
        // and `[k: \`aria-${string}\`]`), union their key types so that excess-property
        // checking accepts a property whose name matches ANY of the patterns.
        for extra in parts.extra_string_indices.drain(..) {
            if let Some(ref mut existing) = parts.string_index {
                existing.key_type = self.interner.union2(existing.key_type, extra.key_type);
                if existing.value_type != extra.value_type {
                    existing.value_type =
                        self.interner.union2(existing.value_type, extra.value_type);
                }
                existing.readonly &= extra.readonly;
            } else {
                parts.string_index = Some(extra);
            }
        }

        let mut properties = Vec::with_capacity(parts.properties.len());
        for (name, entry) in parts.properties {
            // Use forward declaration order when available (corrects reverse iteration order)
            let forward_order = parts.declaration_orders.get(&name).copied();
            if let PropertyMerge::Method(methods) = entry {
                let type_id = self.interner.callable(CallableShape {
                    call_signatures: methods.signatures,
                    construct_signatures: Vec::new(),
                    properties: Vec::new(),
                    ..Default::default()
                });
                properties.push(PropertyInfo {
                    name,
                    type_id,
                    write_type: type_id,
                    optional: methods.optional,
                    readonly: methods.readonly,
                    is_method: true,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: forward_order.unwrap_or(methods.declaration_order),
                    is_string_named: methods.is_string_named,
                    is_symbol_named: methods.is_symbol_named,
                    single_quoted_name: methods.single_quoted_name,
                });
            } else if let PropertyMerge::Property(mut prop) = entry {
                if let Some(order) = forward_order {
                    prop.declaration_order = order;
                }
                properties.push(prop);
            } else if let PropertyMerge::Conflict(mut prop) = entry {
                if let Some(order) = forward_order {
                    prop.declaration_order = order;
                }
                properties.push(prop);
            }
        }

        if !parts.call_signatures.is_empty() || !parts.construct_signatures.is_empty() {
            return self.interner.callable(CallableShape {
                call_signatures: parts.call_signatures,
                construct_signatures: parts.construct_signatures,
                properties,
                string_index: parts.string_index,
                number_index: parts.number_index,
                symbol: symbol_id,
                is_abstract: false,
            });
        }

        let flags = if parts.has_late_bound_members {
            ObjectFlags::HAS_LATE_BOUND_MEMBERS
        } else {
            ObjectFlags::empty()
        };

        if parts.string_index.is_some() || parts.number_index.is_some() {
            if !self.index_signature_properties_compatible(
                &properties,
                parts.string_index.as_ref(),
                parts.number_index.as_ref(),
            ) {
                return TypeId::ERROR;
            }
            return self.interner.object_with_index(ObjectShape {
                properties,
                string_index: parts.string_index,
                number_index: parts.number_index,
                symbol: symbol_id,
                flags,
            });
        }

        self.interner
            .object_with_flags_and_symbol(properties, flags, symbol_id)
    }

    pub(super) fn lower_call_signature(&self, sig: &SignatureData) -> CallSignature {
        let (type_params, (params, this_type, return_type, type_predicate)) = self
            .with_type_params(&sig.type_parameters, || {
                let (params, this_type) = self.lower_signature_params(sig);
                let (return_type, type_predicate) =
                    self.lower_return_type(sig.type_annotation, &params);
                (params, this_type, return_type, type_predicate)
            });

        CallSignature {
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
            is_method: false,
        }
    }

    pub(super) fn lower_method_signature(&self, sig: &SignatureData) -> TypeId {
        let (type_params, (params, this_type, return_type, type_predicate)) = self
            .with_type_params(&sig.type_parameters, || {
                let (params, this_type) = self.lower_signature_params(sig);
                let (return_type, type_predicate) =
                    self.lower_return_type(sig.type_annotation, &params);
                (params, this_type, return_type, type_predicate)
            });

        self.interner.function(FunctionShape {
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
            is_constructor: false,
            is_method: true,
        })
    }

    pub fn lower_interface_member_simple_type(&self, member_idx: NodeIndex) -> Option<TypeId> {
        let member = self.arena.get(member_idx)?;
        let sig = self.arena.get_signature(member)?;

        match member.kind {
            k if k == syntax_kind_ext::METHOD_SIGNATURE => Some(self.lower_method_signature(sig)),
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE => {
                let base = if sig.type_annotation.is_some() {
                    self.lower_type(sig.type_annotation)
                } else {
                    TypeId::ANY
                };
                if sig.question_token {
                    Some(self.interner.union(vec![base, TypeId::UNDEFINED]))
                } else {
                    Some(base)
                }
            }
            _ => None,
        }
    }

    pub fn lower_interface_members_simple_types(
        &self,
        interface_idx: NodeIndex,
        member_indices: &[NodeIndex],
    ) -> Option<LoweredInterfaceMemberTypes> {
        let interface = self
            .arena
            .get(interface_idx)
            .and_then(|node| self.arena.get_interface(node))?;

        let params = if let Some(type_params) = &interface.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.push_type_param_scope();
            Some(self.collect_type_parameters(type_params))
        } else {
            None
        };

        let mut lowered = Vec::with_capacity(member_indices.len());
        for &member_idx in member_indices {
            let Some(member_type) = self.lower_interface_member_simple_type(member_idx) else {
                if params.is_some() {
                    self.pop_type_param_scope();
                }
                return None;
            };
            lowered.push((member_idx, member_type));
        }

        if params.is_some() {
            self.pop_type_param_scope();
        }

        Some((params.unwrap_or_default(), lowered))
    }

    fn lower_signature_params(&self, sig: &SignatureData) -> (Vec<ParamInfo>, Option<TypeId>) {
        let Some(params) = &sig.parameters else {
            return (Vec::new(), None);
        };
        self.lower_params_with_this(params)
    }

    pub(super) fn lower_signature_name(&self, node_idx: NodeIndex) -> Option<Atom> {
        let node = self.arena.get(node_idx)?;
        if let Some(id_data) = self.arena.get_identifier(node) {
            return Some(self.interner.intern_string(&id_data.escaped_text));
        }
        if let Some(lit_data) = self.arena.get_literal(node)
            && !lit_data.text.is_empty()
        {
            // Canonicalize numeric property names (e.g. "1.", "1.0" -> "1")
            if node.is_numeric_literal()
                && let Some(canonical) =
                    tsz_solver::utils::canonicalize_numeric_name(&lit_data.text)
            {
                return Some(self.interner.intern_string(&canonical));
            }
            return Some(self.interner.intern_string(&lit_data.text));
        }
        // Handle computed property names like [Symbol.iterator]
        if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(node)
        {
            // Try the checker-provided computed-name resolver before the
            // syntax-only well-known Symbol fallback. The resolver can use
            // binding identity, which matters when user code shadows `Symbol`.
            // The arena-aware variant takes precedence: it distinguishes the same
            // NodeIndex value across different arenas (cross-arena lowering).
            let arena_ptr: *const NodeArena = self.arena;
            if let Some(resolver) = self.computed_name_resolver_with_arena
                && let Some(name) = resolver(computed.expression, arena_ptr)
            {
                return Some(name);
            } else if let Some(resolver) = self.computed_name_resolver
                && let Some(name) = resolver(computed.expression)
            {
                return Some(name);
            }
            if let Some(symbol_name) = self.get_well_known_symbol_name(computed.expression) {
                return Some(self.interner.intern_string(&symbol_name));
            }
        }
        None
    }

    /// Returns true when `name_idx` refers to a `COMPUTED_PROPERTY_NAME` node whose
    /// expression could not be resolved to a static string/symbol key by
    /// `lower_signature_name`. Callers use this after `lower_signature_name`
    /// returned `None` to distinguish "genuinely unresolvable computed name"
    /// (which implies a late-bound member) from "missing or malformed node".
    pub(super) fn is_unresolved_computed_property_name(&self, name_idx: NodeIndex) -> bool {
        self.arena
            .get(name_idx)
            .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
    }

    pub(super) fn lower_signature_name_is_symbol_named(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let Some(computed) = self.arena.get_computed_property(node) else {
            return false;
        };
        let arena_ptr: *const NodeArena = self.arena;
        if let Some(resolver) = self.computed_symbol_name_resolver_with_arena {
            return resolver(computed.expression, arena_ptr);
        }
        if let Some(resolver) = self.computed_symbol_name_resolver {
            return resolver(computed.expression);
        }
        self.get_well_known_symbol_name(computed.expression)
            .is_some()
    }

    /// Try to resolve a computed property expression to a well-known symbol name.
    /// Returns names like "[Symbol.iterator]", "[Symbol.asyncIterator]", etc.
    fn get_well_known_symbol_name(&self, expr_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(expr_idx)?;

        // Handle Symbol.iterator (property access: Symbol.iterator)
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            let base_node = self.arena.get(access.expression)?;
            let base_ident = self.arena.get_identifier(base_node)?;
            if base_ident.escaped_text == "Symbol" {
                let name_node = self.arena.get(access.name_or_argument)?;
                let name_ident = self.arena.get_identifier(name_node)?;
                return Some(format!("[Symbol.{}]", name_ident.escaped_text));
            }
        }

        None
    }

    pub(super) fn lower_index_signature(&self, sig: &IndexSignatureData) -> Option<IndexSignature> {
        let param_idx = sig
            .parameters
            .nodes
            .first()
            .copied()
            .unwrap_or(NodeIndex::NONE);
        let param_node = self.arena.get(param_idx)?;
        let param_data = self.arena.get_parameter(param_node)?;
        let key_type = self.lower_type(param_data.type_annotation);
        let value_type = self.lower_type(sig.type_annotation);
        let readonly = self
            .arena
            .has_modifier(&sig.modifiers, tsz_scanner::SyntaxKind::ReadonlyKeyword);

        let param_name = self
            .arena
            .get(param_data.name)
            .and_then(|name_node| self.arena.get_identifier(name_node))
            .map(|name_ident| self.interner.intern_string(&name_ident.escaped_text));

        Some(IndexSignature {
            key_type,
            value_type,
            readonly,
            param_name,
        })
    }

    pub(super) const fn index_signature_properties_compatible(
        &self,
        _properties: &[PropertyInfo],
        _string_index: Option<&IndexSignature>,
        _number_index: Option<&IndexSignature>,
    ) -> bool {
        true
    }

    /// Lower a type element (property signature, method signature, etc.)
    pub(super) fn lower_type_element(&self, node_idx: NodeIndex) -> Option<PropertyInfo> {
        let node = self.arena.get(node_idx)?;

        // Check if it's a property or method signature
        if let Some(sig) = self.arena.get_signature(node) {
            // Get property name as Arc<str>
            let name = self.lower_signature_name(sig.name)?;
            let is_symbol_named = self.lower_signature_name_is_symbol_named(sig.name);
            let (is_string_named, single_quoted_name) =
                self.arena.string_property_name_flags(sig.name);

            // Check for readonly modifier
            let readonly = self
                .arena
                .has_modifier(&sig.modifiers, tsz_scanner::SyntaxKind::ReadonlyKeyword);

            // Get visibility (for type literals, always Public)
            let visibility = self.arena.get_visibility_from_modifiers(&sig.modifiers);
            let type_id = self.lower_type(sig.type_annotation);
            let write_type = if readonly { TypeId::NONE } else { type_id };

            Some(PropertyInfo {
                name,
                type_id,
                write_type,
                optional: sig.question_token,
                readonly,
                is_method: false,
                is_class_prototype: false,
                visibility,
                parent_id: None, // Type literals don't have parent_id
                declaration_order: 0,
                is_string_named,
                is_symbol_named,
                single_quoted_name,
            })
        } else {
            None
        }
    }
}
