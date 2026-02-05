//! Type Computation (Complex Operations)
//!
//! Extracted from type_computation.rs: Second half of CheckerState impl
//! containing complex type computation methods for new expressions,
//! call expressions, constructability, union/keyof types, and identifiers.

use crate::binder::SymbolId;
use crate::checker::state::CheckerState;
use crate::parser::NodeIndex;
use crate::solver::types::Visibility;
use crate::solver::{ContextualTypeContext, TypeId};

/// Check if an AST node is contextually sensitive (requires contextual typing).
///
/// A node is contextually sensitive if its type cannot be fully determined
/// without an expected type from its parent. This includes:
/// - Arrow functions and function expressions
/// - Object literals (if ANY property is sensitive)
/// - Array literals (if ANY element is sensitive)
/// - Parenthesized expressions (pass through)
///
/// This is used for two-pass generic type inference, where contextually
/// sensitive arguments are deferred to Round 2 after non-contextual
/// arguments have been processed and type parameters have been partially inferred.
fn is_contextually_sensitive(state: &CheckerState, idx: NodeIndex) -> bool {
    use crate::parser::syntax_kind_ext;

    let Some(node) = state.ctx.arena.get(idx) else {
        return false;
    };

    match node.kind {
        // Functions are the primary sensitive nodes
        k if k == syntax_kind_ext::ARROW_FUNCTION || k == syntax_kind_ext::FUNCTION_EXPRESSION => {
            true
        }

        // Parentheses just pass through sensitivity
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
            if let Some(paren) = state.ctx.arena.get_parenthesized(node) {
                is_contextually_sensitive(state, paren.expression)
            } else {
                false
            }
        }

        // Conditional Expressions: Sensitive if either branch is sensitive
        k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
            if let Some(cond) = state.ctx.arena.get_conditional_expr(node) {
                is_contextually_sensitive(state, cond.when_true)
                    || is_contextually_sensitive(state, cond.when_false)
            } else {
                false
            }
        }

        // Object Literals: Sensitive if any property is sensitive
        k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
            if let Some(obj) = state.ctx.arena.get_literal_expr(node) {
                for &element_idx in &obj.elements.nodes {
                    if let Some(element) = state.ctx.arena.get(element_idx) {
                        match element.kind {
                            // Standard property: check initializer
                            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                                if let Some(prop) = state.ctx.arena.get_property_assignment(element)
                                {
                                    if is_contextually_sensitive(state, prop.initializer) {
                                        return true;
                                    }
                                }
                            }
                            // Shorthand property: { x } refers to a variable, never sensitive
                            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                                // Variable references are not contextually sensitive
                                // (their type is already known from their declaration)
                            }
                            // Spread: check the expression being spread
                            k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                                if let Some(spread) = state.ctx.arena.get_spread(element) {
                                    if is_contextually_sensitive(state, spread.expression) {
                                        return true;
                                    }
                                }
                            }
                            // Methods and Accessors are function-like (always sensitive)
                            k if k == syntax_kind_ext::METHOD_DECLARATION
                                || k == syntax_kind_ext::GET_ACCESSOR
                                || k == syntax_kind_ext::SET_ACCESSOR =>
                            {
                                return true;
                            }
                            _ => {}
                        }
                    }
                }
            }
            false
        }

        // Array Literals: Sensitive if any element is sensitive
        k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
            if let Some(arr) = state.ctx.arena.get_literal_expr(node) {
                for &element_idx in &arr.elements.nodes {
                    if is_contextually_sensitive(state, element_idx) {
                        return true;
                    }
                }
            }
            false
        }

        // Spread Elements (in arrays)
        k if k == syntax_kind_ext::SPREAD_ELEMENT => {
            if let Some(spread) = state.ctx.arena.get_spread(node) {
                is_contextually_sensitive(state, spread.expression)
            } else {
                false
            }
        }

        _ => false,
    }
}

impl<'a> CheckerState<'a> {
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
        use crate::solver::{CallEvaluator, CallResult, CallableShape, CompatChecker};

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
                    let is_type_alias = (symbol.flags & symbol_flags::TYPE_ALIAS) != 0;

                    // Emit TS2693 for type-only symbols used as values
                    // This includes:
                    // 1. Symbols with TYPE flag but no VALUE flag (interfaces without namespace merge, type-only imports)
                    // 2. Type aliases (never have VALUE, even if they reference a class)
                    //
                    // IMPORTANT: Don't emit for interfaces that have VALUE (merged with namespace)
                    if is_type_alias || (has_type && !has_value) {
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
        // TS18050: Cannot construct 'never' type (impossible union after narrowing)
        if constructor_type == TypeId::NEVER {
            use crate::checker::types::diagnostics::{
                diagnostic_codes, diagnostic_messages, format_message,
            };
            let message =
                format_message(diagnostic_messages::VALUE_CANNOT_BE_USED_HERE, &["never"]);
            self.error_at_node(
                new_expr.expression,
                &message,
                diagnostic_codes::VALUE_CANNOT_BE_USED_HERE,
            );
            return TypeId::NEVER;
        }

        // Evaluate application types (e.g., Newable<T>, Constructor<{}>) to get the actual Callable
        let constructor_type = self.evaluate_application_type(constructor_type);

        // Resolve Ref types to ensure we get the actual constructor type, not just a symbolic reference
        // This is critical for classes where we need the Callable with construct signatures
        let constructor_type = self.resolve_ref_type(constructor_type);

        let construct_type = match crate::solver::type_queries::classify_for_new_expression(
            self.ctx.types,
            constructor_type,
        ) {
            crate::solver::type_queries::NewExpressionTypeKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                if shape.construct_signatures.is_empty() {
                    // Functions with a prototype property are constructable
                    // This handles cases like `function Foo() {}` where `Foo.prototype` exists
                    if self.type_has_prototype_property(constructor_type) {
                        Some(constructor_type)
                    } else if !shape.call_signatures.is_empty() {
                        // TSC behavior: callable types with call signatures but no construct
                        // signatures are still "constructable" — `new` returns `any`.
                        // TS2351 is only for types with NO signatures at all (e.g., string).
                        // In strict mode, TSC emits TS7009 instead.
                        Some(TypeId::ANY)
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
                        symbol: None,
                    }))
                }
            }
            crate::solver::type_queries::NewExpressionTypeKind::Function(_) => {
                Some(constructor_type)
            }
            crate::solver::type_queries::NewExpressionTypeKind::TypeQuery(sym_ref) => {
                // Ref to a symbol or TypeQuery (typeof X) - resolve to the symbol's type
                use crate::binder::SymbolId;
                let symbol_id = SymbolId(sym_ref.0);
                if self.ctx.binder.get_symbol(symbol_id).is_some() {
                    // Get the symbol's actual type which should be a Callable with construct signatures
                    let symbol_type = self.get_type_of_symbol(symbol_id);
                    // Check if the symbol's type is constructable
                    self.get_construct_type_from_type(symbol_type)
                } else {
                    None
                }
            }
            crate::solver::type_queries::NewExpressionTypeKind::Intersection(members) => {
                // For intersection of constructors (mixin pattern), the result is an
                // intersection of all instance types. Handle this specially.
                let mut instance_types: Vec<TypeId> = Vec::new();

                for member in members {
                    // Evaluate Application types (e.g., Constructor<T>) to get their Callable shape
                    let evaluated_member = self.evaluate_application_type(member);
                    // Try to get construct signatures from the evaluated member
                    let construct_sig_return =
                        self.get_construct_signature_return_type(evaluated_member);
                    if let Some(return_type) = construct_sig_return {
                        instance_types.push(return_type);
                    }
                }

                if instance_types.is_empty() {
                    // TS2351: This expression is not constructable
                    self.error_not_constructable_at(constructor_type, idx);
                    return TypeId::ERROR;
                } else if instance_types.len() == 1 {
                    return instance_types[0];
                } else {
                    // Return intersection of all instance types
                    return self.ctx.types.intersection(instance_types);
                }
            }
            crate::solver::type_queries::NewExpressionTypeKind::TypeParameter { constraint } => {
                // For type parameters with constructor constraints (e.g., T extends typeof Base),
                // check the constraint for constructor signatures.
                // This handles patterns like:
                //   function f<T extends typeof Base>(ctor: T) {
                //       return new ctor();  // Should work - T has construct signatures from Base
                //   }
                if let Some(constraint) = constraint {
                    // Evaluate the constraint to resolve Application types like Constructor<T>
                    let evaluated_constraint = self.evaluate_application_type(constraint);

                    // Check if the evaluated constraint is an Intersection - handle it specially
                    if let Some(members) = crate::solver::type_queries::get_intersection_members(
                        self.ctx.types,
                        evaluated_constraint,
                    ) {
                        let mut instance_types: Vec<TypeId> = Vec::new();

                        for member in members {
                            // Resolve Refs (type alias references) to their actual types
                            let resolved_member = self.resolve_type_for_property_access(member);
                            // Then evaluate any Application types
                            let evaluated_member = self.evaluate_application_type(resolved_member);
                            let construct_sig_return =
                                self.get_construct_signature_return_type(evaluated_member);
                            if let Some(return_type) = construct_sig_return {
                                instance_types.push(return_type);
                            }
                        }

                        if instance_types.is_empty() {
                            self.error_not_constructable_at(constructor_type, idx);
                            return TypeId::ERROR;
                        } else if instance_types.len() == 1 {
                            return instance_types[0];
                        } else {
                            return self.ctx.types.intersection(instance_types);
                        }
                    }

                    // For non-intersection constraints, get the construct type
                    self.get_construct_type_from_type(evaluated_constraint)
                } else {
                    // No constraint - can't determine if it's a constructor
                    None
                }
            }
            crate::solver::type_queries::NewExpressionTypeKind::Union(members) => {
                // For union types, check if all members are constructors
                // and return the union of their instance types
                let mut instance_types: Vec<TypeId> = Vec::new();
                let mut all_constructable = true;

                for member in members {
                    // Resolve Refs (type alias references) to their actual types
                    let resolved_member = self.resolve_type_for_property_access(member);
                    // Then evaluate any Application types
                    let evaluated_member = self.evaluate_application_type(resolved_member);
                    let construct_sig_return =
                        self.get_construct_signature_return_type(evaluated_member);
                    if let Some(return_type) = construct_sig_return {
                        instance_types.push(return_type);
                    } else {
                        all_constructable = false;
                        break;
                    }
                }

                if all_constructable && !instance_types.is_empty() {
                    Some(self.ctx.types.union(instance_types))
                } else {
                    None
                }
            }
            crate::solver::type_queries::NewExpressionTypeKind::NotConstructable => None,
        };

        let Some(construct_type) = construct_type else {
            // TS2351: This expression is not constructable
            self.error_not_constructable_at(constructor_type, idx);
            return TypeId::ERROR;
        };

        let args = new_expr
            .arguments
            .as_ref()
            .map(|a| &a.nodes)
            .map(|n| n.as_slice())
            .unwrap_or(&[]);

        let overload_signatures = match crate::solver::type_queries::classify_for_call_signatures(
            self.ctx.types,
            construct_type,
        ) {
            crate::solver::type_queries::CallSignaturesKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                if shape.call_signatures.len() > 1 {
                    Some(shape.call_signatures.clone())
                } else {
                    None
                }
            }
            crate::solver::type_queries::CallSignaturesKind::NoSignatures => None,
        };

        if let Some(signatures) = overload_signatures.as_deref()
            && let Some(return_type) =
                self.resolve_overloaded_call_with_signatures(args, signatures, false)
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
            self.ctx.configure_compat_checker(&mut checker);
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
                if actual < expected_min && expected_max != Some(expected_min) {
                    // Too few arguments with rest/optional parameters - use TS2555
                    // expected_max is None (rest params) or Some(max) where max > min (optional params)
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
    pub(crate) fn type_contains_abstract_class(&self, type_id: TypeId) -> bool {
        use crate::binder::SymbolId;
        use crate::binder::symbol_flags;
        use crate::solver::type_queries::{AbstractClassCheckKind, classify_for_abstract_check};

        match classify_for_abstract_check(self.ctx.types, type_id) {
            // TypeQuery is `typeof ClassName` - check if the symbol is abstract
            // Since get_type_from_type_query now uses real SymbolIds, we can directly look up
            AbstractClassCheckKind::TypeQuery(sym_ref) => {
                if let Some(symbol) = self.ctx.binder.get_symbol(SymbolId(sym_ref.0))
                    && symbol.flags & symbol_flags::ABSTRACT != 0
                {
                    return true;
                }
                false
            }
            // Union type - check if ANY constituent is abstract
            AbstractClassCheckKind::Union(members) => members
                .iter()
                .any(|&member| self.type_contains_abstract_class(member)),
            // Intersection type - check if ANY constituent is abstract
            AbstractClassCheckKind::Intersection(members) => members
                .iter()
                .any(|&member| self.type_contains_abstract_class(member)),
            AbstractClassCheckKind::NotAbstract => false,
        }
    }

    /// Get the construct type from a TypeId, used for new expressions.
    ///
    /// This is similar to get_construct_signature_return_type but returns
    /// the full construct type (not just the return type) for new expressions.
    ///
    /// The emit_error parameter controls whether we emit TS2507 errors.
    /// Resolve Ref types to their actual types.
    ///
    /// For symbol references (Ref), this resolves them to the symbol's declared type.
    /// This is important for new expressions where we need the actual constructor type
    /// with construct signatures, not just a symbolic reference.
    pub(crate) fn resolve_ref_type(&mut self, type_id: TypeId) -> TypeId {
        use crate::solver::type_queries::{LazyTypeKind, classify_for_lazy_resolution};

        match classify_for_lazy_resolution(self.ctx.types, type_id) {
            LazyTypeKind::Lazy(def_id) => {
                // New DefId-based case - resolve via DefId
                if let Some(symbol_id) = self.ctx.def_to_symbol_id(def_id) {
                    let symbol_type = self.get_type_of_symbol(symbol_id);
                    if symbol_type == type_id {
                        type_id
                    } else {
                        symbol_type
                    }
                } else {
                    type_id
                }
            }
            LazyTypeKind::NotLazy => type_id,
            _ => type_id, // Handle deprecated variants for compatibility
        }
    }

    pub(crate) fn get_construct_type_from_type(&self, type_id: TypeId) -> Option<TypeId> {
        use crate::solver::type_queries::{
            ConstructableTypeKind, classify_for_constructability, construct_to_call_callable,
        };

        // Recursion depth check to prevent stack overflow from recursive
        // constructability checks (e.g. D<D<T>> patterns, symbol→type→symbol cycles)
        if !self.ctx.enter_recursion() {
            return None;
        }

        let result = match classify_for_constructability(self.ctx.types, type_id) {
            ConstructableTypeKind::CallableWithConstruct => {
                // Return a callable with construct signatures as call signatures
                construct_to_call_callable(self.ctx.types, type_id)
            }
            ConstructableTypeKind::CallableMaybePrototype => {
                // Functions with a prototype property are constructable
                if self.type_has_prototype_property(type_id) {
                    Some(type_id)
                } else {
                    None
                }
            }
            ConstructableTypeKind::Function => Some(type_id),
            ConstructableTypeKind::SymbolRef(sym_ref) => {
                self.check_symbol_constructability(type_id, SymbolId(sym_ref.0), false)
            }
            ConstructableTypeKind::TypeQueryRef(sym_ref) => {
                self.check_symbol_constructability(type_id, SymbolId(sym_ref.0), true)
            }
            ConstructableTypeKind::TypeParameterWithConstraint(constraint) => {
                self.get_construct_type_from_type(constraint)
            }
            ConstructableTypeKind::TypeParameterNoConstraint => None,
            ConstructableTypeKind::Intersection(members) => {
                // All members must be constructable
                if members
                    .iter()
                    .all(|&member| self.get_construct_type_from_type(member).is_some())
                {
                    Some(type_id)
                } else {
                    None
                }
            }
            ConstructableTypeKind::Application => Some(type_id),
            ConstructableTypeKind::Object => Some(type_id),
            ConstructableTypeKind::NotConstructable => None,
        };

        self.ctx.leave_recursion();
        result
    }

    /// Check if a symbol reference is constructable.
    ///
    /// This handles both Ref and TypeQuery cases which have similar logic
    /// for checking symbol flags (class, interface) and cached types.
    pub(crate) fn check_symbol_constructability(
        &self,
        type_id: TypeId,
        symbol_id: SymbolId,
        is_type_query: bool,
    ) -> Option<TypeId> {
        use crate::solver::type_queries;

        let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) else {
            return None;
        };

        // Class symbols are constructable - return the type as-is
        if (symbol.flags & crate::binder::symbol_flags::CLASS) != 0 {
            return Some(type_id);
        }

        // Interface symbols might have construct signatures
        if (symbol.flags & crate::binder::symbol_flags::INTERFACE) != 0 {
            if let Some(&cached_type) = self.ctx.symbol_types.get(&symbol_id) {
                // Check if the cached type has construct signatures
                if crate::solver::type_queries::has_construct_signatures(
                    self.ctx.types,
                    cached_type,
                ) {
                    return Some(type_id);
                }
                // For Ref (not TypeQuery), also check if it's an object type
                if !is_type_query && type_queries::is_object_type(self.ctx.types, cached_type) {
                    return Some(type_id);
                }
            }
            // Return the type for further checking by the caller
            return Some(type_id);
        }

        // For other symbols (variables, parameters, type aliases), check their cached type
        if let Some(&cached_type) = self.ctx.symbol_types.get(&symbol_id) {
            if self.get_construct_type_from_type(cached_type).is_some() {
                return Some(type_id);
            }
        }

        None
    }

    /// Get the return type of a construct signature from a type.
    ///
    /// This handles various type representations:
    /// - Direct Callable with construct signatures
    /// - Ref to class symbols (typeof Class)
    /// - TypeQuery (typeof expressions)
    ///
    /// Returns None if the type doesn't have construct signatures.
    pub(crate) fn get_construct_signature_return_type(&self, type_id: TypeId) -> Option<TypeId> {
        use crate::binder::SymbolId;
        use crate::solver::SymbolRef;
        use crate::solver::type_queries::{
            ConstructSignatureKind, classify_for_construct_signature,
        };
        use crate::solver::types::TypeKey;

        match classify_for_construct_signature(self.ctx.types, type_id) {
            ConstructSignatureKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                shape
                    .construct_signatures
                    .first()
                    .map(|sig| sig.return_type)
            }
            ConstructSignatureKind::Lazy(def_id) => {
                // Phase 4.2: Lazy type - resolve DefId to SymbolId
                // This handles cases like `typeof M1` where M1 is a class
                if let Some(symbol_id) = self.ctx.def_to_symbol_id(def_id) {
                    if let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) {
                        // Check if this is a class symbol
                        if (symbol.flags & crate::binder::symbol_flags::CLASS) != 0 {
                            // For class symbols, the instance type is what we want
                            // The construct signature returns the instance type
                            // We create a Lazy type to this symbol as the instance type
                            return Some(self.ctx.types.intern(TypeKey::Lazy(def_id)));
                        }
                        // Check interfaces and other symbols for cached types with construct signatures
                        if let Some(&cached_type) = self.ctx.symbol_types.get(&symbol_id) {
                            // Recursively check the cached type
                            // Avoid infinite loops by checking if it's the same as the input
                            if cached_type != type_id {
                                return self.get_construct_signature_return_type(cached_type);
                            }
                        }
                    }
                }
                None
            }
            #[allow(deprecated)]
            ConstructSignatureKind::Ref(sym_ref) => {
                // Ref to a symbol - get the symbol's type which should be a Callable
                // This handles cases like `typeof M1` where M1 is a class
                let symbol_id = SymbolId(sym_ref.0);
                if let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) {
                    // Check if this is a class symbol
                    if (symbol.flags & crate::binder::symbol_flags::CLASS) != 0 {
                        // For class symbols, the instance type is what we want
                        // The construct signature returns the instance type
                        // We create a Ref to this symbol as the instance type
                        return Some(self.ctx.types.reference(SymbolRef(sym_ref.0)));
                    }
                    // Check interfaces and other symbols for cached types with construct signatures
                    if let Some(&cached_type) = self.ctx.symbol_types.get(&symbol_id) {
                        // Recursively check the cached type
                        // Avoid infinite loops by checking if it's the same as the input
                        if cached_type != type_id {
                            return self.get_construct_signature_return_type(cached_type);
                        }
                    }
                }
                None
            }
            ConstructSignatureKind::TypeQuery(sym_ref) => {
                // TypeQuery is `typeof ClassName` - the return type is an instance of the class
                let symbol_id = SymbolId(sym_ref.0);
                if let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) {
                    if (symbol.flags & crate::binder::symbol_flags::CLASS) != 0 {
                        // Return a Ref to the class as the instance type
                        return Some(self.ctx.types.reference(SymbolRef(sym_ref.0)));
                    }
                    // Check other symbols for cached types with construct signatures
                    if let Some(&cached_type) = self.ctx.symbol_types.get(&symbol_id) {
                        // Recursively check the cached type
                        // Avoid infinite loops by checking if it's the same as the input
                        if cached_type != type_id {
                            return self.get_construct_signature_return_type(cached_type);
                        }
                    }
                }
                None
            }
            // Handle Application types (e.g., Constructor<T>)
            // Evaluate the application and check the result for construct signatures
            ConstructSignatureKind::Application(app_id) => {
                // We need to evaluate the application type to get its resolved form
                // Since evaluate_application_type is on CheckerState (mutable), we
                // check if the base type is a type alias that resolves to a Callable
                let app = self.ctx.types.type_application(app_id);
                // Check if base is a Ref to a type alias with a Callable body
                if let Some(sym_ref) =
                    crate::solver::type_queries::get_ref_if_symbol(self.ctx.types, app.base)
                {
                    let symbol_id = SymbolId(sym_ref.0);
                    if let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) {
                        // For type aliases, get the cached resolved type
                        if let Some(&cached_type) = self.ctx.symbol_types.get(&symbol_id) {
                            // Recursively check the resolved type
                            return self.get_construct_signature_return_type(cached_type);
                        }
                        // Check if symbol is a class
                        if (symbol.flags & crate::binder::symbol_flags::CLASS) != 0 {
                            return Some(self.ctx.types.reference(SymbolRef(sym_ref.0)));
                        }
                    }
                }
                // Check if base directly has construct signatures
                self.get_construct_signature_return_type(app.base)
            }
            // Handle Union types - ALL members must be constructable
            ConstructSignatureKind::Union(members) => {
                let mut instance_types: Vec<TypeId> = Vec::new();

                for member in members {
                    if let Some(return_type) = self.get_construct_signature_return_type(member) {
                        instance_types.push(return_type);
                    } else {
                        // If any member is not constructable, the whole union is not
                        return None;
                    }
                }

                if instance_types.is_empty() {
                    None
                } else {
                    // Return union of all instance types
                    Some(self.ctx.types.union(instance_types))
                }
            }
            // Handle Intersection types - ANY member being constructable is sufficient
            ConstructSignatureKind::Intersection(members) => {
                for member in members {
                    if let Some(return_type) = self.get_construct_signature_return_type(member) {
                        return Some(return_type);
                    }
                }
                None
            }
            // Handle TypeParameter - check constraint for construct signatures
            ConstructSignatureKind::TypeParameter { constraint } => {
                if let Some(constraint) = constraint {
                    self.get_construct_signature_return_type(constraint)
                } else {
                    None
                }
            }
            // Handle Function types - check if it's actually a Callable
            // (Function types in TypeScript can have construct signatures via
            // overloading, but TypeKey::Function is for simple functions)
            ConstructSignatureKind::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                // If it's marked as a constructor, use its return type
                if shape.is_constructor {
                    Some(shape.return_type)
                } else {
                    None
                }
            }
            ConstructSignatureKind::NoConstruct => None,
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
                return self.ctx.types.readonly_type(inner_type);
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
        use crate::solver::type_queries::{KeyOfTypeKind, classify_for_keyof};
        match classify_for_keyof(self.ctx.types, operand) {
            KeyOfTypeKind::Object(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                if shape.properties.is_empty() {
                    return TypeId::NEVER;
                }
                let key_types: Vec<TypeId> = shape
                    .properties
                    .iter()
                    .map(|p| self.ctx.types.literal_string_atom(p.name))
                    .collect();
                self.ctx.types.union(key_types)
            }
            KeyOfTypeKind::NoKeys => TypeId::NEVER,
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
        use crate::solver::type_queries::{
            StringLiteralKeyKind, classify_for_string_literal_keys, get_string_literal_value,
        };

        match classify_for_string_literal_keys(self.ctx.types, type_id) {
            StringLiteralKeyKind::SingleString(name) => vec![name],
            StringLiteralKeyKind::Union(members) => members
                .iter()
                .filter_map(|&member| get_string_literal_value(self.ctx.types, member))
                .collect(),
            StringLiteralKeyKind::NotStringLiteral => Vec::new(),
        }
    }

    /// Get the Symbol constructor type.
    ///
    /// Creates the type for the global `Symbol` constructor, including:
    /// - Call signature: `Symbol(description?: string | number): symbol`
    /// - Well-known symbol properties (iterator, asyncIterator, etc.)
    #[allow(dead_code)]
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
            is_method: false,
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
                visibility: Visibility::Public,
                parent_id: None,
            });
        }

        self.ctx.types.callable(CallableShape {
            call_signatures: vec![call_signature],
            construct_signatures: Vec::new(),
            properties,
            string_index: None,
            number_index: None,
            symbol: None,
        })
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
        // Fast path: check the direct instance-type-to-class-declaration map first.
        // This correctly handles derived classes that have no brand properties.
        if let Some(&class_idx) = self.ctx.class_instance_type_to_decl.get(&type_id) {
            return Some(class_idx);
        }

        use crate::binder::SymbolId;
        use crate::solver::type_queries::{ClassDeclTypeKind, classify_for_class_decl};

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
            match classify_for_class_decl(checker.ctx.types, type_id) {
                ClassDeclTypeKind::Object(shape_id) => {
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
                ClassDeclTypeKind::Members(members) => {
                    for member in members {
                        collect_candidates(checker, member, out);
                    }
                }
                ClassDeclTypeKind::NotObject => {}
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
    pub(crate) fn get_type_of_call_expression_inner(&mut self, idx: NodeIndex) -> TypeId {
        use crate::parser::node_flags;
        use crate::parser::syntax_kind_ext;
        use crate::solver::{CallEvaluator, CallResult, CompatChecker, instantiate_type};

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
            // This creates Promise<ModuleNamespace> where ModuleNamespace contains all exports
            return self.get_dynamic_import_type(call);
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
        // TS18050: Cannot call 'never' type (impossible union after narrowing)
        if callee_type == TypeId::NEVER {
            use crate::checker::types::diagnostics::{
                diagnostic_codes, diagnostic_messages, format_message,
            };
            let message =
                format_message(diagnostic_messages::VALUE_CANNOT_BE_USED_HERE, &["never"]);
            self.error_at_node(
                call.expression,
                &message,
                diagnostic_codes::VALUE_CANNOT_BE_USED_HERE,
            );
            return TypeId::NEVER;
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

        let overload_signatures = match crate::solver::type_queries::classify_for_call_signatures(
            self.ctx.types,
            callee_type_for_resolution,
        ) {
            crate::solver::type_queries::CallSignaturesKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                if shape.call_signatures.len() > 1 {
                    Some(shape.call_signatures.clone())
                } else {
                    None
                }
            }
            crate::solver::type_queries::CallSignaturesKind::NoSignatures => None,
        };

        // Overload candidates need signature-specific contextual typing.
        let force_bivariant_callbacks = matches!(
            self.ctx.arena.get(call.expression).map(|n| n.kind),
            Some(syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION)
                | Some(syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
        );

        if let Some(signatures) = overload_signatures.as_deref()
            && let Some(return_type) = self.resolve_overloaded_call_with_signatures(
                args,
                signatures,
                force_bivariant_callbacks,
            )
        {
            let return_type =
                self.apply_this_substitution_to_call_return(return_type, call.expression);
            return if nullish_cause.is_some() {
                self.ctx.types.union(vec![return_type, TypeId::UNDEFINED])
            } else {
                return_type
            };
        }

        // Resolve Ref types to get the actual callable for FunctionShape extraction
        // This is needed before we can check if the callee is generic
        let callee_type_for_shape = self.resolve_ref_type(callee_type_for_resolution);

        // Extract function shape to check if this is a generic call that needs two-pass inference
        let callee_shape = CallEvaluator::<CompatChecker>::get_contextual_signature(
            self.ctx.types,
            callee_type_for_shape,
        );
        let is_generic_call = callee_shape
            .as_ref()
            .is_some_and(|s| !s.type_params.is_empty())
            && call.type_arguments.is_none(); // Only use two-pass if no explicit type args

        // Create contextual context from callee type with type arguments applied
        let ctx_helper =
            ContextualTypeContext::with_expected(self.ctx.types, callee_type_for_resolution);
        let check_excess_properties = overload_signatures.is_none();

        // Two-pass argument collection for generic calls without explicit type arguments
        let arg_types = if is_generic_call && let Some(shape) = callee_shape {
            // Pre-compute which arguments are contextually sensitive to avoid borrowing self in closures
            let sensitive_args: Vec<bool> = args
                .iter()
                .map(|&arg| is_contextually_sensitive(self, arg))
                .collect();

            // === Round 1: Collect non-contextual argument types ===
            // This allows type parameters to be inferred from concrete arguments
            let round1_arg_types = self.collect_call_argument_types_with_context(
                args,
                |i, arg_count| {
                    // Skip contextually sensitive arguments in Round 1
                    if sensitive_args[i] {
                        None
                    } else {
                        ctx_helper.get_parameter_type_for_call(i, arg_count)
                    }
                },
                check_excess_properties,
            );

            // === Perform Round 1 Inference ===
            // Use the Solver to infer type parameters from non-contextual arguments only
            let substitution = {
                let env = self.ctx.type_env.borrow();
                let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
                self.ctx.configure_compat_checker(&mut checker);
                let mut evaluator = CallEvaluator::new(self.ctx.types, &mut checker);

                // Set contextual type for downward inference (e.g., `let x: string = id(...)`)
                if let Some(ctx_type) = self.ctx.contextual_type {
                    evaluator.set_contextual_type(Some(ctx_type));
                }

                // Run Round 1 inference and get substitution with fixed type variables
                evaluator.compute_contextual_types(&shape, &round1_arg_types)
            };

            // === Round 2: Collect ALL argument types with contextual typing ===
            // Now that type parameters are partially inferred, lambdas get proper contextual types
            self.collect_call_argument_types_with_context(
                args,
                |i, arg_count| {
                    let param_type = ctx_helper.get_parameter_type_for_call(i, arg_count)?;
                    // Instantiate parameter type with Round 1 substitution
                    // This gives lambdas their contextual types (e.g., `(x: number) => U`)
                    Some(instantiate_type(self.ctx.types, param_type, &substitution))
                },
                check_excess_properties,
            )
        } else {
            // === Single-pass: Standard argument collection ===
            // Non-generic calls or calls with explicit type arguments use the standard flow
            self.collect_call_argument_types_with_context(
                args,
                |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
                check_excess_properties,
            )
        };

        // Use CallEvaluator to resolve the call
        self.ensure_application_symbols_resolved(callee_type_for_resolution);
        for &arg_type in &arg_types {
            self.ensure_application_symbols_resolved(arg_type);
        }

        // Evaluate application types to resolve Ref bases to actual Callable types
        // This is needed for cases like `GenericCallable<string>` where the type is
        // stored as Application(Ref(symbol_id), [string]) and needs to be resolved
        // to the actual Callable with call signatures
        let callee_type_for_call = self.evaluate_application_type(callee_type_for_resolution);
        // Resolve bare Ref types to their underlying callable types.
        // This handles interfaces with call signatures, merged declarations, etc.
        let callee_type_for_call = self.resolve_ref_type(callee_type_for_call);

        // Ensure all Ref types in callee/args are resolved into type_env for assignability.
        self.ensure_refs_resolved(callee_type_for_call);
        for &arg_type in &arg_types {
            self.ensure_refs_resolved(arg_type);
        }

        let result = {
            let env = self.ctx.type_env.borrow();
            let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
            self.ctx.configure_compat_checker(&mut checker);
            let mut evaluator = CallEvaluator::new(self.ctx.types, &mut checker);
            evaluator.set_force_bivariant_callbacks(force_bivariant_callbacks);
            evaluator.resolve_call(callee_type_for_call, &arg_types)
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
                if self.is_constructor_type(callee_type) {
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
                if actual < expected_min && expected_max != Some(expected_min) {
                    // Too few arguments with rest/optional parameters - use TS2555
                    // expected_max is None (rest params) or Some(max) where max > min (optional params)
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

        // === CRITICAL FIX: Check type parameter scope FIRST ===
        // Type parameters in generic functions/classes/type aliases should be resolved
        // before checking any other scope. This is a common source of TS2304 false positives.
        // Examples:
        //   function foo<T>(x: T) { return x; }  // T should be found in the function body
        //   class C<U> { method(u: U) {} }  // U should be found in the class body
        //   type Pair<T> = [T, T];  // T should be found in the type alias definition
        if let Some(type_id) = self.lookup_type_parameter(name) {
            return type_id;
        }

        // Resolve via binder persistent scopes for stateless lookup.
        if let Some(sym_id) = self.resolve_identifier_symbol(idx) {
            // Reference tracking is handled by resolve_identifier_symbol wrapper

            if self.alias_resolves_to_type_only(sym_id) {
                self.error_type_only_value_at(name, idx);
                return TypeId::ERROR;
            }
            // Check symbol flags to detect type-only usage.
            // First try the main binder (fast path for local symbols).
            let local_symbol = self.ctx.binder.get_symbol(sym_id);
            let flags = local_symbol.map(|s| s.flags).unwrap_or(0);
            let has_type = (flags & crate::binder::symbol_flags::TYPE) != 0;
            let has_value = (flags & crate::binder::symbol_flags::VALUE) != 0;
            let is_type_alias = (flags & crate::binder::symbol_flags::TYPE_ALIAS) != 0;

            // Check for type-only symbols used as values
            // This includes:
            // 1. Symbols with TYPE flag but no VALUE flag (interfaces, type-only imports, etc.)
            // 2. Type aliases (never have VALUE, even if they reference a class)
            //
            // IMPORTANT: Only check is_interface if it has no VALUE flag.
            // Interfaces merged with namespaces DO have VALUE and should NOT error.
            //
            // CROSS-LIB MERGING: The same name may have TYPE in one lib file
            // (e.g., `interface Promise<T>` in es5.d.ts) and VALUE in another
            // (e.g., `declare var Promise` in es2015.promise.d.ts). When we find
            // a TYPE-only symbol, check if a VALUE exists elsewhere in libs.
            if is_type_alias || (has_type && !has_value) {
                // Cross-lib merging: check if a VALUE symbol for this name
                // exists in a different lib binder before emitting error.
                if let Some(val_sym_id) = self.find_value_symbol_in_libs(name) {
                    return self.get_type_of_symbol(val_sym_id);
                }

                // Don't emit TS2693 in heritage clause context — the heritage
                // checker will emit the appropriate error (e.g., TS2689 for
                // class extends interface).
                if self.find_enclosing_heritage_clause(idx).is_some() {
                    return TypeId::ERROR;
                }

                // Don't emit TS2693 for export default/export = expressions.
                // `export default InterfaceName` and `export = InterfaceName`
                // are valid TypeScript — they export the type binding.
                if let Some(parent_ext) = self.ctx.arena.get_extended(idx) {
                    if !parent_ext.parent.is_none() {
                        if let Some(parent_node) = self.ctx.arena.get(parent_ext.parent) {
                            use crate::parser::syntax_kind_ext;
                            if parent_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                                || parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                            {
                                return TypeId::ERROR;
                            }
                        }
                    }
                }

                self.error_type_only_value_at(name, idx);
                return TypeId::ERROR;
            }

            // If the symbol wasn't found in the main binder (flags==0), it came
            // from a lib or cross-file binder.  For known ES2015+ global type
            // names (Symbol, Promise, Map, Set, etc.) we need to check whether
            // the lib binder's symbol is type-only.  Only do this for the known
            // set to avoid cross-binder ID collisions causing false TS2693 on
            // arbitrary user symbols from other files.
            if flags == 0 {
                use crate::lib_loader;
                if lib_loader::is_es2015_plus_type(name) {
                    let lib_binders = self.get_lib_binders();
                    let lib_flags = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(sym_id, &lib_binders)
                        .map(|s| s.flags)
                        .unwrap_or(0);
                    let lib_has_type = (lib_flags & crate::binder::symbol_flags::TYPE) != 0;
                    let lib_has_value = (lib_flags & crate::binder::symbol_flags::VALUE) != 0;
                    if lib_has_type && !lib_has_value {
                        // Cross-lib merging: VALUE may be in a different lib binder
                        if let Some(val_sym_id) = self.find_value_symbol_in_libs(name) {
                            return self.get_type_of_symbol(val_sym_id);
                        }
                        self.error_type_only_value_at(name, idx);
                        return TypeId::ERROR;
                    }
                }
            }
            let declared_type = self.get_type_of_symbol(sym_id);
            // Check for TDZ violations (variable used before declaration in source order)
            // 1. Static block TDZ - variable used in static block before its declaration
            // 2. Computed property TDZ - variable used in computed property name before its declaration
            // 3. Heritage clause TDZ - variable used in extends/implements before its declaration
            // Return TypeId::ERROR after emitting TDZ error to prevent cascading errors
            if self.is_variable_used_before_declaration_in_static_block(sym_id, idx) {
                // TS2448: Block-scoped variable used before declaration (TDZ error)
                use crate::checker::types::diagnostics::{
                    diagnostic_codes, diagnostic_messages, format_message,
                };
                let message = format_message(
                    diagnostic_messages::BLOCK_SCOPED_VARIABLE_USED_BEFORE_DECLARATION,
                    &[name],
                );
                self.error_at_node(
                    idx,
                    &message,
                    diagnostic_codes::BLOCK_SCOPED_VARIABLE_USED_BEFORE_DECLARATION,
                );
                return TypeId::ERROR;
            } else if self.is_variable_used_before_declaration_in_computed_property(sym_id, idx) {
                // TS2448: Block-scoped variable used before declaration (TDZ error)
                use crate::checker::types::diagnostics::{
                    diagnostic_codes, diagnostic_messages, format_message,
                };
                let message = format_message(
                    diagnostic_messages::BLOCK_SCOPED_VARIABLE_USED_BEFORE_DECLARATION,
                    &[name],
                );
                self.error_at_node(
                    idx,
                    &message,
                    diagnostic_codes::BLOCK_SCOPED_VARIABLE_USED_BEFORE_DECLARATION,
                );
                return TypeId::ERROR;
            } else if self.is_variable_used_before_declaration_in_heritage_clause(sym_id, idx) {
                // TS2448: Block-scoped variable used before declaration (TDZ error)
                use crate::checker::types::diagnostics::{
                    diagnostic_codes, diagnostic_messages, format_message,
                };
                let message = format_message(
                    diagnostic_messages::BLOCK_SCOPED_VARIABLE_USED_BEFORE_DECLARATION,
                    &[name],
                );
                self.error_at_node(
                    idx,
                    &message,
                    diagnostic_codes::BLOCK_SCOPED_VARIABLE_USED_BEFORE_DECLARATION,
                );
                return TypeId::ERROR;
            }
            // Use check_flow_usage to integrate both DAA and type narrowing
            // This handles TS2454 errors and applies flow-based narrowing
            return self.check_flow_usage(idx, declared_type, sym_id);
        }

        // Intrinsic names - use constant TypeIds
        match name.as_str() {
            "undefined" => TypeId::UNDEFINED,
            "NaN" | "Infinity" => TypeId::NUMBER,
            // Symbol constructor - only synthesize if available in lib contexts or merged into binder
            "Symbol" => {
                // Check if Symbol is available as a VALUE from lib contexts or merged lib symbols
                // This is critical for ES5 mode where Symbol exists as TYPE but not VALUE
                // In ES5: interface Symbol exists (TYPE) but declare var Symbol doesn't (no VALUE)
                // In ES2015+: both interface Symbol (TYPE) and declare var Symbol (VALUE) exist

                // First check if Symbol exists at all in libs
                let symbol_exists = self.ctx.has_symbol_in_lib();

                if !symbol_exists {
                    // Symbol is not available via lib at all — emit TS2583
                    self.error_cannot_find_name_change_lib(name, idx);
                    return TypeId::ERROR;
                }

                // Symbol exists in lib - check if it has VALUE flag (is usable as a value)
                // This distinguishes ES5 (type-only) from ES2015+ (type and value)
                if let Some(val_sym_id) = self.find_value_symbol_in_libs(name) {
                    // Symbol has VALUE flag - it's available as a constructor
                    return self.get_type_of_symbol(val_sym_id);
                }

                // Symbol exists but only as TYPE (ES5 case) - emit TS2585
                // "'Symbol' only refers to a type, but is being used as a value here.
                // Do you need to change your target library?"
                self.error_type_only_value_at(name, idx);
                return TypeId::ERROR;
            }
            _ if self.is_known_global_value_name(name) => {
                // Node.js runtime globals are always available (injected by runtime)
                // We return ANY without emitting an error for these
                if self.is_nodejs_runtime_global(name) {
                    return TypeId::ANY;
                }

                // Global is available in lib - try to resolve it and get its type
                // This eliminates "Any poisoning" by actually resolving the symbol
                // instead of defaulting to Any type which suppresses real type errors.
                let lib_binders = self.get_lib_binders();

                // First, try to get the symbol from file_locals (contains merged lib symbols)
                if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
                    return self.get_type_of_symbol(sym_id);
                }

                // Then try lib binders directly (for lib_contexts path)
                if let Some(sym_id) = self
                    .ctx
                    .binder
                    .get_global_type_with_libs(name, &lib_binders)
                {
                    return self.get_type_of_symbol(sym_id);
                }

                // === Check if lib files are loaded ===
                // When lib files are not loaded (noLib or no lib_contexts), emit errors
                // for missing global types. When lib files ARE loaded, we should have
                // found the symbol above - reaching here means a lookup failure.
                if !self.ctx.has_lib_loaded() {
                    // No lib files loaded - emit appropriate error for global type usage
                    use crate::lib_loader;
                    if lib_loader::is_es2015_plus_type(name) {
                        // ES2015+ type not available - emit TS2583 with library suggestion
                        self.error_cannot_find_name_change_lib(name, idx);
                    } else {
                        // For VALUE globals (console, Math, JSON, etc.), emit TS2304
                        // "Cannot find name" - same as TypeScript behavior
                        self.error_cannot_find_name_at(name, idx);
                    }
                    return TypeId::ERROR;
                }

                // Lib files are loaded but global was not found.
                // For DOM globals (console, window, etc.), emit TS2584 - they require the 'dom' lib.
                // For core ES globals (Math, Array, etc.), return ANY for graceful degradation
                // due to incomplete cross-lib symbol merging.
                {
                    use crate::checker::error_reporter::is_known_dom_global;
                    use crate::lib_loader;

                    // DOM globals require the 'dom' lib - emit TS2584
                    if is_known_dom_global(name) {
                        self.error_cannot_find_name_at(name, idx);
                        return TypeId::ERROR;
                    }

                    // ES2015+ types - emit TS2583 with library suggestion
                    if lib_loader::is_es2015_plus_type(name) {
                        self.error_cannot_find_global_type(name, idx);
                        return TypeId::ERROR;
                    }

                    // Core ES globals (Math, Array, etc.) - return ANY for graceful degradation
                    let first_char = name.chars().next().unwrap_or('a');
                    if first_char.is_uppercase() || self.is_known_global_value_name(name) {
                        return TypeId::ANY;
                    }

                    // Other unknown globals
                    if self.ctx.is_known_global_type(name) {
                        self.error_cannot_find_global_type(name, idx);
                    } else {
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

                // === Check if this is a known global that should be available ===
                // DOM globals require the 'dom' lib - emit TS2584.
                // Core ES globals - return ANY for graceful degradation when lib is loaded.
                // When lib files are NOT loaded, emit appropriate errors.
                if self.is_known_global_value_name(name) {
                    use crate::checker::error_reporter::is_known_dom_global;
                    use crate::lib_loader;

                    // DOM globals (console, window, etc.) require the 'dom' lib
                    if is_known_dom_global(name) {
                        // Emit TS2584 regardless of whether other libs are loaded
                        self.error_cannot_find_name_at(name, idx);
                        return TypeId::ERROR;
                    }

                    if self.ctx.has_lib_loaded() {
                        // Core ES globals - lib files loaded but global not found
                        // Return ANY for graceful degradation
                        return TypeId::ANY;
                    } else {
                        // No lib files loaded - emit appropriate error
                        if lib_loader::is_es2015_plus_type(name) {
                            // ES2015+ type - emit TS2583 with library suggestion
                            self.error_cannot_find_name_change_lib(name, idx);
                        } else if self.ctx.is_known_global_type(name) {
                            // Known global type - emit TS2318
                            self.error_cannot_find_global_type(name, idx);
                        } else {
                            // Other known global - emit TS2304
                            self.error_cannot_find_name_at(name, idx);
                        }
                        return TypeId::ERROR;
                    }
                }

                // Report "cannot find name" error
                // When lib files are loaded, suppress TS2304 for unresolved names
                // that might be from external modules or missing cross-file context.
                // This prevents cascading false positives.
                if !self.ctx.report_unresolved_imports {
                    // In single-file/conformance mode, many names can't be resolved
                    // because they come from other files. Return ANY to prevent cascading.
                    return TypeId::ANY;
                }
                self.error_cannot_find_name_at(name, idx);
                TypeId::ERROR
            }
        }
    }
}
