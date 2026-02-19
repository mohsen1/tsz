//! Type Computation (Complex Operations)
//!
//! Extracted from `type_computation.rs`: Complex type computation methods for
//! new expressions, constructability, union/keyof types, and class type helpers.

use crate::query_boundaries::type_computation_complex as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{ContextualTypeContext, TypeId};

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
pub(crate) fn is_contextually_sensitive(state: &CheckerState, idx: NodeIndex) -> bool {
    use tsz_parser::parser::syntax_kind_ext;

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
                                    && is_contextually_sensitive(state, prop.initializer)
                                {
                                    return true;
                                }
                            }
                            // Shorthand property: { x } refers to a variable, never sensitive
                            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                                // Variable references are not contextually sensitive
                                // (their type is already known from their declaration)
                            }
                            // Spread: check the expression being spread
                            k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                                if let Some(spread) = state.ctx.arena.get_spread(element)
                                    && is_contextually_sensitive(state, spread.expression)
                                {
                                    return true;
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
    fn constructor_identifier_name(&self, expr_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;
        let ident = self.ctx.arena.get_identifier(node)?;
        Some(ident.escaped_text.clone())
    }

    fn weak_collection_method_name_and_receiver(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<(String, NodeIndex)> {
        use tsz_parser::parser::syntax_kind_ext;

        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(node)?;
        let method = self
            .ctx
            .arena
            .get_identifier_at(access.name_or_argument)?
            .escaped_text
            .clone();
        Some((method, access.expression))
    }

    fn expr_contains_symbol_value(&mut self, expr_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        if self.get_type_of_node(expr_idx) == TypeId::SYMBOL {
            return true;
        }

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            && let Some(array_lit) = self.ctx.arena.get_literal_expr(node)
        {
            return array_lit
                .elements
                .nodes
                .iter()
                .copied()
                .any(|element| self.expr_contains_symbol_value(element));
        }

        false
    }

    pub(crate) fn should_suppress_weak_key_arg_mismatch(
        &mut self,
        callee_expr: NodeIndex,
        args: &[NodeIndex],
        mismatch_index: usize,
        actual: TypeId,
    ) -> bool {
        if !self.ctx.has_name_in_lib("WeakSet") {
            return false;
        }

        if actual == TypeId::SYMBOL {
            if let Some((method, receiver_expr)) =
                self.weak_collection_method_name_and_receiver(callee_expr)
                && mismatch_index == 0
            {
                let receiver_type = self.get_type_of_node(receiver_expr);
                let receiver_name = self.format_type(receiver_type);
                if ((method == "add" || method == "has" || method == "delete")
                    && receiver_name.contains("WeakSet"))
                    || ((method == "set"
                        || method == "has"
                        || method == "get"
                        || method == "delete")
                        && receiver_name.contains("WeakMap"))
                    || ((method == "register" || method == "unregister")
                        && receiver_name.contains("FinalizationRegistry"))
                {
                    return true;
                }
            }

            if mismatch_index == 0
                && let Some(callee_name) = self.constructor_identifier_name(callee_expr)
                && callee_name == "WeakRef"
            {
                return true;
            }
        }

        if mismatch_index == 0
            && let Some(callee_name) = self.constructor_identifier_name(callee_expr)
            && (callee_name == "WeakSet" || callee_name == "WeakMap")
            && let Some(&first_arg) = args.first()
            && self.expr_contains_symbol_value(first_arg)
        {
            return true;
        }

        false
    }

    pub(crate) fn should_suppress_weak_key_no_overload(
        &mut self,
        callee_expr: NodeIndex,
        args: &[NodeIndex],
    ) -> bool {
        if !self.ctx.has_name_in_lib("WeakSet") {
            return false;
        }

        let Some(callee_name) = self.constructor_identifier_name(callee_expr) else {
            return false;
        };

        if callee_name != "WeakSet" && callee_name != "WeakMap" {
            return false;
        }

        let Some(&first_arg) = args.first() else {
            return false;
        };

        self.expr_contains_symbol_value(first_arg)
    }

    /// For `new importAlias(...)` where `importAlias` is `import X = require("m")`,
    /// prefer the module's `export =` target type when available.
    ///
    /// This keeps general alias typing unchanged (important for type-position behavior)
    /// while ensuring constructor resolution sees the direct constructable type.
    fn new_expression_export_equals_constructor_type(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<TypeId> {
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if (symbol.flags & tsz_binder::symbol_flags::ALIAS) == 0 {
            return None;
        }

        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind != tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return None;
        }

        let import_decl = self.ctx.arena.get_import_decl(decl_node)?;
        let module_specifier = self.get_require_module_specifier(import_decl.module_specifier)?;
        let exports = self.resolve_effective_module_exports(&module_specifier)?;
        let export_equals_sym = exports.get("export=")?;
        Some(self.get_type_of_symbol(export_equals_sym))
    }

    pub(crate) fn get_type_of_new_expression(&mut self, idx: NodeIndex) -> TypeId {
        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_solver::CallResult;

        let Some(new_expr) = self.ctx.arena.get_call_expr_at(idx) else {
            return TypeId::ERROR; // Missing new expression data - propagate error
        };

        // Validate the constructor target: reject type-only symbols and abstract classes
        if let Some(early) = self.check_new_expression_target(idx, new_expr.expression) {
            return early;
        }

        // Get the type of the constructor expression.
        // Fast path for local class identifiers: avoid full identifier typing
        // machinery after `check_new_expression_target` has already validated
        // type-only/abstract constructor errors for this `new` target.
        let mut constructor_type = if let Some(expr_node) = self.ctx.arena.get(new_expr.expression)
        {
            if expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                let identifier_text = self
                    .ctx
                    .arena
                    .get_identifier(expr_node)
                    .map(|ident| ident.escaped_text.as_str())
                    .unwrap_or_default();
                let direct_symbol = self
                    .ctx
                    .binder
                    .node_symbols
                    .get(&new_expr.expression.0)
                    .copied();
                let fast_symbol = direct_symbol
                    .or_else(|| self.resolve_identifier_symbol(new_expr.expression))
                    .filter(|&sym_id| {
                        self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                            let is_single_class_decl = symbol.declarations.len() == 1
                                && !symbol.value_declaration.is_none()
                                && self.ctx.arena.get(symbol.value_declaration).is_some_and(
                                    |decl| decl.kind == syntax_kind_ext::CLASS_DECLARATION,
                                );
                            symbol.escaped_name == identifier_text
                                && is_single_class_decl
                                && (symbol.flags & tsz_binder::symbol_flags::CLASS) != 0
                                && (symbol.flags & tsz_binder::symbol_flags::VALUE) != 0
                                && (symbol.flags & tsz_binder::symbol_flags::ALIAS) == 0
                                && (symbol.decl_file_idx == u32::MAX
                                    || symbol.decl_file_idx == self.ctx.current_file_idx as u32)
                        })
                    });
                if let Some(sym_id) = fast_symbol {
                    self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
                    self.get_type_of_symbol(sym_id)
                } else {
                    self.get_type_of_node(new_expr.expression)
                }
            } else {
                self.get_type_of_node(new_expr.expression)
            }
        } else {
            self.get_type_of_node(new_expr.expression)
        };
        if let Some(export_equals_ctor) =
            self.new_expression_export_equals_constructor_type(new_expr.expression)
        {
            constructor_type = export_equals_ctor;
        }

        // Self-referencing class in static initializer: `new C()` inside C's static init
        // produces a Lazy placeholder. Return the cached instance type if available.
        if let Some(instance_type) =
            self.resolve_self_referencing_constructor(constructor_type, new_expr.expression)
        {
            return instance_type;
        }

        // Validate explicit type arguments against constraints (TS2344)
        if let Some(ref type_args_list) = new_expr.type_arguments
            && !type_args_list.nodes.is_empty()
        {
            self.validate_new_expression_type_arguments(constructor_type, type_args_list, idx);
        }

        // If the `new` expression provides explicit type arguments (`new Foo<T>()`),
        // instantiate the constructor signatures with those args so we don't fall back to
        // inference (and so we match tsc behavior).
        constructor_type = self.apply_type_arguments_to_constructor_type(
            constructor_type,
            new_expr.type_arguments.as_ref(),
        );

        // Check if the constructor type contains any abstract classes (for union types)
        // e.g., `new cls()` where `cls: typeof AbstractA | typeof AbstractB`
        //
        // First, resolve any Lazy types (type aliases) so we can check the actual types
        let resolved_type = self.resolve_lazy_type(constructor_type);
        if self.type_contains_abstract_class(resolved_type) {
            self.error_at_node(
                idx,
                "Cannot create an instance of an abstract class.",
                diagnostic_codes::CANNOT_CREATE_AN_INSTANCE_OF_AN_ABSTRACT_CLASS,
            );
            return TypeId::ERROR;
        }

        // TSZ-4 Priority 3: Check constructor accessibility (TS2673/TS2674)
        // Private constructors can only be called within the class
        // Protected constructors can only be called within the class hierarchy
        self.check_constructor_accessibility_for_new(idx, constructor_type);

        if constructor_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if constructor_type == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        // Evaluate application types (e.g., Newable<T>, Constructor<{}>) to get the actual Callable
        constructor_type = self.evaluate_application_type(constructor_type);

        // For intersection types (e.g., Constructor<Tagged> & typeof Base), evaluate
        // Application members within the intersection so the solver can find construct
        // signatures from all members. Without this, `Constructor<Tagged>` would remain
        // an unevaluated Application and its construct signature would be missed.
        constructor_type = self.evaluate_application_members_in_intersection(constructor_type);

        // Resolve Ref types to ensure we get the actual constructor type, not just a symbolic reference
        // This is critical for classes where we need the Callable with construct signatures
        constructor_type = self.resolve_ref_type(constructor_type);

        // Resolve type parameter constraints: if the constructor type is a type parameter
        // (e.g., T extends Constructable), resolve the constraint's lazy types so the solver
        // can find construct signatures through the constraint chain.
        constructor_type = self.resolve_type_param_for_construct(constructor_type);

        // Some constructor interfaces are lowered with a synthetic `"new"` property
        // instead of explicit construct signatures.
        let synthetic_new_constructor = self.constructor_type_from_new_property(constructor_type);
        constructor_type = synthetic_new_constructor.unwrap_or(constructor_type);
        // Explicit type arguments on `new` (e.g. `new Promise<number>(...)`) need to
        // apply to synthetic `"new"` member call signatures as well.
        constructor_type = if synthetic_new_constructor.is_some() {
            self.apply_type_arguments_to_callable_type(
                constructor_type,
                new_expr.type_arguments.as_ref(),
            )
        } else {
            constructor_type
        };

        // Collect arguments
        let args = match new_expr.arguments.as_ref() {
            Some(a) => a.nodes.as_slice(),
            None => &[],
        };

        // Prepare argument types with contextual typing
        // Note: We use a generic context helper here because we delegate the specific
        // signature selection to the solver.
        let ctx_helper = ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            constructor_type,
            self.ctx.compiler_options.no_implicit_any,
        );
        let check_excess_properties = true; // Default to true, solver handles specifics
        let arg_types = self.collect_call_argument_types_with_context(
            args,
            |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
            check_excess_properties,
            None, // No skipping needed for constructor calls
        );

        self.ensure_relation_input_ready(constructor_type);
        self.ensure_relation_inputs_ready(&arg_types);

        // Delegate to Solver for constructor resolution
        let result = self.resolve_new_with_checker_adapter(constructor_type, &arg_types, false);

        match result {
            CallResult::Success(return_type) => return_type,
            CallResult::NotCallable { .. } => {
                // In circular class-resolution scenarios, class constructor targets can
                // transiently lose construct signatures. TypeScript suppresses TS2351
                // here and reports the underlying class/argument diagnostics instead.
                if self.new_target_is_class_symbol(new_expr.expression) {
                    return TypeId::ERROR;
                }
                self.error_not_constructable_at(constructor_type, idx);
                TypeId::ERROR
            }
            CallResult::ArgumentCountMismatch {
                expected_min,
                expected_max,
                actual,
            } => {
                // Determine which error to emit:
                // - TS2555: "Expected at least N arguments" only for rest params (unbounded)
                // - TS2554: "Expected N arguments" or "Expected N-M arguments" otherwise
                if actual < expected_min && expected_max.is_none() {
                    // Too few arguments with rest parameters (unbounded) - use TS2555
                    self.error_expected_at_least_arguments_at(expected_min, actual, idx);
                } else {
                    // Use TS2554 for exact count, range, or too many args
                    let expected = expected_max.unwrap_or(expected_min);
                    self.error_argument_count_mismatch_at(expected, actual, idx);
                }
                // Recover with the constructor instance type so downstream checks
                // (e.g. property access TS2339) still run after arity diagnostics.
                self.instance_type_from_constructor_type(constructor_type)
                    .unwrap_or(TypeId::ERROR)
            }
            CallResult::OverloadArgumentCountMismatch {
                actual,
                expected_low,
                expected_high,
            } => {
                self.error_at_node(
                    idx,
                    &format!(
                        "No overload expects {actual} arguments, but overloads do exist that expect either {expected_low} or {expected_high} arguments."
                    ),
                    diagnostic_codes::NO_OVERLOAD_EXPECTS_ARGUMENTS_BUT_OVERLOADS_DO_EXIST_THAT_EXPECT_EITHER_OR_ARGUM,
                );
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
                    if !self.should_suppress_weak_key_arg_mismatch(
                        new_expr.expression,
                        args,
                        index,
                        actual,
                    ) {
                        let _ = self.check_argument_assignable_or_report(actual, expected, arg_idx);
                    }
                }
                TypeId::ERROR
            }
            CallResult::TypeParameterConstraintViolation {
                inferred_type,
                constraint_type,
                return_type,
            } => {
                // Report TS2322 instead of TS2345 for constraint violations from
                // callback return type inference.
                let _ = self.check_assignable_or_report_generic_at(
                    inferred_type,
                    constraint_type,
                    idx,
                    idx,
                );
                return_type
            }
            CallResult::NoOverloadMatch { failures, .. } => {
                if !self.should_suppress_weak_key_no_overload(new_expr.expression, args) {
                    self.error_no_overload_matches_at(idx, &failures);
                }
                TypeId::ERROR
            }
        }
    }

    /// For intersection constructor types, evaluate any Application members so
    /// the solver can resolve their construct signatures.
    ///
    /// e.g. `Constructor<Tagged> & typeof Base` — `Constructor<Tagged>` is an
    /// Application that must be instantiated to reveal `new(...) => Tagged`.
    fn evaluate_application_members_in_intersection(&mut self, type_id: TypeId) -> TypeId {
        let Some(members) = query::intersection_members(self.ctx.types, type_id) else {
            return type_id;
        };

        let mut changed = false;
        let mut new_members = Vec::with_capacity(members.len());

        for member in &members {
            let evaluated = self.evaluate_application_type(*member);
            if evaluated != *member {
                changed = true;
                new_members.push(evaluated);
            } else {
                new_members.push(*member);
            }
        }

        if changed {
            self.ctx.types.intersection(new_members)
        } else {
            type_id
        }
    }

    /// Validate the target of a `new` expression: reject type-only symbols and
    /// abstract classes. Returns `Some(TypeId)` if the expression should bail early.
    fn check_new_expression_target(
        &mut self,
        new_idx: NodeIndex,
        expr_idx: NodeIndex,
    ) -> Option<TypeId> {
        use crate::diagnostics::diagnostic_codes;
        use tsz_binder::symbol_flags;
        use tsz_scanner::SyntaxKind;

        // Primitive type keywords in constructor position (`new number[]`) are
        // type-only and should report TS2693.
        if let Some(expr_node) = self.ctx.arena.get(expr_idx) {
            let keyword_name = match expr_node.kind {
                k if k == SyntaxKind::NumberKeyword as u16 => Some("number"),
                k if k == SyntaxKind::StringKeyword as u16 => Some("string"),
                k if k == SyntaxKind::BooleanKeyword as u16 => Some("boolean"),
                k if k == SyntaxKind::SymbolKeyword as u16 => Some("symbol"),
                k if k == SyntaxKind::VoidKeyword as u16 => Some("void"),
                k if k == SyntaxKind::UndefinedKeyword as u16 => Some("undefined"),
                k if k == SyntaxKind::NullKeyword as u16 => Some("null"),
                k if k == SyntaxKind::AnyKeyword as u16 => Some("any"),
                k if k == SyntaxKind::UnknownKeyword as u16 => Some("unknown"),
                k if k == SyntaxKind::NeverKeyword as u16 => Some("never"),
                k if k == SyntaxKind::ObjectKeyword as u16 => Some("object"),
                k if k == SyntaxKind::BigIntKeyword as u16 => Some("bigint"),
                _ => None,
            };
            if let Some(keyword_name) = keyword_name {
                self.error_type_only_value_at(keyword_name, expr_idx);
                return Some(TypeId::ERROR);
            }
        }

        let ident = self.ctx.arena.get_identifier_at(expr_idx)?;
        let class_name = &ident.escaped_text;

        let sym_id = self
            .resolve_identifier_symbol(expr_idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, expr_idx))
            .or_else(|| self.ctx.binder.get_node_symbol(expr_idx))
            .or_else(|| self.ctx.binder.file_locals.get(class_name))
            .or_else(|| self.ctx.binder.get_symbols().find_by_name(class_name))?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        let has_type = (symbol.flags & symbol_flags::TYPE) != 0;
        let has_value = (symbol.flags & symbol_flags::VALUE) != 0;
        let is_type_alias = (symbol.flags & symbol_flags::TYPE_ALIAS) != 0;

        if is_type_alias || (has_type && !has_value) {
            self.error_type_only_value_at(class_name, expr_idx);
            return Some(TypeId::ERROR);
        }
        if symbol.flags & symbol_flags::ABSTRACT != 0 {
            self.error_at_node(
                new_idx,
                "Cannot create an instance of an abstract class.",
                diagnostic_codes::CANNOT_CREATE_AN_INSTANCE_OF_AN_ABSTRACT_CLASS,
            );
            return Some(TypeId::ERROR);
        }
        None
    }

    fn new_target_is_class_symbol(&self, expr_idx: NodeIndex) -> bool {
        use tsz_binder::symbol_flags;
        let Some(ident) = self.ctx.arena.get_identifier_at(expr_idx) else {
            return false;
        };
        let name = &ident.escaped_text;
        let Some(sym_id) = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expr_idx)
            .or_else(|| self.ctx.binder.get_node_symbol(expr_idx))
            .or_else(|| self.ctx.binder.file_locals.get(name))
            .or_else(|| self.ctx.binder.get_symbols().find_by_name(name))
        else {
            return false;
        };
        self.ctx
            .binder
            .get_symbol(sym_id)
            .is_some_and(|symbol| (symbol.flags & symbol_flags::CLASS) != 0)
    }

    /// Resolve a self-referencing class constructor in a static initializer.
    /// When `new C()` appears inside C's own static property initializer, the
    /// constructor type is a Lazy placeholder. Returns the cached instance type
    /// if the symbol is a class with a cached instance type.
    fn resolve_self_referencing_constructor(
        &self,
        constructor_type: TypeId,
        expr_idx: NodeIndex,
    ) -> Option<TypeId> {
        use tsz_binder::symbol_flags;

        tsz_solver::visitor::lazy_def_id(self.ctx.types, constructor_type)?;
        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expr_idx)
            .or_else(|| self.ctx.binder.get_node_symbol(expr_idx))?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::CLASS == 0 {
            return None;
        }
        if let Some(&instance_type) = self.ctx.symbol_instance_types.get(&sym_id) {
            return Some(instance_type);
        }
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };
        self.ctx.class_instance_type_cache.get(&decl_idx).copied()
    }

    /// Check if a type contains any abstract class constructors.
    ///
    /// This handles union types like `typeof AbstractA | typeof ConcreteB`.
    /// Recursively checks union and intersection types for abstract class members.
    pub(crate) fn type_contains_abstract_class(&self, type_id: TypeId) -> bool {
        self.type_contains_abstract_class_inner(type_id, &mut rustc_hash::FxHashSet::default())
    }

    fn type_contains_abstract_class_inner(
        &self,
        type_id: TypeId,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) -> bool {
        use tsz_binder::SymbolId;
        use tsz_binder::symbol_flags;

        // Prevent infinite loops in circular type references
        if !visited.insert(type_id) {
            return false;
        }

        // Special handling for Callable types - check if the symbol is abstract
        if let Some(callable_shape) = query::callable_shape_for_type(self.ctx.types, type_id)
            && let Some(sym_id) = callable_shape.symbol
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            return (symbol.flags & symbol_flags::ABSTRACT) != 0;
        }
        // If no symbol or not abstract, fall through to general classification

        // Special handling for Lazy types - need to check via context
        if let Some(def_id) = query::lazy_def_id(self.ctx.types, type_id) {
            // Try to get the SymbolId for this DefId
            if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                let is_abstract = (symbol.flags & symbol_flags::ABSTRACT) != 0;
                if is_abstract {
                    return true;
                }
                // If not abstract, check if it's a type alias and recurse into its body
                if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
                    // Get the body from the definition_store and recurse
                    // NOTE: We need to use resolve_lazy_type here to handle nested type aliases
                    if let Some(def) = self.ctx.definition_store.get(def_id)
                        && let Some(body_type) = def.body
                    {
                        // Recursively check the body (which may be a union, another lazy, etc.)
                        return self.type_contains_abstract_class_inner(body_type, visited);
                    }
                }
            }
            // If we can't map to a symbol, fall through to general classification
        }

        match query::classify_for_abstract_check(self.ctx.types, type_id) {
            // TypeQuery is `typeof ClassName` - check if the symbol is abstract
            // Since get_type_from_type_query now uses real SymbolIds, we can directly look up
            query::AbstractClassCheckKind::TypeQuery(sym_ref) => {
                if let Some(symbol) = self.ctx.binder.get_symbol(SymbolId(sym_ref.0))
                    && symbol.flags & symbol_flags::ABSTRACT != 0
                {
                    return true;
                }
                false
            }
            // Union type - check if ANY constituent is abstract
            query::AbstractClassCheckKind::Union(members) => members
                .iter()
                .any(|&member| self.type_contains_abstract_class_inner(member, visited)),
            // Intersection type - check if ANY constituent is abstract
            query::AbstractClassCheckKind::Intersection(members) => members
                .iter()
                .any(|&member| self.type_contains_abstract_class_inner(member, visited)),
            query::AbstractClassCheckKind::NotAbstract => false,
        }
    }

    /// Get the construct type from a `TypeId`, used for new expressions.
    ///
    /// This is similar to `get_construct_signature_return_type` but returns
    /// the full construct type (not just the return type) for new expressions.
    ///
    /// The `emit_error` parameter controls whether we emit TS2507 errors.
    /// Resolve Ref types to their actual types.
    ///
    /// For symbol references (Ref), this resolves them to the symbol's declared type.
    /// This is important for new expressions where we need the actual constructor type
    /// with construct signatures, not just a symbolic reference.
    pub(crate) fn resolve_ref_type(&mut self, type_id: TypeId) -> TypeId {
        match query::classify_for_lazy_resolution(self.ctx.types, type_id) {
            query::LazyTypeKind::Lazy(def_id) => {
                if let Some(symbol_id) = self.ctx.def_to_symbol_id(def_id) {
                    let symbol_type = self.get_type_of_symbol(symbol_id);
                    if symbol_type == type_id {
                        // symbol_types cache contains the Lazy type itself (can happen
                        // when check_variable_declaration overwrites the structural type
                        // with the Lazy annotation type for `declare var X: X` patterns).
                        // Fall back to the type environment which may still have the
                        // structural type from initial symbol resolution.
                        if let Ok(env) = self.ctx.type_env.try_borrow()
                            && let Some(env_type) = env.get_def(def_id)
                            && env_type != type_id
                        {
                            return env_type;
                        }
                        type_id
                    } else {
                        symbol_type
                    }
                } else {
                    type_id
                }
            }
            _ => type_id, // Handle all cases
        }
    }

    /// Resolve type parameter constraints for construct expressions.
    ///
    /// When the constructor type is a `TypeParameter` (e.g., `T extends Constructable`),
    /// the solver's `resolve_new` tries to look through the constraint. But if the
    /// constraint is a Lazy type (interface), the solver can't resolve it because it
    /// lacks the type environment. This method pre-resolves the constraint so the
    /// solver can find construct signatures.
    fn resolve_type_param_for_construct(&mut self, type_id: TypeId) -> TypeId {
        let factory = self.ctx.types.factory();
        let Some(info) = query::type_parameter_info(self.ctx.types, type_id) else {
            return type_id;
        };

        let Some(constraint) = info.constraint else {
            return type_id;
        };

        // Resolve the constraint if it's a Lazy type (interface/type alias)
        let resolved_constraint = self.resolve_lazy_type(constraint);
        if resolved_constraint == constraint {
            return type_id;
        }

        // Create a new TypeParameter with the resolved constraint
        let new_info = tsz_solver::TypeParamInfo {
            constraint: Some(resolved_constraint),
            ..info
        };
        factory.type_param(new_info)
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
        let factory = self.ctx.types.factory();
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

            return factory.union(member_types);
        }

        TypeId::ERROR // Missing composite type data - propagate error
    }

    /// Get type from an intersection type node (A & B).
    ///
    /// Uses `CheckerState`'s `get_type_from_type_node` for each member to ensure
    /// typeof expressions are resolved via binder (same reason as union types).
    pub(crate) fn get_type_from_intersection_type(&mut self, idx: NodeIndex) -> TypeId {
        let factory = self.ctx.types.factory();
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        if let Some(composite) = self.ctx.arena.get_composite_type(node) {
            let mut member_types = Vec::new();
            for &type_idx in &composite.types.nodes {
                member_types.push(self.get_type_from_type_node(type_idx));
            }

            if member_types.is_empty() {
                return TypeId::UNKNOWN;
            }
            if member_types.len() == 1 {
                return member_types[0];
            }

            return factory.intersection(member_types);
        }

        TypeId::ERROR
    }

    /// Get type from a type operator node (readonly T[], readonly [T, U], unique symbol).
    ///
    /// Handles type modifiers like:
    /// - `readonly T[]` - Creates `ReadonlyType` wrapper
    /// - `unique symbol` - Special marker for unique symbols
    pub(crate) fn get_type_from_type_operator(&mut self, idx: NodeIndex) -> TypeId {
        let factory = self.ctx.types.factory();
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        if let Some(type_op) = self.ctx.arena.get_type_operator(node) {
            let operator = type_op.operator;
            let inner_type = self.get_type_from_type_node(type_op.type_node);

            // Handle readonly operator
            if operator == SyntaxKind::ReadonlyKeyword as u16 {
                // Wrap the inner type in ReadonlyType
                return factory.readonly_type(inner_type);
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
    pub(crate) fn get_keyof_type(&mut self, operand: TypeId) -> TypeId {
        use tsz_solver::type_queries_extended::{TypeResolutionKind, classify_for_type_resolution};

        // Handle Lazy types by attempting to resolve them first
        // This allows keyof Lazy(DefId) to work correctly for circular dependencies
        match classify_for_type_resolution(self.ctx.types, operand) {
            TypeResolutionKind::Lazy(def_id) => {
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
                    let resolved = self.get_type_of_symbol(sym_id);
                    // Recursively get keyof of the resolved type
                    return self.get_keyof_type(resolved);
                }
            }
            TypeResolutionKind::Application => {
                // Evaluate application types first
                let evaluated = self.evaluate_type_for_assignability(operand);
                return self.get_keyof_type(evaluated);
            }
            TypeResolutionKind::Resolved => {}
        }

        tsz_solver::type_queries::keyof_object_properties(self.ctx.types, operand)
            .unwrap_or(TypeId::NEVER)
    }

    /// Extract string literal keys from a union or single literal type.
    ///
    /// Given a type that may be a union of string literal types or a single string literal,
    /// Get the class declaration node from a `TypeId`.
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
        if self.ctx.class_decl_miss_cache.borrow().contains(&type_id) {
            return None;
        }

        use tsz_binder::SymbolId;

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
            match query::classify_for_class_decl(checker.ctx.types, type_id) {
                query::ClassDeclTypeKind::Object(shape_id) => {
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
                query::ClassDeclTypeKind::Members(members) => {
                    for member in members {
                        collect_candidates(checker, member, out);
                    }
                }
                query::ClassDeclTypeKind::NotObject => {}
            }
        }

        let mut candidates = Vec::new();
        collect_candidates(self, type_id, &mut candidates);
        if candidates.is_empty() {
            self.ctx.class_decl_miss_cache.borrow_mut().insert(type_id);
            return None;
        }
        if candidates.len() == 1 {
            let class_idx = candidates[0];
            self.ctx.class_decl_miss_cache.borrow_mut().remove(&type_id);
            return Some(class_idx);
        }

        let resolved = candidates
            .iter()
            .find(|&&candidate| {
                candidates.iter().all(|&other| {
                    candidate == other || self.is_class_derived_from(candidate, other)
                })
            })
            .copied();
        if resolved.is_none() {
            self.ctx.class_decl_miss_cache.borrow_mut().insert(type_id);
        } else {
            self.ctx.class_decl_miss_cache.borrow_mut().remove(&type_id);
        }
        resolved
    }

    /// Get the class name from a `TypeId` if it represents a class instance.
    ///
    /// Returns the class name as a string if the type represents a class,
    /// or None if the type doesn't represent a class or the class has no name.
    pub(crate) fn get_class_name_from_type(&self, type_id: TypeId) -> Option<String> {
        self.get_class_decl_from_type(type_id)
            .map(|class_idx| self.get_class_name_from_decl(class_idx))
    }
}
