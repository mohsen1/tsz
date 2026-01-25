//! Function Type Resolution Module
//!
//! This module contains function type resolution methods for CheckerState
//! as part of the Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Function/method/arrow function type resolution
//! - Parameter type inference and contextual typing
//! - Return type inference and validation
//! - Property access type resolution
//! - Async function Promise return type validation
//!
//! This module extends CheckerState with utilities for function type
//! resolution, providing cleaner separation of function typing logic.

use crate::binder::symbol_flags;
use crate::checker::state::{CheckerState, MAX_INSTANTIATION_DEPTH};
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::{ContextualTypeContext, TypeId};

// =============================================================================
// Function Type Resolution
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Function Type Resolution
    // =========================================================================

    /// Get type of function declaration/expression/arrow.
    pub(crate) fn get_type_of_function(&mut self, idx: NodeIndex) -> TypeId {
        use crate::solver::{FunctionShape, ParamInfo};

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        // Determine if this is a function expression or arrow function (a closure)
        let is_closure = matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::ARROW_FUNCTION
        );

        // Rule #42: Increment closure depth when entering a function expression or arrow function
        // This causes mutable variables (let/var) to lose narrowing inside the closure
        if is_closure {
            self.ctx.inside_closure_depth += 1;
        }

        // Helper macro to decrement closure depth before returning
        // This ensures we properly track closure depth even on early returns
        macro_rules! return_with_cleanup {
            ($expr:expr) => {{
                if is_closure {
                    self.ctx.inside_closure_depth -= 1;
                }
                $expr
            }};
        }

        let (type_parameters, parameters, type_annotation, body, name_node, name_for_error) =
            if let Some(func) = self.ctx.arena.get_function(node) {
                let name_node = if func.name.is_none() {
                    None
                } else {
                    Some(func.name)
                };
                let name_for_error = if func.name.is_none() {
                    None
                } else {
                    self.get_function_name_from_node(idx)
                };
                (
                    &func.type_parameters,
                    &func.parameters,
                    func.type_annotation,
                    func.body,
                    name_node,
                    name_for_error,
                )
            } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                (
                    &method.type_parameters,
                    &method.parameters,
                    method.type_annotation,
                    method.body,
                    Some(method.name),
                    self.property_name_for_error(method.name),
                )
            } else {
                return return_with_cleanup!(TypeId::ERROR); // Missing function/method data - propagate error
            };

        // Function declarations don't report implicit any for parameters (handled by check_statement)
        let is_function_declaration = node.kind == syntax_kind_ext::FUNCTION_DECLARATION;
        let is_method_or_constructor = matches!(
            node.kind,
            syntax_kind_ext::METHOD_DECLARATION | syntax_kind_ext::CONSTRUCTOR
        );
        let is_arrow_function = node.kind == syntax_kind_ext::ARROW_FUNCTION;

        // Check for duplicate parameter names in function expressions and arrow functions (TS2300)
        // Note: Methods and constructors are checked in check_method_declaration and check_constructor_declaration
        // Function declarations are checked in check_statement
        if !is_function_declaration && !is_method_or_constructor {
            self.check_duplicate_parameters(parameters);
            // Check for required parameters following optional parameters (TS1016)
            self.check_parameter_ordering(parameters);
        }

        let (type_params, type_param_updates) = self.push_type_parameters(type_parameters);

        // Collect parameter info using solver's ParamInfo struct
        let mut params = Vec::new();
        let mut param_types: Vec<Option<TypeId>> = Vec::new();
        let mut this_type = None;
        let this_atom = self.ctx.types.intern_string("this");

        // Setup contextual typing context if available
        let ctx_helper = if let Some(ctx_type) = self.ctx.contextual_type {
            Some(ContextualTypeContext::with_expected(
                self.ctx.types,
                ctx_type,
            ))
        } else {
            None
        };

        // For arrow functions, capture the outer `this` type to preserve lexical `this`
        // Arrow functions should inherit `this` from their enclosing scope
        let outer_this_type = if is_arrow_function {
            self.current_this_type()
        } else {
            None
        };

        let mut contextual_index = 0;
        for &param_idx in parameters.nodes.iter() {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
            {
                // Get parameter name
                let name = if let Some(name_node) = self.ctx.arena.get(param.name) {
                    if let Some(name_data) = self.ctx.arena.get_identifier(name_node) {
                        Some(self.ctx.types.intern_string(&name_data.escaped_text))
                    } else {
                        None
                    }
                } else {
                    None
                };

                let is_this_param = name == Some(this_atom);

                let contextual_type = if let Some(ref helper) = ctx_helper {
                    helper.get_parameter_type(contextual_index)
                } else {
                    None
                };
                // TS7006: Only count as contextual type if it's not UNKNOWN
                // UNKNOWN is a "no type" value and shouldn't prevent implicit any errors
                let has_contextual_type = contextual_type.is_some_and(|t| t != TypeId::UNKNOWN);

                // Use type annotation if present, otherwise infer from context
                let type_id = if !param.type_annotation.is_none() {
                    // Check parameter type for parameter properties in function types
                    self.check_type_for_parameter_properties(param.type_annotation);
                    self.get_type_from_type_node(param.type_annotation)
                } else if is_this_param {
                    // For `this` parameter without type annotation:
                    // - Arrow functions: inherit outer `this` type to preserve lexical scoping
                    // - Regular functions: use ANY (will trigger TS2683 when used, not TS2571)
                    // - Contextual type: if provided, use it (for function types with explicit `this`)
                    if let Some(ref helper) = ctx_helper {
                        helper
                            .get_this_type()
                            .or(outer_this_type)
                            .unwrap_or(TypeId::ANY)
                    } else {
                        outer_this_type.unwrap_or(TypeId::ANY)
                    }
                } else {
                    // Infer from contextual type
                    contextual_type.unwrap_or(TypeId::UNKNOWN)
                };

                if is_this_param {
                    if this_type.is_none() {
                        this_type = Some(type_id);
                    }
                    param_types.push(None);
                    continue;
                }

                if !is_function_declaration {
                    self.maybe_report_implicit_any_parameter(param, has_contextual_type);
                }

                // Check if optional or has initializer
                let optional = param.question_token || !param.initializer.is_none();
                let rest = param.dot_dot_dot_token;

                params.push(ParamInfo {
                    name,
                    type_id,
                    optional,
                    rest,
                });
                param_types.push(Some(type_id));
                contextual_index += 1;
            }
        }

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors, not in regular functions
        self.check_parameter_properties(&parameters.nodes);

        // Get return type from annotation or infer
        let has_type_annotation = !type_annotation.is_none();
        let (mut return_type, type_predicate) = if has_type_annotation {
            // Check return type for parameter properties in function types
            self.check_type_for_parameter_properties(type_annotation);
            self.return_type_and_predicate(type_annotation)
        } else {
            // Use UNKNOWN as default to enforce strict checking
            // This ensures return statements are checked even without annotation
            (TypeId::UNKNOWN, None)
        };

        // Evaluate Application types in return type to get their structural form
        // This allows proper comparison of return expressions against type alias applications like Reducer<S, A>
        return_type = self.evaluate_application_type(return_type);

        // Check the function body (for type errors within the body)
        if !body.is_none() {
            self.cache_parameter_types(&parameters.nodes, Some(&param_types));

            // Assign contextual types to destructuring parameters (binding patterns)
            // This allows destructuring patterns in callbacks to infer element types from contextual types
            self.assign_contextual_types_to_destructuring_params(&parameters.nodes, &param_types);

            // Check that parameter default values are assignable to declared types (TS2322)
            self.check_parameter_initializers(&parameters.nodes);

            let mut has_contextual_return = false;
            if !has_type_annotation {
                let return_context = ctx_helper
                    .as_ref()
                    .and_then(|helper| helper.get_return_type());
                // TS7010/TS7011: Only count as contextual return if it's not UNKNOWN
                // UNKNOWN is a "no type" value and shouldn't prevent implicit any errors
                has_contextual_return = return_context.is_some_and(|t| t != TypeId::UNKNOWN);
                return_type = self.infer_return_type_from_body(body, return_context);
            }

            // TS7010/TS7011 (implicit any return) is emitted for functions without
            // return type annotations when noImplicitAny is enabled and the return
            // type cannot be inferred (e.g., is 'any' or only returns undefined)
            // maybe_report_implicit_any_return handles the noImplicitAny check internally
            if !is_function_declaration {
                self.maybe_report_implicit_any_return(
                    name_for_error,
                    name_node,
                    return_type,
                    has_type_annotation,
                    has_contextual_return,
                    idx,
                );
            }

            // Check async function requirements
            let (is_async, is_generator, _async_node_idx): (bool, bool, NodeIndex) =
                if let Some(func) = self.ctx.arena.get_function(node) {
                    (func.is_async, func.asterisk_token, func.name)
                } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    (
                        self.has_async_modifier(&method.modifiers),
                        method.asterisk_token,
                        method.name,
                    )
                } else {
                    (false, false, NodeIndex::NONE)
                };

            // TS2697: Check if async function has access to Promise type
            // DISABLED: Causes too many false positives (313x extra errors)
            // The is_promise_global_available check doesn't correctly detect Promise in lib files
            // TODO: Investigate lib loading for Promise detection
            // if is_async && !is_generator && !self.is_promise_global_available() {
            //     use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
            //     let diagnostic_node = if async_node_idx.is_none() {
            //         idx
            //     } else {
            //         async_node_idx
            //     };
            //     self.error_at_node(
            //         diagnostic_node,
            //         diagnostic_messages::ASYNC_FUNCTION_MUST_RETURN_PROMISE,
            //         diagnostic_codes::ASYNC_FUNCTION_MUST_RETURN_PROMISE,
            //     );
            // }

            // TS2705: Async function must return Promise
            // Check ALL async functions (not just arrow functions and function expressions)
            // Note: Async generators (async function* or async *method) should NOT trigger TS2705
            // because they return AsyncGenerator or AsyncIterator, not Promise
            if has_type_annotation {
                // Only check non-generator async functions with explicit return types that aren't Promise
                // Skip this check if:
                // 1. The return type is ERROR (unresolved reference, likely Promise not in lib)
                // 2. The return type annotation text looks like it references Promise
                if is_async && !is_generator {
                    let should_emit_ts2705 = !self.is_promise_type(return_type)
                        && return_type != TypeId::ERROR
                        && !self.return_type_annotation_looks_like_promise(type_annotation);

                    if should_emit_ts2705 {
                        use crate::checker::types::diagnostics::{
                            diagnostic_codes, diagnostic_messages,
                        };
                        self.error_at_node(
                            type_annotation,
                            diagnostic_messages::ASYNC_FUNCTION_RETURNS_PROMISE,
                            diagnostic_codes::ASYNC_FUNCTION_RETURNS_PROMISE,
                        );
                    }
                }

                // Note: TS2705 check for functions without explicit return types has been removed.
                // TypeScript only emits TS2705 when there's an explicit non-Promise return type.
                // The "Promise not in lib" check was also removed as it was emitting false positives.
            }

            // TS2366 (not all code paths return value) for function expressions and arrow functions
            // Check if all code paths return a value when return type requires it
            if !is_function_declaration && !body.is_none() {
                let check_return_type = return_type;
                let requires_return = self.requires_return_value(check_return_type);
                let has_return = self.body_has_return_with_value(body);
                let falls_through = self.function_body_falls_through(body);

                // Determine if this is an async function
                let is_async = if let Some(func) = self.ctx.arena.get_function(node) {
                    func.is_async
                } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    self.has_async_modifier(&method.modifiers)
                } else {
                    false
                };

                // TS2355: Skip for async functions - they implicitly return Promise<void>
                // Async functions without a return statement automatically resolve to Promise<void>
                // so they should not emit "function must return a value" errors
                if has_type_annotation && requires_return && falls_through && !is_async {
                    use crate::checker::types::diagnostics::{
                        diagnostic_codes, diagnostic_messages,
                    };
                    if !has_return {
                        self.error_at_node(
                            type_annotation,
                            "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                            diagnostic_codes::FUNCTION_LACKS_RETURN_TYPE,
                        );
                    } else {
                        self.error_at_node(
                            type_annotation,
                            diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT,
                            diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_VALUE,
                        );
                    }
                } else if self.ctx.no_implicit_returns() && has_return && falls_through {
                    // TS7030: noImplicitReturns - not all code paths return a value
                    use crate::checker::types::diagnostics::{
                        diagnostic_codes, diagnostic_messages,
                    };
                    let error_node = if let Some(nn) = name_node { nn } else { body };
                    self.error_at_node(
                        error_node,
                        diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN,
                        diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN,
                    );
                }
            }

            // Determine if this is an async function for context tracking
            let is_async_for_context = if let Some(func) = self.ctx.arena.get_function(node) {
                func.is_async
            } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                self.has_async_modifier(&method.modifiers)
            } else {
                false
            };

            // Enter async context if applicable
            if is_async_for_context {
                self.ctx.enter_async_context();
            }

            // Push this_type to the stack before checking the body
            // This ensures this references inside the function have the proper type context
            // For functions with explicit this parameter: use that type
            // For arrow functions: use outer this type (already captured in this_type)
            // For regular functions without explicit this: this_type is None, which triggers TS2683 when this is used
            let mut pushed_this_type = false;
            if let Some(this_type) = this_type {
                self.ctx.this_type_stack.push(this_type);
                pushed_this_type = true;
            }

            self.push_return_type(return_type);
            self.check_statement(body);
            self.pop_return_type();

            if pushed_this_type {
                self.ctx.this_type_stack.pop();
            }

            // Exit async context
            if is_async_for_context {
                self.ctx.exit_async_context();
            }
        }

        // Create function type using TypeInterner
        let shape = FunctionShape {
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
            is_constructor: false,
            is_method: false,
        };

        self.pop_type_parameters(type_param_updates);

        return_with_cleanup!(self.ctx.types.function(shape))
    }

    // =========================================================================
    // Property Access Type Resolution
    // =========================================================================

    /// Get type of property access expression.
    pub(crate) fn get_type_of_property_access(&mut self, idx: NodeIndex) -> TypeId {
        if *self.ctx.instantiation_depth.borrow() >= MAX_INSTANTIATION_DEPTH {
            return TypeId::ERROR; // Max instantiation depth exceeded - propagate error
        }

        *self.ctx.instantiation_depth.borrow_mut() += 1;
        let result = self.get_type_of_property_access_inner(idx);
        *self.ctx.instantiation_depth.borrow_mut() -= 1;
        result
    }

    /// Inner implementation of property access type resolution.
    fn get_type_of_property_access_inner(&mut self, idx: NodeIndex) -> TypeId {
        use crate::solver::{PropertyAccessResult, QueryDatabase};

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return TypeId::ERROR; // Missing access expression data - propagate error
        };

        // Get the property name first (needed for abstract property check regardless of object type)
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return TypeId::ERROR; // Missing name node - propagate error
        };
        if let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && ident.escaped_text.is_empty()
        {
            return TypeId::ERROR; // Empty identifier - propagate error
        }

        // Check for abstract property access in constructor BEFORE evaluating types (error 2715)
        // This must happen even when `this` has type ANY
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            if self.is_this_expression(access.expression)
                && let Some(ref class_info) = self.ctx.enclosing_class.clone()
                && class_info.in_constructor
                && self.is_abstract_member(&class_info.member_nodes, property_name)
            {
                self.error_abstract_property_in_constructor(
                    property_name,
                    &class_info.name,
                    access.name_or_argument,
                );
            }
        }

        // Get the type of the object
        let object_type = self.get_type_of_node(access.expression);

        // Evaluate Application types to resolve generic type aliases/interfaces
        let object_type = self.evaluate_application_type(object_type);

        if name_node.kind == SyntaxKind::PrivateIdentifier as u16 {
            return self.get_type_of_private_property_access(
                idx,
                access,
                access.name_or_argument,
                object_type,
            );
        }

        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;
            if self.is_global_this_expression(access.expression) {
                let property_type =
                    self.resolve_global_this_property_type(property_name, access.name_or_argument);
                if property_type == TypeId::ERROR {
                    return TypeId::ERROR;
                }
                return self.apply_flow_narrowing(idx, property_type);
            }
        }

        // Enforce private/protected access modifiers when possible
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;
            if !self.check_property_accessibility(
                access.expression,
                property_name,
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
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        // Check for merged class/enum/function + namespace symbols
        // When a class/enum/function merges with a namespace (same name), the symbol has both
        // value constructor flags and MODULE flags. We need to check the symbol's exports.
        // This handles value access like `Foo.value` when Foo is both a class and namespace.
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            // For value access to merged symbols, check the exports directly
            // This is needed because the type system doesn't track which symbol a Callable came from
            if let Some(expr_node) = self.ctx.arena.get(access.expression)
                && let Some(expr_ident) = self.ctx.arena.get_identifier(expr_node)
            {
                let expr_name = &expr_ident.escaped_text;
                // Try file_locals first (fast path for top-level symbols)
                if let Some(sym_id) = self.ctx.binder.file_locals.get(expr_name)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                {
                    // Check if this is a merged symbol (has both MODULE and value constructor flags)
                    let is_merged = (symbol.flags & symbol_flags::MODULE) != 0
                        && (symbol.flags
                            & (symbol_flags::CLASS
                                | symbol_flags::FUNCTION
                                | symbol_flags::REGULAR_ENUM))
                            != 0;

                    if is_merged
                        && let Some(exports) = symbol.exports.as_ref()
                        && let Some(member_id) = exports.get(property_name)
                    {
                        // For merged symbols, we return the type for any exported member
                        let member_type = self.get_type_of_symbol(member_id);
                        return self.apply_flow_narrowing(idx, member_type);
                    }
                }
            }
        }

        // If it's an identifier, look up the property
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            if let Some(member_type) =
                self.resolve_namespace_value_member(object_type, property_name)
            {
                return self.apply_flow_narrowing(idx, member_type);
            }
            if self.namespace_has_type_only_member(object_type, property_name) {
                self.error_type_only_value_at(property_name, access.name_or_argument);
                return TypeId::ERROR;
            }

            let object_type_for_access = self.resolve_type_for_property_access(object_type);
            if object_type_for_access == TypeId::ANY {
                return TypeId::ANY;
            }
            if object_type_for_access == TypeId::ERROR {
                return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
            }

            // Use solver QueryDatabase to resolve the property access
            let result = self
                .ctx
                .types
                .property_access_type(object_type_for_access, property_name);

            match result {
                PropertyAccessResult::Success {
                    type_id: prop_type,
                    from_index_signature,
                } => {
                    // Check for error 4111: property access from index signature
                    if from_index_signature {
                        use crate::checker::types::diagnostics::diagnostic_codes;
                        self.error_at_node(
                            access.name_or_argument,
                            &format!(
                                "Property '{}' comes from an index signature, so it must be accessed with ['{}'].",
                                property_name, property_name
                            ),
                            diagnostic_codes::PROPERTY_ACCESS_FROM_INDEX_SIGNATURE,
                        );
                    }
                    self.apply_flow_narrowing(idx, prop_type)
                }

                PropertyAccessResult::PropertyNotFound { .. } => {
                    // Check for optional chaining (?.) - suppress TS2339 error when using optional chaining
                    if access.question_dot_token {
                        // With optional chaining, missing property results in undefined
                        return TypeId::UNDEFINED;
                    }
                    // Don't emit TS2339 for private fields (starting with #) - they're handled elsewhere
                    if !property_name.starts_with('#') {
                        // TypeScript emits TS2339 for property access on any type, including functions
                        self.error_property_not_exist_at(
                            property_name,
                            object_type_for_access,
                            idx,
                        );
                    }
                    TypeId::ERROR
                }

                PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type,
                    cause,
                } => {
                    // Check for optional chaining (?.)
                    if access.question_dot_token {
                        // Suppress error, return (property_type | undefined)
                        let base_type = property_type.unwrap_or(TypeId::UNKNOWN);
                        return self.ctx.types.union(vec![base_type, TypeId::UNDEFINED]);
                    }

                    // Report error based on the cause (TS2531/TS2532/TS2533)
                    use crate::checker::types::diagnostics::diagnostic_codes;

                    let (code, message) = if cause == TypeId::NULL {
                        (
                            diagnostic_codes::OBJECT_IS_POSSIBLY_NULL,
                            "Object is possibly 'null'.",
                        )
                    } else if cause == TypeId::UNDEFINED {
                        (
                            diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED,
                            "Object is possibly 'undefined'.",
                        )
                    } else {
                        (
                            diagnostic_codes::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED,
                            "Object is possibly 'null' or 'undefined'.",
                        )
                    };

                    // Report the error on the expression part
                    self.error_at_node(access.expression, message, code);

                    // Error recovery: return the property type found in valid members
                    self.apply_flow_narrowing(idx, property_type.unwrap_or(TypeId::ERROR))
                }

                PropertyAccessResult::IsUnknown => {
                    // TS2571: Object is of type 'unknown'
                    // Unknown requires explicit type narrowing before property access
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        access.expression,
                        "Object is of type 'unknown'.",
                        diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN,
                    );
                    TypeId::ERROR
                }
            }
        } else {
            TypeId::ANY
        }
    }
}
