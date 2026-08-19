use crate::query_boundaries::checkers::generic as query;
use crate::query_boundaries::class::is_incomplete_class_type;
use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl CheckerState<'_> {
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
        // Pad the supplied arguments with each omitted trailing parameter's
        // resolved default (declaration order, so a later default may reference an
        // earlier parameter), matching how tsc fills defaults before instantiating.
        // Without this, a constraint that references an omitted SIBLING parameter —
        // e.g. `K extends S extends 'wide' ? ... : keyof O` where `S` is defaulted —
        // keeps the sibling free, so the conditional never reduces and the check is
        // silently skipped (#14754). `fill_application_defaults` returns the supplied
        // args verbatim when nothing is omitted, so the all-supplied path is
        // unchanged. The loop below still iterates the SUPPLIED args, so diagnostics
        // stay on explicitly-written positions.
        // Only the under-supplied case allocates a padded vector; when every
        // parameter has an explicit argument `full_type_args` borrows `type_args`.
        let padded_type_args = (type_args.len() < type_params.len())
            .then(|| {
                crate::query_boundaries::type_defaults::fill_application_defaults(
                    self.ctx.types.as_type_database(),
                    &type_args,
                    type_params,
                )
            })
            .flatten();
        let full_type_args: &[TypeId] = padded_type_args.as_deref().unwrap_or(&type_args);
        let type_arg_substitutions = type_params
            .iter()
            .zip(full_type_args.iter())
            .map(|(param, &arg)| (param.name, arg))
            .collect::<Vec<_>>();

        // Build the `{type-param-name -> type-arg}` substitution once for the
        // whole call. Constraint instantiation re-asks for this same mapping at
        // several points inside the per-parameter loop below; rebuilding it per
        // iteration is O(D) map construction inside an O(D) loop, i.e. O(D^2)
        // per textual type-reference occurrence for a depth-D bounded-parameter
        // chain (`T0 extends Base, T1 extends T0, ...`). The mapping is
        // invariant across iterations (it depends only on `type_params` /
        // `type_args`), so hoisting it collapses the per-occurrence
        // constraint-substitution cost back to O(D). It is built from the same
        // zipped name/arg pairs the in-loop builders used, so the substitution —
        // and therefore every diagnostic — is unchanged. (#13250)
        let type_arg_subst = {
            let mut subst = crate::query_boundaries::common::TypeSubstitution::new();
            for (param, &arg) in type_params.iter().zip(full_type_args.iter()) {
                subst.insert(param.name, arg);
            }
            subst
        };

        for (i, (param, &type_arg)) in type_params.iter().zip(type_args.iter()).enumerate() {
            if let Some(constraint) = param.constraint {
                let arg_idx = type_args_list.nodes.get(i).copied();
                if let Some(unknown_arg_idx) =
                    arg_idx.filter(|&idx| self.type_arg_is_unknown_keyword(idx))
                {
                    let constraint_resolved = self.resolve_lazy_type(constraint);
                    let inst_constraint = self
                        .instantiate_constraint_with_subst(constraint_resolved, &type_arg_subst);
                    // Reduce the instantiated constraint before deciding whether the
                    // top-type `unknown` argument satisfies it. `A[number]` with
                    // `A = unknown[]` is `unknown[][number]`, which evaluates to
                    // `unknown`; `unknown` satisfies any constraint whose reduced
                    // form is a top type, matching tsc. Reducing also yields the
                    // apparent constraint (`string[][number]` -> `string`) for the
                    // diagnostic display on a genuine violation.
                    let reduced_constraint = self.evaluate_type_for_assignability(inst_constraint);
                    // A constraint is a top type not only when it reduces to the
                    // canonical `any`/`unknown`, but also when it is structurally
                    // equal to `unknown` — e.g. `{} | null | undefined` (TypeScript's
                    // `NonReducibleUnknown` idiom), which `unknown` is assignable to.
                    // Defer to the assignability relation so `unknown` satisfies any
                    // such top-type constraint, matching tsc (xstate `NonReducibleUnknown`).
                    if !matches!(reduced_constraint, TypeId::ANY | TypeId::UNKNOWN)
                        && !self
                            .unknown_type_arg_top_constraint_relation_outcome(reduced_constraint)
                            .related
                    {
                        let constraint_str =
                            self.format_type_diagnostic_constraint(reduced_constraint);
                        self.error_at_node_msg(
                            unknown_arg_idx,
                            crate::diagnostics::diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
                            &["unknown", &constraint_str],
                        );
                        continue;
                    }
                }

                if type_arg == TypeId::ERROR {
                    continue;
                }
                if arg_idx.is_some_and(|idx| self.type_arg_subtree_has_arity_error(idx)) {
                    continue;
                }

                if arg_idx
                    .is_some_and(|idx| self.type_arg_subtree_has_value_used_as_type_error(idx))
                {
                    continue;
                }

                if query::is_this_type(self.ctx.types.as_type_database(), type_arg) {
                    continue;
                }

                if let Some(arg_idx) = arg_idx
                    && self.conditional_flow_type_arg_constraint_handled(
                        type_arg,
                        constraint,
                        &type_arg_subst,
                        arg_idx,
                    )
                {
                    continue;
                }

                if self.substitution_type_arg_constraint_handled(
                    type_arg,
                    constraint,
                    &type_arg_subst,
                    arg_idx,
                ) {
                    continue;
                }

                if let Some(arg_idx) = arg_idx {
                    let constraint_resolved = self.resolve_lazy_type(constraint);
                    if constraint_resolved == TypeId::ANY {
                        continue;
                    }
                    let inst_constraint =
                        self.instantiate_constraint_with_subst(constraint, &type_arg_subst);
                    if self.required_mapped_constraint_source_is_required_and_arg_satisfies(
                        type_arg,
                        inst_constraint,
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
                        let evaluated_original = self.evaluate_type_for_assignability(type_arg);
                        if evaluated_original != type_arg
                            && !matches!(
                                evaluated_original,
                                TypeId::UNKNOWN | TypeId::ERROR | TypeId::NEVER
                            )
                        {
                            // Trust the relation on the evaluated form. With
                            // conditional-flow substitution narrowing the check
                            // variable, a generic argument like `CamelCase<T>`
                            // inside a `T extends string ? …` true branch
                            // evaluates to a string-shaped form (e.g. a template
                            // literal `` `c${T}` ``) that genuinely satisfies the
                            // constraint even though it still mentions `T`. The
                            // relation already accounts for the narrowing, so a
                            // related evaluated form is accepted regardless of
                            // residual type parameters.
                            if self
                                .type_arg_constraint_relation_outcome(
                                    evaluated_original,
                                    constraint_resolved,
                                )
                                .related
                            {
                                if self.generic_boolean_literal_probe_should_remain_indeterminate(
                                    type_arg,
                                    evaluated_original,
                                    constraint_resolved,
                                ) {
                                    self.error_type_constraint_not_satisfied(
                                        TypeId::BOOLEAN,
                                        constraint_resolved,
                                        arg_idx,
                                    );
                                    continue;
                                }
                                continue;
                            }
                            // A fully concrete evaluated form that is not related
                            // is a definite violation. When the evaluated form
                            // still contains type parameters, fall through to the
                            // concrete-substitution probe below.
                            if !query::contains_type_parameters(self.ctx.types, evaluated_original)
                            {
                                self.error_type_constraint_not_satisfied(
                                    evaluated_original,
                                    constraint_resolved,
                                    arg_idx,
                                );
                                continue;
                            }
                        }
                        let concrete_arg = self.scoped_type_param_substituted_form(type_arg);
                        if self.type_arg_evaluates_to_infer_result_conditional(concrete_arg) {
                            continue;
                        }
                        let concrete_arg = self.resolve_lazy_type(concrete_arg);
                        let concrete_arg = self.evaluate_type_for_assignability(concrete_arg);
                        if self
                            .type_arg_constraint_relation_outcome(concrete_arg, constraint_resolved)
                            .related
                        {
                            if self.generic_boolean_literal_probe_should_remain_indeterminate(
                                type_arg,
                                concrete_arg,
                                constraint_resolved,
                            ) {
                                self.error_type_constraint_not_satisfied(
                                    TypeId::BOOLEAN,
                                    constraint_resolved,
                                    arg_idx,
                                );
                                continue;
                            }
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
                let has_typeof_instantiation_node = type_args_list
                    .nodes
                    .get(i)
                    .is_some_and(|&arg_idx| self.is_typeof_instantiation_node(arg_idx));
                let failed_typeof_instantiation_arg =
                    self.is_failed_typeof_instantiation_arg(type_arg);
                if failed_typeof_instantiation_node
                    || (!has_typeof_instantiation_node && failed_typeof_instantiation_arg)
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

                if let Some(&arg_idx) = type_args_list.nodes.get(i)
                    && self.syntax_instantiated_type_arg_satisfies_constraint(
                        type_arg,
                        arg_idx,
                        type_params,
                        full_type_args,
                        constraint,
                    )
                {
                    continue;
                }

                if self.emit_invalid_remapped_mapped_template_index_constraint_error(
                    type_arg,
                    constraint,
                    type_args_list.nodes.get(i).copied(),
                ) {
                    continue;
                }
                let concrete_application_args = crate::query_boundaries::common::type_application(
                    self.ctx.types.as_type_database(),
                    type_arg,
                )
                .is_some_and(|app| {
                    app.args
                        .iter()
                        .all(|&arg| !query::contains_type_parameters(self.ctx.types, arg))
                });
                if concrete_application_args {
                    let constraint_resolved = self.resolve_lazy_type(constraint);
                    let inst_constraint = self
                        .instantiate_constraint_with_subst(constraint_resolved, &type_arg_subst);
                    if !query::contains_type_parameters(self.ctx.types, inst_constraint) {
                        self.ensure_relation_input_ready(type_arg);
                        self.ensure_relation_input_ready(inst_constraint);
                        let evaluated_arg = self.evaluate_type_for_assignability(type_arg);
                        let evaluated_constraint =
                            self.evaluate_type_for_assignability(inst_constraint);
                        if evaluated_arg != type_arg
                            && !matches!(
                                evaluated_arg,
                                TypeId::UNKNOWN | TypeId::ERROR | TypeId::NEVER
                            )
                            && (self
                                .type_arg_constraint_no_weak_relation_outcome(
                                    evaluated_arg,
                                    evaluated_constraint,
                                )
                                .related
                                || self.satisfies_array_like_constraint(
                                    evaluated_arg,
                                    evaluated_constraint,
                                )
                                || self.conditional_result_branches_satisfy_constraint(
                                    evaluated_arg,
                                    evaluated_constraint,
                                ))
                        {
                            continue;
                        }
                    }
                }
                // When the type argument contains type parameters, we generally skip
                // constraint checking (deferred to instantiation time). However, when
                // the type arg IS a bare type parameter, check its base constraint
                // against the required constraint. This matches tsc: `U extends number`
                // used as `T extends string` → TS2344 because `number` is not
                // assignable to `string`.
                let type_arg_contains_type_parameters =
                    query::contains_type_parameters(self.ctx.types, type_arg);
                let type_arg_is_application =
                    query::is_application_type(self.ctx.types.as_type_database(), type_arg);
                if type_arg_contains_type_parameters && type_arg_is_application {
                    // Application type arguments that still contain type parameters
                    // are generally deferred to instantiation time. Check the cheap
                    // callable/indexed-access exception before proving positive
                    // conditional/object-map cases, since those proofs can expand
                    // recursive helper aliases for libraries like ts-toolbelt.
                    let constraint_resolved = self.resolve_lazy_type(constraint);
                    let constraint_is_callable = query::is_callable_type(
                        self.ctx.types.as_type_database(),
                        constraint_resolved,
                    ) || query::constraint_expands_to_callable_union(
                        self.ctx.types.as_type_database(),
                        constraint_resolved,
                    ) || self
                        .is_function_constraint(param.constraint.unwrap_or(TypeId::NEVER));
                    let constraint_is_object_like = constraint_resolved == TypeId::OBJECT
                        || query::get_object_shape(
                            self.ctx.types.as_type_database(),
                            constraint_resolved,
                        )
                        .is_some();
                    let generic_indexed_type_arg = self.generic_indexed_access_subject(type_arg);
                    let keep_eager_check = constraint_is_callable
                        && generic_indexed_type_arg.is_some()
                        && !self.indexed_access_resolves_to_callable(
                            generic_indexed_type_arg.unwrap_or(type_arg),
                        );
                    let is_infer_result_conditional_application = self
                        .type_arg_evaluates_to_infer_result_conditional(type_arg)
                        || self
                            .type_alias_application_infer_result_conditional_components(type_arg)
                            .is_some();
                    if !constraint_is_object_like
                        && !keep_eager_check
                        && !is_infer_result_conditional_application
                    {
                        continue;
                    }
                }
                if type_arg_contains_type_parameters && type_arg_is_application {
                    let constraint_resolved = self.resolve_lazy_type(constraint);
                    let inst_constraint = self
                        .instantiate_constraint_with_subst(constraint_resolved, &type_arg_subst);
                    if self.generic_alias_application_satisfies_object_constraint(
                        type_arg,
                        inst_constraint,
                    ) {
                        continue;
                    }
                    if self
                        .conditional_result_branches_satisfy_constraint(type_arg, inst_constraint)
                        || self
                            .type_alias_application_filters_to_constraint(type_arg, inst_constraint)
                    {
                        continue;
                    }
                    let evaluated_arg = self.evaluate_type_for_assignability(type_arg);
                    if evaluated_arg != type_arg
                        && !matches!(
                            evaluated_arg,
                            TypeId::UNKNOWN | TypeId::ERROR | TypeId::NEVER
                        )
                        && !query::contains_type_parameters(self.ctx.types, evaluated_arg)
                    {
                        if self.conditional_result_branches_satisfy_constraint(
                            type_arg,
                            inst_constraint,
                        ) || self
                            .type_alias_application_filters_to_constraint(type_arg, inst_constraint)
                            || self
                                .type_arg_constraint_relation_outcome(
                                    evaluated_arg,
                                    inst_constraint,
                                )
                                .related
                            || query::homomorphic_mapped_application_should_defer_constraint(
                                self, type_arg,
                            )
                        {
                            continue;
                        }
                        if let Some(&arg_idx) = type_args_list.nodes.get(i)
                            && !self.type_argument_is_narrowed_by_conditional_true_branch(
                                arg_idx,
                                inst_constraint,
                            )
                        {
                            self.error_type_constraint_not_satisfied(
                                evaluated_arg,
                                inst_constraint,
                                arg_idx,
                            );
                        }
                        continue;
                    }
                }
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
                if type_arg_contains_type_parameters
                    && let Some(base) = base_constraint_type
                    && self.bare_type_param_base_satisfies_instantiated_constraint(
                        type_arg,
                        base,
                        constraint,
                        type_params,
                        full_type_args,
                    )
                {
                    continue;
                }
                if type_arg_contains_type_parameters {
                    let constraint_resolved = self.resolve_lazy_type(constraint);
                    let inst_constraint = self
                        .instantiate_constraint_with_subst(constraint_resolved, &type_arg_subst);
                    if self
                        .conditional_result_branches_satisfy_constraint(type_arg, inst_constraint)
                    {
                        continue;
                    }
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
                                                full_type_args,
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
                                            let inst_constraint = self
                                                .instantiate_constraint_with_subst(
                                                    constraint_resolved,
                                                    &type_arg_subst,
                                                );

                                            // Concrete constraints are checked here too:
                                            // `unknown` infer bases fail constraints like `string`.
                                            let is_satisfied = inst_constraint == TypeId::UNKNOWN
                                                || inst_constraint == TypeId::ANY
                                                || self
                                                    .infer_result_constraint_relation_outcome(
                                                        infer_base,
                                                        inst_constraint,
                                                    )
                                                    .related
                                                || {
                                                    let evaluated =
                                                        self.evaluate_type_for_assignability(type_arg);
                                                    evaluated != type_arg
                                                        && self
                                                            .infer_result_constraint_relation_outcome(
                                                            evaluated,
                                                            inst_constraint,
                                                        )
                                                            .related
                                                }
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
                                                    .array_element_infer_alias_satisfies_constraint(
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
                                        if self
                                            .conditional_constraint_component_relation_outcome(
                                                ext_evaluated,
                                                constraint_resolved,
                                            )
                                            .related
                                            || self
                                                .conditional_constraint_component_relation_outcome(
                                                    ext_resolved,
                                                    constraint_resolved,
                                                )
                                                .related
                                        {
                                            continue;
                                        }
                                        // Extract-like pattern (? T : never) but the
                                        // extends type does NOT satisfy the constraint. tsc
                                        // reports TS2344 in this case. Instantiate constraint
                                        // with type args for accurate error messages.
                                        let inst_constraint = self
                                            .instantiate_constraint_with_subst(
                                                constraint_resolved,
                                                &type_arg_subst,
                                            );
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
                                    query::is_callable_type(db, constraint_resolved)
                                        || query::constraint_expands_to_callable_union(
                                            db,
                                            constraint_resolved,
                                        );
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
                                let constraint_is_callable =
                                    query::is_callable_type(
                                        self.ctx.types.as_type_database(),
                                        constraint_resolved,
                                    ) || query::constraint_expands_to_callable_union(
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
                            let inst_constraint =
                                self.instantiate_constraint_with_subst(constraint, &type_arg_subst);
                            let inst_constraint = self.resolve_lazy_type(inst_constraint);
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
                                    || query::constraint_expands_to_callable_union(
                                        db,
                                        inst_constraint,
                                    )
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
                                .type_arg_constraint_relation_outcome(
                                    base_for_check,
                                    inst_constraint,
                                )
                                .related
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
                                )
                                || type_args_list.nodes.get(i).copied().is_some_and(|arg_idx| {
                                    self.type_arg_satisfies_via_hidden_infer_constraints(
                                        type_arg,
                                        arg_idx,
                                        inst_constraint,
                                    )
                                });
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
                                query::full_conditional_type_components(db, type_arg)
                                    .or_else(|| {
                                        query::full_conditional_type_components(
                                            self.ctx.types.as_type_database(),
                                            type_arg_evaluated,
                                        )
                                    })
                                    .or_else(|| {
                                        self.type_alias_application_infer_result_conditional_components(
                                            type_arg,
                                        )
                                    })
                                && cond_false == TypeId::NEVER
                                && query::is_infer_type(db, cond_true)
                            {
                                let constraint_resolved = self.resolve_lazy_type(constraint);
                                let inst_constraint = self
                                    .instantiate_constraint_with_subst(constraint_resolved, &type_arg_subst);
                                let infer_base =
                                    query::get_type_parameter_constraint(db, cond_true)
                                        .unwrap_or(TypeId::UNKNOWN);
                                let is_satisfied = inst_constraint == TypeId::UNKNOWN
                                    || inst_constraint == TypeId::ANY
                                    || self
                                        .infer_result_constraint_relation_outcome(
                                            infer_base,
                                            inst_constraint,
                                        )
                                        .related
                                    || (type_arg_evaluated != type_arg
                                        && self
                                            .infer_result_constraint_relation_outcome(
                                                type_arg_evaluated,
                                                inst_constraint,
                                            )
                                            .related)
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
                                    || self.array_element_infer_alias_satisfies_constraint(
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
                                        .conditional_constraint_component_relation_outcome(
                                            extends_evaluated,
                                            constraint_resolved,
                                        )
                                        .related
                                        || self
                                            .conditional_constraint_component_relation_outcome(
                                                extends_resolved,
                                                constraint_resolved,
                                            )
                                            .related)
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
                                        || query::constraint_expands_to_callable_union(
                                            db,
                                            constraint_evaluated,
                                        )
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
                    if is_bare_type_param
                        && let Some(&arg_idx) = type_args_list.nodes.get(i)
                        && self.explicit_alias_type_parameter_constraint_satisfies_arg_constraint(
                            arg_idx,
                            type_arg,
                            constraint,
                            type_params,
                            full_type_args,
                        )
                    {
                        continue;
                    }
                    if is_bare_type_param
                        && let Some(&arg_idx) = type_args_list.nodes.get(i)
                        && self.merged_interface_sibling_constraint_satisfies_type_arg_constraint(
                            arg_idx, constraint,
                        )
                    {
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
                            let inst_constraint = self.instantiate_constraint_with_subst(
                                constraint_resolved,
                                &type_arg_subst,
                            );
                            if inst_constraint == TypeId::UNKNOWN
                                || inst_constraint == TypeId::ANY
                                || self
                                    .infer_result_constraint_relation_outcome(
                                        positional_constraint,
                                        inst_constraint,
                                    )
                                    .related
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
                            if query::constraint_has_keyof_surface(
                                self.ctx.types,
                                constraint_resolved,
                            ) {
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
                                    let inst_constraint = self.instantiate_constraint_with_subst(
                                        constraint_resolved,
                                        &type_arg_subst,
                                    );
                                    if inst_constraint != TypeId::UNKNOWN
                                        && inst_constraint != TypeId::ANY
                                        && !query::contains_type_parameters(
                                            self.ctx.types,
                                            inst_constraint,
                                        )
                                        && !self
                                            .infer_result_constraint_relation_outcome(
                                                hidden_base,
                                                inst_constraint,
                                            )
                                            .related
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
                            let inst_constraint = self.instantiate_constraint_with_subst(
                                constraint_resolved,
                                &type_arg_subst,
                            );
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
                                    !self
                                        .type_arg_constraint_relation_outcome(
                                            base_for_check,
                                            inst_constraint,
                                        )
                                        .related
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
                        if query::contains_free_type_parameters(self.ctx.types, base)
                            && !crate::query_boundaries::type_predicates::base_admits_any_primitive_index_key(
                                self.ctx.types.as_type_database(),
                                &[base],
                            )
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
                        let inst_constraint =
                            self.instantiate_constraint_with_subst(constraint, &type_arg_subst);
                        let inst_constraint = self.resolve_lazy_type(inst_constraint);
                        if query::contains_type_parameters(self.ctx.types, inst_constraint) {
                            continue;
                        }
                        let inst_constraint_for_message = inst_constraint;
                        // Evaluate indexed access / keyof types in the constraint
                        // before checking. E.g., `WeakKeyTypes[keyof WeakKeyTypes]`
                        // must be reduced to `object | symbol` for the assignability
                        // check to work correctly.
                        // Ensure lazy refs inside the constraint are resolved in the
                        // type environment BEFORE evaluation. Without this, constraints
                        // like `WeakKeyTypes[keyof WeakKeyTypes]` (where WeakKeyTypes is
                        // a Lazy(DefId) from a lib file) remain unevaluated because the
                        // evaluator's `ensure_relation_input_ready` may be skipped due
                        // to depth guards during nested evaluation.
                        self.ensure_relation_input_ready(inst_constraint);
                        let inst_constraint = self.evaluate_type_for_assignability(inst_constraint);
                        if query::keyof_operand(
                            self.ctx.types.as_type_database(),
                            constraint_resolved,
                        )
                        .is_some()
                            && {
                                // Decide membership *structurally* per primitive_key.
                                // Both the unevaluated and evaluated base are
                                // checked so keyof/indexed-access bases that only
                                // decompose into a Union after evaluation are
                                // still recognized.
                                let base_evaluated = self.evaluate_type_for_assignability(base);
                                let present =
                                    crate::query_boundaries::type_predicates::present_primitive_index_keys(
                                        self.ctx.types.as_type_database(),
                                        &[base, base_evaluated],
                                    );
                                present.into_iter().any(|primitive_key| {
                                    !self
                                        .type_arg_constraint_relation_outcome(
                                            primitive_key,
                                            inst_constraint,
                                        )
                                        .related
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
                        self.ensure_refs_resolved(base);
                        let base_for_check = self.resolve_lazy_members_in_union(base);
                        let base_for_check = self.evaluate_type_for_assignability(base_for_check);
                        let mut is_satisfied = self
                            .type_arg_constraint_relation_outcome(base_for_check, inst_constraint)
                            .related
                            || is_incomplete_class_type(self, base_for_check)
                            || is_incomplete_class_type(self, base) // deferred class Lazy (#17743)
                            || self.base_union_members_satisfy_constraint(
                                base_for_check,
                                inst_constraint,
                            )
                            || self
                                .satisfies_array_like_constraint(base_for_check, inst_constraint)
                            || self.infer_result_satisfies_via_referenced_constraints(
                                type_arg,
                                inst_constraint,
                            )
                            || self.array_element_infer_alias_satisfies_constraint(
                                type_arg,
                                inst_constraint,
                            )
                            || type_args_list.nodes.get(i).copied().is_some_and(|arg_idx| {
                                self.type_arg_satisfies_via_hidden_infer_constraints(
                                    type_arg,
                                    arg_idx,
                                    inst_constraint,
                                )
                            });
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

                // Instantiate before resolving so dependent constraints like
                // `Required<Options>` keep the earlier type-argument binding.
                let written_keyof_constraint_display = self
                    .written_keyof_any_constraint_display(constraint)
                    .or_else(|| {
                        self.written_keyof_constraint_display(
                            constraint,
                            type_params,
                            type_args_list,
                        )
                    })
                    .or_else(|| self.written_primitive_key_union_alias_display(constraint));
                let constraint =
                    self.instantiate_constraint_with_subst(constraint, &type_arg_subst);
                let constraint = self.resolve_lazy_type(constraint);
                let constraint = self
                    .resolve_well_known_lib_constraint_type(constraint)
                    .unwrap_or(constraint);
                if let Some(&arg_idx) = type_args_list.nodes.get(i)
                    && self
                        .type_argument_is_narrowed_by_conditional_true_branch(arg_idx, constraint)
                {
                    continue;
                }

                let mut subst = crate::query_boundaries::common::TypeSubstitution::new();
                for (j, p) in type_params.iter().enumerate() {
                    if let Some(&arg) = full_type_args.get(j) {
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
                let mut display_subst = query_common::TypeSubstitution::new();
                for (j, p) in type_params.iter().enumerate() {
                    if let Some(&arg) = full_type_args.get(j) {
                        // Supplied positions keep their written reference form; an
                        // omitted (defaulted) sibling has no argument node, so its
                        // resolved default type is rendered directly.
                        let arg_node = type_args_list.nodes.get(j).copied();
                        display_subst.insert(p.name, self.type_arg_reference_form(arg, arg_node));
                    }
                }
                let constraint_for_message = if display_subst.is_empty() {
                    constraint
                } else {
                    query_common::instantiate_type(self.ctx.types, constraint, &display_subst)
                };
                let primitive_fails_nominal_lib_object =
                    query::is_primitive_type(self.ctx.types.as_type_database(), type_arg)
                        && self.is_nominal_lib_object_constraint_type(constraint);
                if primitive_fails_nominal_lib_object {
                    if let Some(&arg_idx) = type_args_list.nodes.get(i) {
                        self.error_type_constraint_not_satisfied(
                            type_arg,
                            constraint_for_message,
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
                    let constraint_for_weak_check =
                        self.evaluate_type_for_assignability(instantiated_constraint);
                    let constraint_for_weak_check =
                        self.resolve_lazy_type(constraint_for_weak_check);
                    let db = self.ctx.types.as_type_database();
                    if let Some(shape_id) = crate::query_boundaries::common::object_shape_id(
                        db,
                        constraint_for_weak_check,
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
                let mut is_satisfied = !callable_arity_failure
                    && !constructor_accessibility_failure
                    && (primitive_satisfies_weak
                        || if constraint_is_all_optional
                            && !query::is_primitive_type(
                                self.ctx.types.as_type_database(),
                                type_arg,
                            )
                        {
                            self.type_arg_constraint_relation_outcome(
                                type_arg,
                                instantiated_constraint,
                            )
                            .related
                        } else {
                            self.type_arg_constraint_no_weak_relation_outcome(
                                type_arg,
                                instantiated_constraint,
                            )
                            .related
                        });
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
                // For self-referential constraints like `T extends AA<T>` in
                // `interface AA<T extends AA<T>>`, checking if a type arg satisfies
                // the constraint leads to circular structural checks that the
                // subtype checker can't resolve (pre-evaluation destroys DefId
                // identity needed for cycle detection).
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
                        )
                        || self.array_element_infer_alias_satisfies_constraint(
                            type_arg,
                            instantiated_constraint,
                        );
                }
                if !is_satisfied && !constraint_is_all_optional {
                    let evaluated_arg = self.evaluate_type_for_assignability(type_arg);
                    if evaluated_arg != type_arg
                        && !matches!(
                            evaluated_arg,
                            TypeId::UNKNOWN | TypeId::ERROR | TypeId::NEVER
                        )
                    {
                        is_satisfied = self
                            .type_arg_constraint_no_weak_relation_outcome(
                                evaluated_arg,
                                instantiated_constraint,
                            )
                            .related
                            || self.satisfies_array_like_constraint(
                                evaluated_arg,
                                instantiated_constraint,
                            )
                            || self.conditional_result_branches_satisfy_constraint(
                                evaluated_arg,
                                instantiated_constraint,
                            );
                    }
                }
                if !is_satisfied
                    && let Some(base) = base_constraint_type
                    && base != TypeId::UNKNOWN
                    && !query::contains_free_type_parameters(self.ctx.types, base)
                {
                    let base = self.resolve_lazy_members_in_union(base);
                    let base = self.evaluate_type_for_assignability(base);
                    is_satisfied = self
                        .type_arg_constraint_relation_outcome(base, instantiated_constraint)
                        .related
                        || self
                            .base_union_members_satisfy_constraint(base, instantiated_constraint)
                        || self.satisfies_array_like_constraint(base, instantiated_constraint);
                }
                if constructor_accessibility_failure {
                    is_satisfied = false;
                }
                if !is_satisfied && let Some(&arg_idx) = type_args_list.nodes.get(i) {
                    // A TS2344 satisfaction decision requires a fully-resolved
                    // constraint. When the instantiated constraint is still an
                    // unresolved `Lazy(DefId)` (e.g. a lib alias like `PropertyKey`
                    // / `keyof any` whose body could not be resolved here because
                    // the cross-file lazy-resolution budget was exhausted while a
                    // sibling deeply-recursive type argument was evaluated), the
                    // checker cannot know what the constraint is. Make one more
                    // genuine resolution attempt; if it stays opaque, defer rather
                    // than fail closed against the reference — tsc always has the
                    // resolved constraint here, so an unresolved `Lazy` is a tsz
                    // resolution limitation, not a real violation (#14337).
                    if crate::query_boundaries::common::is_lazy_type(
                        self.ctx.types,
                        instantiated_constraint,
                    ) {
                        self.ensure_relation_input_ready(instantiated_constraint);
                        let reresolved = self.resolve_lazy_type(instantiated_constraint);
                        if crate::query_boundaries::common::is_lazy_type(self.ctx.types, reresolved)
                        {
                            continue;
                        }
                        let reresolved = self.evaluate_type_for_assignability(reresolved);
                        if self
                            .type_arg_constraint_no_weak_relation_outcome(type_arg, reresolved)
                            .related
                        {
                            continue;
                        }
                    }
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
                                constraint_for_message,
                                arg_idx,
                            );
                        }
                    } else {
                        self.error_type_constraint_not_satisfied_with_constraint_display(
                            type_arg,
                            constraint_for_message,
                            arg_idx,
                            written_keyof_constraint_display.clone(),
                        );
                    }
                }
            }
        }
    }
}
