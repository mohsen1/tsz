//! Generic type argument constraint validation (TS2344).

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Returns `true` when an arity diagnostic (TS2314 generic type requires
    /// N args, TS2315 type is not generic, or TS2707 generic type requires
    /// between M and N args) was emitted at any byte offset inside the AST
    /// range of `type_arg_idx`. Used to suppress cascading TS2344 on an
    /// outer type reference whose argument carries an inner arity error.
    fn type_arg_subtree_has_arity_error(&self, type_arg_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(type_arg_idx) else {
            return false;
        };
        let (start, end) = (node.pos, node.end);
        if end <= start {
            return false;
        }
        self.ctx
            .diagnostics
            .iter()
            .any(|d| matches!(d.code, 2314 | 2315 | 2707) && d.start >= start && d.start < end)
    }

    fn type_arg_subtree_has_value_used_as_type_error(&self, type_arg_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(type_arg_idx) else {
            return false;
        };
        let (start, end) = (node.pos, node.end);
        if end <= start {
            return false;
        }
        let code = crate::diagnostics::diagnostic_codes::REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF;
        self.ctx
            .diagnostics
            .iter()
            .any(|d| d.code == code && d.start >= start && d.start < end)
    }

    fn type_arg_is_unknown_keyword(&self, type_arg_idx: NodeIndex) -> bool {
        self.node_text(type_arg_idx)
            .is_some_and(|text| text.trim() == "unknown")
            || self
                .type_arg_identifier_name(type_arg_idx)
                .is_some_and(|name| name == "unknown")
            || self
                .ctx
                .arena
                .get(type_arg_idx)
                .is_some_and(|node| node.kind == SyntaxKind::UnknownKeyword as u16)
    }

    /// Validate each type argument against its corresponding type parameter
    /// constraint. Reports TS2344 when a type argument doesn't satisfy its
    /// constraint. Shared by call expressions, new expressions, and type refs.
    pub(crate) fn validate_type_args_against_params(
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
        let type_arg_substitutions = type_params
            .iter()
            .zip(type_args.iter())
            .map(|(param, &arg)| (param.name, arg))
            .collect::<Vec<_>>();

        for (i, (param, &type_arg)) in type_params.iter().zip(type_args.iter()).enumerate() {
            if let Some(constraint) = param.constraint {
                if let Some(&arg_idx) = type_args_list.nodes.get(i)
                    && self.type_arg_is_unknown_keyword(arg_idx)
                {
                    let constraint_resolved = self.resolve_lazy_type(constraint);
                    let inst_constraint = self.instantiate_constraint_with_type_args(
                        constraint_resolved,
                        type_params,
                        &type_args,
                    );
                    if !matches!(inst_constraint, TypeId::ANY | TypeId::UNKNOWN) {
                        let constraint_str =
                            self.format_type_diagnostic_constraint(inst_constraint);
                        self.error_at_node_msg(
                            arg_idx,
                            crate::diagnostics::diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
                            &["unknown", &constraint_str],
                        );
                        continue;
                    }
                }

                // Skip constraint checking when the type argument is an error type
                // (avoids cascading errors from unresolved references)
                if type_arg == TypeId::ERROR {
                    continue;
                }

                // Suppress cascading TS2344 when an inner type ref already
                // emitted a type-argument arity diagnostic.
                if let Some(&arg_idx) = type_args_list.nodes.get(i)
                    && self.type_arg_subtree_has_arity_error(arg_idx)
                {
                    continue;
                }

                if let Some(&arg_idx) = type_args_list.nodes.get(i)
                    && self.type_arg_subtree_has_value_used_as_type_error(arg_idx)
                {
                    continue;
                }

                // `this` is polymorphic; tsc defers this constraint check.
                if query::is_this_type(self.ctx.types.as_type_database(), type_arg) {
                    continue;
                }

                if let Some(&arg_idx) = type_args_list.nodes.get(i) {
                    let constraint_resolved = self.resolve_lazy_type(constraint);
                    if self.required_mapped_constraint_source_is_required_and_arg_satisfies(
                        type_arg,
                        constraint_resolved,
                        &type_arg_substitutions,
                    ) {
                        continue;
                    }
                    // Only use scoped-param substitution for primitive constraints;
                    // richer shapes can lose the relation that makes them valid.
                    if self.type_node_is_generic_ref_with_scoped_type_param_arg(arg_idx)
                        && query::is_primitive_type(
                            self.ctx.types.as_type_database(),
                            constraint_resolved,
                        )
                        && !query::is_callable_type(
                            self.ctx.types.as_type_database(),
                            constraint_resolved,
                        )
                        && !self.is_function_constraint(constraint_resolved)
                        && !query::contains_type_parameters(self.ctx.types, constraint_resolved)
                        && !self.type_arg_evaluates_to_infer_result_conditional(type_arg)
                    {
                        // Generic-reference type args that mention a scoped type
                        // parameter (e.g. `Box<Array<U>>`) cannot be skipped just
                        // because the constraint is concrete: tsc still validates
                        // the instantiation. Substitute scoped params with their
                        // base constraints (or `unknown`) to obtain a concrete
                        // shape, evaluate it, and check assignability against the
                        // constraint. If the concrete shape is assignable, defer;
                        // otherwise emit TS2344 with the original type_arg display
                        // (matches tsc's "Type 'X[]' does not satisfy 'string'"). (#3063)
                        if self.type_alias_application_filters_to_constraint(
                            type_arg,
                            constraint_resolved,
                        ) {
                            continue;
                        }
                        let concrete_arg = self.scoped_type_param_substituted_form(type_arg);
                        if self.type_arg_evaluates_to_infer_result_conditional(concrete_arg) {
                            continue;
                        }
                        let concrete_arg = self.resolve_lazy_type(concrete_arg);
                        let concrete_arg = self.evaluate_type_for_assignability(concrete_arg);
                        if self.is_assignable_to(concrete_arg, constraint_resolved) {
                            continue;
                        }
                        self.error_type_constraint_not_satisfied(
                            type_arg,
                            constraint_resolved,
                            arg_idx,
                        );
                        continue;
                    }
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

                // Defer `this`-containing references; their concrete type is
                // only known at instantiation time.
                if query::is_this_type(self.ctx.types.as_type_database(), type_arg)
                    || crate::query_boundaries::common::contains_this_type(
                        self.ctx.types.as_type_database(),
                        type_arg,
                    )
                {
                    continue;
                }

                // Failed instantiation expressions (`typeof fn<TArgs>` where `TArgs`
                // do not match any signature's type-parameter arity) are treated by
                // tsc as `errorType`, which then fails the surrounding type-parameter
                // constraint check and triggers TS2344 — in addition to the TS2635
                // emitted at the instantiation site. Match that behavior.
                //
                // The Application path further down would otherwise defer constraint
                // checking for any `Application(TypeQuery, args)` whose constraint is
                // not generic-indexed-access shaped, dropping TS2344 in this case.
                let failed_typeof_instantiation_node = type_args_list
                    .nodes
                    .get(i)
                    .is_some_and(|&arg_idx| self.is_failed_typeof_instantiation_node(arg_idx));
                if failed_typeof_instantiation_node
                    || self.is_failed_typeof_instantiation_arg(type_arg)
                {
                    let constraint_resolved = self.resolve_lazy_type(constraint);
                    if let Some(&arg_idx) = type_args_list.nodes.get(i) {
                        self.error_type_constraint_not_satisfied(
                            type_arg,
                            constraint_resolved,
                            arg_idx,
                        );
                    }
                    continue;
                }
                if self.skip_constraint_for_typeof_instantiation(
                    type_arg,
                    constraint,
                    type_args_list.nodes.get(i).copied(),
                ) {
                    continue;
                }

                if self.emit_invalid_remapped_mapped_template_index_constraint_error(
                    type_arg,
                    constraint,
                    type_args_list.nodes.get(i).copied(),
                ) {
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
                    && base_constraint_type.is_none_or(|base| base == TypeId::UNKNOWN)
                    && let Some(&arg_idx) = type_args_list.nodes.get(i)
                    && let Some(name) = self.type_arg_identifier_name(arg_idx)
                    && let Some(&scope_type_id) = self.ctx.type_parameter_scope.get(&name)
                {
                    let db = self.ctx.types.as_type_database();
                    let scoped_base = crate::query_boundaries::common::type_parameter_constraint(
                        db,
                        scope_type_id,
                    )
                    .unwrap_or_else(|| {
                        crate::query_boundaries::common::get_base_constraint_of_type(
                            db,
                            scope_type_id,
                        )
                    });
                    if scoped_base != scope_type_id && scoped_base != TypeId::UNKNOWN {
                        base_constraint_type = Some(scoped_base);
                    }
                }
                if type_arg_contains_type_parameters
                    && base_constraint_type.is_none_or(|base| {
                        base == TypeId::UNKNOWN
                            || query::contains_free_type_parameters(self.ctx.types, base)
                    })
                    && let Some(&arg_idx) = type_args_list.nodes.get(i)
                    && let Some(constraint_node) =
                        self.type_arg_explicit_constraint_node_in_ast(arg_idx)
                    && constraint_node != NodeIndex::NONE
                {
                    let ast_base = self.get_type_from_type_node(constraint_node);
                    if ast_base != TypeId::UNKNOWN && ast_base != type_arg {
                        base_constraint_type = Some(ast_base);
                    }
                }
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
                                        // Extract-like (`T extends C ? T : never`, false == never):
                                        // extends type is a proxy for the result; check it against
                                        // the constraint. Key-filtering and structured true branches
                                        // are non-Extract and must defer to instantiation.
                                        let cond_true_is_bare_param = query::is_bare_type_parameter(
                                            self.ctx.types.as_type_database(),
                                            cond_true,
                                        );
                                        let inst_constraint = self
                                            .instantiate_constraint_for_type_args(
                                                constraint_resolved,
                                                type_params,
                                                &type_args,
                                            );
                                        if self
                                            .conditional_true_type_parameter_base_satisfies_constraint(
                                                cond_check,
                                                cond_true,
                                                inst_constraint,
                                            )
                                        {
                                            continue;
                                        }
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
                                            // Unconstrained infer has `unknown` as its base.
                                            let infer_base = query::get_type_parameter_constraint(
                                                self.ctx.types.as_type_database(),
                                                cond_true,
                                            )
                                            .unwrap_or(TypeId::UNKNOWN);

                                            // Instantiate for accurate error messages.
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

                                            // Concrete constraints are checked here too:
                                            // `unknown` infer bases fail constraints like `string`.
                                            let is_satisfied = inst_constraint == TypeId::UNKNOWN
                                                || inst_constraint == TypeId::ANY
                                                || self
                                                    .is_assignable_to(infer_base, inst_constraint)
                                                || self
                                                    .infer_result_satisfies_via_check_constraint(
                                                        base,
                                                        (cond_check, cond_extends, cond_true),
                                                        inst_constraint,
                                                    )
                                                || self
                                                    .infer_result_satisfies_array_like_constraint(
                                                        cond_extends,
                                                        cond_true,
                                                        inst_constraint,
                                                    )
                                                || self
                                                    .type_arg_evaluates_to_array_like_infer_result_conditional(
                                                        type_arg,
                                                        inst_constraint,
                                                    )
                                                || self
                                                    .infer_result_satisfies_via_application_arg_constraints(
                                                        type_arg,
                                                        inst_constraint,
                                                    )
                                                || self
                                                    .infer_result_satisfies_via_referenced_constraints(
                                                        type_arg,
                                                        inst_constraint,
                                                    );

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
                                    && (query::is_mapped_template_callable(db, mapped_id)
                                        || self
                                            .mapped_template_resolves_to_callable_through_constraint(
                                                obj,
                                            ))
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
                                    .is_some()
                                        || self.indexed_access_resolves_to_callable(template);
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
                                .is_some()
                                    || type_args_list.nodes.get(i).copied().is_some_and(
                                        |arg_idx| {
                                            self.ctx
                                                .arena
                                                .get(arg_idx)
                                                .and_then(|node| self.ctx.arena.get_type_ref(node))
                                                .is_some_and(|type_ref| {
                                                    type_ref.type_arguments.is_some()
                                                })
                                        },
                                    );
                                if type_arg_is_application {
                                    continue;
                                }
                            }

                            let base_for_check = self.resolve_lazy_members_in_union(base);
                            let base_for_check =
                                self.evaluate_type_for_assignability(base_for_check);
                            let mut is_satisfied = self
                                .is_assignable_to(base_for_check, inst_constraint)
                                || self.base_union_members_satisfy_constraint(
                                    base_for_check,
                                    inst_constraint,
                                )
                                || self.satisfies_array_like_constraint(
                                    base_for_check,
                                    inst_constraint,
                                )
                                || self.infer_result_satisfies_via_referenced_constraints(
                                    type_arg,
                                    inst_constraint,
                                );
                            if !is_satisfied {
                                // When the constraint is a function type (e.g., `(...args: any) => any`),
                                // accept any callable base type. For type parameters with callable
                                // constraints (e.g., `F extends Function`), check the constraint.
                                // Also check the structural Function interface pattern (apply/call/bind)
                                // since Function may be lowered as an Object without call signatures.
                                let is_fn_constraint = self
                                    .is_function_constraint(original_constraint)
                                    || query::is_callable_type(db, original_constraint);
                                let base_is_callable = query::is_callable_type(db, base_for_check)
                                    || self.type_parameter_has_callable_constraint(base_for_check)
                                    || self.is_function_constraint(base_for_check)
                                    || query::is_function_interface_structural(db, base_for_check);
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
                        // When base_constraint_type is None or UNKNOWN (composite type with
                        // type params that can't be simplified further), check if the required
                        // constraint is callable. Tsc eagerly emits TS2344 when the constraint is a
                        // callable signature and the composite type arg is not provably callable.
                        // Example: `ReturnType<TypeHardcodedAsParameterWithoutReturnType<T,F>>`
                        // where `TypeHardcodedAsParameterWithoutReturnType<T,F>` = `DataFetchFns[T][F]`.
                        //
                        // The constraint TypeId may come from a lib arena (cross-arena). Resolve
                        // it fully and evaluate before checking callability.
                        if base_constraint_type.is_none_or(|base| base == TypeId::UNKNOWN) {
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
                            if let Some((cond_check, cond_extends, cond_true, cond_false)) =
                                query::full_conditional_type_components(db, type_arg).or_else(
                                    || {
                                        query::full_conditional_type_components(
                                            self.ctx.types.as_type_database(),
                                            type_arg_evaluated,
                                        )
                                    },
                                )
                                && cond_false == TypeId::NEVER
                                && query::is_infer_type(db, cond_true)
                            {
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
                                let infer_base =
                                    query::get_type_parameter_constraint(db, cond_true)
                                        .unwrap_or(TypeId::UNKNOWN);
                                let is_satisfied = inst_constraint == TypeId::UNKNOWN
                                    || inst_constraint == TypeId::ANY
                                    || self.is_assignable_to(infer_base, inst_constraint)
                                    || (type_arg_evaluated != type_arg
                                        && self
                                            .is_assignable_to(type_arg_evaluated, inst_constraint))
                                    || self.infer_result_satisfies_via_check_constraint(
                                        type_arg,
                                        (cond_check, cond_extends, cond_true),
                                        inst_constraint,
                                    )
                                    || self.infer_result_satisfies_array_like_constraint(
                                        cond_extends,
                                        cond_true,
                                        inst_constraint,
                                    )
                                    || self
                                        .type_arg_evaluates_to_array_like_infer_result_conditional(
                                            type_arg,
                                            inst_constraint,
                                        )
                                    || self.infer_result_satisfies_via_application_arg_constraints(
                                        type_arg,
                                        inst_constraint,
                                    )
                                    || self.infer_result_satisfies_via_referenced_constraints(
                                        type_arg,
                                        inst_constraint,
                                    );

                                if !is_satisfied
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
                        // Bare `Infer` — base_constraint_of_type returns the type
                        // unchanged, so base_constraint_type is None. Skip when the
                        // infer var has a hidden structural or positional constraint.
                        let has_implicit_constraint =
                            type_args_list.nodes.get(i).copied().is_some_and(|arg_idx| {
                                self.has_hidden_conditional_infer_constraint_local(arg_idx)
                            });
                        if has_implicit_constraint {
                            continue;
                        }
                        // Positional constraint: `infer R` in `Result<any, infer R>` where
                        // `Rest extends string` gives R an implicit `string` constraint.
                        if let Some(&arg_idx) = type_args_list.nodes.get(i)
                            && let Some(positional_constraint) =
                                self.hidden_conditional_infer_constraint_type(arg_idx)
                        {
                            let constraint_resolved = self.resolve_lazy_type(constraint);
                            let inst_constraint = self.instantiate_constraint_with_type_args(
                                constraint_resolved,
                                type_params,
                                &type_args,
                            );
                            if inst_constraint == TypeId::UNKNOWN
                                || inst_constraint == TypeId::ANY
                                || self.is_assignable_to(positional_constraint, inst_constraint)
                            {
                                continue;
                            }
                            self.error_type_constraint_not_satisfied(
                                type_arg,
                                inst_constraint,
                                arg_idx,
                            );
                            continue;
                        }
                        if let Some(&arg_idx) = type_args_list.nodes.get(i)
                            && self.type_arg_has_explicit_constraint_in_ast(arg_idx)
                        {
                            let constraint_resolved = self.resolve_lazy_type(constraint);
                            if self
                                .format_type_diagnostic(constraint_resolved)
                                .starts_with("keyof ")
                            {
                                self.error_type_constraint_not_satisfied(
                                    type_arg,
                                    constraint_resolved,
                                    arg_idx,
                                );
                                continue;
                            }
                        }
                    }
                    if is_bare_type_param && let Some(base) = base_constraint_type {
                        // Bare type parameter — check its base constraint instead of
                        // eagerly validating the unresolved type parameter itself.
                        if base == TypeId::UNKNOWN {
                            // UNKNOWN base: either truly unconstrained or unresolved
                            // (cross-arena, mapped key, function type param, or infer
                            // var synthesized from a constrained positional slot).
                            // Check for hidden/positional constraints before emitting.
                            let has_hidden_constraint =
                                type_args_list.nodes.get(i).copied().is_some_and(|arg_idx| {
                                    self.is_inside_mapped_type(arg_idx)
                                        || self
                                            .has_hidden_conditional_infer_constraint_local(arg_idx)
                                        || self
                                            .hidden_conditional_infer_constraint_type(arg_idx)
                                            .is_some()
                                });
                            if has_hidden_constraint {
                                if let Some(&arg_idx) = type_args_list.nodes.get(i)
                                    && let Some(hidden_base) =
                                        self.hidden_conditional_infer_constraint_type(arg_idx)
                                {
                                    let constraint_resolved = self.resolve_lazy_type(constraint);
                                    let inst_constraint = self
                                        .instantiate_constraint_with_type_args(
                                            constraint_resolved,
                                            type_params,
                                            &type_args,
                                        );
                                    if inst_constraint != TypeId::UNKNOWN
                                        && inst_constraint != TypeId::ANY
                                        && !query::contains_type_parameters(
                                            self.ctx.types,
                                            inst_constraint,
                                        )
                                        && !self.is_assignable_to(hidden_base, inst_constraint)
                                    {
                                        self.error_type_constraint_not_satisfied(
                                            type_arg,
                                            inst_constraint,
                                            arg_idx,
                                        );
                                    }
                                }
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
                            let base_for_check = (base != TypeId::UNKNOWN).then(|| {
                                let base_for_check = self.resolve_lazy_members_in_union(base);
                                self.evaluate_type_for_assignability(base_for_check)
                            });
                            if is_checkable
                                && base_for_check.is_none_or(|base_for_check| {
                                    !self.is_assignable_to(base_for_check, inst_constraint)
                                        && !self.base_union_members_satisfy_constraint(
                                            base_for_check,
                                            inst_constraint,
                                        )
                                        && !self.satisfies_array_like_constraint(
                                            base_for_check,
                                            inst_constraint,
                                        )
                                })
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
                        let base_allows_primitive_key = |this: &Self, candidate: TypeId| {
                            let display = this.format_type_diagnostic(candidate);
                            candidate == TypeId::STRING
                                || candidate == TypeId::NUMBER
                                || candidate == TypeId::SYMBOL
                                || display == "string | number"
                                || display == "string | number | symbol"
                                || crate::query_boundaries::common::union_members(
                                    this.ctx.types,
                                    candidate,
                                )
                                .is_some_and(|members| {
                                    members.into_iter().any(|member| {
                                        matches!(
                                            member,
                                            TypeId::STRING | TypeId::NUMBER | TypeId::SYMBOL
                                        )
                                    })
                                })
                        };
                        if query::contains_free_type_parameters(self.ctx.types, base)
                            && !base_allows_primitive_key(self, base)
                        {
                            // Base constraint itself contains free type parameters
                            // (e.g., from outer generic scope). Defer check.
                            // Uses free-type-param check to avoid false positives
                            // from bound type params inside method signatures
                            // (e.g., `interface Base { bar<W>(): Inner<W> }` —
                            // W is bound by bar, not free in Base).
                            continue;
                        }
                        let constraint_resolved = self.resolve_lazy_type(constraint);
                        if query::contains_type_parameters(self.ctx.types, constraint_resolved)
                            && query::keyof_operand(
                                self.ctx.types.as_type_database(),
                                constraint_resolved,
                            )
                            .is_none()
                        {
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
                        let inst_constraint_for_message = inst_constraint;
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
                        if query::keyof_operand(
                            self.ctx.types.as_type_database(),
                            constraint_resolved,
                        )
                        .is_some()
                            && {
                                // Decide membership *structurally* per primitive_key.
                                // A display-string match ("string | number" /
                                // "string | number | symbol") is per-base, not
                                // per-key, so it would falsely admit SYMBOL
                                // when base is `string | number` and emit a
                                // spurious TS2344. Use TypeId equality and
                                // union_members on both the unevaluated and
                                // evaluated base — the latter recovers cases
                                // where keyof/indexed-access bases only
                                // decompose into a Union after evaluation.
                                let base_evaluated = self.evaluate_type_for_assignability(base);
                                let base_members = crate::query_boundaries::common::union_members(
                                    self.ctx.types,
                                    base,
                                );
                                let base_evaluated_members =
                                    crate::query_boundaries::common::union_members(
                                        self.ctx.types,
                                        base_evaluated,
                                    );
                                [TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]
                                    .into_iter()
                                    .any(|primitive_key| {
                                        let in_base = base == primitive_key
                                            || base_evaluated == primitive_key
                                            || base_members
                                                .as_ref()
                                                .is_some_and(|m| m.contains(&primitive_key))
                                            || base_evaluated_members
                                                .as_ref()
                                                .is_some_and(|m| m.contains(&primitive_key));
                                        in_base
                                            && !self
                                                .is_assignable_to(primitive_key, inst_constraint)
                                    })
                            }
                            && let Some(&arg_idx) = type_args_list.nodes.get(i)
                        {
                            self.error_type_constraint_not_satisfied(
                                type_arg,
                                inst_constraint_for_message,
                                arg_idx,
                            );
                            continue;
                        }
                        let base_for_check = self.resolve_lazy_members_in_union(base);
                        let base_for_check = self.evaluate_type_for_assignability(base_for_check);
                        let mut is_satisfied = self
                            .is_assignable_to(base_for_check, inst_constraint)
                            || self.base_union_members_satisfy_constraint(
                                base_for_check,
                                inst_constraint,
                            )
                            || self
                                .satisfies_array_like_constraint(base_for_check, inst_constraint)
                            || self.infer_result_satisfies_via_referenced_constraints(
                                type_arg,
                                inst_constraint,
                            );
                        if !is_satisfied {
                            // When the constraint is a function type, accept callable bases.
                            // The `Function` interface may be lowered as an Object type
                            // (without call signatures), so also check for the structural
                            // pattern (apply/call/bind properties).
                            let db2 = self.ctx.types.as_type_database();
                            let is_fn_constraint = self.is_function_constraint(inst_constraint)
                                || query::is_callable_type(db2, inst_constraint);
                            let base_is_callable = query::is_callable_type(db2, base_for_check)
                                || self.type_parameter_has_callable_constraint(base_for_check)
                                || self.is_function_constraint(base_for_check)
                                || query::is_function_interface_structural(db2, base_for_check);
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
                let constraint_name = self.format_type_diagnostic(constraint);
                let constraint = self
                    .is_well_known_lib_type_name(&constraint_name)
                    .then(|| self.resolve_lib_type_by_name(&constraint_name))
                    .flatten()
                    .unwrap_or(constraint);
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
                let primitive_fails_nominal_lib_object =
                    query::is_primitive_type(self.ctx.types.as_type_database(), type_arg)
                        && self.is_nominal_lib_object_type_name(&constraint_name);
                if primitive_fails_nominal_lib_object {
                    if let Some(&arg_idx) = type_args_list.nodes.get(i) {
                        self.error_type_constraint_not_satisfied(
                            type_arg,
                            instantiated_constraint,
                            arg_idx,
                        );
                    }
                    continue;
                }
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
                let callable_arity_failure = self
                    .concrete_function_type_arg_violates_callable_constraint(
                        type_arg,
                        instantiated_constraint,
                    );
                let constructor_accessibility_failure =
                    self.constructor_accessibility_blocks_type_arg_constraint(
                        type_arg,
                        instantiated_constraint,
                    ) || type_args_list.nodes.get(i).is_some_and(|&arg_idx| {
                        self.type_query_constructor_access_level(arg_idx).is_some()
                            && crate::query_boundaries::common::construct_signatures_for_type(
                                self.ctx.types,
                                instantiated_constraint,
                            )
                            .is_some_and(|sigs| !sigs.is_empty())
                    });
                let all_optional_non_primitive = constraint_is_all_optional
                    && !query::is_primitive_type(self.ctx.types.as_type_database(), type_arg);
                let mut constraint_relation_outcome = None;
                let mut is_satisfied = !callable_arity_failure
                    && !constructor_accessibility_failure
                    && (primitive_satisfies_weak
                        || if all_optional_non_primitive {
                            use crate::query_boundaries::assignability::RelationRequest;
                            let (prepared_arg, prepared_constraint) = self
                                .prepare_assignability_inputs(type_arg, instantiated_constraint);
                            let request =
                                RelationRequest::assign(prepared_arg, prepared_constraint);
                            let outcome = self.execute_relation_request(&request);
                            let related = outcome.related;
                            constraint_relation_outcome = Some(outcome);
                            related
                        } else {
                            self.is_assignable_to_no_weak_checks(type_arg, instantiated_constraint)
                        });

                // When the constraint is all-optional and the structural check
                // passed (because all-optional types have no required properties),
                // separately check for weak type violation (TS2559).
                // Non-primitive type arguments with NO common properties should
                // fail, e.g., MyObjA {x: string} vs ObjA {y?: string}.
                if is_satisfied
                    && !primitive_satisfies_weak
                    && constraint_relation_outcome
                        .as_ref()
                        .is_some_and(|outcome| outcome.weak_union_violation)
                {
                    is_satisfied = false;
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
                    is_satisfied = self
                        .is_global_function_interface_constraint(original_constraint)
                        && query::has_call_signatures(db, type_arg);
                }
                if !is_satisfied {
                    is_satisfied = self
                        .satisfies_array_like_constraint(type_arg, instantiated_constraint)
                        || self.type_arg_evaluates_to_array_like_infer_result_conditional(
                            type_arg,
                            instantiated_constraint,
                        );
                }
                if !is_satisfied
                    && let Some(base) = base_constraint_type
                    && base != TypeId::UNKNOWN
                    && !query::contains_free_type_parameters(self.ctx.types, base)
                {
                    let base = self.resolve_lazy_members_in_union(base);
                    let base = self.evaluate_type_for_assignability(base);
                    is_satisfied = self.is_assignable_to(base, instantiated_constraint)
                        || self
                            .base_union_members_satisfy_constraint(base, instantiated_constraint)
                        || self.satisfies_array_like_constraint(base, instantiated_constraint);
                }
                if constructor_accessibility_failure {
                    is_satisfied = false;
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
                    let weak_constraint_violation =
                        if let Some(outcome) = constraint_relation_outcome.as_ref() {
                            outcome.weak_union_violation
                        } else {
                            let analysis = self
                                .analyze_assignability_failure(type_arg, instantiated_constraint);
                            matches!(
                                analysis.failure_reason,
                                Some(tsz_solver::SubtypeFailureReason::NoCommonProperties { .. })
                            )
                        };
                    if weak_constraint_violation {
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

    pub(super) fn ast_indexed_access_property_union_from_declaration(
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

    pub(super) fn constraint_check_indexed_access_value_type(
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

        // For the built-in utility alias `Record<K, V>` and its equivalent
        // user-facing aliases, evaluate the alias body before falling back to
        // structural/object-shape checks. Without this, patterns like
        // `{ [K in keyof O]: Record<O[K], K> }[keyof O]` can still retain
        // an `Application` form and fail TS2344 checks even though
        // `Record`'s template is provably valid for the key space.
        if let Some(alias_object_type) =
            self.resolve_record_alias_type_for_indexed_access_value(object_type)
            && alias_object_type != object_type
            && let Some(value_type) =
                self.constraint_check_indexed_access_value_type(alias_object_type, index_type)
        {
            return Some(value_type);
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
            let keyed_object_type =
                if let Some(keyed_operand) = query::keyof_operand(db, keyed_object_type) {
                    let keyed_operand = self.evaluate_type_for_assignability(keyed_operand);
                    let keyed_operand = self.resolve_lazy_type(keyed_operand);
                    if keyed_operand == object_type {
                        object_keys
                    } else {
                        keyed_object_type
                    }
                } else {
                    keyed_object_type
                };
            let keys_assignable = self.is_assignable_to(keyed_object_type, object_keys);
            if !keys_assignable {
                return None;
            }
            let mut property_types: Vec<TypeId> =
                shape.properties.iter().map(|prop| prop.type_id).collect();
            if let Some(index) = &shape.string_index {
                property_types.push(index.value_type);
            }
            if let Some(index) = &shape.number_index {
                property_types.push(index.value_type);
            }
            return match property_types.len() {
                0 => None,
                1 => property_types.first().copied(),
                _ => Some(self.ctx.types.union(property_types)),
            };
        }

        None
    }

    fn resolve_record_alias_type_for_indexed_access_value(
        &mut self,
        object_type: TypeId,
    ) -> Option<TypeId> {
        let app = crate::query_boundaries::common::type_application(self.ctx.types, object_type)?;
        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias {
            return None;
        }
        if self.ctx.types.resolve_atom(def.name) != "Record" {
            return None;
        }
        if def.type_params.len() != app.args.len() || def.type_params.is_empty() {
            return None;
        }
        let body = def.body?;
        let subst = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            &def.type_params,
            &app.args,
        );
        let instantiated =
            crate::query_boundaries::common::instantiate_type(self.ctx.types, body, &subst);
        let evaluated = self.evaluate_type_for_assignability(instantiated);
        Some(self.resolve_lazy_type(evaluated))
    }

    fn type_alias_application_filters_to_constraint(
        &mut self,
        mut type_arg: TypeId,
        constraint: TypeId,
    ) -> bool {
        for _ in 0..8 {
            if let Some((check, extends_type, true_type, false_type)) =
                query::full_conditional_type_components(self.ctx.types.as_type_database(), type_arg)
            {
                if false_type != TypeId::NEVER || true_type != check {
                    return false;
                }
                let extends_resolved = self.resolve_lazy_type(extends_type);
                let extends_evaluated = self.evaluate_type_for_assignability(extends_resolved);
                let constraint_evaluated = self.evaluate_type_for_assignability(constraint);
                return self.is_assignable_to(extends_evaluated, constraint_evaluated)
                    || self.is_assignable_to(extends_resolved, constraint);
            }

            let Some(app) =
                crate::query_boundaries::common::type_application(self.ctx.types, type_arg)
            else {
                return false;
            };
            let Some(def_id) =
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)
            else {
                return false;
            };
            let Some(def) = self.ctx.definition_store.get(def_id) else {
                return false;
            };
            if def.kind != tsz_solver::def::DefKind::TypeAlias {
                return false;
            }
            let Some(body) = def.body else {
                return false;
            };
            if def.type_params.len() != app.args.len() {
                return false;
            }
            let subst = crate::query_boundaries::common::TypeSubstitution::from_args(
                self.ctx.types,
                &def.type_params,
                &app.args,
            );
            type_arg =
                crate::query_boundaries::common::instantiate_type(self.ctx.types, body, &subst);
            type_arg = self.resolve_lazy_type(type_arg);
        }
        false
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

    pub(super) fn base_union_members_satisfy_constraint(
        &mut self,
        base: TypeId,
        constraint: TypeId,
    ) -> bool {
        let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, base)
        else {
            return false;
        };
        let original_constraint = constraint;
        let constraint = self.resolve_lazy_type(constraint);
        let constraint = self.evaluate_type_for_assignability(constraint);
        !members.is_empty()
            && members.iter().all(|&member| {
                if self.member_extends_constraint_heritage(member, original_constraint) {
                    return true;
                }
                let member = self.resolve_lazy_type(member);
                let member = self.evaluate_type_for_assignability(member);
                self.is_assignable_to(member, constraint)
                    || self.satisfies_array_like_constraint(member, constraint)
            })
    }

    pub(crate) fn member_extends_constraint_heritage(
        &mut self,
        member: TypeId,
        constraint: TypeId,
    ) -> bool {
        let db = self.ctx.types.as_type_database();
        let member_sym = self
            .ctx
            .resolve_type_to_symbol_id(member)
            .or_else(|| {
                query::lazy_def_id(db, member)
                    .and_then(|def| self.ctx.def_to_symbol_id_with_fallback(def))
            })
            .or_else(|| self.symbol_id_for_heritage_type_name(member));
        let constraint_sym = self
            .ctx
            .resolve_type_to_symbol_id(constraint)
            .or_else(|| {
                query::lazy_def_id(db, constraint)
                    .and_then(|def| self.ctx.def_to_symbol_id_with_fallback(def))
            })
            .or_else(|| self.symbol_id_for_heritage_type_name(constraint));
        let (Some(member_sym), Some(constraint_sym)) = (member_sym, constraint_sym) else {
            return false;
        };
        self.interface_extends_symbol(member_sym, constraint_sym)
            && !self.member_has_conflicting_constraint_property(member, constraint)
    }
}
