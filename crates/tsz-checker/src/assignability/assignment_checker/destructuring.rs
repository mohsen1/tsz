//! Destructuring assignment checking — contextual types, property accessibility,
//! rest assignability, and leaf assignability for destructuring patterns.

use crate::context::TypingRequest;
use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn contextual_type_for_assignment_target(&mut self, target_idx: NodeIndex) -> TypeId {
        let target_type = self.get_type_of_assignment_target(target_idx);
        let target_type = self.resolve_type_query_type(target_type);
        if matches!(
            target_type,
            TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN | TypeId::NEVER
        ) {
            TypeId::ANY
        } else {
            self.contextual_type_for_expression(target_type)
        }
    }

    fn build_contextual_type_from_assignment_pattern(
        &mut self,
        pattern_idx: NodeIndex,
    ) -> Option<TypeId> {
        let pattern_node = self.ctx.arena.get(pattern_idx)?;
        let factory = self.ctx.types.factory();

        if pattern_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            let array = self.ctx.arena.get_literal_expr(pattern_node)?;
            let mut tuple_elements = Vec::with_capacity(array.elements.nodes.len());

            for &elem_idx in &array.elements.nodes {
                let (elem_type, is_rest) = if elem_idx.is_none() {
                    (TypeId::ANY, false)
                } else {
                    let elem_idx = self.ctx.arena.skip_parenthesized_and_assertions(elem_idx);
                    let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                        tuple_elements.push(tsz_solver::TupleElement {
                            type_id: TypeId::ANY,
                            optional: false,
                            rest: false,
                            name: None,
                        });
                        continue;
                    };

                    if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                        let spread_target = self
                            .ctx
                            .arena
                            .get_spread(elem_node)
                            .map(|spread| spread.expression)
                            .unwrap_or(NodeIndex::NONE);
                        let spread_target = self
                            .ctx
                            .arena
                            .skip_parenthesized_and_assertions(spread_target);
                        let spread_target_type =
                            self.contextual_type_for_assignment_target(spread_target);
                        let rest_elem_type = crate::query_boundaries::common::array_element_type(
                            self.ctx.types,
                            spread_target_type,
                        )
                        .unwrap_or(TypeId::ANY);
                        (rest_elem_type, true)
                    } else if elem_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                        || elem_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    {
                        (
                            self.build_contextual_type_from_assignment_pattern(elem_idx)
                                .unwrap_or(TypeId::ANY),
                            false,
                        )
                    } else if elem_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                        (
                            self.ctx
                                .arena
                                .get_binary_expr(elem_node)
                                .filter(|bin| bin.operator_token == SyntaxKind::EqualsToken as u16)
                                .map(|bin| {
                                    let lhs =
                                        self.ctx.arena.skip_parenthesized_and_assertions(bin.left);
                                    if self.ctx.arena.get(lhs).is_some_and(|lhs_node| {
                                        lhs_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                            || lhs_node.kind
                                                == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                    }) {
                                        self.build_contextual_type_from_assignment_pattern(lhs)
                                            .unwrap_or(TypeId::ANY)
                                    } else {
                                        self.contextual_type_for_assignment_target(lhs)
                                    }
                                })
                                .unwrap_or(TypeId::ANY),
                            false,
                        )
                    } else {
                        (self.contextual_type_for_assignment_target(elem_idx), false)
                    }
                };

                tuple_elements.push(tsz_solver::TupleElement {
                    type_id: elem_type,
                    optional: false,
                    rest: is_rest,
                    name: None,
                });
            }

            Some(factory.tuple(tuple_elements))
        } else if pattern_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let object = self.ctx.arena.get_literal_expr(pattern_node)?;
            let mut properties = Vec::new();

            for &elem_idx in &object.elements.nodes {
                let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                    continue;
                };

                if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                    || elem_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
                {
                    continue;
                }

                let (name, target_idx) = if let Some(prop) =
                    self.ctx.arena.get_property_assignment(elem_node)
                {
                    (
                        self.get_property_name_resolved(prop.name),
                        Some(prop.initializer),
                    )
                } else if let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node) {
                    let name = self
                        .ctx
                        .arena
                        .get(shorthand.name)
                        .and_then(|node| self.ctx.arena.get_identifier(node))
                        .map(|ident| ident.escaped_text.clone());
                    // Use the non-tracking resolver: this shorthand appears as a
                    // destructuring-assignment WRITE target (`({x} = expr)`), not a
                    // read. The tracking resolver would mark `x` as referenced and
                    // suppress TS6133 for write-only parameters/locals.
                    let target_idx = self
                        .resolve_identifier_symbol_without_tracking(shorthand.name)
                        .map(|_| shorthand.name);
                    (name, target_idx)
                } else {
                    (None, None)
                };

                let Some(name) = name else {
                    continue;
                };
                let Some(target_idx) = target_idx else {
                    continue;
                };

                let target_idx = self.ctx.arena.skip_parenthesized_and_assertions(target_idx);
                let prop_type = if self.ctx.arena.get(target_idx).is_some_and(|node| {
                    node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                        || node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                }) {
                    self.build_contextual_type_from_assignment_pattern(target_idx)
                        .unwrap_or(TypeId::ANY)
                } else if self
                    .ctx
                    .arena
                    .get(target_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::BINARY_EXPRESSION)
                {
                    self.ctx
                        .arena
                        .get_binary_expr(self.ctx.arena.get(target_idx)?)
                        .filter(|bin| bin.operator_token == SyntaxKind::EqualsToken as u16)
                        .map(|bin| {
                            let lhs = self.ctx.arena.skip_parenthesized_and_assertions(bin.left);
                            if self.ctx.arena.get(lhs).is_some_and(|lhs_node| {
                                lhs_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                    || lhs_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            }) {
                                self.build_contextual_type_from_assignment_pattern(lhs)
                                    .unwrap_or(TypeId::ANY)
                            } else {
                                self.contextual_type_for_assignment_target(lhs)
                            }
                        })
                        .unwrap_or(TypeId::ANY)
                } else {
                    self.contextual_type_for_assignment_target(target_idx)
                };

                let atom = self.ctx.types.intern_string(&name);
                properties.push(tsz_solver::PropertyInfo::new(atom, prop_type));
            }

            Some(factory.object(properties))
        } else {
            None
        }
    }

    pub(crate) fn destructuring_assignment_initializer_request(
        &mut self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) -> TypingRequest {
        let initializer_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(initializer_idx);
        let supports_context = self.ctx.arena.get(initializer_idx).is_some_and(|node| {
            matches!(
                node.kind,
                syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            )
        });

        if !supports_context {
            return TypingRequest::NONE;
        }

        self.build_contextual_type_from_assignment_pattern(pattern_idx)
            .map_or(TypingRequest::NONE, TypingRequest::with_contextual_type)
    }

    pub(crate) fn report_abstract_properties_in_destructuring_assignment(
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

    /// Shared object-destructuring validation for assignment-like contexts.
    ///
    /// This mirrors the object-literal branch of assignment checking without
    /// requiring a concrete assignment-expression RHS node. Callers use it when
    /// the source value is already available as a type, such as `for ({...} of xs)`.
    pub(crate) fn check_object_destructuring_assignment_from_source_type(
        &mut self,
        pattern_idx: NodeIndex,
        source_type: TypeId,
        source_expr_idx: Option<NodeIndex>,
    ) {
        if source_type == TypeId::ANY || source_type == TypeId::ERROR {
            return;
        }

        if let Some(source_expr_idx) = source_expr_idx {
            self.report_abstract_properties_in_destructuring_assignment(
                pattern_idx,
                source_expr_idx,
            );
        }

        self.check_destructuring_property_accessibility(pattern_idx, source_type);
        self.check_object_destructuring_rest_assignability(pattern_idx, source_type);
    }

    /// TS2322: Check assignability of the rest element in an object destructuring
    /// assignment. For `({ b, ...rest } = source)`, computes the rest type
    /// (source minus named properties) and checks it against the rest target's
    /// declared type.
    pub(crate) fn check_object_destructuring_rest_assignability(
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

        // Skip assignability check when the rest target is an optional property
        // access (e.g., `{ ...obj?.a } = source`). TS2778 is already emitted for
        // this pattern, and the flow-narrowed type of an invalid optional chain
        // target can be incorrect (narrowed by a prior invalid assignment like
        // `obj?.a = 1`), leading to false TS2322 errors.
        if self.is_optional_chain_access(spread_expr) {
            return;
        }

        // Get the declared type of the rest target variable
        let rest_target_type = self.get_type_of_assignment_target(spread_expr);
        if rest_target_type == TypeId::ANY || rest_target_type == TypeId::ERROR {
            return;
        }

        // Compute the rest type: source minus named properties
        let rest_type = self.omit_properties_from_type(source_type, &named_properties);

        // Check assignability. Anchor the diagnostic at the rest target
        // identifier exactly — tsc reports TS2322 at `notAssignable` in
        // `({ b, ...notAssignable } = o)`, not at the enclosing binary
        // assignment expression. Using the plain `check_assignable_or_report`
        // walks up to the assignment and anchors at its start (col 1 of `(`).
        self.ensure_relation_input_ready(rest_type);
        self.ensure_relation_input_ready(rest_target_type);

        let _ = self.check_assignable_or_report_at_exact_anchor(
            rest_type,
            rest_target_type,
            spread_expr,
            spread_expr,
        );
    }

    /// TS2341/TS2445: Check private/protected accessibility for properties
    /// accessed in destructuring assignment patterns.
    ///
    /// In `{ o: target } = source`, property `o` is accessed on the source type
    /// and must respect visibility modifiers. This walks the destructuring
    /// pattern recursively and checks each property name against the source type.
    pub(crate) fn check_destructuring_property_accessibility(
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
                                {
                                    if lhs_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                        || lhs_node.kind
                                            == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                    {
                                        let prop_type = self
                                            .resolve_property_type_for_destructuring(
                                                source_type,
                                                &name,
                                            );
                                        if let Some(prop_type) = prop_type {
                                            self.check_destructuring_property_accessibility(
                                                bin.left, prop_type,
                                            );
                                        }
                                    } else {
                                        // { name: target = default } — non-shorthand property
                                        // with default value.
                                        // tsc's behavior: if the default value type itself is NOT
                                        // assignable to the target, only report the default mismatch
                                        // (handled by the binary expression checker), skip the full
                                        // property type check. If the default IS assignable but the
                                        // source property type is NOT, report the property type error.
                                        let default_type = self.get_type_of_node(bin.right);
                                        let target_type =
                                            self.get_type_of_assignment_target(bin.left);
                                        let default_assignable = target_type == TypeId::ANY
                                            || target_type == TypeId::ERROR
                                            || default_type == TypeId::ANY
                                            || default_type == TypeId::ERROR
                                            || self.is_assignable_to(default_type, target_type);
                                        if default_assignable {
                                            // Default is fine but property type might not be.
                                            self.check_destructuring_leaf_assignability_with_default(
                                                &name,
                                                source_type,
                                                bin.left,
                                                bin.right,
                                            );
                                        }
                                        // else: default value already fails — the binary expression
                                        // checker will report the error, skip redundant property check.
                                    }
                                }
                            } else {
                                // { name: target } — leaf target without default.
                                // Check that the source property type is assignable to the
                                // target's declared type.
                                self.check_destructuring_leaf_assignability(
                                    &name,
                                    source_type,
                                    prop.initializer,
                                );
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
                        // Destructuring-assignment target: `x` is a WRITE target,
                        // not a read. Use non-tracking resolver so write-only
                        // parameters still emit TS6133.
                        let has_value_binding = self
                            .resolve_identifier_symbol_without_tracking(shorthand.name)
                            .is_some();
                        // TS2322: Check that the source property type is assignable
                        // to the target variable's declared type. This catches cases
                        // like `({ q } = numMapPoint)` where `q` comes from an index
                        // signature and `noUncheckedIndexedAccess` adds `| undefined`.
                        // When a default value is present (`{ x = 1 }`), narrow the
                        // source property type by stripping `undefined` before checking
                        // assignability, since the default handles the absent/undefined case.
                        if shorthand.equals_token && has_value_binding {
                            self.check_destructuring_leaf_assignability_with_default(
                                &ident.escaped_text,
                                source_type,
                                shorthand.name,
                                shorthand.object_assignment_initializer,
                            );
                        } else if has_value_binding {
                            self.check_destructuring_leaf_assignability(
                                &ident.escaped_text,
                                source_type,
                                shorthand.name,
                            );
                        }
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
                    let Some(mut elem_type) = elem_type else {
                        continue;
                    };
                    // With noUncheckedIndexedAccess, array element access may not
                    // exist at runtime — include `| undefined` in the element type.
                    if self.ctx.no_unchecked_indexed_access() {
                        elem_type = crate::query_boundaries::flow::add_undefined_for_indexed_access(
                            self.ctx.types,
                            elem_type,
                        );
                    }
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
                        //
                        // Skip when the spread target is an optional chain access
                        // (e.g., `[...obj?.a] = []`). TS2779 is already emitted for
                        // this pattern, and the flow-narrowed type of the target can
                        // be incorrect from a prior invalid assignment, producing
                        // false TS2322.
                        let target_type = self.get_type_of_assignment_target(target_idx);
                        if target_type != TypeId::ANY
                            && target_type != TypeId::ERROR
                            && check_type != TypeId::ANY
                            && !self.is_optional_chain_access(target_idx)
                        {
                            self.check_assignable_or_report_at_exact_anchor(
                                check_type,
                                target_type,
                                target_idx,
                                target_idx,
                            );
                        }
                    } else if self.ctx.no_unchecked_indexed_access() && !is_spread {
                        // With noUncheckedIndexedAccess, the element type includes
                        // `| undefined`. Check that this augmented type is assignable
                        // to the target variable's declared type. E.g.,
                        // `[target_string] = strArray` should error when
                        // `target_string: string` but element type is `string | undefined`.
                        let target_type = self.get_type_of_assignment_target(target_idx);
                        if target_type != TypeId::ANY
                            && target_type != TypeId::ERROR
                            && check_type != TypeId::ANY
                            && check_type != TypeId::ERROR
                        {
                            self.ensure_relation_input_ready(check_type);
                            self.ensure_relation_input_ready(target_type);
                            if !self.is_assignable_to(check_type, target_type) {
                                // Emit TS2322 directly with pre-resolved types.
                                // The standard error pipeline would re-derive
                                // the source type from the anchor node's parent
                                // assignment, incorrectly showing the RHS array
                                // type instead of the element type.
                                let source_str = self.format_type_diagnostic(check_type);
                                let target_str = self.format_type_diagnostic(target_type);
                                let message = crate::diagnostics::format_message(
                                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                                    &[&source_str, &target_str],
                                );
                                self.error_at_node(
                                    target_idx,
                                    &message,
                                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                                );
                            }
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
            crate::query_boundaries::common::tuple_elements(self.ctx.types, source_type)
        {
            if index < elems.len() {
                return Some(elems[index].type_id);
            }
            return None;
        }
        // Fall back to array element type
        crate::query_boundaries::common::array_element_type(self.ctx.types, source_type)
    }

    /// TS2322: Check that a source property type is assignable to a leaf
    /// destructuring target's declared type.
    ///
    /// For `{ name: target }` or `{ name: target = default }`, resolve the
    /// property type from `source_type` and check it against `target_idx`'s
    /// declared type. This catches cases like:
    ///   `var a: string; [...{ 0: a = "" }] = ["", 1];`
    /// where property "0" has type `string | number` which is not assignable
    /// to `a: string`.
    fn check_destructuring_leaf_assignability(
        &mut self,
        property_name: &str,
        source_type: TypeId,
        target_idx: NodeIndex,
    ) {
        self.check_destructuring_leaf_assignability_impl(
            property_name,
            source_type,
            target_idx,
            NodeIndex::NONE,
        );
    }

    fn check_destructuring_leaf_assignability_with_default(
        &mut self,
        property_name: &str,
        source_type: TypeId,
        target_idx: NodeIndex,
        default_expr: NodeIndex,
    ) {
        self.check_destructuring_leaf_assignability_impl(
            property_name,
            source_type,
            target_idx,
            default_expr,
        );
    }

    fn check_destructuring_leaf_assignability_impl(
        &mut self,
        property_name: &str,
        source_type: TypeId,
        target_idx: NodeIndex,
        default_expr: NodeIndex,
    ) {
        let has_default = default_expr.is_some();
        let target_type = self.get_type_of_assignment_target(target_idx);
        if target_type == TypeId::ANY || target_type == TypeId::ERROR {
            return;
        }
        if has_default {
            let default_type = self
                .literal_type_from_initializer(default_expr)
                .or_else(|| self.numeric_literal_type_from_text(default_expr))
                .unwrap_or_else(|| self.get_type_of_node(default_expr));
            if default_type != TypeId::ANY
                && default_type != TypeId::ERROR
                && !self.is_assignable_to(default_type, target_type)
            {
                if self.try_report_object_default_property_mismatch(
                    default_expr,
                    target_idx,
                    target_type,
                ) {
                    return;
                }
                let source_for_display = {
                    let widened = self.widen_literal_type(default_type);
                    if widened == TypeId::NUMBER {
                        widened
                    } else {
                        default_type
                    }
                };
                let source_str = self.format_type_diagnostic(source_for_display);
                let target_str = self.format_type_diagnostic(target_type);
                let message = crate::diagnostics::format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                self.error_at_node(
                    target_idx,
                    &message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
        }

        // Resolve the source property type. For numeric property names on
        // tuple/array types, use element type access directly to avoid
        // TypeId mismatches from string-based property resolution.
        let prop_type = if let Ok(index) = property_name.parse::<usize>() {
            self.resolve_element_type_for_destructuring(source_type, index)
        } else {
            self.resolve_property_type_for_destructuring(source_type, property_name)
        };
        let Some(mut prop_type) = prop_type else {
            return;
        };
        if prop_type == TypeId::ANY || prop_type == TypeId::ERROR {
            return;
        }
        // When a default value is present, compute the effective destructured
        // type. The default only contributes when the source property can
        // actually be `undefined`; otherwise the runtime value always comes from
        // the source property itself.
        // This matches tsc behavior:
        //   `({ x = 0 } = a)` where `a.x` is `number | undefined`:
        //     effective = number | number = number → assignable to number ✓
        //   `({ x = 0 } = a)` where `a.x` is `boolean`:
        //     effective = boolean → NOT assignable to number ✗
        //   `({ x = undefined } = a)` where `a.x` is `number | undefined`:
        //     effective = number | undefined → NOT assignable to number ✗
        if has_default
            && self.ctx.compiler_options.strict_null_checks
            && crate::query_boundaries::common::type_contains_undefined(self.ctx.types, prop_type)
        {
            let non_undefined = crate::query_boundaries::flow::narrow_destructuring_default(
                self.ctx.types,
                prop_type,
                true,
            );
            let default_type = self
                .literal_type_from_initializer(default_expr)
                .or_else(|| self.numeric_literal_type_from_text(default_expr))
                .unwrap_or_else(|| self.get_type_of_node(default_expr));
            let factory = self.ctx.types.factory();
            prop_type = if non_undefined == TypeId::NEVER {
                default_type
            } else {
                factory.union2(non_undefined, default_type)
            };
            if prop_type == TypeId::ANY || prop_type == TypeId::ERROR {
                return;
            }
        }
        // Ensure both types are fully resolved before relation checking.
        self.ensure_relation_input_ready(prop_type);
        self.ensure_relation_input_ready(target_type);
        if self.is_assignable_to(prop_type, target_type) {
            return;
        }
        // Emit TS2322 directly. Format source type from the TypeId rather
        // than from the anchor node — the anchor is the assignment target,
        // not the source expression. Using the standard error pipeline
        // with the target node would incorrectly resolve the target's own
        // type as the source display string.
        let source_for_display = {
            let widened = self.widen_literal_type(prop_type);
            if widened == TypeId::NUMBER {
                widened
            } else {
                prop_type
            }
        };
        let source_str = self.format_type_diagnostic(source_for_display);
        let target_str = self.format_type_diagnostic(target_type);
        let message = crate::diagnostics::format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        self.error_at_node(
            target_idx,
            &message,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );
    }

    fn try_report_object_default_property_mismatch(
        &mut self,
        default_expr: NodeIndex,
        target_idx: NodeIndex,
        target_type: TypeId,
    ) -> bool {
        let Some(default_node) = self.ctx.arena.get(default_expr) else {
            return false;
        };
        if default_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(object_lit) = self.ctx.arena.get_literal_expr(default_node) else {
            return false;
        };
        for &elem_idx in &object_lit.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            if elem_node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
                continue;
            }
            let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
                continue;
            };
            let Some(name) = self.property_name_text(prop.name) else {
                continue;
            };
            let source_type = self
                .literal_type_from_initializer(prop.initializer)
                .or_else(|| self.numeric_literal_type_from_text(prop.initializer))
                .unwrap_or_else(|| self.get_type_of_node(prop.initializer));
            let Some(target_prop_type) =
                self.resolve_property_type_for_destructuring(target_type, &name)
            else {
                continue;
            };
            if self.is_assignable_to(source_type, target_prop_type) {
                continue;
            }
            let source_for_display = {
                let widened = self.widen_literal_type(source_type);
                if widened == TypeId::NUMBER {
                    widened
                } else {
                    source_type
                }
            };
            let source_str = self.format_type_diagnostic(source_for_display);
            let target_str = self.format_type_diagnostic(target_prop_type);
            let message = crate::diagnostics::format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            );
            let anchor = self
                .target_type_literal_property_anchor(target_idx, &name)
                .unwrap_or(target_idx);
            if anchor == target_idx {
                if let Some(pos) = self.target_inline_type_property_position(target_idx, &name) {
                    self.error_at_position(
                        pos,
                        name.len().max(1) as u32,
                        &message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                } else {
                    self.error_at_node(
                        anchor,
                        &message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }
            } else {
                self.error_at_node(
                    anchor,
                    &message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
            return true;
        }
        false
    }

    fn target_inline_type_property_position(
        &self,
        target_idx: NodeIndex,
        property_name: &str,
    ) -> Option<u32> {
        let node = self.ctx.arena.get(target_idx)?;
        let source = self.ctx.arena.source_files.first()?.text.as_ref();
        let start = node.end as usize;
        let line_end = source[start..]
            .find(['\n', '\r'])
            .map(|offset| start + offset)
            .unwrap_or(source.len());
        Self::inline_type_property_offset(&source[start..line_end], property_name)
            .map(|offset| (start + offset) as u32)
    }

    fn inline_type_property_offset(line: &str, property_name: &str) -> Option<usize> {
        if property_name.is_empty() {
            return None;
        }
        let mut search_start = 0usize;
        while let Some(offset) = line[search_start..].find(property_name) {
            let match_start = search_start + offset;
            let match_end = match_start + property_name.len();
            let before_ok = match_start == 0
                || !Self::is_inline_type_identifier_char(line.as_bytes()[match_start - 1]);
            let after_ok = match_end >= line.len()
                || !Self::is_inline_type_identifier_char(line.as_bytes()[match_end]);
            if before_ok && after_ok {
                return Some(match_start);
            }
            search_start = match_end;
        }

        None
    }

    const fn is_inline_type_identifier_char(ch: u8) -> bool {
        ch.is_ascii_alphanumeric() || ch == b'_' || ch == b'$'
    }

    fn property_name_text(&self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.ctx.arena.get(name_idx)?;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some(ident.escaped_text.to_string());
        }
        self.ctx
            .arena
            .get_literal(name_node)
            .map(|lit| lit.text.clone())
    }

    fn numeric_literal_type_from_text(&self, idx: NodeIndex) -> Option<TypeId> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind != SyntaxKind::NumericLiteral as u16 {
            return None;
        }
        let lit = self.ctx.arena.get_literal(node)?;
        tsz_common::numeric::parse_numeric_literal_value(&lit.text)
            .map(|value| self.ctx.types.literal_number(value))
    }

    fn target_type_literal_property_anchor(
        &mut self,
        target_idx: NodeIndex,
        property_name: &str,
    ) -> Option<NodeIndex> {
        let target_idx =
            self.resolve_identifier_symbol_without_tracking(target_idx)
                .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                .and_then(|symbol| {
                    std::iter::once(symbol.value_declaration)
                        .chain(symbol.declarations.iter().copied())
                        .find(|&decl| {
                            self.ctx.arena.get(decl).is_some_and(|node| {
                                node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                            })
                        })
                        .or(Some(symbol.value_declaration))
                })
                .unwrap_or(target_idx);
        let target_node = self.ctx.arena.get(target_idx)?;
        let parent_node = if target_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            target_node
        } else if target_node.kind == SyntaxKind::Identifier as u16 {
            let parent = self.ctx.arena.node_info(target_idx)?.parent;
            self.ctx.arena.get(parent)?
        } else {
            return None;
        };
        let var_decl = self.ctx.arena.get_variable_declaration(parent_node)?;
        let type_node = self.ctx.arena.get(var_decl.type_annotation)?;
        let type_lit = self.ctx.arena.get_type_literal(type_node)?;
        for &member_idx in &type_lit.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                continue;
            }
            let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                continue;
            };
            if self.property_name_text(prop.name).as_deref() == Some(property_name) {
                return Some(prop.name);
            }
        }
        None
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

        // TS2538: When the key type is `any` or an error type, emit
        // "Type 'any' cannot be used as an index type" — matching tsc's
        // behavior for invalid computed property keys in destructuring
        // assignments (e.g., `[{[foo()]: x}] = [obj]` where foo() is
        // not callable and resolves to `any`).
        if key_type == TypeId::ANY || key_type == TypeId::ERROR {
            let display_type = if key_type == TypeId::ERROR {
                TypeId::ANY
            } else {
                key_type
            };
            let key_type_str = {
                let mut formatter = self.ctx.create_type_formatter();
                formatter.format(display_type).into_owned()
            };
            let message = crate::diagnostics::format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                &[&key_type_str],
            );
            self.error_at_node(
                computed.expression,
                &message,
                diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
            );
            return;
        }

        // TS2538: Check for other invalid index types (void, boolean, object, etc.)
        let key_is_string = key_type == TypeId::STRING;
        let key_is_number = key_type == TypeId::NUMBER;
        if !key_is_string && !key_is_number {
            let is_invalid =
                crate::query_boundaries::type_checking_utilities::get_invalid_index_type_member_strict(
                    self.ctx.types,
                    key_type,
                );
            if let Some(err_type) = is_invalid {
                let key_type_str = {
                    let mut formatter = self.ctx.create_type_formatter();
                    formatter.format(err_type).into_owned()
                };
                let message = crate::diagnostics::format_message(
                    diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                    &[&key_type_str],
                );
                self.error_at_node(
                    computed.expression,
                    &message,
                    diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                );
            }
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
        use crate::query_boundaries::common::IntrinsicKind;
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
            crate::query_boundaries::common::tuple_elements(self.ctx.types, source_type)
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
}

#[cfg(test)]
mod tests {
    use super::CheckerState;

    #[test]
    fn inline_type_property_offset_uses_whole_identifier_match() {
        let line = "{ foobar: number; foo: string }";

        assert_eq!(
            CheckerState::inline_type_property_offset(line, "foo"),
            line.find("foo: string")
        );
    }

    #[test]
    fn inline_type_property_offset_rejects_identifier_continuations() {
        assert_eq!(
            CheckerState::inline_type_property_offset("{ $foo: string }", "foo"),
            None
        );
        assert_eq!(
            CheckerState::inline_type_property_offset("{ foo_bar: string }", "foo"),
            None
        );
    }

    #[test]
    fn inline_type_property_offset_returns_none_for_empty_property_name() {
        // Guard against an infinite loop when property_name is the empty string:
        // `find("")` returns Some(0), and match_end == match_start would never advance
        // search_start if the byte at match_end happened to be an identifier char.
        assert_eq!(
            CheckerState::inline_type_property_offset("{ a: string }", ""),
            None
        );
        assert_eq!(CheckerState::inline_type_property_offset("", ""), None);
    }
}
