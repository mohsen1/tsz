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
        // — tsc doesn't report invalid param types on already-malformed index signatures.
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
        _container_node: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;

        // Get resolved index signatures from the Solver (includes inherited)
        let mut index_info = self.ctx.types.get_index_signatures(iface_type);

        // Scan members for own index signatures and detect duplicates (TS2374)
        // Static and instance index signatures are tracked separately —
        // a class can have both `[p: string]: any` and `static [p: string]: number`.
        let mut string_index_nodes: Vec<NodeIndex> = Vec::new();
        let mut number_index_nodes: Vec<NodeIndex> = Vec::new();
        let mut static_string_index_nodes: Vec<NodeIndex> = Vec::new();
        let mut static_number_index_nodes: Vec<NodeIndex> = Vec::new();

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

        // If no index signatures (neither inherited nor own), nothing to check
        if index_info.string_index.is_none() && index_info.number_index.is_none() {
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

        // If both signatures were invalidated, there is nothing to enforce.
        if index_info.string_index.is_none() && index_info.number_index.is_none() {
            return;
        }

        // TS2413: 'number' index type '{0}' is not assignable to 'string' index type '{1}'.
        // TSC always reports this on the number index signature node — it is the
        // number index that violates the string index contract.  When this function
        // is called per-body (merged interfaces), only the body that contains the
        // number index signature should emit TS2413; the other body has no local
        // number_index_nodes so we skip the error to avoid a duplicate at the wrong
        // location.
        if let Some(number_idx) = &index_info.number_index
            && let Some(string_idx) = &index_info.string_index
            && !number_index_nodes.is_empty()
        {
            let is_assignable = self.is_assignable_to(number_idx.value_type, string_idx.value_type);
            if !is_assignable {
                let num_value_str = self.format_type(number_idx.value_type);
                let str_value_str = self.format_type(string_idx.value_type);

                for &node_idx in &number_index_nodes {
                    self.error_at_node_msg(
                            node_idx,
                            crate::diagnostics::diagnostic_codes::INDEX_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
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

            // Extract property name, name node index, and property type based on
            // member kind. Each branch computes prop_type using the appropriate
            // method for that member kind.
            let (prop_name, name_idx, prop_type) =
                if member_node.kind == syntax_kind_ext::PROPERTY_SIGNATURE {
                    // Interface property members — use type annotation or node type
                    let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                        continue;
                    };
                    let name = self.get_member_name_text(sig.name).unwrap_or_default();
                    let prop_type = if sig.type_annotation.is_some() {
                        self.get_type_from_type_node(sig.type_annotation)
                    } else {
                        self.get_type_of_node(member_idx)
                    };
                    (name, sig.name, prop_type)
                } else if member_node.kind == syntax_kind_ext::METHOD_SIGNATURE {
                    // Interface method members — property type is the function type
                    // (not the return type annotation)
                    let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                        continue;
                    };
                    let name = self.get_member_name_text(sig.name).unwrap_or_default();
                    let prop_type = self.get_type_of_interface_member_simple(member_idx);
                    (name, sig.name, prop_type)
                } else if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                    // Class property declarations
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    // Skip static members — not checked against instance index signatures
                    if self.has_static_modifier(&prop.modifiers) {
                        continue;
                    }
                    // Skip private fields (#name)
                    if let Some(name_node) = self.ctx.arena.get(prop.name)
                        && name_node.kind == tsz_scanner::SyntaxKind::PrivateIdentifier as u16
                    {
                        continue;
                    }
                    let name = self.get_member_name_text(prop.name).unwrap_or_default();
                    let prop_type = if prop.type_annotation.is_some() {
                        self.get_type_from_type_node(prop.type_annotation)
                    } else {
                        self.get_type_of_node(member_idx)
                    };
                    (name, prop.name, prop_type)
                } else if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                    // Class method declarations — property type is the function type
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    // Skip static members — not checked against instance index signatures
                    if self.has_static_modifier(&method.modifiers) {
                        continue;
                    }
                    // Skip private methods (#name)
                    if let Some(name_node) = self.ctx.arena.get(method.name)
                        && name_node.kind == tsz_scanner::SyntaxKind::PrivateIdentifier as u16
                    {
                        continue;
                    }
                    let name = self.get_member_name_text(method.name).unwrap_or_default();
                    let prop_type = self.get_type_of_function(member_idx);
                    (name, method.name, prop_type)
                } else if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                    || member_node.kind == syntax_kind_ext::SET_ACCESSOR
                {
                    // Getter/setter accessor declarations
                    let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                        continue;
                    };
                    // Skip static members — not checked against instance index signatures
                    if self.has_static_modifier(&accessor.modifiers) {
                        continue;
                    }
                    // Skip private accessors (#name)
                    if let Some(name_node) = self.ctx.arena.get(accessor.name)
                        && name_node.kind == tsz_scanner::SyntaxKind::PrivateIdentifier as u16
                    {
                        continue;
                    }
                    let name = self.get_member_name_text(accessor.name).unwrap_or_default();
                    let prop_type = if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                        // For getters, the property type is the return type (T), not
                        // the function type (() => T)
                        if accessor.type_annotation.is_some() {
                            self.get_type_from_type_node(accessor.type_annotation)
                        } else {
                            self.infer_getter_return_type(accessor.body)
                        }
                    } else {
                        // Setter: property type comes from the first parameter's type
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
                    (name, accessor.name, prop_type)
                } else {
                    // Skip other member kinds (index signatures, constructors, etc.)
                    continue;
                };

            // Skip members with unresolved/cascading error types; checker will
            // report those separately and avoid TS2411 cascades.
            if self.type_contains_error(prop_type) {
                continue;
            }

            let is_numeric_property = prop_name.parse::<f64>().is_ok();

            // TSC preserves the original quote style for string-literal property
            // names in TS2411 diagnostics (e.g. `'a': number` → `''a''`,
            // `"-Infinity": string` → `'"-Infinity"'`).  Identifiers and numeric
            // literals are left bare.  We use the raw source text to match TSC.
            let diag_prop_name = if let Some(name_node) = self.ctx.arena.get(name_idx)
                && name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
            {
                self.node_text(name_idx)
                    .unwrap_or_else(|| prop_name.clone())
            } else {
                prop_name.clone()
            };

            // Check against number index signature first (for numeric properties)
            if let Some(ref number_idx) = index_info.number_index
                && is_numeric_property
                && !self.is_assignable_to(prop_type, number_idx.value_type)
            {
                let prop_type_str = self.format_type(prop_type);
                let index_type_str = self.format_type(number_idx.value_type);

                self.error_at_node_msg(
                    name_idx,
                    diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                    &[&diag_prop_name, &prop_type_str, "number", &index_type_str],
                );
            }

            // Check against string index signature
            // Note: ALL properties (including numeric ones) must satisfy string index
            if let Some(ref string_idx) = index_info.string_index
                && !self.is_assignable_to(prop_type, string_idx.value_type)
            {
                let prop_type_str = self.format_type(prop_type);
                let index_type_str = self.format_type(string_idx.value_type);

                self.error_at_node_msg(
                    name_idx,
                    diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                    &[&diag_prop_name, &prop_type_str, "string", &index_type_str],
                );
            }
        }
    }

    /// Check inherited properties (from base interfaces) against the combined
    /// index signatures of the derived interface. This catches cases like:
    /// ```ts
    /// interface A { [s: string]: { a; }; }
    /// interface C { m: {}; }
    /// interface D extends A, C { } // TS2411: C.m not assignable to A's index
    /// ```
    /// The AST-based `check_index_signature_compatibility` only sees own members;
    /// inherited properties live in the solver's resolved object shape.
    pub(crate) fn check_inherited_properties_against_index_signatures(
        &mut self,
        iface_type: TypeId,
        own_members: &[NodeIndex],
        iface_node: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;

        // Collect names of own members so we skip them (already checked by AST walk)
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

        // Get combined index signatures (includes inherited)
        let index_info = self.ctx.types.get_index_signatures(iface_type);

        if index_info.string_index.is_none() && index_info.number_index.is_none() {
            return;
        }

        // Get the object shape from the resolved type to find all properties.
        // Interfaces with index sigs use ObjectWithIndex, so check both variants.
        let evaluated_type = self.evaluate_type_for_assignability(iface_type);
        let shape_id = tsz_solver::object_shape_id(self.ctx.types, evaluated_type)
            .or_else(|| tsz_solver::object_with_index_shape_id(self.ctx.types, evaluated_type));
        let Some(shape_id) = shape_id else {
            return;
        };
        let shape = self.ctx.types.object_shape(shape_id);

        for prop in &shape.properties {
            let prop_name = self.ctx.types.resolve_atom(prop.name);
            // Skip own members (already checked via AST walk)
            if own_names.contains(&prop_name) {
                continue;
            }
            // Skip internal private brand properties (__private_brand_*)
            // These are synthetic properties for private class fields and should
            // not be checked against index signatures
            if prop_name.starts_with("__private_brand_") {
                continue;
            }

            let prop_type = prop.type_id;
            if self.type_contains_error(prop_type) {
                continue;
            }

            let is_numeric_property = prop_name.parse::<f64>().is_ok();

            // Check against number index signature
            if let Some(ref number_idx) = index_info.number_index
                && is_numeric_property
                && !self.is_assignable_to(prop_type, number_idx.value_type)
            {
                let prop_type_str = self.format_type(prop_type);
                let index_type_str = self.format_type(number_idx.value_type);

                // Report on the interface declaration node itself since the
                // inherited property has no local AST node to point to
                self.error_at_node_msg(
                    iface_node,
                    diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                    &[&prop_name, &prop_type_str, "number", &index_type_str],
                );
            }

            // Check against string index signature
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
}
