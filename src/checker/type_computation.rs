//! Type Computation Module
//!
//! This module contains type computation methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Type computation helpers
//! - Type relationship queries
//! - Type format utilities
//!
//! This module extends CheckerState with additional methods for type-related
//! operations, providing cleaner APIs for common patterns.

use crate::checker::state::CheckerState;
use crate::checker::types::{Diagnostic, DiagnosticCategory};
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::solver::{ContextualTypeContext, TupleElement, TypeId, TypeKey};

// =============================================================================
// Type Computation Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Core Type Computation
    // =========================================================================

    /// Get the type of a conditional expression (ternary operator).
    ///
    /// Computes the type of `condition ? whenTrue : whenFalse`.
    /// Returns the union of the two branch types if they differ.
    ///
    /// When a contextual type is available, each branch is checked against it
    /// to catch type errors (TS2322).
    pub(crate) fn get_type_of_conditional_expression(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(cond) = self.ctx.arena.get_conditional_expr(node) else {
            return TypeId::ERROR;
        };

        // Apply contextual typing to each branch and check assignability
        let prev_context = self.ctx.contextual_type;
        let (when_true, when_false) = if let Some(contextual) = prev_context {
            // Check whenTrue branch against contextual type
            self.ctx.contextual_type = Some(contextual);
            let when_true = self.get_type_of_node(cond.when_true);

            // Check whenFalse branch against contextual type
            self.ctx.contextual_type = Some(contextual);
            let when_false = self.get_type_of_node(cond.when_false);

            self.ctx.contextual_type = prev_context;
            (when_true, when_false)
        } else {
            // No contextual type - just compute branch types
            let when_true = self.get_type_of_node(cond.when_true);
            let when_false = self.get_type_of_node(cond.when_false);
            (when_true, when_false)
        };

        if when_true == when_false {
            when_true
        } else {
            // Use TypeInterner's union method for automatic normalization
            self.ctx.types.union(vec![when_true, when_false])
        }
    }

    /// Get type of array literal.
    ///
    /// Computes the type of array literals like `[1, 2, 3]` or `["a", "b"]`.
    /// Handles:
    /// - Empty arrays (infer from context or use never[])
    /// - Tuple contexts (e.g., `[string, number]`)
    /// - Spread elements (`[...arr]`)
    /// - Common type inference for mixed elements
    pub(crate) fn get_type_of_array_literal(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(array) = self.ctx.arena.get_literal_expr(node) else {
            return TypeId::ERROR;
        };

        if array.elements.nodes.is_empty() {
            // Empty array literal: infer from context or use never[]
            if let Some(contextual) = self.ctx.contextual_type {
                return contextual;
            }
            return self.ctx.types.array(TypeId::NEVER);
        }

        let tuple_context = match self.ctx.contextual_type {
            Some(ctx_type) => match self.ctx.types.lookup(ctx_type) {
                Some(TypeKey::Tuple(elements)) => {
                    let elements = self.ctx.types.tuple_list(elements);
                    Some(elements.as_ref().to_vec())
                }
                _ => None,
            },
            None => None,
        };

        let ctx_helper = if let Some(ctx_type) = self.ctx.contextual_type {
            Some(ContextualTypeContext::with_expected(
                self.ctx.types,
                ctx_type,
            ))
        } else {
            None
        };

        // Get types of all elements, applying contextual typing when available.
        let mut element_types = Vec::new();
        let mut tuple_elements = Vec::new();
        for (index, &elem_idx) in array.elements.nodes.iter().enumerate() {
            if elem_idx.is_none() {
                continue;
            }

            let prev_context = self.ctx.contextual_type;
            if let Some(ref helper) = ctx_helper {
                if tuple_context.is_some() {
                    self.ctx.contextual_type = helper.get_tuple_element_type(index);
                } else {
                    self.ctx.contextual_type = helper.get_array_element_type();
                }
            }

            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let elem_is_spread = elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT;

            // Handle spread elements - expand tuple types
            if elem_is_spread {
                if let Some(spread_data) = self.ctx.arena.get_spread(elem_node) {
                    let spread_expr_type = self.get_type_of_node(spread_data.expression);
                    // Check if spread argument is iterable, emit TS2488 if not
                    self.check_spread_iterability(spread_expr_type, spread_data.expression);

                    // If it's a tuple type, expand its elements
                    if let Some(TypeKey::Tuple(elems_id)) = self.ctx.types.lookup(spread_expr_type)
                    {
                        let elems = self.ctx.types.tuple_list(elems_id);

                        if let Some(ref expected) = tuple_context {
                            // For tuple context, add each element with spread flag
                            for elem in elems.iter() {
                                let (name, optional) =
                                    match tuple_context.as_ref().and_then(|tc| tc.get(index)) {
                                        Some(el) => (el.name, el.optional),
                                        None => (None, false),
                                    };
                                tuple_elements.push(TupleElement {
                                    type_id: elem.type_id,
                                    name,
                                    optional,
                                    rest: false, // Individual tuple elements are not spreads
                                });
                                // Don't increment index here - each tuple element maps to position
                            }
                        } else {
                            // For array context, add element types
                            for elem in elems.iter() {
                                element_types.push(elem.type_id);
                            }
                        }
                        self.ctx.contextual_type = prev_context;
                        continue;
                    }

                    // For non-tuple spreads in array context, use element type
                    // For tuple context, use the spread type itself
                    let elem_type = if tuple_context.is_some() {
                        spread_expr_type
                    } else {
                        self.for_of_element_type(spread_expr_type)
                    };

                    self.ctx.contextual_type = prev_context;

                    if let Some(ref expected) = tuple_context {
                        let (name, optional) =
                            match tuple_context.as_ref().and_then(|tc| tc.get(index)) {
                                Some(el) => (el.name, el.optional),
                                None => (None, false),
                            };
                        tuple_elements.push(TupleElement {
                            type_id: elem_type,
                            name,
                            optional,
                            rest: true, // Mark as spread for non-tuple spreads in tuple context
                        });
                    } else {
                        element_types.push(elem_type);
                    }
                    continue;
                }
            }

            // Regular (non-spread) element
            let elem_type = self.get_type_of_node(elem_idx);

            self.ctx.contextual_type = prev_context;

            if let Some(ref expected) = tuple_context {
                let (name, optional) = match tuple_context.as_ref().and_then(|tc| tc.get(index)) {
                    Some(el) => (el.name, el.optional),
                    None => (None, false),
                };
                tuple_elements.push(TupleElement {
                    type_id: elem_type,
                    name,
                    optional,
                    rest: false,
                });
            } else {
                element_types.push(elem_type);
            }
        }

        if tuple_context.is_some() {
            return self.ctx.types.tuple(tuple_elements);
        }

        // Use contextual element type when available for better inference
        if let Some(ref helper) = ctx_helper
            && let Some(context_element_type) = helper.get_array_element_type()
        {
            // Check if all elements are assignable to the contextual type
            // If so, use the contextual type for the array
            if element_types
                .iter()
                .all(|&elem_type| self.is_assignable_to(elem_type, context_element_type))
            {
                return self.ctx.types.array(context_element_type);
            }
        }

        // Choose a best common type if any element is a supertype of all others.
        let element_type = if element_types.len() == 1 {
            // For single element, prefer contextual type if available
            if let Some(ref helper) = ctx_helper
                && let Some(context_element_type) = helper.get_array_element_type()
                && self.is_assignable_to(element_types[0], context_element_type)
            {
                context_element_type
            } else {
                element_types[0]
            }
        } else if element_types.is_empty() {
            TypeId::NEVER
        } else {
            let mut best = None;
            'candidates: for &candidate in &element_types {
                for &elem in &element_types {
                    if !self.is_assignable_to(elem, candidate) {
                        continue 'candidates;
                    }
                }
                best = Some(candidate);
                break;
            }
            best.unwrap_or_else(|| self.ctx.types.union(element_types))
        };

        self.ctx.types.array(element_type)
    }

    /// Get type of prefix unary expression.
    ///
    /// Computes the type of unary expressions like `!x`, `+x`, `-x`, `~x`, `++x`, `--x`, `typeof x`.
    /// Returns boolean for `!`, number for arithmetic operators, string for `typeof`.
    pub(crate) fn get_type_of_prefix_unary(&mut self, idx: NodeIndex) -> TypeId {
        use crate::scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(unary) = self.ctx.arena.get_unary_expr(node) else {
            return TypeId::ERROR;
        };

        match unary.operator {
            // ! returns boolean
            k if k == SyntaxKind::ExclamationToken as u16 => TypeId::BOOLEAN,
            // typeof returns string but still type-check operand for flow/node types.
            k if k == SyntaxKind::TypeOfKeyword as u16 => {
                self.get_type_of_node(unary.operand);
                TypeId::STRING
            }
            // Unary + and - return number unless contextual typing expects a numeric literal.
            k if k == SyntaxKind::PlusToken as u16 || k == SyntaxKind::MinusToken as u16 => {
                if let Some(literal_type) = self.literal_type_from_initializer(idx)
                    && self.contextual_literal_type(literal_type).is_some()
                {
                    return literal_type;
                }
                TypeId::NUMBER
            }
            // ~ returns number
            k if k == SyntaxKind::TildeToken as u16 => TypeId::NUMBER,
            // ++ and -- require numeric operand
            k if k == SyntaxKind::PlusPlusToken as u16
                || k == SyntaxKind::MinusMinusToken as u16 =>
            {
                // Get operand type for validation
                let operand_type = self.get_type_of_node(unary.operand);

                // Check if operand is valid for increment/decrement (number, bigint, any, or enum)
                use crate::solver::BinaryOpEvaluator;
                let evaluator = BinaryOpEvaluator::new(self.ctx.types);
                let is_valid = evaluator.is_arithmetic_operand(operand_type);

                if !is_valid {
                    // Emit TS2362 for invalid increment/decrement operand
                    if let Some(loc) = self.get_source_location(unary.operand) {
                        use crate::checker::types::diagnostics::diagnostic_codes;
                        self.ctx.diagnostics.push(Diagnostic {
                            code: diagnostic_codes::LEFT_HAND_SIDE_OF_ARITHMETIC_MUST_BE_NUMBER,
                            category: DiagnosticCategory::Error,
                            message_text: "The operand of an increment or decrement operator must be a variable or a property access.".to_string(),
                            file: self.ctx.file_name.clone(),
                            start: loc.start,
                            length: loc.length(),
                            related_information: Vec::new(),
                        });
                    }
                }

                TypeId::NUMBER
            }
            _ => TypeId::ANY,
        }
    }

    /// Get type of template expression (template literal with substitutions).
    ///
    /// Type-checks all expressions within template spans to emit errors like TS2304.
    /// Template expressions always produce string type.
    pub(crate) fn get_type_of_template_expression(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::STRING;
        };

        let Some(template) = self.ctx.arena.get_template_expr(node) else {
            return TypeId::STRING;
        };

        // Type-check each template span's expression
        for &span_idx in &template.template_spans.nodes {
            let Some(span_node) = self.ctx.arena.get(span_idx) else {
                continue;
            };

            let Some(span) = self.ctx.arena.get_template_span(span_node) else {
                continue;
            };

            // Type-check the expression - this will emit TS2304 if name is unresolved
            self.get_type_of_node(span.expression);
        }

        // Template expressions always produce string type
        TypeId::STRING
    }

    /// Get type of variable declaration.
    ///
    /// Computes the type of variable declarations like `let x: number = 5` or `const y = "hello"`.
    /// Returns the type annotation if present, otherwise infers from the initializer.
    pub(crate) fn get_type_of_variable_declaration(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return TypeId::ERROR;
        };

        // First check type annotation - this takes precedence
        if !var_decl.type_annotation.is_none() {
            return self.get_type_from_type_node(var_decl.type_annotation);
        }

        if self.is_catch_clause_variable_declaration(idx)
            && self.ctx.use_unknown_in_catch_variables()
        {
            return TypeId::UNKNOWN;
        }

        // Infer from initializer
        if !var_decl.initializer.is_none() {
            return self.get_type_of_node(var_decl.initializer);
        }

        // No initializer - use UNKNOWN to enforce strict checking
        // This requires explicit type annotation or prevents unsafe usage
        TypeId::UNKNOWN
    }

    /// Get the type of an assignment target without definite assignment checks.
    ///
    /// Computes the type of the left-hand side of an assignment expression.
    /// Handles identifier resolution and type-only alias checking.
    pub(crate) fn get_type_of_assignment_target(&mut self, idx: NodeIndex) -> TypeId {
        use crate::scanner::SyntaxKind;

        if let Some(node) = self.ctx.arena.get(idx)
            && node.kind == SyntaxKind::Identifier as u16
            && let Some(sym_id) = self.resolve_identifier_symbol(idx)
        {
            if self.alias_resolves_to_type_only(sym_id) {
                if let Some(ident) = self.ctx.arena.get_identifier(node) {
                    self.error_type_only_value_at(&ident.escaped_text, idx);
                }
                return TypeId::ERROR;
            }
            let declared_type = self.get_type_of_symbol(sym_id);
            return declared_type;
        }

        self.get_type_of_node(idx)
    }

    /// Get the type of a property access when we know the property name.
    ///
    /// This is used for private member access when symbols resolution fails
    /// but the property exists in the object type.
    pub(crate) fn get_type_of_property_access_by_name(
        &mut self,
        idx: NodeIndex,
        access: &crate::parser::node::AccessExprData,
        object_type: TypeId,
        property_name: &str,
    ) -> TypeId {
        use crate::solver::{PropertyAccessResult, QueryDatabase};

        let object_type = self.resolve_type_for_property_access(object_type);
        let result_type = match self
            .ctx
            .types
            .property_access_type(object_type, property_name)
        {
            PropertyAccessResult::Success {
                type_id,
                from_index_signature,
            } => {
                if from_index_signature {
                    self.error_property_not_exist_at(property_name, object_type, idx);
                    return TypeId::ERROR;
                }
                type_id
            }
            PropertyAccessResult::PropertyNotFound { .. } => {
                // Don't emit TS2339 for private fields (starting with #) - they're handled elsewhere
                if !property_name.starts_with('#') {
                    self.error_property_not_exist_at(property_name, object_type, idx);
                }
                TypeId::ERROR
            }
            PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                property_type.unwrap_or(TypeId::UNKNOWN)
            }
            PropertyAccessResult::IsUnknown => {
                // TS2571: Object is of type 'unknown'
                use crate::checker::types::diagnostics::diagnostic_codes;
                self.error_at_node(
                    access.expression,
                    "Object is of type 'unknown'.",
                    diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN,
                );
                TypeId::ERROR
            }
        };

        // Handle nullish coercion
        if access.question_dot_token {
            self.ctx.types.union(vec![result_type, TypeId::UNDEFINED])
        } else {
            result_type
        }
    }

    /// Get the type of a class member.
    ///
    /// Computes the type for class property declarations, method declarations, and getters.
    pub(crate) fn get_type_of_class_member(&mut self, member_idx: NodeIndex) -> TypeId {
        use crate::parser::syntax_kind_ext;

        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return TypeId::ANY;
        };

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                    return TypeId::ANY;
                };

                // Get the type: either from annotation or inferred from initializer
                if !prop.type_annotation.is_none() {
                    self.get_type_from_type_node(prop.type_annotation)
                } else if !prop.initializer.is_none() {
                    self.get_type_of_node(prop.initializer)
                } else {
                    TypeId::ANY
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                // Get the method type
                self.get_type_of_node(member_idx)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                    return TypeId::ANY;
                };

                if !accessor.type_annotation.is_none() {
                    self.get_type_from_type_node(accessor.type_annotation)
                } else {
                    self.infer_getter_return_type(accessor.body)
                }
            }
            _ => TypeId::ANY,
        }
    }

    /// Get the simple type of an interface member (without wrapping in object type).
    ///
    /// For property signatures: returns the property type
    /// For method signatures: returns the function type
    pub(crate) fn get_type_of_interface_member_simple(&mut self, member_idx: NodeIndex) -> TypeId {
        use crate::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};
        use crate::solver::FunctionShape;

        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return TypeId::ANY;
        };

        if member_node.kind == METHOD_SIGNATURE {
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                return TypeId::ANY;
            };

            let (type_params, type_param_updates) = self.push_type_parameters(&sig.type_parameters);
            let (params, this_type) = self.extract_params_from_signature(sig);
            let (return_type, type_predicate) = self.return_type_and_predicate(sig.type_annotation);

            let shape = FunctionShape {
                type_params,
                params,
                this_type,
                return_type,
                type_predicate,
                is_constructor: false,
                is_method: true,
            };
            self.pop_type_parameters(type_param_updates);
            return self.ctx.types.function(shape);
        }

        if member_node.kind == PROPERTY_SIGNATURE {
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                return TypeId::ANY;
            };

            if !sig.type_annotation.is_none() {
                return self.get_type_from_type_node(sig.type_annotation);
            }
            return TypeId::ANY;
        }

        TypeId::ANY
    }

    /// Get the type of an interface member.
    ///
    /// Returns an object type containing the member. For method signatures,
    /// creates a callable type. For property signatures, creates a property type.
    pub(crate) fn get_type_of_interface_member(&mut self, member_idx: NodeIndex) -> TypeId {
        use crate::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};
        use crate::solver::{FunctionShape, PropertyInfo};

        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return TypeId::ERROR;
        };

        if member_node.kind == METHOD_SIGNATURE || member_node.kind == PROPERTY_SIGNATURE {
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                return TypeId::ERROR;
            };
            let name = self.get_property_name(sig.name);
            let Some(name) = name else {
                return TypeId::ERROR;
            };
            let name_atom = self.ctx.types.intern_string(&name);

            if member_node.kind == METHOD_SIGNATURE {
                let (type_params, type_param_updates) =
                    self.push_type_parameters(&sig.type_parameters);
                let (params, this_type) = self.extract_params_from_signature(sig);
                let (return_type, type_predicate) =
                    self.return_type_and_predicate(sig.type_annotation);

                let shape = FunctionShape {
                    type_params,
                    params,
                    this_type,
                    return_type,
                    type_predicate,
                    is_constructor: false,
                    is_method: true,
                };
                self.pop_type_parameters(type_param_updates);
                let method_type = self.ctx.types.function(shape);

                let prop = PropertyInfo {
                    name: name_atom,
                    type_id: method_type,
                    write_type: method_type,
                    optional: sig.question_token,
                    readonly: self.has_readonly_modifier(&sig.modifiers),
                    is_method: true,
                };
                return self.ctx.types.object(vec![prop]);
            }

            let type_id = if !sig.type_annotation.is_none() {
                self.get_type_from_type_node(sig.type_annotation)
            } else {
                TypeId::ANY
            };
            let prop = PropertyInfo {
                name: name_atom,
                type_id,
                write_type: type_id,
                optional: sig.question_token,
                readonly: self.has_readonly_modifier(&sig.modifiers),
                is_method: false,
            };
            return self.ctx.types.object(vec![prop]);
        }

        TypeId::ANY
    }

    /// Get the type of a binary expression.
    ///
    /// Handles all binary operators including arithmetic, comparison, logical,
    /// assignment, nullish coalescing, and comma operators.
    pub(crate) fn get_type_of_binary_expression(&mut self, idx: NodeIndex) -> TypeId {
        use crate::scanner::SyntaxKind;
        use crate::solver::{BinaryOpEvaluator, BinaryOpResult};

        let evaluator = BinaryOpEvaluator::new(self.ctx.types);
        let mut stack = vec![(idx, false)];
        let mut type_stack: Vec<TypeId> = Vec::new();

        while let Some((node_idx, visited)) = stack.pop() {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                // Return UNKNOWN instead of ANY when node cannot be found
                type_stack.push(TypeId::UNKNOWN);
                continue;
            };

            if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                type_stack.push(self.get_type_of_node(node_idx));
                continue;
            }

            let Some(binary) = self.ctx.arena.get_binary_expr(node) else {
                // Return UNKNOWN instead of ANY when binary expression cannot be extracted
                type_stack.push(TypeId::UNKNOWN);
                continue;
            };

            let left_idx = binary.left;
            let right_idx = binary.right;
            let op_kind = binary.operator_token;

            if !visited {
                if self.is_assignment_operator(op_kind) {
                    let assign_type = if op_kind == SyntaxKind::EqualsToken as u16 {
                        self.check_assignment_expression(left_idx, right_idx, node_idx)
                    } else {
                        self.check_compound_assignment_expression(
                            left_idx, right_idx, op_kind, node_idx,
                        )
                    };
                    type_stack.push(assign_type);
                    continue;
                }

                stack.push((node_idx, true));
                stack.push((right_idx, false));
                stack.push((left_idx, false));
                continue;
            }

            // Return UNKNOWN instead of ANY when type_stack is empty
            let right_type = type_stack.pop().unwrap_or(TypeId::UNKNOWN);
            let left_type = type_stack.pop().unwrap_or(TypeId::UNKNOWN);
            if op_kind == SyntaxKind::CommaToken as u16 {
                if self.is_side_effect_free(left_idx)
                    && !self.is_indirect_call(node_idx, left_idx, right_idx)
                {
                    use crate::checker::types::diagnostics::{
                        diagnostic_codes, diagnostic_messages,
                    };
                    self.error_at_node(
                        left_idx,
                        diagnostic_messages::LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS,
                        diagnostic_codes::LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS,
                    );
                }
                type_stack.push(right_type);
                continue;
            }
            if op_kind == SyntaxKind::InKeyword as u16 {
                if let Some(left_node) = self.ctx.arena.get(left_idx)
                    && left_node.kind == SyntaxKind::PrivateIdentifier as u16
                {
                    self.check_private_identifier_in_expression(left_idx, right_type);
                }
                type_stack.push(TypeId::BOOLEAN);
                continue;
            }

            // Nullish coalescing: `a ?? b`
            if op_kind == SyntaxKind::QuestionQuestionToken as u16 {
                // Propagate error types (don't collapse to unknown)
                if left_type == TypeId::ERROR || right_type == TypeId::ERROR {
                    type_stack.push(TypeId::ERROR);
                    continue;
                }

                let (non_nullish, cause) = self.split_nullish_type(left_type);
                if cause.is_none() {
                    type_stack.push(left_type);
                } else {
                    let result = match non_nullish {
                        None => right_type,
                        Some(non_nullish) => self.ctx.types.union2(non_nullish, right_type),
                    };
                    type_stack.push(result);
                }
                continue;
            }
            let op_str = match op_kind {
                k if k == SyntaxKind::PlusToken as u16 => "+",
                k if k == SyntaxKind::MinusToken as u16 => "-",
                k if k == SyntaxKind::AsteriskToken as u16 => "*",
                k if k == SyntaxKind::AsteriskAsteriskToken as u16 => "**",
                k if k == SyntaxKind::SlashToken as u16 => "/",
                k if k == SyntaxKind::PercentToken as u16 => "%",
                k if k == SyntaxKind::LessThanToken as u16 => "<",
                k if k == SyntaxKind::GreaterThanToken as u16 => ">",
                k if k == SyntaxKind::LessThanEqualsToken as u16 => "<=",
                k if k == SyntaxKind::GreaterThanEqualsToken as u16 => ">=",
                k if k == SyntaxKind::EqualsEqualsToken as u16 => "==",
                k if k == SyntaxKind::ExclamationEqualsToken as u16 => "!=",
                k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => "===",
                k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => "!==",
                k if k == SyntaxKind::AmpersandAmpersandToken as u16 => "&&",
                k if k == SyntaxKind::BarBarToken as u16 => "||",
                k if k == SyntaxKind::AmpersandToken as u16
                    || k == SyntaxKind::BarToken as u16
                    || k == SyntaxKind::CaretToken as u16
                    || k == SyntaxKind::LessThanLessThanToken as u16
                    || k == SyntaxKind::GreaterThanGreaterThanToken as u16
                    || k == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 =>
                {
                    // Bitwise operators require integer operands (number, bigint, any, or enum)
                    // Emit TS2362/TS2363 if operands are not valid
                    let op_str = match op_kind {
                        k if k == SyntaxKind::AmpersandToken as u16 => "&",
                        k if k == SyntaxKind::BarToken as u16 => "|",
                        k if k == SyntaxKind::CaretToken as u16 => "^",
                        k if k == SyntaxKind::LessThanLessThanToken as u16 => "<<",
                        k if k == SyntaxKind::GreaterThanGreaterThanToken as u16 => ">>",
                        k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => ">>>",
                        _ => "?",
                    };
                    let result = evaluator.evaluate(left_type, right_type, op_str);
                    let result_type = match result {
                        BinaryOpResult::Success(result_type) => result_type,
                        BinaryOpResult::TypeError { .. } => {
                            // Emit appropriate error for arithmetic type mismatch
                            self.emit_binary_operator_error(node_idx, left_idx, right_idx, left_type, right_type, op_str);
                            TypeId::UNKNOWN
                        }
                    };
                    type_stack.push(result_type);
                    continue;
                }
                _ => {
                    type_stack.push(TypeId::UNKNOWN);
                    continue;
                }
            };

            let result = evaluator.evaluate(left_type, right_type, op_str);
            let result_type = match result {
                BinaryOpResult::Success(result_type) => result_type,
                BinaryOpResult::TypeError { left, right, op } => {
                    // Emit appropriate error for arithmetic type mismatch
                    self.emit_binary_operator_error(node_idx, left_idx, right_idx, left, right, op);
                    TypeId::UNKNOWN
                }
            };
            type_stack.push(result_type);
        }

        type_stack.pop().unwrap_or(TypeId::UNKNOWN)
    }

    /// Get the type of an element access expression (e.g., arr[0], obj["prop"]).
    ///
    /// Handles element access with optional chaining, index signatures,
    /// and nullish coalescing.
    pub(crate) fn get_type_of_element_access(&mut self, idx: NodeIndex) -> TypeId {
        use crate::solver::{PropertyAccessResult, QueryDatabase};

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return TypeId::ERROR;
        };

        // Get the type of the object
        let object_type = self.get_type_of_node(access.expression);
        let object_type = self.evaluate_application_type(object_type);

        let literal_string = self.get_literal_string_from_node(access.name_or_argument);
        let numeric_string_index = literal_string
            .as_deref()
            .and_then(|name| self.get_numeric_index_from_string(name));
        let literal_index = self
            .get_literal_index_from_node(access.name_or_argument)
            .or(numeric_string_index);

        if let Some(name) = literal_string.as_deref()
            && self.is_global_this_expression(access.expression)
        {
            let property_type =
                self.resolve_global_this_property_type(name, access.name_or_argument);
            if property_type == TypeId::ERROR {
                return TypeId::ERROR;
            }
            return self.apply_flow_narrowing(idx, property_type);
        }

        if let Some(name) = literal_string.as_deref() {
            if !self.check_property_accessibility(
                access.expression,
                name,
                access.name_or_argument,
                object_type,
            ) {
                return TypeId::ERROR;
            }
        } else if let Some(index) = literal_index {
            let name = index.to_string();
            if !self.check_property_accessibility(
                access.expression,
                &name,
                access.name_or_argument,
                object_type,
            ) {
                return TypeId::ERROR;
            }
        }

        // Don't report errors for any/error types
        if object_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type == TypeId::ERROR {
            return TypeId::ERROR;
        }

        let object_type = self.resolve_type_for_property_access(object_type);
        if object_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type == TypeId::ERROR {
            return TypeId::ERROR;
        }

        let (object_type_for_access, nullish_cause) = self.split_nullish_type(object_type);
        let Some(object_type_for_access) = object_type_for_access else {
            if access.question_dot_token {
                return TypeId::UNDEFINED;
            }
            if let Some(cause) = nullish_cause {
                self.report_possibly_nullish_object(access.expression, cause);
            }
            return TypeId::ERROR;
        };

        let index_type = self.get_type_of_node(access.name_or_argument);
        let literal_string_is_none = literal_string.is_none();

        let mut result_type = None;
        let mut report_no_index = false;
        let mut use_index_signature_check = true;

        if let Some(name) = literal_string.as_deref() {
            if let Some(member_type) =
                self.resolve_namespace_value_member(object_type_for_access, name)
            {
                result_type = Some(member_type);
                use_index_signature_check = false;
            } else if self.namespace_has_type_only_member(object_type_for_access, name) {
                self.error_type_only_value_at(name, access.name_or_argument);
                return TypeId::ERROR;
            }
        }

        if result_type.is_none()
            && literal_index.is_none()
            && let Some((string_keys, number_keys)) =
                self.get_literal_key_union_from_type(index_type)
        {
            let total_keys = string_keys.len() + number_keys.len();
            if total_keys > 1 || literal_string_is_none {
                if !string_keys.is_empty() && number_keys.is_empty() {
                    use_index_signature_check = false;
                }

                let mut types = Vec::new();
                if !string_keys.is_empty() {
                    match self.get_element_access_type_for_literal_keys(
                        object_type_for_access,
                        &string_keys,
                    ) {
                        Some(result) => types.push(result),
                        None => report_no_index = true,
                    }
                }

                if !number_keys.is_empty() {
                    match self.get_element_access_type_for_literal_number_keys(
                        object_type_for_access,
                        &number_keys,
                    ) {
                        Some(result) => types.push(result),
                        None => report_no_index = true,
                    }
                }

                if report_no_index {
                    result_type = Some(TypeId::ANY);
                } else if !types.is_empty() {
                    result_type = Some(if types.len() == 1 {
                        types[0]
                    } else {
                        self.ctx.types.union(types)
                    });
                }
            }
        }

        if result_type.is_none()
            && let Some(property_name) = self.get_literal_string_from_node(access.name_or_argument)
            && numeric_string_index.is_none()
        {
            use_index_signature_check = false;
            let result = self
                .ctx
                .types
                .property_access_type(object_type_for_access, &property_name);
            result_type = Some(match result {
                PropertyAccessResult::Success { type_id, .. } => type_id,
                PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                    property_type.unwrap_or(TypeId::UNKNOWN)
                }
                PropertyAccessResult::IsUnknown => {
                    // TS2571: Object is of type 'unknown'
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        access.expression,
                        "Object is of type 'unknown'.",
                        diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN,
                    );
                    TypeId::ERROR
                }
                PropertyAccessResult::PropertyNotFound { .. } => {
                    report_no_index = true;
                    // Generate TS2339 for property not found during element access
                    self.error_property_not_exist_at(
                        &property_name.to_string(),
                        object_type_for_access,
                        access.name_or_argument,
                    );
                    TypeId::ERROR // Return ERROR instead of ANY to expose the error
                }
            });
        }

        let mut result_type = result_type.unwrap_or_else(|| {
            self.get_element_access_type(object_type_for_access, index_type, literal_index)
        });

        if use_index_signature_check
            && self.should_report_no_index_signature(
                object_type_for_access,
                index_type,
                literal_index,
            )
        {
            report_no_index = true;
        }

        if report_no_index {
            self.error_no_index_signature_at(index_type, object_type, access.name_or_argument);
        }

        if let Some(cause) = nullish_cause {
            if access.question_dot_token {
                result_type = self.ctx.types.union(vec![result_type, TypeId::UNDEFINED]);
            } else if !report_no_index {
                self.report_possibly_nullish_object(access.expression, cause);
            }
        }

        self.apply_flow_narrowing(idx, result_type)
    }

    /// Get the element access type for array/tuple/object with index signatures.
    ///
    /// Computes the type when accessing an element using an index.
    pub(crate) fn get_element_access_type(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> TypeId {
        use crate::solver::{LiteralValue, QueryDatabase, TypeKey};

        let object_key = match self.ctx.types.lookup(object_type) {
            Some(TypeKey::ReadonlyType(inner)) => self.ctx.types.lookup(inner),
            other => other,
        };

        let literal_index_type = literal_index
            .map(|index| self.ctx.types.literal_number(index as f64))
            .or_else(|| match self.ctx.types.lookup(index_type) {
                Some(TypeKey::Literal(LiteralValue::Number(num))) => {
                    Some(self.ctx.types.literal_number(num.0))
                }
                _ => None,
            });

        match object_key {
            Some(TypeKey::Array(element)) => {
                if let Some(literal_index_type) = literal_index_type {
                    let result = self
                        .ctx
                        .types
                        .evaluate_index_access(object_type, literal_index_type);
                    return if result == TypeId::UNDEFINED {
                        element
                    } else {
                        result
                    };
                }
                element
            }
            Some(TypeKey::Tuple(elements)) => {
                let elements = self.ctx.types.tuple_list(elements);
                if let Some(literal_index_type) = literal_index_type {
                    let result = self
                        .ctx
                        .types
                        .evaluate_index_access(object_type, literal_index_type);
                    return if result == TypeId::UNDEFINED {
                        TypeId::ANY
                    } else {
                        result
                    };
                }

                let element_types: Vec<TypeId> =
                    elements.iter().map(|element| element.type_id).collect();
                if element_types.is_empty() {
                    TypeId::NEVER
                } else if element_types.len() == 1 {
                    element_types[0]
                } else {
                    self.ctx.types.union(element_types)
                }
            }
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.ctx.types.object_shape(shape_id);
                if literal_index.is_some() {
                    if let Some(number_index) = shape.number_index.as_ref() {
                        return number_index.value_type;
                    }
                    if let Some(string_index) = shape.string_index.as_ref() {
                        return string_index.value_type;
                    }
                    return TypeId::ERROR;
                }

                if index_type == TypeId::NUMBER {
                    if let Some(number_index) = shape.number_index.as_ref() {
                        return number_index.value_type;
                    }
                    if let Some(string_index) = shape.string_index.as_ref() {
                        return string_index.value_type;
                    }
                    return TypeId::ERROR;
                }

                if index_type == TypeId::STRING
                    && let Some(string_index) = shape.string_index.as_ref()
                {
                    return string_index.value_type;
                }

                TypeId::ANY
            }
            Some(TypeKey::Union(_)) => {
                let members = self.get_union_members(object_type);
                let member_types: Vec<_> = members
                    .iter()
                    .map(|&member| self.get_element_access_type(member, index_type, literal_index))
                    .collect();
                if member_types.is_empty() {
                    TypeId::ANY
                } else {
                    self.ctx.types.union(member_types)
                }
            }
            _ => {
                // For unresolved types, return ANY for generic array types
                TypeId::ANY
            }
        }
    }

    /// Get the type of the `super` keyword.
    ///
    /// Computes the type of `super` expressions:
    /// - `super()` calls: returns the base class constructor type
    /// - `super.property` access: returns the base class instance type
    /// - Static context: returns constructor type
    /// - Instance context: returns instance type
    pub(crate) fn get_type_of_super_keyword(&mut self, idx: NodeIndex) -> TypeId {
        // Check super expression validity and emit any errors
        self.check_super_expression(idx);

        let Some(class_info) = self.ctx.enclosing_class.clone() else {
            return TypeId::ERROR;
        };

        let Some(base_class_idx) = self.get_base_class_idx(class_info.class_idx) else {
            return TypeId::ERROR;
        };

        let Some(base_node) = self.ctx.arena.get(base_class_idx) else {
            return TypeId::ERROR;
        };
        let Some(base_class) = self.ctx.arena.get_class(base_node) else {
            return TypeId::ERROR;
        };

        // Detect `super(...)` usage by checking if the parent is a CallExpression whose callee is `super`.
        let is_super_call = self
            .ctx
            .arena
            .get_extended(idx)
            .and_then(|ext| self.ctx.arena.get(ext.parent).map(|n| (ext.parent, n)))
            .and_then(|(parent_idx, parent_node)| {
                if parent_node.kind != syntax_kind_ext::CALL_EXPRESSION {
                    return None;
                }
                let call = self.ctx.arena.get_call_expr(parent_node)?;
                Some(call.expression == idx && parent_idx.is_some())
            })
            .unwrap_or(false);

        // Static context: the current `this` type is the current class constructor type.
        let is_static_context = self.current_this_type().is_some_and(|this_ty| {
            if let Some(sym_id) = self.ctx.binder.get_node_symbol(class_info.class_idx) {
                this_ty == self.get_type_of_symbol(sym_id)
            } else if let Some(class_node) = self.ctx.arena.get(class_info.class_idx) {
                if let Some(class) = self.ctx.arena.get_class(class_node) {
                    this_ty == self.get_class_constructor_type(class_info.class_idx, class)
                } else {
                    false
                }
            } else {
                false
            }
        });

        if is_super_call || is_static_context {
            return self.get_class_constructor_type(base_class_idx, base_class);
        }

        self.get_class_instance_type(base_class_idx, base_class)
    }

    /// Get the type of a node with a fallback.
    ///
    /// Returns the computed type, or the fallback if the computed type is ERROR.
    pub fn get_type_of_node_or(&mut self, idx: NodeIndex, fallback: TypeId) -> TypeId {
        let ty = self.get_type_of_node(idx);
        if ty == TypeId::ERROR { fallback } else { ty }
    }

    /// Get the type of an object literal expression.
    ///
    /// Computes the type of object literals like `{ x: 1, y: 2 }` or `{ foo, bar }`.
    /// Handles:
    /// - Property assignments: `{ x: value }`
    /// - Shorthand properties: `{ x }`
    /// - Method shorthands: `{ foo() {} }`
    /// - Getters/setters: `{ get foo() {}, set foo(v) {} }`
    /// - Spread properties: `{ ...obj }`
    /// - Duplicate property detection
    /// - Contextual type inference
    /// - Implicit any reporting (TS7008)
    pub(crate) fn get_type_of_object_literal(&mut self, idx: NodeIndex) -> TypeId {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };
        use crate::interner::Atom;
        use crate::solver::{PropertyInfo, QueryDatabase};
        use rustc_hash::FxHashMap;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
            return TypeId::ERROR; // Missing object literal data - propagate error
        };

        // Collect properties from the object literal (later entries override earlier ones)
        let mut properties: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Property assignment: { x: value }
            if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                if let Some(name) = self.get_property_name(prop.name) {
                    // Get contextual type for this property
                    let property_context_type = if let Some(ctx_type) = self.ctx.contextual_type {
                        self.ctx.types.contextual_property_type(ctx_type, &name)
                    } else {
                        None
                    };

                    // Set contextual type for property value
                    let prev_context = self.ctx.contextual_type;
                    self.ctx.contextual_type = property_context_type;

                    let value_type = self.get_type_of_node(prop.initializer);

                    // Restore context
                    self.ctx.contextual_type = prev_context;

                    // Apply bidirectional type inference - use contextual type to narrow the value type
                    let value_type = crate::solver::apply_contextual_type(
                        self.ctx.types,
                        value_type,
                        property_context_type,
                    );

                    // TS7008: Member implicitly has an 'any' type
                    // Report this error when noImplicitAny is enabled, the object literal has a contextual type,
                    // and the property value type is 'any'
                    if self.ctx.no_implicit_any()
                        && prev_context.is_some()
                        && value_type == TypeId::ANY
                    {
                        let message = format_message(
                            diagnostic_messages::MEMBER_IMPLICIT_ANY,
                            &[&name, "any"],
                        );
                        self.error_at_node(
                            prop.name,
                            &message,
                            diagnostic_codes::IMPLICIT_ANY_MEMBER,
                        );
                    }

                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property
                    if properties.contains_key(&name_atom) {
                        let message = format_message(
                            diagnostic_messages::OBJECT_LITERAL_DUPLICATE_PROPERTY,
                            &[&name],
                        );
                        self.error_at_node(
                            prop.name,
                            &message,
                            diagnostic_codes::OBJECT_LITERAL_DUPLICATE_PROPERTY,
                        );
                    }

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: value_type,
                            write_type: value_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                        },
                    );
                }
            }
            // Shorthand property: { x } - identifier is both name and value
            else if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                if let Some(ident) = self.ctx.arena.get_identifier(elem_node) {
                    let name = ident.escaped_text.clone();

                    // Get contextual type for this property
                    let property_context_type = if let Some(ctx_type) = self.ctx.contextual_type {
                        self.ctx.types.contextual_property_type(ctx_type, &name)
                    } else {
                        None
                    };

                    // Set contextual type for shorthand property value
                    let prev_context = self.ctx.contextual_type;
                    self.ctx.contextual_type = property_context_type;

                    let value_type = self.get_type_of_node(elem_idx);

                    // Restore context
                    self.ctx.contextual_type = prev_context;

                    // Apply bidirectional type inference - use contextual type to narrow the value type
                    let value_type = crate::solver::apply_contextual_type(
                        self.ctx.types,
                        value_type,
                        property_context_type,
                    );

                    // TS7008: Member implicitly has an 'any' type
                    // Report this error when noImplicitAny is enabled, the object literal has a contextual type,
                    // and the shorthand property value type is 'any'
                    if self.ctx.no_implicit_any()
                        && prev_context.is_some()
                        && value_type == TypeId::ANY
                    {
                        let message = format_message(
                            diagnostic_messages::MEMBER_IMPLICIT_ANY,
                            &[&name, "any"],
                        );
                        self.error_at_node(
                            elem_idx,
                            &message,
                            diagnostic_codes::IMPLICIT_ANY_MEMBER,
                        );
                    }

                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property
                    if properties.contains_key(&name_atom) {
                        let message = format_message(
                            diagnostic_messages::OBJECT_LITERAL_DUPLICATE_PROPERTY,
                            &[&name],
                        );
                        self.error_at_node(
                            elem_idx,
                            &message,
                            diagnostic_codes::OBJECT_LITERAL_DUPLICATE_PROPERTY,
                        );
                    }

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: value_type,
                            write_type: value_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                        },
                    );
                }
            }
            // Method shorthand: { foo() {} }
            else if let Some(method) = self.ctx.arena.get_method_decl(elem_node) {
                if let Some(name) = self.get_property_name(method.name) {
                    // Set contextual type for method
                    let prev_context = self.ctx.contextual_type;
                    if let Some(ctx_type) = prev_context {
                        self.ctx.contextual_type =
                            self.ctx.types.contextual_property_type(ctx_type, &name);
                    }

                    let method_type = self.get_type_of_function(elem_idx);

                    // Restore context
                    self.ctx.contextual_type = prev_context;

                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property
                    if properties.contains_key(&name_atom) {
                        let message = format_message(
                            diagnostic_messages::OBJECT_LITERAL_DUPLICATE_PROPERTY,
                            &[&name],
                        );
                        self.error_at_node(
                            method.name,
                            &message,
                            diagnostic_codes::OBJECT_LITERAL_DUPLICATE_PROPERTY,
                        );
                    }

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: method_type,
                            write_type: method_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                        },
                    );
                }
            }
            // Accessor: { get foo() {} } or { set foo(v) {} }
            else if let Some(accessor) = self.ctx.arena.get_accessor(elem_node) {
                // Check for missing body - error 1005 at end of accessor
                if accessor.body.is_none() {
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    // Report at accessor.end - 1 (pointing to the closing paren)
                    let end_pos = elem_node.end.saturating_sub(1);
                    self.error_at_position(
                        end_pos,
                        1,
                        "'{' expected.",
                        diagnostic_codes::TOKEN_EXPECTED,
                    );
                }

                // For setters, check implicit any on parameters (error 7006)
                if elem_node.kind == syntax_kind_ext::SET_ACCESSOR {
                    for &param_idx in &accessor.parameters.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            self.maybe_report_implicit_any_parameter(param, false);
                        }
                    }
                }

                if let Some(name) = self.get_property_name(accessor.name) {
                    // For getter, infer return type; for setter, it's void
                    let accessor_type = if elem_node.kind == syntax_kind_ext::GET_ACCESSOR {
                        self.get_type_of_function(elem_idx)
                    } else {
                        TypeId::VOID
                    };
                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property
                    if properties.contains_key(&name_atom) {
                        let message = format_message(
                            diagnostic_messages::OBJECT_LITERAL_DUPLICATE_PROPERTY,
                            &[&name],
                        );
                        self.error_at_node(
                            accessor.name,
                            &message,
                            diagnostic_codes::OBJECT_LITERAL_DUPLICATE_PROPERTY,
                        );
                    }

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: accessor_type,
                            write_type: accessor_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                        },
                    );
                }
            }
            // Spread assignment: { ...obj }
            else if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                || elem_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
            {
                let spread_expr = self
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
                if let Some(spread_expr) = spread_expr {
                    let spread_type = self.get_type_of_node(spread_expr);
                    for prop in self.collect_object_spread_properties(spread_type) {
                        properties.insert(prop.name, prop);
                    }
                }
            }
            // Skip computed properties for now
        }

        let properties: Vec<PropertyInfo> = properties.into_values().collect();
        self.ctx.types.object(properties)
    }

    /// Collect properties from a spread expression in an object literal.
    ///
    /// Given the type of the spread expression, extracts all properties that would
    /// be spread into the object literal.
    fn collect_object_spread_properties(
        &mut self,
        type_id: TypeId,
    ) -> Vec<crate::solver::PropertyInfo> {
        use crate::interner::Atom;
        use crate::solver::TypeKey;
        use rustc_hash::FxHashMap;

        let resolved = self.resolve_type_for_property_access(type_id);
        match self.ctx.types.lookup(resolved) {
            Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.ctx.types.object_shape(shape_id);
                shape.properties.to_vec()
            }
            Some(TypeKey::Callable(shape_id)) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                shape.properties.to_vec()
            }
            Some(TypeKey::Intersection(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                let mut merged: FxHashMap<Atom, crate::solver::PropertyInfo> = FxHashMap::default();
                for &member in members.iter() {
                    for prop in self.collect_object_spread_properties(member) {
                        merged.insert(prop.name, prop);
                    }
                }
                merged.into_values().collect()
            }
            _ => Vec::new(),
        }
    }

    /// Get the type of a `new` expression.
    ///
    /// Computes the type of `new Constructor(...)` expressions.
    /// Handles:
    /// - Abstract class instantiation errors
    /// - Type argument validation (TS2344)
    /// - Constructor signature resolution
    /// - Overload resolution
    /// - Intersection types (mixin pattern)
    /// - Argument type checking
    pub(crate) fn get_type_of_new_expression(&mut self, idx: NodeIndex) -> TypeId {
        use crate::binder::symbol_flags;
        use crate::checker::types::diagnostics::diagnostic_codes;
        use crate::solver::{CallEvaluator, CallResult, CallableShape, CompatChecker, TypeKey};

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(new_expr) = self.ctx.arena.get_call_expr(node) else {
            return TypeId::ERROR; // Missing new expression data - propagate error
        };

        // Check if trying to instantiate an abstract class or type-only symbol
        // The expression is typically an identifier referencing the class
        if let Some(expr_node) = self.ctx.arena.get(new_expr.expression) {
            // If it's a direct identifier (e.g., `new MyClass()`)
            if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
                let class_name = &ident.escaped_text;

                // Try multiple ways to find the symbol:
                // 1. Check if the identifier node has a direct symbol binding
                // 2. Look up in file_locals
                // 3. Search all symbols by name (handles local scopes like classes inside functions)

                let symbol_opt = self
                    .ctx
                    .binder
                    .get_node_symbol(new_expr.expression)
                    .or_else(|| self.ctx.binder.file_locals.get(class_name))
                    .or_else(|| self.ctx.binder.get_symbols().find_by_name(class_name));

                if let Some(sym_id) = symbol_opt
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                {
                    // Check if it's type-only (interface, type alias without value, or type-only import)
                    let has_type = (symbol.flags & symbol_flags::TYPE) != 0;
                    let has_value = (symbol.flags & symbol_flags::VALUE) != 0;
                    let is_interface = (symbol.flags & symbol_flags::INTERFACE) != 0;
                    let is_type_alias = (symbol.flags & symbol_flags::TYPE_ALIAS) != 0;

                    // Emit TS2693 for interfaces and type aliases used as values
                    // Also check for type-only symbols
                    if (has_type && !has_value) || is_interface || is_type_alias {
                        self.error_type_only_value_at(class_name, new_expr.expression);
                        return TypeId::ERROR;
                    }

                    // Check if it has the ABSTRACT flag
                    if symbol.flags & symbol_flags::ABSTRACT != 0 {
                        self.error_at_node(
                            idx,
                            "Cannot create an instance of an abstract class.",
                            diagnostic_codes::CANNOT_CREATE_INSTANCE_OF_ABSTRACT_CLASS,
                        );
                        return TypeId::ERROR;
                    }
                }
            }
        }

        // Get the type of the constructor expression
        let constructor_type = self.get_type_of_node(new_expr.expression);

        // Validate explicit type arguments against constraints (TS2344)
        if let Some(ref type_args_list) = new_expr.type_arguments
            && !type_args_list.nodes.is_empty()
        {
            self.validate_new_expression_type_arguments(constructor_type, type_args_list, idx);
        }

        // If the `new` expression provides explicit type arguments (`new Foo<T>()`),
        // instantiate the constructor signatures with those args so we don't fall back to
        // inference (and so we match tsc behavior).
        let constructor_type = self.apply_type_arguments_to_constructor_type(
            constructor_type,
            new_expr.type_arguments.as_ref(),
        );

        // Check if the constructor type contains any abstract classes (for union types)
        // e.g., `new cls()` where `cls: typeof AbstractA | typeof AbstractB`
        if self.type_contains_abstract_class(constructor_type) {
            self.error_at_node(
                idx,
                "Cannot create an instance of an abstract class.",
                diagnostic_codes::CANNOT_CREATE_INSTANCE_OF_ABSTRACT_CLASS,
            );
            return TypeId::ERROR;
        }

        if constructor_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if constructor_type == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        let construct_type = match self.ctx.types.lookup(constructor_type) {
            Some(TypeKey::Callable(shape_id)) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                if shape.construct_signatures.is_empty() {
                    // Functions with a prototype property are constructable
                    // This handles cases like `function Foo() {}` where `Foo.prototype` exists
                    if self.type_has_prototype_property(constructor_type) {
                        Some(constructor_type)
                    } else {
                        None
                    }
                } else {
                    Some(self.ctx.types.callable(CallableShape {
                        call_signatures: shape.construct_signatures.clone(),
                        construct_signatures: Vec::new(),
                        properties: Vec::new(),
                        string_index: None,
                        number_index: None,
                    }))
                }
            }
            Some(TypeKey::Function(_)) => Some(constructor_type),
            Some(TypeKey::Intersection(members_id)) => {
                // For intersection of constructors (mixin pattern), the result is an
                // intersection of all instance types. Handle this specially.
                let members = self.ctx.types.type_list(members_id);
                let mut instance_types: Vec<TypeId> = Vec::new();

                for &member in members.iter() {
                    if let Some(TypeKey::Callable(shape_id)) = self.ctx.types.lookup(member) {
                        let shape = self.ctx.types.callable_shape(shape_id);
                        // Get the return type from the first construct signature
                        if let Some(sig) = shape.construct_signatures.first() {
                            instance_types.push(sig.return_type);
                        }
                    }
                }

                if instance_types.is_empty() {
                    return TypeId::ERROR; // No construct signatures in intersection - expose error
                } else if instance_types.len() == 1 {
                    return instance_types[0];
                } else {
                    // Return intersection of all instance types
                    return self.ctx.types.intersection(instance_types);
                }
            }
            _ => None,
        };

        let Some(construct_type) = construct_type else {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        };

        let args = new_expr
            .arguments
            .as_ref()
            .map(|a| &a.nodes)
            .map(|n| n.as_slice())
            .unwrap_or(&[]);

        let overload_signatures = match self.ctx.types.lookup(construct_type) {
            Some(TypeKey::Callable(shape_id)) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                if shape.call_signatures.len() > 1 {
                    Some(shape.call_signatures.clone())
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(signatures) = overload_signatures.as_deref()
            && let Some(return_type) =
                self.resolve_overloaded_call_with_signatures(args, signatures)
        {
            return return_type;
        }

        let ctx_helper = ContextualTypeContext::with_expected(self.ctx.types, construct_type);
        let check_excess_properties = overload_signatures.is_none();
        let arg_types = self.collect_call_argument_types_with_context(
            args,
            |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
            check_excess_properties,
        );

        self.ensure_application_symbols_resolved(construct_type);
        for &arg_type in &arg_types {
            self.ensure_application_symbols_resolved(arg_type);
        }
        let result = {
            let env = self.ctx.type_env.borrow();
            let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
            checker.set_strict_null_checks(self.ctx.strict_null_checks());
            let mut evaluator = CallEvaluator::new(self.ctx.types, &mut checker);
            evaluator.resolve_call(construct_type, &arg_types)
        };

        match result {
            CallResult::Success(return_type) => return_type,
            CallResult::NotCallable { .. } => {
                self.error_not_callable_at(constructor_type, new_expr.expression);
                TypeId::ERROR
            }
            CallResult::ArgumentCountMismatch {
                expected_min,
                expected_max,
                actual,
            } => {
                // Determine which error to emit:
                // - TS2555: "Expected at least N arguments" when got < min and there's a range
                // - TS2554: "Expected N arguments" otherwise
                if actual < expected_min && expected_max.is_some_and(|max| max != expected_min) {
                    // Too few arguments with optional parameters - use TS2555
                    self.error_expected_at_least_arguments_at(expected_min, actual, idx);
                } else {
                    // Either too many, or exact count expected - use TS2554
                    let expected = expected_max.unwrap_or(expected_min);
                    self.error_argument_count_mismatch_at(expected, actual, idx);
                }
                TypeId::ERROR
            }
            CallResult::ArgumentTypeMismatch {
                index,
                expected,
                actual,
            } => {
                if index < args.len() {
                    let arg_idx = args[index];
                    // Check if this is a weak union violation or excess property case
                    // In these cases, TypeScript shows TS2353 (excess property) instead of TS2322
                    // We should skip the TS2322 error regardless of check_excess_properties flag
                    if !self.should_skip_weak_union_error(actual, expected, arg_idx) {
                        self.error_argument_not_assignable_at(actual, expected, arg_idx);
                    }
                }
                TypeId::ERROR
            }
            CallResult::NoOverloadMatch { failures, .. } => {
                self.error_no_overload_matches_at(idx, &failures);
                TypeId::ERROR
            }
        }
    }

    /// Check if a type contains any abstract class constructors.
    ///
    /// This handles union types like `typeof AbstractA | typeof ConcreteB`.
    /// Recursively checks union and intersection types for abstract class members.
    fn type_contains_abstract_class(&self, type_id: TypeId) -> bool {
        use crate::binder::SymbolId;
        use crate::binder::symbol_flags;
        use crate::solver::{SymbolRef, TypeKey};

        let Some(type_key) = self.ctx.types.lookup(type_id) else {
            return false;
        };

        match type_key {
            // TypeQuery is `typeof ClassName` - check if the symbol is abstract
            // Since get_type_from_type_query now uses real SymbolIds, we can directly look up
            TypeKey::TypeQuery(SymbolRef(sym_id)) => {
                if let Some(symbol) = self.ctx.binder.get_symbol(SymbolId(sym_id))
                    && symbol.flags & symbol_flags::ABSTRACT != 0
                {
                    return true;
                }
                false
            }
            // Union type - check if ANY constituent is abstract
            TypeKey::Union(members) => {
                let members = self.ctx.types.type_list(members);
                members
                    .iter()
                    .any(|&member| self.type_contains_abstract_class(member))
            }
            // Intersection type - check if ANY constituent is abstract
            TypeKey::Intersection(members) => {
                let members = self.ctx.types.type_list(members);
                members
                    .iter()
                    .any(|&member| self.type_contains_abstract_class(member))
            }
            _ => false,
        }
    }

    /// Get type from a union type node (A | B).
    ///
    /// Parses a union type expression and creates a Union type with all members.
    ///
    /// ## Type Normalization:
    /// - Empty union → NEVER (the bottom type)
    /// - Single member → the member itself (no union wrapper)
    /// - Multiple members → Union type with all members
    ///
    /// ## Member Resolution:
    /// - Each member is resolved via `get_type_from_type_node`
    /// - Handles nested typeof expressions and type references
    ///
    /// ## TypeScript Semantics:
    /// Union types represent values that can be any of the members:
    /// - Primitives: `string | number` accepts either
    /// - Objects: Combines properties from all members
    /// - Functions: Union of function signatures
    pub(crate) fn get_type_from_union_type(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        // UnionType uses CompositeTypeData which has a types list
        if let Some(composite) = self.ctx.arena.get_composite_type(node) {
            let mut member_types = Vec::new();
            for &type_idx in &composite.types.nodes {
                // Use get_type_from_type_node to properly resolve typeof expressions via binder
                member_types.push(self.get_type_from_type_node(type_idx));
            }

            if member_types.is_empty() {
                return TypeId::NEVER;
            }
            if member_types.len() == 1 {
                return member_types[0];
            }

            return self.ctx.types.union(member_types);
        }

        TypeId::ERROR // Missing composite type data - propagate error
    }

    /// Get type from an intersection type node (A & B).
    ///
    /// Parses an intersection type expression and creates an Intersection type with all members.
    ///
    /// ## Type Normalization:
    /// - Empty intersection → UNKNOWN (the top type for intersections)
    /// - Single member → the member itself (no intersection wrapper)
    /// - Multiple members → Intersection type with all members
    ///
    /// ## Member Resolution:
    /// - Each member is resolved via `get_type_from_type_node`
    /// - Handles nested typeof expressions and type references
    ///
    /// ## TypeScript Semantics:
    /// Intersection types combine multiple types into one:
    /// - Objects: Merges properties from all members
    /// - Primitives: Usually results in NEVER (except for any/unknown)
    /// - Functions: Combines function signatures (overload resolution)
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// type NameAndAge = { name: string } & { age: number };
    /// // Creates Intersection with properties { name: string; age: number }
    ///
    /// type Extended = BaseModel & { id: number } & Serializable;
    /// // Combines all three types into one
    ///
    /// type Neverish = string & number;  // NEVER (primitives don't intersect)
    /// ```
    pub(crate) fn get_type_from_intersection_type(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        // IntersectionType uses CompositeTypeData which has a types list
        if let Some(composite) = self.ctx.arena.get_composite_type(node) {
            let mut member_types = Vec::new();
            for &type_idx in &composite.types.nodes {
                // Use get_type_from_type_node to properly resolve typeof expressions via binder
                member_types.push(self.get_type_from_type_node(type_idx));
            }

            if member_types.is_empty() {
                return TypeId::UNKNOWN; // Empty intersection is unknown
            }
            if member_types.len() == 1 {
                return member_types[0];
            }

            return self.ctx.types.intersection(member_types);
        }

        TypeId::ERROR // Missing composite type data - propagate error
    }

    /// Get type from an array type node (string[]).
    ///
    /// Parses an array type expression and creates an Array type.
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// type Strings = string[];
    /// // Creates Array<string> type
    ///
    /// type Nested = number[][];
    /// // Creates Array<Array<number>> type
    /// ```
    pub(crate) fn get_type_from_array_type(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        if let Some(array_type) = self.ctx.arena.get_array_type(node) {
            let elem_type = self.get_type_from_type_node(array_type.element_type);
            return self.ctx.types.array(elem_type);
        }

        TypeId::ERROR // Missing array type data - propagate error
    }

    /// Get type from a type operator node (readonly T[], readonly [T, U], unique symbol).
    ///
    /// Handles type modifiers like:
    /// - `readonly T[]` - Creates ReadonlyType wrapper
    /// - `unique symbol` - Special marker for unique symbols
    pub(crate) fn get_type_from_type_operator(&mut self, idx: NodeIndex) -> TypeId {
        use crate::scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        if let Some(type_op) = self.ctx.arena.get_type_operator(node) {
            let operator = type_op.operator;
            let inner_type = self.get_type_from_type_node(type_op.type_node);

            // Handle readonly operator
            if operator == SyntaxKind::ReadonlyKeyword as u16 {
                // Wrap the inner type in ReadonlyType
                return self
                    .ctx
                    .types
                    .intern(crate::solver::TypeKey::ReadonlyType(inner_type));
            }

            // Handle unique operator
            if operator == SyntaxKind::UniqueKeyword as u16 {
                // unique is handled differently - it's a type modifier for symbols
                // For now, just return the inner type
                return inner_type;
            }

            // Unknown operator - return inner type
            inner_type
        } else {
            TypeId::ERROR // Missing type operator data - propagate error
        }
    }

    /// Get the `keyof` type for a given type.
    ///
    /// Computes the type of all property keys for a given object type.
    /// For example: `keyof { x: number; y: string }` = `"x" | "y"`.
    ///
    /// ## Behavior:
    /// - Object types: Returns union of string literal types for each property name
    /// - Empty objects: Returns NEVER
    /// - Other types: Returns NEVER
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// type Keys = keyof { x: number; y: string };
    /// // "x" | "y"
    ///
    /// type Empty = keyof {};
    /// // never
    /// ```
    pub(crate) fn get_keyof_type(&self, operand: TypeId) -> TypeId {
        use crate::solver::{LiteralValue, TypeKey};

        let Some(key) = self.ctx.types.lookup(operand) else {
            return TypeId::NEVER;
        };

        match key {
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                if shape.properties.is_empty() {
                    return TypeId::NEVER;
                }
                let key_types: Vec<TypeId> = shape
                    .properties
                    .iter()
                    .map(|p| {
                        self.ctx
                            .types
                            .intern(TypeKey::Literal(LiteralValue::String(p.name)))
                    })
                    .collect();
                self.ctx.types.union(key_types)
            }
            _ => TypeId::NEVER,
        }
    }

    /// Extract string literal keys from a union or single literal type.
    ///
    /// Given a type that may be a union of string literal types or a single string literal,
    /// extracts the actual string atoms.
    ///
    /// ## Behavior:
    /// - String literal: Returns vec with that string
    /// - Union of string literals: Returns vec with all strings
    /// - Other types: Returns empty vec
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // Single literal
    /// extractKeys<"hello">() // ["hello"]
    ///
    /// // Union of literals
    /// extractKeys<"a" | "b" | "c">() // ["a", "b", "c"]
    ///
    /// // Non-literal
    /// extractKeys<string>() // []
    /// ```
    pub(crate) fn extract_string_literal_keys(
        &self,
        type_id: TypeId,
    ) -> Vec<crate::interner::Atom> {
        use crate::solver::{LiteralValue, TypeKey};

        let Some(key) = self.ctx.types.lookup(type_id) else {
            return Vec::new();
        };

        match key {
            TypeKey::Literal(LiteralValue::String(name)) => vec![name],
            TypeKey::Union(list_id) => {
                let members = self.ctx.types.type_list(list_id);
                members
                    .iter()
                    .filter_map(|&member| {
                        if let Some(TypeKey::Literal(LiteralValue::String(name))) =
                            self.ctx.types.lookup(member)
                        {
                            Some(name)
                        } else {
                            None
                        }
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Get the Symbol constructor type.
    ///
    /// Creates the type for the global `Symbol` constructor, including:
    /// - Call signature: `Symbol(description?: string | number): symbol`
    /// - Well-known symbol properties (iterator, asyncIterator, etc.)
    pub(crate) fn get_symbol_constructor_type(&self) -> TypeId {
        use crate::solver::{CallSignature, CallableShape, ParamInfo, PropertyInfo};

        // Parameter: description?: string | number
        let description_param_type = self.ctx.types.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let description_param = ParamInfo {
            name: Some(self.ctx.types.intern_string("description")),
            type_id: description_param_type,
            optional: true,
            rest: false,
        };

        let call_signature = CallSignature {
            type_params: vec![],
            params: vec![description_param],
            this_type: None,
            return_type: TypeId::SYMBOL,
            type_predicate: None,
        };

        let well_known = [
            "iterator",
            "asyncIterator",
            "hasInstance",
            "isConcatSpreadable",
            "match",
            "matchAll",
            "replace",
            "search",
            "split",
            "species",
            "toPrimitive",
            "toStringTag",
            "unscopables",
            "dispose",
            "asyncDispose",
            "metadata",
        ];

        let mut properties = Vec::new();
        for name in well_known {
            let name_atom = self.ctx.types.intern_string(name);
            properties.push(PropertyInfo {
                name: name_atom,
                type_id: TypeId::SYMBOL,
                write_type: TypeId::SYMBOL,
                optional: false,
                readonly: true,
                is_method: false,
            });
        }

        self.ctx.types.callable(CallableShape {
            call_signatures: vec![call_signature],
            construct_signatures: Vec::new(),
            properties,
            string_index: None,
            number_index: None,
        })
    }

    /// Get the type of the receiver object in a method call.
    ///
    /// For a call like `obj.method()`, returns the type of `obj` after
    /// evaluating applications and resolving for property access.
    pub(crate) fn get_call_receiver_type(&mut self, callee_idx: NodeIndex) -> Option<TypeId> {
        let node = self.ctx.arena.get(callee_idx)?;
        let access = self.ctx.arena.get_access_expr(node)?;
        let receiver_type = self.get_type_of_node(access.expression);
        let receiver_type = self.evaluate_application_type(receiver_type);
        let receiver_type = self.resolve_type_for_property_access(receiver_type);
        Some(receiver_type)
    }

    /// Get the class declaration node from a TypeId.
    ///
    /// This function attempts to find the class declaration for a given type
    /// by looking for "private brand" properties that TypeScript adds to class
    /// instances for brand checking.
    ///
    /// ## Private Brand Properties:
    /// TypeScript adds private properties like `__private_brand_XXX` to class
    /// instances for brand checking (e.g., for private class members).
    /// This function searches for these brand properties to find the original
    /// class declaration.
    ///
    /// ## Returns:
    /// - `Some(NodeIndex)` - Found the class declaration
    /// - `None` - Type doesn't represent a class or couldn't determine
    pub(crate) fn get_class_decl_from_type(&self, type_id: TypeId) -> Option<NodeIndex> {
        use crate::binder::SymbolId;
        use crate::solver::TypeKey;

        fn parse_brand_name(name: &str) -> Option<Result<SymbolId, NodeIndex>> {
            const NODE_PREFIX: &str = "__private_brand_node_";
            const PREFIX: &str = "__private_brand_";

            if let Some(rest) = name.strip_prefix(NODE_PREFIX) {
                let node_id: u32 = rest.parse().ok()?;
                return Some(Err(NodeIndex(node_id)));
            }
            if let Some(rest) = name.strip_prefix(PREFIX) {
                let sym_id: u32 = rest.parse().ok()?;
                return Some(Ok(SymbolId(sym_id)));
            }

            None
        }

        fn collect_candidates<'a>(
            checker: &CheckerState<'a>,
            type_id: TypeId,
            out: &mut Vec<NodeIndex>,
        ) {
            let Some(key) = checker.ctx.types.lookup(type_id) else {
                return;
            };

            match key {
                TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                    let shape = checker.ctx.types.object_shape(shape_id);
                    for prop in &shape.properties {
                        let name = checker.ctx.types.resolve_atom_ref(prop.name);
                        if let Some(parsed) = parse_brand_name(&name) {
                            let class_idx = match parsed {
                                Ok(sym_id) => checker.get_class_declaration_from_symbol(sym_id),
                                Err(node_idx) => Some(node_idx),
                            };
                            if let Some(class_idx) = class_idx {
                                out.push(class_idx);
                            }
                        }
                    }
                }
                TypeKey::Union(list_id) | TypeKey::Intersection(list_id) => {
                    let list = checker.ctx.types.type_list(list_id);
                    for &member in list.iter() {
                        collect_candidates(checker, member, out);
                    }
                }
                _ => {}
            }
        }

        let mut candidates = Vec::new();
        collect_candidates(self, type_id, &mut candidates);
        if candidates.is_empty() {
            return None;
        }
        if candidates.len() == 1 {
            return Some(candidates[0]);
        }

        candidates
            .iter()
            .find(|&&candidate| {
                candidates.iter().all(|&other| {
                    candidate == other || self.is_class_derived_from(candidate, other)
                })
            })
            .copied()
    }

    /// Get the class name from a TypeId if it represents a class instance.
    ///
    /// Returns the class name as a string if the type represents a class,
    /// or None if the type doesn't represent a class or the class has no name.
    pub(crate) fn get_class_name_from_type(&self, type_id: TypeId) -> Option<String> {
        self.get_class_decl_from_type(type_id)
            .map(|class_idx| self.get_class_name_from_decl(class_idx))
    }

    /// Get the type of a call expression (e.g., `foo()`, `obj.method()`).
    ///
    /// Computes the return type of function/method calls.
    /// Handles:
    /// - Dynamic imports (returns `Promise<any>`)
    /// - Super calls (returns `void`)
    /// - Optional chaining (`obj?.method()`)
    /// - Overload resolution
    /// - Argument type checking
    /// - Type argument validation (TS2344)
    pub(crate) fn get_type_of_call_expression(&mut self, idx: NodeIndex) -> TypeId {
        use crate::checker::state::MAX_CALL_DEPTH;
        use crate::parser::node_flags;
        use crate::solver::{CallEvaluator, CallResult, CompatChecker, TypeKey};

        // Check call depth limit to prevent infinite recursion
        let mut call_depth = self.ctx.call_depth.borrow_mut();
        if *call_depth >= MAX_CALL_DEPTH {
            return TypeId::ERROR;
        }
        *call_depth += 1;
        drop(call_depth);

        let result = self.get_type_of_call_expression_inner(idx);

        // Decrement call depth
        let mut call_depth = self.ctx.call_depth.borrow_mut();
        *call_depth -= 1;
        result
    }

    /// Inner implementation of call expression type resolution.
    fn get_type_of_call_expression_inner(&mut self, idx: NodeIndex) -> TypeId {
        use crate::parser::node_flags;
        use crate::solver::{CallEvaluator, CallResult, CompatChecker, TypeKey};

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return TypeId::ERROR; // Missing call expression data - propagate error
        };

        // Get the type of the callee
        let mut callee_type = self.get_type_of_node(call.expression);

        // Check for dynamic import module resolution (TS2307)
        if self.is_dynamic_import(call) {
            self.check_dynamic_import_module_specifier(call);
            // Dynamic imports return Promise<typeof module>
            // For unresolved modules, return any to allow type flow to continue
            return TypeId::ANY;
        }

        // Special handling for super() calls - treat as construct call
        let is_super_call = self.is_super_expression(call.expression);

        // Get arguments list (may be None for calls without arguments)
        // IMPORTANT: We must check arguments even if callee is ANY/ERROR to catch definite assignment errors
        let args = call
            .arguments
            .as_ref()
            .map(|a| &a.nodes)
            .map(|n| n.as_slice())
            .unwrap_or(&[]);

        // Check if callee is any/error (don't report for those)
        if callee_type == TypeId::ANY {
            // Still need to check arguments for definite assignment (TS2454) and other errors
            // Create a dummy context helper that returns None for all parameter types
            let _ctx_helper = ContextualTypeContext::new(self.ctx.types);
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None, // No parameter type info for ANY callee
                check_excess_properties,
            );
            return TypeId::ANY;
        }
        if callee_type == TypeId::ERROR {
            // Still need to check arguments for definite assignment (TS2454) and other errors
            let _ctx_helper = ContextualTypeContext::new(self.ctx.types);
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None, // No parameter type info for ERROR callee
                check_excess_properties,
            );
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        let mut nullish_cause = None;
        if (node.flags as u32) & node_flags::OPTIONAL_CHAIN != 0 {
            let (non_nullish, cause) = self.split_nullish_type(callee_type);
            nullish_cause = cause;
            let Some(non_nullish) = non_nullish else {
                return TypeId::UNDEFINED;
            };
            callee_type = non_nullish;
            if callee_type == TypeId::ANY {
                return TypeId::ANY;
            }
            if callee_type == TypeId::ERROR {
                return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
            }
        }

        // args is already defined above before the ANY/ERROR check

        // Validate explicit type arguments against constraints (TS2344)
        if let Some(ref type_args_list) = call.type_arguments
            && !type_args_list.nodes.is_empty()
        {
            self.validate_call_type_arguments(callee_type, type_args_list, idx);
        }

        // Apply explicit type arguments to the callee type before checking arguments.
        // This ensures that when we have `fn<T>(x: T)` and call it as `fn<number>("string")`,
        // the parameter type becomes `number` (after substituting T=number), and we can
        // correctly check if `"string"` is assignable to `number`.
        let callee_type_for_resolution = if call.type_arguments.is_some() {
            self.apply_type_arguments_to_callable_type(callee_type, call.type_arguments.as_ref())
        } else {
            callee_type
        };

        let overload_signatures = match self.ctx.types.lookup(callee_type_for_resolution) {
            Some(TypeKey::Callable(shape_id)) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                if shape.call_signatures.len() > 1 {
                    Some(shape.call_signatures.clone())
                } else {
                    None
                }
            }
            _ => None,
        };

        // Overload candidates need signature-specific contextual typing.
        if let Some(signatures) = overload_signatures.as_deref()
            && let Some(return_type) =
                self.resolve_overloaded_call_with_signatures(args, signatures)
        {
            let return_type =
                self.apply_this_substitution_to_call_return(return_type, call.expression);
            return if nullish_cause.is_some() {
                self.ctx.types.union(vec![return_type, TypeId::UNDEFINED])
            } else {
                return_type
            };
        }

        // Create contextual context from callee type with type arguments applied
        let ctx_helper =
            ContextualTypeContext::with_expected(self.ctx.types, callee_type_for_resolution);
        let check_excess_properties = overload_signatures.is_none();
        let arg_types = self.collect_call_argument_types_with_context(
            args,
            |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
            check_excess_properties,
        );

        // Use CallEvaluator to resolve the call
        self.ensure_application_symbols_resolved(callee_type_for_resolution);
        for &arg_type in &arg_types {
            self.ensure_application_symbols_resolved(arg_type);
        }
        let result = {
            let env = self.ctx.type_env.borrow();
            let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
            checker.set_strict_function_types(self.ctx.strict_function_types());
            checker.set_strict_null_checks(self.ctx.strict_null_checks());
            let mut evaluator = CallEvaluator::new(self.ctx.types, &mut checker);
            evaluator.resolve_call(callee_type_for_resolution, &arg_types)
        };

        match result {
            CallResult::Success(return_type) => {
                let return_type =
                    self.apply_this_substitution_to_call_return(return_type, call.expression);
                let return_type =
                    self.refine_mixin_call_return_type(call.expression, &arg_types, return_type);
                if nullish_cause.is_some() {
                    self.ctx.types.union(vec![return_type, TypeId::UNDEFINED])
                } else {
                    return_type
                }
            }

            CallResult::NotCallable { .. } => {
                // Special case: super() calls are valid in constructors and return void
                if is_super_call {
                    return TypeId::VOID;
                }
                // Check if it's specifically a class constructor called without 'new' (TS2348)
                // Only emit TS2348 for types that have construct signatures but zero call signatures
                if self.is_class_constructor_type(callee_type) {
                    self.error_class_constructor_without_new_at(callee_type, call.expression);
                } else {
                    // For other non-callable types, emit the generic not-callable error
                    self.error_not_callable_at(callee_type, call.expression);
                }
                TypeId::ERROR
            }

            CallResult::ArgumentCountMismatch {
                expected_min,
                expected_max,
                actual,
            } => {
                // Determine which error to emit:
                // - TS2555: "Expected at least N arguments" when got < min and there's a range
                // - TS2554: "Expected N arguments" otherwise
                if actual < expected_min && expected_max.is_some_and(|max| max != expected_min) {
                    // Too few arguments with optional parameters - use TS2555
                    self.error_expected_at_least_arguments_at(expected_min, actual, idx);
                } else {
                    // Either too many, or exact count expected - use TS2554
                    let expected = expected_max.unwrap_or(expected_min);
                    self.error_argument_count_mismatch_at(expected, actual, idx);
                }
                TypeId::ERROR
            }

            CallResult::ArgumentTypeMismatch {
                index,
                expected,
                actual,
            } => {
                // Report error at the specific argument
                // Map the expanded index back to the original argument node
                // When spread arguments are expanded, the index may exceed args.len()
                let arg_idx = self.map_expanded_arg_index_to_original(args, index);
                if let Some(arg_idx) = arg_idx {
                    // Check if this is a weak union violation or excess property case
                    // In these cases, TypeScript shows TS2353 (excess property) instead of TS2322
                    if !self.should_skip_weak_union_error(actual, expected, arg_idx) {
                        self.error_argument_not_assignable_at(actual, expected, arg_idx);
                    }
                } else if !args.is_empty() {
                    // Fall back to the last argument (typically the spread) if mapping fails
                    let last_arg = args[args.len() - 1];
                    if !self.should_skip_weak_union_error(actual, expected, last_arg) {
                        self.error_argument_not_assignable_at(actual, expected, last_arg);
                    }
                }
                TypeId::ERROR
            }

            CallResult::NoOverloadMatch { failures, .. } => {
                self.error_no_overload_matches_at(idx, &failures);
                TypeId::ERROR
            }
        }
    }

    // =========================================================================
    // Type Relationship Queries
    // =========================================================================

    /// Check if a type is the error type.
    ///
    /// Returns true if the type is TypeId::ERROR.
    pub fn is_error_type(&self, ty: TypeId) -> bool {
        ty == TypeId::ERROR
    }

    /// Check if a type is the any type.
    ///
    /// Returns true if the type is TypeId::ANY.
    pub fn is_any_type(&self, ty: TypeId) -> bool {
        ty == TypeId::ANY
    }

    /// Check if a type is the unknown type.
    ///
    /// Returns true if the type is TypeId::UNKNOWN.
    pub fn is_unknown_type(&self, ty: TypeId) -> bool {
        ty == TypeId::UNKNOWN
    }

    /// Check if a type is the undefined type.
    ///
    /// Returns true if the type is TypeId::UNDEFINED.
    pub fn is_undefined_type(&self, ty: TypeId) -> bool {
        ty == TypeId::UNDEFINED
    }

    /// Check if a type is the void type.
    ///
    /// Returns true if the type is TypeId::VOID.
    pub fn is_void_type(&self, ty: TypeId) -> bool {
        ty == TypeId::VOID
    }

    /// Check if a type is the null type.
    ///
    /// Returns true if the type is TypeId::NULL.
    pub fn is_null_type(&self, ty: TypeId) -> bool {
        ty == TypeId::NULL
    }

    /// Check if a type is a nullable type (null or undefined).
    ///
    /// Returns true if the type is null or undefined.
    pub fn is_nullable_type(&self, ty: TypeId) -> bool {
        ty == TypeId::NULL || ty == TypeId::UNDEFINED
    }

    /// Check if a type is a never type.
    ///
    /// Returns true if the type is TypeId::NEVER.
    pub fn is_never_type(&self, ty: TypeId) -> bool {
        ty == TypeId::NEVER
    }

    // =========================================================================
    // Type Format Utilities
    // =========================================================================

    /// Format a type for display in error messages.
    ///
    /// This is a convenience wrapper that calls the internal format_type method.
    pub fn format_type_for_display(&self, ty: TypeId) -> String {
        self.format_type(ty)
    }

    /// Format a type for display, with optional simplification.
    ///
    /// If `simplify` is true, complex types are simplified for readability.
    pub fn format_type_simplified(&self, ty: TypeId, simplify: bool) -> String {
        // For now, just use the regular formatting
        // A future enhancement could add simplification logic
        if simplify {
            self.format_type(ty)
        } else {
            self.format_type(ty)
        }
    }

    // =========================================================================
    // Type Checking Helpers
    // =========================================================================

    /// Check if a type is assignable to another type.
    ///
    /// This is a convenience wrapper around `is_assignable_to`.
    pub fn check_is_assignable(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_assignable_to(source, target)
    }

    /// Check if a type is identical to another type.
    ///
    /// This performs a strict equality check on TypeIds.
    pub fn is_same_type(&self, ty1: TypeId, ty2: TypeId) -> bool {
        ty1 == ty2
    }

    /// Check if a type is a function type.
    ///
    /// Returns true if the type represents a callable function.
    pub fn is_function_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Callable(_)))
    }

    /// Check if a type is an object type.
    ///
    /// Returns true if the type represents an object or class.
    pub fn is_object_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(
            key,
            Some(crate::solver::TypeKey::Object(_) | crate::solver::TypeKey::ObjectWithIndex(_))
        )
    }

    /// Check if a type is an array type.
    ///
    /// Returns true if the type represents an array.
    pub fn is_array_type(&self, ty: TypeId) -> bool {
        // Check if it's a reference to the Array interface or an array literal
        // For now, this is a simplified check
        let type_str = self.format_type(ty);
        type_str.contains("[]") || type_str.starts_with("Array<")
    }

    /// Check if a type is a tuple type.
    ///
    /// Returns true if the type represents a tuple.
    pub fn is_tuple_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Tuple(_)))
    }

    /// Check if a type is a union type.
    ///
    /// Returns true if the type is a union of multiple types.
    pub fn is_union_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Union(_)))
    }

    /// Check if a type is an intersection type.
    ///
    /// Returns true if the type is an intersection of multiple types.
    pub fn is_intersection_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Intersection(_)))
    }

    /// Check if a type is a literal type.
    ///
    /// Returns true if the type is a specific literal value (string, number, boolean).
    pub fn is_literal_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Literal(_)))
    }

    /// Check if a type is a generic type application.
    ///
    /// Returns true if the type is a parameterized generic like Map<K, V>.
    pub fn is_generic_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Application(_)))
    }

    /// Check if a type is a reference to another type.
    ///
    /// Returns true if the type is a type reference (interface, class, type alias).
    pub fn is_type_reference(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Ref(_)))
    }

    /// Check if a type is a conditional type.
    ///
    /// Returns true if the type is a conditional type like T extends U ? X : Y.
    pub fn is_conditional_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Conditional(_)))
    }

    /// Check if a type is a mapped type.
    ///
    /// Returns true if the type is a mapped type like { [K in T]: U }.
    pub fn is_mapped_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Mapped(_)))
    }

    /// Check if a type is a template literal type.
    ///
    /// Returns true if the type is a template literal type like `foo${string}bar`.
    pub fn is_template_literal_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::TemplateLiteral(_)))
    }

    /// Check if a type is a callable type.
    ///
    /// Returns true if the type represents a function or callable.
    pub fn is_callable_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(
            key,
            Some(
                crate::solver::TypeKey::Function(_)
                    | crate::solver::TypeKey::Callable(_)
                    | crate::solver::TypeKey::ObjectWithIndex(_)
            )
        )
    }

    // =========================================================================
    // Special Type Utilities
    // =========================================================================

    /// Get the element type of an array type.
    ///
    /// Returns the type of elements in the array, or ANY if not an array.
    pub fn get_array_element_type(&self, _array_ty: TypeId) -> TypeId {
        // This is a simplified implementation
        // The full version would extract the element type from array types
        TypeId::ANY
    }

    /// Get the return type of a function type.
    ///
    /// Returns the return type of a function, or ANY if not a function.
    pub fn get_function_return_type(&self, _func_ty: TypeId) -> TypeId {
        // This is a simplified implementation
        // The full version would extract the return type from callable types
        TypeId::ANY
    }

    // =========================================================================
    // Type Construction Utilities
    // =========================================================================

    // =========================================================================
    // Type Manipulation Utilities
    // =========================================================================

    /// Create an array type from an element type.
    ///
    /// Creates a type representing T[] for the given element type T.
    pub fn make_array_type(&self, elem_type: TypeId) -> TypeId {
        self.ctx.types.array(elem_type)
    }

    /// Create a tuple type from element types.
    ///
    /// Creates a type representing a tuple with the given elements.
    pub fn make_tuple_type(&self, elem_types: Vec<TypeId>) -> TypeId {
        use crate::solver::TupleElement;
        let elements: Vec<TupleElement> = elem_types
            .into_iter()
            .map(|type_id| TupleElement {
                type_id,
                name: None,
                optional: false,
                rest: false,
            })
            .collect();
        self.ctx.types.tuple(elements)
    }

    /// Create a function type with parameters and return type.
    ///
    /// Creates a callable type representing a function signature.
    pub fn make_function_type(
        &self,
        params: Vec<crate::solver::ParamInfo>,
        return_type: TypeId,
    ) -> TypeId {
        use crate::solver::FunctionShape;
        let func_shape = FunctionShape {
            type_params: vec![],
            params,
            this_type: None,
            return_type,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };
        self.ctx.types.function(func_shape)
    }

    /// Get the base type of a generic application.
    ///
    /// For a type like Map<string, number>, returns the Map base type.
    pub fn get_generic_base(&self, ty: TypeId) -> Option<TypeId> {
        match self.ctx.types.lookup(ty) {
            Some(crate::solver::TypeKey::Application(app)) => {
                let app_info = self.ctx.types.type_application(app);
                Some(app_info.base)
            }
            _ => None,
        }
    }

    /// Get the type arguments from a generic application.
    ///
    /// For a type like Map<string, number>, returns [string, number].
    pub fn get_generic_args(&self, ty: TypeId) -> Option<Vec<TypeId>> {
        match self.ctx.types.lookup(ty) {
            Some(crate::solver::TypeKey::Application(app)) => {
                let app_info = self.ctx.types.type_application(app);
                Some(app_info.args.clone())
            }
            _ => None,
        }
    }

    // =========================================================================
    // Type Analysis Utilities
    // =========================================================================

    /// Check if a type contains a type parameter.
    ///
    /// Recursively checks if the type (or any nested type) is a type parameter.
    pub fn contains_type_parameter(&self, ty: TypeId) -> bool {
        let mut visited = std::collections::HashSet::new();
        self.contains_type_parameter_inner(ty, &mut visited)
    }

    fn contains_type_parameter_inner(
        &self,
        ty: TypeId,
        visited: &mut std::collections::HashSet<TypeId>,
    ) -> bool {
        if !visited.insert(ty) {
            return false;
        }

        match self.ctx.types.lookup(ty) {
            Some(TypeKey::TypeParameter(_)) | Some(TypeKey::Infer(_)) => true,
            Some(TypeKey::Array(elem)) => self.contains_type_parameter_inner(elem, visited),
            Some(TypeKey::Tuple(list_id)) => {
                let elems = self.ctx.types.tuple_list(list_id);
                elems
                    .iter()
                    .any(|e| self.contains_type_parameter_inner(e.type_id, visited))
            }
            Some(TypeKey::Union(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                members
                    .iter()
                    .any(|m| self.contains_type_parameter_inner(*m, visited))
            }
            Some(TypeKey::Intersection(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                members
                    .iter()
                    .any(|m| self.contains_type_parameter_inner(*m, visited))
            }
            Some(TypeKey::Application(app)) => {
                let app = self.ctx.types.type_application(app);
                if self.contains_type_parameter_inner(app.base, visited) {
                    return true;
                }
                app.args
                    .iter()
                    .any(|a| self.contains_type_parameter_inner(*a, visited))
            }
            _ => false,
        }
    }

    /// Check if a type is concrete (no type parameters).
    ///
    /// A concrete type has no type parameters and can be instantiated directly.
    pub fn is_concrete_type(&self, ty: TypeId) -> bool {
        !self.contains_type_parameter(ty)
    }

    /// Get the depth of type nesting.
    ///
    /// Returns how deeply nested the type structure is (useful for complexity analysis).
    pub fn type_depth(&self, ty: TypeId) -> usize {
        let mut visited = std::collections::HashSet::new();
        self.type_depth_inner(ty, &mut visited)
    }

    fn type_depth_inner(
        &self,
        ty: TypeId,
        visited: &mut std::collections::HashSet<TypeId>,
    ) -> usize {
        if !visited.insert(ty) {
            return 0;
        }

        match self.ctx.types.lookup(ty) {
            Some(TypeKey::Array(elem)) => 1 + self.type_depth_inner(elem, visited),
            Some(TypeKey::Tuple(list_id)) => {
                let elems = self.ctx.types.tuple_list(list_id);
                1 + elems
                    .iter()
                    .map(|e| self.type_depth_inner(e.type_id, visited))
                    .max()
                    .unwrap_or(0)
            }
            Some(TypeKey::Union(list_id) | TypeKey::Intersection(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                1 + members
                    .iter()
                    .map(|m| self.type_depth_inner(*m, visited))
                    .max()
                    .unwrap_or(0)
            }
            Some(TypeKey::Application(app)) => {
                let app = self.ctx.types.type_application(app);
                1 + std::cmp::max(
                    self.type_depth_inner(app.base, visited),
                    app.args
                        .iter()
                        .map(|a| self.type_depth_inner(*a, visited))
                        .max()
                        .unwrap_or(0),
                )
            }
            _ => 1,
        }
    }

    /// Get the type of an identifier expression.
    ///
    /// This function resolves the type of an identifier by:
    /// 1. Looking up the symbol through the binder
    /// 2. Getting the declared type of the symbol
    /// 3. Checking for TDZ (temporal dead zone) violations
    /// 4. Checking definite assignment for block-scoped variables
    /// 5. Applying flow-based type narrowing
    ///
    /// ## Symbol Resolution:
    /// - Uses `resolve_identifier_symbol` to find the symbol
    /// - Checks for type-only aliases (error if used as value)
    /// - Validates that symbol has a value declaration
    ///
    /// ## TDZ Checking:
    /// - Static block TDZ: variable used in static block before declaration
    /// - Computed property TDZ: variable in computed property before declaration
    /// - Heritage clause TDZ: variable in extends/implements before declaration
    ///
    /// ## Definite Assignment:
    /// - Checks if variable is definitely assigned before use
    /// - Only applies to block-scoped variables without initializers
    /// - Skipped for parameters, ambient contexts, and captured variables
    ///
    /// ## Flow Narrowing:
    /// - If definitely assigned, applies type narrowing based on control flow
    /// - Refines union types based on typeof guards, null checks, etc.
    ///
    /// ## Intrinsic Names:
    /// - `undefined` → UNDEFINED type
    /// - `NaN` / `Infinity` → NUMBER type
    /// - `Symbol` → Symbol constructor type (if available in lib)
    ///
    /// ## Global Value Names:
    /// - Returns ANY for available globals (Array, Object, etc.)
    /// - Emits error for unavailable ES2015+ types
    ///
    /// ## Error Handling:
    /// - Returns ERROR for:
    ///   - Type-only aliases used as values
    ///   - Variables used before declaration (TDZ)
    ///   - Variables not definitely assigned
    ///   - Static members accessed without `this`
    ///   - `await` in default parameters
    ///   - Unresolved names (with "cannot find name" error)
    /// - Returns ANY for unresolved imports (TS2307 already emitted)
    pub(crate) fn get_type_of_identifier(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(ident) = self.ctx.arena.get_identifier(node) else {
            return TypeId::ERROR; // Missing identifier data - propagate error
        };

        let name = &ident.escaped_text;

        // Resolve via binder persistent scopes for stateless lookup.
        if let Some(sym_id) = self.resolve_identifier_symbol(idx) {
            if self.alias_resolves_to_type_only(sym_id) {
                self.error_type_only_value_at(name, idx);
                return TypeId::ERROR;
            }
            let flags = self
                .ctx
                .binder
                .get_symbol(sym_id)
                .map(|symbol| symbol.flags)
                .unwrap_or(0);
            let has_type = (flags & crate::binder::symbol_flags::TYPE) != 0;
            let has_value = (flags & crate::binder::symbol_flags::VALUE) != 0;
            let is_interface = (flags & crate::binder::symbol_flags::INTERFACE) != 0;
            let is_type_alias = (flags & crate::binder::symbol_flags::TYPE_ALIAS) != 0;

            // Check for type-only symbols used as values
            // This includes:
            // 1. Symbols with TYPE flag but no VALUE flag
            // 2. Interfaces (TYPE flag, but typically no VALUE unless merged)
            // 3. Type aliases (TYPE flag, never have VALUE)
            if (has_type && !has_value) || is_interface || is_type_alias {
                self.error_type_only_value_at(name, idx);
                return TypeId::ERROR;
            }
            let declared_type = self.get_type_of_symbol(sym_id);
            // Check for TDZ violations (variable used before declaration in source order)
            // 1. Static block TDZ - variable used in static block before its declaration
            // 2. Computed property TDZ - variable used in computed property name before its declaration
            // 3. Heritage clause TDZ - variable used in extends/implements before its declaration
            // Return TypeId::ERROR after emitting TS2454 to prevent cascading errors (e.g., TS2571)
            if self.is_variable_used_before_declaration_in_static_block(sym_id, idx) {
                self.error_variable_used_before_assigned_at(name, idx);
                return TypeId::ERROR;
            } else if self.is_variable_used_before_declaration_in_computed_property(sym_id, idx) {
                self.error_variable_used_before_assigned_at(name, idx);
                return TypeId::ERROR;
            } else if self.is_variable_used_before_declaration_in_heritage_clause(sym_id, idx) {
                self.error_variable_used_before_assigned_at(name, idx);
                return TypeId::ERROR;
            } else if self.should_check_definite_assignment(sym_id, idx)
                && !self.is_definitely_assigned_at(idx)
            {
                self.error_variable_used_before_assigned_at(name, idx);
                return TypeId::ERROR;
            }
            return self.apply_flow_narrowing(idx, declared_type);
        }

        // Intrinsic names - use constant TypeIds
        match name.as_str() {
            "undefined" => TypeId::UNDEFINED,
            "NaN" | "Infinity" => TypeId::NUMBER,
            // Symbol constructor - only synthesize if available in lib contexts or merged into binder
            "Symbol" => {
                // Check if Symbol is available from lib contexts or merged lib symbols
                // This uses has_symbol_in_lib which checks lib_contexts, current_scope, and file_locals
                let symbol_available = self.ctx.has_symbol_in_lib();

                if !symbol_available {
                    // Symbol is not available via lib, check if it resolves to a symbol in scope
                    // If resolve_identifier_symbol already failed (we're here), then Symbol is not in scope
                    // Emit TS2585: Symbol only refers to a type, suggest changing lib
                    use crate::checker::types::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };
                    if let Some(loc) = self.get_source_location(idx) {
                        let message = format_message(
                            diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_WITH_LIB,
                            &[name],
                        );
                        self.ctx.diagnostics.push(Diagnostic {
                            code: diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_WITH_LIB,
                            category: DiagnosticCategory::Error,
                            message_text: message,
                            start: loc.start,
                            length: loc.length(),
                            file: self.ctx.file_name.clone(),
                            related_information: Vec::new(),
                        });
                    }
                    return TypeId::ERROR;
                }
                self.get_symbol_constructor_type()
            }
            _ if self.is_known_global_value_name(name) => {
                // Node.js runtime globals are always available (injected by runtime)
                // We return ANY without emitting an error for these
                if self.is_nodejs_runtime_global(name) {
                    return TypeId::ANY;
                }

                // Check if the global is actually available in lib contexts
                // First use get_global_type for accurate lookup
                let lib_binders = self.get_lib_binders();
                let has_global_type = self
                    .ctx
                    .binder
                    .get_global_type_with_libs(name, &lib_binders)
                    .is_some();
                if has_global_type || self.ctx.has_name_in_lib(name) {
                    // Global is available - return ANY to allow property access
                    TypeId::ANY
                } else {
                    // Global is not available - emit appropriate error
                    // Use TS2583 for ES2015+ types, TS2304 for other globals
                    use crate::lib_loader;
                    if lib_loader::is_es2015_plus_type(name) {
                        // ES2015+ type not available - suggest changing lib
                        self.error_cannot_find_global_type(name, idx);
                    } else {
                        // Standard global not available
                        self.error_cannot_find_name_at(name, idx);
                    }
                    TypeId::ERROR
                }
            }
            _ => {
                // Check if we're inside a class and the name matches a static member (error 2662)
                // Clone values to avoid borrow issues
                if let Some(ref class_info) = self.ctx.enclosing_class.clone()
                    && self.is_static_member(&class_info.member_nodes, name)
                {
                    self.error_cannot_find_name_static_member_at(name, &class_info.name, idx);
                    return TypeId::ERROR;
                }
                // TS2524: 'await' in default parameter - emit specific error
                if name == "await" && self.is_in_default_parameter(idx) {
                    use crate::checker::types::diagnostics::{
                        diagnostic_codes, diagnostic_messages,
                    };
                    self.error_at_node(
                        idx,
                        diagnostic_messages::AWAIT_IN_PARAMETER_DEFAULT,
                        diagnostic_codes::AWAIT_IN_PARAMETER_DEFAULT,
                    );
                    return TypeId::ERROR;
                }
                // Suppress TS2304 if this is an unresolved import (TS2307 was already emitted)
                if self.is_unresolved_import_symbol(idx) {
                    return TypeId::ANY;
                }
                // Report "cannot find name" error
                self.error_cannot_find_name_at(name, idx);
                TypeId::ERROR
            }
        }
    }
}
