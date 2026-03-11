//! Index signature checking helpers (TS1268, TS2374, TS2411, TS2413).
//!
//! Extracted from `member_access.rs` to keep files focused and under the
//! 2000-line threshold.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check index signature parameter type (TS1268).
    /// An index signature parameter type must be 'string', 'number', 'symbol', or a template literal type.
    pub(crate) fn check_index_signature_parameter_type(&mut self, member_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_scanner::SyntaxKind;

        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        if member_node.kind != syntax_kind_ext::INDEX_SIGNATURE {
            return;
        }

        let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) else {
            return;
        };

        let param_idx = index_sig
            .parameters
            .nodes
            .first()
            .copied()
            .unwrap_or(NodeIndex::NONE);

        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return;
        };

        let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
            return;
        };

        if self.has_parameter_property_modifier(&param_data.modifiers) {
            self.error_at_node(
                param_idx,
                "A parameter property is only allowed in a constructor implementation.",
                diagnostic_codes::A_PARAMETER_PROPERTY_IS_ONLY_ALLOWED_IN_A_CONSTRUCTOR_IMPLEMENTATION,
            );
        }

        // TSC anchors TS2371 at the parameter name, not the initializer.
        if param_data.initializer.is_some() {
            self.error_at_node(
                param_data.name,
                "A parameter initializer is only allowed in a function or constructor implementation.",
                2371,
            );
        }

        // No type annotation means implicit any, which is allowed
        if param_data.type_annotation.is_none() {
            return;
        }

        let Some(type_node) = self.ctx.arena.get(param_data.type_annotation) else {
            return;
        };

        // Check if the type annotation is a valid index signature parameter type
        // Valid types: string, number, symbol (keywords), template literal type,
        // or type references to string/number/symbol
        let is_valid = match type_node.kind {
            k if k == SyntaxKind::StringKeyword as u16 => true,
            k if k == SyntaxKind::NumberKeyword as u16 => true,
            k if k == SyntaxKind::SymbolKeyword as u16 => true,
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => true,
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                // Type references like "string", "number", "symbol" (referring to built-in types)
                if let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) {
                    tracing::trace!(
                        type_name_idx = type_ref.type_name.0,
                        "check_index_signature_parameter_type: got type_ref"
                    );
                    if let Some(name_node) = self.ctx.arena.get(type_ref.type_name) {
                        tracing::trace!(
                            name_node_kind = name_node.kind,
                            "check_index_signature_parameter_type: got name_node"
                        );
                        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                            let name = ident.escaped_text.as_str();
                            tracing::trace!(
                                type_name = name,
                                "check_index_signature_parameter_type: got identifier"
                            );
                            matches!(name, "string" | "number" | "symbol")
                        } else {
                            tracing::trace!(
                                "check_index_signature_parameter_type: not an identifier"
                            );
                            false
                        }
                    } else {
                        tracing::trace!("check_index_signature_parameter_type: no name_node");
                        false
                    }
                } else {
                    tracing::trace!("check_index_signature_parameter_type: no type_ref");
                    false
                }
            }
            _ => false,
        };

        tracing::trace!(
            is_valid,
            "check_index_signature_parameter_type: validation result"
        );

        // Suppress TS1268 when the parameter already has grammar errors (rest/optional)
        // -- tsc doesn't report invalid param types on already-malformed index signatures.
        let has_param_grammar_error = param_data.dot_dot_dot_token || param_data.question_token;

        if !is_valid && !has_param_grammar_error {
            self.error_at_node(
                param_idx,
                diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
            );
        }
    }

    /// Check that property types are assignable to index signature types (TS2411).
    ///
    /// For each index signature, all properties (including methods and getters/setters)
    /// must have types assignable to the index signature's value type.
    ///
    /// Example:
    /// ```typescript
    /// interface I {
    ///     [s: string]: number;  // All properties must be number
    ///     "": string;           // Error TS2411: string is not assignable to number
    /// }
    /// ```
    pub(crate) fn check_index_signature_compatibility(
        &mut self,
        members: &[NodeIndex],
        iface_type: TypeId,
        container_node: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;

        // Get resolved index signatures from the Solver (includes inherited)
        let mut index_info = self.ctx.types.get_index_signatures(iface_type);

        // The solver's ObjectShape only has string_index/number_index fields,
        // so symbol index signatures get misclassified into string_index with
        // key_type=SYMBOL.  Extract any inherited symbol index from string_index
        // so we can check symbol-keyed properties against it.
        let mut inherited_symbol_value_type: Option<TypeId> = None;
        if let Some(ref si) = index_info.string_index {
            if si.key_type == TypeId::SYMBOL {
                inherited_symbol_value_type = Some(si.value_type);
                index_info.string_index = None;
            }
        }

        // Scan members for own index signatures and detect duplicates (TS2374)
        // Static and instance index signatures are tracked separately --
        // a class can have both `[p: string]: any` and `static [p: string]: number`.
        let mut string_index_nodes: Vec<NodeIndex> = Vec::new();
        let mut number_index_nodes: Vec<NodeIndex> = Vec::new();
        let mut symbol_index_nodes: Vec<NodeIndex> = Vec::new();
        let mut static_string_index_nodes: Vec<NodeIndex> = Vec::new();
        let mut static_number_index_nodes: Vec<NodeIndex> = Vec::new();
        let mut static_symbol_index_nodes: Vec<NodeIndex> = Vec::new();

        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind != syntax_kind_ext::INDEX_SIGNATURE {
                continue;
            }

            let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) else {
                continue;
            };

            let is_static = self.has_static_modifier(&index_sig.modifiers);

            // Get the index signature type
            if index_sig.type_annotation.is_none() {
                continue;
            }

            let value_type = self.get_type_from_type_node(index_sig.type_annotation);

            // Determine if this is a string or number index signature
            let param_idx = index_sig
                .parameters
                .nodes
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE);
            if param_idx.is_none() {
                continue;
            }

            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            if param.type_annotation.is_none() {
                continue;
            }

            let param_type = self.get_type_from_type_node(param.type_annotation);

            // Store the index signature based on parameter type and static-ness
            // Own index signatures take priority over inherited ones
            if param_type == TypeId::NUMBER {
                if is_static {
                    static_number_index_nodes.push(member_idx);
                } else {
                    number_index_nodes.push(member_idx);
                    index_info.number_index = Some(tsz_solver::IndexSignature {
                        key_type: TypeId::NUMBER,
                        value_type,
                        readonly: false,
                        param_name: None,
                    });
                }
            } else if param_type == TypeId::STRING {
                if is_static {
                    static_string_index_nodes.push(member_idx);
                } else {
                    string_index_nodes.push(member_idx);
                    index_info.string_index = Some(tsz_solver::IndexSignature {
                        key_type: TypeId::STRING,
                        value_type,
                        readonly: false,
                        param_name: None,
                    });
                }
            } else if param_type == TypeId::SYMBOL {
                if is_static {
                    static_symbol_index_nodes.push(member_idx);
                } else {
                    symbol_index_nodes.push(member_idx);
                }
            }
        }

        // TS2374: Duplicate index signature for type 'string'/'number'
        // Check instance and static index signatures separately
        for nodes in [&string_index_nodes, &static_string_index_nodes] {
            if nodes.len() > 1 {
                for &node_idx in nodes {
                    self.error_at_node_msg(
                        node_idx,
                        crate::diagnostics::diagnostic_codes::DUPLICATE_INDEX_SIGNATURE_FOR_TYPE,
                        &["string"],
                    );
                }
            }
        }
        for nodes in [&number_index_nodes, &static_number_index_nodes] {
            if nodes.len() > 1 {
                for &node_idx in nodes {
                    self.error_at_node_msg(
                        node_idx,
                        crate::diagnostics::diagnostic_codes::DUPLICATE_INDEX_SIGNATURE_FOR_TYPE,
                        &["number"],
                    );
                }
            }
        }
        for nodes in [&symbol_index_nodes, &static_symbol_index_nodes] {
            if nodes.len() > 1 {
                for &node_idx in nodes {
                    self.error_at_node_msg(
                        node_idx,
                        crate::diagnostics::diagnostic_codes::DUPLICATE_INDEX_SIGNATURE_FOR_TYPE,
                        &["symbol"],
                    );
                }
            }
        }

        // Extract static index signature value types for TS2411 checking.
        let static_string_value_type = if !static_string_index_nodes.is_empty() {
            let node_idx = static_string_index_nodes[0];
            self.ctx
                .arena
                .get(node_idx)
                .and_then(|n| self.ctx.arena.get_index_signature(n))
                .filter(|sig| sig.type_annotation.is_some())
                .map(|sig| self.get_type_from_type_node(sig.type_annotation))
        } else {
            None
        };
        let static_number_value_type = if !static_number_index_nodes.is_empty() {
            let node_idx = static_number_index_nodes[0];
            self.ctx
                .arena
                .get(node_idx)
                .and_then(|n| self.ctx.arena.get_index_signature(n))
                .filter(|sig| sig.type_annotation.is_some())
                .map(|sig| self.get_type_from_type_node(sig.type_annotation))
        } else {
            None
        };

        // Extract symbol index value types (tracked locally, not in IndexInfo).
        // Own symbol index takes priority over inherited.
        let symbol_value_type = if !symbol_index_nodes.is_empty() {
            let node_idx = symbol_index_nodes[0];
            self.ctx
                .arena
                .get(node_idx)
                .and_then(|n| self.ctx.arena.get_index_signature(n))
                .filter(|sig| sig.type_annotation.is_some())
                .map(|sig| self.get_type_from_type_node(sig.type_annotation))
        } else {
            inherited_symbol_value_type
        };
        let static_symbol_value_type = if !static_symbol_index_nodes.is_empty() {
            let node_idx = static_symbol_index_nodes[0];
            self.ctx
                .arena
                .get(node_idx)
                .and_then(|n| self.ctx.arena.get_index_signature(n))
                .filter(|sig| sig.type_annotation.is_some())
                .map(|sig| self.get_type_from_type_node(sig.type_annotation))
        } else {
            None
        };

        let has_instance_index = index_info.string_index.is_some()
            || index_info.number_index.is_some()
            || symbol_value_type.is_some();
        let has_static_index = static_string_value_type.is_some()
            || static_number_value_type.is_some()
            || static_symbol_value_type.is_some();

        // If no index signatures (neither inherited/own instance nor own static),
        // nothing to check.
        if !has_instance_index && !has_static_index {
            return;
        }

        // Skip checks when signature value types are unresolved/cascading errors.
        // This mirrors TS's behavior of avoiding secondary errors after earlier
        // resolution failures, especially for imported module/type alias edges.
        if let Some(number_idx) = &index_info.number_index
            && self.type_contains_error(number_idx.value_type)
        {
            index_info.number_index = None;
        }
        if let Some(string_idx) = &index_info.string_index
            && self.type_contains_error(string_idx.value_type)
        {
            index_info.string_index = None;
        }

        // If all instance signatures were invalidated and no static/symbol ones, nothing to enforce.
        if index_info.string_index.is_none()
            && index_info.number_index.is_none()
            && symbol_value_type.is_none()
            && !has_static_index
        {
            return;
        }

        // TS2413: 'number' index type '{0}' is not assignable to 'string' index type '{1}'.
        // TSC always reports this on the number index signature node -- it is the
        // number index that violates the string index contract.  When this function
        // is called per-body (merged interfaces), only the body that contains the
        // number index signature should emit TS2413; the other body has no local
        // number_index_nodes so we skip the error to avoid a duplicate at the wrong
        // location.
        if let Some(number_idx) = &index_info.number_index
            && let Some(string_idx) = &index_info.string_index
        {
            // Only emit when we have own number index nodes to anchor the error,
            // OR when both signatures are inherited (anchor on container).
            let is_assignable = self.is_assignable_to(number_idx.value_type, string_idx.value_type);
            if !is_assignable {
                let num_value_str = self.format_type(number_idx.value_type);
                let str_value_str = self.format_type(string_idx.value_type);

                if !number_index_nodes.is_empty() {
                    for &node_idx in &number_index_nodes {
                        self.error_at_node_msg(
                            node_idx,
                            diagnostic_codes::INDEX_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                            &["number", &num_value_str, "string", &str_value_str],
                        );
                    }
                } else if string_index_nodes.is_empty() {
                    // Both signatures are truly inherited (not from a merged
                    // body) — report on the declaration name.  When only
                    // number_index_nodes is empty but string_index_nodes has
                    // entries, we are in a merged interface body that doesn't
                    // own the number index; the body that does will emit the
                    // error, so we skip to avoid a duplicate at a wrong
                    // location.
                    let error_node = self
                        .get_declaration_name_node(container_node)
                        .unwrap_or(container_node);
                    self.error_at_node_msg(
                        error_node,
                        diagnostic_codes::INDEX_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                        &["number", &num_value_str, "string", &str_value_str],
                    );
                }
            }
        }

        // TS2413 for static index signatures: same rule applies to the static side.
        if let (Some(static_num_type), Some(static_str_type)) =
            (static_number_value_type, static_string_value_type)
        {
            let is_assignable = self.is_assignable_to(static_num_type, static_str_type);
            if !is_assignable {
                let num_value_str = self.format_type(static_num_type);
                let str_value_str = self.format_type(static_str_type);

                for &node_idx in &static_number_index_nodes {
                    self.error_at_node_msg(
                        node_idx,
                        diagnostic_codes::INDEX_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                        &["number", &num_value_str, "string", &str_value_str],
                    );
                }
            }
        }

        // Check each property/method against applicable index signatures
        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Extract property name, name node index, property type, and
            // whether this member is static.
            let (prop_name, name_idx, prop_type, is_static_member) =
                if member_node.kind == syntax_kind_ext::PROPERTY_SIGNATURE {
                    let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                        continue;
                    };
                    let name = self.get_member_name_text(sig.name).unwrap_or_default();
                    let prop_type = if sig.type_annotation.is_some() {
                        self.get_type_from_type_node(sig.type_annotation)
                    } else {
                        self.get_type_of_node(member_idx)
                    };
                    (name, sig.name, prop_type, false)
                } else if member_node.kind == syntax_kind_ext::METHOD_SIGNATURE {
                    let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                        continue;
                    };
                    let name = self.get_member_name_text(sig.name).unwrap_or_default();
                    let prop_type = self.get_type_of_interface_member_simple(member_idx);
                    (name, sig.name, prop_type, false)
                } else if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    let is_static = self.has_static_modifier(&prop.modifiers);
                    if let Some(name_node) = self.ctx.arena.get(prop.name)
                        && name_node.kind == tsz_scanner::SyntaxKind::PrivateIdentifier as u16
                    {
                        continue;
                    }
                    let name = self.get_member_name_text(prop.name).unwrap_or_default();
                    let prop_type = if let Some(declared_type) =
                        self.effective_class_property_declared_type(member_idx, prop)
                    {
                        declared_type
                    } else {
                        self.get_type_of_node(member_idx)
                    };
                    (name, prop.name, prop_type, is_static)
                } else if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    let is_static = self.has_static_modifier(&method.modifiers);
                    if let Some(name_node) = self.ctx.arena.get(method.name)
                        && name_node.kind == tsz_scanner::SyntaxKind::PrivateIdentifier as u16
                    {
                        continue;
                    }
                    let name = self.get_member_name_text(method.name).unwrap_or_default();
                    let prop_type = self.get_type_of_function(member_idx);
                    (name, method.name, prop_type, is_static)
                } else if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                    || member_node.kind == syntax_kind_ext::SET_ACCESSOR
                {
                    let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                        continue;
                    };
                    let is_static = self.has_static_modifier(&accessor.modifiers);
                    if let Some(name_node) = self.ctx.arena.get(accessor.name)
                        && name_node.kind == tsz_scanner::SyntaxKind::PrivateIdentifier as u16
                    {
                        continue;
                    }
                    let name = self.get_member_name_text(accessor.name).unwrap_or_default();
                    let prop_type = if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                        if accessor.type_annotation.is_some() {
                            self.get_type_from_type_node(accessor.type_annotation)
                        } else {
                            self.infer_getter_return_type(accessor.body)
                        }
                    } else {
                        let type_ann = accessor
                            .parameters
                            .nodes
                            .first()
                            .and_then(|&param_idx| self.ctx.arena.get(param_idx))
                            .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                            .map(|param| param.type_annotation)
                            .unwrap_or(NodeIndex::NONE);
                        if type_ann.is_some() {
                            self.get_type_from_type_node(type_ann)
                        } else {
                            self.get_type_of_node(member_idx)
                        }
                    };
                    (name, accessor.name, prop_type, is_static)
                } else {
                    continue;
                };

            // Symbol-keyed properties are NOT checked against string or number
            // index signatures, but they ARE checked against symbol index
            // signatures (TS2411).
            if self.is_symbol_named_property(name_idx) {
                if !self.type_contains_error(prop_type) {
                    let applicable_symbol_value = if is_static_member {
                        static_symbol_value_type
                    } else {
                        symbol_value_type
                    };
                    if let Some(sym_value_type) = applicable_symbol_value
                        && !self.is_assignable_to(prop_type, sym_value_type)
                    {
                        let prop_type_str = self.format_type(prop_type);
                        let index_type_str = self.format_type(sym_value_type);
                        self.error_at_node_msg(
                            name_idx,
                            diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                            &[&prop_name, &prop_type_str, "symbol", &index_type_str],
                        );
                    }
                }
                continue;
            }

            // Skip members with unresolved/cascading error types
            if self.type_contains_error(prop_type) {
                continue;
            }

            let is_numeric_property = prop_name.parse::<f64>().is_ok();

            // TSC preserves the original quote style for string-literal property
            // names in TS2411 diagnostics.
            let diag_prop_name = if let Some(name_node) = self.ctx.arena.get(name_idx)
                && name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
            {
                self.node_text(name_idx)
                    .unwrap_or_else(|| prop_name.clone())
            } else {
                prop_name.clone()
            };

            // Select the applicable index signatures: static members check
            // against static index signatures, instance members check against
            // instance index signatures.
            let applicable_number_value = if is_static_member {
                static_number_value_type
            } else {
                index_info.number_index.as_ref().map(|idx| idx.value_type)
            };
            let applicable_string_value = if is_static_member {
                static_string_value_type
            } else {
                index_info.string_index.as_ref().map(|idx| idx.value_type)
            };

            // Check against number index signature first (for numeric properties)
            if let Some(number_value_type) = applicable_number_value
                && is_numeric_property
                && !self.is_assignable_to(prop_type, number_value_type)
            {
                let prop_type_str = self.format_type(prop_type);
                let index_type_str = self.format_type(number_value_type);

                self.error_at_node_msg(
                    name_idx,
                    diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                    &[&diag_prop_name, &prop_type_str, "number", &index_type_str],
                );
            }

            // Check against string index signature
            if let Some(string_value_type) = applicable_string_value
                && !self.is_assignable_to(prop_type, string_value_type)
            {
                let prop_type_str = self.format_type(prop_type);
                let index_type_str = self.format_type(string_value_type);

                self.error_at_node_msg(
                    name_idx,
                    diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                    &[&diag_prop_name, &prop_type_str, "string", &index_type_str],
                );
            }
        }
    }

    /// Check inherited properties (from base interfaces) against the combined
    /// index signatures of the derived interface.
    pub(crate) fn check_inherited_properties_against_index_signatures(
        &mut self,
        iface_type: TypeId,
        own_members: &[NodeIndex],
        iface_node: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;

        let mut own_names = std::collections::HashSet::new();
        for &member_idx in own_members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::INDEX_SIGNATURE {
                continue;
            }
            if let Some(name_text) = self.get_member_name(member_idx) {
                own_names.insert(name_text);
            }
        }

        let index_info = self.ctx.types.get_index_signatures(iface_type);

        if index_info.string_index.is_none() && index_info.number_index.is_none() {
            return;
        }

        let evaluated_type = self.evaluate_type_for_assignability(iface_type);
        let shape_id = tsz_solver::object_shape_id(self.ctx.types, evaluated_type)
            .or_else(|| tsz_solver::object_with_index_shape_id(self.ctx.types, evaluated_type));
        let Some(shape_id) = shape_id else {
            return;
        };
        let shape = self.ctx.types.object_shape(shape_id);

        for prop in &shape.properties {
            let prop_name = self.ctx.types.resolve_atom(prop.name);
            if own_names.contains(&prop_name) {
                continue;
            }
            if prop_name.starts_with("__private_brand_") {
                continue;
            }

            let prop_type = prop.type_id;
            if self.type_contains_error(prop_type) {
                continue;
            }

            let is_numeric_property = prop_name.parse::<f64>().is_ok();

            if let Some(ref number_idx) = index_info.number_index
                && is_numeric_property
                && !self.is_assignable_to(prop_type, number_idx.value_type)
            {
                let prop_type_str = self.format_type(prop_type);
                let index_type_str = self.format_type(number_idx.value_type);

                self.error_at_node_msg(
                    iface_node,
                    diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                    &[&prop_name, &prop_type_str, "number", &index_type_str],
                );
            }

            if let Some(ref string_idx) = index_info.string_index
                && !self.is_assignable_to(prop_type, string_idx.value_type)
            {
                let prop_type_str = self.format_type(prop_type);
                let index_type_str = self.format_type(string_idx.value_type);

                self.error_at_node_msg(
                    iface_node,
                    diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                    &[&prop_name, &prop_type_str, "string", &index_type_str],
                );
            }
        }
    }

    /// Check if a property name node refers to a symbol-keyed property.
    fn is_symbol_named_property(&mut self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return false;
        };
        let Some(expr_node) = self.ctx.arena.get(computed.expression) else {
            return false;
        };

        match expr_node.kind {
            ek if ek == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
                    return false;
                };
                let Some(obj_node) = self.ctx.arena.get(access.expression) else {
                    return false;
                };
                if let Some(ident) = self.ctx.arena.get_identifier(obj_node) {
                    ident.escaped_text.as_str() == "Symbol"
                } else {
                    false
                }
            }
            ek if ek == tsz_scanner::SyntaxKind::Identifier as u16 => {
                let expr_type = self.get_type_of_node(computed.expression);
                self.is_symbol_or_unique_symbol(expr_type)
            }
            _ => false,
        }
    }

    fn is_symbol_or_unique_symbol(&self, type_id: TypeId) -> bool {
        use crate::query_boundaries::type_checking as query;
        query::is_symbol_or_unique_symbol(self.ctx.types, type_id)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::check_source_diagnostics;

    #[test]
    fn ts2413_static_index_signature_number_not_assignable_to_string() {
        let diags = check_source_diagnostics(
            r#"
class B {
    static readonly [s: string]: number;
    static readonly [s: number]: boolean;
}
"#,
        );
        let matching: Vec<_> = diags.iter().filter(|d| d.code == 2413).collect();
        assert_eq!(
            matching.len(),
            1,
            "Expected 1 TS2413 for static index sig mismatch, got: {:?}",
            diags.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ts2413_static_index_signature_compatible_no_error() {
        let diags = check_source_diagnostics(
            r#"
class C {
    static readonly [s: string]: number;
    static readonly [s: number]: 42 | 233;
}
"#,
        );
        let matching: Vec<_> = diags.iter().filter(|d| d.code == 2413).collect();
        assert_eq!(
            matching.len(),
            0,
            "Expected no TS2413 when number index is subtype of string index, got: {matching:?}"
        );
    }

    #[test]
    fn ts2413_inherited_index_signature_conflict() {
        let diags = check_source_diagnostics(
            r#"
interface A {
    [x: string]: string;
}
interface B {
    [x: number]: number;
}
interface C extends A, B {}
"#,
        );
        let matching: Vec<_> = diags.iter().filter(|d| d.code == 2413).collect();
        assert!(
            !matching.is_empty(),
            "Expected TS2413 for inherited index signature conflict, got: {:?}",
            diags.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ts2411_symbol_index_signature_own_property() {
        // Symbol-keyed properties must be assignable to symbol index signature type
        let diags = check_source_diagnostics(
            r#"
interface I {
    [Symbol.iterator]: number;
    [s: symbol]: string;
}
"#,
        );
        let matching: Vec<_> = diags.iter().filter(|d| d.code == 2411).collect();
        assert_eq!(
            matching.len(),
            1,
            "Expected 1 TS2411 for symbol property not assignable to symbol index, got codes: {:?}",
            diags.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ts2411_symbol_index_signature_compatible_no_error() {
        // Compatible symbol property should NOT produce TS2411
        let diags = check_source_diagnostics(
            r#"
interface I {
    [Symbol.iterator]: string;
    [s: symbol]: string;
}
"#,
        );
        let matching: Vec<_> = diags.iter().filter(|d| d.code == 2411).collect();
        assert_eq!(
            matching.len(),
            0,
            "Expected no TS2411 when symbol property is assignable to symbol index, got: {matching:?}"
        );
    }
}
