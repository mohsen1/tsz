//! Core type constraint walker for generic type inference.
//!
//! Contains the main structural walker (`constrain_types` / `constrain_types_impl`)
//! that collects type constraints when inferring generic type parameters from
//! argument types.

use crate::inference::infer::InferenceContext;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::core::MAX_CONSTRAINT_STEPS;
use crate::operations::{AssignabilityChecker, CallEvaluator, MAX_CONSTRAINT_RECURSION_DEPTH};
use crate::relations::variance::compute_type_param_variances_with_resolver;
use crate::types::{
    FunctionShape, ParamInfo, PropertyInfo, TemplateSpan, TupleElement, TypeData, TypeId,
    TypeParamInfo, TypePredicate, Variance,
};
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::{debug, trace};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Structural walker to collect constraints: source <: target
    pub(crate) fn constrain_types(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source: TypeId,
        target: TypeId,
        priority: crate::types::InferencePriority,
    ) {
        {
            let steps = self.constraint_step_count.get();
            if steps >= MAX_CONSTRAINT_STEPS {
                return;
            }
            self.constraint_step_count.set(steps + 1);
        }

        if !self.constraint_pairs.borrow_mut().insert((source, target)) {
            return;
        }

        // Check and increment recursion depth to prevent infinite loops
        {
            let depth = self.constraint_recursion_depth.get();
            if depth >= MAX_CONSTRAINT_RECURSION_DEPTH {
                // Safety limit reached - return to prevent infinite loop
                return;
            }
            self.constraint_recursion_depth.set(depth + 1);
        }

        // Perform the actual constraint collection
        self.constrain_types_impl(ctx, var_map, source, target, priority);

        // Decrement depth on return
        self.constraint_recursion_depth
            .set(self.constraint_recursion_depth.get() - 1);
    }

    /// Propagate a top-like type (any/unknown) to all inference placeholders
    /// nested inside a target type. This matches tsc's propagationType mechanism
    /// where `inferFromTypes(target, target)` is called with propagationType set
    /// to the source type, so all type parameter positions receive the source.
    pub(super) fn propagate_type_to_placeholders(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        propagation_type: TypeId,
        target: TypeId,
        priority: crate::types::InferencePriority,
    ) {
        // If target is directly a placeholder, add the propagation type as candidate
        if let Some(&var) = var_map.get(&target) {
            ctx.add_candidate(var, propagation_type, priority);
            return;
        }

        // Recurse into the target structure to find nested placeholders
        let target_key = self.interner.lookup(target);
        match target_key {
            Some(TypeData::Array(elem)) => {
                self.propagate_type_to_placeholders(ctx, var_map, propagation_type, elem, priority);
            }
            Some(TypeData::Tuple(elems_id)) => {
                let elems = self.interner.tuple_list(elems_id);
                for elem in elems.iter() {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        elem.type_id,
                        priority,
                    );
                }
            }
            Some(TypeData::Union(members_id) | TypeData::Intersection(members_id)) => {
                let members = self.interner.type_list(members_id);
                for &member in members.iter() {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        member,
                        priority,
                    );
                }
            }
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        prop.type_id,
                        priority,
                    );
                }
                // Walk index signatures — e.g., { [x: string]: T } needs T to get `any`
                if let Some(ref idx) = shape.string_index {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        idx.value_type,
                        priority,
                    );
                }
                if let Some(ref idx) = shape.number_index {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        idx.value_type,
                        priority,
                    );
                }
            }
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                for param in &shape.params {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        param.type_id,
                        priority,
                    );
                }
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    shape.return_type,
                    priority,
                );
                // Walk type predicate — e.g., `(a: any) => a is T` has T in predicate
                if let Some(ref pred) = shape.type_predicate
                    && let Some(pred_type) = pred.type_id
                {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        pred_type,
                        priority,
                    );
                }
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.interner.callable_shape(shape_id);
                // Walk both call and construct signatures
                for sig in shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                {
                    for param in &sig.params {
                        self.propagate_type_to_placeholders(
                            ctx,
                            var_map,
                            propagation_type,
                            param.type_id,
                            priority,
                        );
                    }
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        sig.return_type,
                        priority,
                    );
                }
                for prop in &shape.properties {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        prop.type_id,
                        priority,
                    );
                }
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                for arg in app.args.iter().copied() {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        arg,
                        priority,
                    );
                }
            }
            Some(TypeData::Mapped(mapped_id)) => {
                let mapped = self.interner.mapped_type(mapped_id);
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    mapped.constraint,
                    priority,
                );
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    mapped.template,
                    priority,
                );
            }
            Some(TypeData::Conditional(cond_id)) => {
                let cond = self.interner.get_conditional(cond_id);
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    cond.check_type,
                    priority,
                );
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    cond.extends_type,
                    priority,
                );
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    cond.true_type,
                    priority,
                );
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    cond.false_type,
                    priority,
                );
            }
            Some(TypeData::ReadonlyType(inner)) | Some(TypeData::KeyOf(inner)) => {
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    inner,
                    priority,
                );
            }
            Some(TypeData::IndexAccess(obj, idx)) => {
                self.propagate_type_to_placeholders(ctx, var_map, propagation_type, obj, priority);
                self.propagate_type_to_placeholders(ctx, var_map, propagation_type, idx, priority);
            }
            _ => {
                // No structural match — stop propagation
            }
        }
    }

    /// Constrain type arguments of two Applications with the same base type,
    /// respecting the variance of each type parameter position.
    ///
    /// For contravariant positions (e.g., T in `type Func<T> = (x: T) => void`),
    /// the source and target are swapped so that inference produces contra-candidates
    /// (resolved via intersection/most-specific) rather than covariant candidates.
    /// This matches tsc's `inferFromTypeArguments` which checks variance flags.
    pub(super) fn constrain_application_type_args(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        base: TypeId,
        source_args: &[TypeId],
        target_args: &[TypeId],
        priority: crate::types::InferencePriority,
    ) {
        // Try to compute variances for the base type's type parameters.
        let variances = self.compute_application_variances(base);
        for (i, (s_arg, t_arg)) in source_args.iter().zip(target_args.iter()).enumerate() {
            let variance = variances
                .as_ref()
                .and_then(|v| v.get(i).copied())
                .unwrap_or(Variance::COVARIANT);
            if variance.is_contravariant() {
                // Contravariant: swap source and target so the inference engine
                // sees the narrower type as source (lower bound on the target
                // placeholder). This causes the placeholder to pick the
                // intersection of candidates instead of the union.
                let was_contra = ctx.in_contra_mode;
                ctx.in_contra_mode = !was_contra;
                self.constrain_types(ctx, var_map, *s_arg, *t_arg, priority);
                ctx.in_contra_mode = was_contra;
            } else {
                self.constrain_types(ctx, var_map, *s_arg, *t_arg, priority);
            }
        }
    }

    /// Compute the variances of each type parameter for a type application's base type.
    pub(super) fn compute_application_variances(
        &self,
        base: TypeId,
    ) -> Option<std::sync::Arc<[Variance]>> {
        let def_id = match self.interner.lookup(base)? {
            TypeData::Lazy(def_id) => def_id,
            _ => return None,
        };
        // Use the checker's resolver which has type alias definitions,
        // falling back to the interner's resolver (which lacks them).
        let resolver = self
            .checker
            .type_resolver()
            .unwrap_or_else(|| self.interner.as_type_resolver());
        compute_type_param_variances_with_resolver(
            self.interner.as_type_database(),
            resolver,
            def_id,
        )
    }

    /// Inner implementation of `constrain_types`
    pub(super) fn constrain_types_impl(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
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

        // If source is an inference placeholder, add upper bound: var <: target.
        // In contra_mode (function parameter inference), add as contra-candidate instead
        // of upper bound. This matches tsc where inference from contravariant positions
        // produces contra-candidates resolved via intersection/most-specific, not hard
        // upper bounds that must each be individually satisfied.
        if let Some(&var) = var_map.get(&source) {
            if ctx.in_contra_mode {
                ctx.add_contra_candidate(var, target, priority);
            } else {
                ctx.add_upper_bound(var, target);
            }
            return;
        }

        // When source is `any`, propagate it to all inference placeholders
        // nested inside the target. This matches tsc's propagationType
        // mechanism: `inferFromTypes(target, target)` with propagationType =
        // source, which ensures that e.g. `f<T>(x: T[]): T` called with
        // `any` infers T = any (not unknown).
        //
        // Note: we only propagate `any`, not `unknown`. While tsc propagates
        // both, `unknown` can appear as an intermediate source type in tsz
        // for non-user-facing reasons, and propagating it causes regressions.
        if source == TypeId::ANY {
            self.propagate_type_to_placeholders(ctx, var_map, source, target, priority);
            return;
        }

        // Stop structural recursion when source or target is a top type.
        if source == TypeId::UNKNOWN || target == TypeId::ANY {
            return;
        }

        // Recurse structurally
        let source_key = self.interner.lookup(source);
        let target_key = self.interner.lookup(target);

        let is_nullish = |ty: TypeId| ty.is_nullable();

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
            (_, Some(TypeData::NoInfer(_t_inner))) => {
                // NoInfer<T> blocks inference: do NOT recurse into the wrapped type.
                // This prevents the source from contributing candidates for type
                // parameters inside the NoInfer wrapper, matching tsc's behavior.
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
            // Reverse keyof inference: source <: keyof T.
            // When a string/number literal is passed as `keyof T`, infer that T has
            // a property with that key. This matches tsc's inferToKeyof behavior
            // where `bar<T>(x: keyof T, y: keyof T)` called with `('a', 'b')`
            // infers T = { a: any } & { b: any }.
            (_, Some(TypeData::KeyOf(keyof_inner))) => {
                if let Some(&var) = var_map.get(&keyof_inner) {
                    // Reverse keyof inference: source <: keyof T → T has property source.
                    // Construct an object `{ [source]: any }` and add it as a contra
                    // candidate — contra candidates combine via intersection, matching
                    // tsc's behavior where `bar<T>(x: keyof T, y: keyof T)` called with
                    // `('a', 'b')` infers T = { a: any } & { b: any }.
                    let key_atom = crate::type_queries::extended::get_literal_property_name(
                        self.interner,
                        source,
                    );
                    if let Some(key_atom) = key_atom {
                        let prop = PropertyInfo::new(key_atom, TypeId::ANY);
                        let obj = self.interner.object(vec![prop]);
                        ctx.add_contra_candidate(var, obj, priority);
                    } else if let Some(TypeData::Union(source_members)) =
                        self.interner.lookup(source)
                    {
                        let members = self.interner.type_list(source_members);
                        for &member in members.iter() {
                            self.constrain_types(ctx, var_map, member, target, priority);
                        }
                    }
                } else {
                    // keyof_inner is not a bare placeholder — it might contain
                    // placeholders deeper (e.g., keyof Application<T>). Try evaluating.
                    let mut visited = FxHashSet::default();
                    if self.type_contains_placeholder(keyof_inner, var_map, &mut visited) {
                        // Contains placeholders — skip for now, will be resolved later
                    } else {
                        // No placeholders — evaluate the keyof and retry
                        let evaluated = crate::evaluation::evaluate::evaluate_type(
                            self.interner.as_type_database(),
                            target,
                        );
                        if evaluated != target {
                            self.constrain_types(ctx, var_map, source, evaluated, priority);
                        }
                    }
                }
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
                let cond = self.interner.get_conditional(cond_id);
                let evaluated = self.interner.evaluate_conditional(&cond);
                if evaluated != source {
                    self.constrain_types(ctx, var_map, evaluated, target, priority);
                }
            }
            (_, Some(TypeData::Conditional(cond_id))) => {
                let cond = self.interner.get_conditional(cond_id);
                let evaluated = self.interner.evaluate_conditional(&cond);
                if evaluated != target {
                    self.constrain_types(ctx, var_map, source, evaluated, priority);
                } else {
                    // When the conditional can't be evaluated and its check type is
                    // an inference placeholder, skip inference entirely. This matches
                    // tsc's inferToConditionalType: tsc only infers from a conditional
                    // when its own `infer` type parameters include the check type
                    // (i.e., `infer X` in the extends clause). When the check type is
                    // an outer inference variable (from a generic function call), tsc
                    // does NOT infer through the conditional, preventing false
                    // candidates that would pollute the inferred type.
                    if var_map.contains_key(&cond.check_type) {
                        return;
                    }
                    let mut visited = FxHashSet::default();
                    if self.type_contains_placeholder(target, var_map, &mut visited) {
                        self.constrain_types(ctx, var_map, source, cond.true_type, priority);
                        self.constrain_types(ctx, var_map, source, cond.false_type, priority);
                    }
                }
            }
            (Some(TypeData::Mapped(mapped_id)), _) => {
                let mapped = self.interner.get_mapped(mapped_id);
                let evaluated = self.checker.evaluate_type(source);
                let evaluated = if evaluated == source {
                    self.interner.evaluate_mapped(&mapped)
                } else {
                    evaluated
                };
                if evaluated != source {
                    self.constrain_types(ctx, var_map, evaluated, target, priority);
                }
            }
            (_, Some(TypeData::Mapped(mapped_id))) => {
                let mapped = self.interner.get_mapped(mapped_id);
                let source_shape_id = match self.interner.lookup(source) {
                    Some(TypeData::Object(id) | TypeData::ObjectWithIndex(id)) => Some(id),
                    _ => None,
                };
                if let Some(source_shape) = source_shape_id {
                    let source_obj = self.interner.object_shape(source_shape);
                    let has_properties = !source_obj.properties.is_empty();
                    let has_index_sigs =
                        source_obj.string_index.is_some() || source_obj.number_index.is_some();
                    if has_properties || has_index_sigs {
                        // Check for reverse mapped type inference pattern:
                        // constraint contains `keyof T` where T is an inference placeholder.
                        // This handles homomorphic mapped types like Boxified<T> =
                        // { [P in keyof T]: Box<T[P]> }. For each source property,
                        // we reverse through the template to reconstruct T.
                        //
                        // Following tsc's inferToMappedType, we decompose Union and
                        // Intersection constraints to find a `keyof T` member.
                        // E.g., `{ [K in keyof T & keyof Constraint]: T[K] }` has
                        // constraint `keyof T & keyof Constraint` — an Intersection
                        // containing `keyof T`.
                        if let Some(keyof_target) =
                            self.find_keyof_inference_target(mapped.constraint, var_map)
                            && self.constrain_reverse_mapped_type(
                                ctx,
                                var_map,
                                &source_obj,
                                &mapped,
                                keyof_target,
                            )
                        {
                            // Reverse mapping succeeded for the homomorphic type param
                            // (e.g., B in `keyof B`). But the template may contain OTHER
                            // inference type params (e.g., A in `{ fn: (a: A) => void; val: B[K] }`).
                            // Constrain those by matching source properties against the
                            // instantiated template for each key.
                            if has_properties {
                                let iter_param_name = mapped.type_param.name;
                                for prop in &source_obj.properties {
                                    let key_literal = self.interner.literal_string_atom(prop.name);
                                    let mut subst = TypeSubstitution::new();
                                    subst.insert(iter_param_name, key_literal);
                                    let instantiated_template =
                                        instantiate_type(self.interner, mapped.template, &subst);
                                    self.constrain_types(
                                        ctx,
                                        var_map,
                                        prop.type_id,
                                        instantiated_template,
                                        priority,
                                    );
                                }
                            }
                            return;
                        }
                        // Reverse inference failed (template too complex),
                        // fall through to simple/evaluate paths

                        if has_properties {
                            // Simple mapped type inference for { [P in K]: T }
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

                            // Infer template (T) from property value types.
                            // Use MappedType priority so that candidates from different
                            // properties are combined via union (matching tsc's
                            // PriorityImpliesCombination for MappedTypeConstraint).
                            let template_priority = crate::types::InferencePriority::MappedType;
                            for prop in &source_obj.properties {
                                self.constrain_types(
                                    ctx,
                                    var_map,
                                    prop.type_id,
                                    mapped.template,
                                    template_priority,
                                );
                            }
                            return;
                        }
                    }
                }
                // Handle Tuple sources against mapped types for reverse-mapped inference.
                // Tuples like [string, number] have numeric keys "0", "1", etc.
                // When target is { [K in keyof T]: ... }, we reverse through each element
                // and infer T as a tuple type.
                if let Some(TypeData::Tuple(s_elems)) = self.interner.lookup(source) {
                    let s_elems = self.interner.tuple_list(s_elems);
                    if !s_elems.is_empty()
                        && let Some(keyof_target) =
                            self.find_keyof_inference_target(mapped.constraint, var_map)
                        && self.constrain_reverse_mapped_tuple(
                            ctx,
                            var_map,
                            &s_elems,
                            &mapped,
                            keyof_target,
                        )
                    {
                        return;
                    }
                }
                let evaluated = self.interner.evaluate_mapped(&mapped);
                if evaluated != target {
                    self.constrain_types(ctx, var_map, source, evaluated, priority);
                }
            }
            (Some(TypeData::Union(s_members)), Some(TypeData::Union(t_members))) => {
                if self
                    .constrain_iterator_result_unions(ctx, var_map, s_members, t_members, priority)
                {
                    return;
                }

                // When both source and target are unions, filter source members that
                // match fixed (non-parameterized) target members before constraining
                // against parameterized members. This implements TypeScript's inference
                // filtering: for `T | undefined`, `undefined` in the source should match
                // the fixed `undefined` in the target, not be inferred as T.
                let s_members = self.interner.type_list(s_members);
                let t_members_list = self.interner.type_list(t_members);

                // Collect fixed target members (those without placeholders) once per
                // target union for this inference pass. Fixed members are resolved and
                // flattened: if a fixed member is a Lazy type alias that evaluates to a
                // union (e.g., `Primitive` = `number | string | boolean | Date`), its
                // constituent types are added individually. This ensures source members
                // like `number` can be matched against the expanded alias contents,
                // preventing them from being incorrectly inferred as type parameter `T`
                // in patterns like `T | Primitive`.
                let fixed_targets = if let Some(cached) = self
                    .constraint_fixed_union_members
                    .borrow()
                    .get(&target)
                    .cloned()
                {
                    cached
                } else {
                    let mut member_visited = FxHashSet::default();
                    let mut computed = FxHashSet::default();
                    for &member in t_members_list.iter() {
                        member_visited.clear();
                        if !self.type_contains_placeholder(member, var_map, &mut member_visited) {
                            computed.insert(member);
                            // Resolve Lazy type aliases and flatten unions so that
                            // individual constituent types can be matched by identity.
                            let evaluated = self.checker.evaluate_type(member);
                            if evaluated != member {
                                if let Some(TypeData::Union(inner_members)) =
                                    self.interner.lookup(evaluated)
                                {
                                    let inner = self.interner.type_list(inner_members);
                                    for &inner_member in inner.iter() {
                                        computed.insert(inner_member);
                                    }
                                } else {
                                    computed.insert(evaluated);
                                }
                            }
                        }
                    }
                    self.constraint_fixed_union_members
                        .borrow_mut()
                        .insert(target, computed.clone());
                    computed
                };

                // Collect source members that matched fixed targets, so we can
                // build a reduced target for the remaining (placeholder) members.
                // This matches tsc's inferFromMatchingTypes: after pairing off
                // identical members (e.g., `null` ↔ `null`), the remaining source
                // placeholders are inferred against the remaining target members.
                // Without this reduction, `T | null` against `HTMLElement | null`
                // would infer T = HTMLElement | null (wrong) instead of T = HTMLElement.
                let matched_fixed: FxHashSet<TypeId> = s_members
                    .iter()
                    .copied()
                    .filter(|member| fixed_targets.contains(member))
                    .collect();
                let has_unmatched_source = s_members
                    .iter()
                    .any(|member| !fixed_targets.contains(member));

                // Build a reduced target union: remove matched fixed members.
                // Only do this when there are unmatched source members (placeholders)
                // that need inference from the remaining target members.
                let reduced_target = if !matched_fixed.is_empty() && has_unmatched_source {
                    let remaining_targets: Vec<TypeId> = t_members_list
                        .iter()
                        .copied()
                        .filter(|member| !matched_fixed.contains(member))
                        .collect();
                    if remaining_targets.is_empty()
                        || remaining_targets.len() == t_members_list.len()
                    {
                        target // No reduction possible or nothing removed
                    } else {
                        crate::utils::union_or_single(self.interner, remaining_targets)
                    }
                } else {
                    target
                };

                for &member in s_members.iter() {
                    // Skip source members that directly match a fixed target member
                    if !fixed_targets.contains(&member) {
                        self.constrain_types(ctx, var_map, member, reduced_target, priority);
                    }
                }
            }
            (Some(TypeData::Union(s_members)), _) => {
                let s_members = self.interner.type_list(s_members);
                // When all union members are Applications with the same base as the
                // target Application, combine type arguments into unions and constrain
                // once. This avoids BCT picking one branch over another for inference
                // from union-of-generics: e.g., Interface<A|B> | Interface<C> against
                // Interface<T> should infer T = A|B|C, not just one branch.
                if let Some(TypeData::Application(t_app_id)) = self.interner.lookup(target) {
                    let t_app = self.interner.type_application(t_app_id);
                    let t_base = t_app.base;
                    let t_args_len = t_app.args.len();
                    let mut all_same_base = !s_members.is_empty() && t_args_len > 0;
                    let mut combined_args: Vec<Vec<TypeId>> = vec![Vec::new(); t_args_len];
                    if all_same_base {
                        for &member in s_members.iter() {
                            if let Some(TypeData::Application(s_app_id)) =
                                self.interner.lookup(member)
                            {
                                let s_app = self.interner.type_application(s_app_id);
                                if s_app.base == t_base && s_app.args.len() == t_args_len {
                                    for (i, &arg) in s_app.args.iter().enumerate() {
                                        combined_args[i].push(arg);
                                    }
                                } else {
                                    all_same_base = false;
                                    break;
                                }
                            } else {
                                all_same_base = false;
                                break;
                            }
                        }
                    }
                    if all_same_base {
                        let t_app_args = t_app.args.clone();
                        for (i, t_arg) in t_app_args.iter().enumerate() {
                            let combined = self.interner.union(combined_args[i].clone());
                            self.constrain_types(ctx, var_map, combined, *t_arg, priority);
                        }
                    } else {
                        // When the target Application evaluates to a conditional type
                        // whose check type is an inference variable, infer the whole
                        // source union against the check type rather than decomposing.
                        // This matches tsc's `inferFromConditionalType` behavior.
                        let cond_eval = self.checker.evaluate_type(target);
                        if cond_eval != target
                            && let Some(TypeData::Conditional(cond_id)) =
                                self.interner.lookup(cond_eval)
                        {
                            let cond = self.interner.get_conditional(cond_id);
                            if var_map.contains_key(&cond.check_type) {
                                self.constrain_types(
                                    ctx,
                                    var_map,
                                    source,
                                    cond.check_type,
                                    priority,
                                );
                                return;
                            }
                        }

                        // When the target Application has placeholder args and expands
                        // to a union, expand it first and use Union-Union logic with
                        // fixed member filtering. This prevents source members that match
                        // fixed target members (e.g., "FAILURE" in `T | "FAILURE"`) from
                        // being incorrectly added as inference candidates.
                        //
                        // Without this, `number | "FAILURE"` against `MyResult<T>` (where
                        // `MyResult<T> = T | "FAILURE"`) would decompose the source into
                        // `number` and `"FAILURE"`, constrain each against the Application
                        // individually, and infer T = number | "FAILURE" instead of T = number.
                        let t_app_args_clone = t_app.args.clone();
                        let has_placeholder = {
                            let mut visited = FxHashSet::default();
                            t_app_args_clone.iter().any(|arg| {
                                self.type_contains_placeholder(*arg, var_map, &mut visited)
                            })
                        };
                        if has_placeholder
                            && let Some(expanded) =
                                self.checker.expand_type_alias_application(target)
                            && expanded != target
                            && matches!(self.interner.lookup(expanded), Some(TypeData::Union(_)))
                        {
                            // Redirect to Union-Union path with the expanded target
                            self.constrain_types(ctx, var_map, source, expanded, priority);
                            return;
                        }

                        for &member in s_members.iter() {
                            self.constrain_types(ctx, var_map, member, target, priority);
                        }
                    }
                } else {
                    for &member in s_members.iter() {
                        self.constrain_types(ctx, var_map, member, target, priority);
                    }
                }
            }
            (_, Some(TypeData::Intersection(t_members))) => {
                let t_members = self.interner.type_list(t_members);
                for &member in t_members.iter() {
                    self.constrain_types(ctx, var_map, source, member, priority);
                }
            }
            // Source is an intersection: decompose and constrain each member against
            // the target. This handles contravariant positions where the intersection
            // type parameter ends up as the source after argument swapping.
            // Example: source = {dispatch: number} & OwnProps, target = {store: string}
            //   → constrain {dispatch: number} against {store: string} (no-op)
            //   → constrain OwnProps against {store: string} (adds upper bound)
            (Some(TypeData::Intersection(s_members)), _) => {
                let s_members = self.interner.type_list(s_members);
                for &member in s_members.iter() {
                    self.constrain_types(ctx, var_map, member, target, priority);
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
                    //
                    // However, if the source matches a fixed (non-placeholder) member
                    // of the target union, skip the constraint. This prevents incorrect
                    // inference when a type alias like `Result<T> = T | "FAILURE"` is
                    // used: source "FAILURE" should match fixed target "FAILURE", not
                    // be inferred as T. This mirrors the filtering done in the
                    // Union-Union handler above.
                    // Collect fixed (non-placeholder) target members
                    let fixed_targets: Vec<TypeId> =
                        t_members.iter().copied().filter(|&t| t != member).collect();

                    // When source is a union, perform union subtraction: filter out
                    // source members matching fixed targets, constrain only the rest.
                    // This mirrors TSC's `inferFromMatchingTypes`.
                    let source_members: Option<Vec<TypeId>> =
                        if let Some(TypeData::Union(s_members)) = self.interner.lookup(source) {
                            Some(self.interner.type_list(s_members).to_vec())
                        } else {
                            None
                        };

                    if let Some(s_members) = source_members {
                        for s_member in s_members {
                            if !self.source_matches_any_fixed(s_member, &fixed_targets) {
                                self.constrain_types(ctx, var_map, s_member, member, priority);
                            }
                        }
                    } else if !self.source_matches_any_fixed(source, &fixed_targets) {
                        self.constrain_types(ctx, var_map, source, member, priority);
                    }
                } else if placeholder_count > 1 {
                    // Multiple placeholder-containing members: prefer structural matches.
                    // For example, when source is `Foo<U>` and target is `V | Foo<V>`,
                    // constrain only against `Foo<V>` (structural match), not `V` (naked
                    // type param). This prevents `Foo<U>` from being added as a candidate
                    // for `V` when a better structural decomposition exists.
                    let placeholder_members: Vec<TypeId> = {
                        let mut result = Vec::new();
                        for &member in t_members.iter() {
                            member_visited.clear();
                            if self.type_contains_placeholder(member, var_map, &mut member_visited)
                            {
                                result.push(member);
                            }
                        }
                        result
                    };

                    // Check if any placeholder member structurally matches the source
                    let structural_matches: Vec<TypeId> = placeholder_members
                        .iter()
                        .filter(|&&member| {
                            self.types_share_outer_structure_for_constraint(source, member)
                        })
                        .copied()
                        .collect();

                    if !structural_matches.is_empty() {
                        // Discriminated union inference: if the source is an object
                        // with discriminant properties (literal-typed properties that
                        // appear in target union members), narrow to only the matching
                        // variant(s). This matches tsc's behavior for patterns like:
                        //   type Item<T> = { kind: 'a', data: T } | { kind: 'b', data: T[] }
                        //   foo({ kind: 'b', data: [1, 2] }) → infer only from kind:'b' variant
                        let infer_targets = if structural_matches.len() > 1 {
                            self.filter_by_discriminant(source, &structural_matches)
                        } else {
                            structural_matches
                        };

                        for member in infer_targets {
                            self.constrain_types(ctx, var_map, source, member, priority);
                        }
                    } else {
                        // No structural match — constrain against all placeholder members
                        for member in placeholder_members {
                            self.constrain_types(ctx, var_map, source, member, priority);
                        }
                    }
                } else if placeholder_count == 0 {
                    // No placeholder members in the target union, but the SOURCE may
                    // contain placeholders (e.g., from contextual return type seeding).
                    // Example: `Promise<__infer_0> <: Obj | PromiseLike<Obj>` —
                    // try constraining source against each non-nullish target member
                    // so structural decomposition can extract inference candidates.
                    member_visited.clear();
                    if self.type_contains_placeholder(source, var_map, &mut member_visited) {
                        for &member in t_members.iter() {
                            if !is_nullish(member) {
                                self.constrain_types(ctx, var_map, source, member, priority);
                            }
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
                        if t_p.rest || s_p.rest {
                            // If target has a rest parameter, we stop the 1-to-1 mapping
                            // The special cases below will handle inferring the rest tuple
                            // or array element type.
                            break;
                        }
                        self.constrain_parameter_types(
                            ctx,
                            var_map,
                            s_p.type_id,
                            t_p.type_id,
                            priority,
                        );
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
                        self.constrain_parameter_types(ctx, var_map, s_this, t_this, priority);
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
                    let mut source_var_map: FxHashMap<
                        TypeId,
                        crate::inference::infer::InferenceVar,
                    > = FxHashMap::default();
                    let mut src_placeholder_visited = FxHashSet::default();
                    let mut src_placeholder_buf = String::with_capacity(28);

                    // Create fresh inference variables for the source function's type parameters
                    for tp in &s_fn.type_params {
                        let var = ctx.fresh_var();
                        use std::fmt::Write;
                        src_placeholder_buf.clear();
                        let global_id = crate::operations::generic_call::PLACEHOLDER_COUNTER
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        write!(src_placeholder_buf, "__infer_src_{global_id}")
                            .expect("write to String is infallible");
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
                                ctx.set_declared_constraint(var, inst_constraint);
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
                            target: pred.target,
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
                        self.constrain_parameter_types(
                            ctx,
                            &combined_var_map,
                            s_p.type_id,
                            t_p.type_id,
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
                        self.constrain_parameter_types(
                            ctx,
                            &combined_var_map,
                            s_this,
                            t_this,
                            priority,
                        );
                    }
                    // Covariant return: instantiated_source_return <: target_return
                    //
                    // When both the source return and target return are inference
                    // placeholders, unify them so they share candidates/bounds.
                    // This handles chains like: compose(unbox, unlist) where
                    // unlist's U is related to B through Array<U> = B, and C = U.
                    // Without unification, U gets no direct candidates.
                    if let (Some(&s_var), Some(&t_var)) = (
                        combined_var_map.get(&instantiated_return),
                        combined_var_map.get(&t_fn.return_type),
                    ) {
                        let _ = ctx.unify_vars(s_var, t_var);
                    } else {
                        self.constrain_types(
                            ctx,
                            &combined_var_map,
                            instantiated_return,
                            t_fn.return_type,
                            priority,
                        );
                    }

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
                self.constrain_properties(
                    ctx,
                    var_map,
                    &s_callable.properties,
                    &t_callable.properties,
                    priority,
                    false, // callables are not fresh object literals
                );
                if let (Some(s_idx), Some(t_idx)) =
                    (&s_callable.string_index, &t_callable.string_index)
                {
                    self.constrain_types(
                        ctx,
                        var_map,
                        s_idx.value_type,
                        t_idx.value_type,
                        priority,
                    );
                }
                if let (Some(s_idx), Some(t_idx)) =
                    (&s_callable.number_index, &t_callable.number_index)
                {
                    self.constrain_types(
                        ctx,
                        var_map,
                        s_idx.value_type,
                        t_idx.value_type,
                        priority,
                    );
                }
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
                    } else {
                        // Generic call signature: convert to a Function type (preserving
                        // type_params) and re-enter constrain_types so the generic-source
                        // function handler (lines 625-808) creates fresh inference vars
                        // and collects constraints properly.
                        let func_type = self.interner.function(FunctionShape {
                            type_params: sig.type_params.clone(),
                            params: sig.params.clone(),
                            this_type: sig.this_type,
                            return_type: sig.return_type,
                            type_predicate: sig.type_predicate,
                            is_constructor: false,
                            is_method: false,
                        });
                        self.constrain_types(ctx, var_map, func_type, target, priority);
                    }
                } else if let Some(index) = self.select_signature_for_target(
                    &s_callable.call_signatures,
                    target,
                    var_map,
                    false,
                ) {
                    let sig = &s_callable.call_signatures[index];
                    self.constrain_call_signature_to_function(ctx, var_map, sig, &t_fn, priority);
                } else if let Some(sig) = s_callable.call_signatures.last()
                    && sig.type_params.is_empty()
                {
                    // Fallback: when no non-generic signature passes assignability
                    // against the erased target (common when the target contains
                    // inference placeholders erased to `unknown`), use the last
                    // non-generic signature for inference. This matches tsc's
                    // behavior of selecting the most general (last) overload.
                    // Skip generic signatures — their type parameters create
                    // additional complexity that can produce incorrect constraints.
                    self.constrain_call_signature_to_function(ctx, var_map, sig, &t_fn, priority);
                }
            }
            (Some(TypeData::Object(s_shape_id)), Some(TypeData::Object(t_shape_id))) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_shape = self.interner.object_shape(t_shape_id);
                let source_is_fresh = s_shape
                    .flags
                    .contains(crate::types::ObjectFlags::FRESH_LITERAL);
                self.constrain_properties(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape.properties,
                    priority,
                    source_is_fresh,
                );
            }
            (
                Some(TypeData::ObjectWithIndex(s_shape_id)),
                Some(TypeData::ObjectWithIndex(t_shape_id)),
            ) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_shape = self.interner.object_shape(t_shape_id);
                let source_is_fresh = s_shape
                    .flags
                    .contains(crate::types::ObjectFlags::FRESH_LITERAL);
                self.constrain_properties(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape.properties,
                    priority,
                    source_is_fresh,
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
                if let (Some(s_idx), Some(t_idx)) = (&s_shape.number_index, &t_shape.string_index) {
                    // Use MappedType priority for number-to-string cross inference so
                    // candidates combine with property-to-index candidates via union.
                    // Without this, the number index contribution (e.g., `string` from
                    // enum reverse mapping) would use a higher priority than property
                    // contributions, causing the resolver to pick only the number index
                    // type instead of the union of all candidates.
                    let idx_priority = crate::types::InferencePriority::MappedType;
                    if let Some(&var) = var_map.get(&t_idx.value_type) {
                        ctx.add_index_signature_candidate_with_index(
                            var,
                            s_idx.value_type,
                            idx_priority,
                            u32::MAX, // sentinel index for number-index cross inference
                            false,
                        );
                    } else {
                        self.constrain_types(
                            ctx,
                            var_map,
                            s_idx.value_type,
                            t_idx.value_type,
                            idx_priority,
                        );
                    }
                }
                if let (Some(s_idx), Some(t_idx)) = (&s_shape.string_index, &t_shape.number_index) {
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
                let source_is_fresh = s_shape
                    .flags
                    .contains(crate::types::ObjectFlags::FRESH_LITERAL);
                self.constrain_properties(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape.properties,
                    priority,
                    source_is_fresh,
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
                let source_is_fresh = s_shape
                    .flags
                    .contains(crate::types::ObjectFlags::FRESH_LITERAL);
                self.constrain_properties(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape.properties,
                    priority,
                    source_is_fresh,
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
                let evaluated_source = self.checker.evaluate_type(source);
                let evaluated_target = self.checker.evaluate_type(target);
                let same_base_application =
                    s_app.base == t_app.base && s_app.args.len() == t_app.args.len();
                tracing::trace!(
                    source = source.0,
                    target = target.0,
                    s_base = s_app.base.0,
                    t_base = t_app.base.0,
                    same_base_application,
                    eval_s = evaluated_source.0,
                    eval_t = evaluated_target.0,
                    "constrain Application-Application"
                );
                // When the target Application's type args contain inference
                // placeholders, always prefer direct arg-level matching.
                // The solver's evaluate_type cannot properly substitute
                // placeholders in interface members (it lacks the checker's
                // TypeEnvironment resolver), so structural matching on the
                // evaluated body introduces spurious TypeParameter references
                // (contra-candidates from unsubstituted method parameters).
                let target_has_placeholder_args =
                    same_base_application && t_app.args.iter().any(|arg| var_map.contains_key(arg));
                let allow_direct_arg_constraints = same_base_application
                    && (target_has_placeholder_args
                        || self.should_directly_constrain_same_base_application(source, target));
                // When bases differ but arities match and the target has
                // placeholder args, seed direct type argument constraints as a
                // supplement. The structural path (via evaluated Object types)
                // loses precision when union simplification reduces `T | any`
                // to `any` — direct arg matching preserves the original type
                // arguments (e.g., `boolean` and `any` from
                // `MyPromise<boolean, any>`).
                //
                // This mirrors tsc's behavior for interfaces extending other
                // interfaces with identity type argument mappings (e.g.,
                // `DoNothingAlias<T,U> extends MyPromise<T,U>`), where inference
                // matches type arguments directly through the inheritance chain.
                if !same_base_application
                    && s_app.args.len() == t_app.args.len()
                    && t_app.args.iter().any(|arg| var_map.contains_key(arg))
                {
                    // Use the TARGET base variance for direct arg matching.
                    // When the target type alias is contravariant (e.g., Func2<T> = ((x: T) => void) | undefined),
                    // the direct constraint should also be contravariant.
                    self.constrain_application_type_args(
                        ctx,
                        var_map,
                        t_app.base,
                        &s_app.args,
                        &t_app.args,
                        priority,
                    );
                }
                // When bases differ and the target has MORE type args than the source
                // (due to default type parameters, e.g., Iterable<T, TReturn = any>
                // referenced as Iterable<T>), match the overlapping prefix of type args.
                // Without this, Set<Foo> vs Iterable<T_placeholder, any> skips direct
                // arg matching because arg counts differ (1 vs 2), falling back to
                // structural evaluation which can't handle placeholder args correctly.
                // The zip in constrain_application_type_args handles different lengths
                // naturally by iterating over the minimum.
                else if !same_base_application
                    && !s_app.args.is_empty()
                    && s_app.args.len() < t_app.args.len()
                    && t_app.args[..s_app.args.len()]
                        .iter()
                        .any(|arg| var_map.contains_key(arg))
                {
                    self.constrain_application_type_args(
                        ctx,
                        var_map,
                        t_app.base,
                        &s_app.args,
                        &t_app.args[..s_app.args.len()],
                        priority,
                    );
                }
                let promise_like_arg_pair = if !same_base_application {
                    self.checker
                        .promise_like_type_argument(source)
                        .zip(self.checker.promise_like_type_argument(target))
                } else {
                    None
                };
                if same_base_application
                    && matches!(
                        self.interner.lookup(evaluated_target),
                        Some(TypeData::Mapped(_))
                    )
                {
                    // Target evaluates to a Mapped type (e.g., Boxified<T> →
                    // { [K in keyof T]: Box<T[K]> }). The Object→Mapped handler
                    // can't reverse-infer type arguments through keyof constraints.
                    // Since bases match, use direct argument unification to capture
                    // the type argument relationship (e.g., Bacon → T from
                    // Boxified<Bacon> vs Boxified<T>).
                    self.constrain_application_type_args(
                        ctx,
                        var_map,
                        s_app.base,
                        &s_app.args,
                        &t_app.args,
                        priority,
                    );
                } else if evaluated_source != source || evaluated_target != target {
                    // For same-base Applications, prefer direct type argument matching
                    // (matches tsc alias inference). Structural decomposition of evaluated
                    // union types causes cross-branch inference pollution (e.g.,
                    // SelectOptions<Thing> vs SelectOptions<KeyT> where the union branches
                    // Array<{key:T}> | Array<T> get cross-matched incorrectly).
                    if allow_direct_arg_constraints {
                        self.constrain_application_type_args(
                            ctx,
                            var_map,
                            s_app.base,
                            &s_app.args,
                            &t_app.args,
                            priority,
                        );
                    } else {
                        self.constrain_types(
                            ctx,
                            var_map,
                            evaluated_source,
                            evaluated_target,
                            priority,
                        );
                        if let Some((s_inner, t_inner)) = promise_like_arg_pair {
                            self.constrain_types(ctx, var_map, s_inner, t_inner, priority);
                        }
                    }
                } else if allow_direct_arg_constraints {
                    self.constrain_application_type_args(
                        ctx,
                        var_map,
                        s_app.base,
                        &s_app.args,
                        &t_app.args,
                        priority,
                    );
                } else if let Some((s_inner, t_inner)) = promise_like_arg_pair {
                    self.constrain_types(ctx, var_map, s_inner, t_inner, priority);
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
            (_, Some(TypeData::Application(t_app_id))) => {
                let t_app = self.interner.type_application(t_app_id);
                let t_app_args = t_app.args.clone();

                // When the Application has placeholder args (inference variables),
                // try expanding it to its body type (e.g., a mapped type) WITHOUT
                // evaluating. This preserves the inference variables in the result,
                // enabling reverse-mapped inference through mapped type aliases like
                // `TupleMapper<T>` where T is an inference variable.
                //
                // evaluate_type resolves inference variables to their constraints
                // (e.g., T extends any[] → any[]), which makes TupleMapper<T>
                // evaluate to Array(Wrap<any>) — losing the T connection.
                {
                    let mut visited = FxHashSet::default();
                    let has_placeholder_arg = t_app_args
                        .iter()
                        .any(|arg| self.type_contains_placeholder(*arg, var_map, &mut visited));
                    if has_placeholder_arg
                        && let Some(expanded) = self.checker.expand_type_alias_application(target)
                        && expanded != target
                    {
                        // When the source is an Array/Tuple/String and the expanded
                        // form is iterable-like (has [Symbol.iterator] or number
                        // index), skip the expansion and fall through to the iterable
                        // special case below. The expanded Object form loses the
                        // connection between array element types and the Application's
                        // type arguments, causing incorrect inference (e.g.,
                        // Map.groupBy([0,2,8], ...) would infer T=number[] instead of
                        // T=number).
                        let is_iterable_source = matches!(
                            source_key,
                            Some(TypeData::Array(_)) | Some(TypeData::Tuple(_))
                        ) || matches!(source, TypeId::STRING)
                            || matches!(
                                source_key,
                                Some(TypeData::Literal(crate::LiteralValue::String(_)))
                                    | Some(TypeData::TemplateLiteral(_))
                            );
                        if !is_iterable_source || !self.is_iterable_like_evaluated_object(expanded)
                        {
                            self.constrain_types(ctx, var_map, source, expanded, priority);
                            return;
                        }
                    }
                }

                let evaluated = self.checker.evaluate_type(target);
                trace!(
                    source = ?source,
                    source_key = ?self.interner.lookup(source),
                    target = ?target,
                    target_key = ?self.interner.lookup(target),
                    evaluated = ?evaluated,
                    evaluated_key = ?self.interner.lookup(evaluated),
                    "constrain_types: evaluated target application"
                );

                // Special case: Array/Tuple source against iterable-like Application target.
                // When the target is e.g. Iterable<readonly [K, V]> and the source is an
                // Array<T>, the evaluated Object form loses the connection between Array
                // element types and the Application's type arguments. We detect this case
                // by checking whether the evaluated form is iterable-like (has
                // [Symbol.iterator]) and directly constrain the array element type against
                // the Application's first type argument.
                // This enables inference for cases like: new Map([["", 0]]) where the
                // constructor parameter is Iterable<readonly [K, V]>.
                if !t_app_args.is_empty() {
                    let is_iterable_like = self.is_iterable_like_evaluated_object(evaluated);
                    if is_iterable_like {
                        match self.interner.lookup(source) {
                            Some(TypeData::Array(s_elem)) => {
                                // Widen the array element type before constraining.
                                // Array literals like [0, 2, 8] have fresh element type
                                // Union(0|2|8), but for iterable inference the type should
                                // be widened to `number` to match tsc's behavior. Without
                                // this, Iterable<T> matched against (0|2|8)[] would infer
                                // T = 0|2|8 instead of T = number, causing false TS2345.
                                let widened_elem = crate::operations::widening::widen_literal_type(
                                    self.interner,
                                    s_elem,
                                );
                                self.constrain_types(
                                    ctx,
                                    var_map,
                                    widened_elem,
                                    t_app_args[0],
                                    priority,
                                );
                                return;
                            }
                            Some(TypeData::Tuple(s_elems)) => {
                                let s_elems = self.interner.tuple_list(s_elems);
                                for s_elem in s_elems.iter() {
                                    let elem_type = if s_elem.rest {
                                        self.rest_element_type(s_elem.type_id)
                                    } else {
                                        s_elem.type_id
                                    };
                                    self.constrain_types(
                                        ctx,
                                        var_map,
                                        elem_type,
                                        t_app_args[0],
                                        priority,
                                    );
                                }
                                return;
                            }
                            _ => {}
                        }

                        // String primitives implement Iterable<string>.
                        // When source is string/literal-string and target is an
                        // iterable-like Application, infer the type argument from
                        // the string element type.
                        if matches!(source, TypeId::STRING)
                            || matches!(
                                self.interner.lookup(source),
                                Some(TypeData::Literal(crate::LiteralValue::String(_)))
                                    | Some(TypeData::TemplateLiteral(_))
                            )
                        {
                            self.constrain_types(
                                ctx,
                                var_map,
                                TypeId::STRING,
                                t_app_args[0],
                                priority,
                            );
                            return;
                        }
                    }
                }

                if let Some(TypeData::Callable(callable_id)) = self.interner.lookup(source) {
                    let callable = self.interner.callable_shape(callable_id);
                    trace!(
                        source_construct_sigs = ?callable
                            .construct_signatures
                            .iter()
                            .map(|sig| (
                                sig.params
                                    .iter()
                                    .map(|p| (p.type_id, self.interner.lookup(p.type_id), p.rest))
                                    .collect::<Vec<_>>(),
                                sig.return_type,
                                self.interner.lookup(sig.return_type),
                            ))
                            .collect::<Vec<_>>(),
                        "constrain_types: source callable signatures"
                    );
                }
                if let Some(TypeData::Callable(callable_id)) = self.interner.lookup(evaluated) {
                    let callable = self.interner.callable_shape(callable_id);
                    trace!(
                        construct_sigs = ?callable
                            .construct_signatures
                            .iter()
                            .map(|sig| (
                                sig.params
                                    .iter()
                                    .map(|p| (p.type_id, self.interner.lookup(p.type_id), p.rest))
                                    .collect::<Vec<_>>(),
                                sig.return_type,
                                self.interner.lookup(sig.return_type),
                            ))
                            .collect::<Vec<_>>(),
                        "constrain_types: evaluated target callable signatures"
                    );
                }
                if evaluated != target {
                    self.constrain_types(ctx, var_map, source, evaluated, priority);
                }
            }
            _ => {}
        }
    }
}
