//! Type Computation (Complex Operations)
//!
//! Extracted from `type_computation.rs`: Second half of `CheckerState` impl
//! containing complex type computation methods for new expressions,
//! call expressions, constructability, union/keyof types, and identifiers.

use crate::query_boundaries::call_checker;
use crate::query_boundaries::type_computation_complex as query;
use crate::state::CheckerState;
use tracing::trace;
use tsz_binder::SymbolId;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{ContextualTypeContext, TypeId};

struct CallResultContext<'a> {
    callee_expr: NodeIndex,
    call_idx: NodeIndex,
    args: &'a [NodeIndex],
    arg_types: &'a [TypeId],
    callee_type: TypeId,
    is_super_call: bool,
    is_optional_chain: bool,
}

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

    fn should_suppress_weak_key_arg_mismatch(
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

    fn should_suppress_weak_key_no_overload(
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
        use tsz_solver::CallResult;

        let Some(new_expr) = self.ctx.arena.get_call_expr_at(idx) else {
            return TypeId::ERROR; // Missing new expression data - propagate error
        };

        // Validate the constructor target: reject type-only symbols and abstract classes
        if let Some(early) = self.check_new_expression_target(idx, new_expr.expression) {
            return early;
        }

        // Get the type of the constructor expression
        let mut constructor_type = self.get_type_of_node(new_expr.expression);
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
        let result = {
            let env = self.ctx.type_env.borrow();
            call_checker::resolve_new_with_context(
                self.ctx.types,
                &self.ctx,
                &env,
                constructor_type,
                &arg_types,
                false,
            )
        };

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
        use tsz_solver::{
            type_queries::KeyOfTypeKind,
            type_queries_extended::{
                TypeResolutionKind, classify_for_keyof, classify_for_type_resolution,
            },
        };

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

        match classify_for_keyof(self.ctx.types, operand) {
            KeyOfTypeKind::Object(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                if shape.properties.is_empty() {
                    return TypeId::NEVER;
                }
                let factory = self.ctx.types.factory();
                let key_types: Vec<TypeId> = shape
                    .properties
                    .iter()
                    .map(|p| self.ctx.types.literal_string_atom(p.name))
                    .collect();
                factory.union(key_types)
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
    ) -> Vec<tsz_common::interner::Atom> {
        match query::classify_for_string_literal_keys(self.ctx.types, type_id) {
            query::StringLiteralKeyKind::SingleString(name) => vec![name],
            query::StringLiteralKeyKind::Union(members) => members
                .iter()
                .filter_map(|&member| query::string_literal_value(self.ctx.types, member))
                .collect(),
            query::StringLiteralKeyKind::NotStringLiteral => Vec::new(),
        }
    }

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

    /// Get the class name from a `TypeId` if it represents a class instance.
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
        // Check call depth limit to prevent infinite recursion
        if !self.ctx.call_depth.borrow_mut().enter() {
            return TypeId::ERROR;
        }

        let result = self.get_type_of_call_expression_inner(idx);

        self.ctx.call_depth.borrow_mut().leave();
        result
    }

    /// Inner implementation of call expression type resolution.
    pub(crate) fn get_type_of_call_expression_inner(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::node_flags;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_solver::instantiate_type;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return TypeId::ERROR; // Missing call expression data - propagate error
        };

        // Get the type of the callee
        let mut callee_type = self.get_type_of_node(call.expression);
        trace!(
            callee_type = ?callee_type,
            callee_expr = ?call.expression,
            "Call expression callee type resolved"
        );

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
        let args = match call.arguments.as_ref() {
            Some(a) => a.nodes.as_slice(),
            None => &[],
        };

        // Check if callee is any/error (don't report for those)
        if callee_type == TypeId::ANY {
            if let Some(ref type_args_list) = call.type_arguments
                && !type_args_list.nodes.is_empty()
            {
                self.error_at_node(
                    idx,
                    crate::diagnostics::diagnostic_messages::UNTYPED_FUNCTION_CALLS_MAY_NOT_ACCEPT_TYPE_ARGUMENTS,
                    crate::diagnostics::diagnostic_codes::UNTYPED_FUNCTION_CALLS_MAY_NOT_ACCEPT_TYPE_ARGUMENTS,
                );
            }
            // Still need to check arguments for definite assignment (TS2454) and other errors
            // Create a dummy context helper that returns None for all parameter types
            let _ctx_helper = ContextualTypeContext::new(self.ctx.types);
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None, // No parameter type info for ANY callee
                check_excess_properties,
                None, // No skipping needed
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
                None, // No skipping needed
            );
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        // Check for never type - emit TS18050 "The value 'never' cannot be used here"
        if callee_type == TypeId::NEVER {
            // Check arguments even for never type to catch other errors
            let _ctx_helper = ContextualTypeContext::new(self.ctx.types);
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None,
                check_excess_properties,
                None,
            );

            // Emit TS18050 for calling never type
            if (node.flags as u32) & node_flags::OPTIONAL_CHAIN == 0 {
                self.report_never_type_usage(call.expression);
            }

            return if (node.flags as u32) & node_flags::OPTIONAL_CHAIN != 0 {
                TypeId::UNDEFINED
            } else {
                TypeId::ERROR
            };
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

        let classification =
            query::classify_for_call_signatures(self.ctx.types, callee_type_for_resolution);
        trace!(
            callee_type_for_resolution = ?callee_type_for_resolution,
            classification = ?classification,
            "Call signatures classified"
        );
        let overload_signatures = match classification {
            query::CallSignaturesKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                (shape.call_signatures.len() > 1).then(|| shape.call_signatures.clone())
            }
            query::CallSignaturesKind::MultipleSignatures(signatures) => {
                (signatures.len() > 1).then_some(signatures)
            }
            query::CallSignaturesKind::NoSignatures => None,
        };

        // Overload candidates need signature-specific contextual typing.
        let force_bivariant_callbacks = matches!(
            self.ctx.arena.get(call.expression).map(|n| n.kind),
            Some(
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            )
        );

        if let Some(signatures) = overload_signatures.as_deref()
            && let Some(return_type) = self.resolve_overloaded_call_with_signatures(
                args,
                signatures,
                force_bivariant_callbacks,
            )
        {
            trace!(
                return_type = ?return_type,
                signatures_count = signatures.len(),
                "Resolved overloaded call return type"
            );
            let return_type =
                self.apply_this_substitution_to_call_return(return_type, call.expression);
            return if nullish_cause.is_some() {
                self.ctx
                    .types
                    .factory()
                    .union(vec![return_type, TypeId::UNDEFINED])
            } else {
                return_type
            };
        }

        // Resolve Ref types to get the actual callable for FunctionShape extraction
        // This is needed before we can check if the callee is generic
        let callee_type_for_shape = self.resolve_ref_type(callee_type_for_resolution);

        // Extract function shape to check if this is a generic call that needs two-pass inference
        let callee_shape =
            call_checker::get_contextual_signature(self.ctx.types, callee_type_for_shape);
        let is_generic_call = callee_shape
            .as_ref()
            .is_some_and(|s| !s.type_params.is_empty())
            && call.type_arguments.is_none(); // Only use two-pass if no explicit type args

        // Create contextual context from callee type with type arguments applied
        let ctx_helper = ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            callee_type_for_resolution,
            self.ctx.compiler_options.no_implicit_any,
        );
        let check_excess_properties = overload_signatures.is_none();

        // Two-pass argument collection for generic calls is only needed when at least one
        // argument is contextually sensitive (e.g. lambdas/object literals needing contextual type).
        let arg_types = if is_generic_call {
            if let Some(shape) = callee_shape {
                // Pre-compute which arguments are contextually sensitive to avoid borrowing self in closures.
                let sensitive_args: Vec<bool> = args
                    .iter()
                    .map(|&arg| is_contextually_sensitive(self, arg))
                    .collect();
                let needs_two_pass = sensitive_args.iter().copied().any(std::convert::identity);

                if needs_two_pass {
                    // === Round 1: Collect non-contextual argument types ===
                    // This allows type parameters to be inferred from concrete arguments.
                    // CRITICAL: Skip checking sensitive arguments entirely to prevent TS7006
                    // from being emitted before inference completes.
                    let mut round1_arg_types = self.collect_call_argument_types_with_context(
                        args,
                        |i, arg_count| {
                            // Skip contextually sensitive arguments in Round 1.
                            if sensitive_args[i] {
                                None
                            } else {
                                ctx_helper.get_parameter_type_for_call(i, arg_count)
                            }
                        },
                        check_excess_properties,
                        Some(&sensitive_args), // Skip sensitive args in Round 1
                    );

                    // For sensitive object literal arguments, extract a partial type
                    // from non-sensitive properties to improve inference.
                    // This handles patterns like:
                    //   app({ state: 100, actions: { foo: s => s } })
                    // where `state: 100` can infer State=number, but `actions` is
                    // context-sensitive and must wait for Round 2.
                    for (i, &arg_idx) in args.iter().enumerate() {
                        if sensitive_args[i]
                            && let Some(partial) = self.extract_non_sensitive_object_type(arg_idx)
                        {
                            trace!(
                                arg_index = i,
                                partial_type = partial.0,
                                "Round 1: extracted non-sensitive partial type for object literal"
                            );
                            round1_arg_types[i] = partial;
                        }
                    }

                    // === Perform Round 1 Inference ===
                    // Pre-evaluate function shape parameter types through the
                    // TypeEnvironment so the solver can constrain against concrete
                    // object types instead of unresolved Application types.
                    // Example: Opts<State, Actions> → { state?: State, actions: Actions }
                    let evaluated_shape = {
                        let new_params: Vec<_> = shape
                            .params
                            .iter()
                            .map(|p| tsz_solver::ParamInfo {
                                name: p.name,
                                type_id: self.evaluate_type_with_env(p.type_id),
                                optional: p.optional,
                                rest: p.rest,
                            })
                            .collect();
                        tsz_solver::FunctionShape {
                            params: new_params,
                            return_type: shape.return_type,
                            this_type: shape.this_type,
                            type_params: shape.type_params.clone(),
                            type_predicate: shape.type_predicate.clone(),
                            is_constructor: shape.is_constructor,
                            is_method: shape.is_method,
                        }
                    };
                    let substitution = {
                        let env = self.ctx.type_env.borrow();
                        call_checker::compute_contextual_types_with_context(
                            self.ctx.types,
                            &self.ctx,
                            &env,
                            &evaluated_shape,
                            &round1_arg_types,
                            self.ctx.contextual_type,
                        )
                    };
                    trace!(
                        substitution_is_empty = substitution.is_empty(),
                        "Round 1 inference: substitution computed"
                    );

                    // === Pre-evaluate instantiated parameter types ===
                    // After instantiation with Round 1 substitution, parameter types may
                    // contain unevaluated IndexAccess/KeyOf over Lazy(DefId) references
                    // (e.g., OptionsForKey[K] → OptionsForKey["a"]). The QueryCache's
                    // evaluate_type uses NoopResolver which can't resolve Lazy types.
                    // Use evaluate_type_with_env which resolves Lazy types via the
                    // TypeEnvironment before evaluation.
                    let arg_count = args.len();
                    let mut round2_contextual_types: Vec<Option<TypeId>> =
                        Vec::with_capacity(arg_count);
                    for i in 0..arg_count {
                        let ctx_type = if let Some(param_type) =
                            ctx_helper.get_parameter_type_for_call(i, arg_count)
                        {
                            let instantiated =
                                instantiate_type(self.ctx.types, param_type, &substitution);
                            Some(self.evaluate_type_with_env(instantiated))
                        } else {
                            None
                        };
                        trace!(
                            arg_index = i,
                            ctx_type_id = ?ctx_type.map(|t| t.0),
                            "Round 2: contextual type for argument"
                        );
                        round2_contextual_types.push(ctx_type);
                    }

                    // === Round 2: Collect ALL argument types with contextual typing ===
                    // Now that type parameters are partially inferred, lambdas get proper contextual types.
                    self.collect_call_argument_types_with_context(
                        args,
                        |i, _arg_count| round2_contextual_types[i],
                        check_excess_properties,
                        None, // Don't skip anything in Round 2 - check all args with inferred context
                    )
                } else {
                    // No context-sensitive arguments: skip Round 1/2 and use single-pass collection.
                    self.collect_call_argument_types_with_context(
                        args,
                        |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
                        check_excess_properties,
                        None, // No skipping needed for single-pass
                    )
                }
            } else {
                // Shouldn't happen for generic call detection, but keep single-pass fallback.
                self.collect_call_argument_types_with_context(
                    args,
                    |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
                    check_excess_properties,
                    None, // No skipping needed for single-pass
                )
            }
        } else {
            // === Single-pass: Standard argument collection ===
            // Non-generic calls or calls with explicit type arguments use the standard flow.
            self.collect_call_argument_types_with_context(
                args,
                |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
                check_excess_properties,
                None, // No skipping needed for single-pass
            )
        };

        // Delegate the call resolution to solver boundary helpers.
        self.ensure_relation_input_ready(callee_type_for_resolution);
        self.ensure_relation_inputs_ready(&arg_types);

        // Evaluate application types to resolve Ref bases to actual Callable types
        // This is needed for cases like `GenericCallable<string>` where the type is
        // stored as Application(Ref(symbol_id), [string]) and needs to be resolved
        // to the actual Callable with call signatures
        let callee_type_for_call = self.evaluate_application_type(callee_type_for_resolution);
        // Resolve lazy (Ref) types to their underlying callable types.
        // This handles interfaces with call signatures, merged declarations, etc.
        // Use resolve_lazy_type instead of resolve_ref_type to also resolve Lazy
        // types nested inside intersection/union members.
        let callee_type_for_call = self.resolve_lazy_type(callee_type_for_call);

        // The `Function` interface from lib.d.ts has no call signatures, but in TypeScript
        // it is callable and returns `any`. Check if the callee is the Function boxed type
        // or the Function intrinsic and handle it like `any`.
        // The `Function` interface from lib.d.ts has no call signatures, but in TypeScript
        // it is callable and returns `any`. We check both the intrinsic TypeId::FUNCTION
        // and the global Function interface type resolved from lib.d.ts.
        // For unions containing Function members, we replace those members with a
        // synthetic callable that returns `any` so resolve_union_call succeeds.
        let callee_type_for_call =
            self.replace_function_type_for_call(callee_type, callee_type_for_call);
        if callee_type_for_call == TypeId::ANY {
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None,
                check_excess_properties,
                None, // No skipping needed
            );
            return if nullish_cause.is_some() {
                self.ctx
                    .types
                    .factory()
                    .union(vec![TypeId::ANY, TypeId::UNDEFINED])
            } else {
                TypeId::ANY
            };
        }

        // Ensure relation preconditions (lazy refs + application symbols) for callee/args.
        self.ensure_relation_input_ready(callee_type_for_call);
        self.ensure_relation_inputs_ready(&arg_types);

        let result = {
            let env = self.ctx.type_env.borrow();
            // super() calls are constructor calls, not function calls.
            // Use resolve_new() which checks construct signatures instead of call signatures.
            if is_super_call {
                call_checker::resolve_new_with_context(
                    self.ctx.types,
                    &self.ctx,
                    &env,
                    callee_type_for_call,
                    &arg_types,
                    force_bivariant_callbacks,
                )
            } else {
                call_checker::resolve_call_with_context(
                    self.ctx.types,
                    &self.ctx,
                    &env,
                    callee_type_for_call,
                    &arg_types,
                    force_bivariant_callbacks,
                    self.ctx.contextual_type,
                )
            }
        };

        let call_context = CallResultContext {
            callee_expr: call.expression,
            call_idx: idx,
            args,
            arg_types: &arg_types,
            callee_type: callee_type_for_call,
            is_super_call,
            is_optional_chain: nullish_cause.is_some(),
        };
        self.handle_call_result(result, call_context)
    }

    /// Handle the result of a call evaluation, emitting diagnostics for errors
    /// and applying this-substitution/mixin refinement for successes.
    fn handle_call_result(
        &mut self,
        result: tsz_solver::CallResult,
        context: CallResultContext<'_>,
    ) -> TypeId {
        use tsz_solver::CallResult;
        let CallResultContext {
            callee_expr,
            call_idx,
            args,
            arg_types,
            callee_type,
            is_super_call,
            is_optional_chain,
            ..
        } = context;
        match result {
            CallResult::Success(return_type) => {
                // super() calls always return void — they call the parent constructor
                // on `this`, they don't create a new instance.
                if is_super_call {
                    return TypeId::VOID;
                }
                let return_type =
                    self.apply_this_substitution_to_call_return(return_type, callee_expr);
                let return_type =
                    self.refine_mixin_call_return_type(callee_expr, arg_types, return_type);
                // Strip freshness from function return types. Object literals returned
                // from functions lose their freshness at the call boundary — the caller
                // should not see excess property checks for the callee's return value.
                let return_type = if !self.ctx.compiler_options.sound_mode {
                    tsz_solver::freshness::widen_freshness(self.ctx.types, return_type)
                } else {
                    return_type
                };
                if is_optional_chain {
                    self.ctx
                        .types
                        .factory()
                        .union(vec![return_type, TypeId::UNDEFINED])
                } else {
                    return_type
                }
            }
            CallResult::NotCallable { .. } => {
                // super() calls now use resolve_new() which checks construct signatures,
                // so NotCallable for super() means the base class has no constructor.
                // This is valid - classes can have implicit constructors.
                if is_super_call {
                    return TypeId::VOID;
                }
                if self.is_constructor_type(callee_type) {
                    self.error_class_constructor_without_new_at(callee_type, callee_expr);
                } else if self.is_get_accessor_call(callee_expr) {
                    self.error_get_accessor_not_callable_at(callee_expr);
                } else {
                    self.error_not_callable_at(callee_type, callee_expr);
                }
                TypeId::ERROR
            }
            CallResult::ArgumentCountMismatch {
                expected_min,
                expected_max,
                actual,
            } => {
                if actual < expected_min && expected_max.is_none() {
                    // Too few arguments with rest parameters (unbounded) - use TS2555
                    self.error_expected_at_least_arguments_at(expected_min, actual, call_idx);
                } else {
                    // Use TS2554 for exact count, range, or too many args
                    let expected = expected_max.unwrap_or(expected_min);
                    self.error_argument_count_mismatch_at(expected, actual, call_idx);
                }
                TypeId::ERROR
            }
            CallResult::OverloadArgumentCountMismatch {
                actual,
                expected_low,
                expected_high,
            } => {
                self.error_at_node(
                    call_idx,
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
                // Avoid cascading TS2345 when the argument type is already invalid or unknown.
                // In these cases, a more specific upstream diagnostic is usually the root cause.
                if actual == TypeId::ERROR
                    || actual == TypeId::UNKNOWN
                    || expected == TypeId::ERROR
                    || expected == TypeId::UNKNOWN
                {
                    return TypeId::ERROR;
                }

                let arg_idx = self.map_expanded_arg_index_to_original(args, index);
                if let Some(arg_idx) = arg_idx {
                    if !self.should_suppress_weak_key_arg_mismatch(callee_expr, args, index, actual)
                    {
                        // Try to elaborate: for object literal arguments, report TS2322
                        // on specific mismatched properties instead of TS2345 on the
                        // whole argument. This matches tsc behavior.
                        if !self.try_elaborate_object_literal_arg_error(arg_idx, expected) {
                            let _ =
                                self.check_argument_assignable_or_report(actual, expected, arg_idx);
                        }
                    }
                } else if !args.is_empty() {
                    let last_arg = args[args.len() - 1];
                    if !self.should_suppress_weak_key_arg_mismatch(callee_expr, args, index, actual)
                        && !self.try_elaborate_object_literal_arg_error(last_arg, expected)
                    {
                        let _ =
                            self.check_argument_assignable_or_report(actual, expected, last_arg);
                    }
                }
                TypeId::ERROR
            }
            CallResult::TypeParameterConstraintViolation {
                inferred_type,
                constraint_type,
                return_type,
            } => {
                // Report TS2322 for constraint violations from callback return type inference
                let _ = self.check_assignable_or_report_generic_at(
                    inferred_type,
                    constraint_type,
                    call_idx,
                    call_idx,
                );
                return_type
            }
            CallResult::NoOverloadMatch { failures, .. } => {
                // Compatibility fallback: built-in toLocaleString supports
                // (locales?, options?) in modern lib typings. Some merged
                // declaration paths can miss those overloads and incorrectly
                // surface TS2769; tsc accepts these calls.
                if self.is_tolocalestring_compat_call(callee_expr, args.len()) {
                    return TypeId::STRING;
                }
                if !self.should_suppress_weak_key_no_overload(callee_expr, args) {
                    self.error_no_overload_matches_at(call_idx, &failures);
                }
                TypeId::ERROR
            }
        }
    }

    fn is_tolocalestring_compat_call(&self, callee_expr: NodeIndex, arg_count: usize) -> bool {
        if arg_count > 2 {
            return false;
        }
        let Some(callee_node) = self.ctx.arena.get(callee_expr) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        ident.escaped_text == "toLocaleString"
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

        // TS2496: 'arguments' cannot be referenced in an arrow function in ES5
        if name == "arguments" {
            // Track that this function body uses `arguments` (for JS implicit rest params)
            self.ctx.js_body_uses_arguments = true;

            // TS2815: 'arguments' cannot be referenced in property initializers
            // or class static initialization blocks. Must check BEFORE regular
            // function body check because arrow functions are transparent.
            if self.is_arguments_in_class_initializer_or_static_block(idx) {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    idx,
                    diagnostic_messages::ARGUMENTS_CANNOT_BE_REFERENCED_IN_PROPERTY_INITIALIZERS_OR_CLASS_STATIC_INITIALI,
                    diagnostic_codes::ARGUMENTS_CANNOT_BE_REFERENCED_IN_PROPERTY_INITIALIZERS_OR_CLASS_STATIC_INITIALI,
                );
                return TypeId::ERROR;
            }

            use tsz_common::common::ScriptTarget;
            let is_es5_or_lower = matches!(
                self.ctx.compiler_options.target,
                ScriptTarget::ES3 | ScriptTarget::ES5
            );
            if is_es5_or_lower && self.is_arguments_in_arrow_function(idx) {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    idx,
                    diagnostic_messages::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ARROW_FUNCTION_IN_ES5_CONSIDER_U,
                    diagnostic_codes::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ARROW_FUNCTION_IN_ES5_CONSIDER_U,
                );
                // Return ERROR to prevent fallthrough to normal resolution which would emit TS2304
                return TypeId::ERROR;
            }
            if is_es5_or_lower && self.is_arguments_in_async_non_arrow_function(idx) {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    idx,
                    diagnostic_messages::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5,
                    diagnostic_codes::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5,
                );
                return TypeId::ERROR;
            }

            // Inside a regular (non-arrow) function body, `arguments` is the implicit
            // IArguments object, overriding any outer `arguments` declaration.
            // EXCEPT: if there's a LOCAL variable named "arguments" in the current function,
            // that shadows the built-in IArguments (e.g., `const arguments = this.arguments;`).
            if self.is_in_regular_function_body(idx) {
                // Check if there's a local "arguments" variable in the current function scope.
                // This handles shadowing: `const arguments = ...` takes precedence over IArguments.
                if let Some(sym_id) = self.resolve_identifier_symbol(idx) {
                    // Found a symbol named "arguments". Check if it's declared locally
                    // in the current function (not in an outer scope).
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                        && !symbol.declarations.is_empty()
                    {
                        let decl_node = symbol.declarations[0];
                        // Find the enclosing function for both the reference and the declaration
                        if let Some(current_fn) = self.find_enclosing_function(idx) {
                            if let Some(decl_fn) = self.find_enclosing_function(decl_node) {
                                // If the declaration is in the same function scope, it shadows IArguments
                                if current_fn == decl_fn {
                                    trace!(
                                        name = name,
                                        idx = ?idx,
                                        sym_id = ?sym_id,
                                        "get_type_of_identifier: local 'arguments' variable shadows built-in IArguments"
                                    );
                                    // Fall through to normal resolution below - use the local variable
                                } else {
                                    // Declaration is in an outer scope - use built-in IArguments
                                    let lib_binders = self.get_lib_binders();
                                    if let Some(iargs_sym) = self
                                        .ctx
                                        .binder
                                        .get_global_type_with_libs("IArguments", &lib_binders)
                                    {
                                        return self.type_reference_symbol_type(iargs_sym);
                                    }
                                    return TypeId::ANY;
                                }
                            } else {
                                // Declaration not in a function (global) - use built-in IArguments
                                let lib_binders = self.get_lib_binders();
                                if let Some(iargs_sym) = self
                                    .ctx
                                    .binder
                                    .get_global_type_with_libs("IArguments", &lib_binders)
                                {
                                    return self.type_reference_symbol_type(iargs_sym);
                                }
                                return TypeId::ANY;
                            }
                        }
                    }
                } else {
                    // No symbol found at all - use built-in IArguments
                    let lib_binders = self.get_lib_binders();
                    if let Some(sym_id) = self
                        .ctx
                        .binder
                        .get_global_type_with_libs("IArguments", &lib_binders)
                    {
                        return self.type_reference_symbol_type(sym_id);
                    }
                    return TypeId::ANY;
                }
            }
        }

        // === CRITICAL FIX: Check type parameter scope FIRST ===
        // Type parameters in generic functions/classes/type aliases should be resolved
        // before checking any other scope. This is a common source of TS2304 false positives.
        // Examples:
        //   function foo<T>(x: T) { return x; }  // T should be found in the function body
        //   class C<U> { method(u: U) {} }  // U should be found in the class body
        //   type Pair<T> = [T, T];  // T should be found in the type alias definition
        if let Some(type_id) = self.lookup_type_parameter(name) {
            // Before emitting TS2693, check if the binder also has a value symbol
            // with the same name. In cases like `function f<A>(A: A)`, the parameter
            // `A` shadows the type parameter `A` in value position.
            let has_value_shadow = self
                .resolve_identifier_symbol(idx)
                .and_then(|sym_id| {
                    self.ctx
                        .binder
                        .get_symbol(sym_id)
                        .map(|s| s.flags & tsz_binder::symbol_flags::VALUE != 0)
                })
                .unwrap_or(false);
            if !has_value_shadow {
                // TS2693: Type parameters cannot be used as values
                // Example: function f<T>() { return T; }  // Error: T is a type, not a value
                self.error_type_parameter_used_as_value(name, idx);
                return type_id;
            }
            // Fall through to binder resolution — the value symbol takes precedence
        }

        // Resolve via binder persistent scopes for stateless lookup.
        if let Some(sym_id) = self.resolve_identifier_symbol(idx) {
            // Reference tracking is handled by resolve_identifier_symbol wrapper
            trace!(
                name = name,
                idx = ?idx,
                sym_id = ?sym_id,
                "get_type_of_identifier: resolved symbol"
            );

            if self.is_type_only_import_equals_namespace_expr(idx) {
                self.error_namespace_used_as_value_at(name, idx);
                if let Some(sym_id) = self.resolve_identifier_symbol(idx)
                    && self.alias_resolves_to_type_only(sym_id)
                {
                    self.error_type_only_value_at(name, idx);
                }
                return TypeId::ERROR;
            }

            if self.alias_resolves_to_type_only(sym_id) {
                // Don't emit TS2693 in heritage clause context (e.g., `extends A`)
                if self.is_direct_heritage_type_reference(idx) {
                    return TypeId::ERROR;
                }
                // Don't emit TS2693 for export default/export = expressions
                if let Some(parent_ext) = self.ctx.arena.get_extended(idx)
                    && !parent_ext.parent.is_none()
                    && let Some(parent_node) = self.ctx.arena.get(parent_ext.parent)
                {
                    use tsz_parser::parser::syntax_kind_ext;
                    if parent_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                        || parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    {
                        return TypeId::ERROR;
                    }
                }
                self.error_type_only_value_at(name, idx);
                return TypeId::ERROR;
            }
            // Check symbol flags to detect type-only usage.
            // First try the main binder (fast path for local symbols).
            let local_symbol = self
                .get_cross_file_symbol(sym_id)
                .or_else(|| self.ctx.binder.get_symbol(sym_id));
            let flags = local_symbol.map_or(0, |s| s.flags);

            // TS2662: Bare identifier resolving to a static class member.
            // Static members must be accessed via `ClassName.member`, not as
            // bare identifiers.  The binder puts them in the class scope so
            // they resolve, but the checker must reject unqualified access.
            if (flags & tsz_binder::symbol_flags::STATIC) != 0
                && let Some(ref class_info) = self.ctx.enclosing_class.clone()
                && self.is_static_member(&class_info.member_nodes, name)
            {
                self.error_cannot_find_name_static_member_at(name, &class_info.name, idx);
                return TypeId::ERROR;
            }

            let has_type = (flags & tsz_binder::symbol_flags::TYPE) != 0;
            let has_value = (flags & tsz_binder::symbol_flags::VALUE) != 0;
            let is_type_alias = (flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0;
            trace!(
                name = name,
                flags = flags,
                has_type = has_type,
                has_value = has_value,
                is_interface = (flags & tsz_binder::symbol_flags::INTERFACE) != 0,
                "get_type_of_identifier: symbol flags"
            );
            let value_decl = local_symbol.map_or(NodeIndex::NONE, |s| s.value_declaration);
            let symbol_declarations = local_symbol
                .map(|s| s.declarations.clone())
                .unwrap_or_default();

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
                trace!(
                    name = name,
                    sym_id = ?sym_id,
                    is_type_alias = is_type_alias,
                    has_type = has_type,
                    has_value = has_value,
                    "get_type_of_identifier: TYPE-only symbol, checking for VALUE in libs"
                );
                // Cross-lib merging: interface/type may be in one lib while VALUE
                // declaration is in another. Resolve by declaration node first to
                // avoid SymbolId collisions across binders.
                let value_type = self.type_of_value_symbol_by_name(name);
                trace!(
                    name = name,
                    value_type = ?value_type,
                    "get_type_of_identifier: value_type from type_of_value_symbol_by_name"
                );
                if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                    trace!(
                        name = name,
                        value_type = ?value_type,
                        "get_type_of_identifier: using cross-lib VALUE type"
                    );
                    return self.check_flow_usage(idx, value_type, sym_id);
                }

                // Don't emit TS2693 in heritage clause context — but ONLY when the
                // identifier is the direct expression of an ExpressionWithTypeArguments
                // (e.g., `extends A`). If the identifier is nested deeper, such as
                // a function argument within the heritage expression (e.g.,
                // `extends factory(A)`), TS2693 should still fire.
                if self.is_direct_heritage_type_reference(idx) {
                    return TypeId::ERROR;
                }

                // Don't emit TS2693 for export default/export = expressions.
                // `export default InterfaceName` and `export = InterfaceName`
                // are valid TypeScript — they export the type binding.
                if let Some(parent_ext) = self.ctx.arena.get_extended(idx)
                    && !parent_ext.parent.is_none()
                    && let Some(parent_node) = self.ctx.arena.get(parent_ext.parent)
                {
                    use tsz_parser::parser::syntax_kind_ext;
                    if parent_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                        || parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    {
                        return TypeId::ERROR;
                    }
                }

                self.error_type_only_value_at(name, idx);
                return TypeId::ERROR;
            }

            // ES2015+ globals (Symbol, Promise, Map, Set, etc.) used as values
            // require target >= ES2015. When target < ES2015, emit TS2585 even
            // if the value is transitively available through DOM typings.
            {
                use tsz_binder::lib_loader;
                if lib_loader::is_es2015_plus_type(name)
                    && !(name == "Promise" && self.ctx.has_promise_constructor_in_scope())
                    && !self.ctx.compiler_options.target.supports_es2015()
                {
                    self.error_type_only_value_at(name, idx);
                    return TypeId::ERROR;
                }
            }

            // If the symbol wasn't found in the main binder (flags==0), it came
            // from a lib or cross-file binder.  For known ES2015+ global type
            // names (Symbol, Promise, Map, Set, etc.) we need to check whether
            // the lib binder's symbol is type-only.  Only do this for the known
            // set to avoid cross-binder ID collisions causing false TS2693 on
            // arbitrary user symbols from other files.
            if flags == 0 {
                use tsz_binder::lib_loader;
                if lib_loader::is_es2015_plus_type(name) {
                    let lib_binders = self.get_lib_binders();
                    let lib_flags = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(sym_id, &lib_binders)
                        .map_or(0, |s| s.flags);
                    let lib_has_type = (lib_flags & tsz_binder::symbol_flags::TYPE) != 0;
                    let lib_has_value = (lib_flags & tsz_binder::symbol_flags::VALUE) != 0;
                    if lib_has_type && !lib_has_value {
                        // Cross-lib merging: VALUE may be in a different lib binder.
                        // Resolve by declaration node first to avoid SymbolId collisions.
                        let value_type = self.type_of_value_symbol_by_name(name);
                        if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                            return self.check_flow_usage(idx, value_type, sym_id);
                        }
                        self.error_type_only_value_at(name, idx);
                        return TypeId::ERROR;
                    }
                }
            }

            // Merged interface+value symbols (e.g. `interface Promise<T>` +
            // `declare var Promise: PromiseConstructor`) must use the VALUE side
            // in value position. Falling back to interface type here causes
            // false TS2339/TS2351 on `Promise.resolve` / `new Promise(...)`.
            //
            // Merged interface+value symbols (e.g. Symbol interface + declare var Symbol: SymbolConstructor)
            // must use the VALUE side in value position. The *Constructor lookup below
            // handles finding the right type (SymbolConstructor, PromiseConstructor, etc.)
            let is_merged_interface_value =
                has_type && has_value && (flags & tsz_binder::symbol_flags::INTERFACE) != 0;
            // TS2585: For ES2015+ global types (Symbol, Promise, Map, Set, etc.)
            // used as values in an ES5/ES3 target, the VALUE binding came from
            // a transitively loaded ES2015+ lib (via DOM's reference directives).
            // The target doesn't natively support these as values, so emit TS2585.
            if is_merged_interface_value {
                use tsz_binder::lib_loader;
                use tsz_common::common::ScriptTarget;
                let is_es5_or_lower = matches!(
                    self.ctx.compiler_options.target,
                    ScriptTarget::ES3 | ScriptTarget::ES5
                );
                if is_es5_or_lower && lib_loader::is_es2015_plus_type(name) {
                    if name == "Promise" && self.ctx.has_promise_constructor_in_scope() {
                        // Promise is explicitly available via es2015.promise lib.
                    } else {
                        self.error_type_only_value_at(name, idx);
                        return TypeId::ERROR;
                    }
                }
            }
            if is_merged_interface_value {
                trace!(
                    name = name,
                    sym_id = ?sym_id,
                    value_decl = ?value_decl,
                    "get_type_of_identifier: merged interface+value path"
                );
                // For ES2015+ types (Symbol, Promise, Map, Set, etc.) used as values
                // in targets below ES2015, tsc emits TS2585 even though the lib files
                // DO include the value declaration (via dom.d.ts → es2015.d.ts chain).
                // tsc checks the target version directly, not lib availability.
                {
                    use tsz_binder::lib_loader;
                    if lib_loader::is_es2015_plus_type(name)
                        && !(name == "Promise" && self.ctx.has_promise_constructor_in_scope())
                        && self.ctx.compiler_options.target.is_es5()
                    {
                        self.error_type_only_value_at(name, idx);
                        return TypeId::ERROR;
                    }
                }
                // Prefer value-declaration resolution for merged symbols so we pick
                // the constructor-side type (e.g. Promise -> PromiseConstructor).
                let mut value_type = self.type_of_value_declaration_for_symbol(sym_id, value_decl);
                if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
                    for &decl_idx in &symbol_declarations {
                        let candidate = self.type_of_value_declaration_for_symbol(sym_id, decl_idx);
                        if candidate != TypeId::UNKNOWN && candidate != TypeId::ERROR {
                            value_type = candidate;
                            break;
                        }
                    }
                }
                if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
                    value_type = self.type_of_value_symbol_by_name(name);
                }
                if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
                    let direct_type = self.get_type_of_symbol(sym_id);
                    trace!(
                        name = name,
                        direct_type = ?direct_type,
                        "get_type_of_identifier: direct type from get_type_of_symbol"
                    );
                    if direct_type != TypeId::UNKNOWN && direct_type != TypeId::ERROR {
                        value_type = direct_type;
                    }
                }
                trace!(
                    name = name,
                    value_type = ?value_type,
                    "get_type_of_identifier: value_type after value-decl resolution"
                );
                // Lib globals often model value-side constructors through a sibling
                // `*Constructor` interface (Promise -> PromiseConstructor).
                // Prefer that when available to avoid falling back to the instance interface.
                trace!(
                    name = name,
                    value_type = ?value_type,
                    "get_type_of_identifier: value_type before *Constructor lookup"
                );
                let constructor_name = format!("{name}Constructor");
                trace!(
                    name = name,
                    constructor_name = %constructor_name,
                    "get_type_of_identifier: looking for *Constructor symbol"
                );
                // BUG FIX: Use find_value_symbol_in_libs instead of resolve_global_value_symbol
                // to ensure we get the correct VALUE symbol, not a type-only or wrong symbol.
                // resolve_global_value_symbol can return the wrong symbol when there are
                // name collisions in file_locals (e.g., SymbolConstructor from ES2015 vs DOM types).
                if let Some(constructor_sym_id) = self.find_value_symbol_in_libs(&constructor_name)
                {
                    trace!(
                        name = name,
                        constructor_sym_id = ?constructor_sym_id,
                        "get_type_of_identifier: found *Constructor symbol"
                    );
                    let constructor_type = self.get_type_of_symbol(constructor_sym_id);
                    trace!(
                        name = name,
                        constructor_type = ?constructor_type,
                        "get_type_of_identifier: *Constructor type"
                    );
                    if constructor_type != TypeId::UNKNOWN && constructor_type != TypeId::ERROR {
                        value_type = constructor_type;
                    }
                } else {
                    trace!(
                        name = name,
                        constructor_name = %constructor_name,
                        "get_type_of_identifier: find_value_symbol_in_libs returned None, trying resolve_lib_type_by_name"
                    );
                    if let Some(constructor_type) = self.resolve_lib_type_by_name(&constructor_name)
                        && constructor_type != TypeId::UNKNOWN
                        && constructor_type != TypeId::ERROR
                    {
                        trace!(
                            name = name,
                            constructor_type = ?constructor_type,
                            current_value_type = ?value_type,
                            "get_type_of_identifier: found *Constructor TYPE"
                        );
                        // BUG FIX: Only use constructor_type if we don't already have a valid type.
                        // For "Symbol", value_type=TypeId(8286) is correct (SymbolConstructor),
                        // but resolve_lib_type_by_name returns TypeId(8282) (DecoratorMetadata).
                        // Don't let the wrong *Constructor type overwrite the correct direct type.
                        if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
                            value_type = constructor_type;
                        }
                    } else {
                        trace!(
                            name = name,
                            constructor_name = %constructor_name,
                            "get_type_of_identifier: resolve_lib_type_by_name returned None/UNKNOWN/ERROR"
                        );
                    }
                }
                // For `declare var X: X` pattern (self-referential type annotation),
                // the type resolved through type_of_value_declaration may be incomplete
                // because the interface is resolved in a child checker with only one
                // lib arena. Use resolve_lib_type_by_name to get the complete interface
                // type merged from all lib files.
                if !self.ctx.lib_contexts.is_empty()
                    && self.is_self_referential_var_type(sym_id, value_decl, name)
                    && let Some(lib_type) = self.resolve_lib_type_by_name(name)
                    && lib_type != TypeId::UNKNOWN
                    && lib_type != TypeId::ERROR
                {
                    value_type = lib_type;
                }
                // Final fallback: if value_type is still a Lazy type (e.g., due to
                // check_variable_declaration overwriting the symbol_types cache with the
                // Lazy annotation type for `declare var X: X` patterns, and DefId
                // collisions corrupting the type_env), force recompute the symbol type.
                if query::lazy_def_id(self.ctx.types, value_type).is_some() {
                    self.ctx.symbol_types.remove(&sym_id);
                    let recomputed = self.get_type_of_symbol(sym_id);
                    if recomputed != value_type
                        && recomputed != TypeId::UNKNOWN
                        && recomputed != TypeId::ERROR
                    {
                        value_type = recomputed;
                    }
                }
                if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                    return self.check_flow_usage(idx, value_type, sym_id);
                }
            }

            let declared_type = self.get_type_of_symbol(sym_id);
            // Check for TDZ violations (variable used before declaration in source order)
            if self.check_tdz_violation(sym_id, idx, name) {
                return TypeId::ERROR;
            }
            // Use check_flow_usage to integrate both DAA and type narrowing
            // This handles TS2454 errors and applies flow-based narrowing
            let flow_type = self.check_flow_usage(idx, declared_type, sym_id);
            trace!(
                ?flow_type,
                ?declared_type,
                "After check_flow_usage in get_type_of_identifier"
            );

            // FIX: Preserve readonly and other type modifiers from declared_type.
            // When declared_type has modifiers like ReadonlyType, we must preserve them
            // even if flow analysis infers a different type from the initializer.
            // IMPORTANT: Only apply this fix when there's NO contextual type to avoid interfering
            // with variance checking and assignability analysis.
            //
            // CRITICAL: Array element narrowing produces a genuinely different type that we must use.
            // Check if flow_type is a meaningful narrowing (not ANY/ERROR and different from declared_type).
            // If so, use it. Otherwise, preserve declared_type if it has special modifiers.
            let result_type = if self.ctx.contextual_type.is_none()
                && declared_type != TypeId::ANY
                && declared_type != TypeId::ERROR
            {
                // Check if we have genuine narrowing (different type that's not ANY/ERROR)
                let has_narrowing = flow_type != declared_type
                    && flow_type != TypeId::ANY
                    && flow_type != TypeId::ERROR;

                if has_narrowing {
                    // Genuine narrowing (e.g., array element narrowing) - use narrowed type
                    flow_type
                } else {
                    // No narrowing or error - check if we should preserve declared_type
                    let has_index_sig = {
                        use tsz_solver::{IndexKind, IndexSignatureResolver};
                        let resolver = IndexSignatureResolver::new(self.ctx.types);
                        resolver.has_index_signature(declared_type, IndexKind::String)
                            || resolver.has_index_signature(declared_type, IndexKind::Number)
                    };
                    if query::is_readonly_type(self.ctx.types, declared_type) || has_index_sig {
                        declared_type
                    } else {
                        flow_type
                    }
                }
            } else {
                flow_type
            };

            // FIX: For mutable variables (let/var), always use declared_type instead of flow_type
            // to preserve literal type widening. Flow analysis may narrow back to literal types
            // from the initializer, but we need to keep the widened type (string, number, etc.)
            // const variables preserve their literal types through flow analysis.
            //
            // CRITICAL EXCEPTION: If flow_type is different from declared_type and not ERROR,
            // we should use flow_type. This allows discriminant narrowing to work for mutable
            // variables while preserving literal type widening in most cases.
            let is_const = self.is_const_variable_declaration(value_decl);
            let result_type = if !is_const {
                // Mutable variable (let/var)
                // If declared type has index signatures (either ObjectWithIndex or a resolved
                // type with index signatures like from a type alias), always preserve it.
                // This prevents false-positive TS2339 errors when accessing properties via
                // index signatures.
                let has_index_sig = {
                    use tsz_solver::{IndexKind, IndexSignatureResolver};
                    let resolver = IndexSignatureResolver::new(self.ctx.types);
                    resolver.has_index_signature(declared_type, IndexKind::String)
                        || resolver.has_index_signature(declared_type, IndexKind::Number)
                };
                if has_index_sig && (flow_type == declared_type || flow_type == TypeId::ERROR) {
                    declared_type
                } else if flow_type != declared_type && flow_type != TypeId::ERROR {
                    // Flow narrowed the type - but check if this is just the initializer
                    // literal being returned. For mutable variables without annotations,
                    // the declared type is already widened (e.g., STRING for "hi"),
                    // so if the flow type widens to the declared type, use declared_type.
                    let widened_flow = tsz_solver::widening::widen_type(self.ctx.types, flow_type);
                    if widened_flow == declared_type {
                        // Flow type is just the initializer literal - use widened declared type
                        declared_type
                    } else {
                        // Genuine narrowing (e.g., discriminant narrowing) - use narrowed type
                        flow_type
                    }
                } else {
                    // No narrowing or error - use declared type to preserve widening
                    declared_type
                }
            } else {
                // Const variable - use flow type (preserves literal type)
                result_type
            };

            // FIX: Flow analysis may return the original fresh type from the initializer expression.
            // For variable references, we must respect the widening that was applied during variable
            // declaration. If the symbol was widened (non-fresh), the flow result should also be widened.
            // This prevents "Zombie Freshness" where CFA bypasses the widened symbol type.
            if !self.ctx.compiler_options.sound_mode {
                use tsz_solver::freshness::{is_fresh_object_type, widen_freshness};
                if is_fresh_object_type(self.ctx.types, result_type) {
                    return widen_freshness(self.ctx.types, result_type);
                }
            }

            return result_type;
        }

        self.resolve_unresolved_identifier(idx, name)
    }

    /// Resolve an identifier that was NOT found in the binder's scope chain.
    ///
    /// Handles intrinsics (`undefined`, `NaN`, `Symbol`), known globals
    /// (`console`, `Math`, `Array`, etc.), static member suggestions, and
    /// "cannot find name" error reporting.
    fn resolve_unresolved_identifier(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        match name {
            "undefined" => TypeId::UNDEFINED,
            "NaN" | "Infinity" => TypeId::NUMBER,
            "Symbol" => self.resolve_symbol_constructor(idx, name),
            _ if self.is_known_global_value_name(name) => self.resolve_known_global(idx, name),
            _ => self.resolve_truly_unknown_identifier(idx, name),
        }
    }

    /// Resolve the `Symbol` constructor. Emits TS2583/TS2585 if Symbol is
    /// unavailable or type-only (ES5 target).
    fn resolve_symbol_constructor(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        if !self.ctx.has_symbol_in_lib() {
            self.error_cannot_find_name_change_lib(name, idx);
            return TypeId::ERROR;
        }
        // When target < ES2015, Symbol is type-only even if transitively
        // loaded through DOM typings. TSC emits TS2585 in this case.
        if !self.ctx.compiler_options.target.supports_es2015() {
            self.error_type_only_value_at(name, idx);
            return TypeId::ERROR;
        }
        let value_type = self.type_of_value_symbol_by_name(name);
        if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
            return value_type;
        }
        self.error_type_only_value_at(name, idx);
        TypeId::ERROR
    }

    /// Resolve a known global value name (e.g. `console`, `Math`, `Array`).
    /// Tries binder `file_locals` and lib binders, then falls back to error reporting.
    fn resolve_known_global(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        if self.is_nodejs_runtime_global(name) {
            // In CommonJS module mode, these globals are implicitly available
            if self.ctx.compiler_options.module.is_commonjs() {
                return TypeId::ANY;
            }
            // Otherwise, emit TS2580 suggesting @types/node installation
            self.error_cannot_find_name_install_node_types(name, idx);
            return TypeId::ERROR;
        }

        let lib_binders = self.get_lib_binders();
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            return self.get_type_of_symbol(sym_id);
        }
        if let Some(sym_id) = self
            .ctx
            .binder
            .get_global_type_with_libs(name, &lib_binders)
        {
            return self.get_type_of_symbol(sym_id);
        }

        self.emit_global_not_found_error(idx, name)
    }

    /// Emit an appropriate error when a known global is not found.
    fn emit_global_not_found_error(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        use crate::error_reporter::is_known_dom_global;
        use tsz_binder::lib_loader;

        if !self.ctx.has_lib_loaded() {
            if lib_loader::is_es2015_plus_type(name) {
                self.error_cannot_find_name_change_lib(name, idx);
            } else {
                self.error_cannot_find_name_at(name, idx);
            }
            return TypeId::ERROR;
        }

        if is_known_dom_global(name) {
            self.error_cannot_find_name_at(name, idx);
            return TypeId::ERROR;
        }
        if lib_loader::is_es2015_plus_type(name) {
            self.error_cannot_find_global_type(name, idx);
            return TypeId::ERROR;
        }

        let first_char = name.chars().next().unwrap_or('a');
        if first_char.is_uppercase() || self.is_known_global_value_name(name) {
            return TypeId::ANY;
        }

        // TS2693: Primitive type keywords used as values
        // TypeScript primitive type keywords (number, string, boolean, etc.) are language keywords
        // for types, not identifiers. When used in value position, emit TS2693.
        // Exception: in import equals module references (e.g., `import r = undefined`),
        // TS2503 is already emitted by check_namespace_import — don't also emit TS2693.
        if matches!(
            name,
            "number"
                | "string"
                | "boolean"
                | "symbol"
                | "void"
                | "undefined"
                | "null"
                | "any"
                | "unknown"
                | "never"
                | "object"
                | "bigint"
        ) {
            self.error_type_only_value_at(name, idx);
            return TypeId::ERROR;
        }

        if self.ctx.is_known_global_type(name) {
            self.error_cannot_find_global_type(name, idx);
        } else {
            self.error_cannot_find_name_at(name, idx);
        }
        TypeId::ERROR
    }

    /// Handle a truly unresolved identifier — not a type parameter, not in the
    /// binder, not a known global. Emits TS2304, TS2524, TS2662 as appropriate.
    fn resolve_truly_unknown_identifier(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        // Check static member suggestion (error 2662)
        if let Some(ref class_info) = self.ctx.enclosing_class.clone()
            && self.is_static_member(&class_info.member_nodes, name)
        {
            self.error_cannot_find_name_static_member_at(name, &class_info.name, idx);
            return TypeId::ERROR;
        }
        // TS2524: 'await' in default parameter
        if name == "await" && self.is_in_default_parameter(idx) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                idx,
                diagnostic_messages::AWAIT_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
                diagnostic_codes::AWAIT_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
            );
            return TypeId::ERROR;
        }
        // TS2523: 'yield' in default parameter
        if name == "yield" && self.is_in_default_parameter(idx) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                idx,
                diagnostic_messages::YIELD_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
                diagnostic_codes::YIELD_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
            );
            return TypeId::ERROR;
        }
        // Suppress TS2304 for unresolved imports (TS2307 was already emitted)
        if self.is_unresolved_import_symbol(idx) {
            return TypeId::ANY;
        }
        // Check known globals that might be missing
        if self.is_known_global_value_name(name) {
            return self.emit_global_not_found_error(idx, name);
        }
        // Suppress in single-file mode to prevent cascading false positives
        if !self.ctx.report_unresolved_imports {
            return TypeId::ANY;
        }
        self.error_cannot_find_name_at(name, idx);
        TypeId::ERROR
    }

    /// Check for TDZ violations: variable used before its declaration in a
    /// static block, computed property, or heritage clause; or class/enum
    /// used before its declaration anywhere in the same scope.
    /// Emits TS2448 (variable), TS2449 (class), or TS2450 (enum) and returns
    /// `true` if a violation is found.
    pub(crate) fn check_tdz_violation(
        &mut self,
        sym_id: SymbolId,
        idx: NodeIndex,
        name: &str,
    ) -> bool {
        // Skip TDZ checks in cross-arena delegation context.
        // TDZ compares node positions, which are meaningless when the usage node
        // and declaration node come from different files' arenas.
        if Self::is_in_cross_arena_delegation() {
            return false;
        }
        let is_tdz = self.is_variable_used_before_declaration_in_static_block(sym_id, idx)
            || self.is_variable_used_before_declaration_in_computed_property(sym_id, idx)
            || self.is_variable_used_before_declaration_in_heritage_clause(sym_id, idx)
            || self.is_class_or_enum_used_before_declaration(sym_id, idx);
        if is_tdz {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            // Emit the correct diagnostic based on symbol kind:
            // TS2449 for classes, TS2450 for enums, TS2448 for variables
            let (msg_template, code) = if let Some(sym) = self.ctx.binder.symbols.get(sym_id) {
                if sym.flags & tsz_binder::symbol_flags::CLASS != 0 {
                    (
                        diagnostic_messages::CLASS_USED_BEFORE_ITS_DECLARATION,
                        diagnostic_codes::CLASS_USED_BEFORE_ITS_DECLARATION,
                    )
                } else if sym.flags & tsz_binder::symbol_flags::REGULAR_ENUM != 0 {
                    (
                        diagnostic_messages::ENUM_USED_BEFORE_ITS_DECLARATION,
                        diagnostic_codes::ENUM_USED_BEFORE_ITS_DECLARATION,
                    )
                } else {
                    (
                        diagnostic_messages::BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION,
                        diagnostic_codes::BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION,
                    )
                }
            } else {
                (
                    diagnostic_messages::BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION,
                    diagnostic_codes::BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION,
                )
            };
            let message = format_message(msg_template, &[name]);
            self.error_at_node(idx, &message, code);

            // For block-scoped variable TDZ reads, TypeScript also reports TS2454
            // ("used before being assigned") in strict-null mode.
            if self.ctx.strict_null_checks()
                && self.ctx.binder.symbols.get(sym_id).is_some_and(|sym| {
                    sym.flags & tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE != 0
                })
                && let Some(usage_node) = self.ctx.arena.get(idx)
            {
                let key = (usage_node.pos, sym_id);
                if self.ctx.emitted_ts2454_errors.insert(key) {
                    self.error_variable_used_before_assigned_at(name, idx);
                }
            }

            // TS2729 companion for static property initializers:
            // in `X.Y`, when `X` is in TDZ, tsc also reports that `Y` is used
            // before initialization at the property name site.
            if self.is_in_static_property_initializer_ast_context(idx)
                && let Some(ext) = self.ctx.arena.get_extended(idx)
                && !ext.parent.is_none()
                && let Some(parent) = self.ctx.arena.get(ext.parent)
                && parent.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.ctx.arena.get_access_expr(parent)
                && access.expression == idx
                && let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
                && let Some(name_ident) = self.ctx.arena.get_identifier(name_node)
            {
                self.error_at_node(
                    access.name_or_argument,
                    &format!(
                        "Property '{}' is used before its initialization.",
                        name_ident.escaped_text
                    ),
                    diagnostic_codes::PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION,
                );
            }

            // TS2538: When a variable is used before declaration in a computed property,
            // it has implicit type 'any', which cannot be used as an index type.
            // Emit this additional error for computed property contexts.
            let is_in_computed_property =
                self.is_variable_used_before_declaration_in_computed_property(sym_id, idx);
            if is_in_computed_property {
                let message = format_message(
                    diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                    &["any"],
                );
                self.error_at_node(
                    idx,
                    &message,
                    diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                );
            }
        }
        is_tdz
    }

    /// Returns true when `usage_idx` is lexically inside a static class property
    /// initializer (`static x = ...`).
    fn is_in_static_property_initializer_ast_context(&self, usage_idx: NodeIndex) -> bool {
        let mut current = usage_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            if ext.parent.is_none() {
                break;
            }
            let parent = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };
            if parent_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                if let Some(prop) = self.ctx.arena.get_property_decl(parent_node) {
                    return !prop.initializer.is_none()
                        && self.has_static_modifier(&prop.modifiers);
                }
                return false;
            }
            current = parent;
        }
        false
    }

    /// Resolve the value-side type from a symbol's value declaration node.
    ///
    /// This is used for merged interface+value globals where value position must
    /// use the constructor/variable declaration type, not the interface type.
    /// Check if a value declaration has a self-referential type annotation.
    /// For example, `declare var Math: Math` has type annotation "Math"
    /// which matches the symbol name "Math". This pattern is common for
    /// lib globals that follow the `declare var X: X` pattern.
    fn is_self_referential_var_type(
        &self,
        _sym_id: SymbolId,
        value_decl: NodeIndex,
        name: &str,
    ) -> bool {
        // Try to find the value declaration in the current arena first
        if let Some(node) = self.ctx.arena.get(value_decl)
            && let Some(var_decl) = self.ctx.arena.get_variable_declaration(node)
            && !var_decl.type_annotation.is_none()
            && let Some(type_node) = self.ctx.arena.get(var_decl.type_annotation)
            && let Some(type_ref) = self.ctx.arena.get_type_ref(type_node)
            && let Some(name_node) = self.ctx.arena.get(type_ref.type_name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            return ident.escaped_text == name;
        }

        // For declarations in other arenas (lib files), check via declaration_arenas
        if let Some(decl_arena) = self
            .ctx
            .binder
            .declaration_arenas
            .get(&(_sym_id, value_decl))
            && let Some(node) = decl_arena.get(value_decl)
            && let Some(var_decl) = decl_arena.get_variable_declaration(node)
            && !var_decl.type_annotation.is_none()
            && let Some(type_node) = decl_arena.get(var_decl.type_annotation)
            && let Some(type_ref) = decl_arena.get_type_ref(type_node)
            && let Some(name_node) = decl_arena.get(type_ref.type_name)
            && let Some(ident) = decl_arena.get_identifier(name_node)
        {
            return ident.escaped_text == name;
        }

        false
    }

    fn type_of_value_declaration(&mut self, decl_idx: NodeIndex) -> TypeId {
        if decl_idx.is_none() {
            return TypeId::UNKNOWN;
        }

        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return TypeId::UNKNOWN;
        };

        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
            if !var_decl.type_annotation.is_none() {
                let annotated = self.get_type_from_type_node(var_decl.type_annotation);
                return self.resolve_ref_type(annotated);
            }
            if !var_decl.initializer.is_none() {
                return self.get_type_of_node(var_decl.initializer);
            }
            return TypeId::ANY;
        }

        if self.ctx.arena.get_function(node).is_some() {
            return self.get_type_of_function(decl_idx);
        }

        if let Some(class_data) = self.ctx.arena.get_class(node) {
            return self.get_class_constructor_type(decl_idx, class_data);
        }

        TypeId::UNKNOWN
    }

    /// Resolve a value declaration type, delegating to the declaration's arena
    /// when the node does not belong to the current checker arena.
    fn type_of_value_declaration_for_symbol(
        &mut self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
    ) -> TypeId {
        if decl_idx.is_none() {
            return TypeId::UNKNOWN;
        }

        // Check declaration_arenas FIRST for the precise arena mapping.
        // This is critical for lib symbols where the same NodeIndex can exist
        // in both the lib arena and the main arena (cross-arena collision).
        // If we checked arena.get() first, we'd read a wrong node from the
        // main arena instead of the correct node from the lib arena.
        let decl_arena =
            if let Some(da) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                if std::ptr::eq(da.as_ref(), self.ctx.arena) {
                    return self.type_of_value_declaration(decl_idx);
                }
                Some(std::sync::Arc::clone(da))
            } else if self.ctx.arena.get(decl_idx).is_some() {
                // Node exists in current arena but no declaration_arenas entry.
                // For non-lib symbols: this is the correct arena — use fast path.
                // For lib symbols: this may be a cross-arena collision — use symbol_arenas.
                if !self.ctx.binder.symbol_arenas.contains_key(&sym_id) {
                    return self.type_of_value_declaration(decl_idx);
                }
                self.ctx.binder.symbol_arenas.get(&sym_id).cloned()
            } else {
                None
            };
        let Some(decl_arena) = decl_arena else {
            return TypeId::UNKNOWN;
        };
        if std::ptr::eq(decl_arena.as_ref(), self.ctx.arena) {
            return self.type_of_value_declaration(decl_idx);
        }

        // For lib declarations, check if the type annotation is a simple type reference
        // to a global lib type. If so, use resolve_lib_type_by_name directly instead of
        // creating a child checker. The child checker inherits the parent's merged binder,
        // which can have wrong symbol IDs for lib types, causing incorrect type resolution.
        if let Some(node) = decl_arena.get(decl_idx)
            && let Some(var_decl) = decl_arena.get_variable_declaration(node)
            && !var_decl.type_annotation.is_none()
        {
            // Try to extract the type name from a simple type reference
            if let Some(type_annotation_node) = decl_arena.get(var_decl.type_annotation)
                && let Some(type_ref) = decl_arena.get_type_ref(type_annotation_node)
            {
                // Check if this is a simple identifier (not qualified name)
                if let Some(type_name_node) = decl_arena.get(type_ref.type_name)
                    && let Some(ident) = decl_arena.get_identifier(type_name_node)
                {
                    let type_name = ident.escaped_text.as_str();
                    // Use resolve_lib_type_by_name for global lib types
                    if let Some(lib_type) = self.resolve_lib_type_by_name(type_name)
                        && lib_type != TypeId::UNKNOWN
                        && lib_type != TypeId::ERROR
                    {
                        return self.resolve_ref_type(lib_type);
                    }
                }
            }
        }

        // Guard against deep cross-arena recursion (shared with all delegation points)
        if !Self::enter_cross_arena_delegation() {
            return TypeId::UNKNOWN;
        }

        let mut checker = Box::new(CheckerState::with_parent_cache(
            decl_arena.as_ref(),
            self.ctx.binder,
            self.ctx.types,
            self.ctx.file_name.clone(),
            self.ctx.compiler_options.clone(),
            self,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.symbol_resolution_set = self.ctx.symbol_resolution_set.clone();
        checker.ctx.symbol_resolution_stack = self.ctx.symbol_resolution_stack.clone();
        checker
            .ctx
            .symbol_resolution_depth
            .set(self.ctx.symbol_resolution_depth.get());
        let result = checker.type_of_value_declaration(decl_idx);

        // DO NOT merge child's symbol_types back. See delegate_cross_arena_symbol_resolution
        // for the full explanation: node_symbols collisions across arenas cause cache poisoning.

        Self::leave_cross_arena_delegation();
        result
    }

    /// Resolve a value-side type by global name, preferring value declarations.
    ///
    /// This avoids incorrect type resolution when symbol IDs collide across
    /// binders (current file vs. lib files).
    fn type_of_value_symbol_by_name(&mut self, name: &str) -> TypeId {
        if let Some((sym_id, value_decl)) = self.find_value_declaration_in_libs(name) {
            let value_type = self.type_of_value_declaration_for_symbol(sym_id, value_decl);
            if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                return value_type;
            }
        }

        if let Some(value_sym_id) = self.find_value_symbol_in_libs(name) {
            let value_type = self.get_type_of_symbol(value_sym_id);
            if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                return value_type;
            }
        }

        TypeId::UNKNOWN
    }

    /// If `type_id` is an object type with a synthetic `"new"` member, return that member type.
    /// This supports constructor-like interfaces that lower construct signatures as properties.
    fn constructor_type_from_new_property(&self, type_id: TypeId) -> Option<TypeId> {
        let shape_id = query::object_shape_id(self.ctx.types, type_id)?;

        let new_atom = self.ctx.types.intern_string("new");
        let shape = self.ctx.types.object_shape(shape_id);
        shape
            .properties
            .iter()
            .find(|prop| prop.name == new_atom)
            .map(|prop| prop.type_id)
    }

    /// Extract a partial object type from non-sensitive properties of an object literal.
    ///
    /// Used during Round 1 of two-pass generic inference to get type information
    /// from concrete properties (like `state: 100`) while skipping context-sensitive
    /// properties (like `actions: { foo: s => s }`).
    ///
    /// This lets inference learn e.g. `State = number` from `state: 100` even when
    /// the overall object literal is context-sensitive.
    fn extract_non_sensitive_object_type(&mut self, idx: NodeIndex) -> Option<TypeId> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let obj = self.ctx.arena.get_literal_expr(node)?;

        let mut properties = Vec::new();

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Property assignment: { x: value }
            if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                // Skip sensitive property initializers (lambdas, nested sensitive objects)
                if is_contextually_sensitive(self, prop.initializer) {
                    continue;
                }
                if let Some(name) = self.get_property_name(prop.name) {
                    // Compute type without contextual type
                    let prev_context = self.ctx.contextual_type;
                    self.ctx.contextual_type = None;
                    let value_type = self.get_type_of_node(prop.initializer);
                    self.ctx.contextual_type = prev_context;

                    let name_atom = self.ctx.types.intern_string(&name);
                    properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
                }
            }
            // Shorthand property: { x }
            else if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                && let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node)
                && let Some(name_node) = self.ctx.arena.get(shorthand.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                let name = ident.escaped_text.clone();
                let value_type = self.get_type_of_node(shorthand.name);
                let name_atom = self.ctx.types.intern_string(&name);
                properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
            }
            // Methods and accessors are always context-sensitive — skip them
        }

        if properties.is_empty() {
            return None;
        }

        Some(self.ctx.types.factory().object(properties))
    }
}
