//! Assignment expression checking (simple, compound, logical, readonly).

use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::flags::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

// =============================================================================
// Assignment Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    fn report_abstract_properties_in_destructuring_assignment(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
    ) {
        let right_idx = self.ctx.arena.skip_parenthesized_and_assertions(right_idx);
        if !self.is_this_expression(right_idx) || self.ctx.function_depth != 0 {
            return;
        }

        let Some(class_idx) = self.ctx.enclosing_class.as_ref().map(|info| info.class_idx) else {
            return;
        };
        if !self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|info| info.in_constructor)
        {
            return;
        }

        let Some(left_node) = self.ctx.arena.get(left_idx) else {
            return;
        };
        if left_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return;
        }

        let Some(obj) = self.ctx.arena.get_literal_expr(left_node) else {
            return;
        };

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            let (prop_name, error_node) =
                if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                    (self.get_property_name_resolved(prop.name), prop.name)
                } else if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                    if let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node) {
                        (
                            self.ctx
                                .arena
                                .get(shorthand.name)
                                .and_then(|node| self.ctx.arena.get_identifier(node))
                                .map(|ident| ident.escaped_text.clone()),
                            shorthand.name,
                        )
                    } else {
                        (None, NodeIndex::NONE)
                    }
                } else {
                    (None, NodeIndex::NONE)
                };

            if let Some(prop_name) = prop_name
                && let Some(declaring_class_name) =
                    self.find_abstract_property_declaring_class(class_idx, &prop_name)
            {
                self.error_abstract_property_in_constructor(
                    &prop_name,
                    &declaring_class_name,
                    error_node,
                );
            }
        }
    }

    /// TS2322: Check assignability of the rest element in an object destructuring
    /// assignment. For `({ b, ...rest } = source)`, computes the rest type
    /// (source minus named properties) and checks it against the rest target's
    /// declared type.
    fn check_object_destructuring_rest_assignability(
        &mut self,
        pattern_idx: NodeIndex,
        source_type: TypeId,
    ) {
        if source_type == TypeId::ANY || source_type == TypeId::ERROR {
            return;
        }

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };

        if pattern_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return;
        }

        let Some(obj) = self.ctx.arena.get_literal_expr(pattern_node) else {
            return;
        };

        // Find the spread/rest element and collect non-rest property names
        let mut named_properties: Vec<String> = Vec::new();
        let mut spread_target: Option<NodeIndex> = None;

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                || elem_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
            {
                // Get the spread expression (the rest target)
                spread_target = self
                    .ctx
                    .arena
                    .get_spread(elem_node)
                    .map(|spread| spread.expression)
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_unary_expr_ex(elem_node)
                            .map(|unary| unary.expression)
                    });
            } else if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                if let Some(name) = self.get_property_name_resolved(prop.name) {
                    named_properties.push(name);
                }
            } else if let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node)
                && let Some(name_node) = self.ctx.arena.get(shorthand.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                named_properties.push(ident.escaped_text.clone());
            }
        }

        let Some(spread_expr) = spread_target else {
            return;
        };

        // Only check valid rest targets (identifiers, property accesses)
        if !self.is_valid_rest_assignment_target(spread_expr) {
            return;
        }

        // Get the declared type of the rest target variable
        let rest_target_type = self.get_type_of_assignment_target(spread_expr);
        if rest_target_type == TypeId::ANY || rest_target_type == TypeId::ERROR {
            return;
        }

        // Compute the rest type: source minus named properties
        let rest_type = self.omit_properties_from_type(source_type, &named_properties);

        // Check assignability
        self.ensure_relation_input_ready(rest_type);
        self.ensure_relation_input_ready(rest_target_type);

        let _ = self.check_assignable_or_report(rest_type, rest_target_type, spread_expr);
    }

    /// TS2341/TS2445: Check private/protected accessibility for properties
    /// accessed in destructuring assignment patterns.
    ///
    /// In `{ o: target } = source`, property `o` is accessed on the source type
    /// and must respect visibility modifiers. This walks the destructuring
    /// pattern recursively and checks each property name against the source type.
    fn check_destructuring_property_accessibility(
        &mut self,
        pattern_idx: NodeIndex,
        source_type: TypeId,
    ) {
        if source_type == TypeId::ANY || source_type == TypeId::ERROR {
            return;
        }

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };

        if pattern_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let Some(obj) = self.ctx.arena.get_literal_expr(pattern_node) else {
                return;
            };

            for &elem_idx in &obj.elements.nodes {
                let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                    continue;
                };

                if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                    // Property assignment: { name: target } or { name: target = default }
                    if let Some(name) = self.get_property_name_resolved(prop.name) {
                        // When a default value is present (e.g., `{ a: x = 1 }`),
                        // the property need not exist on the source object.
                        let has_default_value =
                            self.ctx.arena.get(prop.initializer).is_some_and(|v| {
                                v.kind == syntax_kind_ext::BINARY_EXPRESSION
                                    && self.ctx.arena.get_binary_expr(v).is_some_and(|b| {
                                        b.operator_token == SyntaxKind::EqualsToken as u16
                                    })
                            });
                        if !has_default_value {
                            self.check_destructuring_property_exists(&name, source_type, prop.name);
                        }
                        self.check_property_accessibility(
                            NodeIndex::NONE,
                            &name,
                            prop.name,
                            source_type,
                        );

                        // Recurse into nested patterns: resolve property type from source
                        if let Some(value_node) = self.ctx.arena.get(prop.initializer) {
                            if value_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                || value_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            {
                                let prop_type = self
                                    .resolve_property_type_for_destructuring(source_type, &name);
                                if let Some(prop_type) = prop_type {
                                    self.check_destructuring_property_accessibility(
                                        prop.initializer,
                                        prop_type,
                                    );
                                }
                            } else if value_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                                // { name: pattern = default } — check the LHS of the assignment
                                if let Some(bin) = self.ctx.arena.get_binary_expr(value_node)
                                    && bin.operator_token == SyntaxKind::EqualsToken as u16
                                    && let Some(lhs_node) = self.ctx.arena.get(bin.left)
                                    && (lhs_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                        || lhs_node.kind
                                            == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
                                {
                                    let prop_type = self.resolve_property_type_for_destructuring(
                                        source_type,
                                        &name,
                                    );
                                    if let Some(prop_type) = prop_type {
                                        self.check_destructuring_property_accessibility(
                                            bin.left, prop_type,
                                        );
                                    }
                                }
                            }
                        }
                    } else {
                        // Computed property name that couldn't be resolved to a static string
                        // (e.g., `{ ["x" + ""]: v } = 0`). Check for index signature compatibility.
                        // TS2537: Type 'X' has no matching index signature for type 'Y'.
                        self.check_destructuring_computed_key_index_signature(
                            prop.name,
                            source_type,
                        );
                    }
                } else if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                    // Shorthand: { x } or { x = default } — property name is the identifier.
                    // When a default value is present (e.g. `{ b = '5' }`), tsc does NOT emit
                    // TS2339 for missing properties because the default handles the absent
                    // property. Only check existence when there is no default.
                    if let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node)
                        && let Some(name_node) = self.ctx.arena.get(shorthand.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        if !shorthand.equals_token {
                            self.check_destructuring_property_exists(
                                &ident.escaped_text,
                                source_type,
                                shorthand.name,
                            );
                        }
                        self.check_property_accessibility(
                            NodeIndex::NONE,
                            &ident.escaped_text,
                            shorthand.name,
                            source_type,
                        );
                    }
                }
            }
        } else if pattern_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            // Array destructuring: recurse into elements with element types
            let Some(array_lit) = self.ctx.arena.get_literal_expr(pattern_node) else {
                return;
            };

            for (index, &elem_idx) in array_lit.elements.nodes.iter().enumerate() {
                let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                    continue;
                };
                if elem_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                    continue;
                }

                // Handle spread elements: compute rest type instead of single-element type
                let (target_idx, check_type, is_spread) = if elem_node.kind
                    == syntax_kind_ext::SPREAD_ELEMENT
                {
                    let Some(spread) = self.ctx.arena.get_spread(elem_node) else {
                        continue;
                    };
                    let rest_type = self.compute_rest_type_for_destructuring(source_type, index);
                    let Some(rest_type) = rest_type else {
                        continue;
                    };
                    (spread.expression, rest_type, true)
                } else {
                    let elem_type = self.resolve_element_type_for_destructuring(source_type, index);
                    let Some(elem_type) = elem_type else {
                        continue;
                    };
                    (elem_idx, elem_type, false)
                };

                if let Some(target_node) = self.ctx.arena.get(target_idx) {
                    if target_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        || target_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    {
                        self.check_destructuring_property_accessibility(target_idx, check_type);
                    } else if target_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                        && let Some(bin) = self.ctx.arena.get_binary_expr(target_node)
                        && bin.operator_token == SyntaxKind::EqualsToken as u16
                        && let Some(lhs_node) = self.ctx.arena.get(bin.left)
                        && (lhs_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            || lhs_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
                    {
                        self.check_destructuring_property_accessibility(bin.left, check_type);
                    } else if is_spread {
                        // For spread elements targeting simple expressions (identifiers,
                        // property accesses), check that the rest type is assignable to
                        // the target's declared type. For example:
                        //   var c = { bogus: 0 };
                        //   [...c] = ["", 0];  // TS2741: Property 'bogus' is missing
                        // Use exact anchor to point at the identifier (e.g., `c`), not
                        // the enclosing array literal.
                        let target_type = self.get_type_of_assignment_target(target_idx);
                        if target_type != TypeId::ANY
                            && target_type != TypeId::ERROR
                            && check_type != TypeId::ANY
                        {
                            self.check_assignable_or_report_at_exact_anchor(
                                check_type,
                                target_type,
                                target_idx,
                                target_idx,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Resolve the type of a property on an object type for destructuring checks.
    fn resolve_property_type_for_destructuring(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        use crate::query_boundaries::common::PropertyAccessResult;
        match self.resolve_property_access_with_env(object_type, property_name) {
            PropertyAccessResult::Success { type_id, .. } => Some(type_id),
            _ => None,
        }
    }

    /// Resolve the element type at a given index from an array/tuple type.
    fn resolve_element_type_for_destructuring(
        &mut self,
        source_type: TypeId,
        index: usize,
    ) -> Option<TypeId> {
        // Try tuple element type first
        if let Some(elems) =
            tsz_solver::type_queries::get_tuple_elements(self.ctx.types, source_type)
        {
            if index < elems.len() {
                return Some(elems[index].type_id);
            }
            return None;
        }
        // Fall back to array element type
        tsz_solver::type_queries::get_array_element_type(self.ctx.types, source_type)
    }

    /// TS2339: Check that a property exists on a type during destructuring.
    fn check_destructuring_property_exists(
        &mut self,
        property_name: &str,
        source_type: TypeId,
        error_node: NodeIndex,
    ) {
        if source_type == TypeId::ANY || source_type == TypeId::ERROR {
            return;
        }
        use crate::query_boundaries::common::PropertyAccessResult;
        if let PropertyAccessResult::PropertyNotFound { .. } =
            self.resolve_property_access_with_env(source_type, property_name)
        {
            // For computed property names like `["x"]`, TSC points at the inner
            // expression (`"x"`) rather than the brackets. Resolve the inner node.
            let resolved_error_node = self
                .ctx
                .arena
                .get(error_node)
                .and_then(|node| self.ctx.arena.get_computed_property(node))
                .map_or(error_node, |computed| computed.expression);

            // For primitive types in destructuring assignment, TSC uses the boxed
            // wrapper type name (e.g., "Number" instead of "number") in the error.
            if let Some(boxed_name) = self.get_boxed_type_display_name(source_type) {
                let message =
                    format!("Property '{property_name}' does not exist on type '{boxed_name}'.");
                self.error_at_node(
                    resolved_error_node,
                    &message,
                    diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                );
            } else {
                self.error_property_not_exist_at(property_name, source_type, resolved_error_node);
            }
        }
    }

    /// TS2537: Check that a computed property key in destructuring assignment has a matching
    /// index signature on the source type. For example, `({ ["x" + ""]: v } = 0)` should
    /// error because `Number` has no string index signature.
    fn check_destructuring_computed_key_index_signature(
        &mut self,
        prop_name_idx: NodeIndex,
        source_type: TypeId,
    ) {
        if source_type == TypeId::ANY
            || source_type == TypeId::ERROR
            || source_type == TypeId::UNKNOWN
        {
            return;
        }

        // Get the computed property expression
        let Some(prop_name_node) = self.ctx.arena.get(prop_name_idx) else {
            return;
        };
        let Some(computed) = self.ctx.arena.get_computed_property(prop_name_node) else {
            return;
        };

        // Compute the type of the key expression
        let key_type = self.compute_type_of_node(computed.expression);
        if key_type == TypeId::ANY || key_type == TypeId::ERROR {
            return;
        }

        // Only check for string or number key types
        let key_is_string = key_type == TypeId::STRING;
        let key_is_number = key_type == TypeId::NUMBER;
        if !key_is_string && !key_is_number {
            return;
        }

        // Get the apparent type for the source (boxed wrapper for primitives).
        // For example, `number` -> `Number` interface, `string` -> `String` interface.
        let apparent_type = self.get_apparent_type_for_index_check(source_type);

        // Check if the apparent type has a matching index signature
        let has_matching_index = |ty: TypeId| {
            crate::query_boundaries::state::checking::object_shape(self.ctx.types, ty).is_some_and(
                |shape| {
                    if key_is_string {
                        shape.string_index.is_some()
                    } else {
                        shape.number_index.is_some() || shape.string_index.is_some()
                    }
                },
            )
        };

        let has_index_signature = if let Some(members) =
            crate::query_boundaries::state::checking::union_members(self.ctx.types, apparent_type)
        {
            members.into_iter().all(has_matching_index)
        } else {
            has_matching_index(apparent_type)
        };

        if !has_index_signature {
            // For the error message, use the boxed type name (e.g., "Number" not "number")
            // to match TSC's behavior. For primitive source types, use the capitalized name.
            let object_str = self
                .get_boxed_type_display_name(source_type)
                .unwrap_or_else(|| {
                    let mut formatter = self.ctx.create_type_formatter();
                    formatter.format(apparent_type).into_owned()
                });
            let mut formatter = self.ctx.create_type_formatter();
            let index_str = formatter.format(key_type);
            let message = crate::diagnostics::format_message(
                diagnostic_messages::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                &[&object_str, &index_str],
            );
            self.error_at_node(
                computed.expression,
                &message,
                diagnostic_codes::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
            );
        }
    }

    /// Get the apparent type for index signature checks. For primitive types,
    /// this returns the boxed wrapper type (e.g., `number` -> `Number`).
    /// For other types, returns the type as-is.
    fn get_apparent_type_for_index_check(&self, type_id: TypeId) -> TypeId {
        use tsz_solver::IntrinsicKind;
        let kind = match type_id {
            TypeId::NUMBER => Some(IntrinsicKind::Number),
            TypeId::STRING => Some(IntrinsicKind::String),
            TypeId::BOOLEAN => Some(IntrinsicKind::Boolean),
            TypeId::BIGINT => Some(IntrinsicKind::Bigint),
            TypeId::SYMBOL => Some(IntrinsicKind::Symbol),
            _ => None,
        };
        if let Some(kind) = kind {
            tsz_solver::TypeDatabase::get_boxed_type(self.ctx.types, kind).unwrap_or(type_id)
        } else {
            type_id
        }
    }

    /// Get the display name of the boxed wrapper type for a primitive.
    /// Returns `Some("Number")` for `number`, `Some("String")` for `string`, etc.
    fn get_boxed_type_display_name(&self, type_id: TypeId) -> Option<String> {
        match type_id {
            TypeId::NUMBER => Some("Number".to_string()),
            TypeId::STRING => Some("String".to_string()),
            TypeId::BOOLEAN => Some("Boolean".to_string()),
            TypeId::BIGINT => Some("BigInt".to_string()),
            TypeId::SYMBOL => Some("Symbol".to_string()),
            _ => None,
        }
    }

    /// Compute the rest type for a spread element in array destructuring.
    fn compute_rest_type_for_destructuring(
        &mut self,
        source_type: TypeId,
        from_index: usize,
    ) -> Option<TypeId> {
        if let Some(elems) =
            tsz_solver::type_queries::get_tuple_elements(self.ctx.types, source_type)
        {
            if from_index >= elems.len() {
                return None;
            }
            if from_index == 0 {
                return Some(source_type);
            }
            let rest_elems: Vec<_> = elems[from_index..].to_vec();
            let rest_tuple = self.ctx.types.tuple(rest_elems);
            Some(rest_tuple)
        } else {
            Some(source_type)
        }
    }

    // =========================================================================
    // Assignment Operator Utilities
    // =========================================================================

    /// Check if a token is an assignment operator (=, +=, -=, etc.)
    pub(crate) const fn is_assignment_operator(&self, operator: u16) -> bool {
        matches!(
            operator,
            k if k == SyntaxKind::EqualsToken as u16
                || k == SyntaxKind::PlusEqualsToken as u16
                || k == SyntaxKind::MinusEqualsToken as u16
                || k == SyntaxKind::AsteriskEqualsToken as u16
                || k == SyntaxKind::AsteriskAsteriskEqualsToken as u16
                || k == SyntaxKind::SlashEqualsToken as u16
                || k == SyntaxKind::PercentEqualsToken as u16
                || k == SyntaxKind::LessThanLessThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::AmpersandEqualsToken as u16
                || k == SyntaxKind::BarEqualsToken as u16
                || k == SyntaxKind::BarBarEqualsToken as u16
                || k == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                || k == SyntaxKind::QuestionQuestionEqualsToken as u16
                || k == SyntaxKind::CaretEqualsToken as u16
        )
    }

    // =========================================================================
    // Assignment Expression Checking
    // =========================================================================

    /// Check if a node is a valid assignment target (variable, property access, element access,
    /// or destructuring pattern).
    ///
    /// Returns false for literals, call expressions, and other non-assignable expressions.
    /// Used to emit TS2364: "The left-hand side of an assignment expression must be a variable
    /// or a property access."
    pub(crate) fn is_valid_assignment_target(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => true,
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                true
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                true
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                // Check the inner expression
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.is_valid_assignment_target(paren.expression)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::AS_EXPRESSION =>
            {
                // Satisfies and as expressions are valid assignment targets if their inner expression is valid
                // Example: (x satisfies number) = 10
                if let Some(assertion) = self.ctx.arena.get_type_assertion(node) {
                    self.is_valid_assignment_target(assertion.expression)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Check if a node is part of an optional chain (has `?.` somewhere in its left spine).
    ///
    /// Walks through property access, element access, and call expression chains looking
    /// for any node with `question_dot_token: true` (for accesses) or the `OPTIONAL_CHAIN`
    /// flag (for calls). For example, in `obj?.a.b`, both `obj?.a` and `obj?.a.b` are
    /// considered part of the optional chain.
    ///
    /// Skips through transparent wrappers (parenthesized, non-null, type assertions, satisfies).
    pub(crate) fn is_optional_chain_access(&self, idx: NodeIndex) -> bool {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.ctx.arena.get_access_expr(node) {
                    // This node itself is an optional chain root (has `?.`)
                    if access.question_dot_token {
                        return true;
                    }
                    // Check if the base expression is part of an optional chain
                    self.is_optional_chain_access(access.expression)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                // Call expressions get the OPTIONAL_CHAIN flag from the parser
                if (node.flags as u32 & node_flags::OPTIONAL_CHAIN) != 0 {
                    return true;
                }
                // Check if the callee is part of an optional chain
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    self.is_optional_chain_access(call.expression)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Check if a node is a valid target for object rest assignment.
    /// Valid targets are identifiers, property accesses, and element accesses.
    /// Binary expressions like `a + b` are NOT valid rest targets (TS2701).
    pub(crate) fn is_valid_rest_assignment_target(&self, idx: NodeIndex) -> bool {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        node.kind == SyntaxKind::Identifier as u16
            || node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
    }

    /// Check if an identifier node refers to a const variable.
    ///
    /// Returns `Some(name)` if the identifier refers to a const, `None` otherwise.
    fn get_const_variable_name(&self, ident_idx: NodeIndex) -> Option<String> {
        let ident_idx = self.unwrap_assignment_target_for_symbol(ident_idx);
        let node = self.ctx.arena.get(ident_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let ident = self.ctx.arena.get_identifier(node)?;
        let name = ident.escaped_text.clone();

        // Use binder-level resolution (no tracking side-effect) to avoid marking
        // the assignment target as "read" in `referenced_symbols`. The const check
        // is a read-only query — assignment targets should only be tracked via
        // `resolve_identifier_symbol_for_write` in `get_type_of_assignment_target`.
        // Using the tracking `resolve_identifier_symbol` here would suppress TS6133
        // for write-only parameters (e.g., `person2 = "dummy value"` should still
        // flag `person2` as unused).
        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, ident_idx)?;

        // Find the correct binder and arena for this symbol
        let mut target_binder = self.ctx.binder;
        let mut target_arena = self.ctx.arena;

        if let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id) {
            if let Some(all_binders) = &self.ctx.all_binders
                && let Some(b) = all_binders.get(file_idx)
            {
                target_binder = b;
            }
            if let Some(all_arenas) = &self.ctx.all_arenas
                && let Some(a) = all_arenas.get(file_idx)
            {
                target_arena = a;
            }
        } else if let Some(arena) = self.ctx.binder.symbol_arenas.get(&sym_id) {
            // It could be a lib symbol where target_binder is still self.ctx.binder (due to merging)
            // or one of the lib_contexts.
            target_arena = arena.as_ref();
        }

        // Also check if it's from a lib context
        for lib in &self.ctx.lib_contexts {
            if let Some(sym) = lib.binder.get_symbol(sym_id)
                && sym.escaped_name == name
            {
                target_binder = &lib.binder;
                target_arena = lib.arena.as_ref();
                break;
            }
        }

        let symbol = target_binder
            .get_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))?;
        let value_decl = symbol.value_declaration;
        if value_decl.is_none() {
            return None;
        }

        // Sometimes the declaration is specifically registered in declaration_arenas
        if let Some(arenas) = self
            .ctx
            .binder
            .declaration_arenas
            .get(&(sym_id, value_decl))
            && let Some(first) = arenas.first()
        {
            target_arena = first.as_ref();
        }

        target_arena.get(value_decl)?;
        target_arena
            .is_const_variable_declaration(value_decl)
            .then_some(name)
    }

    /// Strip wrappers that preserve assignment target identity for symbol checks.
    ///
    /// Examples:
    /// - `(x)` -> `x`
    /// - `x!` -> `x`
    /// - `(x as T)` -> `x`
    /// - `(x satisfies T)` -> `x`
    fn unwrap_assignment_target_for_symbol(&self, idx: NodeIndex) -> NodeIndex {
        self.ctx.arena.skip_parenthesized_and_assertions(idx)
    }

    /// Check if the operand of an increment/decrement operator is a valid l-value (TS2357).
    ///
    /// The operand must be a variable (Identifier), property access, or element access.
    /// Expressions like `(1 + 2)++` or `1++` are not valid.
    /// Transparent wrappers are skipped: parenthesized, non-null assertion, type assertion,
    /// and satisfies expressions (e.g., `foo[x]!++` and `(a satisfies number)++` are valid).
    /// Returns `true` if an error was emitted.
    pub(crate) fn check_increment_decrement_operand(&mut self, operand_idx: NodeIndex) -> bool {
        let inner = self.skip_assignment_transparent_wrappers(operand_idx);
        let Some(node) = self.ctx.arena.get(inner) else {
            return false;
        };

        let is_valid = node.kind == SyntaxKind::Identifier as u16
            || node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION;

        if !is_valid {
            self.error_at_node(
                operand_idx,
                diagnostic_messages::THE_OPERAND_OF_AN_INCREMENT_OR_DECREMENT_OPERATOR_MUST_BE_A_VARIABLE_OR_A_PROPER,
                diagnostic_codes::THE_OPERAND_OF_AN_INCREMENT_OR_DECREMENT_OPERATOR_MUST_BE_A_VARIABLE_OR_A_PROPER,
            );
            return true;
        }

        // TS2777: The operand of an increment or decrement operator may not be an optional property access.
        if self.is_optional_chain_access(inner) {
            self.error_at_node(
                operand_idx,
                diagnostic_messages::THE_OPERAND_OF_AN_INCREMENT_OR_DECREMENT_OPERATOR_MAY_NOT_BE_AN_OPTIONAL_PROPERT,
                diagnostic_codes::THE_OPERAND_OF_AN_INCREMENT_OR_DECREMENT_OPERATOR_MAY_NOT_BE_AN_OPTIONAL_PROPERT,
            );
            return true;
        }

        false
    }

    /// Skip through transparent wrapper expressions that don't affect l-value validity.
    ///
    /// Skips: parenthesized, non-null assertion (`!`), type assertion (`as`/angle-bracket),
    /// and `satisfies` expressions.
    pub(crate) fn skip_assignment_transparent_wrappers(&self, idx: NodeIndex) -> NodeIndex {
        self.ctx.arena.skip_parenthesized_and_assertions(idx)
    }

    /// Check if the assignment target (LHS) is a const variable and emit TS2588 if so.
    ///
    /// Resolves through parenthesized expressions to find the underlying identifier.
    /// Returns `true` if a TS2588 error was emitted (caller should skip further type checks).
    pub(crate) fn check_const_assignment(&mut self, target_idx: NodeIndex) -> bool {
        let inner = self.ctx.arena.skip_parenthesized(target_idx);
        if let Some(name) = self.get_const_variable_name(inner) {
            self.error_at_node_msg(
                inner,
                diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_CONSTANT,
                &[&name],
            );
            return true;
        }
        false
    }

    /// TS1100: Cannot assign to `eval` or `arguments` in strict mode.
    pub(crate) fn check_strict_mode_eval_or_arguments_assignment(&mut self, target_idx: NodeIndex) {
        let inner = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(node) = self.ctx.arena.get(inner) else {
            return;
        };
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return;
        }
        let Some(ident) = self.ctx.arena.get_identifier(node) else {
            return;
        };
        let name = &ident.escaped_text;
        if crate::state_checking::is_eval_or_arguments(name) && self.is_strict_mode_for_node(inner)
        {
            self.emit_eval_or_arguments_strict_mode_error(inner, name);
        }
    }

    /// Check if assignment target is a function and emit TS2630 error.
    ///
    /// TypeScript does not allow direct assignment to functions:
    /// ```typescript
    /// function foo() {}
    /// foo = bar;  // Error TS2630: Cannot assign to 'foo' because it is a function.
    /// ```
    ///
    /// Also checks for built-in global functions (eval, arguments) which always
    /// emit TS2630 when assigned to, even without explicit function declarations.
    ///
    /// This check helps catch common mistakes where users try to reassign function names.
    pub(crate) fn check_function_assignment(&mut self, target_idx: NodeIndex) -> bool {
        let inner = self.ctx.arena.skip_parenthesized(target_idx);

        // Only check identifiers - property access like obj.fn = x is allowed
        let Some(node) = self.ctx.arena.get(inner) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        // Get the identifier name
        let Some(id_data) = self.ctx.arena.get_identifier(node) else {
            return false;
        };
        let name = &id_data.escaped_text;

        // `undefined` is not a variable — it's a global constant that cannot be assigned to.
        // TypeScript emits TS2539 for `undefined = ...` or `undefined++` etc.
        if name == "undefined" {
            self.error_at_node_msg(
                inner,
                diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_NOT_A_VARIABLE,
                &[name],
            );
            return true;
        }

        // Check for built-in global functions that always error with TS2630
        // Note: `arguments` is NOT included here because inside function bodies,
        // `arguments` is an IArguments object (handled by type_computation_complex.rs).
        // Only at module scope would `arguments` resolve to a function-like global.
        if name == "eval" {
            self.error_at_node_msg(
                inner,
                diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_FUNCTION,
                &[name],
            );
            return true;
        }

        // TS2632: Check if this identifier is an import binding BEFORE resolving
        // through imports. resolve_identifier follows aliases, so the resolved symbol
        // would be the export target (e.g., `var x`) rather than the import binding.
        // Import bindings are readonly in ESM — you cannot reassign them.
        if let Some(local_sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(local_sym) = self.ctx.binder.get_symbol(local_sym_id)
            && local_sym.flags & symbol_flags::ALIAS != 0
        {
            self.error_at_node_msg(
                inner,
                diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_AN_IMPORT,
                &[name],
            );
            return true;
        }

        // Look up the symbol for this identifier by resolving it through the scope chain
        // Note: We use resolve_identifier instead of node_symbols because node_symbols
        // only contains declaration nodes, not identifier references.
        let sym_id = self.ctx.binder.resolve_identifier(self.ctx.arena, inner);
        let Some(sym_id) = sym_id else {
            return false;
        };

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Check for uninstantiated namespaces first (TS2708)
        let is_namespace = (symbol.flags & symbol_flags::NAMESPACE_MODULE) != 0;
        let value_flags_except_module = symbol_flags::VALUE & !symbol_flags::VALUE_MODULE;
        let has_other_value = (symbol.flags & value_flags_except_module) != 0;

        if is_namespace && !has_other_value {
            let mut is_instantiated = false;
            for decl_idx in &symbol.declarations {
                if self.is_namespace_declaration_instantiated(*decl_idx) {
                    is_instantiated = true;
                    break;
                }
            }
            if !is_instantiated {
                self.report_wrong_meaning_diagnostic(
                    name,
                    inner,
                    crate::query_boundaries::name_resolution::NameLookupKind::Namespace,
                );
                return true;
            }
        }

        // Check for type-only symbols used as values in assignment position (TS2693)
        if symbol.flags & symbol_flags::TYPE != 0 && symbol.flags & symbol_flags::VALUE == 0 {
            self.report_wrong_meaning_diagnostic(
                name,
                inner,
                crate::query_boundaries::name_resolution::NameLookupKind::Type,
            );
            return true;
        }

        // Check if this symbol is a class, enum, function, or namespace (TS2629, TS2628, TS2630, TS2631)
        let code = if symbol.flags & symbol_flags::MODULE != 0 {
            diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_NAMESPACE
        } else if symbol.flags & symbol_flags::CLASS != 0 {
            diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_CLASS
        } else if symbol.flags & symbol_flags::ENUM != 0 {
            diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_AN_ENUM
        } else if symbol.flags & symbol_flags::FUNCTION != 0 {
            diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_FUNCTION
        } else {
            return false;
        };

        self.error_at_node_msg(inner, code, &[name]);
        true
    }

    /// In JS files, `module.exports = X` and `exports = X` are declarations, not assignments.
    /// tsc does not check assignability for these — the type flows from the RHS.
    /// Without this suppression, tsz would emit false TS2322/TS2741 errors when the
    /// module's augmented export type (with later `.D = ...` property assignments)
    /// is used as the assignment target type.
    fn is_commonjs_module_exports_assignment(&self, target_idx: NodeIndex) -> bool {
        if !self.is_js_file() {
            return false;
        }

        let target_idx = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return false;
        };

        // Check for `exports` identifier (unbound)
        if target_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.ctx.arena.get_identifier(target_node)
                && ident.escaped_text == "exports"
            {
                return true;
            }
            return false;
        }

        // Check for `module.exports` property access
        if target_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(target_node)
        {
            let is_module = self
                .ctx
                .arena
                .get_identifier_at(access.expression)
                .is_some_and(|ident| ident.escaped_text == "module");
            let is_exports = self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "exports");
            return is_module && is_exports;
        }

        false
    }

    fn is_js_namespace_enum_rebind_assignment_target(&self, target_idx: NodeIndex) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        if !self.is_js_file() {
            return false;
        }

        let target_idx = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return false;
        };
        if target_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        if let Some(member_sym_id) = self.resolve_qualified_symbol(target_idx)
            && let Some(member_symbol) = self
                .get_cross_file_symbol(member_sym_id)
                .or_else(|| self.ctx.binder.get_symbol(member_sym_id))
            && (member_symbol.flags & symbol_flags::ENUM) != 0
        {
            let parent_sym_id = member_symbol.parent;
            if let Some(parent_symbol) = self
                .get_cross_file_symbol(parent_sym_id)
                .or_else(|| self.ctx.binder.get_symbol(parent_sym_id))
                && (parent_symbol.flags
                    & (symbol_flags::MODULE
                        | symbol_flags::NAMESPACE
                        | symbol_flags::NAMESPACE_MODULE))
                    != 0
                && (parent_symbol.flags & symbol_flags::ENUM) == 0
            {
                return true;
            }
        }

        let Some(access) = self.ctx.arena.get_access_expr(target_node) else {
            return false;
        };
        let Some(prop_ident) = self.ctx.arena.get_identifier_at(access.name_or_argument) else {
            return false;
        };

        let Some(base_sym_id) = self.resolve_identifier_symbol(access.expression) else {
            return false;
        };
        let Some(base_symbol) = self
            .get_cross_file_symbol(base_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(base_sym_id))
        else {
            return false;
        };
        if (base_symbol.flags
            & (symbol_flags::MODULE | symbol_flags::NAMESPACE | symbol_flags::NAMESPACE_MODULE))
            == 0
        {
            return false;
        }

        let Some(exports) = base_symbol.exports.as_ref() else {
            return false;
        };
        let Some(member_sym_id) = exports.get(prop_ident.escaped_text.as_str()) else {
            return false;
        };
        let Some(member_symbol) = self
            .get_cross_file_symbol(member_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(member_sym_id))
        else {
            return false;
        };

        (member_symbol.flags & symbol_flags::ENUM) != 0
    }

    /// Check an assignment expression (=).
    ///
    /// ## Contextual Typing:
    /// - The LHS type is used as contextual type for the RHS expression
    /// - This enables better type inference for object literals, etc.
    ///
    /// ## Validation:
    /// - Checks constructor accessibility (if applicable)
    /// - Validates that RHS is assignable to LHS
    /// - Checks for excess properties in object literals
    /// - Validates readonly assignments
    pub(crate) fn check_assignment_expression(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        expr_idx: NodeIndex,
    ) -> TypeId {
        // TS2364: The left-hand side of an assignment expression must be a variable or a property access.
        // Suppress when the LHS is near a parse error (e.g. `1 >>/**/= 2;` where `>>=` is split
        // by a comment — the parser already emits TS1109 and the assignment is a recovery artifact).
        if !self.is_valid_assignment_target(left_idx) && !self.node_has_nearby_parse_error(left_idx)
        {
            self.error_at_node(
                left_idx,
                "The left-hand side of an assignment expression must be a variable or a property access.",
                diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MUST_BE_A_VARIABLE_OR_A_PROPERTY,
            );
            self.get_type_of_node(left_idx);
            self.get_type_of_node(right_idx);
            return TypeId::ANY;
        }

        // TS2779: The left-hand side of an assignment expression may not be an optional property access.
        {
            let inner = self.skip_assignment_transparent_wrappers(left_idx);
            if self.is_optional_chain_access(inner) {
                self.error_at_node(
                    left_idx,
                    diagnostic_messages::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_A,
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_A,
                );
            }
        }

        // TS2588: Cannot assign to 'x' because it is a constant.
        // Check early - if this fires, skip type assignability checks (tsc behavior).
        let is_const = self.check_const_assignment(left_idx);

        // TS2630: Cannot assign to 'x' because it is a function.
        // This check must come after valid assignment target check but before type checking.
        let is_function_assignment = self.check_function_assignment(left_idx);

        // TS1100: Cannot assign to `eval` or `arguments` in strict mode.
        self.check_strict_mode_eval_or_arguments_assignment(left_idx);

        // Set destructuring flag when LHS is an object/array pattern to suppress
        // TS1117 (duplicate property) checks in destructuring targets.
        let (is_destructuring, is_array_destructuring) =
            if let Some(left_node) = self.ctx.arena.get(left_idx) {
                let is_obj = left_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION;
                let is_arr = left_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION;
                (is_obj || is_arr, is_arr)
            } else {
                (false, false)
            };
        let prev_destructuring = self.ctx.in_destructuring_target;
        if is_destructuring {
            self.ctx.in_destructuring_target = true;
        }
        let left_target = self.get_type_of_assignment_target(left_idx);
        self.ctx.in_destructuring_target = prev_destructuring;
        let mut left_type = self.resolve_type_query_type(left_target);
        let mut has_explicit_jsdoc_left_type = false;

        // In JS/checkJs mode, allow JSDoc `@type` on assignment statements to
        // provide the contextual target type for the LHS.
        //
        // Example:
        //   /** @type {string} */
        //   C.prototype = 12;
        if self.is_js_file()
            && self.ctx.compiler_options.check_js
            && let Some(jsdoc_left_type) = self
                .enclosing_expression_statement(expr_idx)
                .and_then(|stmt_idx| self.js_statement_declared_type(stmt_idx))
                .or_else(|| {
                    // Nested assignments inside JS accessors/functions should not inherit
                    // an enclosing declaration's JSDoc @type as the assignment target.
                    // Only direct JSDoc attached to the assignment expression/LHS should
                    // act as a declared target type here.
                    self.jsdoc_type_annotation_for_node_direct(expr_idx)
                        .or_else(|| self.jsdoc_type_annotation_for_node_direct(left_idx))
                })
        {
            left_type = jsdoc_left_type;
            has_explicit_jsdoc_left_type = true;
        }
        if self.is_js_file()
            && self.ctx.compiler_options.check_js
            && matches!(left_type, TypeId::ANY | TypeId::UNKNOWN)
            && let Some(name) = self.expression_text(left_idx)
            && let Some(jsdoc_left_type) = self.resolve_jsdoc_assigned_value_type(&name)
        {
            left_type = jsdoc_left_type;
        }

        if is_function_assignment {
            // TS2629/TS2628/TS2630 are terminal for simple assignment targets in tsc.
            // Do not contextually type the RHS against the class/function/enum object
            // type, or we can produce spurious follow-on errors like missing
            // `prototype` on a function expression assigned to a class symbol.
            return self.get_type_of_node(right_idx);
        }

        if !is_const && self.is_commonjs_module_exports_assignment(left_idx) {
            // In JS files, `module.exports = X` and `exports = X` are declarations.
            // The export surface is inferred from the RHS, so using the current
            // `module.exports` shape as a contextual type for `X` can introduce
            // false excess-property errors before assignability is even skipped.
            // However, when an explicit JSDoc `@type` provides the assignment target,
            // tsc does contextually type the RHS from that declared type.
            if !has_explicit_jsdoc_left_type {
                return self.get_type_of_node(right_idx);
            }
        }

        let contextual_request = if !is_destructuring
            && left_type != TypeId::ANY
            && left_type != TypeId::NEVER
            && left_type != TypeId::UNKNOWN
            && !self.type_contains_error(left_type)
        {
            let contextual_target = if let Some(right_node) = self.ctx.arena.get(right_idx) {
                if right_node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || right_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                {
                    self.evaluate_contextual_type(left_type)
                } else {
                    left_type
                }
            } else {
                left_type
            };
            if let Some(right_node) = self.ctx.arena.get(right_idx) {
                let needs_fresh_contextual_check = right_node.kind
                    == syntax_kind_ext::ARROW_FUNCTION
                    || right_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || right_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    || right_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    || right_node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
                    || (right_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                        && self
                            .ctx
                            .arena
                            .get_binary_expr(right_node)
                            .is_some_and(|bin| {
                                matches!(
                                    bin.operator_token,
                                    k if k == SyntaxKind::BarBarToken as u16
                                        || k == SyntaxKind::AmpersandAmpersandToken as u16
                                        || k == SyntaxKind::QuestionQuestionToken as u16
                                        || k == SyntaxKind::CommaToken as u16
                                )
                            }));
                if needs_fresh_contextual_check {
                    self.invalidate_expression_for_contextual_retry(right_idx);
                }
            }
            crate::context::TypingRequest::with_contextual_type(contextual_target)
        } else {
            crate::context::TypingRequest::NONE
        };

        let right_raw = self.get_type_of_node_with_request(right_idx, &contextual_request);
        let right_type = self.resolve_type_query_type(right_raw);

        // Ensure the RHS type is also available in node_types for flow analysis.
        // When clear_type_cache_recursive removes the RHS entry for contextual
        // re-checking, the result ends up only in request_node_types. Flow analysis
        // needs node_types to compute assignment-based narrowing (e.g., `d ?? (d = x ?? "x")`).
        if right_raw != TypeId::ERROR && right_raw != TypeId::DELEGATE {
            self.ctx.node_types.entry(right_idx.0).or_insert(right_raw);
        }

        // NOTE: Freshness is now tracked on the TypeId via ObjectFlags.
        // No need to manually track freshness removal here.

        self.ensure_relation_input_ready(right_type);
        self.ensure_relation_input_ready(left_type);

        let mut is_not_iterable = false;
        if is_array_destructuring {
            // TS2488: Array destructuring assignments require an iterable RHS.
            // Keep parity with `[] = value` behavior by skipping empty patterns.
            let should_check_iterability = self
                .ctx
                .arena
                .get(left_idx)
                .and_then(|node| self.ctx.arena.get_literal_expr(node))
                .is_none_or(|array_lit| !array_lit.elements.nodes.is_empty());
            if should_check_iterability {
                let is_iterable =
                    self.check_destructuring_iterability(left_idx, right_type, NodeIndex::NONE);
                is_not_iterable = !is_iterable;
            }
            self.check_array_destructuring_rest_position(left_idx);
            if !is_not_iterable {
                self.check_tuple_destructuring_bounds(left_idx, right_type);
            }
        }

        // TS1186: Check for rest elements with initializers in destructuring assignments.
        if is_destructuring {
            self.check_rest_element_initializer(left_idx);
        }

        // Check readonly — emit TS2540/TS2542 if the target is readonly.
        // tsc suppresses TS2322 for readonly named properties (TS2540) but
        // still emits TS2322 alongside readonly index signatures (TS2542).
        let is_readonly_target = if !is_const {
            self.check_readonly_assignment(left_idx, expr_idx)
        } else {
            false
        };
        // Only suppress assignability for named property readonly (TS2540).
        // For element access (index signatures, TS2542), tsc still checks type compatibility.
        let left_node = self.ctx.arena.get(left_idx);
        let is_element_access =
            left_node.is_some_and(|n| n.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION);
        let suppress_for_readonly = is_readonly_target && !is_element_access;

        if !is_const && self.is_js_namespace_enum_rebind_assignment_target(left_idx) {
            return right_type;
        }

        if !is_const && left_type != TypeId::ANY {
            // For destructuring assignments (both object and array patterns),
            // skip the whole-object assignability check. tsc processes each
            // property/element individually, which correctly handles private
            // members and other access-controlled properties.
            let mut check_assignability = !is_destructuring && !suppress_for_readonly;

            if is_destructuring && !is_not_iterable {
                self.report_abstract_properties_in_destructuring_assignment(left_idx, right_idx);
                self.check_destructuring_property_accessibility(left_idx, right_type);
                // TS2322: Check rest element assignability in object destructuring
                // assignments. For `({ b, ...rest } = source)`, the rest type
                // (source minus named properties) must be assignable to `rest`'s
                // declared type.
                self.check_object_destructuring_rest_assignability(left_idx, right_type);
            }

            if check_assignability {
                let widened_left = tsz_solver::widening::widen_type(self.ctx.types, left_type);
                if widened_left != left_type
                    && let Some(right_node) = self.ctx.arena.get(right_idx)
                {
                    use tsz_parser::parser::syntax_kind_ext;
                    use tsz_scanner::SyntaxKind;
                    if right_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                        && let Some(bin) = self.ctx.arena.get_binary_expr(right_node)
                    {
                        let op = bin.operator_token;
                        let is_compound_like = op == SyntaxKind::PlusToken as u16
                            || op == SyntaxKind::MinusToken as u16
                            || op == SyntaxKind::AsteriskToken as u16
                            || op == SyntaxKind::SlashToken as u16
                            || op == SyntaxKind::PercentToken as u16
                            || op == SyntaxKind::AsteriskAsteriskToken as u16
                            || op == SyntaxKind::LessThanLessThanToken as u16
                            || op == SyntaxKind::GreaterThanGreaterThanToken as u16
                            || op == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16;

                        if is_compound_like && self.is_assignable_to(right_type, widened_left) {
                            check_assignability = false;
                        }
                    }
                }
            }

            self.check_assignment_compatibility(
                left_idx,
                right_idx,
                right_type,
                left_type,
                check_assignability, // check_assignability
                true,
            );

            if left_type != TypeId::UNKNOWN
                && let Some(right_node) = self.ctx.arena.get(right_idx)
                && right_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            {
                self.check_object_literal_excess_properties(right_type, left_type, right_idx);
            }
        }

        right_type
    }

    fn check_tuple_destructuring_bounds(&mut self, left_idx: NodeIndex, right_type: TypeId) {
        let rhs = tsz_solver::type_queries::unwrap_readonly(self.ctx.types, right_type);

        let Some(left_node) = self.ctx.arena.get(left_idx) else {
            return;
        };
        let Some(array_lit) = self.ctx.arena.get_literal_expr(left_node) else {
            return;
        };

        // Single tuple case
        if let Some(tuple_elements) =
            tsz_solver::type_queries::get_tuple_elements(self.ctx.types, rhs)
        {
            let has_rest_tail = tuple_elements.last().is_some_and(|element| element.rest);
            if has_rest_tail {
                return;
            }

            for (index, &element_idx) in array_lit.elements.nodes.iter().enumerate() {
                if index < tuple_elements.len() || element_idx.is_none() {
                    continue;
                }
                let Some(element_node) = self.ctx.arena.get(element_idx) else {
                    continue;
                };
                if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                    continue;
                }
                if element_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                    return;
                }

                let tuple_type_str = self.format_type(rhs);
                self.error_at_node(
                    element_idx,
                    &format!(
                        "Tuple type '{}' of length '{}' has no element at index '{}'.",
                        tuple_type_str,
                        tuple_elements.len(),
                        index
                    ),
                    diagnostic_codes::TUPLE_TYPE_OF_LENGTH_HAS_NO_ELEMENT_AT_INDEX,
                );
                return;
            }
            return;
        }

        // Union of tuples case: check if ALL members are out of bounds
        if let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, rhs) {
            for (index, &element_idx) in array_lit.elements.nodes.iter().enumerate() {
                if element_idx.is_none() {
                    continue;
                }
                let Some(element_node) = self.ctx.arena.get(element_idx) else {
                    continue;
                };
                if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION
                    || element_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                {
                    continue;
                }

                let all_out_of_bounds = !members.is_empty()
                    && members.iter().all(|&m| {
                        let m = tsz_solver::type_queries::unwrap_readonly(self.ctx.types, m);
                        if let Some(elems) =
                            tsz_solver::type_queries::get_tuple_elements(self.ctx.types, m)
                        {
                            let has_rest = elems.iter().any(|e| e.rest);
                            !has_rest && index >= elems.len()
                        } else {
                            false
                        }
                    });

                if all_out_of_bounds {
                    let type_str = self.format_type(right_type);
                    self.error_at_node(
                        element_idx,
                        &format!("Property '{index}' does not exist on type '{type_str}'.",),
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    );
                    return;
                }
            }
        }
    }

    /// TS2462: A rest element in array destructuring must be the last element.
    ///
    /// Enforce syntax for array destructuring assignment targets.
    fn check_array_destructuring_rest_position(&mut self, left_idx: NodeIndex) {
        let Some(left_node) = self.ctx.arena.get(left_idx) else {
            return;
        };
        if left_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return;
        }
        let Some(array_lit) = self.ctx.arena.get_literal_expr(left_node) else {
            return;
        };

        let elements_len = array_lit.elements.nodes.len();
        if elements_len == 0 {
            return;
        }
        for (i, &element_idx) in array_lit.elements.nodes.iter().enumerate() {
            if i + 1 >= elements_len {
                break;
            }
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                self.error_at_node_msg(
                    element_idx,
                    diagnostic_codes::A_REST_ELEMENT_MUST_BE_LAST_IN_A_DESTRUCTURING_PATTERN,
                    &[],
                );
            }
        }
    }

    /// TS1186: A rest element cannot have an initializer.
    ///
    /// In assignment destructuring, `[...x = a] = b` is parsed as a spread of
    /// the assignment expression `x = a`. TypeScript detects this and emits
    /// TS1186 when the spread expression is a binary `=` assignment.
    fn check_rest_element_initializer(&mut self, left_idx: NodeIndex) {
        let Some(left_node) = self.ctx.arena.get(left_idx) else {
            return;
        };

        let elements = if left_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            self.ctx
                .arena
                .get_literal_expr(left_node)
                .map(|lit| &lit.elements.nodes as &[NodeIndex])
        } else if left_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            self.ctx
                .arena
                .get_literal_expr(left_node)
                .map(|lit| &lit.elements.nodes as &[NodeIndex])
        } else {
            None
        };

        let Some(elements) = elements else { return };
        for &element_idx in elements {
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            // Check spread elements and spread assignments
            if element_node.kind != syntax_kind_ext::SPREAD_ELEMENT
                && element_node.kind != syntax_kind_ext::SPREAD_ASSIGNMENT
            {
                continue;
            }
            let spread_expr = self
                .ctx
                .arena
                .get_spread(element_node)
                .map(|s| s.expression)
                .or_else(|| {
                    self.ctx
                        .arena
                        .get_unary_expr_ex(element_node)
                        .map(|u| u.expression)
                });
            let Some(spread_expr) = spread_expr else {
                continue;
            };
            // If the spread expression is a binary assignment (x = a), emit TS1186.
            // tsc anchors this at the `=` operator token, not at the spread element's
            // `...` prefix or the left-hand name. Scan from the left operand's end to
            // find the `=` position.
            if let Some(spread_node) = self.ctx.arena.get(spread_expr)
                && spread_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.ctx.arena.get_binary_expr(spread_node)
                && bin.operator_token == SyntaxKind::EqualsToken as u16
            {
                // Find the `=` token position between left and right operands
                let eq_pos = self.ctx.arena.get(bin.left).map(|left_node| {
                    let search_start = left_node.end as usize;
                    self.ctx
                        .arena
                        .source_files
                        .first()
                        .and_then(|sf| {
                            sf.text[search_start..]
                                .find('=')
                                .map(|offset| (search_start + offset) as u32)
                        })
                        .unwrap_or(left_node.end)
                });
                if let Some(pos) = eq_pos {
                    let message = tsz_common::diagnostics::get_message_template(
                        diagnostic_codes::A_REST_ELEMENT_CANNOT_HAVE_AN_INITIALIZER,
                    )
                    .unwrap_or("");
                    self.error_at_position(
                        pos,
                        1,
                        message,
                        diagnostic_codes::A_REST_ELEMENT_CANNOT_HAVE_AN_INITIALIZER,
                    );
                }
            }
        }
    }

    // =========================================================================
    // Arithmetic Operand Validation
    // =========================================================================

    /// Check if an operand type is valid for arithmetic operations.
    ///
    /// Returns true if the type is number, bigint, any, or an enum type.
    /// This is used to validate operands for TS2362/TS2363 errors.
    fn is_arithmetic_operand(&self, type_id: TypeId) -> bool {
        use tsz_solver::BinaryOpEvaluator;

        // Check if this is an enum type (Lazy/DefId to an enum symbol)
        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            // Check if the symbol is an enum (ENUM flags)
            use tsz_binder::symbol_flags;
            if (symbol.flags & symbol_flags::ENUM) != 0 {
                return true;
            }
        }

        let evaluator = BinaryOpEvaluator::new(self.ctx.types);
        evaluator.is_arithmetic_operand(type_id)
    }

    /// Check and emit TS2362/TS2363 errors for arithmetic operations.
    ///
    /// For operators like -, *, /, %, **, -=, *=, /=, %=, **=,
    /// validates that operands are of type number, bigint, any, or enum.
    /// Emits appropriate errors when operands are invalid.
    /// Returns true if any error was emitted.
    pub(crate) fn check_arithmetic_operands(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        left_type: TypeId,
        right_type: TypeId,
    ) -> bool {
        // Evaluate types to resolve unevaluated conditional/mapped types before checking.
        // e.g. DeepPartial<number> (conditional: number extends object ? ... : number) → number
        let left_eval = self.evaluate_type_for_binary_ops(left_type);
        let right_eval = self.evaluate_type_for_binary_ops(right_type);
        let left_is_valid = self.is_arithmetic_operand(left_eval);
        let right_is_valid = self.is_arithmetic_operand(right_eval);

        // When strictNullChecks is on, null/undefined operands get TS18050 ("The value
        // 'null'/'undefined' cannot be used here") which takes priority over TS2362/TS2363.
        // When strictNullChecks is off, null/undefined are in number's domain and
        // should not trigger arithmetic errors either.
        let left_is_nullish = left_type == TypeId::NULL || left_type == TypeId::UNDEFINED;
        let right_is_nullish = right_type == TypeId::NULL || right_type == TypeId::UNDEFINED;

        let mut emitted = false;

        if !left_is_valid && !(left_is_nullish) {
            self.error_at_node(
                left_idx,
                "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.",
                diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
            );
            emitted = true;
        }

        if !right_is_valid && !(right_is_nullish) {
            self.error_at_node(
                right_idx,
                "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.",
                diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
            );
            emitted = true;
        }

        emitted || !left_is_valid || !right_is_valid
    }

    /// Emit TS2447 error for boolean bitwise operators (&, |, ^, &=, |=, ^=).
    pub(crate) fn emit_boolean_operator_error(
        &mut self,
        node_idx: NodeIndex,
        op_str: &str,
        suggestion: &str,
    ) {
        let message = format!(
            "The '{op_str}' operator is not allowed for boolean types. Consider using '{suggestion}' instead."
        );
        self.error_at_node(
            node_idx,
            &message,
            diagnostic_codes::THE_OPERATOR_IS_NOT_ALLOWED_FOR_BOOLEAN_TYPES_CONSIDER_USING_INSTEAD,
        );
    }

    /// TS2365: Check for bigint/number type mixing in compound assignment operators.
    /// When both operands are individually valid arithmetic types but the binary operation
    /// would fail (e.g., bigint -= number), emit TS2365.
    pub(crate) fn check_compound_assignment_type_compatibility(
        &mut self,
        expr_idx: NodeIndex,
        operator: u16,
        left_read_type: TypeId,
        right_type: TypeId,
        emitted_operator_error: &mut bool,
    ) {
        let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
        let eval_left = self.evaluate_type_for_binary_ops(left_read_type);
        let eval_right = self.evaluate_type_for_binary_ops(right_type);
        if let Some(binary_op) = tsz_solver::map_compound_assignment_to_binary(operator) {
            let result = evaluator.evaluate(eval_left, eval_right, binary_op);
            if let crate::query_boundaries::type_computation::core::BinaryOpResult::TypeError {
                ..
            } = result
            {
                let compound_op_str = match operator {
                    k if k == SyntaxKind::MinusEqualsToken as u16 => "-=",
                    k if k == SyntaxKind::AsteriskEqualsToken as u16 => "*=",
                    k if k == SyntaxKind::SlashEqualsToken as u16 => "/=",
                    k if k == SyntaxKind::PercentEqualsToken as u16 => "%=",
                    k if k == SyntaxKind::AsteriskAsteriskEqualsToken as u16 => "**=",
                    k if k == SyntaxKind::LessThanLessThanEqualsToken as u16 => "<<=",
                    k if k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16 => ">>=",
                    k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16 => {
                        ">>>="
                    }
                    k if k == SyntaxKind::AmpersandEqualsToken as u16 => "&=",
                    k if k == SyntaxKind::BarEqualsToken as u16 => "|=",
                    k if k == SyntaxKind::CaretEqualsToken as u16 => "^=",
                    _ => "?=",
                };
                let left_diag = self.widen_enum_member_type(tsz_solver::widen_literal_type(
                    self.ctx.types,
                    left_read_type,
                ));
                let right_diag = self.widen_enum_member_type(tsz_solver::widen_literal_type(
                    self.ctx.types,
                    right_type,
                ));
                let left_str = self.format_type(left_diag);
                let right_str = self.format_type(right_diag);
                let message = format!(
                    "Operator '{compound_op_str}' cannot be applied to types '{left_str}' and '{right_str}'."
                );
                self.error_at_node(
                    expr_idx,
                    &message,
                    diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPES_AND,
                );
                *emitted_operator_error = true;
            }
        }
    }

    pub(crate) fn check_assignment_compatibility(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        source_type: TypeId,
        target_type: TypeId,
        check_assignability: bool,
        suppress_error_for_error_types: bool,
    ) {
        if let Some((source_level, target_level)) =
            self.constructor_accessibility_mismatch_for_assignment(left_idx, right_idx)
        {
            self.error_constructor_accessibility_not_assignable(
                source_type,
                target_type,
                source_level,
                target_level,
                left_idx,
            );
            return;
        }

        if !check_assignability {
            return;
        }

        if suppress_error_for_error_types
            && (source_type == TypeId::ERROR || target_type == TypeId::ERROR)
        {
            return;
        }

        if let Some(generic_target) =
            self.deferred_generic_element_write_target(left_idx, source_type)
        {
            let _ = self.check_assignable_or_report_at(
                source_type,
                generic_target,
                right_idx,
                left_idx,
            );
            return;
        }

        // TS2322 anchoring should point at the assignment target (LHS), not the RHS expression.
        // This aligns diagnostic fingerprints with tsc for assignment-compatibility suites.
        let _ = self.check_assignable_or_report_at(source_type, target_type, right_idx, left_idx);
    }

    fn deferred_generic_element_write_target(
        &mut self,
        left_idx: NodeIndex,
        source_type: TypeId,
    ) -> Option<TypeId> {
        if source_type == TypeId::ANY
            || source_type == TypeId::NEVER
            || crate::query_boundaries::assignability::contains_type_parameters(
                self.ctx.types,
                source_type,
            )
        {
            return None;
        }

        let node = self.ctx.arena.get(left_idx)?;
        if node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(node)?;
        let object_type = self
            .resolve_identifier_symbol(access.expression)
            .and_then(|sym_id| self.assignment_target_declared_type(sym_id))
            .filter(|declared| {
                tsz_solver::visitor::is_type_parameter(self.ctx.types, *declared)
                    || tsz_solver::visitor::is_this_type(self.ctx.types, *declared)
            })
            .unwrap_or_else(|| self.get_type_of_node(access.expression));
        if !tsz_solver::visitor::is_type_parameter(self.ctx.types, object_type) {
            return None;
        }

        let prev_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let index_type = self.get_type_of_node(access.name_or_argument);
        self.ctx.preserve_literal_types = prev_preserve;

        if !self.is_valid_index_for_type_param(index_type, object_type) {
            return None;
        }

        Some(
            self.ctx
                .types
                .factory()
                .index_access(object_type, index_type),
        )
    }

    fn assignment_target_declared_type(&mut self, sym_id: tsz_binder::SymbolId) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let value_decl = symbol.value_declaration;
        if !value_decl.is_some() {
            return None;
        }

        let node = self.ctx.arena.get(value_decl)?;
        if let Some(param) = self.ctx.arena.get_parameter(node)
            && param.type_annotation.is_some()
        {
            return Some(self.get_type_from_type_node(param.type_annotation));
        }

        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node)
            && var_decl.type_annotation.is_some()
        {
            return Some(self.get_type_from_type_node(var_decl.type_annotation));
        }

        None
    }
}

#[cfg(test)]
#[path = "assignment_checker_tests.rs"]
mod tests;
