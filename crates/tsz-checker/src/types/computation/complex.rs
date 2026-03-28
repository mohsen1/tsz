//! Complex type computation: new expressions and constructability.
//!
//! Contextual sensitivity analysis is in `contextual.rs`.
//! Union/intersection/keyof/class helpers are in `type_operators.rs`.

use crate::call_checker::CallableContext;
use crate::context::TypingRequest;
use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::type_computation::complex as query;
use crate::state::CheckerState;
use tracing::trace;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{ContextualTypeContext, TypeId};

// Re-export for backwards compatibility with existing imports
pub(crate) use super::contextual::{
    expression_needs_contextual_return_type, is_contextually_sensitive,
};

fn should_preserve_contextual_application_shape(
    db: &dyn tsz_solver::TypeDatabase,
    ty: TypeId,
) -> bool {
    if tsz_solver::type_queries::get_application_info(db, ty).is_some() {
        return true;
    }

    if let Some(members) = crate::query_boundaries::common::union_members(db, ty) {
        return members
            .iter()
            .copied()
            .any(|member| should_preserve_contextual_application_shape(db, member));
    }

    if let Some(inner) = tsz_solver::visitor::readonly_inner_type(db, ty)
        .or_else(|| tsz_solver::visitor::no_infer_inner_type(db, ty))
    {
        return should_preserve_contextual_application_shape(db, inner);
    }

    false
}

impl<'a> CheckerState<'a> {
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

    #[allow(dead_code)]
    pub(crate) fn get_type_of_new_expression(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_new_expression_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_new_expression_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        use crate::diagnostics::diagnostic_codes;
        use crate::query_boundaries::common::CallResult;
        use tsz_parser::parser::syntax_kind_ext;
        let contextual_type = request.contextual_type;
        let read_request = request.read().normal_origin().contextual_opt(None);

        let Some(new_expr) = self.ctx.arena.get_call_expr_at(idx) else {
            return TypeId::ERROR; // Missing new expression data - propagate error
        };

        // TS1209: Invalid optional chain from new expression
        if super::access::is_optional_chain(self.ctx.arena, new_expr.expression) {
            let expr_text = self.get_source_text_for_node(new_expr.expression);
            self.error_at_node_msg(
                new_expr.expression,
                diagnostic_codes::INVALID_OPTIONAL_CHAIN_FROM_NEW_EXPRESSION_DID_YOU_MEAN_TO_CALL,
                &[&expr_text],
            );
            return TypeId::ERROR;
        }

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
                    self.get_type_of_node_with_request(new_expr.expression, &read_request)
                }
            } else {
                self.get_type_of_node_with_request(new_expr.expression, &read_request)
            }
        } else {
            self.get_type_of_node_with_request(new_expr.expression, &read_request)
        };
        if let Some(export_equals_ctor) =
            self.new_expression_export_equals_constructor_type(new_expr.expression)
        {
            constructor_type = export_equals_ctor;
        }

        let constructor_for_split = self.evaluate_type_with_env(constructor_type);
        let (non_nullish, nullish_cause) = self.split_nullish_type(constructor_for_split);
        if let Some(cause) = nullish_cause {
            let (code, message) = if let Some(name) = self.expression_text(new_expr.expression) {
                if cause == TypeId::NULL {
                    (
                        diagnostic_codes::IS_POSSIBLY_NULL,
                        format!("'{name}' is possibly 'null'."),
                    )
                } else if cause == TypeId::UNDEFINED {
                    (
                        diagnostic_codes::IS_POSSIBLY_UNDEFINED,
                        format!("'{name}' is possibly 'undefined'."),
                    )
                } else {
                    (
                        diagnostic_codes::IS_POSSIBLY_NULL_OR_UNDEFINED,
                        format!("'{name}' is possibly 'null' or 'undefined'."),
                    )
                }
            } else if cause == TypeId::NULL {
                (
                    diagnostic_codes::OBJECT_IS_POSSIBLY_NULL,
                    "Object is possibly 'null'.".to_string(),
                )
            } else if cause == TypeId::UNDEFINED {
                (
                    diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED,
                    "Object is possibly 'undefined'.".to_string(),
                )
            } else {
                (
                    diagnostic_codes::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED,
                    "Object is possibly 'null' or 'undefined'.".to_string(),
                )
            };
            self.error_at_node(new_expr.expression, &message, code);

            let Some(non_nullish) = non_nullish else {
                return TypeId::ERROR;
            };
            constructor_type = non_nullish;
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
        // inference (and so we match tsc behavior). For implicit calls in JS/checkJs,
        // keep generic constructors intact so `new Foo(1)` can still infer `T = number`
        // instead of defaulting missing type arguments to `any`.
        if new_expr
            .type_arguments
            .as_ref()
            .is_some_and(|type_args| !type_args.nodes.is_empty())
        {
            constructor_type = self.apply_type_arguments_to_constructor_type(
                constructor_type,
                new_expr.type_arguments.as_ref(),
            );
        }

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
        // Protected constructors can only be called within the class hierarchy.
        // When the constructor is inaccessible, tsc only emits the accessibility
        // error and suppresses subsequent arg-count/type-mismatch diagnostics.
        if self.check_constructor_accessibility_for_new(idx, constructor_type) {
            return TypeId::ANY;
        }

        if constructor_type == TypeId::ANY {
            // Before emitting TS2347, check if the new-expression target is a
            // this-property access (e.g., `new this.Map_<K, V>()`). In property
            // initializers, `this.X` may return `any` because the class type is
            // still being constructed. But the member's DECLARED type may have
            // construct signatures with type parameters. If so, suppress TS2347.
            let has_declared_construct_type_params =
                self.new_target_has_declared_generic_construct(new_expr.expression);
            if let Some(ref type_args_list) = new_expr.type_arguments
                && !type_args_list.nodes.is_empty()
                && !has_declared_construct_type_params
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
                CallableContext::none(),
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
                    CallableContext::none(),
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
                CallableContext::none(),
            );
            return TypeId::ANY;
        }

        // Resolve TypeQuery types (`typeof X`) that may come through interface/object
        // property access. The solver cannot resolve TypeQuery internally (no TypeResolver),
        // so we resolve it here to the actual constructor/value type.
        constructor_type = self.resolve_type_query_type(constructor_type);

        // Fully evaluate applied constructor types in the current type environment.
        // `new` on values typed as `ComponentClass<Props>` or `Newable<T>` needs the
        // instantiated construct signatures, not the unevaluated Application shell.
        constructor_type = self.evaluate_type_with_env(constructor_type);

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
        constructor_type = if synthetic_new_constructor.is_some()
            && new_expr
                .type_arguments
                .as_ref()
                .is_some_and(|type_args| !type_args.nodes.is_empty())
        {
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
                type_predicate: shape.type_predicate,
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
                                crate::query_boundaries::common::contains_type_parameters(
                                    self.ctx.types,
                                    shape.params[i].type_id,
                                )
                            } else if let Some(last) = shape.params.last() {
                                last.rest
                                    && crate::query_boundaries::common::contains_type_parameters(
                                        self.ctx.types,
                                        last.type_id,
                                    )
                            } else {
                                false
                            };
                            let from_ctx = ctx_helper
                                .get_parameter_type_for_call(i, arg_count)
                                .is_some_and(|param_type| {
                                    crate::query_boundaries::common::contains_type_parameters(
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
                        CallableContext::none(),
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
                            type_predicate: shape.type_predicate,
                            is_constructor: shape.is_constructor,
                            is_method: shape.is_method,
                        }
                    };
                    let mut substitution = {
                        // When the contextual type is a union containing a Promise member
                        // (e.g., `void | PromiseLike<void> | Promise<void>` from async
                        // function return context), extract the Promise<T> member and use
                        // T for inference. This ensures `new Promise((resolve) => { resolve(); })`
                        // correctly infers T = void when the contextual type comes from
                        // an async function return.
                        let round2_contextual_type = if let Some(contextual) = contextual_type
                            && contextual != TypeId::ANY
                            && contextual != TypeId::UNKNOWN
                            && contextual != TypeId::NEVER
                            && !self.type_contains_error(contextual)
                            && let Some(promise_member) =
                                self.find_promise_in_contextual_type(contextual)
                        {
                            if let Some(inner) =
                                self.promise_like_return_type_argument(promise_member)
                            {
                                let promise_like_t = self.get_promise_like_type(inner);
                                let promise_t = self.get_promise_type(inner);
                                let mut members = vec![inner, promise_like_t];
                                if let Some(pt) = promise_t {
                                    members.push(pt);
                                }
                                Some(self.ctx.types.factory().union(members))
                            } else {
                                contextual_type
                            }
                        } else {
                            contextual_type
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
                    if let Some(contextual) = contextual_type {
                        use tsz_binder::SymbolId;

                        // When the contextual type is a union containing a Promise member
                        // (e.g., from async function return context), use the Promise
                        // member for application-matching against the constructor return type.
                        let contextual_for_app_match = self
                            .find_promise_in_contextual_type(contextual)
                            .unwrap_or(contextual);
                        if let (Some((src_base, src_args)), Some((dst_base, dst_args))) = (
                            query::get_application_info(self.ctx.types, shape.return_type),
                            query::get_application_info(self.ctx.types, contextual_for_app_match),
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

                    // For Promise constructors with a contextual type that doesn't
                    // provide useful T info (any/unknown), default T to void.
                    // TSC infers T from the callback body (e.g., resolve() → void,
                    // resolve("hello") → string). Our architecture can't propagate
                    // that inference, so we apply void only when the contextual type
                    // already indicates "don't care" (any/unknown). Without a
                    // contextual type, T stays unknown to avoid false positives on
                    // resolve(value) calls.
                    if contextual_type.is_some() {
                        let return_type_for_promise_check =
                            self.evaluate_type_with_env(shape.return_type);
                        if self.is_promise_type(return_type_for_promise_check)
                            || self.is_promise_type(shape.return_type)
                        {
                            for tp in &shape.type_params {
                                let needs_void_default = match substitution.get(tp.name) {
                                    None => true,
                                    Some(mapped) => {
                                        query::type_parameter_info(self.ctx.types, mapped).is_some()
                                            || mapped == TypeId::ANY
                                            || mapped == TypeId::UNKNOWN
                                    }
                                };
                                if needs_void_default {
                                    substitution.insert(tp.name, TypeId::VOID);
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
                                if let Some(contextual) = contextual_type
                                    && let Some(promise_member) =
                                        self.find_promise_in_contextual_type(contextual)
                                    && let Some(inner) =
                                        self.promise_like_return_type_argument(promise_member)
                                    // Skip building a custom executor context when the inner
                                    // type is any/unknown — it doesn't add useful information
                                    // and would override the void default from the substitution.
                                    && inner != TypeId::ANY
                                    && inner != TypeId::UNKNOWN
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
                                                .union2(inner, promise_like_inner);
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
                            if let Some(contextual) = contextual_type
                                && let Some(promise_member) =
                                    self.find_promise_in_contextual_type(contextual)
                                && let Some(inner) =
                                    self.promise_like_return_type_argument(promise_member)
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
                                crate::query_boundaries::common::instantiate_type(
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
                                crate::query_boundaries::common::type_param_info(
                                    self.ctx.types,
                                    instantiated,
                                )
                                && let Some(constraint) = tp_info.constraint
                            {
                                let instantiated_constraint =
                                    crate::query_boundaries::common::instantiate_type(
                                        self.ctx.types,
                                        constraint,
                                        &round2_substitution,
                                    );
                                let evaluated =
                                    self.evaluate_type_with_env(instantiated_constraint);
                                if !crate::query_boundaries::common::contains_type_parameters(
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
                            self.invalidate_expression_for_contextual_retry(arg_idx);
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
                        CallableContext::none(),
                    )
                } else {
                    self.collect_call_argument_types_with_context(
                        args,
                        |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
                        check_excess_properties,
                        None,
                        CallableContext::none(),
                    )
                }
            } else {
                self.collect_call_argument_types_with_context(
                    args,
                    |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
                    check_excess_properties,
                    None,
                    CallableContext::none(),
                )
            }
        } else {
            self.collect_call_argument_types_with_context(
                args,
                |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
                check_excess_properties,
                None,
                CallableContext::none(),
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
            contextual_type,
        );

        match result {
            CallResult::Success(return_type) => {
                // For circular classes (TS2506), when `new` is called without
                // explicit type arguments, the solver may return the raw instance
                // type with unresolved type parameters (e.g. `M<T>` instead of
                // `M<unknown>`). Detect this and substitute with `unknown`.
                if self.is_circular_class_new(new_expr.expression)
                    && let Some(fixed) =
                        self.class_instance_type_for_circular_new(new_expr.expression)
                {
                    return fixed;
                }
                // Eagerly evaluate monomorphic Application return types from
                // constructor calls, matching the call expression path in
                // `finalize_call_return_like_success`. Without this, the
                // Application type may be evaluated later through a path that
                // doesn't correctly instantiate interface type parameters,
                // causing false TS2345 errors (e.g., `new FinalizationRegistry(()=>{})`
                // returning raw interface body with unsubstituted `T` instead of
                // `FinalizationRegistry<unknown>` with `T=unknown`).
                // Skip Promise-like types to preserve `await` unwrapping semantics.
                if crate::query_boundaries::common::is_generic_application(
                    self.ctx.types,
                    return_type,
                ) && !self.contains_type_parameters_cached(return_type)
                    && !self.is_promise_type(return_type)
                {
                    self.evaluate_application_type(return_type)
                } else {
                    return_type
                }
            }
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
                    // Instead of returning ERROR (which suppresses TS2339 on property
                    // access), try to return the class's instance type with type
                    // parameters defaulted to `unknown`. This matches tsc behavior:
                    // `(new C).blah` on a circular class produces TS2339 on `C<unknown>`.
                    if let Some(instance_type) =
                        self.class_instance_type_for_circular_new(new_expr.expression)
                    {
                        return instance_type;
                    }
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
                        // Try to elaborate object/array literal arguments into
                        // per-property/element TS2322 errors before falling back
                        // to a blanket TS2345 on the whole argument. This mirrors
                        // the elaboration logic in the regular call result handler.
                        let elaborated = if self.argument_supports_literal_elaboration(arg_idx) {
                            self.try_elaborate_object_literal_arg_error(arg_idx, expected)
                        } else {
                            false
                        };
                        if !elaborated {
                            let _ =
                                self.check_argument_assignable_or_report(actual, expected, arg_idx);
                        }
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
                // Type parameter constraint violations are argument-level
                // mismatches. tsc reports TS2345 at the argument.
                let anchor = args.first().copied().unwrap_or(idx);
                let _ = self.check_argument_assignable_or_report(
                    inferred_type,
                    constraint_type,
                    anchor,
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
}
