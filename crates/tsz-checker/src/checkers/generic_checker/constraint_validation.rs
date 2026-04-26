//! Generic type argument constraint validation (TS2344).
//!
//! Contains `validate_type_args_against_params` and its helper methods for
//! constraint checking, callable detection, heritage-chain coinductive checks,
//! and array-like constraint satisfaction.

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Validate each type argument against its corresponding type parameter constraint.
    /// Reports TS2344 when a type argument doesn't satisfy its constraint.
    ///
    /// Shared implementation used by call expressions, new expressions, and type references.
    pub(super) fn validate_type_args_against_params(
        &mut self,
        type_params: &[tsz_solver::TypeParamInfo],
        type_args_list: &tsz_parser::parser::NodeList,
    ) {
        let type_args: Vec<TypeId> = type_args_list
            .nodes
            .iter()
            .map(|&arg_idx| {
                self.check_type_node_for_static_member_class_type_param_refs(arg_idx);
                self.check_type_node(arg_idx);
                self.get_type_from_type_node(arg_idx)
            })
            .collect();

        for (i, (param, &type_arg)) in type_params.iter().zip(type_args.iter()).enumerate() {
            if let Some(constraint) = param.constraint {
                // Skip constraint checking when the type argument is an error type
                // (avoids cascading errors from unresolved references)
                if type_arg == TypeId::ERROR {
                    continue;
                }

                // Skip constraint checking when the type argument is `this` type.
                // The `this` type is polymorphic (like a type parameter constrained
                // to the enclosing type) and its constraint satisfaction depends on
                // the instantiation context. TSC defers this check, so we should too.
                // Example: `interface Bar extends Foo { other: BoxOfFoo<this>; }`
                // where `BoxOfFoo<T extends Foo>` — `this` satisfies `T extends Foo`
                // because `this` in `Bar` is bounded by `Bar` which extends `Foo`.
                if query::is_this_type(self.ctx.types.as_type_database(), type_arg) {
                    continue;
                }

                // Skip constraint checking for `infer` type arguments in conditional
                // types (e.g., `R extends Reducer<any, infer A>`). TSC does not emit
                // TS2344 for infer positions — constraints on inferred type params
                // are checked during conditional type evaluation, not here.
                // Also look through parenthesized types: `IsNumber<(infer N)>`.
                if let Some(&arg_idx) = type_args_list.nodes.get(i)
                    && self.is_infer_type_node_through_parens(arg_idx)
                {
                    continue;
                }

                // Skip constraint checking for `this` type arguments or types that
                // contain `this` (e.g., `this['params']`). The polymorphic `this` type
                // is type-parameter-like and its concrete type is only known at
                // instantiation time. TSC defers constraint validation for `this`
                // references, so we must skip them to avoid false TS2344 errors.
                // Example: `interface TObject<T> extends TSchema { static: Reduce<T, this['params']> }`
                // where `Reduce<T, P extends unknown[]>` — `this['params']` satisfies
                // `unknown[]` because `TSchema.params: unknown[]`, but we can't prove it
                // structurally at definition time without resolving `this`.
                if query::is_this_type(self.ctx.types.as_type_database(), type_arg)
                    || crate::query_boundaries::common::contains_this_type(
                        self.ctx.types.as_type_database(),
                        type_arg,
                    )
                {
                    continue;
                }

                // When the type argument contains type parameters, we generally skip
                // constraint checking (deferred to instantiation time). However, when
                // the type arg IS a bare type parameter, check its base constraint
                // against the required constraint. This matches tsc: `U extends number`
                // used as `T extends string` → TS2344 because `number` is not
                // assignable to `string`.
                let type_arg_contains_type_parameters =
                    query::contains_type_parameters(self.ctx.types, type_arg);
                let mut base_constraint_from_indexed_access_ast = false;
                let mut base_constraint_type = type_arg_contains_type_parameters
                    .then(|| self.constraint_check_base_type(type_arg))
                    .filter(|&base| base != type_arg)
                    // Discard degenerate base constraints (undefined, null, never)
                    // that arise from incomplete evaluation of composite generic types
                    // like NonNullable<T["states"]>[K]. These are artifacts of the
                    // base-constraint resolution failing to see through type-level
                    // applications (NonNullable, Extract, etc.) and should not be used
                    // to make eager TS2344 decisions.
                    .filter(|&base| {
                        base != TypeId::UNDEFINED
                            && base != TypeId::NULL
                            && base != TypeId::NEVER
                            && base != TypeId::VOID
                    });
                if type_arg_contains_type_parameters
                    && base_constraint_type
                        .is_none_or(|base| query::contains_type_parameters(self.ctx.types, base))
                    && let Some(&arg_idx) = type_args_list.nodes.get(i)
                    && let Some(ast_base) =
                        self.ast_indexed_access_property_union_from_declaration(type_arg, arg_idx)
                {
                    base_constraint_type = Some(ast_base);
                    base_constraint_from_indexed_access_ast = true;
                }
                if type_arg_contains_type_parameters {
                    let is_bare_type_param =
                        query::is_bare_type_parameter(self.ctx.types.as_type_database(), type_arg);
                    if !is_bare_type_param {
                        // Composite type with type parameters (e.g., `T[K]`, `GetProps<C>`,
                        // `Parameters<Target[K]>`). Prefer checking against its resolved
                        // base constraint when one exists; otherwise defer to instantiation
                        // time. This matches tsc for generic indexed-access cases like
                        // `ReturnType<DataFetchFns[T][F]>` while still avoiding false
                        // positives for unconstrained composite generics.
                        if let Some(base) = base_constraint_type
                            && base != TypeId::UNKNOWN
                            && base != type_arg
                        {
                            let mut base = base;
                            if !base_constraint_from_indexed_access_ast
                                && query::contains_free_type_parameters(self.ctx.types, base)
                                && let Some(concrete_indexed_base) =
                                    self.concrete_indexed_access_property_union(base)
                            {
                                base = concrete_indexed_base;
                            }
                            // Base constraint still contains type parameters.
                            // For most cases, defer to instantiation time. However,
                            // when the required constraint is a callable signature
                            // (e.g. `(...args: any) => any` for `ReturnType<T>`),
                            // tsc eagerly reports TS2344 if the base type is not
                            // provably callable (e.g. generic indexed access types
                            // like `DataFetchFns[T][F]` are not callable). This
                            // matches tsc behavior for ReturnType/Parameters/etc.
                            if !base_constraint_from_indexed_access_ast
                                && query::contains_free_type_parameters(self.ctx.types, base)
                            {
                                let constraint_resolved = self.resolve_lazy_type(constraint);
                                let db = self.ctx.types.as_type_database();

                                // Check if the base is a conditional type whose extends
                                // type satisfies the constraint. This check applies
                                // regardless of whether the constraint is callable.
                                // For `Extract<T, C>` (= `T extends C ? T : never`),
                                // the result is always a subtype of C, so if C satisfies
                                // the constraint, skip. If C does NOT satisfy, emit TS2344.
                                //
                                // IMPORTANT: Only apply the eager extends-type check when
                                // the conditional is truly Extract-like (true_type ==
                                // check_type). For general conditionals like
                                // `T extends object ? { [K in keyof T]: T[K] } : never`,
                                // the true branch is a different type from the check type,
                                // so the extends type is NOT a reliable proxy for the
                                // result. Defer those to instantiation time.
                                if let Some((cond_check, cond_extends, cond_true, cond_false)) =
                                    query::full_conditional_type_components(
                                        self.ctx.types.as_type_database(),
                                        base,
                                    )
                                {
                                    if cond_false == TypeId::NEVER {
                                        // Determine if this conditional is "Extract-like":
                                        // the extends type can serve as a proxy for the result.
                                        //
                                        // Extract-like cases (check extends type vs constraint):
                                        // - `T extends C ? T : never` (true == check, classic Extract)
                                        // - `C extends X<infer P> ? P : never` (true is bare type param,
                                        //   opaque — no structural guarantee, e.g. GetProps<C>)
                                        //
                                        // Non-Extract cases (defer to instantiation):
                                        // - `S extends object ? { [K in keyof S]: S[K] } : never`
                                        //   (true branch structurally derived from check type)
                                        // - `T[K] extends Function ? K : never` where K is the
                                        //   index of the check type (key-filtering pattern like
                                        //   FunctionPropertyNames<T>). The result K is always a
                                        //   subtype of keyof T, but the extends type (Function)
                                        //   is not a proxy for this relationship. Defer.
                                        let cond_true_is_bare_param = query::is_bare_type_parameter(
                                            self.ctx.types.as_type_database(),
                                            cond_true,
                                        );
                                        // When the true branch is a bare type param AND the check
                                        // type is an indexed access containing that param as its
                                        // index, this is a key-filtering pattern, not Extract-like.
                                        // Example: `{ [K in keyof T]: T[K] extends Fn ? K : never }[keyof T]`
                                        // Here cond_check = T[K], cond_true = K. K is always a
                                        // key of T, so the result satisfies `keyof T` by construction.
                                        // Deferring avoids false TS2344 for Pick<T, FilteredKeys<T>>.
                                        let is_key_filtering_pattern = cond_true_is_bare_param && {
                                            let db = self.ctx.types.as_type_database();
                                            if let Some((_obj, idx)) =
                                                query::index_access_components(db, cond_check)
                                            {
                                                idx == cond_true
                                            } else {
                                                false
                                            }
                                        };
                                        // When the true branch is an `infer` variable
                                        // (e.g., `F extends (...args: infer L) => any ? L : never`),
                                        // the result is structurally extracted from the extends type
                                        // pattern, not bounded by it. tsc's `getBaseConstraintOfType`
                                        // for such conditionals returns the base constraint of the
                                        // infer variable — `unknown` for unconstrained infer, or the
                                        // explicit constraint for `infer R extends C`. Since `unknown`
                                        // is not assignable to any non-trivial constraint, tsc emits
                                        // TS2344 eagerly. Match that behavior here.
                                        let cond_true_is_infer = query::is_infer_type(
                                            self.ctx.types.as_type_database(),
                                            cond_true,
                                        );
                                        if cond_true_is_infer && !is_key_filtering_pattern {
                                            // Get the infer variable's own constraint (if any).
                                            // Unconstrained infer → base is `unknown`.
                                            let infer_base = query::get_type_parameter_constraint(
                                                self.ctx.types.as_type_database(),
                                                cond_true,
                                            )
                                            .unwrap_or(TypeId::UNKNOWN);

                                            // Instantiate the required constraint with provided
                                            // type arguments for accurate error messages.
                                            let mut subst =
                                                crate::query_boundaries::common::TypeSubstitution::new(
                                                );
                                            for (j, p) in type_params.iter().enumerate() {
                                                if let Some(&arg) = type_args.get(j) {
                                                    subst.insert(p.name, arg);
                                                }
                                            }
                                            let inst_constraint = if subst.is_empty() {
                                                constraint_resolved
                                            } else {
                                                crate::query_boundaries::common::instantiate_type(
                                                    self.ctx.types,
                                                    constraint_resolved,
                                                    &subst,
                                                )
                                            };

                                            // When the instantiated constraint is fully
                                            // concrete (no type parameters), tsc can use
                                            // "restrictive instantiation" — substituting
                                            // the check type's constraint into the
                                            // conditional to resolve the infer variable.
                                            // This often proves satisfaction (e.g.,
                                            // `Parameters<F>` where `F extends Function`
                                            // resolves infer to `any[]` which satisfies
                                            // `ReadonlyArray<any>`). We can't easily
                                            // replicate this, so defer for concrete
                                            // constraints.
                                            if !query::contains_type_parameters(
                                                self.ctx.types,
                                                inst_constraint,
                                            ) {
                                                continue;
                                            }

                                            // Constraint still has type parameters (e.g.,
                                            // self-referential `Shared<T, GetProps<C>>`).
                                            // Check if the infer base satisfies it.
                                            // For unconstrained infer (base=unknown), this
                                            // fails → TS2344.
                                            let is_satisfied = inst_constraint == TypeId::UNKNOWN
                                                || inst_constraint == TypeId::ANY
                                                || self
                                                    .is_assignable_to(infer_base, inst_constraint);

                                            if !is_satisfied
                                                && let Some(&arg_idx) = type_args_list.nodes.get(i)
                                                && !self
                                                    .type_argument_is_narrowed_by_conditional_true_branch(
                                                        arg_idx,
                                                        inst_constraint,
                                                    )
                                            {
                                                self.error_type_constraint_not_satisfied(
                                                    type_arg,
                                                    inst_constraint,
                                                    arg_idx,
                                                );
                                            }
                                            continue;
                                        }
                                        let is_extract_like = cond_true == cond_check
                                            || (cond_true_is_bare_param
                                                && !is_key_filtering_pattern);
                                        if !is_extract_like {
                                            // True branch is a structural type derived from the
                                            // check type (e.g., mapped type). Constraint satisfaction
                                            // depends on the structure, not the extends type.
                                            // Defer to instantiation time.
                                            continue;
                                        }
                                        let ext_resolved = self.resolve_lazy_type(cond_extends);
                                        let ext_evaluated =
                                            self.evaluate_type_for_assignability(ext_resolved);
                                        if self.is_assignable_to(ext_evaluated, constraint_resolved)
                                            || self
                                                .is_assignable_to(ext_resolved, constraint_resolved)
                                        {
                                            continue;
                                        }
                                        // Extract-like pattern (? T : never) but the
                                        // extends type does NOT satisfy the constraint. tsc
                                        // reports TS2344 in this case. Instantiate constraint
                                        // with type args for accurate error messages.
                                        let mut subst =
                                            crate::query_boundaries::common::TypeSubstitution::new(
                                            );
                                        for (j, p) in type_params.iter().enumerate() {
                                            if let Some(&arg) = type_args.get(j) {
                                                subst.insert(p.name, arg);
                                            }
                                        }
                                        let inst_constraint = if subst.is_empty() {
                                            constraint_resolved
                                        } else {
                                            crate::query_boundaries::common::instantiate_type(
                                                self.ctx.types,
                                                constraint_resolved,
                                                &subst,
                                            )
                                        };
                                        if let Some(&arg_idx) = type_args_list.nodes.get(i)
                                            && !self
                                                .type_argument_is_narrowed_by_conditional_true_branch(
                                                    arg_idx,
                                                    inst_constraint,
                                                )
                                        {
                                            self.error_type_constraint_not_satisfied(
                                                type_arg,
                                                inst_constraint,
                                                arg_idx,
                                            );
                                        }
                                        continue;
                                    } else {
                                        // General conditional with type params — defer
                                        // to instantiation time, matching tsc behavior.
                                        continue;
                                    }
                                }

                                let constraint_is_callable =
                                    query::is_callable_type(db, constraint_resolved);
                                if !constraint_is_callable {
                                    continue;
                                }
                                // Constraint is callable — check if base is callable too.
                                // If base still has type params and is not callable, emit TS2344.
                                // Also try evaluating the base (e.g., mapped type indexed access
                                // like `FunctionsObj<T>[keyof T]` → `() => unknown`).
                                //
                                // Special case: when the type argument is Application(TypeQuery(sym), args)
                                // — i.e., `typeof fn<Args>` — the base constraint resolved to the
                                // underlying function type by evaluating through the TypeQuery. But
                                // Special case: when the type argument is `typeof fn<Args>` (an
                                // instantiation expression), check if the type arguments match
                                // any signature's arity. If they don't (TS2635), the instantiation
                                // failed and the result is NOT callable — tsc treats it as errorType.
                                // The base constraint resolves to the underlying function type which
                                // IS callable, but that's misleading since the Application itself
                                // is invalid.
                                let is_failed_instantiation = query::typeof_instantiation_arg_count(
                                    self.ctx.types.as_type_database(),
                                    type_arg,
                                )
                                .is_some_and(|num_args| {
                                    // Check if the base (resolved function type) has any signature
                                    // with matching arity.
                                    let call_sigs =
                                        crate::query_boundaries::common::call_signatures_for_type(
                                            db, base,
                                        );
                                    let construct_sigs =
                                        crate::query_boundaries::common::construct_signatures_for_type(
                                            db, base,
                                        );
                                    let mut has_match = false;
                                    if let Some(sigs) = &call_sigs {
                                        has_match = sigs
                                            .iter()
                                            .any(|sig| sig.type_params.len() == num_args);
                                    }
                                    if !has_match
                                        && let Some(sigs) = &construct_sigs {
                                            has_match = sigs
                                                .iter()
                                                .any(|sig| sig.type_params.len() == num_args);
                                        }
                                    !has_match
                                });
                                let base_is_callable =
                                    query::is_callable_type(db, base) && !is_failed_instantiation;
                                if base_is_callable {
                                    // Base is callable even with type params — satisfied.
                                    continue;
                                }
                                // When the base is an indexed access into a mapped type
                                // (e.g., `{ [K in keyof T]: () => unknown }[keyof T]`),
                                // the template type gives the actual value type. If the
                                // template is callable, the indexed access is callable.
                                if let Some((obj, _idx)) = query::index_access_components(db, base)
                                    && let Some(mapped_id) = query::mapped_type_id(db, obj)
                                    && query::is_mapped_template_callable(db, mapped_id)
                                {
                                    continue;
                                }
                                // Try evaluating base further — indexed access through mapped
                                // types may resolve to a callable template type.
                                let base_evaluated = self.evaluate_type_for_assignability(base);
                                if base_evaluated != base {
                                    let base_eval_callable = query::is_callable_type(
                                        self.ctx.types.as_type_database(),
                                        base_evaluated,
                                    ) || query::callable_shape_for_type(
                                        self.ctx.types.as_type_database(),
                                        base_evaluated,
                                    )
                                    .is_some();
                                    if base_eval_callable {
                                        continue;
                                    }
                                }
                                // Check if base is a mapped type whose template is callable.
                                // For `{ [K in keyof T]: () => unknown }`, the template
                                // `() => unknown` is callable, so indexing yields a callable type.
                                if let Some(template) = query::mapped_type_template(db, base) {
                                    let template_evaluated =
                                        self.evaluate_type_for_assignability(template);
                                    let template_callable = query::is_callable_type(
                                        self.ctx.types.as_type_database(),
                                        template_evaluated,
                                    ) || query::callable_shape_for_type(
                                        self.ctx.types.as_type_database(),
                                        template_evaluated,
                                    )
                                    .is_some();
                                    if template_callable {
                                        continue;
                                    }
                                }
                                // When the base is an indexed access into a type
                                // parameter (e.g., `FuncMap[keyof FuncMap]`), we cannot
                                // determine callability at definition time. The type
                                // parameter's constraint may guarantee callable values
                                // (e.g., `FuncMap extends Record<string, Function>`),
                                // but we can't fully resolve this without instantiation.
                                // Defer to instantiation time to avoid false TS2344.
                                if let Some((obj, _idx)) = query::index_access_components(db, base)
                                    && query::is_bare_type_parameter(db, obj)
                                {
                                    continue;
                                }
                                // Base is not callable and constraint is callable → TS2344.
                                if let Some(&arg_idx) = type_args_list.nodes.get(i)
                                    && !self.type_argument_is_narrowed_by_conditional_true_branch(
                                        arg_idx,
                                        constraint_resolved,
                                    )
                                {
                                    self.error_type_constraint_not_satisfied(
                                        type_arg,
                                        constraint_resolved,
                                        arg_idx,
                                    );
                                }
                                continue;
                            }
                            // When the type argument is an Application type
                            // (e.g., `Merge2<X>`, `Same<U>`) containing type
                            // parameters, the base constraint was obtained by
                            // eagerly evaluating the application with type
                            // parameter constraints substituted. This may
                            // produce a concrete type that doesn't accurately
                            // represent the actual type at instantiation time
                            // (e.g., mapped types like `{ [P in keyof T]: T[P] }`
                            // preserve index signatures from T, but the eagerly-
                            // resolved base may lose this relationship). TSC
                            // defers constraint checking for such Application
                            // types to instantiation time.
                            if query::is_application_type(
                                self.ctx.types.as_type_database(),
                                type_arg,
                            ) {
                                // Keep eager checking for callable constraints when
                                // the application evaluates to a generic indexed-access
                                // form (e.g., `Alias<T, F>` -> `DataFetchFns[T][F]`).
                                // tsc reports TS2344 in this case because callability
                                // is not provable at definition time.
                                let constraint_resolved = self.resolve_lazy_type(constraint);
                                let constraint_is_callable = query::is_callable_type(
                                    self.ctx.types.as_type_database(),
                                    constraint_resolved,
                                ) || self.is_function_constraint(
                                    param.constraint.unwrap_or(TypeId::NEVER),
                                );
                                let generic_indexed_type_arg =
                                    self.generic_indexed_access_subject(type_arg);
                                let keep_eager_check = constraint_is_callable
                                    && generic_indexed_type_arg.is_some()
                                    && !self.indexed_access_resolves_to_callable(
                                        generic_indexed_type_arg.unwrap_or(type_arg),
                                    );
                                if !keep_eager_check {
                                    continue;
                                }
                            }
                            let constraint_resolved = self.resolve_lazy_type(constraint);
                            let mut subst =
                                crate::query_boundaries::common::TypeSubstitution::new();
                            for (j, p) in type_params.iter().enumerate() {
                                if let Some(&arg) = type_args.get(j) {
                                    subst.insert(p.name, arg);
                                }
                            }
                            let inst_constraint = if subst.is_empty() {
                                constraint_resolved
                            } else {
                                crate::query_boundaries::common::instantiate_type(
                                    self.ctx.types,
                                    constraint_resolved,
                                    &subst,
                                )
                            };
                            if query::contains_free_type_parameters(self.ctx.types, inst_constraint)
                            {
                                continue;
                            }
                            let db = self.ctx.types.as_type_database();
                            let original_constraint = param.constraint.unwrap_or(TypeId::NEVER);
                            let generic_indexed_type_arg =
                                self.generic_indexed_access_subject(type_arg);

                            // Special case: tsc eagerly reports TS2344 for generic indexed access
                            // types (A[B] where A contains type params) when the constraint is
                            // callable, even if the evaluated base constraint is callable.
                            // Example: `ReturnType<DataFetchFns[T][F]>` → TS2344 because
                            // `DataFetchFns[T][F]` is not provably callable (T is free).
                            // By contrast, `ReturnType<DataFetchFns['Boat'][F]>` → no TS2344
                            // because 'Boat' is concrete and all its values are callable.
                            let constraint_is_callable =
                                query::is_callable_type(db, inst_constraint)
                                    || self.is_function_constraint(original_constraint);
                            if constraint_is_callable
                                && generic_indexed_type_arg.is_some()
                                && !self.indexed_access_resolves_to_callable(
                                    generic_indexed_type_arg.unwrap_or(type_arg),
                                )
                                && let Some(&arg_idx) = type_args_list.nodes.get(i)
                                && !self.type_argument_is_narrowed_by_conditional_true_branch(
                                    arg_idx,
                                    inst_constraint,
                                )
                            {
                                self.error_type_constraint_not_satisfied(
                                    type_arg,
                                    inst_constraint,
                                    arg_idx,
                                );
                                continue;
                            }

                            // When the base constraint has no type parameters but the
                            // original type argument did, constraint resolution fully
                            // substituted type params with their constraints. This
                            // substitution is lossy — mapped types and intersections
                            // may lose index signature relationships that hold at
                            // instantiation time (e.g., `{ [P in keyof T]: T[P] }`
                            // preserves T's index signatures, but constraint resolution
                            // may produce inconsistent index signatures). For non-callable
                            // constraints, defer to instantiation time to match tsc.
                            if !query::contains_free_type_parameters(self.ctx.types, base)
                                && !query::is_callable_type(db, inst_constraint)
                                && !self.is_function_constraint(original_constraint)
                            {
                                // Check if the type arg is an Application type (type alias
                                // instantiation). These are especially prone to lossy
                                // constraint resolution because the type alias body may
                                // structurally preserve constraints that the base constraint
                                // computation cannot track.
                                let type_arg_is_application = query::application_base_def_id(
                                    self.ctx.types.as_type_database(),
                                    type_arg,
                                )
                                .is_some();
                                if type_arg_is_application {
                                    continue;
                                }
                            }

                            let mut is_satisfied = self.is_assignable_to(base, inst_constraint)
                                || self.satisfies_array_like_constraint(base, inst_constraint);
                            if !is_satisfied {
                                // When the constraint is a function type (e.g., `(...args: any) => any`),
                                // accept any callable base type. For type parameters with callable
                                // constraints (e.g., `F extends Function`), check the constraint.
                                // Also check the structural Function interface pattern (apply/call/bind)
                                // since Function may be lowered as an Object without call signatures.
                                let is_fn_constraint = self
                                    .is_function_constraint(original_constraint)
                                    || query::is_callable_type(db, original_constraint);
                                let base_is_callable = query::is_callable_type(db, base)
                                    || self.type_parameter_has_callable_constraint(base)
                                    || self.is_function_constraint(base)
                                    || query::is_function_interface_structural(db, base);
                                is_satisfied = is_fn_constraint && base_is_callable;
                            }
                            if !is_satisfied && let Some(&arg_idx) = type_args_list.nodes.get(i) {
                                if self.type_argument_is_narrowed_by_conditional_true_branch(
                                    arg_idx,
                                    inst_constraint,
                                ) {
                                    continue;
                                }
                                self.error_type_constraint_not_satisfied(
                                    type_arg,
                                    inst_constraint,
                                    arg_idx,
                                );
                            }
                        }
                        // When base_constraint_type is None (composite type with type params
                        // that can't be simplified further), check if the required constraint
                        // is callable. Tsc eagerly emits TS2344 when the constraint is a
                        // callable signature and the composite type arg is not provably callable.
                        // Example: `ReturnType<TypeHardcodedAsParameterWithoutReturnType<T,F>>`
                        // where `TypeHardcodedAsParameterWithoutReturnType<T,F>` = `DataFetchFns[T][F]`.
                        //
                        // The constraint TypeId may come from a lib arena (cross-arena). Resolve
                        // it fully and evaluate before checking callability.
                        if base_constraint_type.is_none() {
                            // When the type argument is (or evaluates to) a conditional
                            // type like `Extract<T, C>` (= `T extends C ? T : never`),
                            // the result is always a subtype of C (or never). If C
                            // satisfies the required constraint, skip TS2344.
                            // Also handles Application types like `Extract<T, C>` that
                            // evaluate to conditional types.
                            let db = self.ctx.types.as_type_database();
                            let type_arg_evaluated = self.evaluate_type_for_assignability(type_arg);
                            let cond_components = query::conditional_type_components(db, type_arg)
                                .or_else(|| {
                                    query::conditional_type_components(
                                        self.ctx.types.as_type_database(),
                                        type_arg_evaluated,
                                    )
                                });
                            if let Some((extends_type, false_type)) = cond_components {
                                let constraint_resolved = self.resolve_lazy_type(constraint);
                                let extends_resolved = self.resolve_lazy_type(extends_type);
                                let extends_evaluated =
                                    self.evaluate_type_for_assignability(extends_resolved);
                                // If false branch is `never` (Extract pattern) and the
                                // extends type satisfies the constraint, skip TS2344.
                                if false_type == TypeId::NEVER
                                    && (self
                                        .is_assignable_to(extends_evaluated, constraint_resolved)
                                        || self.is_assignable_to(
                                            extends_resolved,
                                            constraint_resolved,
                                        ))
                                {
                                    // Skip: Extract<T, C> always produces subtype of C
                                } else {
                                    // General conditional: defer to instantiation when
                                    // the type argument has unresolved type parameters.
                                    // tsc defers constraint checks for conditional types
                                    // with free type variables.
                                }
                            } else {
                                let constraint_resolved = self.resolve_lazy_type(constraint);
                                // Also try evaluating the constraint in case it's a lazy reference
                                // to a function type from the lib (e.g., `(...args: any) => any`).
                                let constraint_evaluated =
                                    self.evaluate_type_for_assignability(constraint_resolved);
                                let constraint_is_callable =
                                    query::is_callable_type(db, constraint_resolved)
                                        || query::is_callable_type(db, constraint_evaluated)
                                        || self.is_function_constraint(constraint)
                                        || self.is_function_constraint(constraint_resolved);
                                // For indexed access types like `T[M]` where T's constraint
                                // is a mapped type with a callable template, the indexed
                                // access result is callable — skip TS2344.
                                // Example: `ReturnType<T[M]>` where
                                // `T extends { [K in keyof T]: () => unknown }`.
                                let type_arg_is_callable_via_mapped = constraint_is_callable
                                    && self.indexed_access_resolves_to_callable(type_arg);
                                // When the type arg is an indexed access into a type
                                // parameter (e.g., `FuncMap[P]`), the result type depends
                                // on the type parameter's actual type at instantiation
                                // time. We cannot determine callability at definition
                                // time — defer to instantiation to avoid false TS2344.
                                let type_arg_is_indexed_into_type_param = {
                                    let db2 = self.ctx.types.as_type_database();
                                    query::index_access_components(db2, type_arg).is_some_and(
                                        |(obj, _)| query::is_bare_type_parameter(db2, obj),
                                    )
                                };
                                if constraint_is_callable
                                    && !type_arg_is_callable_via_mapped
                                    && !type_arg_is_indexed_into_type_param
                                    && !query::is_callable_type(db, type_arg)
                                    && query::callable_shape_for_type(db, type_arg).is_none()
                                    && let Some(&arg_idx) = type_args_list.nodes.get(i)
                                    && !self.type_argument_is_narrowed_by_conditional_true_branch(
                                        arg_idx,
                                        constraint_resolved,
                                    )
                                {
                                    self.error_type_constraint_not_satisfied(
                                        type_arg,
                                        constraint_resolved,
                                        arg_idx,
                                    );
                                }
                            }
                        }
                        continue;
                    }
                    if is_bare_type_param && base_constraint_type.is_none() {
                        // Bare `Infer` type parameter — base_constraint_of_type returns
                        // the type unchanged for Infer types, so base_constraint_type is
                        // None. Check if the infer variable has an implicit constraint
                        // from its structural position (e.g., template literal → string,
                        // rest element → array). If so, skip TS2344 — tsc defers these
                        // checks to conditional type evaluation.
                        let has_implicit_constraint =
                            type_args_list.nodes.get(i).copied().is_some_and(|arg_idx| {
                                self.has_hidden_conditional_infer_constraint_local(arg_idx)
                            });
                        if has_implicit_constraint {
                            continue;
                        }
                    }
                    if is_bare_type_param && let Some(base) = base_constraint_type {
                        // Bare type parameter — check its base constraint instead of
                        // eagerly validating the unresolved type parameter itself.
                        if base == TypeId::UNKNOWN {
                            // Base constraint is UNKNOWN. This can mean either:
                            // (a) The type param is truly unconstrained (no `extends`)
                            // (b) The constraint wasn't resolved (cross-arena,
                            //     function type params, mapped type keys, etc.)
                            //
                            // For case (a), tsc reports TS2344 when the required
                            // constraint is non-trivial. For case (b), we must
                            // skip to avoid false positives.
                            //
                            // Detect case (b) by checking if the type arg's AST
                            // source has an explicit constraint or is inside a
                            // mapped type body (implicit constraint).
                            let has_hidden_constraint =
                                type_args_list.nodes.get(i).copied().is_some_and(|arg_idx| {
                                    self.is_inside_mapped_type(arg_idx)
                                        || self.type_arg_has_explicit_constraint_in_ast(arg_idx)
                                        || self
                                            .has_hidden_conditional_infer_constraint_local(arg_idx)
                                });
                            if has_hidden_constraint {
                                continue;
                            }

                            let constraint_resolved = self.resolve_lazy_type(constraint);
                            let mut subst =
                                crate::query_boundaries::common::TypeSubstitution::new();
                            for (j, p) in type_params.iter().enumerate() {
                                if let Some(&arg) = type_args.get(j) {
                                    subst.insert(p.name, arg);
                                }
                            }
                            let inst_constraint = if subst.is_empty() {
                                constraint_resolved
                            } else {
                                crate::query_boundaries::common::instantiate_type(
                                    self.ctx.types,
                                    constraint_resolved,
                                    &subst,
                                )
                            };
                            // Skip trivial constraints (unknown/any) and bare type
                            // parameter constraints (deferred to instantiation).
                            let is_checkable = inst_constraint != TypeId::UNKNOWN
                                && inst_constraint != TypeId::ANY
                                && !query::is_bare_type_parameter(
                                    self.ctx.types.as_type_database(),
                                    inst_constraint,
                                );
                            if is_checkable
                                && !self.is_assignable_to(base, inst_constraint)
                                && !self.satisfies_array_like_constraint(base, inst_constraint)
                                && let Some(&arg_idx) = type_args_list.nodes.get(i)
                                && !self.type_argument_is_narrowed_by_conditional_true_branch(
                                    arg_idx,
                                    inst_constraint,
                                )
                            {
                                self.error_type_constraint_not_satisfied(
                                    type_arg,
                                    inst_constraint,
                                    arg_idx,
                                );
                            }
                            continue;
                        }
                        // When the base constraint is a union, only skip if the type
                        // arg is inside a conditional type's FALSE branch where it
                        // could be narrowed by exclusion (Exclude<T, extends>). In
                        // true branches and non-conditional contexts, proceed with
                        // the constraint check.
                        if query::has_union_members(self.ctx.types.as_type_database(), base) {
                            let defer_for_conditional =
                                type_args_list.nodes.get(i).is_some_and(|&arg_idx| {
                                    self.type_arg_is_in_conditional_false_branch_of_check_type(
                                        arg_idx,
                                    )
                                });
                            if defer_for_conditional {
                                continue;
                            }
                            // Fall through to perform the constraint check
                        }
                        if query::contains_free_type_parameters(self.ctx.types, base) {
                            // Base constraint itself contains free type parameters
                            // (e.g., from outer generic scope). Defer check.
                            // Uses free-type-param check to avoid false positives
                            // from bound type params inside method signatures
                            // (e.g., `interface Base { bar<W>(): Inner<W> }` —
                            // W is bound by bar, not free in Base).
                            continue;
                        }
                        let constraint_resolved = self.resolve_lazy_type(constraint);
                        if query::contains_type_parameters(self.ctx.types, constraint_resolved) {
                            continue;
                        }
                        // Instantiate the constraint with all provided type arguments
                        let mut subst = crate::query_boundaries::common::TypeSubstitution::new();
                        for (j, p) in type_params.iter().enumerate() {
                            if let Some(&arg) = type_args.get(j) {
                                subst.insert(p.name, arg);
                            }
                        }
                        let inst_constraint = if subst.is_empty() {
                            constraint_resolved
                        } else {
                            crate::query_boundaries::common::instantiate_type(
                                self.ctx.types,
                                constraint_resolved,
                                &subst,
                            )
                        };
                        if query::contains_type_parameters(self.ctx.types, inst_constraint) {
                            continue;
                        }
                        // Evaluate indexed access / keyof types in the constraint
                        // before checking. E.g., `WeakKeyTypes[keyof WeakKeyTypes]`
                        // must be reduced to `object | symbol` for the assignability
                        // check to work correctly.
                        //
                        // Ensure lazy refs inside the constraint are resolved in the
                        // type environment BEFORE evaluation. Without this, constraints
                        // like `WeakKeyTypes[keyof WeakKeyTypes]` (where WeakKeyTypes is
                        // a Lazy(DefId) from a lib file) remain unevaluated because the
                        // evaluator's `ensure_relation_input_ready` may be skipped due
                        // to depth guards during nested evaluation.
                        self.ensure_refs_resolved(inst_constraint);
                        let inst_constraint = self.evaluate_type_for_assignability(inst_constraint);
                        let mut is_satisfied = self.is_assignable_to(base, inst_constraint)
                            || self.satisfies_array_like_constraint(base, inst_constraint);
                        if !is_satisfied {
                            // When the constraint is a function type, accept callable bases.
                            // The `Function` interface may be lowered as an Object type
                            // (without call signatures), so also check for the structural
                            // pattern (apply/call/bind properties).
                            let db2 = self.ctx.types.as_type_database();
                            let is_fn_constraint = self.is_function_constraint(inst_constraint)
                                || query::is_callable_type(db2, inst_constraint);
                            let base_is_callable = query::is_callable_type(db2, base)
                                || self.type_parameter_has_callable_constraint(base)
                                || self.is_function_constraint(base)
                                || query::is_function_interface_structural(db2, base);
                            is_satisfied = is_fn_constraint && base_is_callable;
                        }
                        if !is_satisfied && let Some(&arg_idx) = type_args_list.nodes.get(i) {
                            if self.type_argument_is_narrowed_by_conditional_true_branch(
                                arg_idx,
                                inst_constraint,
                            ) {
                                continue;
                            }
                            self.error_type_constraint_not_satisfied(
                                type_arg,
                                inst_constraint,
                                arg_idx,
                            );
                        }
                        continue;
                    }
                }

                // Resolve the constraint in case it's a Lazy type
                let constraint = self.resolve_lazy_type(constraint);

                if let Some(&arg_idx) = type_args_list.nodes.get(i)
                    && self
                        .type_argument_is_narrowed_by_conditional_true_branch(arg_idx, constraint)
                {
                    continue;
                }

                // Evaluate type arguments before substitution so that unevaluated
                // IndexAccess types (e.g., `SettingsTypes["audio" | "video"]`) are
                // resolved to their concrete types. This prevents the instantiated
                // constraint from containing unresolvable Lazy(DefId) references
                // inside nested types (KeyOf, IndexAccess, Mapped).
                let mut subst = crate::query_boundaries::common::TypeSubstitution::new();
                for (j, p) in type_params.iter().enumerate() {
                    if let Some(&arg) = type_args.get(j) {
                        let evaluated_arg = self.evaluate_type_with_env(arg);
                        subst.insert(p.name, evaluated_arg);
                    }
                }
                let instantiated_constraint = if subst.is_empty() {
                    constraint
                } else {
                    crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        constraint,
                        &subst,
                    )
                };
                // Skip if the instantiated constraint still contains type parameters.
                // This avoids false positive TS2344 when the constraint cannot be fully
                // resolved (e.g., conditional type narrowing contexts like
                // `Parameters<Target[K]>` inside a `Target[K] extends Function` branch).
                if query::contains_type_parameters(self.ctx.types, instantiated_constraint) {
                    continue;
                }

                // When the constraint is an object type with ONLY optional properties
                // (a "weak type" like `{t?: string}`), primitive types always satisfy
                // it in tsc (e.g., `bigint extends {t?: string}` is valid). However,
                // non-primitive types that share no common properties should fail
                // with TS2559 ("Type has no properties in common").
                let constraint_is_all_optional = {
                    let db = self.ctx.types.as_type_database();
                    if let Some(shape_id) = crate::query_boundaries::common::object_shape_id(
                        db,
                        instantiated_constraint,
                    ) {
                        let shape = db.object_shape(shape_id);
                        !shape.properties.is_empty()
                            && shape.properties.iter().all(|p| p.optional)
                            && shape.string_index.is_none()
                            && shape.number_index.is_none()
                    } else {
                        false
                    }
                };
                // Only skip for primitives: they always satisfy weak type constraints.
                // Non-primitive types must still go through assignability to detect
                // TS2559 (no common properties).
                let primitive_satisfies_weak = constraint_is_all_optional
                    && query::is_primitive_type(self.ctx.types.as_type_database(), type_arg);
                // When the constraint is a weak type (all-optional) and the type arg
                // is NOT primitive, use assignability WITH weak type checks so that
                // TS2559 is emitted when source has no common properties with the
                // constraint. Without this, `{x: string}` would pass against
                // `{y?: string}` structurally (all target props optional) but miss
                // the weak type violation.
                let mut is_satisfied = primitive_satisfies_weak
                    || if constraint_is_all_optional
                        && !query::is_primitive_type(self.ctx.types.as_type_database(), type_arg)
                    {
                        self.is_assignable_to(type_arg, instantiated_constraint)
                    } else {
                        self.is_assignable_to_no_weak_checks(type_arg, instantiated_constraint)
                    };

                // When the constraint is all-optional and the structural check
                // passed (because all-optional types have no required properties),
                // separately check for weak type violation (TS2559).
                // Non-primitive type arguments with NO common properties should
                // fail, e.g., MyObjA {x: string} vs ObjA {y?: string}.
                if is_satisfied && constraint_is_all_optional && !primitive_satisfies_weak {
                    let analysis =
                        self.analyze_assignability_failure(type_arg, instantiated_constraint);
                    if matches!(
                        analysis.failure_reason,
                        Some(tsz_solver::SubtypeFailureReason::NoCommonProperties { .. })
                    ) {
                        is_satisfied = false;
                    }
                }

                // Fallback for recursive generic constraints (coinductive semantics).
                //
                // For self-referential constraints like `T extends AA<T>` in
                // `interface AA<T extends AA<T>>`, checking if a type arg satisfies
                // the constraint leads to circular structural checks that the
                // subtype checker can't resolve (pre-evaluation destroys DefId
                // identity needed for cycle detection).
                //
                // Coinductive fix: if the constraint is an Application of some base
                // interface, and the type arg's interface extends that same base
                // interface (via heritage), the constraint is coinductively satisfied.
                // e.g., for `interface BB extends AA<AA<BB>>`, BB extends AA, so
                // BB satisfies any AA<...> constraint.
                if !is_satisfied {
                    is_satisfied = self
                        .satisfies_recursive_heritage_constraint(type_arg, instantiated_constraint);
                }

                // Fallback: if assignability failed but the constraint is the Function
                // interface and the type argument has call signatures, accept it.
                // This handles the case where Function has multiple TypeIds that
                // aren't recognized as equivalent during assignability checking.
                // IMPORTANT: Use has_call_signatures (not is_callable_type) to reject
                // class constructor types that only have construct signatures.
                // E.g., `Parameters<typeof MyClass>` should emit TS2344 because
                // `typeof MyClass` has construct signatures but no call signatures.
                if !is_satisfied {
                    // Check original (pre-resolution) constraint which may still be
                    // Lazy(DefId), making it easier to identify via boxed DefId lookup.
                    let original_constraint = param.constraint.unwrap_or(TypeId::NEVER);
                    let db = self.ctx.types.as_type_database();
                    is_satisfied = self.is_function_constraint(original_constraint)
                        && query::has_call_signatures(db, type_arg);
                }
                if !is_satisfied {
                    is_satisfied =
                        self.satisfies_array_like_constraint(type_arg, instantiated_constraint);
                }
                if !is_satisfied
                    && let Some(base) = base_constraint_type
                    && base != TypeId::UNKNOWN
                    && !query::contains_free_type_parameters(self.ctx.types, base)
                {
                    is_satisfied = self.is_assignable_to(base, instantiated_constraint)
                        || self.satisfies_array_like_constraint(base, instantiated_constraint);
                }

                if !is_satisfied && let Some(&arg_idx) = type_args_list.nodes.get(i) {
                    if self.type_argument_is_narrowed_by_conditional_true_branch(
                        arg_idx,
                        instantiated_constraint,
                    ) {
                        continue;
                    }
                    // Check if the failure is due to a weak type violation (TS2559).
                    // In tsc, when the constraint is a "weak type" (all-optional properties)
                    // and the type argument shares no common properties, tsc emits TS2559
                    // instead of TS2344. However, primitive types satisfy weak type
                    // constraints in tsc (e.g., `bigint extends {t?: string}` is valid).
                    let analysis =
                        self.analyze_assignability_failure(type_arg, instantiated_constraint);
                    if matches!(
                        analysis.failure_reason,
                        Some(tsz_solver::SubtypeFailureReason::NoCommonProperties { .. })
                    ) {
                        // Primitives satisfy weak type constraints — skip TS2559
                        if !query::is_primitive_type(self.ctx.types.as_type_database(), type_arg) {
                            self.error_no_common_properties_constraint(
                                type_arg,
                                instantiated_constraint,
                                arg_idx,
                            );
                        }
                    } else {
                        self.error_type_constraint_not_satisfied(
                            type_arg,
                            instantiated_constraint,
                            arg_idx,
                        );
                    }
                }
            }
        }
    }

    fn constraint_check_base_type(&mut self, type_id: TypeId) -> TypeId {
        let evaluated = self.evaluate_type_for_assignability(type_id);
        if evaluated != type_id {
            return self.constraint_check_base_type(evaluated);
        }

        let db = self.ctx.types.as_type_database();
        // For TypeParameter: returns constraint or UNKNOWN; for non-TypeParameter: returns type_id
        let base = query::base_constraint_of_type(db, type_id);
        if base == TypeId::UNKNOWN
            && query::is_bare_type_parameter(db, type_id)
            && let Some(name_atom) = query::type_parameter_name(db, type_id)
        {
            let name = self.ctx.types.resolve_atom(name_atom);
            if let Some(&scoped_type_id) = self.ctx.type_parameter_scope.get(&name)
                && scoped_type_id != type_id
            {
                let scoped_base = query::base_constraint_of_type(db, scoped_type_id);
                if scoped_base != TypeId::UNKNOWN && scoped_base != scoped_type_id {
                    return self.constraint_check_base_type(scoped_base);
                }
            }
        }
        if base != type_id {
            let base = self.evaluate_type_for_assignability(base);
            if let Some(keyof_operand) = query::keyof_operand(db, base) {
                // Only normalize `keyof X` when X is a fully concrete type. When
                // X is itself a (free) type parameter, `get_keyof_type` would
                // resolve through X's constraint and return a concrete union of
                // the constraint's keys (e.g., `keyof T` for `T extends unknown[]`
                // becomes `number | "length" | "concat" | ...`). That breaks the
                // upstream `contains_free_type_parameters(base)` deferral, causing
                // false TS2344 on patterns like `{ [K in keyof T]: F<K> }`.
                // Keeping `keyof X` deferred lets the caller defer the constraint
                // check to instantiation time, matching tsc.
                if !query::contains_free_type_parameters(self.ctx.types, keyof_operand) {
                    let normalized = self.get_keyof_type(keyof_operand);
                    if normalized != self.ctx.types.keyof(keyof_operand) {
                        return normalized;
                    }
                }
            }
            return base;
        }
        if let Some((object_type, index_type)) = query::index_access_components(db, type_id) {
            let constrained_object_type =
                if query::is_bare_type_parameter(self.ctx.types.as_type_database(), object_type) {
                    self.constraint_check_base_type(object_type)
                } else {
                    object_type
                };
            let constrained_index_type = self.constraint_check_base_type(index_type);
            let resolved_object_type = if constrained_object_type == TypeId::UNKNOWN {
                object_type
            } else {
                constrained_object_type
            };
            let resolved_index_type = if constrained_index_type == TypeId::UNKNOWN {
                index_type
            } else {
                constrained_index_type
            };
            if let Some(indexed_value_type) = self.constraint_check_indexed_access_value_type(
                resolved_object_type,
                resolved_index_type,
            ) {
                return self.evaluate_type_for_assignability(indexed_value_type);
            }
            if resolved_object_type == object_type && resolved_index_type == index_type {
                type_id
            } else {
                let constrained_access = self
                    .ctx
                    .types
                    .index_access(resolved_object_type, resolved_index_type);
                self.evaluate_type_for_assignability(constrained_access)
            }
        } else {
            type_id
        }
    }

    fn ast_indexed_access_property_union_from_declaration(
        &mut self,
        type_arg: TypeId,
        arg_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<TypeId> {
        let node = self.ctx.arena.get(arg_idx)?;
        let indexed = self.ctx.arena.get_indexed_access_type(node)?;

        let db = self.ctx.types.as_type_database();
        let (object_type, _index_type) = query::index_access_components(db, type_arg)?;
        if matches!(object_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }

        let object_type_for_check = self.evaluate_type_for_assignability(object_type);
        let object_type_for_check = self.resolve_lazy_type(object_type_for_check);
        let index_constraint = self
            .resolve_index_constraint_from_declaration(indexed.index_type, indexed.object_type)?;

        if !self.is_keyof_for_current_object(index_constraint, object_type, object_type_for_check) {
            return None;
        }

        let key_space = if let Some(keyof_operand) = query::keyof_operand(db, index_constraint) {
            self.get_keyof_type(keyof_operand)
        } else {
            self.evaluate_type_for_assignability(index_constraint)
        };
        let key_space = self.resolve_lazy_type(key_space);
        let value_type =
            self.constraint_check_indexed_access_value_type(object_type_for_check, key_space)?;
        let value_type = self.evaluate_type_for_assignability(value_type);
        let value_type = self.resolve_lazy_type(value_type);
        (!query::contains_free_type_parameters(self.ctx.types, value_type)).then_some(value_type)
    }

    fn constraint_check_indexed_access_value_type(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> Option<TypeId> {
        let object_type = self.evaluate_type_for_assignability(object_type);
        let mut object_type = self.resolve_lazy_type(object_type);
        if query::get_object_shape(self.ctx.types.as_type_database(), object_type).is_none()
            && let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(object_type)
        {
            object_type = self.type_reference_symbol_type(sym_id);
            object_type = self.evaluate_type_for_assignability(object_type);
            object_type = self.resolve_lazy_type(object_type);
        }
        let key_type = self.evaluate_type_for_assignability(index_type);
        let key_type = self.resolve_lazy_type(key_type);
        let db = self.ctx.types.as_type_database();
        let key_kind = query::classify_index_key(db, key_type);

        if let Some(shape) = query::get_object_shape(db, object_type) {
            if let Some(index) = &shape.string_index
                && query::key_matches_string_index(db, key_type, &key_kind)
            {
                return Some(index.value_type);
            }
            if let Some(index) = &shape.number_index
                && query::key_matches_number_index(db, key_type, &key_kind)
            {
                return Some(index.value_type);
            }
        }

        if let Some(shape) = query::callable_shape_for_type(db, object_type) {
            if let Some(index) = &shape.string_index
                && query::key_matches_string_index(db, key_type, &key_kind)
            {
                return Some(index.value_type);
            }
            if let Some(index) = &shape.number_index
                && query::key_matches_number_index(db, key_type, &key_kind)
            {
                return Some(index.value_type);
            }
        }

        // For mapped types `{ [K in C]: Template }`, the indexed access value
        // type is the template type. This handles cases like
        // `FunctionsObj<T>[keyof T]` where FunctionsObj is `{ [K in keyof T]: () => unknown }`.
        if let Some(template) = query::mapped_type_template(db, object_type) {
            return Some(template);
        }

        // For concrete object maps like `HTMLElementTagNameMap`, an indexed access
        // `Map[K]` with `K extends keyof Map` has a base constraint equal to the
        // union of all mapped property value types. tsc eagerly uses that union for
        // TS2344 checks on `HTMLCollectionOf<HTMLElementTagNameMap[K]>` /
        // `NodeListOf<HTMLElementTagNameMap[K]>` instead of deferring the relation.
        let keyed_object_type = if query::is_bare_type_parameter(db, key_type) {
            let key_base = self.constraint_check_base_type(key_type);
            if key_base == TypeId::UNKNOWN {
                key_type
            } else {
                key_base
            }
        } else {
            key_type
        };

        if let Some(shape) = query::get_object_shape(db, object_type)
            && !shape.properties.is_empty()
            && let Some(object_keys) =
                crate::query_boundaries::common::keyof_object_properties(db, object_type)
        {
            let keys_assignable = self.is_assignable_to(keyed_object_type, object_keys);
            if !keys_assignable {
                return None;
            }
            let property_types: Vec<TypeId> =
                shape.properties.iter().map(|prop| prop.type_id).collect();
            return match property_types.len() {
                0 => None,
                1 => property_types.first().copied(),
                _ => Some(self.ctx.types.union(property_types)),
            };
        }

        None
    }

    fn concrete_indexed_access_property_union(&mut self, type_id: TypeId) -> Option<TypeId> {
        let evaluated = self.evaluate_type_for_assignability(type_id);
        let evaluated = self.resolve_lazy_type(evaluated);
        let db = self.ctx.types.as_type_database();
        let (object_type, index_type) = query::index_access_components(db, evaluated)?;
        let value_type =
            self.constraint_check_indexed_access_value_type(object_type, index_type)?;
        let value_type = self.evaluate_type_for_assignability(value_type);
        let value_type = self.resolve_lazy_type(value_type);
        (!query::contains_free_type_parameters(self.ctx.types, value_type)).then_some(value_type)
    }

    /// Check if a type represents the global `Function` interface from lib.d.ts.
    ///
    /// Checks via Lazy(DefId) against the interner's registered boxed `DefIds`,
    /// or by direct TypeId match against the interner's registered boxed type.
    fn is_function_constraint(&self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        // Direct match against interner's boxed Function TypeId
        if query::is_boxed_function_type(db, type_id) {
            return true;
        }
        // A function signature type (e.g., `(...args: any) => any`) is also a
        // function constraint. This handles cases like `Parameters<F>` where
        // the constraint is `T extends (...args: any) => any` and F extends Function.
        if query::is_callable_type(db, type_id) {
            return true;
        }
        // Cross-arena DefId equality alone is not strong enough here: imported
        // aliases can reuse a Lazy(DefId) shape that collides with boxed lib
        // DefIds, which falsely classifies unrelated constraints as `Function`.
        // Only accept the boxed-def fallback when the resolved symbol itself is
        // the lib `Function` symbol.
        if !query::is_boxed_function_def(db, type_id) {
            return false;
        }

        let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id) else {
            return false;
        };
        if !self.ctx.symbol_is_from_lib(sym_id) {
            return false;
        }

        let lib_binders = self.get_lib_binders();
        self.ctx
            .binder
            .get_symbol_with_libs(sym_id, &lib_binders)
            .is_some_and(|symbol| symbol.escaped_name == "Function")
    }

    /// Check if a type parameter has a callable constraint (e.g., `F extends Function`).
    /// Used during constraint satisfaction to accept callable type parameters
    /// against function signature constraints.
    fn type_parameter_has_callable_constraint(&self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        if let Some(tp) =
            crate::query_boundaries::type_computation::complex::type_parameter_info(db, type_id)
            && let Some(constraint) = tp.constraint
        {
            return query::is_callable_type(db, constraint)
                || self.is_function_constraint(constraint);
        }
        false
    }

    /// Check if a type is a generic indexed access (`T[M]`) where the object
    /// Check if a type is a "generic indexed access" — an `IndexAccess(A, B)` where
    /// the object part `A` contains free type parameters.
    ///
    /// tsc treats such types as not provably callable at definition time, even if
    /// substituting the type parameter's constraint would produce a callable union.
    /// For example, `DataFetchFns[T][F]` is a generic indexed access (T is a free
    /// type param in the object), while `DataFetchFns['Boat'][F]` is not (the object
    /// `DataFetchFns['Boat']` is concrete).
    fn is_generic_indexed_access(&self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        if let Some((object, _)) = query::index_access_components(db, type_id) {
            return query::contains_type_parameters(self.ctx.types, object);
        }
        false
    }

    /// Return the indexed-access subject used for TS2344 callable checks.
    ///
    /// Supports both direct indexed-access type arguments (`A[B]`) and
    /// application-wrapped aliases whose instantiated body is indexed access
    /// (e.g., `Alias<T, F>` where `type Alias<T, F> = A[T][F]`).
    fn generic_indexed_access_subject(&mut self, type_id: TypeId) -> Option<TypeId> {
        if self.is_generic_indexed_access(type_id) {
            return Some(type_id);
        }

        let db = self.ctx.types.as_type_database();
        let (Some(base_def), app_args) = query::application_base_def_and_args(db, type_id)? else {
            return None;
        };
        let Some(def_info) = self.ctx.definition_store.get(base_def) else {
            return None;
        };
        if def_info.kind != tsz_solver::def::DefKind::TypeAlias {
            return None;
        }
        let body = self.ctx.definition_store.get_body(base_def)?;

        let mut instantiated_body = body;
        if let Some(type_params) = self.ctx.definition_store.get_type_params(base_def)
            && !type_params.is_empty()
            && !app_args.is_empty()
        {
            let mut subst = crate::query_boundaries::common::TypeSubstitution::new();
            for (param, arg) in type_params.iter().zip(app_args.iter()) {
                subst.insert(param.name, *arg);
            }
            if !subst.is_empty() {
                instantiated_body =
                    crate::query_boundaries::common::instantiate_type(self.ctx.types, body, &subst);
            }
        }

        self.is_generic_indexed_access(instantiated_body)
            .then_some(instantiated_body)
    }

    /// Check if an indexed access type `T[M]` resolves to a callable type
    /// through its constraint chain. This handles cases like:
    /// `T[M]` where `T extends { [K in keyof T]: () => unknown }` and `M extends keyof T`.
    /// The mapped type template `() => unknown` is callable, so `T[M]` resolves
    /// to a callable type. It also handles callable string/number index
    /// signatures like `T extends { [key: string]: (...args: any) => void }`.
    fn indexed_access_resolves_to_callable(&mut self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        let Some((object, _index)) = query::index_access_components(db, type_id) else {
            return false;
        };
        // Resolve the object type's constraint chain to find a mapped type
        let object_constraint = if query::is_bare_type_parameter(db, object) {
            let base = query::base_constraint_of_type(db, object);
            if base != object {
                self.evaluate_type_for_assignability(base)
            } else {
                return false;
            }
        } else {
            return false;
        };
        let db = self.ctx.types.as_type_database();
        // Check if the resolved constraint is a mapped type with callable template
        if let Some(template) = query::mapped_type_template(db, object_constraint) {
            let template_eval = self.evaluate_type_for_assignability(template);
            let db2 = self.ctx.types.as_type_database();
            return query::is_callable_type(db2, template_eval)
                || query::callable_shape_for_type(db2, template_eval).is_some()
                || query::is_callable_type(db2, template);
        }
        for value_type in query::index_signature_value_types(db, object_constraint)
            .into_iter()
            .flatten()
        {
            let value_eval = self.evaluate_type_for_assignability(value_type);
            let db2 = self.ctx.types.as_type_database();
            if query::is_callable_type(db2, value_eval)
                || query::callable_shape_for_type(db2, value_eval).is_some()
                || query::is_callable_type(db2, value_type)
                || query::callable_shape_for_type(db2, value_type).is_some()
            {
                return true;
            }
        }
        false
    }

    fn satisfies_array_like_constraint(&mut self, source: TypeId, target: TypeId) -> bool {
        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);
        let target_elem = crate::query_boundaries::checkers::call::array_element_type_for_type(
            self.ctx.types,
            target,
        )
        .unwrap_or_else(|| self.get_element_access_type(target, TypeId::NUMBER, Some(0)));
        if target_elem == TypeId::ERROR {
            return false;
        }

        if !self.is_array_like_type(source) && !self.has_structural_array_surface(source, target) {
            return false;
        }

        if target_elem == TypeId::ANY {
            return true;
        }

        let source_elem = self.get_element_access_type(source, TypeId::NUMBER, Some(0));
        source_elem != TypeId::ERROR && self.is_assignable_to(source_elem, target_elem)
    }

    /// Check if a type argument coinductively satisfies a recursive constraint
    /// via its heritage chain.
    ///
    /// When an interface extends a generic base (e.g., `interface BB extends AA<AA<BB>>`),
    /// and the constraint is an Application of that same base (e.g., `AA<BB>`), the
    /// structural subtype check becomes circular. The subtype checker can't detect the
    /// cycle because pre-evaluation destroys DefId identity. This method detects the
    /// pattern and returns true (coinductive assumption).
    fn satisfies_recursive_heritage_constraint(
        &self,
        type_arg: TypeId,
        constraint: TypeId,
    ) -> bool {
        let db = self.ctx.types.as_type_database();

        // Get the Application base DefId from the constraint.
        // e.g., for AA<BB>, get the DefId of AA.
        let Some(constraint_base_def) = query::application_base_def_id(db, constraint) else {
            return false;
        };

        // Get the type_arg's DefId (it must be an interface/class, i.e., Lazy type).
        let type_arg_def = query::lazy_def_id(db, type_arg);
        let Some(type_arg_def) = type_arg_def else {
            // When the type_arg is an Application (e.g., AA<BB>) with the same base
            // as the constraint (e.g., AA<AA<BB>>), AND the inner type arguments of
            // the Application type_arg extend the constraint base via heritage, the
            // constraint is coinductively satisfied. This handles recursive constraints
            // like `T extends AA<T>` where `interface BB extends AA<AA<BB>>` — checking
            // `AA<BB>` against `AA<AA<BB>>` leads to infinite nesting that tsc resolves
            // via deeply-nested type detection.
            if let Some((Some(type_arg_base_def), ref type_arg_args)) =
                query::application_base_def_and_args(db, type_arg)
                && type_arg_base_def == constraint_base_def
            {
                // Same base type (e.g., both are AA<...>).
                // Check if any inner type argument extends the constraint base,
                // which would create the circular recursion pattern.
                for &inner_arg in type_arg_args.iter() {
                    if let Some(inner_def) = query::lazy_def_id(db, inner_arg) {
                        let inner_sym = self.ctx.def_to_symbol_id(inner_def);
                        let constraint_sym = self.ctx.def_to_symbol_id(constraint_base_def);
                        if let (Some(inner_sym_id), Some(constraint_sym_id)) =
                            (inner_sym, constraint_sym)
                            && self.interface_extends_symbol(inner_sym_id, constraint_sym_id)
                        {
                            return true;
                        }
                    }
                }
            }
            return false;
        };

        // Resolve DefIds to SymbolIds
        let type_arg_sym = self.ctx.def_to_symbol_id(type_arg_def);
        let constraint_base_sym = self.ctx.def_to_symbol_id(constraint_base_def);

        let (Some(type_arg_sym_id), Some(constraint_base_sym_id)) =
            (type_arg_sym, constraint_base_sym)
        else {
            return false;
        };

        // Check if type_arg's interface heritage chain includes the constraint's
        // base interface. Walk the heritage clauses in the binder to find if BB
        // extends any instantiation of AA.
        self.interface_extends_symbol(type_arg_sym_id, constraint_base_sym_id)
    }

    /// Check if an interface symbol extends (directly or transitively) a target symbol.
    fn interface_extends_symbol(
        &self,
        interface_sym_id: tsz_binder::SymbolId,
        target_sym_id: tsz_binder::SymbolId,
    ) -> bool {
        if interface_sym_id == target_sym_id {
            return true;
        }

        let Some(symbol) = self.ctx.binder.get_symbol(interface_sym_id) else {
            return false;
        };

        // Check each declaration's heritage clauses
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.ctx.arena.get_interface(node) else {
                continue;
            };
            let Some(ref heritage_clauses) = interface.heritage_clauses else {
                continue;
            };
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };
                    // Extract the base expression (might be an ExpressionWithTypeArguments)
                    let expr_idx = if let Some(eta) = self.ctx.arena.get_expr_type_args(type_node) {
                        eta.expression
                    } else {
                        type_idx
                    };
                    if let Some(base_sym) = self.resolve_heritage_symbol(expr_idx)
                        && base_sym == target_sym_id
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn has_structural_array_surface(&self, source: TypeId, target: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();

        let Some(shape) = query::get_object_shape(db, source) else {
            return false;
        };
        if shape.number_index.is_none() {
            return false;
        }

        for name in ["length", "concat", "slice"] {
            if !query::has_property_by_name(db, source, name) {
                return false;
            }
        }

        if !matches!(
            query::classify_array_like(db, target),
            query::ArrayLikeKind::Readonly(_)
        ) && !query::has_property_by_name(db, source, "push")
        {
            return false;
        }

        true
    }

    /// Check if a symbol's declaration has type parameters, even if they couldn't be
    /// resolved via `get_type_params_for_symbol` (e.g., cross-arena lib types).
    pub(crate) fn symbol_declaration_has_type_parameters(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders);
        let Some(symbol) = symbol else {
            return false;
        };

        // Check the value declaration and all declarations for type parameters
        for decl_idx in symbol.all_declarations() {
            // Try current arena first
            if let Some(node) = self.ctx.arena.get(decl_idx) {
                if let Some(ta) = self.ctx.arena.get_type_alias(node) {
                    if ta.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(iface) = self.ctx.arena.get_interface(node) {
                    if iface.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(class) = self.ctx.arena.get_class(node) {
                    if class.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
            }

            // Try cross-arena (lib files)
            if let Some(decl_arena) = self.ctx.binder.symbol_arenas.get(&sym_id)
                && let Some(node) = decl_arena.get(decl_idx)
            {
                if let Some(ta) = decl_arena.get_type_alias(node) {
                    if ta.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(iface) = decl_arena.get_interface(node) {
                    if iface.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(class) = decl_arena.get_class(node) {
                    if class.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
            }

            // Try declaration_arenas
            if let Some(decl_arena) = self
                .ctx
                .binder
                .declaration_arenas
                .get(&(sym_id, decl_idx))
                .and_then(|v| v.first())
                && let Some(node) = decl_arena.get(decl_idx)
            {
                if let Some(ta) = decl_arena.get_type_alias(node) {
                    if ta.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(iface) = decl_arena.get_interface(node) {
                    if iface.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(class) = decl_arena.get_class(node) {
                    if class.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
            }
        }

        false
    }
}
