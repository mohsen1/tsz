//! Control flow narrowing: assignments, predicates, instanceof, in-operator,
//! typeof, discriminants, and literal comparisons.
//!
//! Reference matching, literal parsing, and symbol resolution utilities are in
//! `references.rs`.

use tsz_binder::symbol_flags;
use tsz_common::interner::Atom;
use tsz_parser::parser::node::CallExprData;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{GuardSense, ParamInfo, TypeGuard, TypeId, TypePredicate, TypePredicateTarget};

use super::{FlowAnalyzer, PredicateSignature};
use crate::query_boundaries::flow_analysis::{
    self as flow_query, PredicateSignatureKind, classify_for_predicate_signature,
    is_narrowing_literal, stringify_literal_type, union_members_for_type,
};

impl<'a> FlowAnalyzer<'a> {
    pub(crate) fn assignment_affects_reference(&self, left: NodeIndex, target: NodeIndex) -> bool {
        self.assignment_matches_reference_core(left, target, true)
    }

    pub(crate) fn assignment_targets_reference_internal(
        &self,
        left: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        self.assignment_matches_reference_core(left, target, false)
    }

    /// Core implementation for both `assignment_affects_reference` and
    /// `assignment_targets_reference_internal`.
    ///
    /// When `check_property_access` is true (the "affects" variant), this also:
    /// - Recurses through `reference_base` on the target
    /// - Traverses property/element access expressions on the left side
    fn assignment_matches_reference_core(
        &self,
        left: NodeIndex,
        target: NodeIndex,
        check_property_access: bool,
    ) -> bool {
        let left = self.skip_parenthesized(left);
        let target = self.skip_parenthesized(target);
        if self.is_matching_reference(left, target) {
            return true;
        }
        if check_property_access
            && let Some(base) = self.reference_base(target)
            && self.assignment_matches_reference_core(left, base, true)
        {
            return true;
        }

        let Some(node) = self.arena.get(left) else {
            return false;
        };

        if check_property_access
            && (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
        {
            let Some(access) = self.arena.get_access_expr(node) else {
                return false;
            };
            if access.question_dot_token {
                return false;
            }
            return self.assignment_matches_reference_core(access.expression, target, true);
        }

        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
            && let Some(unary) = self.arena.get_unary_expr_ex(node)
        {
            return self.assignment_matches_reference_core(
                unary.expression,
                target,
                check_property_access,
            );
        }

        if (node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
            && let Some(assertion) = self.arena.get_type_assertion(node)
        {
            return self.assignment_matches_reference_core(
                assertion.expression,
                target,
                check_property_access,
            );
        }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(node)
            && self.is_assignment_operator(bin.operator_token)
        {
            return self.assignment_matches_reference_core(bin.left, target, check_property_access);
        }

        if (node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
            && let Some(lit) = self.arena.get_literal_expr(node)
        {
            for &elem in &lit.elements.nodes {
                if elem.is_none() {
                    continue;
                }
                if self.assignment_matches_reference_core(elem, target, check_property_access) {
                    return true;
                }
            }
        }

        if node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && let Some(prop) = self.arena.get_property_assignment(node)
            && self.assignment_matches_reference_core(
                prop.initializer,
                target,
                check_property_access,
            )
        {
            return true;
        }

        if node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            && let Some(prop) = self.arena.get_shorthand_property(node)
            && self.assignment_matches_reference_core(prop.name, target, check_property_access)
        {
            return true;
        }

        if (node.kind == syntax_kind_ext::SPREAD_ELEMENT
            || node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT)
            && let Some(spread) = self.arena.get_spread(node)
            && self.assignment_matches_reference_core(
                spread.expression,
                target,
                check_property_access,
            )
        {
            return true;
        }

        if (node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
            && let Some(pattern) = self.arena.get_binding_pattern(node)
        {
            for &elem in &pattern.elements.nodes {
                if elem.is_none() {
                    continue;
                }
                if self.assignment_matches_reference_core(elem, target, check_property_access) {
                    return true;
                }
            }
        }

        if node.kind == syntax_kind_ext::BINDING_ELEMENT
            && let Some(binding) = self.arena.get_binding_element(node)
            && self.assignment_matches_reference_core(binding.name, target, check_property_access)
        {
            return true;
        }

        false
    }

    pub(crate) fn array_mutation_affects_reference(
        &self,
        call: &CallExprData,
        target: NodeIndex,
    ) -> bool {
        let Some(callee_node) = self.arena.get(call.expression) else {
            return false;
        };
        let Some(access) = self.arena.get_access_expr(callee_node) else {
            return false;
        };
        if access.question_dot_token {
            return false;
        }
        self.assignment_affects_reference(access.expression, target)
    }

    pub(crate) fn narrow_by_call_predicate(
        &self,
        type_id: TypeId,
        call: &CallExprData,
        target: NodeIndex,
        is_true_branch: bool,
    ) -> Option<TypeId> {
        // CRITICAL: Optional chaining behavior for type predicates
        // If call is optional (obj?.method(x)):
        //   - If true branch: method was called, so narrowing applies
        //   - If false branch: method might not have been called, so NO narrowing
        // Check if the callee expression is an optional property access
        if let Some(callee_node) = self.arena.get(call.expression)
            && (callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || callee_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.arena.get_access_expr(callee_node)
            && access.question_dot_token
        {
            // For optional chaining, only narrow the true branch
            if !is_true_branch {
                return None;
            }
        }

        let node_types = self.node_types?;
        let callee_type = *node_types.get(&call.expression.0)?;
        let signature = self.predicate_signature_for_type(callee_type)?;
        let predicate_target =
            self.predicate_target_expression(call, &signature.predicate, &signature.params)?;

        if !self.is_matching_reference(predicate_target, target) {
            // Optional chain intermediate narrowing:
            // When a type predicate on `x?.y?.z` would make the chain non-nullish,
            // intermediates `x` and `x.y` must also be non-nullish.
            // Applies in both branches (TRUE of isNotNull, FALSE of isNil, etc.).
            if self.contains_optional_chain(predicate_target)
                && self.is_optional_chain_prefix(predicate_target, target)
            {
                let narrowing = self.make_narrowing_context();
                let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                return Some(narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED));
            }
            return None;
        }

        // Resolve generic predicates: for `hasOwnProperty<P>(target, property: P): target is { [K in P]: unknown }`,
        // we need to instantiate the predicate type with inferred type arguments (e.g., P = "length").
        let resolved_predicate = self.resolve_generic_predicate(
            &signature.predicate,
            &signature.params,
            call,
            callee_type,
            node_types,
        );
        Some(self.apply_type_predicate_narrowing(type_id, &resolved_predicate, is_true_branch))
    }

    pub(crate) fn predicate_signature_for_type(
        &self,
        callee_type: TypeId,
    ) -> Option<PredicateSignature> {
        // Resolve Lazy(DefId) types before classification — type aliases
        // for callback types (e.g., JSDoc @callback) are stored as Lazy(DefId)
        // and must be resolved to their underlying function type first.
        let resolved_type =
            if let Some(def_id) = flow_query::get_lazy_def_id(self.interner, callee_type) {
                if let Some(env) = self.type_environment {
                    env.borrow().get_def(def_id).unwrap_or(callee_type)
                } else {
                    callee_type
                }
            } else {
                callee_type
            };
        match classify_for_predicate_signature(self.interner, resolved_type) {
            PredicateSignatureKind::Function(_) | PredicateSignatureKind::Callable(_) => {
                // Delegate to solver query for Function and Callable types.
                // For Callable, this picks the first signature with a predicate (heuristic).
                let extracted =
                    flow_query::extract_predicate_signature(self.interner, resolved_type)?;
                Some(PredicateSignature {
                    predicate: extracted.predicate,
                    params: extracted.params,
                })
            }
            PredicateSignatureKind::Union(members) => {
                // For unions, all members must either:
                //   (a) be a type predicate (contributing to the common narrowing), or
                //   (b) be a non-predicate callable that returns exclusively `false` or `never`.
                // A member returning general `boolean` (or any non-false truthy type) makes
                // the overall union guard unsound, regardless of predicate target.
                // If multiple predicate members exist, their predicates must match.
                //
                let mut common_sig: Option<PredicateSignature> = None;
                let mut has_non_predicate_boolean = false;

                for member in members {
                    if let Some(sig) = self.predicate_signature_for_type(member) {
                        if let Some(ref common) = common_sig {
                            if common.predicate != sig.predicate {
                                return None;
                            }
                        } else {
                            common_sig = Some(sig);
                        }
                    } else {
                        // Non-predicate member: only allowed if it returns exclusively `false`
                        // or `never`. A member returning `boolean` (or any truthy type) makes
                        // the overall union guard unsound.
                        if !callable_returns_only_false_or_never(self.interner, member) {
                            has_non_predicate_boolean = true;
                        }
                    }
                }
                // If any non-predicate member returns something other than `false`/`never`,
                // the union is NOT a type predicate — regardless of whether the predicate
                // targets `this` or a parameter.  This matches tsc behavior.
                if has_non_predicate_boolean {
                    return None;
                }
                common_sig
            }
            PredicateSignatureKind::Intersection(members) => {
                // For intersections, search ALL members and return the first predicate found
                // Intersections of functions are rare but possible (e.g., overloaded functions)
                // In an intersection A & B, if A has a predicate, the intersection has that predicate
                for member in members {
                    if let Some(sig) = self.predicate_signature_for_type(member) {
                        return Some(sig);
                    }
                }
                None
            }
            PredicateSignatureKind::None => None,
        }
    }

    pub(crate) fn predicate_target_expression(
        &self,
        call: &CallExprData,
        predicate: &TypePredicate,
        params: &[ParamInfo],
    ) -> Option<NodeIndex> {
        match predicate.target {
            TypePredicateTarget::Identifier(name) => {
                let param_index = params.iter().position(|param| param.name == Some(name))?;
                let args = call.arguments.as_ref()?.nodes.as_slice();

                // Walk through arguments, accounting for spread elements.
                // A spread argument expands to an unknown number of positional args,
                // so once we encounter one we can no longer map param_index to a
                // specific argument expression — bail out.
                for (arg_pos, &arg_idx) in args.iter().enumerate() {
                    if let Some(arg_node) = self.arena.get(arg_idx)
                        && arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                    {
                        return None;
                    }
                    if arg_pos == param_index {
                        return Some(arg_idx);
                    }
                }
                None
            }
            TypePredicateTarget::This => {
                // CRITICAL: Skip parens/assertions to find the actual access node
                // Handles cases like (obj.isString)() and (obj.isString as any)()
                let callee_idx = self.skip_parens_and_assertions(call.expression);
                // Check for PropertyAccess or ElementAccess
                if let Some(access) = self.arena.get_access_expr_at(callee_idx) {
                    return Some(access.expression);
                }

                None
            }
        }
    }

    /// Resolve a generic assertion predicate's type from the call's actual argument types.
    ///
    /// For `assertEqual<T>(value: any, type: T): asserts value is T` called as
    /// `assertEqual(animal.type, 'cat' as const)`, the predicate's `type_id` is the
    /// unresolved type parameter T. This method finds which parameter shares that type
    /// and resolves it to the corresponding argument's concrete type (e.g., literal 'cat').
    pub(crate) fn resolve_generic_predicate(
        &self,
        predicate: &TypePredicate,
        params: &[ParamInfo],
        call: &CallExprData,
        callee_type: TypeId,
        node_types: &crate::context::NodeTypeCache,
    ) -> TypePredicate {
        let Some(pred_type) = predicate.type_id else {
            return *predicate;
        };

        // Extract type params from the predicate signature via solver query
        let type_params = flow_query::extract_predicate_signature(self.interner, callee_type)
            .map(|sig| sig.type_params)
            .unwrap_or_default();
        if type_params.is_empty() {
            return *predicate;
        }

        let args = match call.arguments.as_ref() {
            Some(args) => args.nodes.as_slice(),
            None => return *predicate,
        };

        // Case 1: Direct match — predicate type IS a type parameter (e.g., `x is T`)
        for (i, param) in params.iter().enumerate() {
            if param.type_id == pred_type
                && let Some(&arg_idx) = args.get(i)
                && let Some(&arg_type) = node_types.get(&arg_idx.0)
            {
                return TypePredicate {
                    type_id: Some(arg_type),
                    ..*predicate
                };
            }
        }

        // Case 1b: Predicate type is a type parameter T, but the parameter type is a
        // union containing T (e.g., `isSuccess<T>(result: Result<T>): result is T`
        // where `type Result<T> = T | "FAILURE"`).
        // Infer T by subtracting the non-T union members from the argument type.
        // The parameter type may be a type alias (Lazy/Application) that needs
        // evaluation to expose the underlying union.
        let pred_is_type_param = type_params.iter().any(|tp| {
            flow_query::type_param_info(self.interner, pred_type)
                .is_some_and(|info| info.name == tp.name)
        });
        if pred_is_type_param {
            for (i, param) in params.iter().enumerate() {
                // Evaluate the parameter type in case it's a type alias like Result<T>.
                // Use the flow query boundary to expand type applications
                // (e.g., Result<T> -> T | "FAILURE").
                let evaluated_param = if let Some(env) = &self.type_environment {
                    let env_borrow = env.borrow();
                    flow_query::evaluate_application_type(self.interner, &env_borrow, param.type_id)
                } else {
                    flow_query::evaluate_type_structure(self.interner, param.type_id)
                };
                if let Some(param_members) = union_members_for_type(self.interner, evaluated_param)
                    && param_members.contains(&pred_type)
                    && let Some(&arg_idx) = args.get(i)
                    && let Some(&arg_type) = node_types.get(&arg_idx.0)
                {
                    let concrete_members: Vec<TypeId> = param_members
                        .iter()
                        .filter(|&&m| m != pred_type)
                        .copied()
                        .collect();
                    let inferred_t = if let Some(arg_members) =
                        union_members_for_type(self.interner, arg_type)
                    {
                        let remaining: Vec<TypeId> = arg_members
                            .iter()
                            .filter(|&&m| !concrete_members.contains(&m))
                            .copied()
                            .collect();
                        match remaining.len() {
                            0 => arg_type,
                            1 => remaining[0],
                            _ => self.interner.factory().union(remaining),
                        }
                    } else {
                        arg_type
                    };
                    return TypePredicate {
                        type_id: Some(inferred_t),
                        ..*predicate
                    };
                }
            }
        }

        // Case 2: Complex predicate type CONTAINS type parameters (e.g., mapped types
        // like `target is { readonly [K in P]: unknown }`). Build a substitution from
        // function type params to call argument types and instantiate the predicate type.
        let mut substitution = crate::query_boundaries::common::TypeSubstitution::new();
        for tp in &type_params {
            for (i, param) in params.iter().enumerate() {
                if let Some(info) = flow_query::type_param_info(self.interner, param.type_id)
                    && info.name == tp.name
                {
                    if let Some(&arg_idx) = args.get(i)
                        && let Some(&arg_type) = node_types.get(&arg_idx.0)
                    {
                        substitution.insert(tp.name, arg_type);
                    }
                    break;
                }
            }
        }

        if !substitution.is_empty() {
            let instantiated = crate::query_boundaries::common::instantiate_type(
                self.interner,
                pred_type,
                &substitution,
            );
            if instantiated != pred_type {
                // Evaluate to resolve mapped types (e.g., `{ [K in "length"]: unknown }` -> `{ length: unknown }`)
                let evaluated = flow_query::evaluate_type_structure(self.interner, instantiated);
                return TypePredicate {
                    type_id: Some(evaluated),
                    ..*predicate
                };
            }
        }

        // Case 3: The predicate type is a TypeParameter whose corresponding
        // parameter type is a wrapper (e.g., `Result<T>` instead of just `T`).
        // Case 1 failed because param.type_id != pred_type, and Case 2 failed
        // because param.type_id is not directly a TypeParameter.
        //
        // Handle the common pattern where the parameter type is a union
        // containing the type parameter:
        //   `function isSuccess<T>(result: T | "FAILURE"): result is T`
        //   param type = T | "FAILURE", arg type = number | "FAILURE"
        //   → infer T = number (subtract fixed union members from arg type)
        if let Some(pred_param_info) = flow_query::type_param_info(self.interner, pred_type) {
            let pred_param_name = pred_param_info.name;
            if type_params.iter().any(|tp| tp.name == pred_param_name) {
                for (i, param) in params.iter().enumerate() {
                    if let Some(&arg_idx) = args.get(i)
                        && let Some(&arg_type) = node_types.get(&arg_idx.0)
                        && let Some(inferred) = self.infer_type_param_from_union(
                            param.type_id,
                            arg_type,
                            pred_param_name,
                        )
                    {
                        return TypePredicate {
                            type_id: Some(inferred),
                            ..*predicate
                        };
                    }
                }
            }
        }

        *predicate
    }

    /// Attempt to infer a type parameter from a union-typed parameter.
    ///
    /// When a parameter type is a union like `T | "FAILURE"` and the argument type
    /// is `number | "FAILURE"`, we can infer T = number by subtracting the fixed
    /// (non-type-parameter) union members from the argument type.
    ///
    /// Returns `Some(inferred_type)` if inference succeeds, `None` otherwise.
    fn infer_type_param_from_union(
        &self,
        param_type: TypeId,
        arg_type: TypeId,
        target_param_name: Atom,
    ) -> Option<TypeId> {
        // Evaluate/expand the parameter type to get its structural form.
        // Type aliases like `Result<T>` may be represented as Application types
        // that need expansion to reveal the underlying union `T | "FAILURE"`.
        // Use the type environment's resolver for Application types.
        let expanded_param = if let Some(env_ref) = &self.type_environment {
            let env = env_ref.borrow();
            let result = flow_query::evaluate_application_type(self.interner, &env, param_type);
            if result == param_type {
                flow_query::evaluate_type_structure(self.interner, param_type)
            } else {
                result
            }
        } else {
            flow_query::evaluate_type_structure(self.interner, param_type)
        };
        // Get union members of the (expanded) parameter type
        let param_members = union_members_for_type(self.interner, expanded_param)?;

        // Check if any member is the target type parameter
        let is_target_param = |m: TypeId| -> bool {
            flow_query::type_param_info(self.interner, m)
                .is_some_and(|info| info.name == target_param_name)
        };
        let has_target_param = param_members.iter().any(|&m| is_target_param(m));
        if !has_target_param {
            return None;
        }

        // Collect the fixed (non-type-parameter) members from the parameter type.
        // Evaluate each to resolve Lazy/Application types to their concrete forms
        // (e.g., `FAILURE` alias → `"FAILURE"` literal) so TypeId comparison works
        // when subtracting from arg members.
        let fixed_members: Vec<TypeId> = param_members
            .iter()
            .copied()
            .filter(|&m| !is_target_param(m))
            .map(|m| {
                // Resolve Lazy/Application types to their concrete forms.
                // Lazy(DefId) types are type aliases that need resolver lookup.
                if let Some(env_ref) = &self.type_environment {
                    let env = env_ref.borrow();
                    // First try to resolve Lazy types via the environment
                    if let Some(def_id) = flow_query::get_lazy_def_id(self.interner, m)
                        && let Some(resolved) = env.get_def(def_id)
                    {
                        return resolved;
                    }
                    // Then try Application evaluation
                    let result = flow_query::evaluate_application_type(self.interner, &env, m);
                    if result != m {
                        return result;
                    }
                }
                flow_query::evaluate_type_structure(self.interner, m)
            })
            .collect();

        // Get union members of the argument type (or treat as single-member)
        let arg_members =
            union_members_for_type(self.interner, arg_type).unwrap_or_else(|| vec![arg_type]);

        // Subtract the fixed members from the argument type
        let remaining: Vec<TypeId> = arg_members
            .iter()
            .copied()
            .filter(|arg_m| !fixed_members.contains(arg_m))
            .collect();

        if remaining.is_empty() {
            return None;
        }

        // Build the inferred type from the remaining members
        let inferred = if remaining.len() == 1 {
            remaining[0]
        } else {
            self.interner.union(remaining)
        };
        Some(inferred)
    }

    pub(crate) fn apply_type_predicate_narrowing(
        &self,
        type_id: TypeId,
        predicate: &TypePredicate,
        is_true_branch: bool,
    ) -> TypeId {
        if predicate.asserts && !is_true_branch {
            return type_id;
        }

        // Create narrowing context and wire up TypeEnvironment if available
        let env_borrow;
        let narrowing = if let Some(env) = &self.type_environment {
            env_borrow = env.borrow();
            self.make_narrowing_context().with_resolver(&*env_borrow)
        } else {
            self.make_narrowing_context()
        };

        if let Some(predicate_type) = predicate.type_id {
            // Route through TypeGuard::Predicate for proper intersection semantics.
            // When source and target don't overlap (e.g. successive type guards
            // hasLegs then hasWings), the solver falls back to intersection.
            let guard = TypeGuard::Predicate {
                type_id: Some(predicate_type),
                asserts: predicate.asserts,
            };
            return narrowing.narrow_type(type_id, &guard, GuardSense::from(is_true_branch));
        }

        // Assertion guards without type predicate (asserts x) narrow to truthy
        // This is the CRITICAL fix: use TypeGuard::Truthy instead of just excluding null/undefined
        if is_true_branch {
            // Delegate to narrow_type with TypeGuard::Truthy for comprehensive narrowing
            return narrowing.narrow_type(type_id, &TypeGuard::Truthy, GuardSense::Positive);
        }

        // Use Solver's narrow_to_falsy for correct NaN handling
        narrowing.narrow_to_falsy(type_id)
    }

    pub(crate) fn narrow_by_instanceof(
        &self,
        type_id: TypeId,
        bin: &tsz_parser::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
    ) -> TypeId {
        if !self.is_matching_reference(bin.left, target) {
            return type_id;
        }

        // When the constructor expression is typed as `any`, instanceof narrowing is not
        // well-defined — TypeScript keeps the source type unchanged in this case.
        let constructor_expr_type = self.node_types.and_then(|nt| nt.get(&bin.right.0).copied());
        if constructor_expr_type == Some(TypeId::ANY) {
            return type_id;
        }

        // Extract instance type from constructor expression (AST -> TypeId).
        // If we can't determine the instance type:
        // - True branch: narrow to object-like types (instanceof always returns
        //   false for null/undefined/primitives)
        // - False branch: keep source type unchanged (we can't exclude anything
        //   without knowing the constructor)
        // We must NOT fall back to TypeId::OBJECT because the solver would treat
        // it as `instanceof Object` and incorrectly exclude all non-primitives
        // from the false branch.
        let instance_type = match self.instance_type_from_constructor(bin.right) {
            Some(t) => t,
            None => {
                if is_true_branch {
                    // Even without knowing the constructor, instanceof true
                    // means the value is definitely an object (not null/undefined).
                    let env_borrow;
                    let narrowing = if let Some(env) = &self.type_environment {
                        env_borrow = env.borrow();
                        self.make_narrowing_context().with_resolver(&*env_borrow)
                    } else {
                        self.make_narrowing_context()
                    };
                    return narrowing.narrow_to_objectish(type_id);
                }
                return type_id;
            }
        };

        // Delegate to solver via unified narrow_type API
        let env_borrow;
        let narrowing = if let Some(env) = &self.type_environment {
            env_borrow = env.borrow();
            self.make_narrowing_context().with_resolver(&*env_borrow)
        } else {
            self.make_narrowing_context()
        };

        narrowing.narrow_type(
            type_id,
            &TypeGuard::Instanceof(instance_type, false),
            GuardSense::from(is_true_branch),
        )
    }

    pub(crate) fn instance_type_from_constructor(&self, expr: NodeIndex) -> Option<TypeId> {
        if let Some(node_types) = self.node_types
            && let Some(&type_id) = node_types.get(&expr.0)
            && let Some(instance_type) =
                crate::query_boundaries::flow_analysis::instance_type_from_constructor(
                    self.interner,
                    type_id,
                )
        {
            return Some(instance_type);
        }

        let expr = self.skip_parens_and_assertions(expr);
        let sym_id = self.binder.resolve_identifier(self.arena, expr)?;
        let symbol = self.binder.get_symbol(sym_id)?;
        let symbol_ref = tsz_solver::SymbolRef(sym_id.0);
        if (symbol.flags & symbol_flags::CLASS) != 0 {
            return Some(
                self.resolve_symbol_to_lazy(symbol_ref)
                    .unwrap_or_else(|| self.interner.reference(symbol_ref)),
            );
        }

        // Global constructor variables (e.g., `declare var Array: ArrayConstructor`)
        // have both INTERFACE and VARIABLE flags. The interface type IS the instance type
        // since interfaces describe instances, not constructors.
        // This handles `x instanceof Array`, `x instanceof Date`, etc.
        if (symbol.flags & symbol_flags::INTERFACE) != 0
            && (symbol.flags & symbol_flags::VARIABLE) != 0
        {
            return Some(
                self.resolve_symbol_to_lazy(symbol_ref)
                    .unwrap_or_else(|| self.interner.reference(symbol_ref)),
            );
        }

        // For plain VARIABLE symbols (e.g., `declare var C: CConstructor`),
        // resolve the variable's type annotation to find the constructor type,
        // then extract the instance type from its construct signatures or prototype.
        if (symbol.flags & symbol_flags::VARIABLE) != 0 {
            // Strategy 1: Try type environment lookup for the variable
            if let Some(env) = &self.type_environment {
                let env_borrow = env.borrow();
                if let Some(constructor_type) = env_borrow.get(symbol_ref)
                    && let Some(instance_type) =
                        crate::query_boundaries::flow_analysis::instance_type_from_constructor(
                            self.interner,
                            constructor_type,
                        )
                {
                    return Some(instance_type);
                }
            }

            // Strategy 2: Follow the variable's type annotation to find the
            // constructor interface/class type, then look THAT up in the env.
            // For `declare var C: CConstructor`, we need to find the `CConstructor`
            // symbol from the type annotation and resolve its type.
            if let Some(instance_type) = self.instance_type_from_variable_annotation(symbol) {
                return Some(instance_type);
            }
        }

        None
    }

    /// For VARIABLE symbols, follow the type annotation on the variable
    /// declaration to find the constructor type, then extract the instance type.
    ///
    /// Example: `declare var C: CConstructor;` — find `CConstructor` from
    /// the type annotation, look it up in the type environment, and extract
    /// the instance type from its construct signatures or prototype property.
    fn instance_type_from_variable_annotation(
        &self,
        symbol: &tsz_binder::Symbol,
    ) -> Option<TypeId> {
        // Get the variable's first declaration
        let decl_idx = symbol.declarations.first().copied()?;
        let decl_node = self.arena.get(decl_idx)?;

        // Get the VariableDeclaration data to access the type annotation
        let var_decl = self.arena.get_variable_declaration(decl_node)?;
        if var_decl.type_annotation == tsz_parser::NodeIndex::NONE {
            return None;
        }

        // The type annotation is a TypeReference node; find its identifier
        let type_ref_node = self.arena.get(var_decl.type_annotation)?;
        let type_name_idx = if let Some(type_ref) = self.arena.get_type_ref(type_ref_node) {
            type_ref.type_name
        } else {
            return None;
        };

        // Resolve the type name identifier to a symbol
        let type_sym_id = self.binder.resolve_identifier(self.arena, type_name_idx)?;
        let type_symbol_ref = tsz_solver::SymbolRef(type_sym_id.0);

        // Look up the constructor type in the type environment
        if let Some(env) = &self.type_environment {
            let env_borrow = env.borrow();
            if let Some(constructor_type) = env_borrow.get(type_symbol_ref)
                && let Some(instance_type) =
                    crate::query_boundaries::flow_analysis::instance_type_from_constructor(
                        self.interner,
                        constructor_type,
                    )
            {
                return Some(instance_type);
            }
        }

        // Fallback: create a lazy reference to the type annotation's symbol
        // and try to extract the instance type from it
        if let Some(lazy_type) = self.resolve_symbol_to_lazy(type_symbol_ref)
            && let Some(instance_type) =
                crate::query_boundaries::flow_analysis::instance_type_from_constructor(
                    self.interner,
                    lazy_type,
                )
        {
            return Some(instance_type);
        }

        None
    }

    pub(crate) fn narrow_by_in_operator(
        &self,
        type_id: TypeId,
        bin: &tsz_parser::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
    ) -> TypeId {
        // AST extraction: check if we're narrowing the right reference
        if !self.is_matching_reference(bin.right, target) {
            return type_id;
        }

        // AST extraction: get property name from left side of `in` operator
        let Some((prop_name, _prop_is_number)) = self.in_property_name(bin.left) else {
            return type_id;
        };

        // Delegate ALL type algebra to solver via unified narrow_type API
        // Solver handles: ANY, UNKNOWN, type parameters, unions, non-union types
        let env_borrow;
        let narrowing = if let Some(env) = &self.type_environment {
            env_borrow = env.borrow();
            self.make_narrowing_context().with_resolver(&*env_borrow)
        } else {
            self.make_narrowing_context()
        };

        narrowing.narrow_type(
            type_id,
            &TypeGuard::InProperty(prop_name),
            GuardSense::from(is_true_branch),
        )
    }

    pub(crate) fn in_property_name(&self, idx: NodeIndex) -> Option<(Atom, bool)> {
        let idx = self.skip_parenthesized(idx);

        // Handle private identifiers (e.g., `#field in obj`)
        if let Some(node) = self.arena.get(idx)
            && node.kind == SyntaxKind::PrivateIdentifier as u16
            && let Some(ident) = self.arena.get_identifier(node)
        {
            return Some((self.interner.intern_string(&ident.escaped_text), false));
        }

        self.literal_atom_and_kind_from_node_or_type(idx)
    }

    pub(crate) fn skip_parenthesized(&self, idx: NodeIndex) -> NodeIndex {
        self.arena.skip_parenthesized_and_assertions_and_comma(idx)
    }

    pub(crate) fn skip_parens_and_assertions(&self, idx: NodeIndex) -> NodeIndex {
        self.arena.skip_parenthesized_and_assertions(idx)
    }

    pub(crate) fn typeof_comparison_literal(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        target: NodeIndex,
    ) -> Option<&str> {
        if self.is_typeof_target(left, target) {
            return self.literal_string_from_node(right);
        }
        if self.is_typeof_target(right, target) {
            return self.literal_string_from_node(left);
        }
        None
    }

    pub(crate) fn is_typeof_target(&self, expr: NodeIndex, target: NodeIndex) -> bool {
        let expr = self.skip_parenthesized(expr);
        let node = match self.arena.get(expr) {
            Some(node) => node,
            None => return false,
        };

        if node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            return false;
        }

        let Some(unary) = self.arena.get_unary_expr(node) else {
            return false;
        };

        if unary.operator != SyntaxKind::TypeOfKeyword as u16 {
            return false;
        }

        self.is_matching_reference(unary.operand, target)
    }

    pub(crate) fn literal_string_from_node(&self, idx: NodeIndex) -> Option<&str> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return self.arena.get_literal(node).map(|lit| lit.text.as_str());
        }

        // Handle private identifiers (e.g., #a) for `in` operator narrowing
        if node.kind == SyntaxKind::PrivateIdentifier as u16 {
            return self
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.as_str());
        }

        None
    }

    pub(crate) fn literal_type_from_node(&self, idx: NodeIndex) -> Option<TypeId> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.arena.get_literal(node)?;
                Some(self.interner.literal_string(&lit.text))
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                let value = self.parse_numeric_literal_value(lit.value, &lit.text)?;
                Some(self.interner.literal_number(value))
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                let text = lit.text.strip_suffix('n').unwrap_or(&lit.text);
                let normalized = self.normalize_bigint_literal(text)?;
                Some(self.interner.literal_bigint(normalized.as_ref()))
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some(self.interner.literal_boolean(true)),
            k if k == SyntaxKind::FalseKeyword as u16 => Some(self.interner.literal_boolean(false)),
            k if k == SyntaxKind::NullKeyword as u16 => Some(TypeId::NULL),
            k if k == SyntaxKind::UndefinedKeyword as u16 => Some(TypeId::UNDEFINED),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.arena.get_unary_expr(node)?;
                let op = unary.operator;
                if op != SyntaxKind::MinusToken as u16 && op != SyntaxKind::PlusToken as u16 {
                    return None;
                }

                let operand = self.skip_parenthesized(unary.operand);
                let operand_node = self.arena.get(operand)?;
                match operand_node.kind {
                    k if k == SyntaxKind::NumericLiteral as u16 => {
                        let lit = self.arena.get_literal(operand_node)?;
                        let value = self.parse_numeric_literal_value(lit.value, &lit.text)?;
                        let value = if op == SyntaxKind::MinusToken as u16 {
                            -value
                        } else {
                            value
                        };
                        Some(self.interner.literal_number(value))
                    }
                    k if k == SyntaxKind::BigIntLiteral as u16 => {
                        let lit = self.arena.get_literal(operand_node)?;
                        let text = lit.text.strip_suffix('n').unwrap_or(&lit.text);
                        let normalized = self.normalize_bigint_literal(text)?;
                        let negative = op == SyntaxKind::MinusToken as u16;
                        Some(
                            self.interner
                                .literal_bigint_with_sign(negative, normalized.as_ref()),
                        )
                    }
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                // Template expression with substitutions like `${AnimalType.cat}`.
                // Try to evaluate as a literal string when all parts are known literals.
                // This enables discriminated union narrowing in switch cases like:
                //   case `${AnimalType.cat}`: ...
                self.literal_type_from_template_expression(idx, node)
            }
            _ => {
                // Handle `undefined` in value position (it's an Identifier, not UndefinedKeyword)
                if let Some(ident) = self.arena.get_identifier(node)
                    && ident.escaped_text == "undefined"
                {
                    return Some(TypeId::UNDEFINED);
                }
                // Fallback: look up the already-computed type for this expression.
                // This handles enum member access (e.g., Types.Str), const enum members,
                // and other expressions that evaluate to literal or enum types.
                if let Some(node_types) = self.node_types
                    && let Some(&type_id) = node_types.get(&idx.0)
                {
                    return is_narrowing_literal(self.interner, type_id);
                }

                // Second fallback: resolve enum member accesses through the type
                // environment when node_types is unavailable (e.g., during call
                // argument collection where node_types is temporarily cleared).
                if let Some(type_id) = self.resolve_enum_member_via_env(idx, node) {
                    return is_narrowing_literal(self.interner, type_id);
                }

                None
            }
        }
    }

    /// Try to evaluate a template expression to a literal string type.
    ///
    /// For template expressions like `` `${AnimalType.cat}` ``, examines each
    /// span's expression type. If all expressions resolve to known literal
    /// values (string/number/boolean literals or enum members wrapping them),
    /// concatenates the parts and returns the resulting string literal type.
    ///
    /// This enables discriminated union narrowing when switch cases use
    /// template expressions with enum values as discriminants.
    fn literal_type_from_template_expression(
        &self,
        _idx: NodeIndex,
        node: &tsz_parser::parser::node::Node,
    ) -> Option<TypeId> {
        let template = self.arena.get_template_expr(node)?;

        // Get the head text (text before the first ${})
        let head_node = self.arena.get(template.head)?;
        let head_lit = self.arena.get_literal(head_node)?;
        let mut result = head_lit.text.clone();

        for &span_idx in &template.template_spans.nodes {
            let span_node = self.arena.get(span_idx)?;
            let span = self.arena.get_template_span(span_node)?;

            // Get the expression type. First try node_types, then try extracting
            // a literal type from the sub-expression AST directly (handles cases
            // where node_types isn't populated yet during flow analysis).
            let expr_type = if let Some(node_types) = self.node_types
                && let Some(&ty) = node_types.get(&span.expression.0)
            {
                ty
            } else {
                // Fallback: try to extract a literal from the sub-expression
                // via the same literal_type_from_node path (handles enum member
                // access like AnimalType.cat directly from the AST).
                self.literal_type_from_node(span.expression)?
            };

            // Extract the string representation of the literal type.
            // If the expression doesn't resolve to a known literal, bail out.
            let literal_str = stringify_literal_type(self.interner, expr_type)?;
            result.push_str(&literal_str);

            // Get the tail text (text after the } and before the next ${ or `)
            let tail_node = self.arena.get(span.literal)?;
            let tail_lit = self.arena.get_literal(tail_node)?;
            result.push_str(&tail_lit.text);
        }

        Some(self.interner.literal_string(&result))
    }

    /// Resolve an enum member property access (e.g., `AnimalType.cat`) to its
    /// type via the type environment, bypassing `node_types`.
    ///
    /// During call argument collection, `node_types` is temporarily cleared for
    /// overload resolution. This method resolves the enum member by:
    /// 1. Parsing the property access to get base + member name
    /// 2. Resolving the base to the enum symbol via the binder
    /// 3. Looking up the member in the enum's exports to get its SymbolId
    /// 4. Looking up the member's type via `SymbolRef` in the type environment
    fn resolve_enum_member_via_env(
        &self,
        _idx: NodeIndex,
        node: &tsz_parser::parser::node::Node,
    ) -> Option<TypeId> {
        let access = self.arena.get_access_expr(node)?;
        let base_expr = access.expression;
        let member_name_node = access.name_or_argument;

        // Get the member name — identifier for En.B, string literal for En["B"]
        let member_name_owned: String;
        if let Some(member_ident) = self.arena.get_identifier_at(member_name_node) {
            member_name_owned = member_ident.escaped_text.clone();
        } else if let Some(member_node) = self.arena.get(member_name_node) {
            if member_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
                || member_node.kind == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16
            {
                if let Some(lit) = self.arena.get_literal(member_node) {
                    member_name_owned = lit.text.clone();
                } else {
                    return None;
                }
            } else {
                return None;
            }
        } else {
            return None;
        }
        let member_name = &member_name_owned;

        // Resolve the base expression to the enum symbol
        let base_sym_id = self
            .binder
            .resolve_identifier(self.arena, base_expr)
            .or_else(|| self.binder.get_node_symbol(base_expr))?;
        let base_sym = self.binder.get_symbol(base_sym_id)?;

        // Check that the base is an enum
        if base_sym.flags & symbol_flags::ENUM == 0 {
            return None;
        }

        // Look up the member in the enum's exports
        let exports = base_sym.exports.as_ref()?;
        let member_sym_id = exports.get(member_name)?;

        // Look up the member's type through the type environment
        let type_env = self.type_environment.as_ref()?;
        let env = type_env.borrow();
        let sym_ref = tsz_solver::SymbolRef(member_sym_id.0);
        env.get(sym_ref)
    }

    pub(crate) fn nullish_literal_type(&self, idx: NodeIndex) -> Option<TypeId> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        if node.kind == SyntaxKind::NullKeyword as u16 {
            return Some(TypeId::NULL);
        }
        if node.kind == SyntaxKind::UndefinedKeyword as u16 {
            return Some(TypeId::UNDEFINED);
        }
        // In value position, `undefined` is an Identifier, not UndefinedKeyword
        if let Some(ident) = self.arena.get_identifier(node)
            && ident.escaped_text == "undefined"
        {
            return Some(TypeId::UNDEFINED);
        }

        None
    }

    pub(crate) fn nullish_comparison(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        target: NodeIndex,
    ) -> Option<TypeId> {
        if self.is_matching_reference(left, target) {
            return self.nullish_literal_type(right);
        }
        if self.is_matching_reference(right, target) {
            return self.nullish_literal_type(left);
        }
        None
    }

    pub(crate) fn discriminant_property(
        &self,
        expr: NodeIndex,
        target: NodeIndex,
    ) -> Option<Vec<Atom>> {
        self.discriminant_property_info(expr, target)
            .and_then(|(path, is_optional, base)| {
                if is_optional {
                    return None;
                }
                // Only apply discriminant narrowing if the base of the property
                // access matches the target being narrowed. For example, if narrowing
                // `x` based on `x.kind`, the base `x` must match target `x`.
                // Without this check, narrowing `x.prop` based on `x.kind` would
                // incorrectly try to find `kind` on the type of `x.prop`.
                self.is_matching_reference(base, target).then_some(path)
            })
    }

    /// For a const-declared identifier that is a destructuring alias,
    /// return the (`base_initializer`, `property_name`).
    ///
    /// Example: `const { type: alias } = obj` → `(obj, "type")`
    ///
    /// Returns `None` for non-identifiers, non-const bindings, or nested patterns.
    fn binding_element_property_alias(&self, node: NodeIndex) -> Option<(NodeIndex, Atom)> {
        let node_data = self.arena.get(node)?;
        if node_data.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.binder.resolve_identifier(self.arena, node)?;
        let symbol = self.binder.get_symbol(sym_id)?;
        // Must be a block-scoped (const/let) variable
        if (symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0 {
            return None;
        }
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let decl_node = self.arena.get(decl_idx)?;
        // `value_declaration` for destructuring bindings may point to the identifier node
        // (the name/alias) rather than the BINDING_ELEMENT itself, because the binder calls
        // `declare_symbol(name_ident, ...)`. In that case, walk up to the parent to find
        // the actual BINDING_ELEMENT.
        let decl_idx = if decl_node.kind == SyntaxKind::Identifier as u16 {
            let ext = self.arena.get_extended(decl_idx)?;
            ext.parent
        } else {
            decl_idx
        };
        let decl_node = self.arena.get(decl_idx)?;
        // Must be a binding element (from object destructuring)
        if decl_node.kind != syntax_kind_ext::BINDING_ELEMENT {
            return None;
        }
        let be = self.arena.get_binding_element(decl_node)?;
        // Must not be a rest element (`...rest`)
        if be.dot_dot_dot_token {
            return None;
        }
        // Must not have a default initializer (const { type: alias = "default" } = ...)
        if be.initializer.is_some() {
            return None;
        }
        // Get the property name being destructured
        // `{ type: alias }` → property_name node is "type"
        // `{ type }` shorthand → name node IS the property name
        let prop_name_idx = if be.property_name.is_some() {
            be.property_name
        } else {
            be.name
        };
        let prop_name_node = self.arena.get(prop_name_idx)?;
        let prop_ident = self.arena.get_identifier(prop_name_node)?;
        let prop_name = self.interner.intern_string(&prop_ident.escaped_text);

        // Walk up: BindingElement → ObjectBindingPattern → VariableDeclaration
        let be_ext = self.arena.get_extended(decl_idx)?;
        let binding_pattern_idx = be_ext.parent;
        if binding_pattern_idx.is_none() {
            return None;
        }
        let binding_pattern_node = self.arena.get(binding_pattern_idx)?;
        if binding_pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return None;
        }
        let bp_ext = self.arena.get_extended(binding_pattern_idx)?;
        let var_decl_idx = bp_ext.parent;
        if var_decl_idx.is_none() {
            return None;
        }
        let var_decl_node = self.arena.get(var_decl_idx)?;
        if var_decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        if !self.is_const_variable_declaration(var_decl_idx) {
            return None;
        }
        let var_decl = self.arena.get_variable_declaration(var_decl_node)?;
        if var_decl.initializer.is_none() {
            return None;
        }
        let base = self.skip_parenthesized(var_decl.initializer);
        Some((base, prop_name))
    }

    pub(crate) fn discriminant_property_info(
        &self,
        expr: NodeIndex,
        _target: NodeIndex,
    ) -> Option<(Vec<Atom>, bool, NodeIndex)> {
        let expr = self.skip_parenthesized(expr);
        self.arena.get(expr)?;

        // Collect the property path by walking up the access chain
        // For action.payload.kind, we want ["payload", "kind"]
        let mut path: Vec<Atom> = Vec::new();
        let mut is_optional = false;
        let mut current = expr;

        loop {
            let current_node = self.arena.get(current)?;
            let access = if current_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || current_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                self.arena.get_access_expr(current_node)?
            } else {
                // Not a property/element access - we've reached the base
                break;
            };

            // Track if any segment uses optional chaining
            if access.question_dot_token {
                is_optional = true;
            }

            // Get the property name for this segment
            let prop_name = if current_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let ident = self.arena.get_identifier_at(access.name_or_argument)?;
                self.interner.intern_string(&ident.escaped_text)
            } else {
                // Element access
                self.literal_atom_from_node_or_type(access.name_or_argument)?
            };

            // Add to path (will be reversed later)
            path.push(prop_name);

            // Move to the next level up
            let access_target = access.expression;
            let access_target = self.skip_parenthesized(access_target);
            let access_target_node = self.arena.get(access_target)?;

            // Unwrap assignment and comma expressions to get the actual target
            let effective_target = if access_target_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            {
                let binary = self.arena.get_binary_expr(access_target_node)?;
                if binary.operator_token == SyntaxKind::EqualsToken as u16 {
                    // (x = y).prop -> unwrap to x.prop
                    binary.left
                } else if binary.operator_token == SyntaxKind::CommaToken as u16 {
                    // (a, b).prop -> unwrap to b.prop
                    binary.right
                } else {
                    access_target
                }
            } else {
                access_target
            };

            current = effective_target;
        }

        // Reverse the path to get correct order (["payload", "kind"] not ["kind", "payload"])
        path.reverse();

        if path.is_empty() {
            return None;
        }

        // current is now the base (e.g., "action" in action.payload.kind)
        Some((path, is_optional, current))
    }

    pub(crate) fn discriminant_comparison(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        target: NodeIndex,
    ) -> Option<(Vec<Atom>, TypeId, bool, NodeIndex)> {
        // Use relative_discriminant_path to find the property path from target to left.
        // This correctly handles both:
        //   - Direct: `t.kind === "a"` narrowing `t` → path=["kind"], base=t
        //   - Nested: `this.test.type === "a"` narrowing `this.test` → path=["type"], base=this.test
        // (discriminant_property_info returns the full path from the root, which is wrong when
        //  target is not the root — e.g., returns path=["test","type"] base=this for `this.test.type`
        //  when we need path=["type"] base=this.test relative to target=this.test)
        if let Some(literal) = self.discriminant_literal_candidate(right)
            && let Some((rel_path, is_optional)) = self.relative_discriminant_path(left, target)
            && !rel_path.is_empty()
        {
            return Some((rel_path, literal, is_optional, target));
        }

        if let Some(literal) = self.discriminant_literal_candidate(left)
            && let Some((rel_path, is_optional)) = self.relative_discriminant_path(right, target)
            && !rel_path.is_empty()
        {
            return Some((rel_path, literal, is_optional, target));
        }

        // Try aliased discriminant: const alias = target.prop (or target.a.b)
        // where alias is a const identifier initialized from a property access of target.
        // e.g., `const testType = this.test.type` and target = `this.test`
        //   → path = ["type"], base = this.test
        // Also handles destructuring: `const { type: alias } = target`
        //   → path = ["type"], base = target
        if let Some(result) = self.aliased_discriminant(left, right, target) {
            return Some(result);
        }
        if let Some(result) = self.aliased_discriminant(right, left, target) {
            return Some(result);
        }

        None
    }

    fn discriminant_literal_candidate(&self, idx: NodeIndex) -> Option<TypeId> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        // Do not treat arbitrary identifiers as discriminant literals based on
        // flow-inferred node_types. This can incorrectly narrow unrelated targets
        // (e.g., `e === Ns.Enum.Member` while narrowing `Ns`).
        //
        // Keep two safe identifier cases:
        // 1) enum members,
        // 2) const aliases with literal initializers.
        if node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.arena.get_identifier(node)
                && ident.escaped_text == "undefined"
            {
                return Some(TypeId::UNDEFINED);
            }

            if let Some(sym_id) = self.reference_symbol(idx)
                && let Some(sym) = self.binder.get_symbol(sym_id)
                && (sym.flags & symbol_flags::ENUM_MEMBER) != 0
            {
                return self.literal_type_from_node(idx);
            }

            if let Some((_sym, initializer)) = self.const_condition_initializer(idx) {
                return self.literal_type_from_node(initializer);
            }

            return None;
        }

        self.literal_type_from_node(idx)
    }

    /// Try to extract a discriminant guard for an aliased condition.
    ///
    /// Handles:
    /// - `const alias = target.prop` → `alias === literal` narrows `target` by `prop`
    /// - `const { prop: alias } = target` → `alias === literal` narrows `target` by `prop`
    ///
    /// Returns `(path, literal_type, is_optional, base)` where `base = target`.
    fn aliased_discriminant(
        &self,
        alias_node: NodeIndex,
        literal_node: NodeIndex,
        target: NodeIndex,
    ) -> Option<(Vec<Atom>, TypeId, bool, NodeIndex)> {
        let node_data = self.arena.get(self.skip_parenthesized(alias_node))?;
        if node_data.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let literal = self.literal_type_from_node(literal_node)?;

        // Case 1: Simple const alias `const alias = target.prop` (or deeper: target.a.b)
        // Resolve alias to its property access initializer, then compute relative path.
        if let Some((_, initializer)) = self.const_condition_initializer(alias_node) {
            let init_expr = self.skip_parenthesized(initializer);
            let init_node = self.arena.get(init_expr)?;
            if init_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || init_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                // Walk from `init_expr` towards the root, collecting segments until we hit `target`.
                if let Some((rel_path, is_optional)) =
                    self.relative_discriminant_path(init_expr, target)
                {
                    return Some((rel_path, literal, is_optional, target));
                }
            }
        }

        // Case 2: Destructuring alias `const { prop: alias } = target`
        if let Some((base, prop_name)) = self.binding_element_property_alias(alias_node)
            && self.is_matching_reference(base, target)
        {
            return Some((vec![prop_name], literal, false, target));
        }

        None
    }

    /// Given a property access `prop_access` (e.g. `this.test.type`) and a target node
    /// (e.g. `this.test`), walk backwards collecting property names until we reach `target`.
    ///
    /// Returns `(relative_path, is_optional)` where `relative_path` is the list of property
    /// names from `target` to `prop_access` (e.g. `["type"]`).
    ///
    /// Returns `None` if `target` is not found in the access chain.
    pub(super) fn relative_discriminant_path(
        &self,
        prop_access: NodeIndex,
        target: NodeIndex,
    ) -> Option<(Vec<Atom>, bool)> {
        let mut path: Vec<Atom> = Vec::new();
        let mut is_optional = false;
        let mut current = self.skip_parenthesized(prop_access);

        loop {
            let current_node = self.arena.get(current)?;
            let access = if current_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || current_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                self.arena.get_access_expr(current_node)?
            } else {
                // Reached a non-access node without finding target
                return None;
            };

            if access.question_dot_token {
                is_optional = true;
            }

            let prop_name = if current_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let ident = self.arena.get_identifier_at(access.name_or_argument)?;
                self.interner.intern_string(&ident.escaped_text)
            } else {
                self.literal_atom_from_node_or_type(access.name_or_argument)?
            };

            // This is the prop name at the current level; push it (path is built backwards)
            path.push(prop_name);

            // Move to the base of this access
            let base_expr = self.skip_parenthesized(access.expression);

            // Check if the base matches the target
            if self.is_matching_reference(base_expr, target) {
                // Found! Reverse path to get correct order.
                path.reverse();
                return Some((path, is_optional));
            }

            current = base_expr;
        }
    }

    pub(crate) fn literal_comparison(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        target: NodeIndex,
    ) -> Option<TypeId> {
        if self.is_matching_reference(left, target) {
            return self.literal_type_from_node(right);
        }
        if self.is_matching_reference(right, target) {
            return self.literal_type_from_node(left);
        }
        None
    }

    /// For `typeof a.prop === "undefined"`, extract the property path from
    /// the typeof operand relative to `target` and the comparison literal.
    /// Returns (`property_path`, `is_optional_chain`, `typeof_literal_string`) if the typeof operand
    /// is a property access chain rooted at `target`.
    pub(super) fn typeof_discriminant_path(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        target: NodeIndex,
    ) -> Option<(Vec<Atom>, bool, &str)> {
        // Try left = typeof expr, right = string literal
        if let Some(operand) = self.get_typeof_operand(self.skip_parenthesized(left))
            && let Some((path, is_optional)) = self.relative_discriminant_path(operand, target)
            && !path.is_empty()
            && let Some(lit) = self.literal_string_from_node(right)
        {
            return Some((path, is_optional, lit));
        }
        // Try right = typeof expr, left = string literal
        if let Some(operand) = self.get_typeof_operand(self.skip_parenthesized(right))
            && let Some((path, is_optional)) = self.relative_discriminant_path(operand, target)
            && !path.is_empty()
            && let Some(lit) = self.literal_string_from_node(left)
        {
            return Some((path, is_optional, lit));
        }
        None
    }
}

/// Returns true if the callable type's return type is exclusively `false` or `never`.
///
/// Used to validate non-predicate members in a union of callables: TSC permits a union
/// to act as a type guard only when non-predicate members can never return a truthy value.
fn callable_returns_only_false_or_never(
    interner: &dyn tsz_solver::QueryDatabase,
    callable_type: TypeId,
) -> bool {
    match flow_query::function_return_type(interner, callable_type) {
        Some(rt) => flow_query::is_only_false_or_never(interner, rt),
        None => false,
    }
}
