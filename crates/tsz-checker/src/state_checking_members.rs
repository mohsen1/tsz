//! Declaration & Statement Checking Module (Members)
//!
//! Extracted from state_checking.rs: Second half of CheckerState impl
//! containing interface checking, class member checking, type member
//! validation, and StatementCheckCallbacks implementation.

use crate::state::{CheckerState, MemberAccessInfo, MemberAccessLevel, MemberLookup};
use crate::statements::StatementCheckCallbacks;
use std::rc::Rc;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{ContextualTypeContext, TypeId};

impl<'a> CheckerState<'a> {
    fn enclosing_class_constructor_param_names(&self) -> rustc_hash::FxHashSet<String> {
        let mut names = rustc_hash::FxHashSet::default();

        let Some(class_info) = self.ctx.enclosing_class.as_ref() else {
            return names;
        };

        for &member_idx in &class_info.member_nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                continue;
            };

            for &param_idx in &ctor.parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if let Some(name) = self.get_node_text(param.name) {
                    names.insert(name);
                }
            }
        }

        names
    }

    fn symbol_declared_within_subtree(
        &self,
        sym_id: tsz_binder::SymbolId,
        root_idx: NodeIndex,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        if !symbol.value_declaration.is_none()
            && self.is_node_within(symbol.value_declaration, root_idx)
        {
            return true;
        }

        symbol
            .declarations
            .iter()
            .any(|&decl_idx| self.is_node_within(decl_idx, root_idx))
    }

    fn enclosing_constructor_of_node(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        let mut steps = 0;
        while steps < 256 {
            steps += 1;
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            let parent_idx = ext.parent;
            let parent_node = self.ctx.arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::CONSTRUCTOR {
                return Some(parent_idx);
            }
            current = parent_idx;
        }
        None
    }

    fn symbol_is_constructor_parameter_of_current_class(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let Some(class_info) = self.ctx.enclosing_class.as_ref() else {
            return false;
        };

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        let mut decl_nodes = symbol.declarations.clone();
        if !symbol.value_declaration.is_none() {
            decl_nodes.push(symbol.value_declaration);
        }

        decl_nodes.into_iter().any(|decl_idx| {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            if self.ctx.arena.get_parameter(decl_node).is_none() {
                return false;
            }

            self.enclosing_constructor_of_node(decl_idx)
                .is_some_and(|ctor_idx| class_info.member_nodes.contains(&ctor_idx))
        })
    }

    fn collect_unqualified_identifier_references(
        &self,
        node_idx: NodeIndex,
        refs: &mut Vec<(String, NodeIndex)>,
    ) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.ctx.arena.get_identifier(node) {
                refs.push((ident.escaped_text.clone(), node_idx));
            }
            return;
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if let Some(access) = self.ctx.arena.get_access_expr(node) {
                self.collect_unqualified_identifier_references(access.expression, refs);
            }
            return;
        }

        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            if let Some(access) = self.ctx.arena.get_access_expr(node) {
                self.collect_unqualified_identifier_references(access.expression, refs);
                self.collect_unqualified_identifier_references(access.name_or_argument, refs);
            }
            return;
        }

        if let Some(func) = self.ctx.arena.get_function(node) {
            for &param_idx in &func.parameters.nodes {
                if let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                    && !param.initializer.is_none()
                {
                    self.collect_unqualified_identifier_references(param.initializer, refs);
                }
            }
            if !func.body.is_none() {
                self.collect_unqualified_identifier_references(func.body, refs);
            }
            return;
        }

        match node.kind {
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.collect_unqualified_identifier_references(stmt_idx, refs);
                    }
                }
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
                    self.collect_unqualified_identifier_references(expr_stmt.expression, refs);
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.ctx.arena.get_variable(node) {
                    for &list_idx in &var_stmt.declarations.nodes {
                        if let Some(list_node) = self.ctx.arena.get(list_idx)
                            && let Some(decl_list) = self.ctx.arena.get_variable(list_node)
                        {
                            for &decl_idx in &decl_list.declarations.nodes {
                                if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                                    && let Some(var_decl) =
                                        self.ctx.arena.get_variable_declaration(decl_node)
                                    && !var_decl.initializer.is_none()
                                {
                                    self.collect_unqualified_identifier_references(
                                        var_decl.initializer,
                                        refs,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    self.collect_unqualified_identifier_references(call.expression, refs);
                    if let Some(args) = &call.arguments {
                        for &arg_idx in &args.nodes {
                            self.collect_unqualified_identifier_references(arg_idx, refs);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.collect_unqualified_identifier_references(paren.expression, refs);
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(binary) = self.ctx.arena.get_binary_expr(node) {
                    self.collect_unqualified_identifier_references(binary.left, refs);
                    self.collect_unqualified_identifier_references(binary.right, refs);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                    self.collect_unqualified_identifier_references(cond.condition, refs);
                    self.collect_unqualified_identifier_references(cond.when_true, refs);
                    self.collect_unqualified_identifier_references(cond.when_false, refs);
                }
            }
            _ => {}
        }
    }

    fn check_constructor_param_capture_in_instance_initializer(
        &mut self,
        member_name: &str,
        initializer_idx: NodeIndex,
    ) {
        use crate::types::diagnostics::diagnostic_codes;

        let ctor_param_names = self.enclosing_class_constructor_param_names();
        if ctor_param_names.is_empty() {
            return;
        }

        let mut refs = Vec::new();
        self.collect_unqualified_identifier_references(initializer_idx, &mut refs);

        for (name, ident_idx) in refs {
            if !ctor_param_names.contains(&name) {
                continue;
            }

            if let Some(sym_id) = self
                .ctx
                .binder
                .resolve_identifier(self.ctx.arena, ident_idx)
            {
                if self.symbol_is_constructor_parameter_of_current_class(sym_id) {
                    self.error_at_node_msg(
                        ident_idx,
                        diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS,
                        &[&name],
                    );
                    continue;
                }

                let treat_as_unresolved =
                    self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                        let source_is_external_module = self
                            .ctx
                            .get_binder_for_file(symbol.decl_file_idx as usize)
                            .map(|binder| binder.is_external_module())
                            .unwrap_or(false);

                        self.ctx.binder.is_external_module()
                            && symbol.decl_file_idx != self.ctx.current_file_idx as u32
                            && (source_is_external_module || symbol.is_exported)
                            && (symbol.flags
                                & (tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE
                                    | tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE))
                                != 0
                    });

                if treat_as_unresolved {
                    self.error_at_node_msg(
                        ident_idx,
                        diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS,
                        &[&name],
                    );
                    continue;
                }

                if self.symbol_declared_within_subtree(sym_id, initializer_idx) {
                    continue;
                }

                self.error_at_node_msg(
                    ident_idx,
                    diagnostic_codes::INITIALIZER_OF_INSTANCE_MEMBER_VARIABLE_CANNOT_REFERENCE_IDENTIFIER_DECLARED_IN,
                    &[member_name, &name],
                );
            } else {
                self.error_at_node_msg(
                    ident_idx,
                    diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS,
                    &[&name],
                );
            }
        }
    }

    /// Check an interface declaration.
    pub(crate) fn check_interface_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::types::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(iface) = self.ctx.arena.get_interface(node) else {
            return;
        };

        // TS1042: async modifier cannot be used on interface declarations
        self.check_async_modifier_on_declaration(&iface.modifiers);

        // Check for reserved interface names (error 2427)
        if !iface.name.is_none()
            && let Some(name_node) = self.ctx.arena.get(iface.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            // Reserved type names that can't be used as interface names
            match ident.escaped_text.as_str() {
                "string" | "number" | "boolean" | "symbol" | "void" | "object" => {
                    self.error_at_node(
                        iface.name,
                        &format!("Interface name cannot be '{}'.", ident.escaped_text),
                        diagnostic_codes::INTERFACE_NAME_CANNOT_BE,
                    );
                }
                _ => {}
            }
        }

        // Push type parameters BEFORE checking heritage clauses
        // This allows heritage clauses to reference the interface's type parameters
        let (_type_params, type_param_updates) = self.push_type_parameters(&iface.type_parameters);

        // Collect interface type parameter names for TS2304 checking in heritage clauses
        let interface_type_param_names: Vec<String> = type_param_updates
            .iter()
            .map(|(name, _)| name.clone())
            .collect();

        // Check heritage clauses for unresolved names (TS2304)
        // Must be checked AFTER type parameters are pushed so heritage can reference type params
        self.check_heritage_clauses_for_unresolved_names(
            &iface.heritage_clauses,
            false,
            &interface_type_param_names,
        );

        // Check for unused type parameters (TS6133)
        self.check_unused_type_params(&iface.type_parameters, stmt_idx);

        // Check each interface member for missing type references and parameter properties
        for &member_idx in &iface.members.nodes {
            self.check_type_member_for_missing_names(member_idx);
            self.check_type_member_for_parameter_properties(member_idx);
            // TS1268: Check index signature parameter types
            self.check_index_signature_parameter_type(member_idx);
        }

        // Check for duplicate member names (TS2300)
        self.check_duplicate_interface_members(&iface.members.nodes);

        // Check that properties are assignable to index signatures (TS2411)
        // This includes both directly declared and inherited index signatures.
        // Get the interface type to check for any index signatures (direct or inherited)
        // NOTE: Use get_type_of_symbol to get the cached type, avoiding recursion issues
        let iface_type = if !iface.name.is_none() {
            // Get symbol from the interface name and resolve its type
            if let Some(name_node) = self.ctx.arena.get(iface.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(&ident.escaped_text) {
                        self.get_type_of_symbol(sym_id)
                    } else {
                        TypeId::ERROR
                    }
                } else {
                    TypeId::ERROR
                }
            } else {
                TypeId::ERROR
            }
        } else {
            // Anonymous interface - compute type directly
            self.get_type_of_interface(stmt_idx)
        };

        let index_info = self.ctx.types.get_index_signatures(iface_type);

        // Check if there are own index signatures by scanning members
        let has_own_index_sig = iface.members.nodes.iter().any(|&member_idx| {
            self.ctx
                .arena
                .get(member_idx)
                .map(|node| node.kind == tsz_parser::parser::syntax_kind_ext::INDEX_SIGNATURE)
                .unwrap_or(false)
        });

        // If there are any index signatures (direct, own, or inherited), check compatibility
        if index_info.string_index.is_some()
            || index_info.number_index.is_some()
            || has_own_index_sig
        {
            self.check_index_signature_compatibility(&iface.members.nodes, iface_type);
        }

        // Check that interface correctly extends base interfaces (error 2430)
        self.check_interface_extension_compatibility(stmt_idx, iface);

        self.pop_type_parameters(type_param_updates);
    }

    /// Check index signature parameter type (TS1268).
    /// An index signature parameter type must be 'string', 'number', 'symbol', or a template literal type.
    fn check_index_signature_parameter_type(&mut self, member_idx: NodeIndex) {
        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_parser::parser::syntax_kind_ext;
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

        if !is_valid {
            self.error_at_node(
                param_idx,
                diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
            );
        }
    }

    /// Check for duplicate property names in interface members (TS2300).
    /// TypeScript reports "Duplicate identifier 'X'." for each duplicate occurrence.
    /// NOTE: Method signatures (overloads) are NOT considered duplicates - interfaces allow
    /// multiple method signatures with the same name for function overloading.
    pub(crate) fn check_duplicate_interface_members(&mut self, members: &[NodeIndex]) {
        use crate::types::diagnostics::diagnostic_codes;
        use rustc_hash::FxHashMap;

        // Track property names and their indices (methods are allowed to have overloads)
        let mut seen_properties: FxHashMap<String, Vec<NodeIndex>> = FxHashMap::default();

        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Only check property signatures for duplicates
            // Method signatures can have multiple overloads (same name, different types)
            let name = match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_SIGNATURE => self
                    .ctx
                    .arena
                    .get_signature(member_node)
                    .and_then(|sig| self.get_member_name_text(sig.name)),
                // Method signatures are allowed to have overloads - don't flag as duplicates
                k if k == syntax_kind_ext::METHOD_SIGNATURE => None,
                // Call, construct, and index signatures don't have names that can conflict
                _ => None,
            };

            if let Some(name) = name {
                seen_properties.entry(name).or_default().push(member_idx);
            }
        }

        // Report errors for duplicates
        for (name, indices) in seen_properties {
            if indices.len() > 1 {
                // Report TS2300 for subsequent occurrences only (matching tsc behavior)
                // Skip the first declaration as it's valid
                for &idx in indices.iter().skip(1) {
                    // Get the name node for precise error location
                    let error_node = self.get_interface_member_name_node(idx).unwrap_or(idx);
                    self.error_at_node_msg(
                        error_node,
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                        &[&name],
                    );
                }
            }
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
    ) {
        use crate::types::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext;

        // Get resolved index signatures from the Solver (includes inherited)
        let mut index_info = self.ctx.types.get_index_signatures(iface_type);

        // ALSO scan members array directly for own index signatures
        // The type might not include the interface's own index signatures yet
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

            // Store the index signature based on parameter type
            // Own index signatures take priority over inherited ones
            if param_type == TypeId::NUMBER {
                index_info.number_index = Some(tsz_solver::types::IndexSignature {
                    key_type: TypeId::NUMBER,
                    value_type,
                    readonly: false,
                });
            } else if param_type == TypeId::STRING {
                index_info.string_index = Some(tsz_solver::types::IndexSignature {
                    key_type: TypeId::STRING,
                    value_type,
                    readonly: false,
                });
            }
        }

        // If no index signatures (neither inherited nor own), nothing to check
        if index_info.string_index.is_none() && index_info.number_index.is_none() {
            return;
        }

        // Check each property/method against applicable index signatures
        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Extract property name and type annotation based on member kind
            let (prop_name, name_idx, type_annotation_idx) = if member_node.kind
                == syntax_kind_ext::PROPERTY_SIGNATURE
                || member_node.kind == syntax_kind_ext::METHOD_SIGNATURE
            {
                // Interface members
                let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                    continue;
                };
                let name = self.get_member_name_text(sig.name).unwrap_or_default();
                (name, sig.name, sig.type_annotation)
            } else if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                // Class property declarations
                let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                    continue;
                };
                // Skip private fields (#name) - they are not subject to index signature checks
                if let Some(name_node) = self.ctx.arena.get(prop.name) {
                    if name_node.kind == tsz_scanner::SyntaxKind::PrivateIdentifier as u16 {
                        continue;
                    }
                }
                let name = self.get_member_name_text(prop.name).unwrap_or_default();
                (name, prop.name, prop.type_annotation)
            } else if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                // Class method declarations
                let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                    continue;
                };
                // Skip private methods (#name)
                if let Some(name_node) = self.ctx.arena.get(method.name) {
                    if name_node.kind == tsz_scanner::SyntaxKind::PrivateIdentifier as u16 {
                        continue;
                    }
                }
                let name = self.get_member_name_text(method.name).unwrap_or_default();
                (name, method.name, NodeIndex::NONE) // Methods use member_idx for type
            } else {
                // Skip other member kinds (index signatures, constructors, etc.)
                continue;
            };

            // Get property type from type annotation if available, otherwise from member node
            let prop_type = if !type_annotation_idx.is_none() {
                self.get_type_from_type_node(type_annotation_idx)
            } else {
                // For methods without type annotations, use the member node type
                self.get_type_of_node(member_idx)
            };

            let is_numeric_property = prop_name.parse::<f64>().is_ok();

            // Check against number index signature first (for numeric properties)
            if let Some(ref number_idx) = index_info.number_index {
                if is_numeric_property
                    && !self
                        .ctx
                        .types
                        .is_assignable_to(prop_type, number_idx.value_type)
                {
                    let prop_type_str = self.format_type(prop_type);
                    let index_type_str = self.format_type(number_idx.value_type);

                    self.error_at_node_msg(
                        name_idx,
                        diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                        &[&prop_name, &prop_type_str, "number", &index_type_str],
                    );
                }
            }

            // Check against string index signature
            // Note: ALL properties (including numeric ones) must satisfy string index
            if let Some(ref string_idx) = index_info.string_index {
                if !self
                    .ctx
                    .types
                    .is_assignable_to(prop_type, string_idx.value_type)
                {
                    let prop_type_str = self.format_type(prop_type);
                    let index_type_str = self.format_type(string_idx.value_type);

                    self.error_at_node_msg(
                        name_idx,
                        diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                        &[&prop_name, &prop_type_str, "string", &index_type_str],
                    );
                }
            }
        }
    }

    /// Get property information needed for index signature checking.
    /// Returns (property_name, property_type, name_node_index).

    /// Get the name text from a member name node (identifier, string literal, or computed).
    fn get_member_name_text(&self, name_idx: NodeIndex) -> Option<String> {
        use tsz_scanner::SyntaxKind;

        if name_idx.is_none() {
            return None;
        }

        let name_node = self.ctx.arena.get(name_idx)?;

        match name_node.kind {
            k if k == SyntaxKind::Identifier as u16 => self
                .ctx
                .arena
                .get_identifier(name_node)
                .map(|id| id.escaped_text.to_string()),
            k if k == SyntaxKind::StringLiteral as u16 => self
                .ctx
                .arena
                .get_literal(name_node)
                .map(|lit| lit.text.to_string()),
            k if k == SyntaxKind::NumericLiteral as u16 => self
                .ctx
                .arena
                .get_literal(name_node)
                .map(|lit| lit.text.to_string()),
            k if k == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                // For computed property names with string/numeric literal expressions like ["a"],
                // extract the value for duplicate checking. tsc formats these as ["a"] in diagnostics.
                let computed = self.ctx.arena.get_computed_property(name_node)?;
                let expr_node = self.ctx.arena.get(computed.expression)?;
                match expr_node.kind {
                    ek if ek == SyntaxKind::StringLiteral as u16 => {
                        let lit = self.ctx.arena.get_literal(expr_node)?;
                        Some(format!("[\"{}\"]", lit.text))
                    }
                    ek if ek == SyntaxKind::NumericLiteral as u16 => {
                        let lit = self.ctx.arena.get_literal(expr_node)?;
                        Some(lit.text.to_string())
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Get the name node from an interface member for error reporting.
    fn get_interface_member_name_node(&self, member_idx: NodeIndex) -> Option<NodeIndex> {
        let member_node = self.ctx.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE => self
                .ctx
                .arena
                .get_signature(member_node)
                .map(|sig| sig.name)
                .filter(|idx: &NodeIndex| !idx.is_none()),
            k if k == syntax_kind_ext::METHOD_SIGNATURE => self
                .ctx
                .arena
                .get_signature(member_node)
                .map(|sig| sig.name)
                .filter(|idx: &NodeIndex| !idx.is_none()),
            _ => None,
        }
    }

    /// Report TS2300 "Duplicate identifier" error for a class member (property or method).
    /// Helper function to avoid code duplication in check_duplicate_class_members.
    fn report_duplicate_class_member_ts2300(&mut self, member_idx: NodeIndex) {
        use crate::types::diagnostics::diagnostic_codes;

        let member_node = self.ctx.arena.get(member_idx);
        let (name, error_node) = match member_node.map(|n| n.kind) {
            Some(k) if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let prop = self.ctx.arena.get_property_decl(member_node.unwrap());
                let name = prop.and_then(|p| self.get_member_name_text(p.name));
                let node = prop
                    .map(|p| p.name)
                    .filter(|idx| !idx.is_none())
                    .unwrap_or(member_idx);
                (name, node)
            }
            Some(k) if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.ctx.arena.get_method_decl(member_node.unwrap());
                let name = method.and_then(|m| self.get_member_name_text(m.name));
                let node = method
                    .map(|m| m.name)
                    .filter(|idx| !idx.is_none())
                    .unwrap_or(member_idx);
                (name, node)
            }
            Some(k) if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.ctx.arena.get_accessor(member_node.unwrap());
                let name = accessor.and_then(|a| self.get_member_name_text(a.name));
                let node = accessor
                    .map(|a| a.name)
                    .filter(|idx| !idx.is_none())
                    .unwrap_or(member_idx);
                (name, node)
            }
            _ => return,
        };

        if let Some(name) = name {
            self.error_at_node_msg(error_node, diagnostic_codes::DUPLICATE_IDENTIFIER, &[&name]);
        }
    }

    /// Check for duplicate property/method names in class members (TS2300, TS2393).
    /// TypeScript reports:
    /// - TS2300 "Duplicate identifier 'X'." for duplicate properties
    /// - TS2393 "Duplicate function implementation." for multiple method implementations
    ///
    /// NOTE: Method overloads (signatures + implementation) are allowed:
    ///   foo(x: number): void;    // overload signature
    ///   foo(x: string): void;    // overload signature  
    ///   foo(x: any) { }          // implementation - this is valid!
    pub(crate) fn check_duplicate_class_members(&mut self, members: &[NodeIndex]) {
        use crate::types::diagnostics::diagnostic_codes;
        use rustc_hash::FxHashMap;

        // Track member names with their info
        struct MemberInfo {
            indices: Vec<NodeIndex>,
            is_property: Vec<bool>, // true for PROPERTY_DECLARATION, false for METHOD_DECLARATION
            method_has_body: Vec<bool>, // only valid when is_property is false
            is_static: Vec<bool>,
        }

        let mut seen_names: FxHashMap<String, MemberInfo> = FxHashMap::default();

        // Track accessor occurrences for duplicate detection
        // Key: "get:name" or "set:name" (with "static:" prefix for static members)
        let mut seen_accessors: FxHashMap<String, Vec<NodeIndex>> = FxHashMap::default();

        // Track accessor plain names (without get/set prefix) for cross-checking
        // against properties/methods. Key: "name" or "static:name"
        let mut accessor_plain_names: FxHashMap<String, Vec<NodeIndex>> = FxHashMap::default();

        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Get the member name and type info
            let (name, is_property, method_has_body, is_static) = match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                    .ctx
                    .arena
                    .get_property_decl(member_node)
                    .and_then(|prop| {
                        let is_static = self.has_static_modifier(&prop.modifiers);
                        self.get_member_name_text(prop.name)
                            .map(|n| (n, true, false, is_static))
                    })
                    .unwrap_or_default(),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(member_node)
                    .and_then(|method| {
                        let has_body = !method.body.is_none();
                        let is_static = self.has_static_modifier(&method.modifiers);
                        self.get_member_name_text(method.name)
                            .map(|n| (n, false, has_body, is_static))
                    })
                    .unwrap_or_default(),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    // Track accessors for duplicate detection (getter/setter pairs are allowed,
                    // but duplicate getters or duplicate setters are not)
                    if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                        if let Some(name) = self.get_member_name_text(accessor.name) {
                            let is_static = self.has_static_modifier(&accessor.modifiers);
                            let kind = if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                                "get"
                            } else {
                                "set"
                            };
                            let key = if is_static {
                                format!("static:{}:{}", kind, name)
                            } else {
                                format!("{}:{}", kind, name)
                            };
                            seen_accessors.entry(key).or_default().push(member_idx);

                            // Also track plain name for cross-checking with properties/methods
                            let plain_key = if is_static {
                                format!("static:{}", name)
                            } else {
                                name.clone()
                            };
                            accessor_plain_names
                                .entry(plain_key)
                                .or_default()
                                .push(member_idx);
                        }
                    }
                    continue;
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    // Constructors have separate duplicate checking (TS2392)
                    continue;
                }
                _ => continue,
            };

            if name.is_empty() {
                continue;
            }

            // Create a key that considers static vs instance members separately
            let key = if is_static {
                format!("static:{}", name)
            } else {
                name.clone()
            };

            let info = seen_names.entry(key).or_insert(MemberInfo {
                indices: Vec::new(),
                is_property: Vec::new(),
                method_has_body: Vec::new(),
                is_static: Vec::new(),
            });
            info.indices.push(member_idx);
            info.is_property.push(is_property);
            info.method_has_body.push(method_has_body);
            info.is_static.push(is_static);
        }

        // Report errors for duplicates
        for (_key, info) in &seen_names {
            if info.indices.len() <= 1 {
                continue;
            }

            // Count types of members
            let property_count = info.is_property.iter().filter(|&&p| p).count();
            let method_count = info.is_property.len() - property_count;
            let method_impl_count = info
                .is_property
                .iter()
                .zip(info.method_has_body.iter())
                .filter(|(is_prop, has_body)| !**is_prop && **has_body)
                .count();

            // Case 1: Multiple properties with same name (no methods) -> TS2300 for subsequent only
            // Case 2: Property mixed with methods:
            //   - If property comes first: TS2300 for ALL (both property and method)
            //   - If method comes first: TS2300 for subsequent (only property)
            // Case 3: Multiple method implementations -> TS2393 for implementations only
            // Case 4: Method overloads (signatures + 1 implementation) -> Valid, no error

            if property_count > 0 && method_count == 0 {
                // All properties: only report subsequent declarations
                for &idx in info.indices.iter().skip(1) {
                    self.report_duplicate_class_member_ts2300(idx);
                }
            } else if property_count > 0 && method_count > 0 {
                // Mixed properties and methods: check if first is property
                let first_is_property = info.is_property.first().copied().unwrap_or(false);
                let skip_count = if first_is_property { 0 } else { 1 };

                for &idx in info.indices.iter().skip(skip_count) {
                    self.report_duplicate_class_member_ts2300(idx);
                }
            } else if method_impl_count > 1 {
                // Multiple method implementations -> TS2393 for implementations only
                for ((&idx, &is_prop), &has_body) in info
                    .indices
                    .iter()
                    .zip(info.is_property.iter())
                    .zip(info.method_has_body.iter())
                {
                    if !is_prop && has_body {
                        let member_node = self.ctx.arena.get(idx);
                        let error_node = member_node
                            .and_then(|n| self.ctx.arena.get_method_decl(n))
                            .map(|m| m.name)
                            .filter(|idx| !idx.is_none())
                            .unwrap_or(idx);
                        self.error_at_node(
                            error_node,
                            "Duplicate function implementation.",
                            diagnostic_codes::DUPLICATE_FUNCTION_IMPLEMENTATION,
                        );
                    }
                }
            }
            // else: Only method signatures + at most 1 implementation = valid overloads
        }

        // Report TS2300 for duplicate accessors (e.g., two getters or two setters with same name)
        for (_key, indices) in &seen_accessors {
            if indices.len() <= 1 {
                continue;
            }
            // Emit errors for ALL duplicate declarations (matching tsc behavior)
            for &idx in indices.iter() {
                self.report_duplicate_class_member_ts2300(idx);
            }
        }

        // Cross-check accessors against properties/methods for TS2300
        // A field+getter, field+setter, or method+getter/setter conflict is TS2300
        for (key, accessor_indices) in &accessor_plain_names {
            if seen_names.contains_key(key) {
                // Report TS2300 on the accessor declarations
                for &idx in accessor_indices {
                    self.report_duplicate_class_member_ts2300(idx);
                }
            }
        }
    }

    /// Check for invalid 'async' modifier on class, enum, interface, or module declarations.
    /// TS1042: 'async' modifier cannot be used here.
    ///
    /// In TypeScript, the `async` modifier is only valid on function declarations,
    /// method declarations, and arrow functions. When placed on class, enum, interface,
    /// or namespace/module declarations, TS1042 is reported.
    ///
    /// This matches tsc's checker behavior (checkGrammarModifiers) rather than
    /// emitting the error at parse time.
    pub(crate) fn check_async_modifier_on_declaration(
        &mut self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) {
        use crate::types::diagnostics::diagnostic_codes;

        if let Some(async_mod_idx) = self.find_async_modifier(modifiers) {
            self.error_at_node(
                async_mod_idx,
                "'async' modifier cannot be used here.",
                diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
            );
        }
    }

    pub(crate) fn lookup_member_access_in_class(
        &self,
        class_idx: NodeIndex,
        name: &str,
        is_static: bool,
    ) -> MemberLookup {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return MemberLookup::NotFound;
        };
        let Some(class) = self.ctx.arena.get_class(node) else {
            return MemberLookup::NotFound;
        };

        let mut accessor_access: Option<MemberAccessLevel> = None;

        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&prop.modifiers) != is_static {
                        continue;
                    }
                    let Some(prop_name) = self.get_property_name(prop.name) else {
                        continue;
                    };
                    if prop_name == name {
                        let access_level = if self.is_private_identifier_name(prop.name) {
                            Some(MemberAccessLevel::Private)
                        } else {
                            self.member_access_level_from_modifiers(&prop.modifiers)
                        };
                        return match access_level {
                            Some(level) => MemberLookup::Restricted(level),
                            None => MemberLookup::Public,
                        };
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&method.modifiers) != is_static {
                        continue;
                    }
                    let Some(method_name) = self.get_property_name(method.name) else {
                        continue;
                    };
                    if method_name == name {
                        let access_level = if self.is_private_identifier_name(method.name) {
                            Some(MemberAccessLevel::Private)
                        } else {
                            self.member_access_level_from_modifiers(&method.modifiers)
                        };
                        return match access_level {
                            Some(level) => MemberLookup::Restricted(level),
                            None => MemberLookup::Public,
                        };
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&accessor.modifiers) != is_static {
                        continue;
                    }
                    let Some(accessor_name) = self.get_property_name(accessor.name) else {
                        continue;
                    };
                    if accessor_name == name {
                        let access_level = if self.is_private_identifier_name(accessor.name) {
                            Some(MemberAccessLevel::Private)
                        } else {
                            self.member_access_level_from_modifiers(&accessor.modifiers)
                        };
                        // Don't return immediately - a getter/setter pair may have
                        // different visibility. Use the most permissive level (tsc
                        // allows reads when getter is public even if setter is private).
                        match access_level {
                            None => return MemberLookup::Public,
                            Some(level) => {
                                accessor_access = Some(match accessor_access {
                                    // First accessor found
                                    None => level,
                                    // Second accessor: use the more permissive level
                                    Some(MemberAccessLevel::Private) => level,
                                    Some(prev) => prev,
                                });
                            }
                        }
                    }
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    if is_static {
                        continue;
                    }
                    let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                        continue;
                    };
                    if ctor.body.is_none() {
                        continue;
                    }
                    for &param_idx in &ctor.parameters.nodes {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            continue;
                        };
                        if !self.has_parameter_property_modifier(&param.modifiers) {
                            continue;
                        }
                        let Some(param_name) = self.get_property_name(param.name) else {
                            continue;
                        };
                        if param_name == name {
                            return match self.member_access_level_from_modifiers(&param.modifiers) {
                                Some(level) => MemberLookup::Restricted(level),
                                None => MemberLookup::Public,
                            };
                        }
                    }
                }
                _ => {}
            }
        }

        // If we found accessor(s) but didn't early-return Public, return
        // the most permissive access level across getter/setter pair.
        if let Some(level) = accessor_access {
            return MemberLookup::Restricted(level);
        }

        MemberLookup::NotFound
    }

    pub(crate) fn find_member_access_info(
        &self,
        class_idx: NodeIndex,
        name: &str,
        is_static: bool,
    ) -> Option<MemberAccessInfo> {
        use rustc_hash::FxHashSet;

        let mut current = class_idx;
        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();

        while visited.insert(current) {
            match self.lookup_member_access_in_class(current, name, is_static) {
                MemberLookup::Restricted(level) => {
                    return Some(MemberAccessInfo {
                        level,
                        declaring_class_idx: current,
                        declaring_class_name: self.get_class_name_from_decl(current),
                    });
                }
                MemberLookup::Public => return None,
                MemberLookup::NotFound => {
                    let Some(base_idx) = self.get_base_class_idx(current) else {
                        return None;
                    };
                    current = base_idx;
                }
            }
        }

        None
    }

    pub(crate) fn is_method_member_in_class_hierarchy(
        &self,
        class_idx: NodeIndex,
        name: &str,
        is_static: bool,
    ) -> Option<bool> {
        use rustc_hash::FxHashSet;

        let mut current = class_idx;
        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();

        while visited.insert(current) {
            let Some(node) = self.ctx.arena.get(current) else {
                return None;
            };
            let Some(class) = self.ctx.arena.get_class(node) else {
                return None;
            };

            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };

                match member_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                            continue;
                        };
                        if self.has_static_modifier(&method.modifiers) != is_static {
                            continue;
                        }
                        if let Some(method_name) = self.get_property_name(method.name)
                            && method_name == name
                        {
                            return Some(true);
                        }
                    }
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                        let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                            continue;
                        };
                        if self.has_static_modifier(&prop.modifiers) != is_static {
                            continue;
                        }
                        if let Some(prop_name) = self.get_property_name(prop.name)
                            && prop_name == name
                        {
                            return Some(false);
                        }
                    }
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                            continue;
                        };
                        if self.has_static_modifier(&accessor.modifiers) != is_static {
                            continue;
                        }
                        if let Some(accessor_name) = self.get_property_name(accessor.name)
                            && accessor_name == name
                        {
                            // Getters/setters are always accessible via super — they are methods.
                            return Some(true);
                        }
                    }
                    k if k == syntax_kind_ext::CONSTRUCTOR => {
                        if is_static {
                            continue;
                        }
                        let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                            continue;
                        };
                        if ctor.body.is_none() {
                            continue;
                        }
                        for &param_idx in &ctor.parameters.nodes {
                            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                                continue;
                            };
                            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                                continue;
                            };
                            if !self.has_parameter_property_modifier(&param.modifiers) {
                                continue;
                            }
                            if let Some(param_name) = self.get_property_name(param.name)
                                && param_name == name
                            {
                                return Some(false);
                            }
                        }
                    }
                    _ => {}
                }
            }

            let Some(base_idx) = self.get_base_class_idx(current) else {
                return None;
            };
            current = base_idx;
        }

        None
    }

    /// Recursively check a type node for parameter properties in function types.
    /// Function types (like `(x: T) => R` or `new (x: T) => R`) cannot have parameter properties.
    /// Walk a type node and emit TS2304 for unresolved type names inside complex types.
    /// Check type for missing names, but skip top-level TYPE_REFERENCE nodes.
    /// This is used when the caller will separately check TYPE_REFERENCE nodes
    /// to avoid duplicate error emissions.
    pub(crate) fn check_type_for_missing_names_skip_top_level_ref(&mut self, type_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        use tsz_parser::parser::syntax_kind_ext;

        // Skip TYPE_REFERENCE at top level to avoid duplicates
        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            return;
        }

        // For all other types, use the normal check
        self.check_type_for_missing_names(type_idx);
    }

    pub(crate) fn check_type_for_missing_names(&mut self, type_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let _ = self.get_type_from_type_reference(type_idx);
            }
            k if k == syntax_kind_ext::TYPE_QUERY => {
                let _ = self.get_type_from_type_query(type_idx);
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    for &member_idx in &type_lit.members.nodes {
                        self.check_type_member_for_missing_names(member_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    let updates =
                        self.push_missing_name_type_parameters(&func_type.type_parameters);
                    self.check_type_parameters_for_missing_names(&func_type.type_parameters);
                    self.check_duplicate_type_parameters(&func_type.type_parameters);
                    for &param_idx in &func_type.parameters.nodes {
                        self.check_parameter_type_for_missing_names(param_idx);
                    }
                    if !func_type.type_annotation.is_none() {
                        self.check_type_for_missing_names(func_type.type_annotation);
                    }
                    self.pop_type_parameters(updates);
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    self.check_type_for_missing_names(arr.element_type);
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
                    for &elem_idx in &tuple.elements.nodes {
                        self.check_tuple_element_for_missing_names(elem_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE
                || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
            {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.check_type_for_missing_names(wrapped.type_node);
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in &composite.types.nodes {
                        self.check_type_for_missing_names(member_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
                    // Check check_type and extends_type first (infer type params not in scope yet)
                    self.check_type_for_missing_names(cond.check_type);
                    self.check_type_for_missing_names(cond.extends_type);

                    // Collect infer type parameters from extends_type and add them to scope for true_type
                    let infer_params = self.collect_infer_type_parameters(cond.extends_type);
                    let mut param_bindings = Vec::new();
                    for param_name in &infer_params {
                        let atom = self.ctx.types.intern_string(param_name);
                        let type_id = self.ctx.types.intern(tsz_solver::TypeKey::TypeParameter(
                            tsz_solver::TypeParamInfo {
                                name: atom,
                                constraint: None,
                                default: None,
                                is_const: false,
                            },
                        ));
                        let previous = self
                            .ctx
                            .type_parameter_scope
                            .insert(param_name.clone(), type_id);
                        param_bindings.push((param_name.clone(), previous));
                    }

                    // Check true_type with infer type parameters in scope
                    self.check_type_for_missing_names(cond.true_type);

                    // Remove infer type parameters from scope
                    for (name, previous) in param_bindings.into_iter().rev() {
                        if let Some(prev_type) = previous {
                            self.ctx.type_parameter_scope.insert(name, prev_type);
                        } else {
                            self.ctx.type_parameter_scope.remove(&name);
                        }
                    }

                    // Check false_type (infer type params not in scope)
                    self.check_type_for_missing_names(cond.false_type);
                }
            }
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer) = self.ctx.arena.get_infer_type(node) {
                    self.check_type_parameter_node_for_missing_names(infer.type_parameter);
                }
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(op) = self.ctx.arena.get_type_operator(node) {
                    self.check_type_for_missing_names(op.type_node);
                }
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                    self.check_type_for_missing_names(indexed.object_type);
                    self.check_type_for_missing_names(indexed.index_type);
                }
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped) = self.ctx.arena.get_mapped_type(node) {
                    // TS7039: Mapped object type implicitly has an 'any' template type.
                    if self.ctx.no_implicit_any() && mapped.type_node.is_none() {
                        let pos = node.pos;
                        let len = node.end.saturating_sub(node.pos);
                        self.ctx.error(
                            pos,
                            len,
                            "Mapped object type implicitly has an 'any' template type.".to_string(),
                            7039,
                        );
                    }
                    self.check_type_parameter_node_for_missing_names(mapped.type_parameter);
                    let mut param_binding: Option<(String, Option<TypeId>)> = None;
                    if let Some(param_node) = self.ctx.arena.get(mapped.type_parameter)
                        && let Some(param) = self.ctx.arena.get_type_parameter(param_node)
                        && let Some(name_node) = self.ctx.arena.get(param.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        let name = ident.escaped_text.clone();
                        let atom = self.ctx.types.intern_string(&name);
                        let type_id = self.ctx.types.intern(tsz_solver::TypeKey::TypeParameter(
                            tsz_solver::TypeParamInfo {
                                name: atom,
                                constraint: None,
                                default: None,
                                is_const: false,
                            },
                        ));
                        let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
                        param_binding = Some((name, previous));
                    }
                    if !mapped.name_type.is_none() {
                        self.check_type_for_missing_names(mapped.name_type);
                    }
                    if !mapped.type_node.is_none() {
                        self.check_type_for_missing_names(mapped.type_node);
                    } else if self.ctx.no_implicit_any() {
                        // TS7039: Mapped object type implicitly has an 'any' template type
                        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.error_at_node(
                            type_idx,
                            diagnostic_messages::MAPPED_OBJECT_TYPE_IMPLICITLY_HAS_AN_ANY_TEMPLATE_TYPE,
                            diagnostic_codes::MAPPED_OBJECT_TYPE_IMPLICITLY_HAS_AN_ANY_TEMPLATE_TYPE,
                        );
                    }
                    if let Some(ref members) = mapped.members {
                        for &member_idx in &members.nodes {
                            self.check_type_member_for_missing_names(member_idx);
                        }
                    }
                    if let Some((name, previous)) = param_binding {
                        if let Some(prev_type) = previous {
                            self.ctx.type_parameter_scope.insert(name, prev_type);
                        } else {
                            self.ctx.type_parameter_scope.remove(&name);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_PREDICATE => {
                if let Some(pred) = self.ctx.arena.get_type_predicate(node)
                    && !pred.type_node.is_none()
                {
                    self.check_type_for_missing_names(pred.type_node);
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(template) = self.ctx.arena.get_template_literal_type(node) {
                    for &span_idx in &template.template_spans.nodes {
                        let Some(span_node) = self.ctx.arena.get(span_idx) else {
                            continue;
                        };
                        let Some(span) = self.ctx.arena.get_template_span(span_node) else {
                            continue;
                        };
                        self.check_type_for_missing_names(span.expression);
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn push_missing_name_type_parameters(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) -> Vec<(String, Option<TypeId>)> {
        use tsz_solver::{TypeKey, TypeParamInfo};

        let Some(list) = type_parameters else {
            return Vec::new();
        };

        let mut updates = Vec::new();
        for &param_idx in &list.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };
            let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                continue;
            };
            let name = ident.escaped_text.clone();
            let atom = self.ctx.types.intern_string(&name);
            let type_id = self.ctx.types.intern(TypeKey::TypeParameter(TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
                is_const: false,
            }));
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name, previous));
        }

        updates
    }

    pub(crate) fn check_type_member_for_missing_names(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        if let Some(sig) = self.ctx.arena.get_signature(member_node) {
            let updates = self.push_missing_name_type_parameters(&sig.type_parameters);
            self.check_type_parameters_for_missing_names(&sig.type_parameters);
            self.check_duplicate_type_parameters(&sig.type_parameters);
            if let Some(ref params) = sig.parameters {
                for &param_idx in &params.nodes {
                    self.check_parameter_type_for_missing_names(param_idx);
                }
            }
            if !sig.type_annotation.is_none() {
                self.check_type_for_missing_names(sig.type_annotation);
            }
            self.pop_type_parameters(updates);
            return;
        }

        if let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) {
            for &param_idx in &index_sig.parameters.nodes {
                self.check_parameter_type_for_missing_names(param_idx);
            }
            if !index_sig.type_annotation.is_none() {
                self.check_type_for_missing_names(index_sig.type_annotation);
            }
        }
    }

    /// Check a type literal member for parameter properties (call/construct signatures).
    pub(crate) fn check_type_member_for_parameter_properties(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        // Check call signatures and construct signatures for parameter properties
        if node.kind == syntax_kind_ext::CALL_SIGNATURE
            || node.kind == syntax_kind_ext::CONSTRUCT_SIGNATURE
        {
            if let Some(sig) = self.ctx.arena.get_signature(node) {
                if let Some(params) = &sig.parameters {
                    self.check_parameter_properties(&params.nodes);
                    for &param_idx in &params.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            if !param.type_annotation.is_none() {
                                self.check_type_for_parameter_properties(param.type_annotation);
                            }
                            self.maybe_report_implicit_any_parameter(param, false);
                        }
                    }
                }
                // Recursively check the return type
                self.check_type_for_parameter_properties(sig.type_annotation);
            }
        }
        // Check method signatures in type literals
        else if node.kind == syntax_kind_ext::METHOD_SIGNATURE {
            if let Some(sig) = self.ctx.arena.get_signature(node) {
                if let Some(params) = &sig.parameters {
                    self.check_parameter_properties(&params.nodes);
                    for &param_idx in &params.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            if !param.type_annotation.is_none() {
                                self.check_type_for_parameter_properties(param.type_annotation);
                            }
                            self.maybe_report_implicit_any_parameter(param, false);
                        }
                    }
                }
                self.check_type_for_parameter_properties(sig.type_annotation);
                if self.ctx.no_implicit_any()
                    && sig.type_annotation.is_none()
                    && let Some(name) = self.property_name_for_error(sig.name)
                {
                    use crate::types::diagnostics::diagnostic_codes;
                    self.error_at_node_msg(
                        sig.name,
                        diagnostic_codes::WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE,
                        &[&name, "any"],
                    );
                }
            }
        }
        // Check property signatures for implicit any (error 7008)
        else if node.kind == syntax_kind_ext::PROPERTY_SIGNATURE {
            if let Some(sig) = self.ctx.arena.get_signature(node) {
                if !sig.type_annotation.is_none() {
                    self.check_type_for_parameter_properties(sig.type_annotation);
                }
                // Property signature without type annotation implicitly has 'any' type
                // Only emit TS7008 when noImplicitAny is enabled
                if self.ctx.no_implicit_any()
                    && sig.type_annotation.is_none()
                    && let Some(member_name) = self.get_property_name(sig.name)
                {
                    use crate::types::diagnostics::diagnostic_codes;
                    self.error_at_node_msg(
                        sig.name,
                        diagnostic_codes::MEMBER_IMPLICITLY_HAS_AN_TYPE,
                        &[&member_name, "any"],
                    );
                }
            }
        }
        // Check accessors in type literals/interfaces - cannot have body (error 1183)
        else if (node.kind == syntax_kind_ext::GET_ACCESSOR
            || node.kind == syntax_kind_ext::SET_ACCESSOR)
            && let Some(accessor) = self.ctx.arena.get_accessor(node)
        {
            // Accessors in type literals and interfaces cannot have implementations
            if !accessor.body.is_none() {
                use crate::types::diagnostics::diagnostic_codes;
                // Report error on the body
                self.error_at_node(
                    accessor.body,
                    "An implementation cannot be declared in ambient contexts.",
                    diagnostic_codes::AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS,
                );
            }
        }
    }

    /// Check that all method/constructor overload signatures have implementations.
    /// Reports errors 2389, 2390, 2391, 1042.
    pub(crate) fn check_class_member_implementations(&mut self, members: &[NodeIndex]) {
        use crate::types::diagnostics::diagnostic_codes;

        let mut i = 0;
        while i < members.len() {
            let member_idx = members[i];
            let Some(node) = self.ctx.arena.get(member_idx) else {
                i += 1;
                continue;
            };

            match node.kind {
                // TS1042: 'async' modifier cannot be used on getters/setters
                syntax_kind_ext::GET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(node)
                        && self.has_async_modifier(&accessor.modifiers)
                    {
                        self.error_at_node(
                            member_idx,
                            "'async' modifier cannot be used here.",
                            diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
                        );
                    }
                }
                syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(node)
                        && self.has_async_modifier(&accessor.modifiers)
                    {
                        self.error_at_node(
                            member_idx,
                            "'async' modifier cannot be used here.",
                            diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
                        );
                    }
                }
                syntax_kind_ext::CONSTRUCTOR => {
                    if let Some(ctor) = self.ctx.arena.get_constructor(node)
                        && ctor.body.is_none()
                    {
                        // Constructor overload signature - check for implementation
                        let has_impl = self.find_constructor_impl(members, i + 1);
                        if !has_impl {
                            self.error_at_node(
                                member_idx,
                                "Constructor implementation is missing.",
                                diagnostic_codes::CONSTRUCTOR_IMPLEMENTATION_IS_MISSING,
                            );
                        }
                    }
                }
                syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(node) {
                        // Abstract methods don't need implementations (they're meant for derived classes)
                        let is_abstract = self.has_abstract_modifier(&method.modifiers);
                        if method.body.is_none() && !is_abstract {
                            // Method overload signature - check for implementation
                            let method_name = self.get_method_name_from_node(member_idx);
                            if let Some(name) = method_name {
                                let (has_impl, impl_name) =
                                    self.find_method_impl(members, i + 1, &name);
                                if !has_impl {
                                    self.error_at_node(
                                        member_idx,
                                        "Function implementation is missing or not immediately following the declaration.",
                                        diagnostic_codes::FUNCTION_IMPLEMENTATION_IS_MISSING_OR_NOT_IMMEDIATELY_FOLLOWING_THE_DECLARATION
                                    );
                                } else if let Some(actual_name) = impl_name
                                    && actual_name != name
                                {
                                    // Implementation has wrong name
                                    self.error_at_node(
                                        members[i + 1],
                                        &format!(
                                            "Function implementation name must be '{}'.",
                                            name
                                        ),
                                        diagnostic_codes::FUNCTION_IMPLEMENTATION_NAME_MUST_BE,
                                    );
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    pub(crate) fn maybe_report_implicit_any_parameter(
        &mut self,
        param: &tsz_parser::parser::node::ParameterData,
        has_contextual_type: bool,
    ) {
        use crate::types::diagnostics::diagnostic_codes;

        if !self.ctx.no_implicit_any() || has_contextual_type {
            return;
        }
        // Skip parameters that have explicit type annotations
        if !param.type_annotation.is_none() {
            return;
        }
        // Check if parameter has an initializer
        if !param.initializer.is_none() {
            // TypeScript infers type from initializer, EXCEPT for null and undefined
            // Parameters initialized with null/undefined still trigger TS7006
            use tsz_scanner::SyntaxKind;
            let initializer_is_null_or_undefined =
                if let Some(init_node) = self.ctx.arena.get(param.initializer) {
                    init_node.kind == SyntaxKind::NullKeyword as u16
                        || init_node.kind == SyntaxKind::UndefinedKeyword as u16
                } else {
                    false
                };

            // Skip only if initializer is NOT null or undefined
            if !initializer_is_null_or_undefined {
                return;
            }
            // Otherwise continue to emit TS7006 for null/undefined initializers
        }
        if self.is_this_parameter_name(param.name) {
            return;
        }

        // Enhanced destructuring parameter detection
        // Check if the parameter name is a destructuring pattern (object/array binding)
        if let Some(name_node) = self.ctx.arena.get(param.name) {
            let kind = name_node.kind;

            // Direct destructuring patterns
            if kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                // For destructuring parameters, recursively check nested binding elements
                self.emit_implicit_any_parameter_for_pattern(param.name, param.dot_dot_dot_token);
                return;
            }
        }

        // Skip TS7006 for parameters on nodes with parse errors.
        // This prevents cascading "implicitly has any type" errors on malformed AST nodes.
        // The parse error itself should already be emitted (e.g., TS1005, TS2390).
        use tsz_parser::parser::node_flags;
        if let Some(name_node) = self.ctx.arena.get(param.name) {
            let flags = name_node.flags as u32;
            if (flags & node_flags::THIS_NODE_HAS_ERROR) != 0
                || (flags & node_flags::THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR) != 0
            {
                return;
            }
        }

        let param_name = self.parameter_name_for_error(param.name);
        // Skip if the parameter name is empty (parse recovery artifact)
        if param_name.is_empty() {
            return;
        }

        // Rest parameters use TS7019, regular parameters use TS7006
        if param.dot_dot_dot_token {
            self.error_at_node_msg(
                param.name,
                diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE,
                &[&param_name],
            );
        } else {
            self.error_at_node_msg(
                param.name,
                diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE,
                &[&param_name, "any"],
            );
        }
    }

    /// Emit TS7006 errors for nested binding elements in destructuring parameters.
    /// TypeScript reports implicit 'any' for individual bindings in patterns like:
    ///   function foo({ x, y }: any) {}  // no error on x, y with type annotation
    ///   function bar({ x, y }) {}        // errors on x and y
    pub(crate) fn emit_implicit_any_parameter_for_pattern(
        &mut self,
        pattern_idx: NodeIndex,
        is_rest_parameter: bool,
    ) {
        use crate::types::diagnostics::diagnostic_codes;

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };

        let pattern_kind = pattern_node.kind;

        // Handle object binding patterns: { x, y, z }
        if pattern_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            if let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) {
                for &element_idx in &pattern.elements.nodes {
                    if let Some(element_node) = self.ctx.arena.get(element_idx) {
                        // Skip omitted expressions
                        if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                            continue;
                        }

                        if let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node)
                        {
                            // Check if this binding element has an initializer
                            let has_initializer = !binding_elem.initializer.is_none();

                            // If no initializer, report error for implicit any
                            if !has_initializer {
                                // Get the property name (could be identifier or string literal)
                                let binding_name = if !binding_elem.property_name.is_none() {
                                    self.parameter_name_for_error(binding_elem.property_name)
                                } else {
                                    self.parameter_name_for_error(binding_elem.name)
                                };

                                let implicit_type = if is_rest_parameter { "any[]" } else { "any" };
                                self.error_at_node_msg(
                                    binding_elem.name,
                                    diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE,
                                    &[&binding_name, implicit_type],
                                );
                            }

                            // Recursively check nested patterns
                            if let Some(name_node) = self.ctx.arena.get(binding_elem.name) {
                                let name_kind = name_node.kind;
                                if name_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                    || name_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                {
                                    self.emit_implicit_any_parameter_for_pattern(
                                        binding_elem.name,
                                        is_rest_parameter,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        // Handle array binding patterns: [ x, y, z ]
        else if pattern_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            && let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node)
        {
            for &element_idx in &pattern.elements.nodes {
                if let Some(element_node) = self.ctx.arena.get(element_idx) {
                    let element_kind = element_node.kind;

                    // Skip omitted expressions (holes in array patterns)
                    if element_kind == syntax_kind_ext::OMITTED_EXPRESSION {
                        continue;
                    }

                    // Check if this element is a binding element with initializer
                    if let Some(binding_elem) = self.ctx.arena.get_binding_element(element_node) {
                        let has_initializer = !binding_elem.initializer.is_none();

                        if !has_initializer {
                            let binding_name = self.parameter_name_for_error(binding_elem.name);

                            let implicit_type = if is_rest_parameter { "any[]" } else { "any" };
                            self.error_at_node_msg(
                                binding_elem.name,
                                diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE,
                                &[&binding_name, implicit_type],
                            );
                        }

                        // Recursively check nested patterns
                        if let Some(name_node) = self.ctx.arena.get(binding_elem.name) {
                            let name_kind = name_node.kind;
                            if name_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                || name_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            {
                                self.emit_implicit_any_parameter_for_pattern(
                                    binding_elem.name,
                                    is_rest_parameter,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Report an error at a specific node.

    /// Check an expression node for TS1359: await outside async function.
    /// Recursively checks the expression tree for await expressions.
    /// Report an error with context about a related symbol.

    /// Check a class member (property, method, constructor, accessor).
    pub(crate) fn check_class_member(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let mut pushed_this = false;
        if let Some(this_type) = self.class_member_this_type(member_idx) {
            self.ctx.this_type_stack.push(this_type);
            pushed_this = true;
        }

        self.check_class_member_name(member_idx);

        // TS2302: Static members cannot reference class type parameters
        self.check_static_member_for_class_type_param_refs(member_idx);

        match node.kind {
            syntax_kind_ext::PROPERTY_DECLARATION => {
                self.check_property_declaration(member_idx);
            }
            syntax_kind_ext::METHOD_DECLARATION => {
                self.check_method_declaration(member_idx);
            }
            syntax_kind_ext::CONSTRUCTOR => {
                self.check_constructor_declaration(member_idx);
            }
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                self.check_accessor_declaration(member_idx);
            }
            syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => {
                // Static blocks contain statements that must be type-checked
                if let Some(block) = self.ctx.arena.get_block(node) {
                    // Check for unreachable code in the static block
                    self.check_unreachable_code_in_block(&block.statements.nodes);

                    // Check each statement in the block
                    for &stmt_idx in &block.statements.nodes {
                        self.check_statement(stmt_idx);
                    }
                }
            }
            syntax_kind_ext::INDEX_SIGNATURE => {
                // Index signatures are metadata used during type resolution, not members
                // with their own types. They're handled separately by get_index_signatures.
                // Nothing to check here.
            }
            _ => {
                // Other class member types (semicolons, etc.)
                self.get_type_of_node(member_idx);
            }
        }

        if pushed_this {
            self.ctx.this_type_stack.pop();
        }
    }

    /// Check if a type node references class type parameters (TS2302).
    /// Called for static members to ensure they don't reference the enclosing class's type params.
    fn check_type_node_for_class_type_param_refs(
        &mut self,
        type_idx: NodeIndex,
        class_type_param_names: &[String],
    ) {
        use crate::types::diagnostics::diagnostic_codes;

        if type_idx.is_none() || class_type_param_names.is_empty() {
            return;
        }
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node) {
                    // Check if type_name is an identifier matching a class type param
                    if let Some(name_node) = self.ctx.arena.get(type_ref.type_name) {
                        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                            if class_type_param_names.contains(&ident.escaped_text) {
                                self.error_at_node(
                                    type_idx,
                                    "Static members cannot reference class type parameters.",
                                    diagnostic_codes::STATIC_MEMBERS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS,
                                );
                            }
                        }
                    }
                    // Also check type arguments
                    if let Some(type_ref) = self.ctx.arena.get_type_ref(node) {
                        if let Some(ref type_args) = type_ref.type_arguments {
                            for &arg_idx in &type_args.nodes {
                                self.check_type_node_for_class_type_param_refs(
                                    arg_idx,
                                    class_type_param_names,
                                );
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    self.check_type_node_for_class_type_param_refs(
                        arr.element_type,
                        class_type_param_names,
                    );
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
                    for &elem_idx in &tuple.elements.nodes {
                        self.check_type_node_for_class_type_param_refs(
                            elem_idx,
                            class_type_param_names,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in &composite.types.nodes {
                        self.check_type_node_for_class_type_param_refs(
                            member_idx,
                            class_type_param_names,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    // Exclude function type's own type parameters (they shadow class ones)
                    let own_params = self.collect_type_param_names(&func_type.type_parameters);
                    let filtered: Vec<String> = class_type_param_names
                        .iter()
                        .filter(|n| !own_params.contains(n))
                        .cloned()
                        .collect();
                    let names_to_check = if own_params.is_empty() {
                        class_type_param_names
                    } else if filtered.is_empty() {
                        return;
                    } else {
                        &filtered
                    };
                    for &param_idx in &func_type.parameters.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx) {
                            if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                                self.check_type_node_for_class_type_param_refs(
                                    param.type_annotation,
                                    names_to_check,
                                );
                            }
                        }
                    }
                    self.check_type_node_for_class_type_param_refs(
                        func_type.type_annotation,
                        names_to_check,
                    );
                }
            }
            k if k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE
                || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
            {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.check_type_node_for_class_type_param_refs(
                        wrapped.type_node,
                        class_type_param_names,
                    );
                }
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    for &member_idx in &type_lit.members.nodes {
                        self.check_type_member_for_class_type_param_refs(
                            member_idx,
                            class_type_param_names,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    /// Check a type literal member for class type parameter references.
    fn check_type_member_for_class_type_param_refs(
        &mut self,
        member_idx: NodeIndex,
        class_type_param_names: &[String],
    ) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };
        if let Some(sig) = self.ctx.arena.get_signature(node) {
            if let Some(ref params) = sig.parameters {
                for &param_idx in &params.nodes {
                    if let Some(param_node) = self.ctx.arena.get(param_idx) {
                        if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                            self.check_type_node_for_class_type_param_refs(
                                param.type_annotation,
                                class_type_param_names,
                            );
                        }
                    }
                }
            }
            self.check_type_node_for_class_type_param_refs(
                sig.type_annotation,
                class_type_param_names,
            );
        }
    }

    /// Check a static class member for references to class type parameters (TS2302).
    /// Collect type parameter names from a type parameter list.
    fn collect_type_param_names(
        &self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) -> Vec<String> {
        let Some(list) = type_parameters else {
            return Vec::new();
        };
        let mut names = Vec::new();
        for &param_idx in &list.nodes {
            if let Some(node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_type_parameter(node)
                && let Some(name_node) = self.ctx.arena.get(param.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                names.push(ident.escaped_text.clone());
            }
        }
        names
    }

    /// Check a static class member for references to class type parameters (TS2302).
    fn check_static_member_for_class_type_param_refs(&mut self, member_idx: NodeIndex) {
        let class_type_param_names: Vec<String> = self
            .ctx
            .enclosing_class
            .as_ref()
            .map(|c| c.type_param_names.clone())
            .unwrap_or_default();

        if class_type_param_names.is_empty() {
            return;
        }

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.ctx.arena.get_property_decl(node) {
                    if self.has_static_modifier(&prop.modifiers) {
                        self.check_type_node_for_class_type_param_refs(
                            prop.type_annotation,
                            &class_type_param_names,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    if self.has_static_modifier(&method.modifiers) {
                        // Exclude the method's own type parameters (they shadow class ones)
                        let own_params = self.collect_type_param_names(&method.type_parameters);
                        let filtered: Vec<String> = class_type_param_names
                            .iter()
                            .filter(|n| !own_params.contains(n))
                            .cloned()
                            .collect();
                        if filtered.is_empty() {
                            return;
                        }
                        for &param_idx in &method.parameters.nodes {
                            if let Some(param_node) = self.ctx.arena.get(param_idx) {
                                if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                                    self.check_type_node_for_class_type_param_refs(
                                        param.type_annotation,
                                        &filtered,
                                    );
                                }
                            }
                        }
                        self.check_type_node_for_class_type_param_refs(
                            method.type_annotation,
                            &filtered,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                    if self.has_static_modifier(&accessor.modifiers) {
                        // Exclude the accessor's own type parameters (they shadow class ones)
                        let own_params = self.collect_type_param_names(&accessor.type_parameters);
                        let filtered: Vec<String> = class_type_param_names
                            .iter()
                            .filter(|n| !own_params.contains(n))
                            .cloned()
                            .collect();
                        if filtered.is_empty() {
                            return;
                        }
                        for &param_idx in &accessor.parameters.nodes {
                            if let Some(param_node) = self.ctx.arena.get(param_idx) {
                                if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                                    self.check_type_node_for_class_type_param_refs(
                                        param.type_annotation,
                                        &filtered,
                                    );
                                }
                            }
                        }
                        self.check_type_node_for_class_type_param_refs(
                            accessor.type_annotation,
                            &filtered,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    /// Check a property declaration.
    #[tracing::instrument(level = "debug", skip(self), fields(file = %self.ctx.file_name))]
    pub(crate) fn check_property_declaration(&mut self, member_idx: NodeIndex) {
        use crate::types::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(prop) = self.ctx.arena.get_property_decl(node) else {
            return;
        };

        // TS8009/TS8010: Check for TypeScript-only features in JavaScript files
        let is_js_file = self.ctx.file_name.ends_with(".js")
            || self.ctx.file_name.ends_with(".jsx")
            || self.ctx.file_name.ends_with(".mjs")
            || self.ctx.file_name.ends_with(".cjs");
        tracing::debug!(is_js_file, file_name = %self.ctx.file_name, "Checking if JS file for TS8009/TS8010");

        if is_js_file {
            use crate::types::diagnostics::{diagnostic_messages, format_message};

            // TS8009: Modifiers like 'declare' can only be used in TypeScript files
            if self.ctx.has_modifier(
                &prop.modifiers,
                tsz_scanner::SyntaxKind::DeclareKeyword as u16,
            ) {
                let message = format_message(
                    diagnostic_messages::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    &["declare"],
                );
                self.error_at_node(
                    member_idx,
                    &message,
                    diagnostic_codes::THE_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                );
            }

            // TS8010: Type annotations can only be used in TypeScript files
            if !prop.type_annotation.is_none() {
                self.error_at_node(
                    prop.type_annotation,
                    diagnostic_messages::TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    diagnostic_codes::TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                );
            }
        }

        // Track static property initializer context for TS17011
        let is_static = self.has_static_modifier(&prop.modifiers);
        let prev_static_prop_init = self
            .ctx
            .enclosing_class
            .as_ref()
            .map(|c| c.in_static_property_initializer)
            .unwrap_or(false);
        if is_static && !prop.initializer.is_none() {
            if let Some(ref mut class_info) = self.ctx.enclosing_class {
                class_info.in_static_property_initializer = true;
            }
        }

        if !is_static
            && !prop.initializer.is_none()
            && let Some(member_name) = self.get_property_name(prop.name)
        {
            self.check_constructor_param_capture_in_instance_initializer(
                &member_name,
                prop.initializer,
            );
        }

        // TS18045: accessor modifier only allowed when targeting ES2015+
        // Ambient contexts (declare class) are exempt.
        if self.has_accessor_modifier(&prop.modifiers) {
            use crate::context::ScriptTarget;
            let is_es5_or_lower = matches!(
                self.ctx.compiler_options.target,
                ScriptTarget::ES3 | ScriptTarget::ES5
            );
            let in_ambient = self
                .ctx
                .enclosing_class
                .as_ref()
                .map(|c| c.is_declared)
                .unwrap_or(false);
            if is_es5_or_lower && !in_ambient {
                self.error_at_node(
                    member_idx,
                    "Properties with the 'accessor' modifier are only available when targeting ECMAScript 2015 and higher.",
                    diagnostic_codes::PROPERTIES_WITH_THE_ACCESSOR_MODIFIER_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ECMASCRI,
                );
            }
        }

        // Error 1248: A class member cannot have the 'const' keyword
        if let Some(const_mod) = self.get_const_modifier(&prop.modifiers) {
            self.error_at_node(
                const_mod,
                "A class member cannot have the 'const' keyword.",
                diagnostic_codes::A_CLASS_MEMBER_CANNOT_HAVE_THE_KEYWORD,
            );
        }

        // Check for await expressions in the initializer (TS1308)
        if !prop.initializer.is_none() {
            self.check_await_expression(prop.initializer);
        }

        // If property has type annotation and initializer, check type compatibility
        if !prop.type_annotation.is_none() && !prop.initializer.is_none() {
            // Check for undefined type names in nested types (e.g., function type parameters).
            // This matches the variable declaration path in check_variable_declaration.
            self.check_type_for_missing_names_skip_top_level_ref(prop.type_annotation);
            let declared_type = self.get_type_from_type_node(prop.type_annotation);
            let prev_context = self.ctx.contextual_type;
            if declared_type != TypeId::ANY && !self.type_contains_error(declared_type) {
                self.ctx.contextual_type = Some(declared_type);
                // Clear cached type to force recomputation with contextual type.
                // Function expressions may have been typed without contextual info
                // during build_type_environment, missing parameter type inference.
                self.clear_type_cache_recursive(prop.initializer);
            }
            let init_type = self.get_type_of_node(prop.initializer);
            self.ctx.contextual_type = prev_context;

            if declared_type != TypeId::ANY
                && !self.type_contains_error(declared_type)
                && self.should_report_assignability_mismatch(
                    init_type,
                    declared_type,
                    prop.initializer,
                )
            {
                self.error_type_not_assignable_with_reason_at(
                    init_type,
                    declared_type,
                    prop.initializer,
                );
            }
        } else if !prop.initializer.is_none() {
            // Just check the initializer to catch errors within it
            self.get_type_of_node(prop.initializer);
        }

        // Error 2729: Property is used before its initialization
        // Check if initializer references properties declared after this one
        if !prop.initializer.is_none() && !self.has_static_modifier(&prop.modifiers) {
            self.check_property_initialization_order(member_idx, prop.initializer);
        }

        // TS7008: Member implicitly has an 'any' type
        // Report this error when noImplicitAny is enabled and the property has no type annotation
        // AND no initializer (if there's an initializer, TypeScript can infer the type)
        if self.ctx.no_implicit_any()
            && prop.type_annotation.is_none()
            && prop.initializer.is_none()
            && let Some(member_name) = self.get_property_name(prop.name)
        {
            use crate::types::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                prop.name,
                diagnostic_codes::MEMBER_IMPLICITLY_HAS_AN_TYPE,
                &[&member_name, "any"],
            );
        }

        // Cache the inferred type for the property node so DeclarationEmitter can use it
        // Get type: either from annotation or inferred from initializer
        let prop_type = if !prop.type_annotation.is_none() {
            self.get_type_from_type_node(prop.type_annotation)
        } else if !prop.initializer.is_none() {
            self.get_type_of_node(prop.initializer)
        } else {
            TypeId::ANY
        };

        self.ctx.node_types.insert(member_idx.0, prop_type);

        // Restore static property initializer context
        if let Some(ref mut class_info) = self.ctx.enclosing_class {
            class_info.in_static_property_initializer = prev_static_prop_init;
        }
    }

    /// Check a method declaration.
    pub(crate) fn check_method_declaration(&mut self, member_idx: NodeIndex) {
        use crate::types::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(method) = self.ctx.arena.get_method_decl(node) else {
            return;
        };

        // Error 1248: A class member cannot have the 'const' keyword
        if let Some(const_mod) = self.get_const_modifier(&method.modifiers) {
            self.error_at_node(
                const_mod,
                "A class member cannot have the 'const' keyword.",
                diagnostic_codes::A_CLASS_MEMBER_CANNOT_HAVE_THE_KEYWORD,
            );
        }

        // Error 1183: An implementation cannot be declared in ambient contexts
        // Check if we're in a declared class and the method has a body
        if !method.body.is_none()
            && let Some(ref class_info) = self.ctx.enclosing_class
            && class_info.is_declared
        {
            self.error_at_node(
                member_idx,
                "An implementation cannot be declared in ambient contexts.",
                diagnostic_codes::AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS,
            );
        }

        // Push type parameters (like <U> in `fn<U>(id: U)`) before checking types
        let (_type_params, type_param_updates) = self.push_type_parameters(&method.type_parameters);

        // Check for unused type parameters (TS6133)
        self.check_unused_type_params(&method.type_parameters, member_idx);

        // Extract parameter types from contextual type (for object literal methods)
        // This enables shorthand method parameter type inference
        let mut param_types: Vec<Option<TypeId>> = Vec::new();
        if let Some(ctx_type) = self.ctx.contextual_type {
            let ctx_helper = ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                ctx_type,
                self.ctx.compiler_options.no_implicit_any,
            );

            for (i, &param_idx) in method.parameters.nodes.iter().enumerate() {
                if let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                {
                    let type_id = if !param.type_annotation.is_none() {
                        // Use explicit type annotation if present
                        Some(self.get_type_from_type_node(param.type_annotation))
                    } else {
                        // Infer from contextual type
                        ctx_helper.get_parameter_type(i)
                    };
                    param_types.push(type_id);
                }
            }
        }

        let has_type_annotation = !method.type_annotation.is_none();
        let mut return_type = if has_type_annotation {
            self.get_type_from_type_node(method.type_annotation)
        } else {
            TypeId::ANY
        };

        // Cache parameter types for use in method body
        // If we have contextual types, use them; otherwise fall back to type annotations or UNKNOWN
        if param_types.is_empty() {
            self.cache_parameter_types(&method.parameters.nodes, None);
        } else {
            self.cache_parameter_types(&method.parameters.nodes, Some(&param_types));
        }

        // Check for duplicate parameter names (TS2300)
        self.check_duplicate_parameters(&method.parameters);

        // TS1210: Check for 'arguments' parameter name in class methods (strict mode)
        // Classes are implicitly strict mode, and 'arguments' cannot be used as a parameter name
        for &param_idx in &method.parameters.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
            {
                if let Some(name_text) = self.node_text(param.name) {
                    if name_text == "arguments" {
                        self.ctx.error(
                            param_node.pos,
                            param_node.end - param_node.pos,
                            "Code contained in a class is evaluated in JavaScript's strict mode which does not allow this use of 'arguments'. For more information, see https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Strict_mode.".to_string(),
                            1210,
                        );
                    }
                }
            }
        }

        // Check for required parameters following optional parameters (TS1016)
        self.check_parameter_ordering(&method.parameters);

        // Check that rest parameters have array types (TS2370)
        self.check_rest_parameter_types(&method.parameters.nodes);

        // Check that parameter default values are assignable to declared types (TS2322)
        self.check_parameter_initializers(&method.parameters.nodes);

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors, not in methods
        self.check_parameter_properties(&method.parameters.nodes);

        // Check parameter type annotations for parameter properties in function types
        for &param_idx in &method.parameters.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
            {
                if !param.type_annotation.is_none() {
                    self.check_type_for_parameter_properties(param.type_annotation);
                }
                self.maybe_report_implicit_any_parameter(param, false);
            }
        }

        // Check return type annotation for parameter properties in function types
        if !method.type_annotation.is_none() {
            self.check_type_for_parameter_properties(method.type_annotation);
        }

        // Check for async modifier (needed for both abstract and concrete methods)
        let is_async = self.has_async_modifier(&method.modifiers);
        let is_generator = method.asterisk_token;

        // Check method body
        if !method.body.is_none() {
            if !has_type_annotation {
                return_type = self.infer_return_type_from_body(method.body, None);
            }

            // TS2697: Check if async method has access to Promise type
            // DISABLED: Causes too many false positives
            // TODO: Investigate lib loading for Promise detection
            // if is_async && !is_generator && !self.is_promise_global_available() {
            //     use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
            //     self.error_at_node(
            //         method.name,
            //         diagnostic_messages::ASYNC_FUNCTION_MUST_RETURN_PROMISE,
            //         diagnostic_codes::ASYNC_FUNCTION_MUST_RETURN_PROMISE,
            //     );
            // }

            // TS7011 (implicit any return) is only emitted for ambient methods,
            // matching TypeScript's behavior
            // Async methods infer Promise<void>, not 'any', so they should NOT trigger TS7011
            let is_ambient_class = self
                .ctx
                .enclosing_class
                .as_ref()
                .map(|c| c.is_declared)
                .unwrap_or(false);
            let is_ambient_file = self.ctx.file_name.ends_with(".d.ts");

            if (is_ambient_class || is_ambient_file) && !is_async {
                let method_name = self.get_property_name(method.name);
                self.maybe_report_implicit_any_return(
                    method_name,
                    Some(method.name),
                    return_type,
                    has_type_annotation,
                    false,
                    member_idx,
                );
            }

            // For async functions, unwrap Promise<T> to T for return type checking
            // The function body should return T, which gets auto-wrapped in Promise
            let effective_return_type = if is_async && !is_generator {
                self.unwrap_promise_type(return_type).unwrap_or(return_type)
            } else {
                return_type
            };

            self.push_return_type(effective_return_type);

            // Enter async context for await expression checking
            if is_async {
                self.ctx.enter_async_context();
            }

            self.check_statement(method.body);

            // Exit async context
            if is_async {
                self.ctx.exit_async_context();
            }

            let check_return_type =
                self.return_type_for_implicit_return_check(return_type, is_async, is_generator);
            let requires_return = self.requires_return_value(check_return_type);
            let has_return = self.body_has_return_with_value(method.body);
            let falls_through = self.function_body_falls_through(method.body);

            // TS2355: Skip for async methods - they implicitly return Promise<void>
            if has_type_annotation && requires_return && falls_through && !is_async {
                if !has_return {
                    self.error_at_node(
                        method.type_annotation,
                        "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                        diagnostic_codes::A_FUNCTION_WHOSE_DECLARED_TYPE_IS_NEITHER_UNDEFINED_VOID_NOR_ANY_MUST_RETURN_A_V,
                    );
                } else if self.ctx.strict_null_checks() {
                    // TS2366: Only emit when strictNullChecks is enabled, because
                    // without it, undefined is implicitly assignable to any type.
                    use crate::types::diagnostics::diagnostic_messages;
                    self.error_at_node(
                        method.type_annotation,
                        diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                        diagnostic_codes::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                    );
                }
            } else if self.ctx.no_implicit_returns() && has_return && falls_through {
                // TS7030: noImplicitReturns - not all code paths return a value
                use crate::types::diagnostics::diagnostic_messages;
                let error_node = if !method.name.is_none() {
                    method.name
                } else {
                    method.body
                };
                self.error_at_node(
                    error_node,
                    diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                    diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                );
            }

            self.pop_return_type();
        } else {
            // Abstract method or method overload signature
            // Report TS7010 for abstract methods without return type annotation
            // Async methods infer Promise<void>, not 'any', so they should NOT trigger TS7010
            if !is_async {
                let method_name = self.get_property_name(method.name);
                self.maybe_report_implicit_any_return(
                    method_name,
                    Some(method.name),
                    return_type,
                    has_type_annotation,
                    false,
                    member_idx,
                );
            }
        }

        // Check overload compatibility for method implementations
        if !method.body.is_none() {
            self.check_overload_compatibility(member_idx);
        }

        self.pop_type_parameters(type_param_updates);
    }

    /// Check a constructor declaration.
    pub(crate) fn check_constructor_declaration(&mut self, member_idx: NodeIndex) {
        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(ctor) = self.ctx.arena.get_constructor(node) else {
            return;
        };

        // Error 1242: 'abstract' modifier can only appear on a class, method, or property declaration.
        // Constructors cannot be abstract.
        if self.has_abstract_modifier(&ctor.modifiers) {
            self.error_at_node(
                member_idx,
                "'abstract' modifier can only appear on a class, method, or property declaration.",
                diagnostic_codes::ABSTRACT_METHODS_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS,
            );
        }

        // Error 1183: An implementation cannot be declared in ambient contexts
        // Check if we're in a declared class and the constructor has a body
        if !ctor.body.is_none()
            && let Some(ref class_info) = self.ctx.enclosing_class
            && class_info.is_declared
        {
            self.error_at_node(
                member_idx,
                "An implementation cannot be declared in ambient contexts.",
                diagnostic_codes::AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS,
            );
        }

        // Check for parameter properties in constructor overload signatures (error 2369)
        // Parameter properties are only allowed in constructor implementations (with body).
        // This applies to both regular constructors and ambient (declare class) constructors.
        if ctor.body.is_none() {
            self.check_parameter_properties(&ctor.parameters.nodes);
        }

        // Check parameter type annotations for parameter properties in function types
        for &param_idx in &ctor.parameters.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
            {
                if !param.type_annotation.is_none() {
                    self.check_type_for_parameter_properties(param.type_annotation);
                }
                self.maybe_report_implicit_any_parameter(param, false);
            }
        }

        // Constructors don't have explicit return types, but they implicitly return the class instance type
        // Get the class instance type to validate constructor return expressions (TS2322)

        self.cache_parameter_types(&ctor.parameters.nodes, None);

        // Check for duplicate parameter names (TS2300)
        self.check_duplicate_parameters(&ctor.parameters);

        // TS1210: Check for 'arguments' parameter name in constructors (strict mode)
        // Classes are implicitly strict mode, and 'arguments' cannot be used as a parameter name
        for &param_idx in &ctor.parameters.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
            {
                if let Some(name_text) = self.node_text(param.name) {
                    if name_text == "arguments" {
                        self.ctx.error(
                            param_node.pos,
                            param_node.end - param_node.pos,
                            "Code contained in a class is evaluated in JavaScript's strict mode which does not allow this use of 'arguments'. For more information, see https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Strict_mode.".to_string(),
                            1210,
                        );
                    }
                }
            }
        }

        // Check for required parameters following optional parameters (TS1016)
        self.check_parameter_ordering(&ctor.parameters);

        // Check that rest parameters have array types (TS2370)
        self.check_rest_parameter_types(&ctor.parameters.nodes);

        // Check that parameter default values are assignable to declared types (TS2322)
        self.check_parameter_initializers(&ctor.parameters.nodes);

        // Set in_constructor flag for abstract property checks (error 2715)
        if let Some(ref mut class_info) = self.ctx.enclosing_class {
            class_info.in_constructor = true;
            class_info.has_super_call_in_current_constructor = false;
        }

        // Check constructor body
        if !ctor.body.is_none() {
            // Get class instance type for constructor return expression validation
            let instance_type = if let Some(ref class_info) = self.ctx.enclosing_class {
                let class_node = self.ctx.arena.get(class_info.class_idx);
                if let Some(class) = class_node.and_then(|n| self.ctx.arena.get_class(n)) {
                    self.get_class_instance_type(class_info.class_idx, class)
                } else {
                    TypeId::ANY
                }
            } else {
                TypeId::ANY
            };

            // Set expected return type to class instance type
            self.push_return_type(instance_type);
            self.check_statement(ctor.body);
            self.pop_return_type();

            // TS2377: Constructors for derived classes must contain a super() call.
            let requires_super = self
                .ctx
                .enclosing_class
                .as_ref()
                .and_then(|info| self.ctx.arena.get(info.class_idx))
                .and_then(|class_node| self.ctx.arena.get_class(class_node))
                .map(|class| self.class_requires_super_call(class))
                .unwrap_or(false);
            let has_super_call = self
                .ctx
                .enclosing_class
                .as_ref()
                .map(|info| info.has_super_call_in_current_constructor)
                .unwrap_or(false);

            if requires_super && !has_super_call {
                self.error_at_node(
                    member_idx,
                    diagnostic_messages::CONSTRUCTORS_FOR_DERIVED_CLASSES_MUST_CONTAIN_A_SUPER_CALL,
                    diagnostic_codes::CONSTRUCTORS_FOR_DERIVED_CLASSES_MUST_CONTAIN_A_SUPER_CALL,
                );
            }
        }

        // Reset in_constructor flag
        if let Some(ref mut class_info) = self.ctx.enclosing_class {
            class_info.in_constructor = false;
        }

        // Check overload compatibility for constructor implementations
        if !ctor.body.is_none() {
            self.check_overload_compatibility(member_idx);
        }
    }

    /// Check an accessor declaration (getter/setter).
    pub(crate) fn check_accessor_declaration(&mut self, member_idx: NodeIndex) {
        use crate::types::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(accessor) = self.ctx.arena.get_accessor(node) else {
            return;
        };

        // Error 1183: An implementation cannot be declared in ambient contexts
        // Check if we're in a declared class and the accessor has a body
        if !accessor.body.is_none()
            && let Some(ref class_info) = self.ctx.enclosing_class
            && class_info.is_declared
        {
            self.error_at_node(
                member_idx,
                "An implementation cannot be declared in ambient contexts.",
                diagnostic_codes::AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS,
            );
        }

        // Error 1318: An abstract accessor cannot have an implementation
        // Abstract accessors must not have a body
        if !accessor.body.is_none() && self.has_abstract_modifier(&accessor.modifiers) {
            self.error_at_node(
                member_idx,
                "An abstract accessor cannot have an implementation.",
                diagnostic_codes::METHOD_CANNOT_HAVE_AN_IMPLEMENTATION_BECAUSE_IT_IS_MARKED_ABSTRACT,
            );
        }

        let is_getter = node.kind == syntax_kind_ext::GET_ACCESSOR;
        let has_type_annotation = is_getter && !accessor.type_annotation.is_none();
        let mut return_type = if is_getter {
            if has_type_annotation {
                self.get_type_from_type_node(accessor.type_annotation)
            } else {
                TypeId::VOID // Default to void for getters without type annotation
            }
        } else {
            TypeId::VOID
        };

        self.cache_parameter_types(&accessor.parameters.nodes, None);

        // Check that parameter default values are assignable to declared types (TS2322)
        self.check_parameter_initializers(&accessor.parameters.nodes);

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors, not in accessors
        self.check_parameter_properties(&accessor.parameters.nodes);

        // Check getter parameters for TS7006 here.
        // Setter parameters are checked in check_setter_parameter() below, which also
        // validates other setter constraints (no initializer, no rest parameter).
        if is_getter {
            for &param_idx in &accessor.parameters.nodes {
                if let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                {
                    self.maybe_report_implicit_any_parameter(param, false);
                }
            }
        }

        // For setters, check parameter constraints (1052, 1053)
        if node.kind == syntax_kind_ext::SET_ACCESSOR {
            // Check if a paired getter exists — if so, setter parameter type is
            // inferred from the getter return type (contextually typed, no TS7006)
            let has_paired_getter = self.setter_has_paired_getter(member_idx, &accessor);
            self.check_setter_parameter(&accessor.parameters.nodes, has_paired_getter);
        }

        // Check accessor body
        if !accessor.body.is_none() {
            if is_getter && !has_type_annotation {
                return_type = self.infer_getter_return_type(accessor.body);
            }

            // TS7010 (implicit any return) is only emitted for ambient accessors,
            // matching TypeScript's behavior
            // Async getters infer Promise<void>, not 'any', so they should NOT trigger TS7010
            if is_getter {
                let is_ambient_class = self
                    .ctx
                    .enclosing_class
                    .as_ref()
                    .map(|c| c.is_declared)
                    .unwrap_or(false);
                let is_ambient_file = self.ctx.file_name.ends_with(".d.ts");
                let is_async = self.has_async_modifier(&accessor.modifiers);

                if (is_ambient_class || is_ambient_file) && !is_async {
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

            self.push_return_type(return_type);

            self.check_statement(accessor.body);
            if is_getter {
                // Check if this is an async getter
                let is_async = self.has_async_modifier(&accessor.modifiers);
                // For async getters, extract the inner type from Promise<T>
                let check_return_type = self.return_type_for_implicit_return_check(
                    return_type,
                    is_async,
                    false, // getters cannot be generators
                );
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
                } else if has_type_annotation
                    && requires_return
                    && falls_through
                    && self.ctx.strict_null_checks()
                {
                    // TS2366: Only emit with strictNullChecks
                    use crate::types::diagnostics::diagnostic_messages;
                    self.error_at_node(
                        accessor.type_annotation,
                        diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                        diagnostic_codes::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                    );
                } else if self.ctx.no_implicit_returns() && has_return && falls_through {
                    // TS7030: noImplicitReturns - not all code paths return a value
                    use crate::types::diagnostics::diagnostic_messages;
                    let error_node = if !accessor.name.is_none() {
                        accessor.name
                    } else {
                        accessor.body
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
        let Some(setter_name) = self.get_property_name(setter_accessor.name) else {
            return false;
        };
        let Some(ref class_info) = self.ctx.enclosing_class else {
            return false;
        };
        for &member_idx in &class_info.member_nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                if let Some(getter) = self.ctx.arena.get_accessor(member_node) {
                    if let Some(getter_name) = self.get_property_name(getter.name) {
                        if getter_name == setter_name {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Promise/async type checking methods moved to promise_checker.rs
    /// The lower_type_with_bindings helper remains here as it requires
    /// access to private resolver methods.

    /// Lower a type node with type parameter bindings.
    ///
    /// This is used to substitute type parameters with concrete types
    /// when extracting type arguments from generic Promise types.
    /// Made pub(crate) so it can be called from promise_checker.rs.
    pub(crate) fn lower_type_with_bindings(
        &self,
        type_node: NodeIndex,
        bindings: Vec<(tsz_common::interner::Atom, TypeId)>,
    ) -> TypeId {
        use tsz_solver::TypeLowering;

        let type_resolver = |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
        let value_resolver = |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
        let lowering = TypeLowering::with_resolvers(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &value_resolver,
        )
        .with_type_param_bindings(bindings);
        lowering.lower_type(type_node)
    }

    // Note: type_contains_any, implicit_any_return_display, should_report_implicit_any_return are in type_checking.rs

    pub(crate) fn maybe_report_implicit_any_return(
        &mut self,
        name: Option<String>,
        name_node: Option<NodeIndex>,
        return_type: TypeId,
        has_type_annotation: bool,
        has_contextual_return: bool,
        fallback_node: NodeIndex,
    ) {
        use crate::types::diagnostics::diagnostic_codes;

        if !self.ctx.no_implicit_any() || has_type_annotation || has_contextual_return {
            return;
        }
        if !self.should_report_implicit_any_return(return_type) {
            return;
        }

        let return_text = self.implicit_any_return_display(return_type);
        if let Some(name) = name {
            self.error_at_node_msg(
                name_node.unwrap_or(fallback_node),
                diagnostic_codes::WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE,
                &[&name, &return_text],
            );
        } else {
            self.error_at_node_msg(
                fallback_node,
                diagnostic_codes::FUNCTION_EXPRESSION_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN,
                &[&return_text],
            );
        }
    }

    /// Check overload compatibility: implementation must be assignable to all overload signatures.
    ///
    /// Reports TS2394 when an implementation signature is not compatible with its overload signatures.
    /// This check ensures that the implementation can handle all valid calls that match the overloads.
    ///
    /// Per TypeScript's variance rules:
    /// - Implementation parameters must be supertypes of overload parameters (contravariant)
    /// - Implementation return type must be subtype of overload return type (covariant)
    /// - Effectively: Implementation <: Overload (implementation is assignable to overload)
    ///
    /// This handles:
    /// - Function declarations
    /// - Method declarations (class methods)
    /// - Constructor declarations
    pub(crate) fn check_overload_compatibility(&mut self, impl_node_idx: NodeIndex) {
        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        // 1. Get the implementation's symbol
        let Some(impl_sym_id) = self.ctx.binder.get_node_symbol(impl_node_idx) else {
            return;
        };

        let Some(symbol) = self.ctx.binder.get_symbol(impl_sym_id) else {
            return;
        };

        // 2. Create TypeLowering instance for manual signature lowering
        // This unblocks overload validation for methods/constructors where get_type_of_node returns ERROR
        let type_resolver = |node_idx: NodeIndex| -> Option<u32> {
            self.ctx.binder.get_node_symbol(node_idx).map(|id| id.0)
        };
        let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            self.ctx.binder.get_node_symbol(node_idx).map(|id| id.0)
        };
        let lowering = tsz_solver::TypeLowering::with_resolvers(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &value_resolver,
        );

        // 3. Get the implementation's type using manual lowering
        // When the implementation has no return type annotation, lower_return_type returns ERROR.
        // Use ANY as the return type override to avoid false TS2394 errors, since `any` is
        // assignable to any return type (matching TypeScript's behavior for untyped implementations).
        let impl_return_override = self.get_impl_return_type_override(impl_node_idx);
        let mut impl_type =
            lowering.lower_signature_from_declaration(impl_node_idx, impl_return_override);
        if impl_type == tsz_solver::TypeId::ERROR {
            // Fall back to get_type_of_node for cases where manual lowering fails
            impl_type = self.get_type_of_node(impl_node_idx);
            if impl_type == tsz_solver::TypeId::ERROR {
                return;
            }
        }

        // Fix up ERROR parameter types in the implementation signature.
        // When implementation params lack type annotations, lowering produces ERROR.
        // Replace with ANY since TypeScript treats untyped impl params as `any`.
        impl_type = self.fix_error_params_in_function(impl_type);

        // 4. Check each overload declaration
        for &decl_idx in &symbol.declarations {
            // Skip the implementation itself
            if decl_idx == impl_node_idx {
                continue;
            }

            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            // 5. Check if this declaration is an overload (has no body)
            // We must handle Functions, Methods, and Constructors
            let is_overload = match decl_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                    .ctx
                    .arena
                    .get_function(decl_node)
                    .map(|f| f.body.is_none())
                    .unwrap_or(false),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(decl_node)
                    .map(|m| m.body.is_none())
                    .unwrap_or(false),
                k if k == syntax_kind_ext::CONSTRUCTOR => self
                    .ctx
                    .arena
                    .get_constructor(decl_node)
                    .map(|c| c.body.is_none())
                    .unwrap_or(false),
                _ => false, // Not a callable declaration we care about
            };

            if !is_overload {
                continue;
            }

            // 6. Get the overload's type using manual lowering
            // For overloads without return type annotations, use VOID (matching tsc behavior).
            let overload_return_override = self.get_overload_return_type_override(decl_idx);
            let mut overload_type =
                lowering.lower_signature_from_declaration(decl_idx, overload_return_override);
            if overload_type == tsz_solver::TypeId::ERROR {
                // Fall back to get_type_of_node for cases where manual lowering fails
                overload_type = self.get_type_of_node(decl_idx);
                if overload_type == tsz_solver::TypeId::ERROR {
                    continue;
                }
            }
            // Fix ERROR param types in overload (untyped params → any)
            overload_type = self.fix_error_params_in_function(overload_type);

            // 7. Check compatibility using tsc's bidirectional return type rule:
            // First check if return types are compatible in EITHER direction,
            // then check parameter-only assignability (ignoring return types).
            // This matches tsc's isImplementationCompatibleWithOverload.
            if !self.is_implementation_compatible_with_overload(impl_type, overload_type) {
                self.error_at_node(
                    decl_idx,
                    diagnostic_messages::THIS_OVERLOAD_SIGNATURE_IS_NOT_COMPATIBLE_WITH_ITS_IMPLEMENTATION_SIGNATURE,
                    diagnostic_codes::THIS_OVERLOAD_SIGNATURE_IS_NOT_COMPATIBLE_WITH_ITS_IMPLEMENTATION_SIGNATURE,
                );
            }
        }
    }

    /// Returns `Some(TypeId::ANY)` if the implementation node has no explicit return type annotation.
    /// Replace ERROR parameter types with ANY in a function type.
    /// Used for overload compatibility: untyped implementation params are treated as `any`.
    fn fix_error_params_in_function(&mut self, type_id: tsz_solver::TypeId) -> tsz_solver::TypeId {
        use tsz_solver::type_queries::get_function_shape;
        let Some(shape) = get_function_shape(self.ctx.types, type_id) else {
            return type_id;
        };
        let has_error = shape
            .params
            .iter()
            .any(|p| p.type_id == tsz_solver::TypeId::ERROR)
            || shape.return_type == tsz_solver::TypeId::ERROR;
        if !has_error {
            return type_id;
        }
        let new_params: Vec<tsz_solver::ParamInfo> = shape
            .params
            .iter()
            .map(|p| tsz_solver::ParamInfo {
                type_id: if p.type_id == tsz_solver::TypeId::ERROR {
                    tsz_solver::TypeId::ANY
                } else {
                    p.type_id
                },
                ..p.clone()
            })
            .collect();
        let new_return = if shape.return_type == tsz_solver::TypeId::ERROR {
            tsz_solver::TypeId::ANY
        } else {
            shape.return_type
        };
        self.ctx.types.function(tsz_solver::FunctionShape {
            type_params: shape.type_params.clone(),
            params: new_params,
            this_type: shape.this_type,
            return_type: new_return,
            type_predicate: shape.type_predicate.clone(),
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        })
    }

    /// This is used for overload compatibility checking: when the implementation omits a return type,
    /// the lowering would produce ERROR, but TypeScript treats it as `any` for compatibility purposes.
    fn get_impl_return_type_override(&self, node_idx: NodeIndex) -> Option<tsz_solver::TypeId> {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return None;
        };
        let has_annotation = match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                .ctx
                .arena
                .get_function(node)
                .map(|f| !f.type_annotation.is_none())
                .unwrap_or(false),
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .map(|m| !m.type_annotation.is_none())
                .unwrap_or(false),
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                // Constructors never have return type annotations
                return None;
            }
            _ => return None,
        };
        if has_annotation {
            None
        } else {
            Some(tsz_solver::TypeId::ANY)
        }
    }

    /// Returns `Some(TypeId::VOID)` if an overload node has no explicit return type annotation.
    /// Overloads without return type annotations default to void (matching tsc behavior).
    fn get_overload_return_type_override(&self, node_idx: NodeIndex) -> Option<tsz_solver::TypeId> {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return None;
        };
        let has_annotation = match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                .ctx
                .arena
                .get_function(node)
                .map(|f| !f.type_annotation.is_none())
                .unwrap_or(false),
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .map(|m| !m.type_annotation.is_none())
                .unwrap_or(false),
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                return None;
            }
            _ => return None,
        };
        if has_annotation {
            None
        } else {
            Some(tsz_solver::TypeId::VOID)
        }
    }

    /// Check overload compatibility using tsc's bidirectional return type rule.
    /// Matches tsc's `isImplementationCompatibleWithOverload`:
    /// 1. Check if return types are compatible in EITHER direction (or target is void)
    /// 2. If so, check parameter-only assignability (with return types ignored)
    fn is_implementation_compatible_with_overload(
        &mut self,
        impl_type: tsz_solver::TypeId,
        overload_type: tsz_solver::TypeId,
    ) -> bool {
        use tsz_solver::type_queries::get_return_type;

        // Get return types of both signatures
        let impl_return = get_return_type(self.ctx.types, impl_type);
        let overload_return = get_return_type(self.ctx.types, overload_type);

        match (impl_return, overload_return) {
            (Some(impl_ret), Some(overload_ret)) => {
                // Bidirectional return type check: either direction must be assignable,
                // or the overload returns void
                let return_compatible = overload_ret == tsz_solver::TypeId::VOID
                    || self.is_assignable_to(overload_ret, impl_ret)
                    || self.is_assignable_to(impl_ret, overload_ret);

                if !return_compatible {
                    return false;
                }

                // Now check parameter-only compatibility by creating versions
                // with ANY return types
                let impl_with_any_ret =
                    self.replace_return_type(impl_type, tsz_solver::TypeId::ANY);
                let overload_with_any_ret =
                    self.replace_return_type(overload_type, tsz_solver::TypeId::ANY);
                self.is_assignable_to(impl_with_any_ret, overload_with_any_ret)
            }
            _ => {
                // If we can't get return types, fall back to direct assignability
                self.is_assignable_to(impl_type, overload_type)
            }
        }
    }

    /// Replace the return type of a function type with the given type.
    /// Returns the original type unchanged if it's not a Function.
    fn replace_return_type(
        &mut self,
        type_id: tsz_solver::TypeId,
        new_return: tsz_solver::TypeId,
    ) -> tsz_solver::TypeId {
        use tsz_solver::type_queries::get_function_shape;
        let Some(shape) = get_function_shape(self.ctx.types, type_id) else {
            return type_id;
        };
        if shape.return_type == new_return {
            return type_id;
        }
        self.ctx.types.function(tsz_solver::FunctionShape {
            type_params: shape.type_params.clone(),
            params: shape.params.clone(),
            this_type: shape.this_type,
            return_type: new_return,
            type_predicate: shape.type_predicate.clone(),
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        })
    }

    /// Check for TS1038: 'declare' modifier in already ambient context
    /// Scans module body for declarations with 'declare' modifiers
    fn check_declare_modifiers_in_ambient_body(&mut self, body_idx: NodeIndex) {
        use crate::types::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            return;
        }

        let Some(block) = self.ctx.arena.get_module_block(body_node) else {
            return;
        };

        let Some(ref statements) = block.statements else {
            return;
        };

        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            // Check different declaration types for 'declare' modifier
            let modifiers = match stmt_node.kind {
                syntax_kind_ext::FUNCTION_DECLARATION => {
                    self.ctx.arena.get_function(stmt_node).map(|f| &f.modifiers)
                }
                syntax_kind_ext::VARIABLE_STATEMENT => {
                    self.ctx.arena.get_variable(stmt_node).map(|v| &v.modifiers)
                }
                syntax_kind_ext::CLASS_DECLARATION => {
                    self.ctx.arena.get_class(stmt_node).map(|c| &c.modifiers)
                }
                syntax_kind_ext::INTERFACE_DECLARATION => self
                    .ctx
                    .arena
                    .get_interface(stmt_node)
                    .map(|i| &i.modifiers),
                syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                    .ctx
                    .arena
                    .get_type_alias(stmt_node)
                    .map(|t| &t.modifiers),
                syntax_kind_ext::ENUM_DECLARATION => {
                    self.ctx.arena.get_enum(stmt_node).map(|e| &e.modifiers)
                }
                syntax_kind_ext::MODULE_DECLARATION => {
                    self.ctx.arena.get_module(stmt_node).map(|m| &m.modifiers)
                }
                _ => None,
            };

            if let Some(mods) = modifiers {
                if let Some(declare_mod) = self.get_declare_modifier(mods) {
                    self.error_at_node(
                        declare_mod,
                        "A 'declare' modifier cannot be used in an already ambient context.",
                        diagnostic_codes::A_DECLARE_MODIFIER_CANNOT_BE_USED_IN_AN_ALREADY_AMBIENT_CONTEXT,
                    );
                }
            }
        }
    }

    /// TS1039: Check for variable initializers in ambient contexts.
    /// This is checked even when we skip full type checking of ambient module bodies.
    fn check_initializers_in_ambient_body(&mut self, body_idx: NodeIndex) {
        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_parser::parser::syntax_kind_ext;

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            return;
        }

        let Some(block) = self.ctx.arena.get_module_block(body_node) else {
            return;
        };

        let Some(ref statements) = block.statements else {
            return;
        };

        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            // Get the actual variable statement - it might be wrapped in an export declaration
            // For example: export var x = 1; is parsed as EXPORT_DECLARATION with export_clause pointing to VARIABLE_STATEMENT
            let var_stmt_node = if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                if let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node) {
                    if export_decl.export_clause.is_none() {
                        continue;
                    }
                    let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause) else {
                        continue;
                    };
                    clause_node
                } else {
                    continue;
                }
            } else if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                stmt_node
            } else {
                continue;
            };

            // Check variable statements for initializers
            if var_stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                if let Some(var_stmt) = self.ctx.arena.get_variable(var_stmt_node) {
                    // var_stmt.declarations.nodes contains VariableDeclarationList nodes
                    // We need to get each list and then iterate its declarations
                    for &list_idx in &var_stmt.declarations.nodes {
                        if let Some(list_node) = self.ctx.arena.get(list_idx)
                            && let Some(decl_list) = self.ctx.arena.get_variable(list_node)
                        {
                            // TypeScript allows `export const x = literal;` in ambient contexts (Constant Ambient Modules)
                            // TS1039 only applies to non-const variables with initializers
                            use tsz_parser::parser::node_flags;
                            let is_const = (list_node.flags & node_flags::CONST as u16) != 0;

                            if !is_const {
                                for &decl_idx in &decl_list.declarations.nodes {
                                    if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                                        && let Some(var_decl) =
                                            self.ctx.arena.get_variable_declaration(decl_node)
                                        && !var_decl.initializer.is_none()
                                    {
                                        // TS1039: Initializers are not allowed in ambient contexts
                                        self.error_at_node(
                                            var_decl.initializer,
                                            diagnostic_messages::INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                                            diagnostic_codes::INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Recursively check nested modules/namespaces
            if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                if let Some(module) = self.ctx.arena.get_module(stmt_node) {
                    if !module.body.is_none() {
                        self.check_initializers_in_ambient_body(module.body);
                    }
                }
            }
        }
    }

    /// Check a break statement for validity.
    /// Check a with statement and emit TS2410.
    /// The 'with' statement is not supported in TypeScript.
    pub(crate) fn check_with_statement(&mut self, stmt_idx: NodeIndex) {
        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        self.error_at_node(
            stmt_idx,
            diagnostic_messages::THE_WITH_STATEMENT_IS_NOT_SUPPORTED_ALL_SYMBOLS_IN_A_WITH_BLOCK_WILL_HAVE_TYPE_A,
            diagnostic_codes::THE_WITH_STATEMENT_IS_NOT_SUPPORTED_ALL_SYMBOLS_IN_A_WITH_BLOCK_WILL_HAVE_TYPE_A,
        );

        if self.is_with_statement_in_strict_mode_context(stmt_idx) {
            self.error_at_node(
                stmt_idx,
                diagnostic_messages::WITH_STATEMENTS_ARE_NOT_ALLOWED_IN_STRICT_MODE,
                diagnostic_codes::WITH_STATEMENTS_ARE_NOT_ALLOWED_IN_STRICT_MODE,
            );
        }
    }

    fn is_with_statement_in_strict_mode_context(&self, stmt_idx: NodeIndex) -> bool {
        if self.ctx.compiler_options.always_strict {
            return true;
        }

        let mut current = stmt_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                || parent_node.kind == syntax_kind_ext::METHOD_DECLARATION
                || parent_node.kind == syntax_kind_ext::GET_ACCESSOR
                || parent_node.kind == syntax_kind_ext::SET_ACCESSOR
                || parent_node.kind == syntax_kind_ext::CONSTRUCTOR
                || parent_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
            {
                return true;
            }

            current = parent_idx;
        }

        false
    }

    /// TS1105: A 'break' statement can only be used within an enclosing iteration statement or switch statement.
    /// TS1107: Jump target cannot cross function boundary.
    /// TS1116: A 'break' statement can only jump to a label of an enclosing statement.
    pub(crate) fn check_break_statement(&mut self, stmt_idx: NodeIndex) {
        use crate::types::diagnostics::diagnostic_codes;

        // Get the label if any
        let label_name = self
            .ctx
            .arena
            .get(stmt_idx)
            .and_then(|node| self.ctx.arena.get_jump_data(node))
            .and_then(|jump_data| {
                if jump_data.label.is_none() {
                    None
                } else {
                    self.get_node_text(jump_data.label)
                }
            });

        if let Some(label) = label_name {
            // Labeled break - look up the label
            if let Some(label_info) = self.find_label(&label) {
                // Check if the label crosses a function boundary
                if label_info.function_depth < self.ctx.function_depth {
                    self.error_at_node(
                        stmt_idx,
                        "Jump target cannot cross function boundary.",
                        diagnostic_codes::JUMP_TARGET_CANNOT_CROSS_FUNCTION_BOUNDARY,
                    );
                }
                // Otherwise, labeled break is valid (can target any label, not just iteration)
            } else {
                // Label not found - emit TS1116
                self.error_at_node(
                    stmt_idx,
                    "A 'break' statement can only jump to a label of an enclosing statement.",
                    diagnostic_codes::A_BREAK_STATEMENT_CAN_ONLY_JUMP_TO_A_LABEL_OF_AN_ENCLOSING_STATEMENT,
                );
            }
        } else {
            // Unlabeled break - must be inside iteration or switch
            if self.ctx.iteration_depth == 0 && self.ctx.switch_depth == 0 {
                // Check if we're inside a function that's inside a loop
                // If so, emit TS1107 (crossing function boundary) instead of TS1105
                if self.ctx.function_depth > 0 && self.ctx.had_outer_loop {
                    self.error_at_node(
                        stmt_idx,
                        "Jump target cannot cross function boundary.",
                        diagnostic_codes::JUMP_TARGET_CANNOT_CROSS_FUNCTION_BOUNDARY,
                    );
                } else {
                    self.error_at_node(
                        stmt_idx,
                        "A 'break' statement can only be used within an enclosing iteration or switch statement.",
                        diagnostic_codes::A_BREAK_STATEMENT_CAN_ONLY_BE_USED_WITHIN_AN_ENCLOSING_ITERATION_OR_SWITCH_STATE,
                    );
                }
            }
        }
    }

    /// Check a continue statement for validity.
    /// TS1104: A 'continue' statement can only be used within an enclosing iteration statement.
    /// TS1107: Jump target cannot cross function boundary.
    /// TS1116: A 'continue' statement can only jump to a label of an enclosing iteration statement.
    pub(crate) fn check_continue_statement(&mut self, stmt_idx: NodeIndex) {
        use crate::types::diagnostics::diagnostic_codes;

        // Get the label if any
        let label_name = self
            .ctx
            .arena
            .get(stmt_idx)
            .and_then(|node| self.ctx.arena.get_jump_data(node))
            .and_then(|jump_data| {
                if jump_data.label.is_none() {
                    None
                } else {
                    self.get_node_text(jump_data.label)
                }
            });

        if let Some(label) = label_name {
            // Labeled continue - look up the label
            if let Some(label_info) = self.find_label(&label) {
                // Check if the label crosses a function boundary
                if label_info.function_depth < self.ctx.function_depth {
                    self.error_at_node(
                        stmt_idx,
                        "Jump target cannot cross function boundary.",
                        diagnostic_codes::JUMP_TARGET_CANNOT_CROSS_FUNCTION_BOUNDARY,
                    );
                } else if !label_info.is_iteration {
                    // Continue can only target iteration labels (label found but not on loop) - TS1115
                    self.error_at_node(
                        stmt_idx,
                        "A 'continue' statement can only target a label of an enclosing iteration statement.",
                        diagnostic_codes::A_CONTINUE_STATEMENT_CAN_ONLY_JUMP_TO_A_LABEL_OF_AN_ENCLOSING_ITERATION_STATEMEN,
                    );
                }
                // Otherwise, labeled continue to iteration label is valid
            } else {
                // Label not found - emit TS1115 (same as when label exists but not on iteration)
                self.error_at_node(
                    stmt_idx,
                    "A 'continue' statement can only target a label of an enclosing iteration statement.",
                    diagnostic_codes::A_CONTINUE_STATEMENT_CAN_ONLY_JUMP_TO_A_LABEL_OF_AN_ENCLOSING_ITERATION_STATEMEN,
                );
            }
        } else {
            // Unlabeled continue - must be inside iteration
            if self.ctx.iteration_depth == 0 {
                // Check if we're inside a function that's inside a loop
                // If so, emit TS1107 (crossing function boundary) instead of TS1104
                if self.ctx.function_depth > 0 && self.ctx.had_outer_loop {
                    self.error_at_node(
                        stmt_idx,
                        "Jump target cannot cross function boundary.",
                        diagnostic_codes::JUMP_TARGET_CANNOT_CROSS_FUNCTION_BOUNDARY,
                    );
                } else {
                    self.error_at_node(
                        stmt_idx,
                        "A 'continue' statement can only be used within an enclosing iteration statement.",
                        diagnostic_codes::A_CONTINUE_STATEMENT_CAN_ONLY_BE_USED_WITHIN_AN_ENCLOSING_ITERATION_STATEMENT,
                    );
                }
            }
        }
    }

    /// Find a label in the label stack by name.
    fn find_label(&self, name: &str) -> Option<&crate::context::LabelInfo> {
        self.ctx
            .label_stack
            .iter()
            .rev()
            .find(|info| info.name == name)
    }
}

/// Implementation of StatementCheckCallbacks for CheckerState.
///
/// This provides the actual implementation of statement checking operations
/// that StatementChecker delegates to. Each callback method calls the
/// corresponding method on CheckerState.
impl<'a> StatementCheckCallbacks for CheckerState<'a> {
    fn arena(&self) -> &tsz_parser::parser::node::NodeArena {
        self.ctx.arena
    }

    fn get_type_of_node(&mut self, idx: NodeIndex) -> TypeId {
        CheckerState::get_type_of_node(self, idx)
    }

    fn check_variable_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_variable_statement(self, stmt_idx)
    }

    fn check_variable_declaration_list(&mut self, list_idx: NodeIndex) {
        CheckerState::check_variable_declaration_list(self, list_idx)
    }

    fn check_variable_declaration(&mut self, decl_idx: NodeIndex) {
        CheckerState::check_variable_declaration(self, decl_idx)
    }

    fn check_return_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_return_statement(self, stmt_idx)
    }

    fn check_unreachable_code_in_block(&mut self, stmts: &[NodeIndex]) {
        CheckerState::check_unreachable_code_in_block(self, stmts)
    }

    fn check_function_implementations(&mut self, stmts: &[NodeIndex]) {
        CheckerState::check_function_implementations(self, stmts)
    }

    fn check_function_declaration(&mut self, func_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return;
        };

        // Delegate to DeclarationChecker for function declaration-specific checks
        // (only for actual function declarations, not expressions/arrows)
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            let mut checker = crate::declarations::DeclarationChecker::new(&mut self.ctx);
            checker.check_function_declaration(func_idx);
        }

        // Re-get node after DeclarationChecker borrows ctx
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return;
        };

        let Some(func) = self.ctx.arena.get_function(node) else {
            return;
        };

        // Error 1183: An implementation cannot be declared in ambient contexts
        // Check if function has 'declare' modifier but also has a body
        // Point error at the body (opening brace) to match tsc
        if !func.body.is_none() && self.has_declare_modifier(&func.modifiers) {
            use crate::types::diagnostics::diagnostic_codes;
            self.error_at_node(
                func.body,
                "An implementation cannot be declared in ambient contexts.",
                diagnostic_codes::AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS,
            );
        }

        // Check for missing Promise global type when function is async (TS2318)
        // TSC emits this at the start of the file when Promise is not available
        // Only check for non-generator async functions (async generators use AsyncGenerator, not Promise)
        if func.is_async && !func.asterisk_token {
            self.check_global_promise_available();
        }

        let (_type_params, type_param_updates) = self.push_type_parameters(&func.type_parameters);

        // Check for unused type parameters (TS6133)
        self.check_unused_type_params(&func.type_parameters, func_idx);

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors
        self.check_parameter_properties(&func.parameters.nodes);

        // Check for duplicate parameter names (TS2300)
        self.check_duplicate_parameters(&func.parameters);

        // Check for required parameters following optional parameters (TS1016)
        self.check_parameter_ordering(&func.parameters);

        // Check that rest parameters have array types (TS2370)
        self.check_rest_parameter_types(&func.parameters.nodes);

        // Check return type annotation for parameter properties in function types
        if !func.type_annotation.is_none() {
            self.check_type_for_parameter_properties(func.type_annotation);
            // Check for undefined type names in return type
            self.check_type_for_missing_names(func.type_annotation);
        }

        // Check parameter type annotations for parameter properties
        for &param_idx in &func.parameters.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
                && !param.type_annotation.is_none()
            {
                self.check_type_for_parameter_properties(param.type_annotation);
                // Check for undefined type names in parameter type
                self.check_type_for_missing_names(param.type_annotation);
            }
        }

        // Extract JSDoc for function declarations to suppress TS7006/TS7010 in JS files
        let func_decl_jsdoc = self.get_jsdoc_for_function(func_idx);

        for &param_idx in &func.parameters.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            // Check if JSDoc provides a @param type for this parameter
            let has_jsdoc_param = if param.type_annotation.is_none() {
                if let Some(ref jsdoc) = func_decl_jsdoc {
                    let pname = self.parameter_name_for_error(param.name);
                    Self::jsdoc_has_param_type(jsdoc, &pname)
                } else {
                    false
                }
            } else {
                false
            };
            self.maybe_report_implicit_any_parameter(param, has_jsdoc_param);
        }

        // Check function body if present
        let has_type_annotation = !func.type_annotation.is_none();
        if !func.body.is_none() {
            let mut return_type = if has_type_annotation {
                self.get_type_of_node(func.type_annotation)
            } else {
                // Use UNKNOWN to enforce strict checking
                TypeId::UNKNOWN
            };

            // Extract this type from explicit `this` parameter EARLY
            // so that infer_return_type_from_body has the correct `this` context
            // (prevents false TS2683 during return type inference)
            let mut pushed_this_type = false;
            if let Some(&first_param) = func.parameters.nodes.first() {
                if let Some(param_node) = self.ctx.arena.get(first_param)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                {
                    // Check if parameter name is "this"
                    // Must check both ThisKeyword and Identifier("this") to match parser behavior
                    let is_this = if let Some(name_node) = self.ctx.arena.get(param.name) {
                        if name_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16 {
                            true
                        } else if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                            ident.escaped_text == "this"
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    if is_this && !param.type_annotation.is_none() {
                        let this_type = self.get_type_from_type_node(param.type_annotation);
                        self.ctx.this_type_stack.push(this_type);
                        pushed_this_type = true;
                    }
                }
            }

            // Cache parameter types from annotations (so for-of binding uses correct types)
            // and then infer for any remaining unknown parameters using contextual information.
            self.cache_parameter_types(&func.parameters.nodes, None);
            self.infer_parameter_types_from_context(&func.parameters.nodes);

            // Check that parameter default values are assignable to declared types (TS2322)
            self.check_parameter_initializers(&func.parameters.nodes);

            // Check for parameter initializers in ambient functions (TS2371)
            self.check_ambient_parameter_initializers(
                &func.parameters.nodes,
                self.has_declare_modifier(&func.modifiers),
            );

            if !has_type_annotation {
                // Suppress definite assignment errors during return type inference.
                // The function body will be checked again below, and that's when
                // we want to emit TS2454 errors to avoid duplicates.
                let prev_suppress = self.ctx.suppress_definite_assignment_errors;
                self.ctx.suppress_definite_assignment_errors = true;
                return_type = self.infer_return_type_from_body(func.body, None);
                self.ctx.suppress_definite_assignment_errors = prev_suppress;
            }

            // TS7010 (implicit any return) is emitted for functions without
            // return type annotations when noImplicitAny is enabled and the return
            // type cannot be inferred (e.g., is 'any' or only returns undefined)
            // Async functions infer Promise<void>, not 'any', so they should NOT trigger TS7010
            // maybe_report_implicit_any_return handles the noImplicitAny check internally
            //
            // JSDoc type annotations suppress TS7010 in JS files.
            // When a function has any JSDoc type info (@param, @returns, @template),
            // tsc considers it as having explicit types and doesn't emit TS7010.
            let has_jsdoc_return = func_decl_jsdoc
                .as_ref()
                .is_some_and(|j| Self::jsdoc_has_type_annotations(j));
            if !func.is_async && !has_jsdoc_return {
                let func_name = self.get_function_name_from_node(func_idx);
                let name_node = if !func.name.is_none() {
                    Some(func.name)
                } else {
                    None
                };
                self.maybe_report_implicit_any_return(
                    func_name,
                    name_node,
                    return_type,
                    has_type_annotation,
                    false,
                    func_idx,
                );
            }

            // TS2705: Async function must return Promise
            // Only check if there's an explicit return type annotation that is NOT Promise
            // Skip this check if the return type is ERROR or the annotation looks like Promise
            // Note: Async generators (async function*) return AsyncGenerator, not Promise
            if func.is_async && !func.asterisk_token && has_type_annotation {
                let should_emit_ts2705 = !self.is_promise_type(return_type)
                    && return_type != TypeId::ERROR
                    && !self.return_type_annotation_looks_like_promise(func.type_annotation);

                if should_emit_ts2705 {
                    use crate::context::ScriptTarget;
                    use crate::types::diagnostics::diagnostic_codes;

                    // For ES5/ES3 targets, emit TS1055 instead of TS2705
                    let is_es5_or_lower = matches!(
                        self.ctx.compiler_options.target,
                        ScriptTarget::ES3 | ScriptTarget::ES5
                    );

                    let type_name = self.format_type(return_type);
                    if is_es5_or_lower {
                        self.error_at_node_msg(
                            func.type_annotation,
                            diagnostic_codes::TYPE_IS_NOT_A_VALID_ASYNC_FUNCTION_RETURN_TYPE_IN_ES5_BECAUSE_IT_DOES_NOT_REFER,
                            &[&type_name],
                        );
                    } else {
                        // TS1064: For ES6+ targets, the return type must be Promise<T>
                        self.error_at_node_msg(
                            func.type_annotation,
                            diagnostic_codes::THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE,
                            &[&type_name],
                        );
                    }
                }
            }

            // Enter async context for await expression checking
            if func.is_async {
                self.ctx.enter_async_context();
            }

            // For generator functions with explicit return type (Generator<Y, R, N> or AsyncGenerator<Y, R, N>),
            // return statements should be checked against TReturn (R), not the full Generator type.
            // This matches TypeScript's behavior where `return x` in a generator checks `x` against TReturn.
            let is_generator = func.asterisk_token;
            let body_return_type = if is_generator && has_type_annotation {
                self.get_generator_return_type_argument(return_type)
                    .unwrap_or(return_type)
            } else if func.is_async && has_type_annotation {
                // Unwrap Promise<T> to T for async function return type checking.
                // The function body returns T, which gets auto-wrapped in a Promise.
                self.unwrap_promise_type(return_type).unwrap_or(return_type)
            } else {
                return_type
            };

            self.push_return_type(body_return_type);
            // Save and reset control flow context (function body creates new context)
            let saved_cf_context = (
                self.ctx.iteration_depth,
                self.ctx.switch_depth,
                self.ctx.label_stack.len(),
                self.ctx.had_outer_loop,
            );
            // If we were in a loop/switch, or already had an outer loop, mark it
            if self.ctx.iteration_depth > 0 || self.ctx.switch_depth > 0 || self.ctx.had_outer_loop
            {
                self.ctx.had_outer_loop = true;
            }
            self.ctx.iteration_depth = 0;
            self.ctx.switch_depth = 0;
            self.ctx.function_depth += 1;
            // Note: we don't truncate label_stack here - labels remain visible
            // but function_depth is used to detect crosses over function boundary
            self.check_statement(func.body);
            // Restore control flow context
            self.ctx.iteration_depth = saved_cf_context.0;
            self.ctx.switch_depth = saved_cf_context.1;
            self.ctx.function_depth -= 1;
            self.ctx.label_stack.truncate(saved_cf_context.2);
            self.ctx.had_outer_loop = saved_cf_context.3;

            // Check for error 2355: function with return type must return a value
            // Only check if there's an explicit return type annotation
            let is_async = func.is_async;
            let is_generator = func.asterisk_token;
            let check_return_type =
                self.return_type_for_implicit_return_check(return_type, is_async, is_generator);
            let requires_return = self.requires_return_value(check_return_type);
            let has_return = self.body_has_return_with_value(func.body);
            let falls_through = self.function_body_falls_through(func.body);

            // TS2355: Skip for async functions - they implicitly return Promise<void>
            if has_type_annotation && requires_return && falls_through && !is_async {
                if !has_return {
                    use crate::types::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        func.type_annotation,
                        "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                        diagnostic_codes::A_FUNCTION_WHOSE_DECLARED_TYPE_IS_NEITHER_UNDEFINED_VOID_NOR_ANY_MUST_RETURN_A_V,
                    );
                } else if self.ctx.strict_null_checks() {
                    // TS2366: Only emit with strictNullChecks
                    use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        func.type_annotation,
                        diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                        diagnostic_codes::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                    );
                }
            } else if self.ctx.no_implicit_returns() && has_return && falls_through {
                // TS7030: noImplicitReturns - not all code paths return a value
                use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
                let error_node = if !func.name.is_none() {
                    func.name
                } else {
                    func.body
                };
                self.error_at_node(
                    error_node,
                    diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                    diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                );
            }

            self.pop_return_type();

            // Exit async context
            if func.is_async {
                self.ctx.exit_async_context();
            }

            if pushed_this_type {
                self.ctx.this_type_stack.pop();
            }
        } else if self.ctx.no_implicit_any() && !has_type_annotation {
            let is_ambient =
                self.has_declare_modifier(&func.modifiers) || self.ctx.file_name.ends_with(".d.ts");
            if is_ambient && let Some(func_name) = self.get_function_name_from_node(func_idx) {
                use crate::types::diagnostics::diagnostic_codes;
                let name_node = if !func.name.is_none() {
                    Some(func.name)
                } else {
                    None
                };
                self.error_at_node_msg(
                    name_node.unwrap_or(func_idx),
                    diagnostic_codes::WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE,
                    &[&func_name, "any"],
                );
            }
        }

        // Check overload compatibility: implementation must be assignable to all overloads
        // This is the function implementation validation (TS2394)
        if !func.body.is_none() {
            // Only check for implementations (functions with bodies)
            self.check_overload_compatibility(func_idx);
        }

        self.pop_type_parameters(type_param_updates);
    }

    fn check_class_declaration(&mut self, class_idx: NodeIndex) {
        // Note: DeclarationChecker::check_class_declaration handles TS2564 (property
        // initialization) but CheckerState::check_class_declaration also handles it
        // more comprehensively (with parameter properties, derived classes, etc.).
        // We skip the DeclarationChecker delegation for classes to avoid duplicate
        // TS2564 emissions. DeclarationChecker::check_class_declaration is tested
        // independently via its own test suite.
        CheckerState::check_class_declaration(self, class_idx)
    }

    fn check_interface_declaration(&mut self, iface_idx: NodeIndex) {
        // Delegate to DeclarationChecker first
        let mut checker = crate::declarations::DeclarationChecker::new(&mut self.ctx);
        checker.check_interface_declaration(iface_idx);

        // Continue with comprehensive interface checking in CheckerState
        CheckerState::check_interface_declaration(self, iface_idx)
    }

    fn check_import_declaration(&mut self, import_idx: NodeIndex) {
        CheckerState::check_import_declaration(self, import_idx)
    }

    fn check_import_equals_declaration(&mut self, import_idx: NodeIndex) {
        CheckerState::check_import_equals_declaration(self, import_idx)
    }

    fn check_export_declaration(&mut self, export_idx: NodeIndex) {
        if let Some(export_decl) = self.ctx.arena.get_export_decl_at(export_idx) {
            // Check module specifier for unresolved modules (TS2792)
            if !export_decl.module_specifier.is_none() {
                self.check_export_module_specifier(export_idx);
            }
            // Check the wrapped declaration
            if !export_decl.export_clause.is_none() {
                self.check_statement(export_decl.export_clause);
            }
        }
    }

    fn check_type_alias_declaration(&mut self, type_alias_idx: NodeIndex) {
        if let Some(node) = self.ctx.arena.get(type_alias_idx) {
            // Delegate to DeclarationChecker first
            let mut checker = crate::declarations::DeclarationChecker::new(&mut self.ctx);
            checker.check_type_alias_declaration(type_alias_idx);

            // Continue with comprehensive type alias checking
            if let Some(type_alias) = self.ctx.arena.get_type_alias(node) {
                // TS2457: Type alias name cannot be 'undefined'
                if let Some(name_node) = self.ctx.arena.get(type_alias.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && ident.escaped_text == "undefined"
                {
                    use crate::types::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        type_alias.name,
                        "Type alias name cannot be 'undefined'.",
                        diagnostic_codes::TYPE_ALIAS_NAME_CANNOT_BE,
                    );
                }
                let (_params, updates) = self.push_type_parameters(&type_alias.type_parameters);
                // Check for unused type parameters (TS6133)
                self.check_unused_type_params(&type_alias.type_parameters, type_alias_idx);
                self.check_type_for_missing_names(type_alias.type_node);
                self.check_type_for_parameter_properties(type_alias.type_node);
                self.pop_type_parameters(updates);
            }
        }
    }
    fn check_enum_duplicate_members(&mut self, enum_idx: NodeIndex) {
        // TS1042: async modifier cannot be used on enum declarations
        if let Some(node) = self.ctx.arena.get(enum_idx)
            && let Some(enum_data) = self.ctx.arena.get_enum(node)
        {
            self.check_async_modifier_on_declaration(&enum_data.modifiers);
        }

        // Delegate to DeclarationChecker first
        let mut checker = crate::declarations::DeclarationChecker::new(&mut self.ctx);
        checker.check_enum_declaration(enum_idx);

        // Continue with enum duplicate members checking
        CheckerState::check_enum_duplicate_members(self, enum_idx)
    }

    fn check_module_declaration(&mut self, module_idx: NodeIndex) {
        if let Some(node) = self.ctx.arena.get(module_idx) {
            // Delegate to DeclarationChecker first
            let mut checker = crate::declarations::DeclarationChecker::new(&mut self.ctx);
            checker.check_module_declaration(module_idx);

            // Check module body and modifiers
            if let Some(module) = self.ctx.arena.get_module(node) {
                // TS1042: async modifier cannot be used on module/namespace declarations
                self.check_async_modifier_on_declaration(&module.modifiers);

                let is_ambient = self.has_declare_modifier(&module.modifiers);
                if !module.body.is_none() && !is_ambient {
                    self.check_module_body(module.body);
                }

                // TS1038: Check for 'declare' modifiers inside ambient module/namespace
                // TS1039: Check for initializers in ambient contexts
                // Even if we don't fully check the body, we still need to emit these errors
                if is_ambient && !module.body.is_none() {
                    self.check_declare_modifiers_in_ambient_body(module.body);
                    self.check_initializers_in_ambient_body(module.body);

                    // TS2300/TS2309: Check for duplicate export assignments even in ambient modules
                    // TS2300: Check for duplicate import aliases even in ambient modules
                    // TS2303: Check for circular import aliases in ambient modules
                    // Need to extract statements from module body
                    if let Some(body_node) = self.ctx.arena.get(module.body)
                        && body_node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_BLOCK
                        && let Some(block) = self.ctx.arena.get_module_block(body_node)
                        && let Some(ref statements) = block.statements
                    {
                        self.check_export_assignment(&statements.nodes);
                        self.check_import_alias_duplicates(&statements.nodes);
                        // Check import equals declarations for circular imports (TS2303)
                        for &stmt_idx in &statements.nodes {
                            if let Some(stmt_node) = self.ctx.arena.get(stmt_idx) {
                                if stmt_node.kind == tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                                    self.check_import_equals_declaration(stmt_idx);
                                }
                            }
                        }
                    }
                }

                // TS2300: Check for duplicate import aliases in non-ambient modules too
                // This handles namespace { import X = ...; import X = ...; }
                if !is_ambient && !module.body.is_none() {
                    if let Some(body_node) = self.ctx.arena.get(module.body)
                        && body_node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_BLOCK
                        && let Some(block) = self.ctx.arena.get_module_block(body_node)
                        && let Some(ref statements) = block.statements
                    {
                        self.check_import_alias_duplicates(&statements.nodes);
                    }
                }
            }
        }
    }

    fn check_await_expression(&mut self, expr_idx: NodeIndex) {
        CheckerState::check_await_expression(self, expr_idx)
    }

    fn check_for_await_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_for_await_statement(self, stmt_idx)
    }

    fn check_truthy_or_falsy(&mut self, node_idx: NodeIndex) {
        CheckerState::check_truthy_or_falsy(self, node_idx)
    }

    fn assign_for_in_of_initializer_types(
        &mut self,
        decl_list_idx: NodeIndex,
        loop_var_type: TypeId,
        is_for_in: bool,
    ) {
        CheckerState::assign_for_in_of_initializer_types(
            self,
            decl_list_idx,
            loop_var_type,
            is_for_in,
        )
    }

    fn for_of_element_type(&mut self, expr_type: TypeId) -> TypeId {
        CheckerState::for_of_element_type(self, expr_type)
    }

    fn check_for_of_iterability(
        &mut self,
        expr_type: TypeId,
        expr_idx: NodeIndex,
        await_modifier: bool,
    ) {
        CheckerState::check_for_of_iterability(self, expr_type, expr_idx, await_modifier);
    }

    fn check_for_in_of_expression_initializer(
        &mut self,
        initializer: NodeIndex,
        element_type: TypeId,
        is_for_of: bool,
    ) {
        CheckerState::check_for_in_of_expression_initializer(
            self,
            initializer,
            element_type,
            is_for_of,
        );
    }

    fn check_statement(&mut self, stmt_idx: NodeIndex) {
        // This calls back to the main check_statement which will delegate to StatementChecker
        CheckerState::check_statement(self, stmt_idx)
    }

    fn check_switch_exhaustiveness(
        &mut self,
        _stmt_idx: NodeIndex,
        expression: NodeIndex,
        case_block: NodeIndex,
        has_default: bool,
    ) {
        // If there's a default clause, the switch is syntactically exhaustive
        if has_default {
            return;
        }

        // Get the discriminant type
        let discriminant_type = self.get_type_of_node(expression);

        // Create a FlowAnalyzer to check exhaustiveness
        let analyzer =
            crate::control_flow::FlowAnalyzer::new(self.ctx.arena, self.ctx.binder, self.ctx.types)
                .with_type_environment(Rc::clone(&self.ctx.type_environment));

        // Create a narrowing context
        let narrowing = tsz_solver::NarrowingContext::new(self.ctx.types);

        // Calculate the "no-match" type (what type the discriminant would have
        // if none of the case clauses match)
        let _no_match_type = analyzer.narrow_by_default_switch_clause(
            discriminant_type,
            expression,
            case_block,
            expression, // target is the discriminant itself
            &narrowing,
        );

        // The no_match_type is used for narrowing within the flow analyzer.
        // The actual "not all code paths return" error (TS2366) should be
        // reported at the FUNCTION level in control flow analysis, not here.
        //
        // This is because:
        // 1. Code after the switch might handle missing cases
        // 2. The return type might accept undefined (e.g., number | undefined)
        // 3. Exhaustiveness must be checked in the context of the entire function
        //
        // The FlowAnalyzer uses no_match_type to correctly narrow types within
        // subsequent code blocks, but the error emission happens elsewhere.
    }

    fn check_switch_case_comparable(
        &mut self,
        switch_type: TypeId,
        case_type: TypeId,
        case_expr: NodeIndex,
    ) {
        // Skip if either type is error/any/unknown to avoid cascade errors
        if switch_type == TypeId::ERROR
            || case_type == TypeId::ERROR
            || switch_type == TypeId::ANY
            || case_type == TypeId::ANY
            || switch_type == TypeId::UNKNOWN
            || case_type == TypeId::UNKNOWN
        {
            return;
        }

        // Use literal type for the case expression if available, since
        // get_type_of_node widens literals (e.g., "c" -> string).
        // tsc's checkExpression preserves literal types for comparability checks.
        let effective_case_type = self
            .literal_type_from_initializer(case_expr)
            .unwrap_or(case_type);

        // Check if the types are comparable (assignable in either direction)
        // tsc uses: isTypeComparableTo(caseType, switchType) which checks both directions
        if !self.are_mutually_assignable(effective_case_type, switch_type) {
            // TS2678: Type 'X' is not comparable to type 'Y'
            if let Some(loc) = self.get_source_location(case_expr) {
                let case_str = self.format_type(effective_case_type);
                let switch_str = self.format_type(switch_type);
                use crate::types::diagnostics::{
                    Diagnostic, DiagnosticCategory, diagnostic_codes, diagnostic_messages,
                    format_message,
                };
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_COMPARABLE_TO_TYPE,
                    &[&case_str, &switch_str],
                );
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::TYPE_IS_NOT_COMPARABLE_TO_TYPE,
                    category: DiagnosticCategory::Error,
                    message_text: message,
                    start: loc.start,
                    length: loc.length(),
                    file: self.ctx.file_name.clone(),
                    related_information: Vec::new(),
                });
            }
        }
    }

    fn check_with_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_with_statement(self, stmt_idx)
    }

    fn check_break_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_break_statement(self, stmt_idx)
    }

    fn check_continue_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_continue_statement(self, stmt_idx)
    }

    fn enter_iteration_statement(&mut self) {
        self.ctx.iteration_depth += 1;
    }

    fn leave_iteration_statement(&mut self) {
        self.ctx.iteration_depth = self.ctx.iteration_depth.saturating_sub(1);
    }

    fn enter_switch_statement(&mut self) {
        self.ctx.switch_depth += 1;
    }

    fn leave_switch_statement(&mut self) {
        self.ctx.switch_depth = self.ctx.switch_depth.saturating_sub(1);
    }

    fn save_and_reset_control_flow_context(&mut self) -> (u32, u32, bool) {
        let saved = (
            self.ctx.iteration_depth,
            self.ctx.switch_depth,
            self.ctx.had_outer_loop,
        );
        // If we were in a loop/switch, or already had an outer loop, mark it
        if self.ctx.iteration_depth > 0 || self.ctx.switch_depth > 0 || self.ctx.had_outer_loop {
            self.ctx.had_outer_loop = true;
        }
        self.ctx.iteration_depth = 0;
        self.ctx.switch_depth = 0;
        saved
    }

    fn restore_control_flow_context(&mut self, saved: (u32, u32, bool)) {
        self.ctx.iteration_depth = saved.0;
        self.ctx.switch_depth = saved.1;
        self.ctx.had_outer_loop = saved.2;
    }

    fn enter_labeled_statement(&mut self, label: String, is_iteration: bool) {
        self.ctx.label_stack.push(crate::context::LabelInfo {
            name: label,
            is_iteration,
            function_depth: self.ctx.function_depth,
        });
    }

    fn leave_labeled_statement(&mut self) {
        self.ctx.label_stack.pop();
    }

    fn get_node_text(&self, idx: NodeIndex) -> Option<String> {
        // For identifiers (like label names), get the identifier data and resolve the text
        let ident = self.ctx.arena.get_identifier_at(idx)?;
        // Use the resolved text from the identifier data
        Some(self.ctx.arena.resolve_identifier_text(ident).to_string())
    }

    fn check_declaration_in_statement_position(&mut self, stmt_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        // TS1156: '{0}' declarations can only be declared inside a block.
        // This fires when an interface or type alias declaration appears as
        // the body of a control flow statement (if/while/for) without braces.
        let decl_kind = match node.kind {
            syntax_kind_ext::INTERFACE_DECLARATION => Some("interface"),
            _ => None,
        };

        if let Some(kind_name) = decl_kind {
            let msg = format!(
                "'{}' declarations can only be declared inside a block.",
                kind_name
            );
            self.error_at_node(
                stmt_idx,
                &msg,
                crate::types::diagnostics::diagnostic_codes::DECLARATIONS_CAN_ONLY_BE_DECLARED_INSIDE_A_BLOCK,
            );
        }
    }
}
