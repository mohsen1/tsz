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
use crate::solver::types::Visibility;
use crate::solver::{ContextualTypeContext, TupleElement, TypeId, expression_ops};

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
    ///
    /// Uses solver::compute_conditional_expression_type for type computation
    /// as part of the Solver-First architecture migration.
    pub(crate) fn get_type_of_conditional_expression(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(cond) = self.ctx.arena.get_conditional_expr(node) else {
            return TypeId::ERROR;
        };

        // Get condition type for type computation
        let condition_type = self.get_type_of_node(cond.condition);

        // Apply contextual typing to each branch and check assignability
        let prev_context = self.ctx.contextual_type;
        let (when_true, when_false) = if let Some(contextual) = prev_context {
            // Check whenTrue branch against contextual type
            self.ctx.contextual_type = Some(contextual);
            let when_true = self.get_type_of_node(cond.when_true);

            // Emit TS2322 if whenTrue is not assignable to contextual type
            if contextual != TypeId::ANY
                && contextual != TypeId::UNKNOWN
                && !self.type_contains_error(contextual)
                && !self.is_assignable_to(when_true, contextual)
            {
                self.error_type_not_assignable_with_reason_at(
                    when_true,
                    contextual,
                    cond.when_true,
                );
            }

            // Check whenFalse branch against contextual type
            self.ctx.contextual_type = Some(contextual);
            let when_false = self.get_type_of_node(cond.when_false);

            // Emit TS2322 if whenFalse is not assignable to contextual type
            if contextual != TypeId::ANY
                && contextual != TypeId::UNKNOWN
                && !self.type_contains_error(contextual)
                && !self.is_assignable_to(when_false, contextual)
            {
                self.error_type_not_assignable_with_reason_at(
                    when_false,
                    contextual,
                    cond.when_false,
                );
            }

            self.ctx.contextual_type = prev_context;
            (when_true, when_false)
        } else {
            // No contextual type - just compute branch types
            let when_true = self.get_type_of_node(cond.when_true);
            let when_false = self.get_type_of_node(cond.when_false);
            (when_true, when_false)
        };

        // Use Solver API for type computation (Solver-First architecture)
        expression_ops::compute_conditional_expression_type(
            self.ctx.types,
            condition_type,
            when_true,
            when_false,
        )
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
            // Empty array literal: infer from context or use any[]
            // TypeScript uses "evolving array types" where [] starts as never[] and widens
            // via control flow. Since we don't yet support evolving arrays, use any[] to
            // avoid false TS2322 errors on subsequent element assignments.
            if let Some(contextual) = self.ctx.contextual_type {
                // Resolve lazy types (type aliases) before using the contextual type
                let resolved = self.resolve_type_for_property_access(contextual);
                return self.resolve_lazy_type(resolved);
            }
            return self.ctx.types.array(TypeId::ANY);
        }

        // Resolve lazy type aliases once and reuse for both tuple_context and ctx_helper
        // This ensures type aliases (e.g., type Tup = [string, number]) are expanded
        // before checking for tuple elements and providing contextual typing
        let resolved_contextual_type = match self.ctx.contextual_type {
            Some(ctx_type) => Some(self.resolve_lazy_type(ctx_type)),
            None => None,
        };

        let tuple_context = match resolved_contextual_type {
            Some(resolved) => {
                // Evaluate Application types to get their structural form
                // This handles cases like: type MyTuple<T, U> = [T, U]; function f<A, B>(): MyTuple<A, B>
                let evaluated = self.evaluate_application_type(resolved);
                crate::solver::type_queries::get_tuple_elements(self.ctx.types, evaluated)
            }
            None => None,
        };

        let ctx_helper = match resolved_contextual_type {
            Some(resolved) => Some(ContextualTypeContext::with_expected(
                self.ctx.types,
                resolved,
            )),
            None => None,
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
                // Clear application eval cache when contextual types change to ensure
                // generic function type inference uses updated contextual information
                self.ctx.application_eval_cache.clear();
            }

            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let elem_is_spread = elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT;

            // Handle spread elements - expand tuple types
            if elem_is_spread {
                if let Some(spread_data) = self.ctx.arena.get_spread(elem_node) {
                    let spread_expr_type = self.get_type_of_node(spread_data.expression);
                    let spread_expr_type = self.resolve_lazy_type(spread_expr_type);
                    // Check if spread argument is iterable, emit TS2488 if not
                    self.check_spread_iterability(spread_expr_type, spread_data.expression);

                    // If it's a tuple type, expand its elements
                    if let Some(elems) = crate::solver::type_queries::get_tuple_elements(
                        self.ctx.types,
                        spread_expr_type,
                    ) {
                        if let Some(ref _expected) = tuple_context {
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

                    if let Some(ref _expected) = tuple_context {
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

            if let Some(ref _expected) = tuple_context {
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

        // Use Solver API for Best Common Type computation (Solver-First architecture)
        let element_type = expression_ops::compute_best_common_type(
            self.ctx.types,
            &element_types,
            Some(&self.ctx), // Pass TypeResolver for class hierarchy BCT
        );

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
            // Note: tsc does NOT validate operand types for unary +/-. Unary + is a
            // common idiom for number conversion (+someString), and tsc allows it freely.
            k if k == SyntaxKind::PlusToken as u16 || k == SyntaxKind::MinusToken as u16 => {
                // Evaluate operand for side effects / flow analysis but don't type-check it
                self.get_type_of_node(unary.operand);

                if let Some(literal_type) = self.literal_type_from_initializer(idx)
                    && self.contextual_literal_type(literal_type).is_some()
                {
                    return literal_type;
                }
                TypeId::NUMBER
            }
            // ~ (bitwise NOT) returns number
            // Note: tsc does NOT validate operand types for ~, same as unary +/-
            k if k == SyntaxKind::TildeToken as u16 => {
                // Evaluate operand for side effects / flow analysis but don't type-check it
                self.get_type_of_node(unary.operand);
                TypeId::NUMBER
            }
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
                    // Emit TS2356 for invalid increment/decrement operand type
                    if let Some(loc) = self.get_source_location(unary.operand) {
                        use crate::checker::types::diagnostics::{
                            diagnostic_codes, diagnostic_messages,
                        };
                        self.ctx.diagnostics.push(Diagnostic {
                            code: diagnostic_codes::ARITHMETIC_OPERAND_MUST_BE_NUMBER,
                            category: DiagnosticCategory::Error,
                            message_text: diagnostic_messages::ARITHMETIC_OPERAND_MUST_BE_NUMBER
                                .to_string(),
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
    ///
    /// Uses solver::compute_template_expression_type for type computation
    /// as part of the Solver-First architecture migration.
    pub(crate) fn get_type_of_template_expression(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::STRING;
        };

        let Some(template) = self.ctx.arena.get_template_expr(node) else {
            return TypeId::STRING;
        };

        // Type-check each template span's expression and collect types for solver
        let mut part_types = Vec::new();
        for &span_idx in &template.template_spans.nodes {
            let Some(span_node) = self.ctx.arena.get(span_idx) else {
                continue;
            };

            let Some(span) = self.ctx.arena.get_template_span(span_node) else {
                continue;
            };

            // Type-check the expression - this will emit TS2304 if name is unresolved
            let part_type = self.get_type_of_node(span.expression);
            part_types.push(part_type);
        }

        // Use Solver API for type computation (Solver-First architecture)
        // Template literals always produce string type, but we check for ERROR/NEVER propagation
        expression_ops::compute_template_expression_type(self.ctx.types, &part_types)
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
            let init_type = self.get_type_of_node(var_decl.initializer);

            // Rule #10: Literal Widening
            // For mutable bindings (let/var), widen literals to their primitive type
            // For const bindings, preserve literal types (unless in array/object context)
            if !self.is_const_variable_declaration(idx) {
                // let/var: widen literals
                return self.widen_literal_type(init_type);
            }

            // const: preserve literal type
            init_type
        } else {
            // No initializer - use UNKNOWN to enforce strict checking
            // This requires explicit type annotation or prevents unsafe usage
            TypeId::UNKNOWN
        }
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
        use crate::solver::PropertyAccessResult;

        let object_type = self.resolve_type_for_property_access(object_type);
        let result_type = match self.resolve_property_access_with_env(object_type, property_name) {
            PropertyAccessResult::Success { type_id, .. } => type_id,
            PropertyAccessResult::PropertyNotFound { .. } => {
                // Don't emit TS2339 for private fields (starting with #) - they're handled elsewhere
                if !property_name.starts_with('#') {
                    self.error_property_not_exist_at(property_name, object_type, idx);
                }
                TypeId::ERROR
            }
            PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                // Use ERROR instead of UNKNOWN to prevent TS2571 errors
                property_type.unwrap_or(TypeId::ERROR)
            }
            PropertyAccessResult::IsUnknown => {
                // TS2339: Property does not exist on type 'unknown'
                // Use the same error as TypeScript for property access on unknown
                self.error_property_not_exist_at(property_name, object_type, idx);
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
                    visibility: Visibility::Public,
                    parent_id: None,
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
                visibility: Visibility::Public,
                parent_id: None,
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
                // TS2695: Only emit when neither side is ERROR/ANY/UNKNOWN (cascade prevention)
                // TypeScript suppresses this diagnostic when allowUnreachableCode is enabled
                if !self.ctx.compiler_options.allow_unreachable_code
                    && left_type != TypeId::ERROR
                    && left_type != TypeId::ANY
                    && left_type != TypeId::UNKNOWN
                    && self.is_side_effect_free(left_idx)
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

            // Logical AND: `a && b`
            // In TypeScript, the result type is the falsy parts of `a` unioned with `b`.
            // When `a` is `boolean` (common case: comparisons, type guards), the falsy
            // part is `false` which TypeScript drops, yielding just `typeof b`.
            if op_kind == SyntaxKind::AmpersandAmpersandToken as u16 {
                if left_type == TypeId::ERROR || right_type == TypeId::ERROR {
                    type_stack.push(TypeId::ERROR);
                    continue;
                }
                // When the left type is boolean (comparisons, type guards), the result
                // is just the right type.  Otherwise fall back to union.
                let result = if left_type == TypeId::BOOLEAN {
                    right_type
                } else {
                    self.ctx.types.union2(left_type, right_type)
                };
                type_stack.push(result);
                continue;
            }

            // Logical OR: `a || b`
            // In TypeScript, the result type is the truthy parts of `a` unioned with `b`.
            // When `a` is `boolean`, the truthy part is `true` which is `boolean`, so
            // we keep the full union.
            if op_kind == SyntaxKind::BarBarToken as u16 {
                if left_type == TypeId::ERROR || right_type == TypeId::ERROR {
                    type_stack.push(TypeId::ERROR);
                    continue;
                }
                type_stack.push(self.ctx.types.union2(left_type, right_type));
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
                // && and || are handled above
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
                        k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => {
                            ">>>"
                        }
                        _ => "?",
                    };
                    let result = evaluator.evaluate(left_type, right_type, op_str);
                    let result_type = match result {
                        BinaryOpResult::Success(result_type) => result_type,
                        BinaryOpResult::TypeError { .. } => {
                            // Emit appropriate error for arithmetic type mismatch
                            self.emit_binary_operator_error(
                                node_idx, left_idx, right_idx, left_type, right_type, op_str,
                            );
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
                    // Check if this is actually valid because we have enum types
                    // The evaluator doesn't have access to symbol information, so it can't
                    // detect enum types. We need to check here at the checker layer.
                    let left_is_enum = self.is_enum_type(left_type);
                    let right_is_enum = self.is_enum_type(right_type);
                    let is_arithmetic_op = matches!(op_str, "+" | "-" | "*" | "/" | "%" | "**");

                    // If both operands are enum types and this is an arithmetic operation,
                    // treat it as valid (enum members are numbers for numeric enums)
                    if is_arithmetic_op && left_is_enum && right_is_enum {
                        // For + operation, result is number; for other ops, also number
                        TypeId::NUMBER
                    } else if is_arithmetic_op
                        && left_is_enum
                        && evaluator.is_arithmetic_operand(right)
                    {
                        // Enum op number => number
                        TypeId::NUMBER
                    } else if is_arithmetic_op
                        && right_is_enum
                        && evaluator.is_arithmetic_operand(left)
                    {
                        // Number op enum => number
                        TypeId::NUMBER
                    } else {
                        // Emit appropriate error for arithmetic type mismatch
                        self.emit_binary_operator_error(
                            node_idx, left_idx, right_idx, left, right, op,
                        );
                        TypeId::UNKNOWN
                    }
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
        use crate::solver::PropertyAccessResult;

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
        // TS18050: Cannot access elements on 'never' type (impossible union after narrowing)
        if object_type == TypeId::NEVER {
            use crate::checker::types::diagnostics::{
                diagnostic_codes, diagnostic_messages, format_message,
            };
            let message =
                format_message(diagnostic_messages::VALUE_CANNOT_BE_USED_HERE, &["never"]);
            self.error_at_node(
                access.expression,
                &message,
                diagnostic_codes::VALUE_CANNOT_BE_USED_HERE,
            );
            return TypeId::NEVER;
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
                // Type is entirely nullish - emit TS18050 "The value X cannot be used here"
                self.report_nullish_object(access.expression, cause, true);
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
                // Don't emit TS2693 in heritage clause context — the heritage
                // checker will emit the appropriate error (e.g., TS2689).
                if self
                    .find_enclosing_heritage_clause(access.name_or_argument)
                    .is_none()
                {
                    self.error_type_only_value_at(name, access.name_or_argument);
                }
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
            // Resolve type references (Ref, TypeQuery, etc.) before property access lookup
            let resolved_type = self.resolve_type_for_property_access(object_type_for_access);
            let result = self.resolve_property_access_with_env(resolved_type, &property_name);
            result_type = Some(match result {
                PropertyAccessResult::Success { type_id, .. } => type_id,
                PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                    // Use ERROR instead of UNKNOWN to prevent TS2571 errors
                    property_type.unwrap_or(TypeId::ERROR)
                }
                PropertyAccessResult::IsUnknown => {
                    // TS2339: Property does not exist on type 'unknown'
                    // Use the same error as TypeScript for property access on unknown
                    self.error_property_not_exist_at(
                        &property_name,
                        object_type_for_access,
                        access.name_or_argument,
                    );
                    TypeId::ERROR
                }
                PropertyAccessResult::PropertyNotFound { .. } => {
                    // TypeScript does NOT emit TS2339 for element access (bracket notation)
                    // when the property doesn't exist. It returns 'any' instead.
                    // This is different from property access (dot notation) which does emit TS2339.
                    // Only mark report_no_index if we should report a missing index signature error
                    // (which is TS7053, not TS2339)
                    report_no_index = true;
                    TypeId::ANY
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
    /// Uses ElementAccessEvaluator from solver for structured error handling.
    pub(crate) fn get_element_access_type(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> TypeId {
        use crate::checker::state::EnumKind;
        use crate::solver::element_access::{ElementAccessEvaluator, ElementAccessResult};

        let cache_key = (object_type, index_type, literal_index);
        if let Some(&cached) = self.ctx.element_access_type_cache.get(&cache_key) {
            return cached;
        }
        if !self.ctx.element_access_type_set.insert(cache_key) {
            return TypeId::ANY;
        }

        // Normalize index type for enum values
        let solver_index_type = if let Some(index) = literal_index {
            self.ctx.types.literal_number(index as f64)
        } else if self
            .enum_symbol_from_type(index_type)
            .is_some_and(|sym_id| self.enum_kind(sym_id) == Some(EnumKind::Numeric))
        {
            // Numeric enum values are number-like at runtime.
            TypeId::NUMBER
        } else {
            index_type
        };

        // Use ElementAccessEvaluator for structured results
        let mut evaluator = ElementAccessEvaluator::new(self.ctx.types);
        evaluator.set_no_unchecked_indexed_access(self.ctx.no_unchecked_indexed_access());

        let result =
            match evaluator.resolve_element_access(object_type, solver_index_type, literal_index) {
                ElementAccessResult::Success(ty) => {
                    // UNDEFINED from evaluator means "not found" - fallback to ANY
                    if ty == TypeId::UNDEFINED {
                        TypeId::ANY
                    } else {
                        ty
                    }
                }
                ElementAccessResult::IndexOutOfBounds {
                    type_id: _,
                    index: _,
                    length: _,
                } => {
                    // TS2493 - Tuple index out of bounds (reported by caller via get_type_of_element_access)
                    // Return ERROR here; diagnostic is handled at call site with node context
                    TypeId::ERROR
                }
                ElementAccessResult::NotIndexable { .. } => {
                    // Object is not indexable - return ERROR
                    TypeId::ERROR
                }
                ElementAccessResult::NoIndexSignature { .. } => {
                    // TS7053 - No index signature (reported by caller)
                    TypeId::ANY
                }
            };

        self.ctx.element_access_type_set.remove(&cache_key);
        self.ctx.element_access_type_cache.insert(cache_key, result);
        result
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
        use crate::solver::PropertyInfo;
        use rustc_hash::FxHashMap;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
            return TypeId::ERROR; // Missing object literal data - propagate error
        };

        // Collect properties from the object literal (later entries override earlier ones)
        let mut properties: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        // Track getter/setter names to allow getter+setter pairs with the same name
        let mut getter_names: rustc_hash::FxHashSet<Atom> = rustc_hash::FxHashSet::default();
        let mut setter_names: rustc_hash::FxHashSet<Atom> = rustc_hash::FxHashSet::default();

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
                            visibility: Visibility::Public,
                            parent_id: None,
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
                            visibility: Visibility::Public,
                            parent_id: None,
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
                            is_method: true, // Object literal methods should be bivariant
                            visibility: Visibility::Public,
                            parent_id: None,
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

                    // Check for duplicate property - but allow getter+setter pairs
                    // A getter and setter with the same name is valid, not a duplicate
                    let is_getter = elem_node.kind == syntax_kind_ext::GET_ACCESSOR;
                    let is_complementary_pair = if is_getter {
                        setter_names.contains(&name_atom) && !getter_names.contains(&name_atom)
                    } else {
                        getter_names.contains(&name_atom) && !setter_names.contains(&name_atom)
                    };
                    if properties.contains_key(&name_atom) && !is_complementary_pair {
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

                    if is_getter {
                        getter_names.insert(name_atom);
                    } else {
                        setter_names.insert(name_atom);
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
                            visibility: Visibility::Public,
                            parent_id: None,
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
        let object_type = self.ctx.types.object_fresh(properties);

        // NOTE: Freshness is now tracked on the TypeId via ObjectFlags.
        // This fixes the "Zombie Freshness" bug by distinguishing fresh vs
        // non-fresh object types at interning time.

        object_type
    }

    /// Collect properties from a spread expression in an object literal.
    ///
    /// Given the type of the spread expression, extracts all properties that would
    /// be spread into the object literal.
    pub(crate) fn collect_object_spread_properties(
        &mut self,
        type_id: TypeId,
    ) -> Vec<crate::solver::PropertyInfo> {
        use crate::interner::Atom;
        use crate::solver::type_queries::{SpreadPropertyKind, classify_for_spread_properties};
        use rustc_hash::FxHashMap;

        let resolved = self.resolve_type_for_property_access(type_id);
        let resolved = self.resolve_lazy_type(resolved);
        if let Some(cached) = self.ctx.object_spread_property_cache.get(&resolved) {
            return cached.clone();
        }
        if !self.ctx.object_spread_property_set.insert(resolved) {
            return Vec::new();
        }

        let properties = match classify_for_spread_properties(self.ctx.types, resolved) {
            SpreadPropertyKind::Object(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                shape.properties.to_vec()
            }
            SpreadPropertyKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                shape.properties.to_vec()
            }
            SpreadPropertyKind::Intersection(members) => {
                let mut merged: FxHashMap<Atom, crate::solver::PropertyInfo> = FxHashMap::default();
                for member in members {
                    for prop in self.collect_object_spread_properties(member) {
                        merged.insert(prop.name, prop);
                    }
                }
                merged.into_values().collect()
            }
            SpreadPropertyKind::NoProperties => Vec::new(),
        };

        self.ctx.object_spread_property_set.remove(&resolved);
        self.ctx
            .object_spread_property_cache
            .insert(resolved, properties.clone());
        properties
    }
}
