//! Type checking validation: utility methods, AST traversal helpers,
//! member/declaration/private identifier/parameter property validation,
//! destructuring, variable/using declaration validation.
//!
//! Type alias declaration checking and type node validation are in
//! `type_alias_checking.rs`.

use crate::context::TypingRequest;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_conditional_return_branches_against_type(
        &mut self,
        expr_idx: NodeIndex,
        expected_type: TypeId,
        unwrap_async_branch_promises: bool,
    ) {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return;
        };
        let Some(cond) = self.ctx.arena.get_conditional_expr(node) else {
            return;
        };

        let request = crate::context::TypingRequest::with_contextual_type(expected_type);

        for branch_idx in [cond.when_true, cond.when_false] {
            self.invalidate_expression_for_contextual_retry(branch_idx);
            let mut branch_type = self.get_type_of_node_with_request(branch_idx, &request);

            if unwrap_async_branch_promises {
                branch_type = self.unwrap_promise_type(branch_type).unwrap_or(branch_type);
            }

            if branch_type == TypeId::ANY || branch_type == TypeId::ERROR {
                continue;
            }

            // Skip parenthesized expressions for error anchor — tsc points at
            // the inner expression (e.g. `1` in `(1)`), not the outer parens.
            let anchor_idx = self.ctx.arena.skip_parenthesized(branch_idx);

            let _ = self.check_assignable_or_report_at(
                branch_type,
                expected_type,
                anchor_idx,
                anchor_idx,
            );
        }
    }

    // --- AST Traversal Helpers ---

    /// Get modifiers from a declaration node.
    pub(crate) fn get_declaration_modifiers(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> Option<&tsz_parser::parser::NodeList> {
        match node.kind {
            syntax_kind_ext::FUNCTION_DECLARATION => self
                .ctx
                .arena
                .get_function(node)
                .and_then(|f| f.modifiers.as_ref()),
            syntax_kind_ext::CLASS_DECLARATION => self
                .ctx
                .arena
                .get_class(node)
                .and_then(|c| c.modifiers.as_ref()),
            syntax_kind_ext::VARIABLE_STATEMENT => self
                .ctx
                .arena
                .get_variable(node)
                .and_then(|v| v.modifiers.as_ref()),
            syntax_kind_ext::INTERFACE_DECLARATION => self
                .ctx
                .arena
                .get_interface(node)
                .and_then(|i| i.modifiers.as_ref()),
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                .ctx
                .arena
                .get_type_alias(node)
                .and_then(|t| t.modifiers.as_ref()),
            syntax_kind_ext::ENUM_DECLARATION => self
                .ctx
                .arena
                .get_enum(node)
                .and_then(|e| e.modifiers.as_ref()),
            syntax_kind_ext::MODULE_DECLARATION => self
                .ctx
                .arena
                .get_module(node)
                .and_then(|m| m.modifiers.as_ref()),
            syntax_kind_ext::IMPORT_EQUALS_DECLARATION => self
                .ctx
                .arena
                .get_import_decl(node)
                .and_then(|i| i.modifiers.as_ref()),
            _ => None,
        }
    }

    /// Get the name node from a class member node.
    ///
    /// This helper eliminates the repeated pattern of matching member kinds
    /// and extracting their name nodes.
    pub(crate) fn get_member_name_node(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> Option<NodeIndex> {
        match node.kind {
            syntax_kind_ext::PROPERTY_DECLARATION => {
                self.ctx.arena.get_property_decl(node).map(|p| p.name)
            }
            syntax_kind_ext::METHOD_DECLARATION => {
                self.ctx.arena.get_method_decl(node).map(|m| m.name)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                self.ctx.arena.get_accessor(node).map(|a| a.name)
            }
            syntax_kind_ext::PROPERTY_SIGNATURE | syntax_kind_ext::METHOD_SIGNATURE => {
                self.ctx.arena.get_signature(node).map(|s| s.name)
            }
            _ => None,
        }
    }

    /// Get identifier text from a node, if it's an identifier.
    ///
    /// This helper eliminates the repeated pattern of checking for identifier
    /// and extracting `escaped_text`.
    pub(crate) fn get_identifier_text(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> Option<String> {
        self.ctx
            .arena
            .get_identifier(node)
            .map(|ident| ident.escaped_text.clone())
    }

    /// Get identifier text from a node index, if it's an identifier.
    pub(crate) fn get_identifier_text_from_idx(&self, idx: NodeIndex) -> Option<String> {
        self.ctx
            .arena
            .get(idx)
            .and_then(|node| self.get_identifier_text(node))
    }

    /// Generic helper to check if modifiers include a specific keyword.
    ///
    /// This eliminates the duplicated pattern of checking for specific modifier keywords.
    pub(crate) fn has_modifier_kind(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
        kind: SyntaxKind,
    ) -> bool {
        self.ctx.arena.has_modifier(modifiers, kind)
    }

    // --- Member and Declaration Validation ---

    /// Check a class member name for computed property validation and
    /// constructor-name restrictions (TS1341, TS1368).
    ///
    /// This dispatches to `check_computed_property_name` for properties,
    /// methods, and accessors that use computed names, and also checks
    /// that "constructor" is not used as an accessor or generator name.
    pub(crate) fn check_class_member_name(&mut self, member_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let kind = node.kind;

        // Use helper to get member name node
        if let Some(name_idx) = self.get_member_name_node(node) {
            self.check_computed_property_name(name_idx);

            // Check constructor-name restrictions for class members
            if let Some(name_text) = self.get_identifier_text_from_idx(name_idx)
                && name_text == "constructor"
            {
                // TS1341: Class constructor may not be an accessor
                if kind == syntax_kind_ext::GET_ACCESSOR || kind == syntax_kind_ext::SET_ACCESSOR {
                    self.error_at_node(
                        name_idx,
                        diagnostic_messages::CLASS_CONSTRUCTOR_MAY_NOT_BE_AN_ACCESSOR,
                        diagnostic_codes::CLASS_CONSTRUCTOR_MAY_NOT_BE_AN_ACCESSOR,
                    );
                }

                // TS1368: Class constructor may not be a generator
                if kind == syntax_kind_ext::METHOD_DECLARATION {
                    let node = self.ctx.arena.get(member_idx);
                    if let Some(method) = node.and_then(|n| self.ctx.arena.get_method_decl(n))
                        && method.asterisk_token
                    {
                        self.error_at_node(
                            name_idx,
                            diagnostic_messages::CLASS_CONSTRUCTOR_MAY_NOT_BE_A_GENERATOR,
                            diagnostic_codes::CLASS_CONSTRUCTOR_MAY_NOT_BE_A_GENERATOR,
                        );
                    }
                }
            }
        }
    }

    /// Check for duplicate enum member names.
    ///
    /// This function validates that all enum members have unique names.
    /// If duplicates are found, it emits TS2308 errors for each duplicate.
    ///
    /// ## Duplicate Detection:
    /// - Collects all member names into a `HashSet`
    /// - Reports error for each name that appears more than once
    /// - Error TS2308: "Duplicate identifier '{name}'"
    pub(crate) fn check_enum_duplicate_members(&mut self, enum_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        let Some(enum_node) = self.ctx.arena.get(enum_idx) else {
            return;
        };
        let Some(enum_decl) = self.ctx.arena.get_enum(enum_node) else {
            return;
        };

        let mut seen_names: rustc_hash::FxHashMap<String, NodeIndex> =
            rustc_hash::FxHashMap::default();
        for &member_idx in &enum_decl.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                continue;
            };

            self.check_computed_property_name(member.name);

            // Get the member name
            let Some(name_node) = self.ctx.arena.get(member.name) else {
                continue;
            };

            // TS1164: Computed property names are not allowed in enums.
            // Emitted here (checker grammar check) rather than in the parser to avoid
            // position-based dedup conflicts with TS1357 (missing comma between members).
            // tsc only emits TS1164 for non-literal computed names (e.g. [e]).
            // Literal computed names like [2], ["foo"] get TS2452 instead (if numeric).
            // Suppress when parse errors exist — tsc doesn't emit TS1164 alongside
            // parse-level errors like TS1357 on the same enum members.
            if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME && !self.has_parse_errors()
            {
                let is_literal_computed = self
                    .ctx
                    .arena
                    .get_computed_property(name_node)
                    .and_then(|cp| self.ctx.arena.get(cp.expression))
                    .is_some_and(|expr| {
                        expr.kind == SyntaxKind::NumericLiteral as u16
                            || expr.kind == SyntaxKind::StringLiteral as u16
                            || expr.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                    });
                if !is_literal_computed && !self.has_parse_errors() {
                    self.error_at_node(
                        member.name,
                        "Computed property names are not allowed in enums.",
                        diagnostic_codes::COMPUTED_PROPERTY_NAMES_ARE_NOT_ALLOWED_IN_ENUMS,
                    );
                }
            }

            let name_text = if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                ident.escaped_text.clone()
            } else {
                continue;
            };

            // Check for duplicate — report on both first and subsequent occurrences
            match seen_names.entry(name_text.clone()) {
                std::collections::hash_map::Entry::Occupied(entry) => {
                    // Report on the first occurrence (only once)
                    let first_name_idx = *entry.get();
                    if first_name_idx != NodeIndex::NONE {
                        self.error_at_node_msg(
                            first_name_idx,
                            diagnostic_codes::DUPLICATE_IDENTIFIER,
                            &[&name_text],
                        );
                        *entry.into_mut() = NodeIndex::NONE;
                    }
                    // Report on this (duplicate) occurrence
                    self.error_at_node_msg(
                        member.name,
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                        &[&name_text],
                    );
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(member.name);
                }
            }
        }
    }

    // --- Private Identifier Validation ---

    /// Check that a private identifier expression is valid.
    ///
    /// Validates that private field/property access is used correctly:
    /// - The private identifier must be declared in a class
    /// - The object type must be assignable to the declaring class type
    /// - Emits appropriate errors for invalid private identifier usage
    ///
    // ## Parameters:
    /// - `name_idx`: The private identifier node index
    /// - `rhs_type`: The type of the object on which the private identifier is accessed
    ///
    /// ## Validation:
    /// - Resolves private identifier symbols
    /// - Checks if the object type is assignable to the declaring class
    /// - Handles shadowed private members (from derived classes)
    /// - Emits property does not exist errors for invalid access
    pub(crate) fn check_private_identifier_in_expression(
        &mut self,
        name_idx: NodeIndex,
        rhs_type: TypeId,
    ) {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return;
        };
        let property_name = ident.escaped_text.clone();

        let (symbols, saw_class_scope) = self.resolve_private_identifier_symbols(name_idx);
        if symbols.is_empty() {
            if saw_class_scope {
                // Use original rhs_type for error message to preserve nominal identity (e.g., D<string>)
                self.error_property_not_exist_at(&property_name, rhs_type, name_idx);
            }
            return;
        }

        // Mark the private identifier symbol as referenced for unused-variable tracking.
        // `#brand in obj` counts as a read of `#brand` — without this, the private member
        // would be falsely reported as unused (TS6133).
        for &sym_id in &symbols {
            self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
        }

        // Evaluate for type checking but keep original for error messages
        let evaluated_rhs_type = self.evaluate_application_type(rhs_type);
        if evaluated_rhs_type == TypeId::ANY
            || evaluated_rhs_type == TypeId::ERROR
            || evaluated_rhs_type == TypeId::UNKNOWN
        {
            return;
        }

        let declaring_type = match self.private_member_declaring_type(symbols[0]) {
            Some(ty) => ty,
            None => {
                if saw_class_scope {
                    // Use original rhs_type for error message to preserve nominal identity
                    self.error_property_not_exist_at(&property_name, rhs_type, name_idx);
                }
                return;
            }
        };

        if !self.is_assignable_to(evaluated_rhs_type, declaring_type) {
            let shadowed = symbols.iter().skip(1).any(|sym_id| {
                self.private_member_declaring_type(*sym_id)
                    .is_some_and(|ty| self.is_assignable_to(evaluated_rhs_type, ty))
            });
            if shadowed {
                return;
            }

            // Use original rhs_type for error message to preserve nominal identity
            self.error_property_not_exist_at(&property_name, rhs_type, name_idx);
        }
    }

    // --- Type Name Validation ---

    /// Check a parameter's type annotation for missing type names.
    ///
    /// Validates that type references within a parameter's type annotation
    /// can be resolved. This helps catch typos and undefined types.
    ///
    // ## Parameters:
    /// - `param_idx`: The parameter node index to check
    pub(crate) fn check_parameter_type_for_missing_names(&mut self, param_idx: NodeIndex) {
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return;
        };
        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
            return;
        };
        if param.type_annotation.is_some() {
            self.check_type_for_missing_names(param.type_annotation);
        }
    }

    /// Check a tuple element for missing type names.
    ///
    /// Validates that type references within a tuple element can be resolved.
    /// Handles both named tuple members and regular tuple elements.
    ///
    /// ## Parameters:
    /// - `elem_idx`: The tuple element node index to check
    pub(crate) fn check_tuple_element_for_missing_names(&mut self, elem_idx: NodeIndex) {
        let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
            return;
        };
        if elem_node.kind == syntax_kind_ext::NAMED_TUPLE_MEMBER
            && let Some(member) = self.ctx.arena.get_named_tuple_member(elem_node)
        {
            self.check_type_for_missing_names(member.type_node);
        }
    }

    /// Check type parameters for missing type names.
    ///
    /// Iterates through a list of type parameters and validates that
    /// their constraints and defaults reference valid types.
    ///
    /// ## Parameters:
    /// - `type_parameters`: The type parameter list to check
    pub(crate) fn check_type_parameters_for_missing_names(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) {
        self.check_type_parameters_for_missing_names_inner(type_parameters, None);
    }

    /// Like `check_type_parameters_for_missing_names` but with the enclosing
    /// declaration name for TS2716 circular default detection.
    pub(crate) fn check_type_parameters_for_missing_names_with_enclosing(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
        enclosing_name: &str,
    ) {
        self.check_type_parameters_for_missing_names_inner(type_parameters, Some(enclosing_name));
    }

    fn check_type_parameters_for_missing_names_inner(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
        enclosing_name: Option<&str>,
    ) {
        let Some(list) = type_parameters else {
            return;
        };

        // TS2706: Required type parameters may not follow optional type parameters.
        // Track whether we've seen an optional type parameter (one with a default).
        let mut seen_optional = false;
        for &param_idx in &list.nodes {
            let has_default = self
                .ctx
                .arena
                .get(param_idx)
                .and_then(|n| self.ctx.arena.get_type_parameter(n))
                .is_some_and(|p| p.default.is_some());

            if has_default {
                seen_optional = true;
            } else if seen_optional {
                // Required param after optional — emit TS2706
                self.error_at_node_msg(
                    param_idx,
                    crate::diagnostics::diagnostic_codes::REQUIRED_TYPE_PARAMETERS_MAY_NOT_FOLLOW_OPTIONAL_TYPE_PARAMETERS,
                    &[],
                );
            }
        }

        // TS2744: Type parameter defaults can only reference previously declared type parameters.
        // Collect all type parameter names, then check each default for forward references.
        let param_names: Vec<(NodeIndex, String)> = list
            .nodes
            .iter()
            .filter_map(|&idx| {
                let node = self.ctx.arena.get(idx)?;
                let param = self.ctx.arena.get_type_parameter(node)?;
                let name_node = self.ctx.arena.get(param.name)?;
                let ident = self.ctx.arena.get_identifier(name_node)?;
                Some((idx, ident.escaped_text.to_string()))
            })
            .collect();

        for (i, &param_idx) in list.nodes.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };
            if param.default.is_none() {
                continue;
            }

            // Collect names declared before this parameter
            let declared_before: FxHashSet<&str> = param_names[..i]
                .iter()
                .map(|(_, name)| name.as_str())
                .collect();

            // Collect all param names (for cycle detection)
            let all_names: FxHashSet<&str> =
                param_names.iter().map(|(_, name)| name.as_str()).collect();

            // Check if the default references any type parameter not yet declared
            let mut refs_in_default = Vec::new();
            self.collect_type_references_in_type(param.default, &all_names, &mut refs_in_default);

            for (_ref_node, ref_name) in &refs_in_default {
                if !declared_before.contains(ref_name.as_str()) {
                    // This is a forward reference — emit TS2744
                    self.error_at_node_msg(
                        param.default,
                        crate::diagnostics::diagnostic_codes::TYPE_PARAMETER_DEFAULTS_CAN_ONLY_REFERENCE_PREVIOUSLY_DECLARED_TYPE_PARAMETERS,
                        &[],
                    );
                    break;
                }
            }
        }

        // TS2716: Type parameter has a circular default.
        // Detects when a default references the enclosing type itself,
        // e.g., `interface SelfRef<T = SelfRef>`.
        if let Some(enc_name) = enclosing_name {
            let enc_set: FxHashSet<&str> = std::iter::once(enc_name).collect();
            for (i, &param_idx) in list.nodes.iter().enumerate() {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                    continue;
                };
                if param.default.is_none() {
                    continue;
                }
                let mut refs_in_default = Vec::new();
                self.collect_type_references_in_type(param.default, &enc_set, &mut refs_in_default);
                if !refs_in_default.is_empty() {
                    // Get the type parameter name for the error message
                    let param_name = param_names
                        .get(i)
                        .map(|(_, name)| name.as_str())
                        .unwrap_or("T");
                    self.error_at_node_msg(
                        param.default,
                        crate::diagnostics::diagnostic_codes::TYPE_PARAMETER_HAS_A_CIRCULAR_DEFAULT,
                        &[param_name],
                    );
                }
            }
        }

        for &param_idx in &list.nodes {
            self.check_type_parameter_node_for_missing_names(param_idx);
        }
    }

    /// Check for duplicate type parameter names in a type parameter list (TS2300).
    ///
    /// This is used for type parameter lists that are NOT processed through
    /// `push_type_parameters` during the checking pass, such as interface method
    /// signatures and function type expressions.
    pub(crate) fn check_duplicate_type_parameters(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) {
        let Some(list) = type_parameters else {
            return;
        };
        let mut seen = FxHashSet::default();
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
            let name = &ident.escaped_text;
            if !seen.insert(name.clone()) {
                self.error_at_node_msg(
                    param.name,
                    crate::diagnostics::diagnostic_codes::DUPLICATE_IDENTIFIER,
                    &[name],
                );
            }
        }
    }

    /// Check a single type parameter node for missing type names.
    ///
    /// Validates that the constraint and default type of a type parameter
    /// reference valid types.
    ///
    /// ## Parameters:
    /// - `param_idx`: The type parameter node index to check
    pub(crate) fn check_type_parameter_node_for_missing_names(&mut self, param_idx: NodeIndex) {
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return;
        };
        let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
            return;
        };

        // Check if type parameter name is a reserved type name (TS2368)
        if let Some(name_node) = self.ctx.arena.get(param.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            self.check_type_name_is_reserved(param.name, &ident.escaped_text);
        }

        // Check constraint type (missing names + structural validation like TS2313)
        if param.constraint.is_some() {
            self.check_type_for_missing_names(param.constraint);
            self.check_type_node(param.constraint);
        }

        // Check default type
        if param.default.is_some() {
            self.check_type_for_missing_names(param.default);
        }
    }

    /// Check if a type name is a reserved type keyword (TS2368).
    ///
    /// The predefined type keywords (string, number, boolean, etc.) are reserved
    /// and cannot be used as names of user-defined types.
    pub(crate) fn check_type_name_is_reserved(&mut self, name_idx: NodeIndex, name: &str) {
        if matches!(
            name,
            "any"
                | "unknown"
                | "never"
                | "number"
                | "bigint"
                | "boolean"
                | "string"
                | "symbol"
                | "void"
                | "object"
                | "undefined"
        ) {
            self.error_at_node_msg(
                name_idx,
                crate::diagnostics::diagnostic_codes::TYPE_PARAMETER_NAME_CANNOT_BE,
                &[name],
            );
        }
    }

    /// Walk a type AST node and collect type reference names that match a given set.
    /// Used for TS2744/TS2716 checks to find forward/self references in type parameter defaults.
    fn collect_type_references_in_type(
        &self,
        type_idx: NodeIndex,
        names_to_find: &FxHashSet<&str>,
        found: &mut Vec<(NodeIndex, String)>,
    ) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::TYPE_REFERENCE => {
                // Check if the type name is a simple identifier matching one of the names
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node) {
                    if let Some(name_node) = self.ctx.arena.get(type_ref.type_name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                        && names_to_find.contains(ident.escaped_text.as_str())
                    {
                        found.push((type_ref.type_name, ident.escaped_text.to_string()));
                    }
                    // Also check type arguments
                    if let Some(ref type_args) = type_ref.type_arguments {
                        for &arg in &type_args.nodes {
                            self.collect_type_references_in_type(arg, names_to_find, found);
                        }
                    }
                }
            }
            syntax_kind_ext::UNION_TYPE | syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member in &composite.types.nodes {
                        self.collect_type_references_in_type(member, names_to_find, found);
                    }
                }
            }
            syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    self.collect_type_references_in_type(arr.element_type, names_to_find, found);
                }
            }
            syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
                    for &elem in &tuple.elements.nodes {
                        self.collect_type_references_in_type(elem, names_to_find, found);
                    }
                }
            }
            syntax_kind_ext::PARENTHESIZED_TYPE => {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.collect_type_references_in_type(wrapped.type_node, names_to_find, found);
                }
            }
            _ => {
                // For other type nodes, don't recurse deeper for now
            }
        }
    }

    // --- Parameter Properties Validation ---

    /// Check a type node for parameter properties.
    ///
    /// Recursively walks a type node and checks function/constructor types
    /// and type literals for parameter properties (public/private/protected/readonly
    /// parameters in class constructors).
    ///
    /// ## Parameters:
    /// - `type_idx`: The type node index to check
    ///
    /// ## Validation:
    /// - Checks function/constructor types for parameter property modifiers
    /// - Checks type literals for call/construct signatures with parameter properties
    /// - Recursively checks nested types (arrays, unions, intersections, etc.)
    pub(crate) fn check_type_for_parameter_properties(&mut self, type_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        // Check if this is a function type or constructor type
        if node.kind == syntax_kind_ext::FUNCTION_TYPE
            || node.kind == syntax_kind_ext::CONSTRUCTOR_TYPE
        {
            if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                // Check each parameter for parameter property modifiers
                self.check_strict_mode_reserved_parameter_names(
                    &func_type.parameters.nodes,
                    type_idx,
                    self.ctx.enclosing_class.is_some(),
                );
                self.check_parameter_properties(&func_type.parameters.nodes);
                for (pi, &param_idx) in func_type.parameters.nodes.iter().enumerate() {
                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                    {
                        if param.type_annotation.is_some() {
                            self.check_type_for_parameter_properties(param.type_annotation);
                        }
                        self.maybe_report_implicit_any_parameter(param, false, pi);
                    }
                }
                // Recursively check the return type
                self.check_type_for_parameter_properties(func_type.type_annotation);
            }
        }
        // Check type literals (object types) for call/construct signatures and duplicate properties
        else if node.kind == syntax_kind_ext::TYPE_LITERAL {
            if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                self.check_type_literal_duplicate_properties(&type_lit.members.nodes);
                self.check_type_literal_overload_optionality(&type_lit.members.nodes);
                for &member_idx in &type_lit.members.nodes {
                    self.check_type_member_for_parameter_properties(member_idx);
                    // TS1170: Computed property in type literal must have literal/unique symbol type
                    if let Some(member_node) = self.ctx.arena.get(member_idx) {
                        if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                            self.check_type_literal_computed_property_name(sig.name);
                        } else if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                            // For get/set accessors in type literals, use TS2464
                            // (general computed property check) matching tsc behavior
                            self.check_computed_property_name(accessor.name);
                        }
                    }
                }
                // TS2411: Check that properties are assignable to index signature types.
                // Type literals don't inherit, so we pass ERROR as the "parent type"
                // and rely on direct member scanning inside the method.
                self.check_index_signature_compatibility(
                    &type_lit.members.nodes,
                    TypeId::ERROR,
                    type_idx,
                );
            }
        }
        // Recursively check array types, union types, intersection types, etc.
        else if node.kind == syntax_kind_ext::ARRAY_TYPE {
            if let Some(arr) = self.ctx.arena.get_array_type(node) {
                self.check_type_for_parameter_properties(arr.element_type);
            }
        } else if node.kind == syntax_kind_ext::UNION_TYPE
            || node.kind == syntax_kind_ext::INTERSECTION_TYPE
        {
            if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                for &type_idx in &composite.types.nodes {
                    self.check_type_for_parameter_properties(type_idx);
                }
            }
        } else if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                && let Some(type_arguments) = &type_ref.type_arguments
            {
                for &arg_idx in &type_arguments.nodes {
                    self.check_type_for_parameter_properties(arg_idx);
                }
            }
        } else if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(paren) = self.ctx.arena.get_wrapped_type(node)
        {
            self.check_type_for_parameter_properties(paren.type_node);
        } else if node.kind == syntax_kind_ext::TYPE_PREDICATE
            && let Some(pred) = self.ctx.arena.get_type_predicate(node)
            && pred.type_node.is_some()
        {
            self.check_type_for_parameter_properties(pred.type_node);
        }
    }

    /// Check for duplicate property names in type literals (TS2300).
    /// e.g. `{ a: string; a: number; }` has duplicate property `a`.
    ///
    /// Method signatures (overloads) with the same name are allowed — only
    /// property signatures are checked for duplicates.
    pub(crate) fn check_type_literal_duplicate_properties(&mut self, members: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext::PROPERTY_SIGNATURE;

        // Track (member_idx, type_annotation, is_syntactic_name) for TS2717 comparison.
        // `is_syntactic_name` is true when the name was determined from syntax alone
        // (literal property name), false when it required evaluating a computed expression.
        let mut seen: rustc_hash::FxHashMap<String, (NodeIndex, NodeIndex, bool)> =
            rustc_hash::FxHashMap::default();

        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Only check property signatures for duplicates.
            // Method signatures with the same name are valid overloads.
            if member_node.kind != PROPERTY_SIGNATURE {
                continue;
            }

            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                continue;
            };
            // Try syntactic name first; fall back to resolved computed property name.
            // This handles cases like `[c0]` where c0 is a const variable — the
            // property name can only be determined by evaluating the expression type.
            let (name, is_syntactic) = if let Some(n) = self.get_member_name(member_idx) {
                (n, true)
            } else if let Some(n) = self.get_property_name_resolved(sig.name) {
                (n, false)
            } else {
                continue;
            };
            let type_ann = sig.type_annotation;

            if let Some(&(prev_idx, prev_type_ann, prev_syntactic)) = seen.get(&name) {
                let name_idx = sig.name;

                // TS2300 "Duplicate identifier" only when both declarations use
                // syntactic (literal) names. Computed property names that resolve
                // to the same value (e.g., `[c0]` and `[c1]` where c0="1", c1=1)
                // get only TS2717, matching tsc behavior.
                if is_syntactic && prev_syntactic {
                    self.error_at_node(
                        name_idx,
                        &format!("Duplicate identifier '{name}'."),
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                    );
                    // Also mark the first occurrence
                    if let Some(prev_node) = self.ctx.arena.get(prev_idx) {
                        let prev_name_idx =
                            if let Some(prev_sig) = self.ctx.arena.get_signature(prev_node) {
                                prev_sig.name
                            } else {
                                prev_idx
                            };
                        self.error_at_node(
                            prev_name_idx,
                            &format!("Duplicate identifier '{name}'."),
                            diagnostic_codes::DUPLICATE_IDENTIFIER,
                        );
                    }
                }

                // TS2717 on the subsequent declaration when types differ.
                // Use display text for the property name to match TSC's
                // declarationNameToString (e.g., "1.0" not "1").
                let first_type = if prev_type_ann.is_some() {
                    self.get_type_from_type_node(prev_type_ann)
                } else {
                    TypeId::ANY
                };
                let this_type = if type_ann.is_some() {
                    self.get_type_from_type_node(type_ann)
                } else {
                    TypeId::ANY
                };
                if !self.type_contains_error(first_type)
                    && !self.type_contains_error(this_type)
                    && first_type != this_type
                {
                    let display_name = self
                        .get_member_name_display_text(name_idx)
                        .unwrap_or_else(|| name.clone());
                    let first_type_str = self.format_type(first_type);
                    let this_type_str = self.format_type(this_type);
                    self.error_at_node_msg(
                        name_idx,
                        diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                        &[&display_name, &first_type_str, &this_type_str],
                    );
                }
            } else {
                seen.insert(name, (member_idx, type_ann, is_syntactic));
            }
        }
    }

    /// TS2386: Check overload optionality agreement in type literal members.
    /// Method signatures with the same name must all be optional or all required.
    pub(crate) fn check_type_literal_overload_optionality(&mut self, members: &[NodeIndex]) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use rustc_hash::FxHashMap;
        use tsz_parser::parser::syntax_kind_ext::METHOD_SIGNATURE;

        // Group method signatures by name
        let mut method_groups: FxHashMap<String, Vec<(NodeIndex, bool)>> = FxHashMap::default();
        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != METHOD_SIGNATURE {
                continue;
            }
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                continue;
            };
            let Some(name) = self.get_member_name(member_idx) else {
                continue;
            };
            method_groups
                .entry(name)
                .or_default()
                .push((member_idx, sig.question_token));
        }

        for group in method_groups.values() {
            if group.len() < 2 {
                continue;
            }
            let first_optional = group[0].1;
            for &(member_idx, optional) in &group[1..] {
                if optional != first_optional {
                    let error_node = self
                        .ctx
                        .arena
                        .get(member_idx)
                        .and_then(|n| self.ctx.arena.get_signature(n))
                        .map(|s| s.name)
                        .unwrap_or(member_idx);
                    self.error_at_node(
                        error_node,
                        diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_OPTIONAL_OR_REQUIRED,
                        diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_OPTIONAL_OR_REQUIRED,
                    );
                }
            }
        }
    }

    // --- Destructuring Validation ---

    /// Check a binding pattern for destructuring validity.
    ///
    /// Validates that destructuring patterns (object/array destructuring) are applied
    /// to valid types and that default values are assignable to their expected types.
    ///
    /// ## Parameters:
    /// - `pattern_idx`: The binding pattern node index to check
    /// - `pattern_type`: The type being destructured
    ///
    /// ## Validation:
    /// - Checks array destructuring target types (TS2461)
    /// - Validates default value assignability for binding elements
    /// - Recursively checks nested binding patterns
    #[allow(dead_code)]
    pub(crate) fn check_binding_pattern(
        &mut self,
        pattern_idx: NodeIndex,
        pattern_type: TypeId,
        check_default_assignability: bool,
    ) {
        self.check_binding_pattern_with_request(
            pattern_idx,
            pattern_type,
            check_default_assignability,
            &TypingRequest::NONE,
        );
    }

    pub(crate) fn check_binding_pattern_with_request(
        &mut self,
        pattern_idx: NodeIndex,
        pattern_type: TypeId,
        check_default_assignability: bool,
        request: &TypingRequest,
    ) {
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };

        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        let elements_len = pattern_data.elements.nodes.len();

        // TS2531/TS2532/TS2533: Destructuring from a possibly-nullish value is an error.
        // TypeScript only emits this directly on the pattern when the pattern is empty.
        // For non-empty patterns, errors are emitted when accessing the individual properties/elements.
        if elements_len == 0 && pattern_type != TypeId::ANY && pattern_type != TypeId::ERROR {
            let (non_nullish_type, nullish_cause) = self.split_nullish_type(pattern_type);
            if let Some(cause) = nullish_cause {
                self.report_nullish_object(pattern_idx, cause, non_nullish_type.is_none());
            }
        }

        // Traverse binding elements
        // Note: Array destructuring iterability (TS2488) is checked by the caller
        // (state_checking.rs) via check_destructuring_iterability before invoking
        // check_binding_pattern, so we do NOT call check_array_destructuring_target_type
        // here to avoid duplicate TS2488 errors.

        let _pattern_kind = pattern_node.kind;

        let is_declarative_pattern = self
            .ctx
            .arena
            .get_extended(pattern_idx)
            .and_then(|ext| self.ctx.arena.get(ext.parent))
            .is_some_and(|parent| {
                matches!(
                    parent.kind,
                    syntax_kind_ext::VARIABLE_DECLARATION | syntax_kind_ext::PARAMETER
                )
            });

        for (i, &element_idx) in pattern_data.elements.nodes.iter().enumerate() {
            if let Some(element_node) = self.ctx.arena.get(element_idx)
                && let Some(element_data) = self.ctx.arena.get_binding_element(element_node)
                && element_data.dot_dot_dot_token
            {
                use tsz_common::diagnostics::diagnostic_codes;

                // TS2566: A rest element cannot have a property name.
                if element_data.property_name.is_some() {
                    self.error_at_node_msg(
                        element_data.name,
                        diagnostic_codes::A_REST_ELEMENT_CANNOT_HAVE_A_PROPERTY_NAME,
                        &[],
                    );
                }

                // TS2700: Rest types may only be created from object types.
                // For object binding patterns with rest, the source type must
                // not be exclusively null/undefined (e.g., `null | undefined`).
                if _pattern_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    && pattern_type != TypeId::ANY
                    && pattern_type != TypeId::ERROR
                {
                    let (non_nullish, _) = self.split_nullish_type(pattern_type);
                    if non_nullish.is_none() {
                        self.error_at_node_msg(
                            pattern_idx,
                            diagnostic_codes::REST_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES,
                            &[],
                        );
                    }
                }

                // TS2462: A rest element must be last in a destructuring pattern.
                // Applies to both array and object binding patterns.
                if i < elements_len - 1 {
                    let diag_node = if is_declarative_pattern {
                        self.ctx
                            .arena
                            .get(element_data.name)
                            .and_then(|node| self.ctx.arena.get_identifier(node))
                            .and(Some(element_data.name))
                            .unwrap_or(element_idx)
                    } else {
                        element_idx
                    };
                    self.error_at_node_msg(
                        diag_node,
                        diagnostic_codes::A_REST_ELEMENT_MUST_BE_LAST_IN_A_DESTRUCTURING_PATTERN,
                        &[],
                    );
                }
            }

            self.check_binding_element_with_request(
                element_idx,
                pattern_idx,
                i,
                pattern_type,
                check_default_assignability,
                request,
            );
        }
    }

    /// Check a single binding element for default value assignability.
    ///
    /// Validates that default values in destructuring patterns are assignable
    /// to the expected property/element type.
    ///
    /// ## Parameters:
    /// - `element_idx`: The binding element node index to check
    /// - `pattern_idx`: The binding pattern node index (object or array)
    /// - `element_index`: The index of this element in the pattern
    /// - `parent_type`: The type being destructured
    ///
    /// ## Validation:
    /// - Checks computed property names for unresolved identifiers
    /// - Validates default value type assignability
    /// - Recursively checks nested binding patterns
    #[allow(dead_code)]
    pub(crate) fn check_binding_element(
        &mut self,
        element_idx: NodeIndex,
        pattern_idx: NodeIndex,
        element_index: usize,
        parent_type: TypeId,
        check_default_assignability: bool,
    ) {
        self.check_binding_element_with_request(
            element_idx,
            pattern_idx,
            element_index,
            parent_type,
            check_default_assignability,
            &TypingRequest::NONE,
        );
    }

    pub(crate) fn check_binding_element_with_request(
        &mut self,
        element_idx: NodeIndex,
        pattern_idx: NodeIndex,
        element_index: usize,
        parent_type: TypeId,
        check_default_assignability: bool,
        request: &TypingRequest,
    ) {
        let Some(element_node) = self.ctx.arena.get(element_idx) else {
            return;
        };

        // Handle holes in array destructuring: [a, , b]
        if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
            return;
        }

        let Some(element_data) = self.ctx.arena.get_binding_element(element_node) else {
            return;
        };

        // Check computed property name expression for unresolved identifiers (TS2304)
        // e.g., in `{[z]: x}` where `z` is undefined
        if element_data.property_name.is_some() {
            self.check_computed_property_name(element_data.property_name);
        }

        // Get the expected type for this binding element from the parent type
        let element_type = if parent_type != TypeId::ANY {
            // For object binding patterns, look up the property type
            // For array binding patterns, look up the tuple element type
            self.get_binding_element_type_with_request(
                pattern_idx,
                element_index,
                parent_type,
                element_data,
                request,
            )
        } else {
            TypeId::ANY
        };

        // Set contextual type for default initializers so that:
        // - Arrow/function parameters get inferred from the expected element type
        // - Literal defaults preserve their literal type for assignability checks
        //   (e.g. "foo" stays as "foo", not widened to string)
        // This must happen unconditionally (not gated on assignability checks)
        // because the initializer's type is computed and cached on first access.
        if element_data.initializer.is_some() && element_type != TypeId::ANY {
            let request = request.read().contextual(element_type);
            let default_value_type =
                self.get_type_of_node_with_request(element_data.initializer, &request);

            // TypeScript checks default value assignability for binding elements
            // regardless of whether the property type includes undefined.
            // Even for required properties, if the user provides a default value,
            // tsc still validates it against the declared type.
            if check_default_assignability {
                let _ = self.check_assignable_or_report(
                    default_value_type,
                    element_type,
                    element_data.initializer,
                );
            }
        }

        // TS1212/TS1213/TS1214: Check binding element name for strict-mode reserved words.
        // This covers destructuring patterns like `var [public] = [1]` in strict mode.
        if let Some(name_node) = self.ctx.arena.get(element_data.name)
            && name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
        {
            self.check_strict_mode_reserved_name_at(element_data.name, element_data.name);
        }

        // If the name is a nested binding pattern, recursively check it
        if let Some(name_node) = self.ctx.arena.get(element_data.name)
            && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
        {
            // When the binding element has a default value (e.g., `= {}`),
            // strip `undefined` from the element type before recursing.
            // The default guarantees the value won't be undefined at runtime,
            // so nested property lookups should not see `| undefined`.
            let nested_type = if element_data.initializer.is_some() && self.ctx.strict_null_checks()
            {
                crate::query_boundaries::flow::narrow_destructuring_default(
                    self.ctx.types,
                    element_type,
                    true,
                )
            } else {
                element_type
            };
            let nested_request = request.read().contextual(nested_type);
            self.check_binding_pattern_with_request(
                element_data.name,
                nested_type,
                check_default_assignability,
                &nested_request,
            );
        }
    }

    // --- Import Validation ---
}

// =============================================================================
// Statement Validation
// =============================================================================

impl<'a> CheckerState<'a> {
    /// Check a variable statement by iterating through declaration lists.
    pub(crate) fn check_variable_statement(&mut self, stmt_idx: NodeIndex) {
        self.check_variable_statement_with_request(stmt_idx, &TypingRequest::NONE);
    }

    pub(crate) fn check_variable_statement_with_request(
        &mut self,
        stmt_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        if let Some(var) = self.ctx.arena.get_variable(node) {
            // VariableStatement.declarations contains VariableDeclarationList nodes
            for &list_idx in &var.declarations.nodes {
                self.check_variable_declaration_list_with_request(list_idx, request);
            }
        }
    }

    /// Check a variable declaration list (var/let/const x, y, z).
    ///
    /// Iterates through individual variable declarations in a list and
    /// validates each one.
    ///
    /// ## Parameters:
    /// - `list_idx`: The variable declaration list node index to check
    pub(crate) fn check_variable_declaration_list(&mut self, list_idx: NodeIndex) {
        self.check_variable_declaration_list_with_request(list_idx, &TypingRequest::NONE);
    }

    pub(crate) fn check_variable_declaration_list_with_request(
        &mut self,
        list_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        let Some(node) = self.ctx.arena.get(list_idx) else {
            return;
        };

        // Check if this is a using/await using declaration list.
        // Only check the USING bit (bit 2) — AWAIT_USING (6) = CONST (2) | USING (4),
        // so checking just the USING bit correctly matches both using and await using
        // but not const.
        use tsz_parser::parser::flags::node_flags;
        let flags_u32 = node.flags as u32;
        let is_using = (flags_u32 & node_flags::USING) != 0;
        let is_await_using = flags_u32 == node_flags::AWAIT_USING;

        // TS2854: Top-level 'await using' requires specific module + target options.
        // Routes through the environment capability boundary to determine whether
        // a diagnostic should be emitted.
        if is_await_using && self.ctx.function_depth == 0 {
            use crate::query_boundaries::capabilities::FeatureGate;
            if self
                .ctx
                .capabilities
                .check_feature_gate(FeatureGate::TopLevelAwaitUsing)
                .is_some()
            {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    list_idx,
                    diagnostic_messages::TOP_LEVEL_AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET,
                    diagnostic_codes::TOP_LEVEL_AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET,
                );
            }
        }

        // VariableDeclarationList uses the same VariableData structure
        if let Some(var_list) = self.ctx.arena.get_variable(node) {
            // Now these are actual VariableDeclaration nodes
            for &decl_idx in &var_list.declarations.nodes {
                self.check_variable_declaration_with_request(decl_idx, request);

                // Check using/await using declarations have Symbol.dispose
                if is_using || is_await_using {
                    self.check_using_declaration_disposable(decl_idx, is_await_using);
                }
            }

            // TS2492: Check if let/const declarations inside a catch block shadow
            // the catch clause variable. `var` is allowed (different scoping), but
            // `let`/`const` are not.
            let is_let_or_const =
                (flags_u32 & (node_flags::LET | node_flags::CONST)) != 0 && !is_using;
            if is_let_or_const {
                self.check_catch_clause_variable_redeclaration(
                    list_idx,
                    &var_list.declarations.nodes,
                );
            }
        }
    }

    /// TS2492: Check if any `let`/`const` declaration in a catch block shadows
    /// the catch clause variable name.
    ///
    /// In TypeScript, `try {} catch (x) { let x; }` is an error because the
    /// block-scoped `x` would shadow the catch clause binding `x`.
    fn check_catch_clause_variable_redeclaration(
        &mut self,
        list_idx: NodeIndex,
        declarations: &[NodeIndex],
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        // Walk up: VarDeclList -> VarStatement -> Block -> CatchClause
        let var_stmt_idx = self
            .ctx
            .arena
            .get_extended(list_idx)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);
        let block_idx = self
            .ctx
            .arena
            .get_extended(var_stmt_idx)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);
        let catch_clause_idx = self
            .ctx
            .arena
            .get_extended(block_idx)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);

        // Check if the ancestor is a CatchClause
        let Some(catch_node) = self.ctx.arena.get(catch_clause_idx) else {
            return;
        };
        if catch_node.kind != syntax_kind_ext::CATCH_CLAUSE {
            return;
        }
        let Some(catch_data) = self.ctx.arena.get_catch_clause(catch_node) else {
            return;
        };
        if catch_data.variable_declaration.is_none() {
            return;
        }

        // Get the catch clause variable name
        let catch_var_name = (|| {
            let var_node = self.ctx.arena.get(catch_data.variable_declaration)?;
            let var_decl = self.ctx.arena.get_variable_declaration(var_node)?;
            let name_node = self.ctx.arena.get(var_decl.name)?;
            let ident = self.ctx.arena.get_identifier(name_node)?;
            Some(ident.escaped_text.clone())
        })();
        let Some(catch_var_name) = catch_var_name else {
            return;
        };

        // Check each declaration in the list
        for &decl_idx in declarations {
            let decl_name = (|| {
                let decl_node = self.ctx.arena.get(decl_idx)?;
                let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
                let name_node = self.ctx.arena.get(var_decl.name)?;
                let ident = self.ctx.arena.get_identifier(name_node)?;
                Some((ident.escaped_text.clone(), var_decl.name))
            })();
            if let Some((name, name_idx)) = decl_name.filter(|(name, _)| name == &catch_var_name) {
                let message = format_message(
                    diagnostic_messages::CANNOT_REDECLARE_IDENTIFIER_IN_CATCH_CLAUSE,
                    &[&name],
                );
                self.error_at_node(
                    name_idx,
                    &message,
                    diagnostic_codes::CANNOT_REDECLARE_IDENTIFIER_IN_CATCH_CLAUSE,
                );
            }
        }
    }

    // --- Using Declaration Validation (TS2804, TS2803) ---

    /// Check if a using/await using declaration's initializer type has the required dispose method.
    ///
    /// ## Parameters
    /// - `decl_idx`: The variable declaration node index
    /// - `is_await_using`: Whether this is an await using declaration
    ///
    /// Checks:
    /// - `using` requires type to have `[Symbol.dispose]()` method
    /// - `await using` requires type to have `[Symbol.asyncDispose]()` or `[Symbol.dispose]()` method
    fn check_using_declaration_disposable(&mut self, decl_idx: NodeIndex, is_await_using: bool) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return;
        };

        // Skip if no initializer
        if var_decl.initializer.is_none() {
            return;
        }

        // Get the type of the initializer
        let init_type = self.get_type_of_node(var_decl.initializer);

        // Skip error type and any (suppressed by convention)
        if init_type == TypeId::ERROR || init_type == TypeId::ANY {
            return;
        }

        // Check for the required dispose method
        if !self.type_has_disposable_method(init_type, is_await_using) {
            let (message, code) = if is_await_using {
                (
                    diagnostic_messages::THE_INITIALIZER_OF_AN_AWAIT_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SY,
                    diagnostic_codes::THE_INITIALIZER_OF_AN_AWAIT_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SY,
                )
            } else {
                (
                    diagnostic_messages::THE_INITIALIZER_OF_A_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SYMBOL_DI,
                    diagnostic_codes::THE_INITIALIZER_OF_A_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SYMBOL_DI,
                )
            };
            self.error_at_node(var_decl.initializer, message, code);
        }
    }

    /// Check if a type has the appropriate dispose method.
    ///
    /// For `using`: checks for `[Symbol.dispose]()`
    /// For `await using`: checks for `[Symbol.asyncDispose]()` or `[Symbol.dispose]()`
    fn type_has_disposable_method(&mut self, type_id: TypeId, is_await_using: bool) -> bool {
        // Check intrinsic types
        if type_id == TypeId::ANY
            || type_id == TypeId::UNKNOWN
            || type_id == TypeId::ERROR
            || type_id == TypeId::NEVER
        {
            return true; // Suppress errors on these types
        }

        // null and undefined can be disposed (no-op)
        if type_id == TypeId::NULL || type_id == TypeId::UNDEFINED {
            return true;
        }

        // Only check for dispose methods if Symbol.dispose is available in the current environment
        // Check by looking for the dispose property on SymbolConstructor
        let symbol_type = if let Some(sym_id) = self.ctx.binder.file_locals.get("Symbol") {
            self.get_type_of_symbol(sym_id)
        } else {
            TypeId::ERROR
        };

        let symbol_has_dispose = self.object_has_property(symbol_type, "dispose")
            || self.object_has_property(symbol_type, "[Symbol.dispose]")
            || self.object_has_property(symbol_type, "Symbol.dispose");

        let symbol_has_async_dispose = self.object_has_property(symbol_type, "asyncDispose")
            || self.object_has_property(symbol_type, "[Symbol.asyncDispose]")
            || self.object_has_property(symbol_type, "Symbol.asyncDispose");

        // For await using, we need either Symbol.asyncDispose or Symbol.dispose
        if is_await_using && !symbol_has_async_dispose && !symbol_has_dispose {
            // Symbol.asyncDispose and Symbol.dispose are not available in this lib
            // Don't check for them (TypeScript will emit other errors about missing globals)
            return true;
        }

        // For regular using, we need Symbol.dispose
        if !is_await_using && !symbol_has_dispose {
            // Symbol.dispose is not available in this lib
            // Don't check for it
            return true;
        }

        // Check for the dispose method on the object type
        // Try both "[Symbol.dispose]" and "Symbol.dispose" formats
        let has_dispose = self.object_has_property(type_id, "[Symbol.dispose]")
            || self.object_has_property(type_id, "Symbol.dispose");

        if is_await_using {
            // await using accepts either Symbol.asyncDispose or Symbol.dispose
            return has_dispose
                || self.object_has_property(type_id, "[Symbol.asyncDispose]")
                || self.object_has_property(type_id, "Symbol.asyncDispose");
        }

        has_dispose
    }
}
