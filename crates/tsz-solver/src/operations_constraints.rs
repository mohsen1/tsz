//! Type constraint collection for generic type inference.
//!
//! This module implements the structural walker that collects type constraints
//! when inferring generic type parameters from argument types. It handles
//! recursive traversal of complex type structures (objects, functions, tuples,
//! conditionals, mapped types, etc.) to extract inference candidates.

use crate::infer::InferenceContext;
use crate::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::{AssignabilityChecker, CallEvaluator, MAX_CONSTRAINT_RECURSION_DEPTH};
use crate::types::{
    CallSignature, FunctionShape, ObjectShape, ObjectShapeId, ParamInfo, PropertyInfo,
    TemplateSpan, TupleElement, TypeData, TypeId, TypeParamInfo, TypePredicate,
};
use crate::utils;
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::{debug, trace};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Structural walker to collect constraints: source <: target
    pub(crate) fn constrain_types(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::infer::InferenceVar>,
        source: TypeId,
        target: TypeId,
        priority: crate::types::InferencePriority,
    ) {
        if !self.constraint_pairs.borrow_mut().insert((source, target)) {
            return;
        }

        // Check and increment recursion depth to prevent infinite loops
        {
            let mut depth = self.constraint_recursion_depth.borrow_mut();
            if *depth >= MAX_CONSTRAINT_RECURSION_DEPTH {
                // Safety limit reached - return to prevent infinite loop
                return;
            }
            *depth += 1;
        }

        // Perform the actual constraint collection
        self.constrain_types_impl(ctx, var_map, source, target, priority);

        // Decrement depth on return
        *self.constraint_recursion_depth.borrow_mut() -= 1;
    }

    /// Inner implementation of `constrain_types`
    fn constrain_types_impl(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::infer::InferenceVar>,
        source: TypeId,
        target: TypeId,
        priority: crate::types::InferencePriority,
    ) {
        if source == target {
            return;
        }

        // If target is an inference placeholder, add lower bound: source <: var
        if let Some(&var) = var_map.get(&target) {
            ctx.add_candidate(var, source, priority);
            return;
        }

        // If source is an inference placeholder, add upper bound: var <: target
        if let Some(&var) = var_map.get(&source) {
            ctx.add_upper_bound(var, target);
            return;
        }

        // Recurse structurally
        let source_key = self.interner.lookup(source);
        let target_key = self.interner.lookup(target);

        let is_nullish = |ty: TypeId| matches!(ty, TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID);

        match (source_key, target_key) {
            (Some(TypeData::ReadonlyType(s_inner)), Some(TypeData::ReadonlyType(t_inner)))
            | (Some(TypeData::NoInfer(s_inner)), Some(TypeData::NoInfer(t_inner))) => {
                self.constrain_types(ctx, var_map, s_inner, t_inner, priority);
            }
            (Some(TypeData::ReadonlyType(s_inner)), _) => {
                self.constrain_types(ctx, var_map, s_inner, target, priority);
            }
            (_, Some(TypeData::ReadonlyType(t_inner))) => {
                self.constrain_types(ctx, var_map, source, t_inner, priority);
            }
            (Some(TypeData::NoInfer(s_inner)), _) => {
                self.constrain_types(ctx, var_map, s_inner, target, priority);
            }
            (_, Some(TypeData::NoInfer(t_inner))) => {
                self.constrain_types(ctx, var_map, source, t_inner, priority);
            }
            (
                Some(TypeData::IndexAccess(s_obj, s_idx)),
                Some(TypeData::IndexAccess(t_obj, t_idx)),
            ) => {
                self.constrain_types(ctx, var_map, s_obj, t_obj, priority);
                self.constrain_types(ctx, var_map, s_idx, t_idx, priority);
            }
            (Some(TypeData::KeyOf(s_inner)), Some(TypeData::KeyOf(t_inner))) => {
                self.constrain_types(ctx, var_map, t_inner, s_inner, priority);
            }
            (
                Some(TypeData::TemplateLiteral(s_spans)),
                Some(TypeData::TemplateLiteral(t_spans)),
            ) => {
                let s_spans = self.interner.template_list(s_spans);
                let t_spans = self.interner.template_list(t_spans);
                if s_spans.len() != t_spans.len() {
                    return;
                }

                for (s_span, t_span) in s_spans.iter().zip(t_spans.iter()) {
                    match (s_span, t_span) {
                        (TemplateSpan::Text(s_text), TemplateSpan::Text(t_text))
                            if s_text == t_text => {}
                        (TemplateSpan::Type(_), TemplateSpan::Type(_)) => {}
                        _ => return,
                    }
                }

                for (s_span, t_span) in s_spans.iter().zip(t_spans.iter()) {
                    if let (TemplateSpan::Type(s_type), TemplateSpan::Type(t_type)) =
                        (s_span, t_span)
                    {
                        self.constrain_types(ctx, var_map, *s_type, *t_type, priority);
                    }
                }
            }
            (Some(TypeData::IndexAccess(s_obj, s_idx)), _) => {
                let evaluated = self.interner.evaluate_index_access(s_obj, s_idx);
                if evaluated != source {
                    self.constrain_types(ctx, var_map, evaluated, target, priority);
                }
            }
            (_, Some(TypeData::IndexAccess(t_obj, t_idx))) => {
                let evaluated = self.interner.evaluate_index_access(t_obj, t_idx);
                if evaluated != target {
                    self.constrain_types(ctx, var_map, source, evaluated, priority);
                }
            }
            (Some(TypeData::Conditional(cond_id)), _) => {
                let cond = self.interner.conditional_type(cond_id);
                let evaluated = self.interner.evaluate_conditional(cond.as_ref());
                if evaluated != source {
                    self.constrain_types(ctx, var_map, evaluated, target, priority);
                }
            }
            (_, Some(TypeData::Conditional(cond_id))) => {
                let cond = self.interner.conditional_type(cond_id);
                let evaluated = self.interner.evaluate_conditional(cond.as_ref());
                if evaluated != target {
                    self.constrain_types(ctx, var_map, source, evaluated, priority);
                }
            }
            (Some(TypeData::Mapped(mapped_id)), _) => {
                let mapped = self.interner.mapped_type(mapped_id);
                let evaluated = self.interner.evaluate_mapped(mapped.as_ref());
                if evaluated != source {
                    self.constrain_types(ctx, var_map, evaluated, target, priority);
                }
            }
            (_, Some(TypeData::Mapped(mapped_id))) => {
                let mapped = self.interner.mapped_type(mapped_id);
                // When source is an object and target is a mapped type with
                // inference placeholders, infer directly from the object's
                // properties rather than trying to evaluate the mapped type
                // (which can't expand when its constraint is a placeholder).
                // e.g., source { a: "hello" } against { [P in K]: T }
                //   -> K = "a", T = string
                if let Some(TypeData::Object(source_shape)) = self.interner.lookup(source) {
                    let source_obj = self.interner.object_shape(source_shape);
                    if !source_obj.properties.is_empty() {
                        // Infer constraint (K) from property name literals
                        let name_literals: Vec<TypeId> = source_obj
                            .properties
                            .iter()
                            .map(|p| self.interner.literal_string_atom(p.name))
                            .collect();
                        let names_union = if name_literals.len() == 1 {
                            name_literals[0]
                        } else {
                            self.interner.union(name_literals)
                        };
                        self.constrain_types(
                            ctx,
                            var_map,
                            names_union,
                            mapped.constraint,
                            priority,
                        );

                        // Infer template (T) from property value types
                        for prop in &source_obj.properties {
                            self.constrain_types(
                                ctx,
                                var_map,
                                prop.type_id,
                                mapped.template,
                                priority,
                            );
                        }
                        return;
                    }
                }
                let evaluated = self.interner.evaluate_mapped(mapped.as_ref());
                if evaluated != target {
                    self.constrain_types(ctx, var_map, source, evaluated, priority);
                }
            }
            (Some(TypeData::Union(s_members)), Some(TypeData::Union(t_members))) => {
                // When both source and target are unions, filter source members that
                // match fixed (non-parameterized) target members before constraining
                // against parameterized members. This implements TypeScript's inference
                // filtering: for `T | undefined`, `undefined` in the source should match
                // the fixed `undefined` in the target, not be inferred as T.
                let s_members = self.interner.type_list(s_members);
                let t_members_list = self.interner.type_list(t_members);

                // Collect fixed target members (those without placeholders)
                let mut member_visited = FxHashSet::default();
                let fixed_targets: Vec<TypeId> = t_members_list
                    .iter()
                    .filter(|&&m| {
                        member_visited.clear();
                        !self.type_contains_placeholder(m, var_map, &mut member_visited)
                    })
                    .copied()
                    .collect();

                for &member in s_members.iter() {
                    // Skip source members that directly match a fixed target member
                    let matches_fixed = fixed_targets.contains(&member);
                    if !matches_fixed {
                        self.constrain_types(ctx, var_map, member, target, priority);
                    }
                }
            }
            (Some(TypeData::Union(s_members)), _) => {
                let s_members = self.interner.type_list(s_members);
                for &member in s_members.iter() {
                    self.constrain_types(ctx, var_map, member, target, priority);
                }
            }
            (_, Some(TypeData::Intersection(t_members))) => {
                let t_members = self.interner.type_list(t_members);
                for &member in t_members.iter() {
                    self.constrain_types(ctx, var_map, source, member, priority);
                }
            }
            (_, Some(TypeData::Union(t_members))) => {
                let t_members = self.interner.type_list(t_members);
                let mut non_nullable = None;
                let mut count = 0;
                for &member in t_members.iter() {
                    if !is_nullish(member) {
                        count += 1;
                        if count == 1 {
                            non_nullable = Some(member);
                        } else {
                            break;
                        }
                    }
                }
                if count == 1
                    && let Some(member) = non_nullable
                {
                    self.constrain_types(ctx, var_map, source, member, priority);
                    return;
                }

                let mut placeholder_member = None;
                let mut placeholder_count = 0;
                let mut member_visited = FxHashSet::default();
                for &member in t_members.iter() {
                    member_visited.clear();
                    if self.type_contains_placeholder(member, var_map, &mut member_visited) {
                        placeholder_count += 1;
                        if placeholder_count == 1 {
                            placeholder_member = Some(member);
                        } else {
                            break;
                        }
                    }
                }
                if placeholder_count == 1
                    && let Some(member) = placeholder_member
                {
                    // Single placeholder-containing member in a union like
                    // `T | undefined | null` — constrain source against it.
                    // Defaults don't prevent inference; they're used as fallback
                    // during resolution when no candidates are found.
                    self.constrain_types(ctx, var_map, source, member, priority);
                } else if placeholder_count > 1 {
                    // Multiple placeholder-containing members: constrain against each.
                    // For example, when source is `number` and target is
                    // `TResult | PromiseLike<TResult>`, we should try constraining
                    // against both members so TResult gets `number` as a candidate.
                    let t_members_copy = t_members.to_vec();
                    for member in t_members_copy {
                        member_visited.clear();
                        if self.type_contains_placeholder(member, var_map, &mut member_visited) {
                            self.constrain_types(ctx, var_map, source, member, priority);
                        }
                    }
                }
            }
            (Some(TypeData::Array(s_elem)), Some(TypeData::Array(t_elem))) => {
                self.constrain_types(ctx, var_map, s_elem, t_elem, priority);
            }
            (Some(TypeData::Tuple(s_elems)), Some(TypeData::Array(t_elem))) => {
                let s_elems = self.interner.tuple_list(s_elems);
                for s_elem in s_elems.iter() {
                    if s_elem.rest {
                        let rest_elem_type = self.rest_element_type(s_elem.type_id);
                        self.constrain_types(ctx, var_map, rest_elem_type, t_elem, priority);
                    } else {
                        self.constrain_types(ctx, var_map, s_elem.type_id, t_elem, priority);
                    }
                }
            }
            (Some(TypeData::Tuple(s_elems)), Some(TypeData::Tuple(t_elems))) => {
                let s_elems = self.interner.tuple_list(s_elems);
                let t_elems = self.interner.tuple_list(t_elems);
                self.constrain_tuple_types(ctx, var_map, &s_elems, &t_elems, priority);
            }
            // Array/Tuple → Object/ObjectWithIndex: constrain elements against index signatures
            (
                Some(TypeData::Array(s_elem)),
                Some(TypeData::Object(t_shape_id) | TypeData::ObjectWithIndex(t_shape_id)),
            ) => {
                self.constrain_elements_against_index_sigs(
                    ctx,
                    var_map,
                    &[s_elem],
                    t_shape_id,
                    priority,
                );
            }
            (
                Some(TypeData::Tuple(s_elems)),
                Some(TypeData::Object(t_shape_id) | TypeData::ObjectWithIndex(t_shape_id)),
            ) => {
                let s_elems = self.interner.tuple_list(s_elems);
                let elem_types: Vec<TypeId> = s_elems
                    .iter()
                    .map(|e| {
                        if e.rest {
                            self.rest_element_type(e.type_id)
                        } else {
                            e.type_id
                        }
                    })
                    .collect();
                self.constrain_elements_against_index_sigs(
                    ctx,
                    var_map,
                    &elem_types,
                    t_shape_id,
                    priority,
                );
            }
            (Some(TypeData::Function(s_fn_id)), Some(TypeData::Function(t_fn_id))) => {
                let s_fn = self.interner.function_shape(s_fn_id);
                let t_fn = self.interner.function_shape(t_fn_id);

                tracing::debug!(
                    has_s_pred = s_fn.type_predicate.is_some(),
                    has_t_pred = t_fn.type_predicate.is_some(),
                    "constrain_types_impl: Function"
                );

                if s_fn.type_params.is_empty() {
                    // Non-generic source function - direct comparison
                    // Unpack tuple rest parameters for proper matching
                    use crate::type_queries::unpack_tuple_rest_parameter;
                    let s_params_unpacked: Vec<ParamInfo> = s_fn
                        .params
                        .iter()
                        .flat_map(|p| unpack_tuple_rest_parameter(self.interner, p))
                        .collect();
                    let t_params_unpacked: Vec<ParamInfo> = t_fn
                        .params
                        .iter()
                        .flat_map(|p| unpack_tuple_rest_parameter(self.interner, p))
                        .collect();

                    // Contravariant parameters: target_param <: source_param
                    for (s_p, t_p) in s_params_unpacked.iter().zip(t_params_unpacked.iter()) {
                        self.constrain_types(ctx, var_map, t_p.type_id, s_p.type_id, priority);
                    }

                    // Special case: If target has a rest parameter with a type parameter,
                    // and source has more parameters, we should infer the tuple type.
                    // Example: source `(a: string, b: number) => R` vs target `(...args: A) => R`
                    // should infer `A = [string, number]`.
                    if let Some(t_last) = t_params_unpacked.last()
                        && t_last.rest
                        && var_map.contains_key(&t_last.type_id)
                    {
                        let target_fixed_count = t_params_unpacked.len().saturating_sub(1);
                        if s_params_unpacked.len() > target_fixed_count {
                            // Create tuple from source's extra parameters
                            let tuple_elements: Vec<TupleElement> = s_params_unpacked
                                [target_fixed_count..]
                                .iter()
                                .map(|p| TupleElement {
                                    type_id: p.type_id,
                                    name: p.name,
                                    optional: p.optional,
                                    rest: p.rest,
                                })
                                .collect();
                            let source_tuple = self.interner.tuple(tuple_elements);

                            // Infer: A = [string, number]
                            // When matching (x: string, y: number) => R against (...args: A) => R
                            // We want to infer A = [string, number] (the tuple of parameter types)
                            if let Some(&var) = var_map.get(&t_last.type_id) {
                                // Add as a high-priority candidate since this is structural information
                                ctx.add_candidate(
                                    var,
                                    source_tuple,
                                    crate::types::InferencePriority::NakedTypeVariable,
                                );
                            }
                        }
                    }

                    if let (Some(s_this), Some(t_this)) = (s_fn.this_type, t_fn.this_type) {
                        self.constrain_types(ctx, var_map, t_this, s_this, priority);
                    }
                    // Covariant return: source_return <: target_return
                    debug!(
                        source_return_id = s_fn.return_type.0,
                        source_return_key = ?self.interner.lookup(s_fn.return_type),
                        target_return_id = t_fn.return_type.0,
                        target_return_key = ?self.interner.lookup(t_fn.return_type),
                        var_map_keys = ?var_map.keys().collect::<Vec<_>>(),
                        priority = ?priority,
                        "Constraining return types"
                    );
                    self.constrain_types(
                        ctx,
                        var_map,
                        s_fn.return_type,
                        t_fn.return_type,
                        priority,
                    );

                    // Constrain type predicates if both functions have them
                    // Example: source `(x: any) => x is number` vs target `(value: T) => value is S`
                    // Should infer S = number from the predicates
                    if let (Some(s_pred), Some(t_pred)) =
                        (&s_fn.type_predicate, &t_fn.type_predicate)
                    {
                        // Only constrain if both predicates have type annotations
                        if let (Some(s_pred_type), Some(t_pred_type)) =
                            (s_pred.type_id, t_pred.type_id)
                        {
                            // Type predicates are covariant: source_pred_type <: target_pred_type
                            self.constrain_types(ctx, var_map, s_pred_type, t_pred_type, priority);
                        }
                    }
                } else {
                    // Generic source function - instantiate with fresh inference variables
                    // This allows inferring the source function's type parameters from the target
                    let mut source_subst = TypeSubstitution::new();
                    let mut source_var_map: FxHashMap<TypeId, crate::infer::InferenceVar> =
                        FxHashMap::default();
                    let mut src_placeholder_visited = FxHashSet::default();
                    let mut src_placeholder_buf = String::with_capacity(28);

                    // Create fresh inference variables for the source function's type parameters
                    for tp in &s_fn.type_params {
                        let var = ctx.fresh_var();
                        use std::fmt::Write;
                        src_placeholder_buf.clear();
                        write!(src_placeholder_buf, "__infer_src_{}", var.0).unwrap();
                        let placeholder_atom = self.interner.intern_string(&src_placeholder_buf);
                        ctx.register_type_param(placeholder_atom, var, tp.is_const);

                        let placeholder_key = TypeData::TypeParameter(TypeParamInfo {
                            is_const: tp.is_const,
                            name: placeholder_atom,
                            constraint: tp.constraint,
                            default: None,
                        });
                        let placeholder_id = self.interner.intern(placeholder_key);
                        source_subst.insert(tp.name, placeholder_id);
                        source_var_map.insert(placeholder_id, var);

                        // Add constraint as upper bound if it's concrete
                        if let Some(constraint) = tp.constraint {
                            let inst_constraint =
                                instantiate_type(self.interner, constraint, &source_subst);
                            src_placeholder_visited.clear();
                            // Create combined var_map for type_contains_placeholder check
                            let combined_for_check: FxHashMap<_, _> = var_map
                                .iter()
                                .chain(source_var_map.iter())
                                .map(|(k, v)| (*k, *v))
                                .collect();
                            if !self.type_contains_placeholder(
                                inst_constraint,
                                &combined_for_check,
                                &mut src_placeholder_visited,
                            ) {
                                ctx.add_upper_bound(var, inst_constraint);
                            }
                        }
                    }

                    // Instantiate source function's parameters and return type
                    let instantiated_params: Vec<ParamInfo> = s_fn
                        .params
                        .iter()
                        .map(|p| ParamInfo {
                            name: p.name,
                            type_id: instantiate_type(self.interner, p.type_id, &source_subst),
                            optional: p.optional,
                            rest: p.rest,
                        })
                        .collect();
                    let instantiated_return =
                        instantiate_type(self.interner, s_fn.return_type, &source_subst);
                    let instantiated_this = s_fn
                        .this_type
                        .map(|t| instantiate_type(self.interner, t, &source_subst));

                    // Instantiate type predicate if present
                    let instantiated_predicate =
                        s_fn.type_predicate.as_ref().map(|pred| TypePredicate {
                            asserts: pred.asserts,
                            target: pred.target.clone(),
                            type_id: pred
                                .type_id
                                .map(|t| instantiate_type(self.interner, t, &source_subst)),
                            parameter_index: pred.parameter_index,
                        });

                    // Create combined var_map for constraint collection
                    let combined_var_map: FxHashMap<_, _> = var_map
                        .iter()
                        .chain(source_var_map.iter())
                        .map(|(k, v)| (*k, *v))
                        .collect();

                    // Unpack tuple rest parameters for proper generic inference.
                    // In TypeScript, `(...args: [A, B]) => R` should match `(a: X, b: Y) => R`
                    // and infer the tuple type. We unpack tuple rest params into fixed params.
                    use crate::type_queries::unpack_tuple_rest_parameter;
                    let instantiated_params_unpacked: Vec<ParamInfo> = instantiated_params
                        .iter()
                        .flat_map(|p| unpack_tuple_rest_parameter(self.interner, p))
                        .collect();
                    let target_params_unpacked: Vec<ParamInfo> = t_fn
                        .params
                        .iter()
                        .flat_map(|p| unpack_tuple_rest_parameter(self.interner, p))
                        .collect();

                    // Contravariant parameters: target_param <: instantiated_source_param
                    for (s_p, t_p) in instantiated_params_unpacked
                        .iter()
                        .zip(target_params_unpacked.iter())
                    {
                        self.constrain_types(
                            ctx,
                            &combined_var_map,
                            t_p.type_id,
                            s_p.type_id,
                            priority,
                        );
                    }

                    // Special case: If target has a rest parameter with a type parameter,
                    // and source has more parameters, we should infer the tuple type.
                    // Example: source `<T>(a: T) => T[]` vs target `(...args: A) => B`
                    // should infer `A = [T]`.
                    if let Some(t_last) = target_params_unpacked.last()
                        && t_last.rest
                        && combined_var_map.contains_key(&t_last.type_id)
                    {
                        let target_fixed_count = target_params_unpacked.len().saturating_sub(1);
                        if instantiated_params_unpacked.len() > target_fixed_count {
                            // Create tuple from source's extra parameters
                            let tuple_elements: Vec<TupleElement> = instantiated_params_unpacked
                                [target_fixed_count..]
                                .iter()
                                .map(|p| TupleElement {
                                    type_id: p.type_id,
                                    name: p.name,
                                    optional: p.optional,
                                    rest: p.rest,
                                })
                                .collect();
                            let source_tuple = self.interner.tuple(tuple_elements);

                            // Infer: A = [T, U, ...]
                            // When matching generic function parameters, infer the tuple type
                            if let Some(&var) = combined_var_map.get(&t_last.type_id) {
                                ctx.add_candidate(
                                    var,
                                    source_tuple,
                                    crate::types::InferencePriority::NakedTypeVariable,
                                );
                            }
                        }
                    }

                    if let (Some(s_this), Some(t_this)) = (instantiated_this, t_fn.this_type) {
                        self.constrain_types(ctx, &combined_var_map, t_this, s_this, priority);
                    }
                    // Covariant return: instantiated_source_return <: target_return
                    self.constrain_types(
                        ctx,
                        &combined_var_map,
                        instantiated_return,
                        t_fn.return_type,
                        priority,
                    );

                    // Constrain type predicates if both functions have them
                    if let (Some(s_pred), Some(t_pred)) =
                        (&instantiated_predicate, &t_fn.type_predicate)
                        && let (Some(s_pred_type), Some(t_pred_type)) =
                            (s_pred.type_id, t_pred.type_id)
                    {
                        // Type predicates are covariant: source_pred_type <: target_pred_type
                        self.constrain_types(
                            ctx,
                            &combined_var_map,
                            s_pred_type,
                            t_pred_type,
                            priority,
                        );
                    }
                }
            }
            (Some(TypeData::Function(s_fn_id)), Some(TypeData::Callable(t_callable_id))) => {
                let s_fn = self.interner.function_shape(s_fn_id);
                let t_callable = self.interner.callable_shape(t_callable_id);
                for sig in &t_callable.call_signatures {
                    self.constrain_function_to_call_signature(ctx, var_map, &s_fn, sig, priority);
                }
                if s_fn.is_constructor && t_callable.construct_signatures.len() == 1 {
                    let sig = &t_callable.construct_signatures[0];
                    if sig.type_params.is_empty() {
                        self.constrain_function_to_call_signature(
                            ctx, var_map, &s_fn, sig, priority,
                        );
                    }
                }
            }
            (Some(TypeData::Callable(s_callable_id)), Some(TypeData::Callable(t_callable_id))) => {
                let s_callable = self.interner.callable_shape(s_callable_id);
                let t_callable = self.interner.callable_shape(t_callable_id);
                self.constrain_matching_signatures(
                    ctx,
                    var_map,
                    &s_callable.call_signatures,
                    &t_callable.call_signatures,
                    false,
                    priority,
                );
                self.constrain_matching_signatures(
                    ctx,
                    var_map,
                    &s_callable.construct_signatures,
                    &t_callable.construct_signatures,
                    true,
                    priority,
                );
            }
            (Some(TypeData::Callable(s_callable_id)), Some(TypeData::Function(t_fn_id))) => {
                let s_callable = self.interner.callable_shape(s_callable_id);
                let t_fn = self.interner.function_shape(t_fn_id);
                if s_callable.call_signatures.len() == 1 {
                    let sig = &s_callable.call_signatures[0];
                    if sig.type_params.is_empty() {
                        self.constrain_call_signature_to_function(
                            ctx, var_map, sig, &t_fn, priority,
                        );
                    }
                } else if let Some(index) = self.select_signature_for_target(
                    &s_callable.call_signatures,
                    target,
                    var_map,
                    false,
                ) {
                    let sig = &s_callable.call_signatures[index];
                    self.constrain_call_signature_to_function(ctx, var_map, sig, &t_fn, priority);
                }
            }
            (Some(TypeData::Object(s_shape_id)), Some(TypeData::Object(t_shape_id))) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_shape = self.interner.object_shape(t_shape_id);
                self.constrain_properties(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape.properties,
                    priority,
                );
            }
            (
                Some(TypeData::ObjectWithIndex(s_shape_id)),
                Some(TypeData::ObjectWithIndex(t_shape_id)),
            ) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_shape = self.interner.object_shape(t_shape_id);
                self.constrain_properties(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape.properties,
                    priority,
                );
                if let (Some(s_idx), Some(t_idx)) = (&s_shape.string_index, &t_shape.string_index) {
                    self.constrain_types(
                        ctx,
                        var_map,
                        s_idx.value_type,
                        t_idx.value_type,
                        priority,
                    );
                }
                if let (Some(s_idx), Some(t_idx)) = (&s_shape.number_index, &t_shape.number_index) {
                    self.constrain_types(
                        ctx,
                        var_map,
                        s_idx.value_type,
                        t_idx.value_type,
                        priority,
                    );
                }
                self.constrain_properties_against_index_signatures(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape,
                    priority,
                );
                self.constrain_index_signatures_to_properties(
                    ctx,
                    var_map,
                    &s_shape,
                    &t_shape.properties,
                    priority,
                );
            }
            (Some(TypeData::Object(s_shape_id)), Some(TypeData::ObjectWithIndex(t_shape_id))) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_shape = self.interner.object_shape(t_shape_id);
                self.constrain_properties(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape.properties,
                    priority,
                );
                self.constrain_properties_against_index_signatures(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape,
                    priority,
                );
            }
            (Some(TypeData::ObjectWithIndex(s_shape_id)), Some(TypeData::Object(t_shape_id))) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_shape = self.interner.object_shape(t_shape_id);
                self.constrain_properties(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape.properties,
                    priority,
                );
                self.constrain_index_signatures_to_properties(
                    ctx,
                    var_map,
                    &s_shape,
                    &t_shape.properties,
                    priority,
                );
            }
            // Object/ObjectWithIndex to Array/Tuple: constrain index signatures to sequence element type
            (Some(TypeData::Object(s_shape_id)), Some(TypeData::Array(t_elem)))
            | (Some(TypeData::ObjectWithIndex(s_shape_id)), Some(TypeData::Array(t_elem))) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                // Constrain source's string/number index signatures against array element type
                if let Some(string_idx) = &s_shape.string_index {
                    self.constrain_types(ctx, var_map, string_idx.value_type, t_elem, priority);
                }
                if let Some(number_idx) = &s_shape.number_index {
                    self.constrain_types(ctx, var_map, number_idx.value_type, t_elem, priority);
                }
            }
            (Some(TypeData::Object(s_shape_id)), Some(TypeData::Tuple(t_elems)))
            | (Some(TypeData::ObjectWithIndex(s_shape_id)), Some(TypeData::Tuple(t_elems))) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_elems = self.interner.tuple_list(t_elems);
                // Constrain source's string/number index signatures against each tuple element
                for t_elem in t_elems.iter() {
                    let elem_type = if t_elem.rest {
                        self.rest_element_type(t_elem.type_id)
                    } else {
                        t_elem.type_id
                    };
                    if let Some(string_idx) = &s_shape.string_index {
                        self.constrain_types(
                            ctx,
                            var_map,
                            string_idx.value_type,
                            elem_type,
                            priority,
                        );
                    }
                    if let Some(number_idx) = &s_shape.number_index {
                        self.constrain_types(
                            ctx,
                            var_map,
                            number_idx.value_type,
                            elem_type,
                            priority,
                        );
                    }
                }
            }
            (Some(TypeData::Application(s_app_id)), Some(TypeData::Application(t_app_id))) => {
                let s_app = self.interner.type_application(s_app_id);
                let t_app = self.interner.type_application(t_app_id);
                if s_app.base == t_app.base && s_app.args.len() == t_app.args.len() {
                    for (s_arg, t_arg) in s_app.args.iter().zip(t_app.args.iter()) {
                        self.constrain_types(ctx, var_map, *s_arg, *t_arg, priority);
                    }
                }
            }
            (Some(TypeData::Enum(_, s_mem)), Some(TypeData::Enum(_, t_mem))) => {
                self.constrain_types(ctx, var_map, s_mem, t_mem, priority);
            }
            // Application on source side (not matched by Application-Application above):
            // evaluate and recurse. Use the checker's resolver to expand Application
            // types like `Func<T>` that reference DefId-based interfaces.
            (Some(TypeData::Application(_)), _) => {
                let evaluated = self.checker.evaluate_type(source);
                if evaluated != source {
                    self.constrain_types(ctx, var_map, evaluated, target, priority);
                }
            }
            // Application on target side (not matched by Application-Application above):
            // evaluate and recurse. This handles inference from object literal against
            // generic types like Options<T, U>.
            (_, Some(TypeData::Application(_))) => {
                let evaluated = self.checker.evaluate_type(target);
                if evaluated != target {
                    self.constrain_types(ctx, var_map, source, evaluated, priority);
                }
            }
            _ => {}
        }
    }

    fn constrain_properties(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::infer::InferenceVar>,
        source_props: &[PropertyInfo],
        target_props: &[PropertyInfo],
        priority: crate::types::InferencePriority,
    ) {
        let mut source_idx = 0;
        let mut target_idx = 0;

        while source_idx < source_props.len() && target_idx < target_props.len() {
            let source = &source_props[source_idx];
            let target = &target_props[target_idx];

            match source.name.cmp(&target.name) {
                std::cmp::Ordering::Equal => {
                    self.constrain_types(ctx, var_map, source.type_id, target.type_id, priority);
                    // Check write type compatibility for mutable targets
                    // A readonly source cannot satisfy a mutable target
                    if !target.readonly {
                        // If source is readonly but target is mutable, this is a mismatch
                        // We constrain with ERROR to signal the failure
                        if source.readonly {
                            self.constrain_types(
                                ctx,
                                var_map,
                                TypeId::ERROR,
                                target.write_type,
                                priority,
                            );
                        }
                        self.constrain_types(
                            ctx,
                            var_map,
                            target.write_type,
                            source.write_type,
                            priority,
                        );
                    }
                    source_idx += 1;
                    target_idx += 1;
                }
                std::cmp::Ordering::Less => {
                    source_idx += 1;
                }
                std::cmp::Ordering::Greater => {
                    // Target property is missing from source.
                    // For optional properties, only constrain to `undefined` when the
                    // target type is NOT a direct inference variable.  Constraining an
                    // inference placeholder to `undefined` from a missing optional
                    // property would incorrectly fix `T = undefined` during partial
                    // Round 1 inference (where context-sensitive properties are
                    // intentionally omitted from the source).
                    if target.optional && !var_map.contains_key(&target.type_id) {
                        self.constrain_types(
                            ctx,
                            var_map,
                            TypeId::UNDEFINED,
                            target.type_id,
                            priority,
                        );
                    }
                    target_idx += 1;
                }
            }
        }

        // Handle remaining target properties that are missing from source
        while target_idx < target_props.len() {
            let target = &target_props[target_idx];
            if target.optional && !var_map.contains_key(&target.type_id) {
                self.constrain_types(ctx, var_map, TypeId::UNDEFINED, target.type_id, priority);
            }
            target_idx += 1;
        }
    }

    fn constrain_function_to_call_signature(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::infer::InferenceVar>,
        source: &FunctionShape,
        target: &CallSignature,
        priority: crate::types::InferencePriority,
    ) {
        for (s_p, t_p) in source.params.iter().zip(target.params.iter()) {
            self.constrain_types(ctx, var_map, t_p.type_id, s_p.type_id, priority);
        }
        if let (Some(s_this), Some(t_this)) = (source.this_type, target.this_type) {
            self.constrain_types(ctx, var_map, t_this, s_this, priority);
        }
        self.constrain_types(
            ctx,
            var_map,
            source.return_type,
            target.return_type,
            priority,
        );
        // Constrain type predicates if both have them
        trace!(
            source_has_predicate = source.type_predicate.is_some(),
            target_has_predicate = target.type_predicate.is_some(),
            "constrain_function_to_call_signature: checking type predicates"
        );
        if let (Some(s_pred), Some(t_pred)) = (&source.type_predicate, &target.type_predicate) {
            trace!(
                source_pred_asserts = s_pred.asserts,
                source_pred_type = ?s_pred.type_id,
                target_pred_asserts = t_pred.asserts,
                target_pred_type = ?t_pred.type_id,
                "constrain_function_to_call_signature: both have predicates"
            );
            if let (Some(s_pred_type), Some(t_pred_type)) = (s_pred.type_id, t_pred.type_id) {
                trace!(
                    s_pred_type = ?s_pred_type,
                    t_pred_type = ?t_pred_type,
                    "constrain_function_to_call_signature: adding constraint"
                );
                self.constrain_types(ctx, var_map, s_pred_type, t_pred_type, priority);
            }
        }
    }

    fn constrain_call_signature_to_function(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::infer::InferenceVar>,
        source: &CallSignature,
        target: &FunctionShape,
        priority: crate::types::InferencePriority,
    ) {
        for (s_p, t_p) in source.params.iter().zip(target.params.iter()) {
            self.constrain_types(ctx, var_map, t_p.type_id, s_p.type_id, priority);
        }
        if let (Some(s_this), Some(t_this)) = (source.this_type, target.this_type) {
            self.constrain_types(ctx, var_map, t_this, s_this, priority);
        }
        self.constrain_types(
            ctx,
            var_map,
            source.return_type,
            target.return_type,
            priority,
        );
        // Constrain type predicates if both have them
        if let (Some(s_pred), Some(t_pred)) = (&source.type_predicate, &target.type_predicate)
            && let (Some(s_pred_type), Some(t_pred_type)) = (s_pred.type_id, t_pred.type_id)
        {
            self.constrain_types(ctx, var_map, s_pred_type, t_pred_type, priority);
        }
    }

    fn constrain_call_signature_to_call_signature(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::infer::InferenceVar>,
        source: &CallSignature,
        target: &CallSignature,
        priority: crate::types::InferencePriority,
    ) {
        for (s_p, t_p) in source.params.iter().zip(target.params.iter()) {
            self.constrain_types(ctx, var_map, t_p.type_id, s_p.type_id, priority);
        }
        if let (Some(s_this), Some(t_this)) = (source.this_type, target.this_type) {
            self.constrain_types(ctx, var_map, t_this, s_this, priority);
        }
        self.constrain_types(
            ctx,
            var_map,
            source.return_type,
            target.return_type,
            priority,
        );
        // Constrain type predicates if both have them
        if let (Some(s_pred), Some(t_pred)) = (&source.type_predicate, &target.type_predicate)
            && let (Some(s_pred_type), Some(t_pred_type)) = (s_pred.type_id, t_pred.type_id)
        {
            self.constrain_types(ctx, var_map, s_pred_type, t_pred_type, priority);
        }
    }

    fn function_type_from_signature(&self, sig: &CallSignature, is_constructor: bool) -> TypeId {
        self.interner.function(FunctionShape {
            type_params: Vec::new(),
            params: sig.params.clone(),
            this_type: sig.this_type,
            return_type: sig.return_type,
            type_predicate: sig.type_predicate.clone(),
            is_constructor,
            is_method: false,
        })
    }

    fn erase_placeholders_for_inference(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, crate::infer::InferenceVar>,
    ) -> TypeId {
        if var_map.is_empty() {
            return ty;
        }
        let mut visited = FxHashSet::default();
        if !self.type_contains_placeholder(ty, var_map, &mut visited) {
            return ty;
        }

        let mut substitution = TypeSubstitution::new();
        for &placeholder in var_map.keys() {
            if let Some(TypeData::TypeParameter(info)) = self.interner.lookup(placeholder) {
                // Use UNKNOWN instead of ANY for unresolved placeholders
                // to expose hidden type errors instead of silently accepting all values
                substitution.insert(info.name, TypeId::UNKNOWN);
            }
        }

        instantiate_type(self.interner, ty, &substitution)
    }

    fn select_signature_for_target(
        &mut self,
        signatures: &[CallSignature],
        target_fn: TypeId,
        var_map: &FxHashMap<TypeId, crate::infer::InferenceVar>,
        is_constructor: bool,
    ) -> Option<usize> {
        let target_erased = self.erase_placeholders_for_inference(target_fn, var_map);
        for (index, sig) in signatures.iter().enumerate() {
            if !sig.type_params.is_empty() {
                continue;
            }
            let source_fn = self.function_type_from_signature(sig, is_constructor);
            if self.checker.is_assignable_to(source_fn, target_erased) {
                return Some(index);
            }
        }
        None
    }

    fn constrain_matching_signatures(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::infer::InferenceVar>,
        source_signatures: &[CallSignature],
        target_signatures: &[CallSignature],
        is_constructor: bool,
        priority: crate::types::InferencePriority,
    ) {
        if source_signatures.is_empty() || target_signatures.is_empty() {
            return;
        }

        if source_signatures.len() == 1 && target_signatures.len() == 1 {
            let source_sig = &source_signatures[0];
            let target_sig = &target_signatures[0];
            if source_sig.type_params.is_empty() && target_sig.type_params.is_empty() {
                self.constrain_call_signature_to_call_signature(
                    ctx, var_map, source_sig, target_sig, priority,
                );
            }
            return;
        }

        if target_signatures.len() == 1 {
            let target_sig = &target_signatures[0];
            if target_sig.type_params.is_empty() {
                let source_sig = if source_signatures.len() == 1 {
                    let sig = &source_signatures[0];
                    sig.type_params.is_empty().then_some(sig)
                } else {
                    let target_fn = self.function_type_from_signature(target_sig, is_constructor);
                    self.select_signature_for_target(
                        source_signatures,
                        target_fn,
                        var_map,
                        is_constructor,
                    )
                    .and_then(|index| source_signatures.get(index))
                };
                if let Some(source_sig) = source_sig {
                    self.constrain_call_signature_to_call_signature(
                        ctx, var_map, source_sig, target_sig, priority,
                    );
                }
            }
            return;
        }

        if source_signatures.len() == 1 {
            let source_sig = &source_signatures[0];
            if source_sig.type_params.is_empty() {
                for target_sig in target_signatures {
                    if target_sig.type_params.is_empty() {
                        self.constrain_call_signature_to_call_signature(
                            ctx, var_map, source_sig, target_sig, priority,
                        );
                    }
                }
            }
            return;
        }

        for target_sig in target_signatures {
            if target_sig.type_params.is_empty() {
                let target_fn = self.function_type_from_signature(target_sig, is_constructor);
                if let Some(index) = self.select_signature_for_target(
                    source_signatures,
                    target_fn,
                    var_map,
                    is_constructor,
                ) {
                    let source_sig = &source_signatures[index];
                    self.constrain_call_signature_to_call_signature(
                        ctx, var_map, source_sig, target_sig, priority,
                    );
                }
            }
        }
    }

    fn constrain_properties_against_index_signatures(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::infer::InferenceVar>,
        source_props: &[PropertyInfo],
        target: &ObjectShape,
        priority: crate::types::InferencePriority,
    ) {
        let string_index = target.string_index.as_ref();
        let number_index = target.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() {
            return;
        }

        for prop in source_props {
            let prop_type = self.optional_property_type(prop);

            if let Some(number_idx) = number_index
                && utils::is_numeric_property_name(self.interner, prop.name)
            {
                self.constrain_types(ctx, var_map, prop_type, number_idx.value_type, priority);
            }

            if let Some(string_idx) = string_index {
                self.constrain_types(ctx, var_map, prop_type, string_idx.value_type, priority);
            }
        }
    }

    fn constrain_index_signatures_to_properties(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::infer::InferenceVar>,
        source: &ObjectShape,
        target_props: &[PropertyInfo],
        priority: crate::types::InferencePriority,
    ) {
        let string_index = source.string_index.as_ref();
        let number_index = source.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() {
            return;
        }

        for prop in target_props {
            // CRITICAL: Only infer from index signatures if the property is optional.
            // Required properties missing from the source cause a structural mismatch,
            // so TypeScript does not infer from them.
            if !prop.optional {
                continue;
            }

            let prop_type = self.optional_property_type(prop);

            if let Some(number_idx) = number_index
                && utils::is_numeric_property_name(self.interner, prop.name)
            {
                self.constrain_types(ctx, var_map, number_idx.value_type, prop_type, priority);
            }

            if let Some(string_idx) = string_index {
                self.constrain_types(ctx, var_map, string_idx.value_type, prop_type, priority);
            }
        }
    }

    fn optional_property_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional {
            self.interner.union2(prop.type_id, TypeId::UNDEFINED)
        } else {
            prop.type_id
        }
    }

    /// Constrain each element type against the string and number index signatures
    /// of a target object shape. Used for Array→Object and Tuple→Object inference.
    fn constrain_elements_against_index_sigs(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::infer::InferenceVar>,
        element_types: &[TypeId],
        target_shape_id: ObjectShapeId,
        priority: crate::types::InferencePriority,
    ) {
        let t_shape = self.interner.object_shape(target_shape_id);
        let string_idx_type = t_shape.string_index.as_ref().map(|idx| idx.value_type);
        let number_idx_type = t_shape.number_index.as_ref().map(|idx| idx.value_type);
        for &elem in element_types {
            if let Some(string_target) = string_idx_type {
                self.constrain_types(ctx, var_map, elem, string_target, priority);
            }
            if let Some(number_target) = number_idx_type {
                self.constrain_types(ctx, var_map, elem, number_target, priority);
            }
        }
    }

    fn constrain_tuple_types(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::infer::InferenceVar>,
        source: &[TupleElement],
        target: &[TupleElement],
        priority: crate::types::InferencePriority,
    ) {
        for (i, t_elem) in target.iter().enumerate() {
            if t_elem.rest {
                if var_map.contains_key(&t_elem.type_id) {
                    let tail = &target[i + 1..];
                    let mut trailing_count = 0usize;
                    let mut source_index = source.len();
                    for tail_elem in tail.iter().rev() {
                        if source_index <= i {
                            break;
                        }
                        let s_elem = &source[source_index - 1];
                        if s_elem.rest {
                            break;
                        }
                        let assignable = self
                            .checker
                            .is_assignable_to(s_elem.type_id, tail_elem.type_id);
                        if tail_elem.optional && !assignable {
                            break;
                        }
                        trailing_count += 1;
                        source_index -= 1;
                    }

                    let end_index = source.len().saturating_sub(trailing_count).max(i);
                    let mut tail = Vec::new();
                    for s_elem in source.iter().take(end_index).skip(i) {
                        tail.push(TupleElement {
                            type_id: s_elem.type_id,
                            name: s_elem.name,
                            optional: s_elem.optional,
                            rest: s_elem.rest,
                        });
                        if s_elem.rest {
                            break;
                        }
                    }
                    if tail.len() == 1 && tail[0].rest {
                        self.constrain_types(
                            ctx,
                            var_map,
                            tail[0].type_id,
                            t_elem.type_id,
                            priority,
                        );
                    } else {
                        let tail_tuple = self.interner.tuple(tail);
                        self.constrain_types(ctx, var_map, tail_tuple, t_elem.type_id, priority);
                    }
                    return;
                }
                let rest_elem_type = self.rest_element_type(t_elem.type_id);
                for s_elem in source.iter().skip(i) {
                    if s_elem.rest {
                        self.constrain_types(
                            ctx,
                            var_map,
                            s_elem.type_id,
                            t_elem.type_id,
                            priority,
                        );
                    } else {
                        self.constrain_types(
                            ctx,
                            var_map,
                            s_elem.type_id,
                            rest_elem_type,
                            priority,
                        );
                    }
                }
                return;
            }

            let Some(s_elem) = source.get(i) else {
                if t_elem.optional {
                    continue;
                }
                return;
            };

            if s_elem.rest {
                return;
            }

            self.constrain_types(ctx, var_map, s_elem.type_id, t_elem.type_id, priority);
        }
    }
}
