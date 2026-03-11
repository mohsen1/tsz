//! Complex type computation: new expressions, constructability, union/keyof types,
//! and class type helpers.

use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::type_computation::complex as query;
use crate::state::CheckerState;
use tracing::trace;
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
        // Functions are sensitive ONLY if they have at least one parameter without a type annotation
        k if k == syntax_kind_ext::ARROW_FUNCTION || k == syntax_kind_ext::FUNCTION_EXPRESSION => {
            if let Some(func) = state.ctx.arena.get_function(node) {
                let has_unannotated_params = func.parameters.nodes.iter().any(|&param_idx| {
                    if let Some(param_node) = state.ctx.arena.get(param_idx)
                        && let Some(param) = state.ctx.arena.get_parameter(param_node)
                    {
                        return param.type_annotation.is_none();
                    }
                    false
                });

                has_unannotated_params
                    || (func.parameters.nodes.is_empty()
                        && func.type_annotation.is_none()
                        && function_body_needs_contextual_return_type(state, func.body))
            } else {
                false
            }
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

        // Nested calls/constructs: sensitive if any of their own arguments are sensitive.
        // This lets outer generic calls defer wrapper expressions like
        // `handler(type, state => state)` to Round 2, so the outer call can first
        // infer type arguments from non-contextual inputs and then provide a concrete
        // contextual return type to the inner generic call.
        k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => state
            .ctx
            .arena
            .get_call_expr(node)
            .and_then(|call| call.arguments.as_ref())
            .is_some_and(|args| {
                args.nodes
                    .iter()
                    .any(|&arg| is_contextually_sensitive(state, arg))
            }),

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

fn should_preserve_contextual_application_shape(
    db: &dyn tsz_solver::TypeDatabase,
    ty: TypeId,
) -> bool {
    if tsz_solver::type_queries::get_application_info(db, ty).is_some() {
        return true;
    }

    if let Some(members) = tsz_solver::type_queries::get_union_members(db, ty) {
        return members
            .iter()
            .copied()
            .any(|member| should_preserve_contextual_application_shape(db, member));
    }

    match db.lookup(ty) {
        Some(tsz_solver::TypeData::ReadonlyType(inner) | tsz_solver::TypeData::NoInfer(inner)) => {
            should_preserve_contextual_application_shape(db, inner)
        }
        _ => false,
    }
}

fn function_body_needs_contextual_return_type(state: &CheckerState, body_idx: NodeIndex) -> bool {
    use tsz_parser::parser::syntax_kind_ext;

    let Some(body_node) = state.ctx.arena.get(body_idx) else {
        return false;
    };

    if body_node.kind != syntax_kind_ext::BLOCK {
        return expression_needs_contextual_return_type(state, body_idx);
    }

    let Some(block) = state.ctx.arena.get_block(body_node) else {
        return false;
    };

    block.statements.nodes.iter().any(|&stmt_idx| {
        let Some(stmt_node) = state.ctx.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
            return false;
        }
        state
            .ctx
            .arena
            .get_return_statement(stmt_node)
            .is_some_and(|ret| {
                ret.expression.is_some()
                    && expression_needs_contextual_return_type(state, ret.expression)
            })
    })
}

pub(crate) fn expression_needs_contextual_return_type(
    state: &CheckerState,
    expr_idx: NodeIndex,
) -> bool {
    use tsz_parser::parser::syntax_kind_ext;

    if is_contextually_sensitive(state, expr_idx) {
        return true;
    }

    let Some(node) = state.ctx.arena.get(expr_idx) else {
        return false;
    };

    match node.kind {
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => state
            .ctx
            .arena
            .get_parenthesized(node)
            .is_some_and(|paren| expression_needs_contextual_return_type(state, paren.expression)),
        k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => state
            .ctx
            .arena
            .get_conditional_expr(node)
            .is_some_and(|cond| {
                expression_needs_contextual_return_type(state, cond.when_true)
                    || expression_needs_contextual_return_type(state, cond.when_false)
            }),
        k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || k == syntax_kind_ext::CALL_EXPRESSION
            || k == syntax_kind_ext::NEW_EXPRESSION
            || k == syntax_kind_ext::YIELD_EXPRESSION
            || k == syntax_kind_ext::TEMPLATE_EXPRESSION =>
        {
            true
        }
        _ => false,
    }
}

impl<'a> CheckerState<'a> {
    fn type_node_contains_abstract_constructor(
        &self,
        type_idx: NodeIndex,
        visited_aliases: &mut rustc_hash::FxHashSet<NodeIndex>,
    ) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(type_idx) else {
            return false;
        };

        if let Some(query) = self.ctx.arena.get_type_query(node) {
            return self
                .class_symbol_from_expression(query.expr_name)
                .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                .is_some_and(|symbol| (symbol.flags & symbol_flags::ABSTRACT) != 0);
        }

        if let Some(composite) = self.ctx.arena.get_composite_type(node) {
            return composite.types.nodes.iter().any(|&member| {
                self.type_node_contains_abstract_constructor(member, visited_aliases)
            });
        }

        if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
            return self
                .type_node_contains_abstract_constructor(wrapped.type_node, visited_aliases);
        }

        if let Some(type_ref) = self.ctx.arena.get_type_ref(node) {
            let Some(sym_id) = self
                .resolve_identifier_symbol(type_ref.type_name)
                .or_else(|| {
                    self.ctx
                        .binder
                        .resolve_identifier(self.ctx.arena, type_ref.type_name)
                })
            else {
                return false;
            };
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                return false;
            };

            if (symbol.flags & symbol_flags::ABSTRACT) != 0 {
                return true;
            }

            if (symbol.flags & symbol_flags::TYPE_ALIAS) == 0 {
                return false;
            }

            for &decl_idx in &symbol.declarations {
                if !visited_aliases.insert(decl_idx) {
                    continue;
                }
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                if decl_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                    continue;
                }
                if let Some(alias) = self.ctx.arena.get_type_alias(decl_node)
                    && self
                        .type_node_contains_abstract_constructor(alias.type_node, visited_aliases)
                {
                    return true;
                }
            }
        }

        false
    }

    fn declared_new_target_contains_abstract_constructor(&mut self, expr_idx: NodeIndex) -> bool {
        let Some(sym_id) = self
            .resolve_identifier_symbol(expr_idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, expr_idx))
        else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(symbol.value_declaration) else {
            return false;
        };

        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
            && var_decl.type_annotation.is_some()
        {
            let mut visited_aliases = rustc_hash::FxHashSet::default();
            return self.type_node_contains_abstract_constructor(
                var_decl.type_annotation,
                &mut visited_aliases,
            );
        }

        if let Some(param) = self.ctx.arena.get_parameter(decl_node)
            && param.type_annotation.is_some()
        {
            let mut visited_aliases = rustc_hash::FxHashSet::default();
            return self.type_node_contains_abstract_constructor(
                param.type_annotation,
                &mut visited_aliases,
            );
        }

        false
    }

    pub(crate) const fn should_suppress_weak_key_arg_mismatch(
        &mut self,
        _callee_expr: NodeIndex,
        _args: &[NodeIndex],
        _mismatch_index: usize,
        _actual: TypeId,
    ) -> bool {
        false
    }
    pub(crate) const fn should_suppress_weak_key_no_overload(
        &mut self,
        _callee_expr: NodeIndex,
        _args: &[NodeIndex],
    ) -> bool {
        false
    }
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

        let decl_idx = if symbol.value_declaration.is_some() {
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
        let resolved_export_equals_sym = self
            .ctx
            .binder
            .get_symbol(export_equals_sym)
            .is_some_and(|symbol| (symbol.flags & tsz_binder::symbol_flags::ALIAS) != 0)
            .then(|| {
                let mut visited_aliases = Vec::new();
                self.resolve_alias_symbol(export_equals_sym, &mut visited_aliases)
            })
            .flatten()
            .unwrap_or(export_equals_sym);

        let mut constructor_type = self.get_type_of_symbol(resolved_export_equals_sym);
        if constructor_type == TypeId::UNKNOWN || constructor_type == TypeId::ERROR {
            constructor_type = self.get_type_of_symbol(export_equals_sym);
        }

        // If `export =` resolves to an alias chain we couldn't lower to a concrete
        // constructor type, prefer any concrete value export from the module over
        // propagating unknown into TS18046 false positives.
        if constructor_type == TypeId::UNKNOWN || constructor_type == TypeId::ERROR {
            let mut preferred_candidate: Option<TypeId> = None;
            let mut fallback_candidate: Option<TypeId> = None;
            for (export_name, export_sym) in exports.iter() {
                if export_name == "export=" {
                    continue;
                }
                let candidate = self.get_type_of_symbol(*export_sym);
                if candidate == TypeId::UNKNOWN || candidate == TypeId::ERROR {
                    continue;
                }

                let symbol_flags = self
                    .ctx
                    .binder
                    .get_symbol(*export_sym)
                    .map_or(0, |sym| sym.flags);
                let is_likely_constructor_symbol = (symbol_flags
                    & (tsz_binder::symbol_flags::CLASS | tsz_binder::symbol_flags::FUNCTION))
                    != 0;
                if is_likely_constructor_symbol && preferred_candidate.is_none() {
                    preferred_candidate = Some(candidate);
                }
                if fallback_candidate.is_none() {
                    fallback_candidate = Some(candidate);
                }
            }
            if let Some(candidate) = preferred_candidate.or(fallback_candidate) {
                constructor_type = candidate;
            }
        }

        Some(constructor_type)
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

        if self.declared_new_target_contains_abstract_constructor(new_expr.expression) {
            self.error_at_node(
                idx,
                "Cannot create an instance of an abstract class.",
                diagnostic_codes::CANNOT_CREATE_AN_INSTANCE_OF_AN_ABSTRACT_CLASS,
            );
            return TypeId::ERROR;
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
                                && symbol.value_declaration.is_some()
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

        // Check abstract constructor unions before constructor-type normalization
        // collapses nested aliases into a merged callable shape. Mixed unions like
        // `Concretes | Abstracts` need to preserve their member structure here.
        let raw_resolved_constructor_type = self.resolve_lazy_type(constructor_type);
        if self.type_contains_abstract_class(raw_resolved_constructor_type) {
            self.error_at_node(
                idx,
                "Cannot create an instance of an abstract class.",
                diagnostic_codes::CANNOT_CREATE_AN_INSTANCE_OF_AN_ABSTRACT_CLASS,
            );
            return TypeId::ERROR;
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
            if let Some(ref type_args_list) = new_expr.type_arguments
                && !type_args_list.nodes.is_empty()
            {
                self.error_at_node(
                    idx,
                    crate::diagnostics::diagnostic_messages::UNTYPED_FUNCTION_CALLS_MAY_NOT_ACCEPT_TYPE_ARGUMENTS,
                    crate::diagnostics::diagnostic_codes::UNTYPED_FUNCTION_CALLS_MAY_NOT_ACCEPT_TYPE_ARGUMENTS,
                );
            }

            // Still need to check arguments for definite assignment and other errors
            let args = match new_expr.arguments.as_ref() {
                Some(a) => a.nodes.as_slice(),
                None => &[],
            };
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None, // No parameter type info for ANY callee
                check_excess_properties,
                None, // No skipping needed
            );

            return TypeId::ANY;
        }
        if constructor_type == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        // TS18046: Constructing an expression of type `unknown` is not allowed.
        // tsc emits TS18046 instead of TS2351 when the constructor type is `unknown`.
        // Without strictNullChecks, unknown is treated like any (constructable, returns any).
        if constructor_type == TypeId::UNKNOWN {
            if self.error_is_of_type_unknown(new_expr.expression) {
                // Still need to check arguments for definite assignment (TS2454)
                let args = match new_expr.arguments.as_ref() {
                    Some(a) => a.nodes.as_slice(),
                    None => &[],
                };
                let check_excess_properties = false;
                self.collect_call_argument_types_with_context(
                    args,
                    |_i, _arg_count| None,
                    check_excess_properties,
                    None,
                );
                return TypeId::ERROR;
            }
            // Without strictNullChecks, treat unknown like any
            let args = match new_expr.arguments.as_ref() {
                Some(a) => a.nodes.as_slice(),
                None => &[],
            };
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None,
                check_excess_properties,
                None,
            );
            return TypeId::ANY;
        }

        // Resolve TypeQuery types (`typeof X`) that may come through interface/object
        // property access. The solver cannot resolve TypeQuery internally (no TypeResolver),
        // so we resolve it here to the actual constructor/value type.
        constructor_type = self.resolve_type_query_type(constructor_type);

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

        // Extract construct signature to check for generic constructor needing two-pass inference.
        // Use get_construct_signature (not get_contextual_signature) to include generic
        // construct signatures — those are skipped by contextual extraction but needed
        // for two-pass inference where we infer the type params ourselves.
        let constructor_shape_type = self.resolve_ref_type(constructor_type);
        let constructor_shape = call_checker::get_construct_signature(
            self.ctx.types,
            constructor_shape_type,
            args.len(),
        );
        let is_generic_new = constructor_shape
            .as_ref()
            .is_some_and(|s| !s.type_params.is_empty())
            && new_expr.type_arguments.is_none();
        trace!(
            is_generic_new = is_generic_new,
            constructor_shape_found = constructor_shape.is_some(),
            type_params_count = constructor_shape
                .as_ref()
                .map(|s| s.type_params.len())
                .unwrap_or(0),
            constructor_param_types = ?constructor_shape.as_ref().map(|s| s.params.iter().map(|p| (
                self.format_type(p.type_id),
                self.ctx.types.lookup(p.type_id),
                tsz_solver::type_queries::get_application_info(self.ctx.types, p.type_id)
                    .map(|(_, args)| args),
            )).collect::<Vec<_>>()),
            "New expression: two-pass inference check"
        );

        // When the constructor has a generic signature, use that signature's function shape as the
        // contextual type source. This is needed for overloaded constructors like Map where the first
        // signature is non-generic (`new(): Map<any,any>`) but a later one is generic
        // (`new<K,V>(entries?): Map<K,V>`). Without this, `ParameterForCallExtractor` would skip all
        // generic construct signatures and return no contextual type, causing array/object literals
        // passed as arguments to be over-widened (e.g. `[["",true]]` → `(string|boolean)[][]`
        // instead of `[string, boolean][]`).
        let ctx_helper = if is_generic_new && let Some(ref shape) = constructor_shape {
            // Build a Function type from the generic signature so that
            // `ParameterForCallExtractor::visit_function` can extract param types directly,
            // bypassing the Callable-level logic that skips generic construct signatures.
            let factory = self.ctx.types.factory();
            let func_type = factory.function(tsz_solver::FunctionShape {
                params: shape.params.clone(),
                return_type: shape.return_type,
                this_type: shape.this_type,
                type_params: shape.type_params.clone(),
                type_predicate: shape.type_predicate.clone(),
                is_constructor: true,
                is_method: false,
            });
            ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                func_type,
                self.ctx.compiler_options.no_implicit_any,
            )
        } else {
            ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                constructor_type,
                self.ctx.compiler_options.no_implicit_any,
            )
        };
        let check_excess_properties = true;
        let prev_generic_excess_skip = self.ctx.generic_excess_skip.take();

        let arg_types = if is_generic_new {
            if let Some(shape) = constructor_shape {
                // Pre-compute which parameter positions should skip excess property
                // checking because the original parameter type contains a type parameter.
                let excess_skip: Vec<bool> = {
                    let arg_count = args.len();
                    (0..arg_count)
                        .map(|i| {
                            let from_shape = if i < shape.params.len() {
                                tsz_solver::type_queries::contains_type_parameters_db(
                                    self.ctx.types,
                                    shape.params[i].type_id,
                                )
                            } else if let Some(last) = shape.params.last() {
                                last.rest
                                    && tsz_solver::type_queries::contains_type_parameters_db(
                                        self.ctx.types,
                                        last.type_id,
                                    )
                            } else {
                                false
                            };
                            let from_ctx = ctx_helper
                                .get_parameter_type_for_call(i, arg_count)
                                .is_some_and(|param_type| {
                                    tsz_solver::type_queries::contains_type_parameters_db(
                                        self.ctx.types,
                                        param_type,
                                    )
                                });
                            from_shape || from_ctx
                        })
                        .collect()
                };
                if excess_skip.iter().any(|&s| s) {
                    self.ctx.generic_excess_skip = Some(excess_skip);
                }

                // Two-pass inference for generic constructors (same as call expressions)
                let sensitive_args: Vec<bool> = args
                    .iter()
                    .map(|&arg| is_contextually_sensitive(self, arg))
                    .collect();
                let round1_skip_outer_context: Vec<bool> = args
                    .iter()
                    .map(|&arg| self.round1_should_skip_outer_contextual_type(arg))
                    .collect();
                let needs_two_pass = sensitive_args.iter().copied().any(std::convert::identity);

                if needs_two_pass {
                    // === Round 1: Collect non-contextual argument types ===
                    // Skip checking sensitive arguments entirely to prevent TS7006
                    // from being emitted before inference completes.
                    let mut round1_arg_types = self.collect_call_argument_types_with_context(
                        args,
                        |i, arg_count| {
                            let skip_round1_context = (i < sensitive_args.len()
                                && sensitive_args[i])
                                || (i < round1_skip_outer_context.len()
                                    && round1_skip_outer_context[i]);
                            if skip_round1_context {
                                None
                            } else {
                                ctx_helper.get_parameter_type_for_call(i, arg_count)
                            }
                        },
                        check_excess_properties,
                        Some(&sensitive_args),
                    );

                    // For sensitive object literal arguments, extract a partial type
                    // from non-sensitive properties to improve inference.
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
                    let mut substitution = {
                        let round2_contextual_type = if let Some(contextual) =
                            self.ctx.contextual_type
                            && contextual != TypeId::ANY
                            && contextual != TypeId::UNKNOWN
                            && contextual != TypeId::NEVER
                            && !self.type_contains_error(contextual)
                            && self.is_promise_type(contextual)
                        {
                            if let Some(inner) = self.promise_like_return_type_argument(contextual)
                            {
                                let promise_like_t = self.get_promise_like_type(inner);
                                let promise_t = self.get_promise_type(inner);
                                let mut members = vec![inner, promise_like_t];
                                if let Some(pt) = promise_t {
                                    members.push(pt);
                                }
                                Some(self.ctx.types.factory().union(members))
                            } else {
                                self.ctx.contextual_type
                            }
                        } else {
                            self.ctx.contextual_type
                        };
                        let env = self.ctx.type_env.borrow();
                        call_checker::compute_contextual_types_with_context(
                            self.ctx.types,
                            &self.ctx,
                            &env,
                            &evaluated_shape,
                            &round1_arg_types,
                            round2_contextual_type,
                        )
                    };
                    if let Some(contextual) = self.ctx.contextual_type {
                        use tsz_binder::SymbolId;

                        if let (Some((src_base, src_args)), Some((dst_base, dst_args))) = (
                            query::get_application_info(self.ctx.types, shape.return_type),
                            query::get_application_info(self.ctx.types, contextual),
                        ) {
                            let base_name = |base: TypeId| -> Option<&str> {
                                query::lazy_def_id(self.ctx.types, base)
                                    .and_then(|def_id| self.ctx.def_to_symbol_id(def_id))
                                    .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                                    .map(|symbol| symbol.escaped_name.as_str())
                                    .or_else(|| {
                                        tsz_solver::visitor::type_query_symbol(self.ctx.types, base)
                                            .and_then(|sym_ref| {
                                                self.ctx
                                                    .binder
                                                    .get_symbol(SymbolId(sym_ref.0))
                                                    .map(|symbol| symbol.escaped_name.as_str())
                                            })
                                    })
                            };
                            let same_base = src_base == dst_base
                                || matches!(
                                    (base_name(src_base), base_name(dst_base)),
                                    (Some(left), Some(right)) if left == right
                                );
                            if same_base && src_args.len() == dst_args.len() {
                                for (src_arg, dst_arg) in src_args.iter().zip(dst_args.iter()) {
                                    if let Some(info) =
                                        query::type_parameter_info(self.ctx.types, *src_arg)
                                    {
                                        let current = substitution.get(info.name);
                                        let unresolved = current.is_none_or(|ty| {
                                            query::type_parameter_info(self.ctx.types, ty).is_some()
                                        });
                                        if unresolved {
                                            substitution.insert(info.name, *dst_arg);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Round 2: apply inferred types as contextual types for sensitive args
                    let arg_count = args.len();
                    let mut round2_contextual_types: Vec<Option<TypeId>> =
                        Vec::with_capacity(arg_count);
                    for i in 0..arg_count {
                        let ctx_type = if let Some(param_type) =
                            ctx_helper.get_parameter_type_for_call(i, arg_count)
                        {
                            let promise_executor_context = if i == 0 {
                                if let Some(contextual) = self.ctx.contextual_type
                                    && self.is_promise_type(contextual)
                                    && let Some(inner) =
                                        self.promise_like_return_type_argument(contextual)
                                    && let Some(exec_shape) =
                                        query::get_function_shape(self.ctx.types, param_type)
                                {
                                    let mut exec_shape = (*exec_shape).clone();
                                    if let Some(first_param) = exec_shape.params.first_mut()
                                        && let Some(resolve_shape) = query::get_function_shape(
                                            self.ctx.types,
                                            first_param.type_id,
                                        )
                                    {
                                        let mut resolve_shape = (*resolve_shape).clone();
                                        if let Some(resolve_first) =
                                            resolve_shape.params.first_mut()
                                        {
                                            let promise_like_inner =
                                                self.get_promise_like_type(inner);
                                            resolve_first.type_id = self
                                                .ctx
                                                .types
                                                .factory()
                                                .union(vec![inner, promise_like_inner]);
                                            first_param.type_id =
                                                self.ctx.types.function(resolve_shape);
                                            Some(self.ctx.types.function(exec_shape))
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            let mut round2_substitution = substitution.clone();
                            if let Some(contextual) = self.ctx.contextual_type
                                && self.is_promise_type(contextual)
                                && let Some(inner) =
                                    self.promise_like_return_type_argument(contextual)
                            {
                                for ty in tsz_solver::visitor::collect_all_types(
                                    self.ctx.types,
                                    param_type,
                                ) {
                                    if let Some(info) =
                                        query::type_parameter_info(self.ctx.types, ty)
                                    {
                                        let current = round2_substitution.get(info.name);
                                        let unresolved = current.is_none_or(|mapped| {
                                            query::type_parameter_info(self.ctx.types, mapped)
                                                .is_some()
                                        });
                                        if unresolved {
                                            round2_substitution.insert(info.name, inner);
                                        }
                                    }
                                }
                            }
                            let instantiated = promise_executor_context.unwrap_or_else(|| {
                                tsz_solver::instantiate_type(
                                    self.ctx.types,
                                    param_type,
                                    &round2_substitution,
                                )
                            });
                            // Resolve type parameter constraints for contextual typing.
                            // When a param is a TypeParameter with a constraint (e.g.,
                            // TCallback extends Callback<TFoo, TBar>), use the
                            // instantiated constraint as contextual type. Only if the
                            // result is fully resolved (no outer-scope type params).
                            // (See matching logic in call.rs Round 2.)
                            let instantiated = if let Some(tp_info) =
                                tsz_solver::type_param_info(self.ctx.types, instantiated)
                                && let Some(constraint) = tp_info.constraint
                            {
                                let instantiated_constraint = tsz_solver::instantiate_type(
                                    self.ctx.types,
                                    constraint,
                                    &round2_substitution,
                                );
                                let evaluated =
                                    self.evaluate_type_with_env(instantiated_constraint);
                                if !tsz_solver::type_queries::contains_type_parameters_db(
                                    self.ctx.types,
                                    evaluated,
                                ) {
                                    evaluated
                                } else {
                                    instantiated
                                }
                            } else {
                                instantiated
                            };
                            let contextual = if should_preserve_contextual_application_shape(
                                self.ctx.types,
                                instantiated,
                            ) {
                                instantiated
                            } else {
                                self.evaluate_type_with_env(instantiated)
                            };
                            trace!(
                                arg_index = i,
                                param_type_display = %self.format_type(param_type),
                                instantiated_display = %self.format_type(instantiated),
                                contextual_display = %self.format_type(contextual),
                                contextual_key = ?self.ctx.types.lookup(contextual),
                                "New expression Round 2 contextual type"
                            );
                            Some(contextual)
                        } else {
                            None
                        };
                        round2_contextual_types.push(ctx_type);
                    }

                    for (i, &arg_idx) in args.iter().enumerate() {
                        if i < sensitive_args.len() && sensitive_args[i] {
                            self.clear_type_cache_recursive(arg_idx);
                        }
                    }

                    self.collect_call_argument_types_with_context(
                        args,
                        |i, arg_count| {
                            if i < round2_contextual_types.len() {
                                round2_contextual_types[i]
                            } else {
                                ctx_helper.get_parameter_type_for_call(i, arg_count)
                            }
                        },
                        check_excess_properties,
                        None,
                    )
                } else {
                    self.collect_call_argument_types_with_context(
                        args,
                        |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
                        check_excess_properties,
                        None,
                    )
                }
            } else {
                self.collect_call_argument_types_with_context(
                    args,
                    |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
                    check_excess_properties,
                    None,
                )
            }
        } else {
            self.collect_call_argument_types_with_context(
                args,
                |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
                check_excess_properties,
                None,
            )
        };
        self.ctx.generic_excess_skip = prev_generic_excess_skip;

        self.ensure_relation_input_ready(constructor_type);
        self.ensure_relation_inputs_ready(&arg_types);

        // Delegate to Solver for constructor resolution, passing contextual type
        // so generic constructors like `new Promise(...)` can infer type parameters
        // from the expected type (e.g., `const x: Obj = new Promise(...)` infers T=Obj).
        let result = self.resolve_new_with_checker_adapter(
            constructor_type,
            &arg_types,
            false,
            self.ctx.contextual_type,
        );

        match result {
            CallResult::Success(return_type) => return_type,
            CallResult::VoidFunctionCalledWithNew | CallResult::NonVoidFunctionCalledWithNew => {
                // In JS/checkJs files, functions with `this.prop = value` assignments
                // are treated as constructor functions (tsc's isJSConstructor). Synthesize
                // an instance type from the collected this-property assignments.
                if self.ctx.is_js_file()
                    && let Some(instance_type) = self.synthesize_js_constructor_instance_type(
                        new_expr.expression,
                        constructor_type,
                        &arg_types,
                    )
                {
                    return instance_type;
                }

                // TS7009: 'new' expression whose target lacks a construct signature
                // implicitly has an 'any' type (only under noImplicitAny).
                // In JS/checkJs, suppress only when we successfully recognized the
                // target as a JS constructor via `this`-property synthesis above.
                if self.ctx.no_implicit_any() {
                    self.error_at_node(
                        idx,
                        crate::diagnostics::diagnostic_messages::NEW_EXPRESSION_WHOSE_TARGET_LACKS_A_CONSTRUCT_SIGNATURE_IMPLICITLY_HAS_AN_ANY_TY,
                        crate::diagnostics::diagnostic_codes::NEW_EXPRESSION_WHOSE_TARGET_LACKS_A_CONSTRUCT_SIGNATURE_IMPLICITLY_HAS_AN_ANY_TY,
                    );
                }
                TypeId::ANY
            }
            CallResult::NotCallable { .. } => {
                // In circular class-resolution scenarios, class constructor targets can
                // transiently lose construct signatures. TypeScript suppresses TS2351
                // here and reports the underlying class/argument diagnostics instead.
                if self.new_target_is_class_symbol(new_expr.expression) {
                    return TypeId::ERROR;
                }
                self.error_not_constructable_at(constructor_type, new_expr.expression);
                TypeId::ERROR
            }
            CallResult::ArgumentCountMismatch {
                expected_min,
                expected_max,
                actual,
            } => {
                // Suppress TS2554/TS2555 when parse errors exist to avoid cascading diagnostics
                if !self.ctx.has_parse_errors {
                    // Suppress arity errors when the call contains non-tuple spread
                    // arguments — the spread provides an indeterminate number of values.
                    // TSC only emits TS2556 in this case, not TS2555/TS2554.
                    // However, tuple spreads have known length, so TS2554 should
                    // still fire for those.
                    let has_non_tuple_spread = args.iter().any(|&arg_idx| {
                        if let Some(n) = self.ctx.arena.get(arg_idx)
                            && n.kind == syntax_kind_ext::SPREAD_ELEMENT
                            && let Some(spread_data) = self.ctx.arena.get_spread(n)
                        {
                            let spread_type = self.get_type_of_node(spread_data.expression);
                            let spread_type = self.resolve_type_for_property_access(spread_type);
                            let spread_type = self.resolve_lazy_type(spread_type);
                            crate::query_boundaries::common::tuple_elements(
                                self.ctx.types,
                                spread_type,
                            )
                            .is_none()
                        } else {
                            false
                        }
                    });
                    if has_non_tuple_spread {
                        // TS2556 was already emitted; don't cascade with TS2555/TS2554.
                    } else if actual < expected_min && expected_max.is_none() {
                        // Too few arguments with rest parameters (unbounded) - use TS2555
                        self.error_expected_at_least_arguments_at(expected_min, actual, idx);
                    } else {
                        // Use TS2554 for exact count, range, or too many args
                        let max = expected_max.unwrap_or(expected_min);
                        let expanded_args = self.build_expanded_args_for_error(args);
                        let args_for_error = if expanded_args.len() > args.len() {
                            &expanded_args
                        } else {
                            args
                        };
                        self.error_argument_count_mismatch_at(
                            expected_min,
                            max,
                            actual,
                            idx,
                            args_for_error,
                        );
                    }
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
                if !self.ctx.has_parse_errors {
                    self.error_at_node(
                        idx,
                        &format!(
                            "No overload expects {actual} arguments, but overloads do exist that expect either {expected_low} or {expected_high} arguments."
                        ),
                        diagnostic_codes::NO_OVERLOAD_EXPECTS_ARGUMENTS_BUT_OVERLOADS_DO_EXIST_THAT_EXPECT_EITHER_OR_ARGUM,
                    );
                }
                TypeId::ERROR
            }
            CallResult::ArgumentTypeMismatch {
                index,
                expected,
                actual,
                fallback_return,
            } => {
                if let Some((new_start, new_end)) = self.get_node_span(idx)
                    && self.has_diagnostic_code_within_span(
                        new_start,
                        new_end,
                        diagnostic_codes::STATIC_MEMBERS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS,
                    )
                {
                    if fallback_return != TypeId::ERROR {
                        return fallback_return;
                    }
                    return TypeId::ERROR;
                }
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
                if fallback_return != TypeId::ERROR {
                    fallback_return
                } else {
                    TypeId::ERROR
                }
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
            CallResult::NoOverloadMatch {
                failures,
                fallback_return: _,
                ..
            } => {
                if !self.should_suppress_weak_key_no_overload(new_expr.expression, args) {
                    self.error_no_overload_matches_at(idx, &failures);
                }
                TypeId::ERROR
            }
            CallResult::ThisTypeMismatch {
                expected_this,
                actual_this,
            } => {
                self.error_this_type_mismatch_at(expected_this, actual_this, idx);
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

        if self.alias_resolves_to_type_only(sym_id) {
            self.error_type_only_value_at(class_name, expr_idx);
            return Some(TypeId::ERROR);
        }

        let has_type = (symbol.flags & symbol_flags::TYPE) != 0;
        let has_value = (symbol.flags & symbol_flags::VALUE) != 0;
        let is_type_alias = (symbol.flags & symbol_flags::TYPE_ALIAS) != 0;

        if !has_value && (is_type_alias || has_type) {
            // Type parameters only shadow in type contexts, not value contexts.
            // `new A()` where `A` is a type param shadowing an outer class should
            // resolve to the outer class, not emit TS2693.
            let is_type_param_only =
                (symbol.flags & symbol_flags::TYPE_PARAMETER) != 0 && !has_value;
            if is_type_param_only {
                let lib_binders = self.get_lib_binders();
                let has_outer_value = self
                    .ctx
                    .binder
                    .resolve_identifier_with_filter(self.ctx.arena, expr_idx, &lib_binders, |sid| {
                        self.ctx
                            .binder
                            .get_symbol_with_libs(sid, &lib_binders)
                            .is_some_and(|s| s.flags & symbol_flags::VALUE != 0)
                    })
                    .is_some();
                if has_outer_value {
                    // Fall through — the new expression will use the outer value
                    return None;
                }
            }
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
        use tsz_parser::parser::syntax_kind_ext;
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
        if self
            .ctx
            .binder
            .get_symbol(sym_id)
            .is_some_and(|symbol| (symbol.flags & symbol_flags::CLASS) != 0)
        {
            return true;
        }
        // Cross-file: in multi-file mode, the name may resolve to a namespace in the
        // current file while a class with the same name exists in another file
        // (class+namespace declaration merging across files). Walk up enclosing
        // namespaces and search all binders for a CLASS symbol with the same name.
        if let Some(all_binders) = self.ctx.all_binders.as_ref()
            && !self.ctx.binder.is_external_module()
        {
            let arena = self.ctx.arena;
            let mut current = expr_idx;
            for _ in 0..100 {
                let Some(ext) = arena.get_extended(current) else {
                    break;
                };
                let parent_idx = ext.parent;
                if parent_idx.is_none() {
                    break;
                }
                let Some(parent_node) = arena.get(parent_idx) else {
                    break;
                };
                if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                    && let Some(module_data) = arena.get_module(parent_node)
                    && let Some(ns_name_ident) = arena.get_identifier_at(module_data.name)
                {
                    let ns_name = ns_name_ident.escaped_text.as_str();
                    // Search all binders for a CLASS symbol with the target
                    // name exported from a namespace matching ns_name.
                    for binder in all_binders.iter() {
                        for (_, &parent_sym_id) in binder.file_locals.iter() {
                            let Some(parent_sym) = self.ctx.binder.get_symbol(parent_sym_id) else {
                                continue;
                            };
                            if parent_sym.flags
                                & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                                == 0
                            {
                                continue;
                            }
                            if let Some(parent_exports) = parent_sym.exports.as_ref()
                                && let Some(nested_ns_id) = parent_exports.get(ns_name)
                                && let Some(nested_ns) = self.ctx.binder.get_symbol(nested_ns_id)
                                && nested_ns.flags
                                    & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                                    != 0
                                && let Some(nested_exports) = nested_ns.exports.as_ref()
                                && let Some(member_id) = nested_exports.get(name)
                                && self
                                    .ctx
                                    .binder
                                    .get_symbol(member_id)
                                    .is_some_and(|s| (s.flags & symbol_flags::CLASS) != 0)
                            {
                                return true;
                            }
                        }
                    }
                }
                current = parent_idx;
            }
        }
        false
    }

    /// Synthesize an instance type for a JS constructor function.
    /// Given `function Foo(x) { this.a = x; this.b = "hello"; }`, collects the
    /// `this.prop = value` assignments and builds an object type `{ a: T, b: string }`.
    /// Also scans `Foo.prototype.m = function() { this.y = ... }` patterns for
    /// properties assigned in prototype methods (typed as `T | undefined`).
    /// Returns `None` if the target is not a plain function or has no this-property assignments.
    pub(crate) fn synthesize_js_constructor_instance_type(
        &mut self,
        expr_idx: NodeIndex,
        constructor_type: TypeId,
        arg_types: &[TypeId],
    ) -> Option<TypeId> {
        use rustc_hash::FxHashMap;
        use tsz_binder::symbol_flags;
        use tsz_solver::PropertyInfo;

        // Resolve the function symbol from the new expression target
        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expr_idx)
            .or_else(|| self.ctx.binder.get_node_symbol(expr_idx))?;

        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        // Only handle plain function declarations (not classes)
        if symbol.flags & symbol_flags::CLASS != 0 {
            return None;
        }
        if symbol.flags & symbol_flags::FUNCTION == 0 {
            return None;
        }

        // Find the function body
        let value_decl = symbol.value_declaration;
        let node = self.ctx.arena.get(value_decl)?;
        let func = self.ctx.arena.get_function(node)?;
        let body_idx = func.body;
        if body_idx.is_none() {
            return None;
        }

        // Get the function name for prototype pattern matching
        let func_name = func.name;
        let func_name_str = self
            .ctx
            .arena
            .get(func_name)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|ident| ident.escaped_text.clone());

        // Check if the constructor function has @template type params
        let func_shape =
            tsz_solver::type_queries::get_function_shape(self.ctx.types, constructor_type);
        let is_generic = func_shape
            .as_ref()
            .is_some_and(|s| !s.type_params.is_empty());

        let mut properties: FxHashMap<tsz_common::interner::Atom, PropertyInfo> =
            FxHashMap::default();

        if is_generic {
            // For generic constructor functions, we build the instance type using
            // the function shape's correctly-resolved parameter types (which were
            // created during get_type_of_function when @template was in scope).
            // We cannot use collect_js_constructor_this_properties here because
            // get_type_of_node resolves types in the call site context, not the
            // function declaration context.
            let shape = func_shape.as_ref().unwrap();

            // Build param name → type map from the function shape
            let param_type_map: FxHashMap<String, TypeId> = func
                .parameters
                .nodes
                .iter()
                .zip(shape.params.iter())
                .filter_map(|(&param_idx, param_info)| {
                    let param_node = self.ctx.arena.get(param_idx)?;
                    let param_data = self.ctx.arena.get_parameter(param_node)?;
                    let name_node = self.ctx.arena.get(param_data.name)?;
                    let ident = self.ctx.arena.get_identifier(name_node)?;
                    Some((ident.escaped_text.clone(), param_info.type_id))
                })
                .collect();

            // Push type params into scope for @type resolution
            let mut scope_restore: Vec<(String, Option<TypeId>)> = Vec::new();
            let factory = self.ctx.types.factory();
            for tp in &shape.type_params {
                let name = self.ctx.types.resolve_atom(tp.name);
                let ty = factory.type_param(tp.clone());
                let previous = self.ctx.type_parameter_scope.insert(name.clone(), ty);
                scope_restore.push((name, previous));
            }

            // Scan the constructor body for this-property patterns
            self.collect_generic_constructor_this_properties(
                body_idx,
                &param_type_map,
                &mut properties,
                Some(sym_id),
            );

            // Restore type_parameter_scope
            for (name, previous) in scope_restore {
                if let Some(prev) = previous {
                    self.ctx.type_parameter_scope.insert(name, prev);
                } else {
                    self.ctx.type_parameter_scope.remove(&name);
                }
            }
        } else {
            // Non-generic: use standard property collection
            self.collect_js_constructor_this_properties(body_idx, &mut properties, Some(sym_id));
        }

        // Also scan Foo.prototype.m = ... patterns for:
        // 1. Method bindings (added directly as instance properties)
        // 2. this.prop assignments inside prototype methods (typed as T | undefined)
        if let Some(ref func_name_s) = func_name_str {
            let (method_bindings, this_props) =
                self.collect_prototype_members_and_this_properties(value_decl, func_name_s, sym_id);

            // Add prototype methods as instance properties
            for (name, prop) in method_bindings {
                properties.entry(name).or_insert(prop);
            }

            // Add this-properties from prototype methods (with | undefined)
            for (name, mut prop) in this_props {
                if let std::collections::hash_map::Entry::Vacant(e) = properties.entry(name) {
                    let factory = self.ctx.types.factory();
                    prop.type_id = factory.union(vec![prop.type_id, TypeId::UNDEFINED]);
                    prop.write_type = prop.type_id;
                    e.insert(prop);
                }
            }
        }

        if properties.is_empty() {
            return None;
        }

        // Build an object type from the collected properties
        let props: Vec<PropertyInfo> = properties.into_values().collect();
        let factory = self.ctx.types.factory();
        let instance_type = factory.object(props);

        // If the constructor function has @template type params, instantiate the
        // instance type by inferring type arguments from the actual call arguments.
        if let Some(ref shape) = func_shape
            && !shape.type_params.is_empty()
            && !arg_types.is_empty()
        {
            let mut type_args = Vec::with_capacity(shape.type_params.len());
            for tp in &shape.type_params {
                let tp_id = self.ctx.types.factory().type_param(tp.clone());
                let mut inferred = None;
                for (i, param) in shape.params.iter().enumerate() {
                    if param.type_id == tp_id
                        && let Some(&arg_ty) = arg_types.get(i)
                    {
                        // Widen literal types (e.g., 1 → number)
                        // since non-const type params don't preserve literals
                        inferred = Some(tsz_solver::operations::widening::widen_type(
                            self.ctx.types,
                            arg_ty,
                        ));
                        break;
                    }
                }
                type_args.push(inferred.unwrap_or(TypeId::UNKNOWN));
            }
            let instantiated = tsz_solver::instantiate_generic(
                self.ctx.types,
                instance_type,
                &shape.type_params,
                &type_args,
            );
            return Some(instantiated);
        }

        Some(instance_type)
    }

    /// Collect this-property assignments from a generic JS constructor function body.
    ///
    /// Unlike `collect_js_constructor_this_properties`, this uses the function shape's
    /// parameter types (correctly resolved during function type creation) instead of
    /// re-evaluating types via `get_type_of_node` (which would resolve in the wrong scope).
    /// Also handles bare `this.prop` expressions with `@type` JSDoc annotations.
    fn collect_generic_constructor_this_properties(
        &mut self,
        body_idx: NodeIndex,
        param_type_map: &rustc_hash::FxHashMap<String, TypeId>,
        properties: &mut rustc_hash::FxHashMap<
            tsz_common::interner::Atom,
            tsz_solver::PropertyInfo,
        >,
        parent_sym: Option<tsz_binder::SymbolId>,
    ) {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;
        use tsz_solver::{PropertyInfo, Visibility};

        let stmts: Vec<NodeIndex> = {
            let Some(body_node) = self.ctx.arena.get(body_idx) else {
                return;
            };
            let Some(block) = self.ctx.arena.get_block(body_node) else {
                return;
            };
            block.statements.nodes.clone()
        };

        for &stmt_idx in &stmts {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(expr_stmt) = self.ctx.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(expr_node) = self.ctx.arena.get(expr_stmt.expression) else {
                continue;
            };

            // Case 1: `this.prop = rhs` (binary assignment)
            if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                if let Some(binary) = self.ctx.arena.get_binary_expr(expr_node)
                    && binary.operator_token == SyntaxKind::EqualsToken as u16
                    && let Some((prop_name, rhs_type)) = self.extract_generic_this_assignment(
                        binary.left,
                        binary.right,
                        param_type_map,
                        stmt_idx,
                    )
                {
                    let name_atom = self.ctx.types.intern_string(&prop_name);
                    properties.entry(name_atom).or_insert(PropertyInfo {
                        name: name_atom,
                        type_id: rhs_type,
                        write_type: rhs_type,
                        optional: false,
                        readonly: false,
                        is_method: false,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: parent_sym,
                        declaration_order: 0,
                    });
                }
                continue;
            }

            // Case 2: bare `this.prop` with `/** @type {T} */` annotation
            if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.ctx.arena.get_access_expr(expr_node)
                && let Some(obj_node) = self.ctx.arena.get(access.expression)
                && obj_node.kind == SyntaxKind::ThisKeyword as u16
                && let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                let prop_name = ident.escaped_text.clone();
                // Check for @type annotation on the expression statement
                if let Some(jsdoc_type) = self.jsdoc_type_annotation_for_node(stmt_idx) {
                    let name_atom = self.ctx.types.intern_string(&prop_name);
                    properties.entry(name_atom).or_insert(PropertyInfo {
                        name: name_atom,
                        type_id: jsdoc_type,
                        write_type: jsdoc_type,
                        optional: false,
                        readonly: false,
                        is_method: false,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: parent_sym,
                        declaration_order: 0,
                    });
                }
            }
        }
    }

    /// Extract a this-property assignment's name and type for generic constructor context.
    ///
    /// For `this.prop = rhs`:
    /// - If rhs is an identifier matching a parameter name, uses the function shape's type
    /// - If there's a @type JSDoc annotation, uses that
    /// - Otherwise falls back to `get_type_of_node` for the rhs
    fn extract_generic_this_assignment(
        &mut self,
        lhs_idx: NodeIndex,
        rhs_idx: NodeIndex,
        param_type_map: &rustc_hash::FxHashMap<String, TypeId>,
        stmt_idx: NodeIndex,
    ) -> Option<(String, TypeId)> {
        use tsz_scanner::SyntaxKind;

        let lhs_node = self.ctx.arena.get(lhs_idx)?;
        if lhs_node.kind != tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(lhs_node)?;
        let obj_node = self.ctx.arena.get(access.expression)?;
        if obj_node.kind != SyntaxKind::ThisKeyword as u16 {
            return None;
        }
        let name_node = self.ctx.arena.get(access.name_or_argument)?;
        let ident = self.ctx.arena.get_identifier(name_node)?;
        let prop_name = ident.escaped_text.clone();

        // Determine type: @type annotation > param type > get_type_of_node
        let type_id = if let Some(jsdoc_type) = self.jsdoc_type_annotation_for_node(stmt_idx) {
            jsdoc_type
        } else {
            // Check if RHS is a parameter identifier
            let rhs_node = self.ctx.arena.get(rhs_idx)?;
            if rhs_node.kind == SyntaxKind::Identifier as u16 {
                if let Some(rhs_ident) = self.ctx.arena.get_identifier(rhs_node) {
                    if let Some(&param_type) = param_type_map.get(&rhs_ident.escaped_text) {
                        param_type
                    } else {
                        self.get_type_of_node(rhs_idx)
                    }
                } else {
                    self.get_type_of_node(rhs_idx)
                }
            } else {
                self.get_type_of_node(rhs_idx)
            }
        };

        Some((prop_name, type_id))
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
        use tsz_solver::type_queries::{TypeResolutionKind, classify_for_type_resolution};

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
