//! Structural type matching for inference.
//!
//! This module implements the structural type-walking algorithm that collects
//! inference candidates by recursing into type shapes (objects, functions,
//! tuples, unions, intersections, template literals, etc.).
//!
//! It is the core of `infer_from_types`: given a source type and a target type
//! containing type parameters, it walks both structures in parallel and records
//! lower/upper bound candidates for each inference variable.

use crate::def::DefId;
use crate::instantiation::instantiate::{
    TypeSubstitution, instantiate_generic_cached, instantiate_type,
};
use crate::relations::variance::compute_type_param_variances_with_resolver;
use crate::types::{
    CallableShapeId, FunctionShapeId, InferencePriority, IntrinsicKind, LiteralValue, MappedTypeId,
    ObjectShapeId, ParamInfo, PropertyInfo, TemplateLiteralId, TemplateSpan, TupleElement,
    TupleListId, TypeApplicationId, TypeData, TypeId, TypeListId, Variance,
};
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;

use super::infer::{InferenceContext, InferenceError};
use super::infer_matching_guard_state as guard_state;
use super::infer_matching_helpers::constraint_is_nullable_union;

impl<'a> InferenceContext<'a> {
    /// Perform structural type inference from a source type to a target type.
    ///
    /// This is the core algorithm for inferring type parameters from function arguments.
    /// It walks the structure of both types, collecting constraints for type parameters.
    ///
    /// # Arguments
    /// * `source` - The type from the value argument (e.g., `string` from `identity("hello")`)
    /// * `target` - The type from the parameter (e.g., `T` from `function identity<T>(x: T)`)
    /// * `priority` - The inference priority (e.g., `NakedTypeVariable` for direct arguments)
    ///
    /// # Type Inference Algorithm
    ///
    /// TypeScript uses structural type inference with the following rules:
    ///
    /// 1. **Direct Parameter Match**: If target is a type parameter `T` we're inferring,
    ///    add source as a lower bound candidate for `T`.
    ///
    /// 2. **Structural Recursion**: For complex types, recurse into the structure:
    ///    - Objects: Match properties recursively
    ///    - Arrays: Match element types
    ///    - Functions: Match parameters (contravariant) and return types (covariant)
    ///
    /// 3. **Variance Handling**:
    ///    - Covariant positions (properties, arrays, return types): `infer(source, target)`
    ///    - Contravariant positions (function parameters): `infer(target, source)` (swapped!)
    ///
    /// # Example
    /// ```text
    /// let mut ctx = InferenceContext::new(&interner);
    /// let t_var = ctx.fresh_type_param(interner.intern_string("T"), false);
    ///
    /// // Inference: identity("hello") should infer T = string
    /// ctx.infer_from_types(string_type, t_type, InferencePriority::NakedTypeVariable)?;
    /// ```
    pub fn infer_from_types(
        &mut self,
        source: TypeId,
        target: TypeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let inserted_visit = self.infer_depth < Self::MAX_INFER_DEPTH
            && self
                .infer_visited
                .insert((source, target, self.inference_visit_mode()));
        match guard_state::infer_match_entry_state(
            self.infer_depth,
            Self::MAX_INFER_DEPTH,
            inserted_visit,
        ) {
            guard_state::InferMatchEntryState::Entered { depth } => self.infer_depth = depth,
            guard_state::InferMatchEntryState::DepthExceeded
            | guard_state::InferMatchEntryState::AlreadyVisited => return Ok(()),
        }
        let result = self.infer_from_types_inner(source, target, priority);
        self.infer_depth -= 1;
        result
    }

    fn infer_from_types_inner(
        &mut self,
        source: TypeId,
        target: TypeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        // Resolve the types to their actual TypeDatas
        let source_key = self.interner.lookup(source);
        let target_key = self.interner.lookup(target);

        // Block inference if target is NoInfer<T> (TypeScript 5.4+)
        // NoInfer prevents inference from flowing through this type position
        if let Some(TypeData::NoInfer(_)) = target_key {
            return Ok(()); // Stop inference - don't descend into NoInfer
        }

        // Unwrap NoInfer from source if present (rare but possible)
        let source_key = if let Some(TypeData::NoInfer(inner)) = source_key {
            self.interner.lookup(inner)
        } else {
            source_key
        };

        // Case 1: Target is a TypeParameter we're inferring (Lower Bound: source <: T)
        if let Some(TypeData::TypeParameter(ref param_info)) = target_key
            && let Some(var) = self.find_type_param(param_info.name)
        {
            // Add source as a lower bound candidate for this type parameter.
            // Flag the candidate as a top-level naked-argument match when this is
            // the outermost inference (depth 1): the parameter type IS the bare
            // type parameter, so the source is a whole argument expression, not a
            // constituent of a structural walk. Only in that position does tsz's
            // candidate order match tsc's source order, making the primitive
            // leftmost-wins fallback safe (#17484).
            let prev = self.candidate_from_top_level_naked;
            let prev_walk = self.candidate_at_top_level_of_walk;
            self.candidate_from_top_level_naked = self.infer_depth == 1;
            self.candidate_at_top_level_of_walk = self.infer_depth == 1;
            self.add_candidate(var, source, priority);
            self.candidate_from_top_level_naked = prev;
            self.candidate_at_top_level_of_walk = prev_walk;
            return Ok(());
        }

        // Case 2: Source is a TypeParameter we're inferring (Upper Bound: T <: target)
        // CRITICAL: This handles contravariance! When function parameters are swapped,
        // the TypeParameter moves to source position and becomes an upper bound.
        if let Some(TypeData::TypeParameter(ref param_info)) = source_key
            && let Some(var) = self.find_type_param(param_info.name)
        {
            // Beneath a structural variance walk, route through the ordinary candidate
            // entrypoint. Its live polarity selects regular versus contra-candidates, while
            // method bivariance can suppress contra classification. Outside such a walk this
            // remains a hard upper bound.
            self.add_source_type_param_candidate(var, target, priority);
            return Ok(());
        }

        // Resolve Lazy types before structural dispatch. Lazy(DefId) types are
        // opaque references that the inference engine can't match structurally.
        // Resolve them to their underlying types so inference can see the structure.
        if let Some(TypeData::Lazy(def_id)) = source_key
            && let Some(resolved) = self.resolve_lazy_for_inference(def_id, source)
            && resolved != source
        {
            return self.infer_from_types(resolved, target, priority);
        }
        if let Some(TypeData::Lazy(def_id)) = target_key
            && let Some(resolved) = self.resolve_lazy_for_inference(def_id, target)
            && resolved != target
        {
            return self.infer_from_types(source, resolved, priority);
        }

        // Case 3: Structural recursion - match based on type structure
        match (source_key, target_key) {
            // Object types: recurse into properties
            (
                Some(TypeData::Object(source_shape) | TypeData::ObjectWithIndex(source_shape)),
                Some(TypeData::Object(target_shape) | TypeData::ObjectWithIndex(target_shape)),
            ) => {
                self.infer_objects(source_shape, target_shape, priority)?;
            }

            // Function types: handle variance (parameters are contravariant, return is covariant)
            (Some(TypeData::Function(source_func)), Some(TypeData::Function(target_func))) => {
                self.infer_functions(source_func, target_func, priority)?;
            }

            // Callable types: infer across signatures and properties
            (Some(TypeData::Callable(source_call)), Some(TypeData::Callable(target_call))) => {
                self.infer_callables(source_call, target_call, priority)?;
            }

            // Cross-type Function ↔ Callable inference: when a Function needs to be
            // inferred against a Callable's call signature (or vice versa), bridge them
            // by matching the function shape against the callable's last call signature.
            (Some(TypeData::Function(source_func)), Some(TypeData::Callable(target_call))) => {
                let target = self.interner.callable_shape(target_call);
                if let Some(target_sig) = target.call_signatures.last() {
                    self.infer_function_vs_signature(source_func, target_sig, priority)?;
                }
            }
            (Some(TypeData::Callable(source_call)), Some(TypeData::Function(target_func))) => {
                let source = self.interner.callable_shape(source_call);
                if let Some(source_sig) = source.call_signatures.last() {
                    self.infer_signature_vs_function(source_sig, target_func, priority)?;
                }
            }

            // Array types: recurse into element types.
            (Some(TypeData::Array(source_elem)), Some(TypeData::Array(target_elem))) => {
                let prev = self.in_array_element_context;
                self.in_array_element_context = true;
                self.infer_from_types(source_elem, target_elem, priority)?;
                self.in_array_element_context = prev;
            }

            // Tuple types: recurse into elements
            (Some(TypeData::Tuple(source_elems)), Some(TypeData::Tuple(target_elems))) => {
                self.infer_tuples(source_elems, target_elems, priority)?;
            }

            // Array source against single-rest variadic tuple target `[...T]`
            // where `T` is itself a type parameter being inferred: the variadic
            // tuple is structurally equivalent to its rest element, so infer
            // the source array against that type parameter. This is the case
            // tsc handles for parameters like `(t: [...T]) => ...` called with
            // an array argument — tsc infers `T = sourceArray`. Without this
            // rule, `T` falls back to its constraint (`unknown[]`) and the
            // assignability check then reports the constraint in the
            // diagnostic. The rest element must be a type parameter (i.e. an
            // inference variable); for concrete-array rest elements like
            // `[...string[]]` there is nothing to infer, and for nested
            // structural rest types we want the regular structural recursion
            // to apply, not this single-rest reduction.
            (Some(TypeData::Array(_)), Some(TypeData::Tuple(target_elems))) => {
                let target_list = self.interner.tuple_list(target_elems);
                if target_list.len() == 1 && target_list[0].rest {
                    let rest_type = target_list[0].type_id;
                    let rest_is_inference_param = match self.interner.lookup(rest_type) {
                        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                            self.find_type_param(info.name).is_some()
                        }
                        _ => false,
                    };
                    if rest_is_inference_param {
                        self.infer_from_types(source, rest_type, priority)?;
                    }
                }
            }

            // Union types: try to infer against each member
            (Some(TypeData::Union(source_members)), Some(TypeData::Union(target_members))) => {
                self.infer_unions(source_members, target_members, priority)?;
            }

            // Intersection types: both source and target are intersections
            (
                Some(TypeData::Intersection(source_members)),
                Some(TypeData::Intersection(target_members)),
            ) => {
                self.infer_intersections(source_members, target_members, priority)?;
            }

            // Target is a union but source is not: decompose the target, preferring
            // structured inference targets over naked type parameters. This matches
            // the union-to-union path below for cases like Promise.then, where the
            // callback return target is `T | PromiseLike<T>` and a source
            // `Promise<any>` must infer `T = any` from the structured thenable arm
            // instead of `T = Promise<any>` from the naked arm.
            (_, Some(TypeData::Union(target_members))) => {
                let target_list = self.interner.type_list(target_members);
                let resolved_targets = self.resolve_and_flatten_union_members(&target_list);
                let parameterized: Vec<TypeId> = resolved_targets
                    .iter()
                    .copied()
                    .filter(|&target_member| self.target_contains_inference_param(target_member))
                    .collect();

                let (naked_params, structured_params): (Vec<TypeId>, Vec<TypeId>) =
                    parameterized.iter().partition(|&&target_member| {
                        !target_member.is_intrinsic()
                            && matches!(
                                self.interner.lookup(target_member),
                                Some(TypeData::TypeParameter(_))
                            )
                    });

                let mut matched_structured = false;
                for &target_member in &structured_params {
                    if self.types_share_outer_structure(source, target_member) {
                        matched_structured = true;
                        self.infer_from_types(source, target_member, priority)?;
                    }
                }

                if !matched_structured {
                    for &target_member in &naked_params {
                        self.infer_from_types(source, target_member, priority)?;
                    }
                }
            }

            // Target is an intersection but source is not: decompose the target
            // and infer against each member. This handles cases like:
            //   source: {store: string}  target: {dispatch: number} & OwnProps
            // We try each intersection member so that type parameters within the
            // target (like OwnProps, or union branches) can be inferred from the source.
            (_, Some(TypeData::Intersection(target_members))) => {
                let target_list = self.interner.type_list(target_members);
                for &target_member in target_list.iter() {
                    let _ = self.infer_from_types(source, target_member, priority);
                }
            }

            // Source is an intersection but target is not: try inferring from
            // each source member against the target.
            (Some(TypeData::Intersection(source_members)), _) => {
                let source_list = self.interner.type_list(source_members);
                for &source_member in source_list.iter() {
                    let _ = self.infer_from_types(source_member, target, priority);
                }
            }

            // TypeApplication: recurse into instantiated type
            (Some(TypeData::Application(source_app)), Some(TypeData::Application(target_app))) => {
                self.infer_applications(source, source_app, target, target_app, priority)?;
            }

            // Index access types: infer both object and index types
            (
                Some(TypeData::IndexAccess(source_obj, source_idx)),
                Some(TypeData::IndexAccess(target_obj, target_idx)),
            ) => {
                self.infer_from_types(source_obj, target_obj, priority)?;
                self.infer_from_types(source_idx, target_idx, priority)?;
            }

            // Reverse mapped type inference: target is T[K] where T is an
            // inference parameter and K is a concrete literal key.
            // This arises from homomorphic mapped types like
            //   { [K in keyof T]: Reducer<T[K], A> }
            // After substituting K with a property name, the template contains
            // T["propName"]. We accumulate (key, source_type) pairs so that
            // `infer_from_mapped_type` can build a single object candidate for T.
            (_, Some(TypeData::IndexAccess(target_obj, target_idx))) => {
                // Resolve Lazy wrappers on the object part — type parameters
                // may be stored as Lazy(DefId) rather than TypeParameter directly.
                let resolved_obj =
                    if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(target_obj) {
                        self.resolve_lazy_for_inference(def_id, target_obj)
                            .unwrap_or(target_obj)
                    } else {
                        target_obj
                    };
                if let Some(TypeData::TypeParameter(ref param_info)) =
                    self.interner.lookup(resolved_obj)
                    && let Some(var) = self.find_type_param(param_info.name)
                    && let Some(TypeData::Literal(LiteralValue::String(key_atom))) =
                        self.interner.lookup(target_idx)
                {
                    self.reverse_mapped_properties
                        .entry(var)
                        .or_default()
                        .push((key_atom, source));
                }
            }

            // Preserve structure through keyof when inferring mapped/apparent relations.
            // Without this, `<T>(x: { [K in keyof T]: T[K] })` matched against a
            // concrete mapped type over `keyof U` falls back to `T = unknown`,
            // which makes later assignability too permissive.
            // KeyOf and ReadonlyType: unwrap structural wrappers and infer inner types
            (Some(TypeData::KeyOf(source_inner)), Some(TypeData::KeyOf(target_inner)))
            | (
                Some(TypeData::ReadonlyType(source_inner)),
                Some(TypeData::ReadonlyType(target_inner)),
            ) => {
                self.infer_from_types(source_inner, target_inner, priority)?;
            }

            // Mapped-to-mapped inference: walk both the key space and the template
            // so generic source parameters can retain the target's apparent-member
            // constraint. We intentionally do NOT infer from the templates here:
            // inferring `T` from `T[K]` against `Obj[K]` would incorrectly add
            // `T <: Obj`, collapsing array-constrained sources to plain objects.
            (Some(TypeData::Mapped(source_mapped)), Some(TypeData::Mapped(target_mapped))) => {
                let source_mapped = self.interner.mapped_type(source_mapped);
                let target_mapped = self.interner.mapped_type(target_mapped);

                self.infer_from_types(
                    source_mapped.constraint,
                    target_mapped.constraint,
                    priority,
                )?;
            }

            // Unwrap ReadonlyType when only target is readonly (mutable source is compatible)
            (_, Some(TypeData::ReadonlyType(target_inner))) => {
                self.infer_from_types(source, target_inner, priority)?;
            }

            // Task #40: Template literal deconstruction for infer patterns
            // Handles: source extends `prefix${infer T}suffix` ? true : false
            (Some(source_key), Some(TypeData::TemplateLiteral(target_id))) => {
                self.infer_from_template_literal(source, Some(&source_key), target_id, priority)?;
            }

            // Mapped type inference: infer from object properties against mapped type
            // Handles: source { a: string, b: number } against target { [P in K]: T }
            // Infers K from property names and T from property value types
            (
                Some(TypeData::Object(source_shape) | TypeData::ObjectWithIndex(source_shape)),
                Some(TypeData::Mapped(mapped_id)),
            ) => {
                self.infer_from_mapped_type(source_shape, mapped_id, priority)?;
            }

            // Tuple against mapped type: reverse-mapped inference from tuple elements.
            // Handles: source [Wrap<string>, Wrap<number>] against
            //   target { [K in keyof Tuple]: Wrap<Tuple[K]> }
            // Infers Tuple from the tuple elements by substituting numeric keys
            // into the template and inferring each element type.
            (Some(TypeData::Tuple(source_elems)), Some(TypeData::Mapped(mapped_id))) => {
                self.infer_from_mapped_type_tuple(source_elems, mapped_id, priority)?;
            }

            // Array against mapped type: infer element type against mapped template.
            // Handles: source Wrap<string>[] against
            //   target { [K in keyof Arr]: Wrap<Arr[K]> }
            (Some(TypeData::Array(source_elem)), Some(TypeData::Mapped(mapped_id))) => {
                self.infer_from_mapped_type_array(source_elem, mapped_id, priority)?;
            }

            // When a non-inferred source type parameter faces a structured
            // generic target (`Record<K, V>`/mapped type), infer from the
            // source's apparent type (its constraint) so target placeholders
            // get the constraint's components instead of their defaults. Do
            // not lift nullable union constraints: their nullish constituent
            // must remain visible to the final argument check.
            (
                Some(TypeData::TypeParameter(ref param_info)),
                Some(TypeData::Application(_) | TypeData::Mapped(_)),
            ) => {
                if let Some(constraint) = param_info.constraint
                    && constraint != source
                    && !constraint_is_nullable_union(self.interner, constraint)
                    && self.target_contains_inference_param(target)
                {
                    self.infer_from_types(constraint, target, priority)?;
                }
            }

            // TypeApplication source: expand type alias and recurse.
            // When a source type is a type alias application (e.g., `Mapper<string, number>`),
            // expand it to its structural form so inference can match structurally against
            // the target. Without this, composing generic functions whose return types use
            // type aliases fails inference.
            (Some(TypeData::Application(source_app_id)), _) => {
                if let Some(expanded) = self.try_expand_application(source_app_id) {
                    self.infer_from_types(expanded, target, priority)?;
                }
            }

            // TypeApplication target: expand type alias and recurse.
            // This handles cases like `Spec<T[P]>` where Spec is a mapped type alias.
            // Without expansion, inference against recursive type alias applications
            // silently fails (e.g., `{ [P in keyof T]: Func<T[P]> | Spec<T[P]> }`).
            (_, Some(TypeData::Application(target_app_id))) => {
                if let Some(expanded) = self.try_expand_application(target_app_id) {
                    self.infer_from_types(source, expanded, priority)?;
                }
            }

            // Extract Inference Improvement (TypeScript issue #25065): when the
            // target is a distributive conditional `T extends U ? T : Y` whose
            // check_type and true_type are the same naked type parameter we are
            // currently inferring, infer the source against that type parameter
            // directly. This mirrors tsc's behaviour for `Extract<T, U>` and
            // similar Extract-like aliases used in parameter positions.
            //
            // Without this rule the conditional target falls through with no
            // candidate, so K is left to its constraint default and the
            // diagnostic surface ends up reporting the constraint instead of
            // the instantiated `Extract<K, U>` (e.g. `keyof T` instead of
            // `never`).
            (_, Some(TypeData::Conditional(cond_id))) => {
                let cond = self.interner.get_conditional(cond_id);
                if cond.is_distributive
                    && cond.check_type == cond.true_type
                    && let Some(TypeData::TypeParameter(ref param_info)) =
                        self.interner.lookup(cond.check_type)
                    && self.find_type_param(param_info.name).is_some()
                {
                    self.infer_from_types(source, cond.check_type, priority)?;
                }
            }

            // If we can't match structurally, that's okay - it might mean the types are incompatible
            // The Checker will handle this with proper error reporting
            _ => {
                // No structural match possible
                // This is not an error - the Checker will verify assignability separately
            }
        }

        Ok(())
    }

    /// Infer from object types by matching properties
    fn infer_objects(
        &mut self,
        source_shape: ObjectShapeId,
        target_shape: ObjectShapeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_shape = self.interner.object_shape(source_shape);
        let target_shape = self.interner.object_shape(target_shape);

        // For each property in the target, try to find a matching property in the source
        for target_prop in &target_shape.properties {
            if let Some(source_prop) = source_shape
                .properties
                .iter()
                .find(|p| p.name == target_prop.name)
            {
                // Use partially inferable type to prevent implicit `any` from
                // flowing contravariantly into inference. Matches tsc behavior.
                let inferable_type = self.get_partially_inferable_type(source_prop.type_id);
                let was_pending_method = self.pending_target_method;
                self.pending_target_method |= target_prop.is_method;
                let result = self.infer_from_types(inferable_type, target_prop.type_id, priority);
                self.pending_target_method = was_pending_method;
                result?;
            }
        }

        // Also check index signatures for inference
        // If target has a string index signature, infer from source's string index
        if let Some(target_string_idx) = &target_shape.string_index {
            if let Some(source_string_idx) = &source_shape.string_index {
                self.infer_from_types(
                    source_string_idx.value_type,
                    target_string_idx.value_type,
                    priority,
                )?;
            } else {
                // Source has no explicit string index. Collect contributions from:
                // 1. Number index (for anonymous/enum types only — named
                //    class/interface types must declare an explicit string index)
                // 2. Named properties (implicit index signature)
                //
                // This matches tsc's behavior where `typeof E1` (numeric enum with
                // number index + named members) infers T from all value types when
                // matched against `{ [x: string]: T }`.
                //
                // Named class/interface types (e.g. `NumberMap<Function>`) are
                // excluded from implicit inference: having only
                // `[index: number]: T` does NOT let tsc infer T for a target's
                // string index parameter.
                let has_implicit_index = source_shape.symbol.is_none()
                    || source_shape
                        .flags
                        .contains(crate::types::ObjectFlags::ENUM_NAMESPACE);

                let implicit_capacity = if has_implicit_index {
                    source_shape.properties.len() + usize::from(source_shape.number_index.is_some())
                } else {
                    0
                };
                let mut implicit_parts = Vec::with_capacity(implicit_capacity);

                // Contribution from number index: in JS, numeric keys are converted
                // to strings, so for anonymous/enum types a source number index
                // contributes to string index inference. E.g., enum namespace
                // `typeof E1` has `[n: number]: string` for reverse mappings.
                //
                // For named class/interface types this is skipped — they must
                // declare an explicit string index to participate in inference.
                if has_implicit_index && let Some(s_number_idx) = &source_shape.number_index {
                    implicit_parts.push(s_number_idx.value_type);
                }

                // Contribution from named properties (implicit index signature).
                // For anonymous object types (no symbol), all property values
                // contribute. For enum namespace types (ENUM_NAMESPACE flag), named
                // properties also contribute — tsc treats enum namespaces as
                // having implicit string index signatures derived from their members.
                //
                // Named class/interface instance types are excluded — they must
                // declare an explicit index signature.
                //
                // Symbol-keyed properties (stored with "__unique_" prefix) must be
                // excluded: they do not participate in string index signatures.
                // e.g. `{ [sym]?: true }` should not contribute `true` when inferring
                // T from `{ [s: string]: T }`.
                if has_implicit_index && !source_shape.properties.is_empty() {
                    for p in &source_shape.properties {
                        // Skip symbol-keyed properties — they are not reachable via
                        // a string index and must not pollute string-index inference.
                        if p.is_symbol_named {
                            continue;
                        }
                        // Optionality represents that the property may be missing;
                        // the stored property type represents the value when present.
                        // Use it directly so `a?: number` contributes `number`, while
                        // an explicitly annotated `b?: number | undefined` preserves
                        // its written `undefined` member.
                        implicit_parts.push(p.type_id);
                    }
                }

                if !implicit_parts.is_empty() {
                    let implicit_index_type = if implicit_parts.len() == 1 {
                        implicit_parts[0]
                    } else {
                        self.interner.union(implicit_parts)
                    };
                    self.infer_from_types(
                        implicit_index_type,
                        target_string_idx.value_type,
                        priority,
                    )?;
                }
            }
        }

        // If target has a number index signature, infer from source's number index
        if let Some(target_number_idx) = &target_shape.number_index {
            if let Some(source_number_idx) = &source_shape.number_index {
                self.infer_from_types(
                    source_number_idx.value_type,
                    target_number_idx.value_type,
                    priority,
                )?;
            } else if !source_shape.properties.is_empty() {
                // Implicit number index: collect types of numeric-named properties.
                // Same rule as string index: allow anonymous types and enum namespaces.
                let has_implicit_index = source_shape.symbol.is_none()
                    || source_shape
                        .flags
                        .contains(crate::types::ObjectFlags::ENUM_NAMESPACE);
                if has_implicit_index {
                    let numeric_types: Vec<TypeId> = source_shape
                        .properties
                        .iter()
                        .filter(|p| crate::utils::is_numeric_property_name(self.interner, p.name))
                        .map(|p| p.type_id)
                        .collect();
                    if !numeric_types.is_empty() {
                        let implicit_index_type = if numeric_types.len() == 1 {
                            numeric_types[0]
                        } else {
                            self.interner.union(numeric_types)
                        };
                        self.infer_from_types(
                            implicit_index_type,
                            target_number_idx.value_type,
                            priority,
                        )?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Infer type arguments from an object type matched against a mapped type.
    ///
    /// When source is `{ a: string, b: number }` and target is `{ [P in K]: T }`:
    /// - Infer K from the union of source property name literals ("a" | "b")
    /// - Infer T from each source property value type against the mapped template
    fn infer_from_mapped_type(
        &mut self,
        source_shape: ObjectShapeId,
        mapped_id: MappedTypeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let mapped = self.interner.mapped_type(mapped_id);
        let source = self.interner.object_shape(source_shape);

        // Detect homomorphic mapped type: constraint is `keyof T` where T
        // is an inference parameter. For these, we use reverse-mapped inference
        // to construct a candidate for T from the source type's structure.
        // This detection is shared across named-property and index-signature paths.
        let homomorphic_var = if let Some(TypeData::KeyOf(inner)) =
            self.interner.lookup(mapped.constraint)
        {
            // Resolve Lazy wrappers — type parameters may be stored as
            // Lazy(DefId) rather than TypeParameter directly.
            let resolved_inner = if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(inner) {
                self.resolve_lazy_for_inference(def_id, inner)
                    .unwrap_or(inner)
            } else {
                inner
            };
            if let Some(TypeData::TypeParameter(ref param_info)) =
                self.interner.lookup(resolved_inner)
            {
                self.find_type_param(param_info.name)
            } else {
                None
            }
        } else {
            None
        };

        // Collect only string/number-named properties for mapped-type inference.
        // Symbol-keyed properties (stored with "__unique_" prefix) must be excluded:
        // they do not participate in string/number key spaces and must not
        // contribute to constraint (`K`) or template (`T`) inference.
        let string_named_props: Vec<_> = source
            .properties
            .iter()
            .filter(|p| !p.is_symbol_named)
            .collect();

        if !string_named_props.is_empty() {
            // Infer the constraint type (K) from the union of source property names
            // e.g., for { foo: string, bar: number }, K = "foo" | "bar"
            let name_literals: Vec<TypeId> = string_named_props
                .iter()
                .map(|p| {
                    crate::utils::literal_key_for_property_name(
                        self.interner,
                        p.name,
                        p.is_string_named,
                    )
                })
                .collect();
            let names_union = if name_literals.len() == 1 {
                name_literals[0]
            } else {
                self.interner.union(name_literals)
            };
            self.infer_from_types(names_union, mapped.constraint, priority)?;

            // Infer the template type (T) from each source property value type.
            // Use MappedType priority so candidates are combined via union (not
            // common supertype). This matches tsc's PriorityImpliesCombination
            // which includes MappedTypeConstraint: when multiple properties each
            // contribute a different type for T, the result should be their union
            // (e.g., Box<number> | Box<string> | Box<boolean>), not a single "best" type.
            let template_priority = InferencePriority::MappedType;
            for prop in &string_named_props {
                let key_literal = crate::utils::literal_key_for_property_name(
                    self.interner,
                    prop.name,
                    prop.is_string_named,
                );
                let subst = TypeSubstitution::single(mapped.type_param.name, key_literal);
                let instantiated_template =
                    instantiate_type(self.interner, mapped.template, &subst);
                // Use the partially inferable version of the source property type.
                // This replaces implicit `any` parameters in function types with
                // `unknown`, preventing them from flowing contravariantly into
                // inference candidates. Matches tsc's getPartiallyInferableType.
                let inferable_prop_type = self.get_partially_inferable_type(prop.type_id);
                self.infer_from_types(
                    inferable_prop_type,
                    instantiated_template,
                    template_priority,
                )?;
            }

            // Flush accumulated reverse-mapped properties into a single object
            // candidate for the homomorphic type parameter. This handles cases
            // like `{ [K in keyof T]: Reducer<T[K], A> }` matched against
            // `{ counter1: Reducer<number> }` → T = { counter1: number }.
            //
            // Carry the source property's `declaration_order` onto each
            // candidate property so the diagnostic printer renders the
            // inferred T in the source's declared member order (matching
            // tsc's `getTypeFromInference`). Without this, candidate
            // properties default to `declaration_order = 0`, which the
            // interner overwrites using the atom-id-sorted insertion
            // index — producing a name-hash order that doesn't match tsc.
            if let Some(var) = homomorphic_var
                && let Some(props) = self.reverse_mapped_properties.remove(&var)
                && !props.is_empty()
            {
                let source_decl_order: FxHashMap<Atom, u32> = source
                    .properties
                    .iter()
                    .map(|p| (p.name, p.declaration_order))
                    .collect();
                let obj_props: Vec<PropertyInfo> = props
                    .into_iter()
                    .map(|(name, type_id)| {
                        let mut prop = PropertyInfo::new(name, type_id);
                        if let Some(&order) = source_decl_order.get(&name) {
                            prop.declaration_order = order;
                        }
                        prop
                    })
                    .collect();
                let obj_type = self.interner.object(obj_props);
                self.add_candidate(var, obj_type, InferencePriority::HomomorphicMappedType);
            }
        } else if let Some(ref string_index) = source.string_index {
            // Source has no named properties but has a string index signature
            // (e.g., `{ [index: string]: number }`). Infer K from `string`
            // and V from the index signature value type.
            self.infer_from_types(TypeId::STRING, mapped.constraint, priority)?;
            self.infer_from_types(
                string_index.value_type,
                mapped.template,
                InferencePriority::MappedType,
            )?;

            // For homomorphic mapped types (constraint is `keyof T`), the source
            // type with its index signature structure is the reverse-mapped candidate
            // for T. This matches tsc's getReverseMappedType which, for sources with
            // only index signatures matched against `{ [K in keyof T]: Template<T[K]> }`,
            // produces a type structurally equivalent to the source as the candidate.
            // This is critical for recursive mapped types like `Deep<T>` where the
            // template wraps `T[K]` — the coinductive equivalence between the source
            // and the mapped result means the source itself is a valid candidate for T.
            if let Some(var) = homomorphic_var {
                let source_type = self
                    .interner
                    .object_with_index_type_from_shape(source_shape);
                self.add_candidate(var, source_type, InferencePriority::HomomorphicMappedType);
            }
        } else if let Some(ref number_index) = source.number_index {
            // Source has a number index signature (e.g., `{ [index: number]: V }`).
            self.infer_from_types(TypeId::NUMBER, mapped.constraint, priority)?;
            self.infer_from_types(
                number_index.value_type,
                mapped.template,
                InferencePriority::MappedType,
            )?;

            // Same homomorphic reverse-mapping for number index signatures.
            if let Some(var) = homomorphic_var {
                let source_type = self
                    .interner
                    .object_with_index_type_from_shape(source_shape);
                self.add_candidate(var, source_type, InferencePriority::HomomorphicMappedType);
            }
        } else {
            // Empty object literals still constrain the mapped key space. For
            // `Pick<T, K>`, tsc infers `K = never` from `{}` instead of falling
            // back to K's constraint (`keyof T`) and requiring every property.
            self.infer_from_types(TypeId::NEVER, mapped.constraint, priority)?;
        }

        Ok(())
    }

    /// Infer type parameters from a tuple source against a mapped type target.
    ///
    /// For a mapped type `{ [K in keyof T]: Template<T[K]> }` and a source tuple
    /// `[Wrap<string>, Wrap<number>]`, this:
    /// 1. Substitutes K with "0", "1", etc. in the template
    /// 2. Infers each element type against the instantiated template
    ///
    /// This matches tsc's `inferFromMappedType` which handles both object and
    /// tuple sources against mapped types.
    fn infer_from_mapped_type_tuple(
        &mut self,
        source_elems: TupleListId,
        mapped_id: MappedTypeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let mapped = self.interner.mapped_type(mapped_id);
        let source_elems = self.interner.tuple_list(source_elems);
        if source_elems.is_empty() {
            return Ok(());
        }

        let iter_param_name = mapped.type_param.name;

        // Infer the constraint type from numeric key literals
        // e.g., for [a, b, c], K = "0" | "1" | "2"
        let name_literals: Vec<TypeId> = (0..source_elems.len())
            .map(|i| {
                let key_str = i.to_string();
                let key_atom = self.interner.intern_string(&key_str);
                self.interner.literal_string_atom(key_atom)
            })
            .collect();
        let names_union = if name_literals.len() == 1 {
            name_literals[0]
        } else {
            self.interner.union_from_slice(&name_literals)
        };
        self.infer_from_types(names_union, mapped.constraint, priority)?;

        // Infer the template type from each tuple element
        let template_priority = InferencePriority::MappedType;
        for (i, elem) in source_elems.iter().enumerate() {
            let key_literal = name_literals[i];
            let subst = TypeSubstitution::single(iter_param_name, key_literal);
            let instantiated_template = instantiate_type(self.interner, mapped.template, &subst);
            let inferable_elem_type = self.get_partially_inferable_type(elem.type_id);
            self.infer_from_types(
                inferable_elem_type,
                instantiated_template,
                template_priority,
            )?;
        }

        Ok(())
    }

    /// Infer type parameters from an array source against a mapped type target.
    ///
    /// For a mapped type `{ [K in keyof T]: Template<T[K]> }` and a source array
    /// `Wrap<string>[]`, this infers from the array element type against the template
    /// using `number` as the key type.
    fn infer_from_mapped_type_array(
        &mut self,
        source_elem: TypeId,
        mapped_id: MappedTypeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let mapped = self.interner.mapped_type(mapped_id);

        // Infer the constraint from `number` (array index type)
        self.infer_from_types(TypeId::NUMBER, mapped.constraint, priority)?;

        // Infer the template from the element type
        self.infer_from_types(source_elem, mapped.template, InferencePriority::MappedType)?;

        Ok(())
    }

    /// Resolve a `Lazy(DefId)` type for inference purposes.
    ///
    /// Returns the resolved type if available, or `None` if the resolver isn't present
    /// or the `DefId` can't be resolved.
    fn resolve_lazy_for_inference(&self, def_id: DefId, _original: TypeId) -> Option<TypeId> {
        let resolver = self.resolver?;
        resolver.resolve_lazy(def_id, self.interner)
    }

    /// Try to expand a `TypeApplication` target into its instantiated body.
    ///
    /// For type aliases like `type Spec<T> = { [P in keyof T]: ... }`, this expands
    /// `Spec<SomeArg>` into the substituted mapped type body, enabling structural
    /// inference to proceed. Without this, `(Object, Application)` falls through
    /// the match and inference candidates are lost.
    ///
    fn try_expand_application(&mut self, app_id: TypeApplicationId) -> Option<TypeId> {
        let resolver = self.resolver?;
        let app = self.interner.type_application(app_id);

        if app.base.is_intrinsic() {
            return None;
        }
        let def_id = match self.interner.lookup(app.base)? {
            TypeData::Lazy(def_id) => def_id,
            _ => return None,
        };

        let depth = self.app_expansion_depth;
        match guard_state::app_expansion_state(depth, Self::MAX_APP_EXPANSION_DEPTH) {
            guard_state::AppExpansionState::Entered { depth } => {
                self.app_expansion_depth = depth;
            }
            guard_state::AppExpansionState::DepthExceeded => return None,
        }

        // Resolve the type alias body and its type parameters
        let type_params = resolver.get_lazy_type_params(def_id)?;
        let body = resolver.resolve_lazy(def_id, self.interner)?;

        // Instantiate the body with the application's type arguments
        let instantiated =
            instantiate_generic_cached(self.interner, self.query_db, body, &type_params, &app.args);

        // Restore depth after expansion
        self.app_expansion_depth = depth;

        Some(instantiated)
    }

    /// Compute the variances of each type parameter for a type application's base type.
    ///
    /// Given a base type (e.g., the `Func1` in `Func1<T>`), this resolves the DefId
    /// and computes how each type parameter is used (covariantly, contravariantly, etc.).
    /// Returns `None` if no resolver is available or the base isn't a resolvable definition.
    fn compute_application_variances(&self, base: TypeId) -> Option<std::sync::Arc<[Variance]>> {
        let resolver = self.resolver?;
        let def_id = match self.interner.lookup(base)? {
            TypeData::Lazy(def_id) => def_id,
            TypeData::TypeQuery(sym_ref) => resolver.symbol_to_def_id(sym_ref)?,
            _ => return None,
        };
        compute_type_param_variances_with_resolver(self.interner, resolver, def_id)
    }

    pub(super) fn application_base_def_id(&self, base: TypeId) -> Option<DefId> {
        if base.is_intrinsic() {
            return None;
        }
        let resolver = self.resolver?;
        match self.interner.lookup(base)? {
            TypeData::Lazy(def_id) => Some(def_id),
            TypeData::TypeQuery(sym_ref) => resolver.symbol_to_def_id(sym_ref),
            TypeData::UnresolvedTypeName(atom) => {
                let name = self.interner.resolve_atom(atom);
                resolver.resolve_unresolved_type_name(&name)
            }
            _ => None,
        }
    }

    pub(super) fn shared_application_base_def_id(
        &self,
        source_base: TypeId,
        target_base: TypeId,
    ) -> Option<DefId> {
        let resolver = self.resolver?;
        let source_def = self.application_base_def_id(source_base)?;
        let target_def = self.application_base_def_id(target_base)?;
        let source_def = resolver.canonical_def_id(source_def);
        let target_def = resolver.canonical_def_id(target_def);
        resolver
            .defs_are_equivalent(source_def, target_def)
            .then_some(source_def)
    }

    pub(super) fn application_bases_share_declaration(
        &self,
        source_base: TypeId,
        target_base: TypeId,
    ) -> bool {
        source_base == target_base
            || self
                .shared_application_base_def_id(source_base, target_base)
                .is_some()
    }

    /// Infer from function types, handling variance correctly
    fn infer_functions(
        &mut self,
        source_func: FunctionShapeId,
        target_func: FunctionShapeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        self.with_restored_inference_modes(|ctx| {
            ctx.infer_functions_scoped(source_func, target_func, priority)
        })
    }

    fn infer_functions_scoped(
        &mut self,
        source_func: FunctionShapeId,
        target_func: FunctionShapeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_sig = self.interner.function_shape(source_func);
        let target_sig = self.interner.function_shape(target_func);

        tracing::trace!(
            source_params = source_sig.params.len(),
            target_params = target_sig.params.len(),
            "infer_functions called"
        );

        // Parameters are contravariant: swap source and target.
        // Set in_contra_mode so that type parameters found in the source position
        // (after direction swap) are recorded as contra-candidates rather than hard
        // upper bounds. This matches tsc's handling of function parameter inference.
        let was_contra = self.in_contra_mode;
        let was_variance_walk = self.in_variance_walk;
        let was_bivariant = self.in_bivariant_mode;
        let was_pending_method = self.pending_target_method;
        self.in_contra_mode = !was_contra;
        self.in_variance_walk = true;
        self.in_bivariant_mode |=
            target_sig.is_method || (was_pending_method && !target_sig.is_constructor);
        self.pending_target_method = false;
        if let (Some(source_this), Some(target_this)) = (source_sig.this_type, target_sig.this_type)
        {
            self.infer_from_types(target_this, source_this, priority)?;
        }
        let mut source_params = source_sig.params.iter().peekable();
        let mut target_params = target_sig.params.iter().peekable();

        loop {
            let source_rest = source_params.peek().is_some_and(|p| p.rest);
            let target_rest = target_params.peek().is_some_and(|p| p.rest);

            tracing::trace!(
                source_rest,
                target_rest,
                "Checking rest params in loop iteration"
            );

            // If both have rest params, infer the rest element types
            if source_rest && target_rest {
                let source_param = source_params
                    .next()
                    .expect("source_rest flag guarantees next element");
                let target_param = target_params
                    .next()
                    .expect("target_rest flag guarantees next element");
                self.infer_from_types(target_param.type_id, source_param.type_id, priority)?;
                break;
            }

            // If source has rest param, infer all remaining target params into it
            if source_rest {
                let source_param = source_params
                    .next()
                    .expect("source_rest flag guarantees next element");
                for target_param in target_params.by_ref() {
                    self.infer_from_types(target_param.type_id, source_param.type_id, priority)?;
                }
                break;
            }

            // If target has rest param, infer all remaining source params into it
            if target_rest {
                let target_param = target_params
                    .next()
                    .expect("target_rest flag guarantees next element");

                // A tuple-typed rest parameter (`...args: [...T, ...U]`,
                // `[A, ...B]`, …) carries its own variadic structure; route the
                // remaining source params through variadic tuple inference so
                // adjacent-variadic arity is preserved (e.g. `bind`).
                if matches!(
                    self.interner.lookup(target_param.type_id),
                    Some(TypeData::Tuple(_))
                ) {
                    let rest: Vec<ParamInfo> = source_params.by_ref().copied().collect();
                    self.infer_source_params_against_rest_tuple(
                        &rest,
                        target_param.type_id,
                        priority,
                    )?;
                    break;
                }

                // CRITICAL: Check if target rest param is a type parameter (like A extends any[])
                // If so, we need to infer it as a TUPLE of all remaining source params,
                // not as individual param types.
                //
                // Example: wrap<A extends any[], R>(fn: (...args: A) => R)
                //          with add(a: number, b: number): number
                //          should infer A = [number, number], not A = number
                let target_is_type_param = matches!(
                    self.interner.lookup(target_param.type_id),
                    Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
                );

                tracing::trace!(
                    target_is_type_param,
                    target_param_type = ?target_param.type_id,
                    "Rest parameter inference - target is type param check"
                );

                if target_is_type_param {
                    // Collect all remaining source params into a tuple
                    let mut tuple_elements = Vec::new();
                    for source_param in source_params.by_ref() {
                        tuple_elements.push(TupleElement {
                            type_id: source_param.type_id,
                            name: source_param.name,
                            optional: source_param.optional,
                            rest: source_param.rest,
                        });
                    }

                    tracing::trace!(
                        num_elements = tuple_elements.len(),
                        "Collected source params into tuple"
                    );

                    // Infer the tuple type against the type parameter even
                    // when the source has zero fixed params. Skipping the
                    // inference for empty source params leaves the target
                    // type parameter unbound, which causes it to default to
                    // its constraint (e.g. `unknown[]`) — that hides arity
                    // mismatches in trailing rest args of the same generic
                    // tuple, like `f<U extends unknown[]>(cb: (...args: U) =>
                    // T, ...args: U)` called as `f(() => 0, "extra")` where
                    // U should resolve to `[]` and the extra trailing arg
                    // should be rejected.
                    // Note: Parameters are contravariant, so target comes first
                    let tuple_type = self.interner.tuple(tuple_elements);
                    tracing::trace!(
                        tuple_type = ?tuple_type,
                        target_param = ?target_param.type_id,
                        "Inferring tuple against type parameter"
                    );
                    self.infer_from_types(target_param.type_id, tuple_type, priority)?;
                } else {
                    // Target rest param is not a type parameter (e.g., number[] or Array<string>)
                    // Infer each source param individually against the rest element type
                    for source_param in source_params.by_ref() {
                        self.infer_from_types(
                            target_param.type_id,
                            source_param.type_id,
                            priority,
                        )?;
                    }
                }
                break;
            }

            // Neither has rest param, do normal pairwise comparison
            match (source_params.next(), target_params.next()) {
                (Some(source_param), Some(target_param)) => {
                    // Note the swapped arguments! This is the key to handling contravariance.
                    self.infer_from_types(target_param.type_id, source_param.type_id, priority)?;
                }
                _ => break, // Mismatch in arity - stop here
            }
        }

        // Restore variance modes before covariant inference (return type, type predicates).
        self.in_contra_mode = was_contra;
        self.in_variance_walk = was_variance_walk;
        self.in_bivariant_mode = was_bivariant;

        // Return type is covariant: normal order
        self.infer_from_types(source_sig.return_type, target_sig.return_type, priority)?;

        // Type predicates are covariant
        if let (Some(source_pred), Some(target_pred)) =
            (&source_sig.type_predicate, &target_sig.type_predicate)
        {
            // Compare targets by index if possible
            let targets_match = match (source_pred.parameter_index, target_pred.parameter_index) {
                (Some(s_idx), Some(t_idx)) => s_idx == t_idx,
                _ => source_pred.target == target_pred.target,
            };

            tracing::trace!(
                targets_match,
                ?source_pred.parameter_index,
                ?target_pred.parameter_index,
                "Inferring from type predicates"
            );

            if targets_match
                && source_pred.asserts == target_pred.asserts
                && let (Some(source_ty), Some(target_ty)) =
                    (source_pred.type_id, target_pred.type_id)
            {
                self.infer_from_types(source_ty, target_ty, priority)?;
            }
        }

        self.pending_target_method = was_pending_method;
        Ok(())
    }

    /// Infer from callable types, handling signatures and properties
    fn infer_callables(
        &mut self,
        source_id: CallableShapeId,
        target_id: CallableShapeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        self.with_restored_inference_modes(|ctx| {
            ctx.infer_callables_scoped(source_id, target_id, priority)
        })
    }

    fn infer_callables_scoped(
        &mut self,
        source_id: CallableShapeId,
        target_id: CallableShapeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source = self.interner.callable_shape(source_id);
        let target = self.interner.callable_shape(target_id);
        let was_pending_method = self.pending_target_method;
        self.pending_target_method = false;

        // For each call signature in the target, try to find a compatible one in the source
        for target_sig in &target.call_signatures {
            for source_sig in &source.call_signatures {
                if source_sig.params.len() == target_sig.params.len() {
                    let was_contra = self.in_contra_mode;
                    let was_variance_walk = self.in_variance_walk;
                    let was_bivariant = self.in_bivariant_mode;
                    self.in_contra_mode = !was_contra;
                    self.in_variance_walk = true;
                    self.in_bivariant_mode |= was_pending_method || target_sig.is_method;
                    if let (Some(source_this), Some(target_this)) =
                        (source_sig.this_type, target_sig.this_type)
                    {
                        self.infer_from_types(target_this, source_this, priority)?;
                    }
                    for (s_param, t_param) in source_sig.params.iter().zip(target_sig.params.iter())
                    {
                        let result =
                            self.infer_from_types(t_param.type_id, s_param.type_id, priority);
                        if result.is_err() {
                            self.in_contra_mode = was_contra;
                            self.in_variance_walk = was_variance_walk;
                            self.in_bivariant_mode = was_bivariant;
                            self.pending_target_method = was_pending_method;
                            return result;
                        }
                    }
                    self.in_contra_mode = was_contra;
                    self.in_variance_walk = was_variance_walk;
                    self.in_bivariant_mode = was_bivariant;
                    self.infer_from_types(
                        source_sig.return_type,
                        target_sig.return_type,
                        priority,
                    )?;
                    break;
                }
            }
        }

        // For each construct signature
        for target_sig in &target.construct_signatures {
            for source_sig in &source.construct_signatures {
                if source_sig.params.len() == target_sig.params.len() {
                    let was_contra = self.in_contra_mode;
                    let was_variance_walk = self.in_variance_walk;
                    let was_bivariant = self.in_bivariant_mode;
                    self.in_contra_mode = !was_contra;
                    self.in_variance_walk = true;
                    self.in_bivariant_mode |= target_sig.is_method;
                    if let (Some(source_this), Some(target_this)) =
                        (source_sig.this_type, target_sig.this_type)
                    {
                        self.infer_from_types(target_this, source_this, priority)?;
                    }
                    for (s_param, t_param) in source_sig.params.iter().zip(target_sig.params.iter())
                    {
                        let result =
                            self.infer_from_types(t_param.type_id, s_param.type_id, priority);
                        if result.is_err() {
                            self.in_contra_mode = was_contra;
                            self.in_variance_walk = was_variance_walk;
                            self.in_bivariant_mode = was_bivariant;
                            self.pending_target_method = was_pending_method;
                            return result;
                        }
                    }
                    self.in_contra_mode = was_contra;
                    self.in_variance_walk = was_variance_walk;
                    self.in_bivariant_mode = was_bivariant;
                    self.infer_from_types(
                        source_sig.return_type,
                        target_sig.return_type,
                        priority,
                    )?;
                    break;
                }
            }
        }

        // Properties
        for target_prop in &target.properties {
            if let Some(source_prop) = source
                .properties
                .iter()
                .find(|p| p.name == target_prop.name)
            {
                let child_pending_method = self.pending_target_method;
                self.pending_target_method |= target_prop.is_method;
                let result =
                    self.infer_from_types(source_prop.type_id, target_prop.type_id, priority);
                self.pending_target_method = child_pending_method;
                result?;
            }
        }

        // String index
        if let (Some(target_idx), Some(source_idx)) = (&target.string_index, &source.string_index) {
            self.infer_from_types(source_idx.value_type, target_idx.value_type, priority)?;
        }

        // Number index
        if let (Some(target_idx), Some(source_idx)) = (&target.number_index, &source.number_index) {
            self.infer_from_types(source_idx.value_type, target_idx.value_type, priority)?;
        }

        self.pending_target_method = was_pending_method;
        Ok(())
    }

    /// Infer from a Function shape against a Callable's call signature.
    /// Bridges Function ↔ Callable for cross-type inference. Only call
    /// signatures reach this bridge; construct signatures are paired by
    /// `infer_callables` above.
    fn infer_function_vs_signature(
        &mut self,
        source_func: FunctionShapeId,
        target_sig: &crate::types::CallSignature,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        self.with_restored_inference_modes(|ctx| {
            ctx.infer_function_vs_signature_scoped(source_func, target_sig, priority)
        })
    }

    fn infer_function_vs_signature_scoped(
        &mut self,
        source_func: FunctionShapeId,
        target_sig: &crate::types::CallSignature,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source = self.interner.function_shape(source_func);
        let was_pending_method = self.pending_target_method;
        let was_bivariant = self.in_bivariant_mode;
        self.pending_target_method = false;
        // Parameters are contravariant
        let was_contra = self.in_contra_mode;
        let was_variance_walk = self.in_variance_walk;
        self.in_contra_mode = !was_contra;
        self.in_variance_walk = true;
        self.in_bivariant_mode |= was_pending_method || target_sig.is_method;
        if let (Some(source_this), Some(target_this)) = (source.this_type, target_sig.this_type) {
            self.infer_from_types(target_this, source_this, priority)?;
        }
        for (s_param, t_param) in source.params.iter().zip(target_sig.params.iter()) {
            let result = self.infer_from_types(t_param.type_id, s_param.type_id, priority);
            if result.is_err() {
                self.in_contra_mode = was_contra;
                self.in_variance_walk = was_variance_walk;
                self.in_bivariant_mode = was_bivariant;
                self.pending_target_method = was_pending_method;
                return result;
            }
        }
        self.in_contra_mode = was_contra;
        self.in_variance_walk = was_variance_walk;
        self.in_bivariant_mode = was_bivariant;
        // Return type is covariant
        self.infer_from_types(source.return_type, target_sig.return_type, priority)?;
        self.pending_target_method = was_pending_method;
        Ok(())
    }

    /// Infer from a Callable's call signature against a Function shape.
    /// Bridges Callable → Function for cross-type inference.
    fn infer_signature_vs_function(
        &mut self,
        source_sig: &crate::types::CallSignature,
        target_func: FunctionShapeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        self.with_restored_inference_modes(|ctx| {
            ctx.infer_signature_vs_function_scoped(source_sig, target_func, priority)
        })
    }

    fn infer_signature_vs_function_scoped(
        &mut self,
        source_sig: &crate::types::CallSignature,
        target_func: FunctionShapeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let target = self.interner.function_shape(target_func);
        let was_pending_method = self.pending_target_method;
        let was_bivariant = self.in_bivariant_mode;
        self.pending_target_method = false;
        // Parameters are contravariant
        let was_contra = self.in_contra_mode;
        let was_variance_walk = self.in_variance_walk;
        self.in_contra_mode = !was_contra;
        self.in_variance_walk = true;
        self.in_bivariant_mode |=
            target.is_method || (was_pending_method && !target.is_constructor);
        if let (Some(source_this), Some(target_this)) = (source_sig.this_type, target.this_type) {
            self.infer_from_types(target_this, source_this, priority)?;
        }
        for (s_param, t_param) in source_sig.params.iter().zip(target.params.iter()) {
            let result = self.infer_from_types(t_param.type_id, s_param.type_id, priority);
            if result.is_err() {
                self.in_contra_mode = was_contra;
                self.in_variance_walk = was_variance_walk;
                self.in_bivariant_mode = was_bivariant;
                self.pending_target_method = was_pending_method;
                return result;
            }
        }
        self.in_contra_mode = was_contra;
        self.in_variance_walk = was_variance_walk;
        self.in_bivariant_mode = was_bivariant;
        // Return type is covariant
        self.infer_from_types(source_sig.return_type, target.return_type, priority)?;
        self.pending_target_method = was_pending_method;
        Ok(())
    }

    /// Infer from union types
    ///
    /// Implements TSC's union-to-union inference strategy:
    /// 1. Partition target members into parameterized (contains inference vars) and fixed.
    /// 2. Further split parameterized into naked type params vs structured (e.g., `Foo<V>`).
    /// 3. Filter out source members that match fixed targets.
    /// 4. For remaining source members, prefer structural matches over naked type params.
    fn infer_unions(
        &mut self,
        source_members: TypeListId,
        target_members: TypeListId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_list = self.interner.type_list(source_members);
        let target_list = self.interner.type_list(target_members);

        // Resolve Lazy types in target members and flatten any unions they resolve to.
        // This is critical for type aliases used in union targets: e.g., `T | Primitive`
        // where `Primitive = number | string | boolean | Date` must be flattened so that
        // fixed member matching can properly skip source members like `number` or `string`.
        let resolved_targets = self.resolve_and_flatten_union_members(&target_list);

        // Similarly resolve source members so they can match resolved fixed targets.
        let resolved_sources = self.resolve_and_flatten_union_members(&source_list);

        let (parameterized, fixed): (Vec<TypeId>, Vec<TypeId>) = resolved_targets
            .iter()
            .copied()
            .partition(|&t| self.target_contains_inference_param(t));

        if parameterized.is_empty() {
            // No inference targets — nothing to infer
            return Ok(());
        }

        // Further split parameterized into naked type params vs structured.
        // Match TypeScript's inferToMultipleTypes ordering: structured targets
        // get the first chance to consume source union members, and only source
        // members that did not structurally match flow to naked type variables.
        // This prevents `B | PromiseLike<B>` from inferring `T = B | PromiseLike<B>`
        // against `T | PromiseLike<T>` after the wrapper member already matched.
        let (naked_params, structured_params): (Vec<TypeId>, Vec<TypeId>) =
            parameterized.iter().partition(|&&t| {
                !t.is_intrinsic()
                    && matches!(self.interner.lookup(t), Some(TypeData::TypeParameter(_)))
            });

        let mut structurally_matched_sources = std::collections::HashSet::new();
        for &source_ty in resolved_sources.iter() {
            // Skip source members that match a fixed target, by identity or tsc's
            // number/string literal->base leg (`isTypeOrBaseIdenticalTo`); without
            // the literal leg a literal source (`13`, `"12"`) would leak past the
            // fixed `number`/`string` targets into a naked variable (#16948).
            if crate::type_queries::source_is_or_base_identical_to_fixed(
                self.interner,
                source_ty,
                |candidate| fixed.contains(&candidate),
            ) {
                continue;
            }

            for &target_ty in &structured_params {
                if self.types_share_outer_structure(source_ty, target_ty) {
                    structurally_matched_sources.insert(source_ty);
                    self.infer_from_types(source_ty, target_ty, priority)?;
                }
            }
        }

        let unmatched: Vec<TypeId> = resolved_sources
            .iter()
            .copied()
            .filter(|&source_ty| {
                !crate::type_queries::source_is_or_base_identical_to_fixed(
                    self.interner,
                    source_ty,
                    |candidate| fixed.contains(&candidate),
                ) && !structurally_matched_sources.contains(&source_ty)
            })
            .collect();

        if naked_params.len() == 1 && !unmatched.is_empty() {
            // tsc's `inferToMultipleTypes`: when the union target has exactly one
            // naked inference variable, the source constituents that did not
            // match a structured arm are *unioned* and inferred against that
            // variable as a single candidate (`inferFromTypes(getUnionType(
            // unmatched), nakedTypeVariable)`). Inferring them individually would
            // let common-supertype resolution — which governs the
            // `NakedTypeVariable` priority these candidates carry — keep only the
            // leftmost branch and silently drop the rest, e.g. collapsing the
            // element inference for `flat([1, 'a', [2]])` from `string | number`
            // down to `string` and reporting a spurious `TS2322`.
            let unioned = crate::operations::widening::union_unmatched_naked_candidate(
                self.interner,
                unmatched,
                self.in_readonly_source_context,
            );
            self.infer_from_types(unioned, naked_params[0], priority)?;
        } else if !naked_params.is_empty() {
            // Multiple naked variables (or none matched): preserve the existing
            // per-source decomposition. With more than one naked variable tsc
            // does not perform the single-variable union reduction.
            let naked_priority = if structured_params.is_empty() {
                priority
            } else {
                InferencePriority::LowPriority
            };
            for &source_ty in unmatched.iter() {
                for &target_ty in &naked_params {
                    self.infer_from_types(source_ty, target_ty, naked_priority)?;
                }
            }
        }

        Ok(())
    }

    /// Resolve Lazy types in union members and flatten any unions they resolve to.
    ///
    /// When a union contains `Lazy(DefId)` members (e.g., type alias references like
    /// `Primitive` in `T | Primitive`), this resolves them and flattens the result.
    /// For example, if `Primitive = number | string | boolean | Date`, then:
    ///   `[T, Lazy(Primitive)]` → `[T, number, string, boolean, Date]`
    ///
    /// This is necessary for correct inference matching: without flattening,
    /// source members like `number` can't be matched against the opaque `Lazy(Primitive)`
    /// and incorrectly get inferred against type parameter `T`.
    fn resolve_and_flatten_union_members(&self, members: &[TypeId]) -> Vec<TypeId> {
        let mut result = Vec::with_capacity(members.len());
        for &member in members {
            if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(member)
                && let Some(resolved) = self.resolve_lazy_for_inference(def_id, member)
                && resolved != member
            {
                if let Some(TypeData::Union(inner_members)) = self.interner.lookup(resolved) {
                    // Flatten: the lazy resolved to a union, add its members
                    let inner = self.interner.type_list(inner_members);
                    result.extend(inner.iter().copied());
                    continue;
                }
                // Resolved to a non-union type, use the resolved form
                result.push(resolved);
                continue;
            }
            result.push(member);
        }
        result
    }

    /// Check if a target type directly is or contains an inference type parameter.
    ///
    /// This must be recursive: for `V | Foo<V>`, both `V` (direct type param)
    /// and `Foo<V>` (application containing a type param) are parameterized.
    /// Without recursion, `Foo<V>` would be classified as "fixed", causing
    /// source members like `Foo<U>` to be inferred against the naked `V`
    /// instead of structurally matching `Foo<V>`.
    fn target_contains_inference_param(&self, target: TypeId) -> bool {
        self.target_contains_inference_param_inner(target, &mut std::collections::HashSet::new())
    }

    fn target_contains_inference_param_inner(
        &self,
        target: TypeId,
        visited: &mut std::collections::HashSet<TypeId>,
    ) -> bool {
        if target.is_intrinsic() {
            return false;
        }
        match guard_state::target_param_visit_state(visited.insert(target)) {
            guard_state::TargetParamVisitState::Entered => {}
            guard_state::TargetParamVisitState::AlreadyVisited { fallback } => return fallback,
        }
        let Some(key) = self.interner.lookup(target) else {
            return false;
        };
        match key {
            TypeData::TypeParameter(ref info) => self.find_type_param(info.name).is_some(),
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.target_contains_inference_param_inner(app.base, visited)
                    || app
                        .args
                        .iter()
                        .copied()
                        .any(|arg| self.target_contains_inference_param_inner(arg, visited))
            }
            TypeData::IndexAccess(object, index) => {
                self.target_contains_inference_param_inner(object, visited)
                    || self.target_contains_inference_param_inner(index, visited)
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.target_contains_inference_param_inner(inner, visited)
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.get_conditional(cond_id);
                self.target_contains_inference_param_inner(cond.check_type, visited)
                    || self.target_contains_inference_param_inner(cond.extends_type, visited)
                    || self.target_contains_inference_param_inner(cond.true_type, visited)
                    || self.target_contains_inference_param_inner(cond.false_type, visited)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
                self.target_contains_inference_param_inner(mapped.constraint, visited)
                    || mapped.name_type.is_some_and(|name_type| {
                        self.target_contains_inference_param_inner(name_type, visited)
                    })
                    || self.target_contains_inference_param_inner(mapped.template, visited)
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let list = self.interner.type_list(members);
                list.iter()
                    .any(|&m| self.target_contains_inference_param_inner(m, visited))
            }
            _ => false,
        }
    }

    /// Infer from intersection types
    fn infer_intersections(
        &mut self,
        source_members: TypeListId,
        target_members: TypeListId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_list = self.interner.type_list(source_members);
        let target_list = self.interner.type_list(target_members);

        // For structured intersection members, we can pick any source member
        // that matches. A naked target type parameter is different: matching it
        // against every source member turns `A & B` against `T & B` into
        // candidates from both `A` and `B`, so `T` resolves to an artificial
        // `A & B`. When both intersections have the same arity, preserve the
        // positional correspondence for those naked parameters while keeping
        // broad matching for the structured members.
        let use_positional_naked_params = source_list.len() == target_list.len();
        for (target_index, target_ty) in target_list.iter().enumerate() {
            if use_positional_naked_params && self.is_naked_inference_target(*target_ty) {
                let _ = self.infer_from_types(source_list[target_index], *target_ty, priority);
                continue;
            }
            for source_ty in source_list.iter() {
                let _ = self.infer_from_types(*source_ty, *target_ty, priority);
            }
        }

        Ok(())
    }

    fn is_naked_inference_target(&self, target: TypeId) -> bool {
        matches!(
            self.interner.lookup(target),
            Some(TypeData::TypeParameter(info)) if self.find_type_param(info.name).is_some()
        )
    }

    /// Infer from `TypeApplication` (generic type instantiations)
    fn infer_applications(
        &mut self,
        source: TypeId,
        source_app: TypeApplicationId,
        target: TypeId,
        target_app: TypeApplicationId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_info = self.interner.type_application(source_app);
        let target_info = self.interner.type_application(target_app);

        // When both applications share the same base type, infer directly from
        // type arguments, respecting the variance of each type parameter position.
        // This matches tsc's inferFromTypeArguments: contravariant type parameters
        // (e.g., T in `type Func<T> = (x: T) => void`) swap source/target direction
        // so that inference candidates are correctly categorized.
        let shared_base_def = if source_info.base == target_info.base {
            self.application_base_def_id(source_info.base)
        } else {
            self.shared_application_base_def_id(source_info.base, target_info.base)
        };
        if source_info.base == target_info.base || shared_base_def.is_some() {
            // Try to compute variances for the base type's type parameters.
            // This requires a resolver and a Lazy(DefId) base type.
            let variances = shared_base_def
                .and_then(|def_id| {
                    let resolver = self.resolver?;
                    compute_type_param_variances_with_resolver(self.interner, resolver, def_id)
                })
                .or_else(|| self.compute_application_variances(source_info.base));
            for (i, (source_arg, target_arg)) in source_info
                .args
                .iter()
                .zip(target_info.args.iter())
                .enumerate()
            {
                let variance = variances
                    .as_ref()
                    .and_then(|v| v.get(i).copied())
                    .unwrap_or(Variance::COVARIANT);
                if variance.is_contravariant() {
                    // Contravariant position: swap source and target so that
                    // candidates are recorded as contra-candidates (via in_contra_mode)
                    // or equivalently, infer in the reverse direction.
                    let was_contra = self.in_contra_mode;
                    let was_variance_walk = self.in_variance_walk;
                    self.in_contra_mode = !was_contra;
                    self.in_variance_walk = true;
                    let result = self.infer_from_types(*source_arg, *target_arg, priority);
                    self.in_contra_mode = was_contra;
                    self.in_variance_walk = was_variance_walk;
                    result?;
                } else {
                    self.infer_from_types(*source_arg, *target_arg, priority)?;
                }
            }
            return Ok(());
        }

        // When the bases differ, expand both applications when possible before
        // falling back to one-sided expansion. This mirrors tsc's reference
        // inference path, where structurally related generic references can still
        // contribute candidates even when their declarations differ.
        let expanded_source = self.try_expand_application(source_app);
        let expanded_target = self.try_expand_application(target_app);
        match (expanded_source, expanded_target) {
            (Some(expanded_source), Some(expanded_target)) => {
                return self.infer_from_types(expanded_source, expanded_target, priority);
            }
            (Some(expanded_source), None) => {
                return self.infer_from_types(expanded_source, target, priority);
            }
            (None, Some(expanded_target)) => {
                return self.infer_from_types(source, expanded_target, priority);
            }
            (None, None) => {}
        }

        Ok(())
    }

    // =========================================================================
    // Task #40: Template Literal Deconstruction
    // =========================================================================

    /// Infer from template literal patterns with `infer` placeholders.
    ///
    /// This implements the "Reverse String Matcher" for extracting type information
    /// from string literals that match template patterns like `user_${infer ID}`.
    ///
    /// # Example
    ///
    /// ```typescript
    /// type GetID<T> = T extends `user_${infer ID}` ? ID : never;
    /// // GetID<"user_123"> should infer ID = "123"
    /// ```
    ///
    /// # Algorithm
    ///
    /// The matching is **non-greedy** for all segments except the last:
    /// 1. Scan through template spans sequentially
    /// 2. For text spans: match literal text at current position
    /// 3. For infer type spans: capture text until next literal anchor (non-greedy)
    /// 4. For the last span: capture all remaining text (greedy)
    ///
    /// # Arguments
    ///
    /// * `source` - The source type being checked (e.g., `"user_123"`)
    /// * `source_key` - The `TypeData` of the source (cached for efficiency)
    /// * `target_template` - The template literal pattern to match against
    /// * `priority` - Inference priority for the extracted candidates
    fn infer_from_template_literal(
        &mut self,
        source: TypeId,
        source_key: Option<&TypeData>,
        target_template: TemplateLiteralId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let spans = self.interner.template_list(target_template);

        // Special case: if source is `any` or the intrinsic `string` type, all infer vars get that type
        if source == TypeId::ANY
            || matches!(source_key, Some(TypeData::Intrinsic(IntrinsicKind::String)))
        {
            for span in spans.iter() {
                if let TemplateSpan::Type(type_id) = span
                    && let Some(TypeData::Infer(param_info) | TypeData::TypeParameter(param_info)) =
                        self.interner.lookup(*type_id)
                    && let Some(var) = self.find_type_param(param_info.name)
                {
                    // Source is `any` or `string`, so infer that for all variables
                    self.add_candidate(var, source, priority);
                }
            }
            return Ok(());
        }

        // If source is a union, try to match each member against the template
        if let Some(TypeData::Union(source_members)) = source_key {
            let members = self.interner.type_list(*source_members);
            for &member in members.iter() {
                let member_key = self.interner.lookup(member);
                self.infer_from_template_literal(
                    member,
                    member_key.as_ref(),
                    target_template,
                    priority,
                )?;
            }
            return Ok(());
        }

        if let Some(TypeData::TemplateLiteral(source_template)) = source_key {
            let source_spans = self.interner.template_list(*source_template);
            if source_spans.len() == spans.len() {
                let source_spans: Vec<TemplateSpan> = source_spans.iter().cloned().collect();
                let target_spans: Vec<TemplateSpan> = spans.iter().cloned().collect();
                let mut matched = true;
                for (source_span, target_span) in source_spans.iter().zip(target_spans.iter()) {
                    match (source_span, target_span) {
                        (TemplateSpan::Text(source_text), TemplateSpan::Text(target_text))
                            if source_text == target_text => {}
                        (TemplateSpan::Type(source_type), TemplateSpan::Type(target_type)) => {
                            // Promote so a captured `${number}` segment yields
                            // a string subtype rather than the bare `number`.
                            let promoted = crate::type_queries::extended::string_like_type_for_type(
                                self.interner,
                                *source_type,
                            );
                            self.infer_from_types(promoted, *target_type, priority)?;
                        }
                        _ => {
                            matched = false;
                            break;
                        }
                    }
                }
                if matched {
                    return Ok(());
                }
            }
        }

        // For literal string types, perform the actual pattern matching
        if let Some(source_str) = self.extract_string_literal(source)
            && let Some(captures) = self.match_template_pattern(&source_str, &spans)
        {
            // Convert captured strings to literal types and add as candidates
            for (infer_var, captured_string) in captures {
                let literal_type =
                    self.coerce_captured_template_segment(infer_var, &captured_string);
                self.add_candidate(infer_var, literal_type, priority);
            }
        }

        Ok(())
    }

    /// Extract a string literal value from a `TypeId`.
    ///
    /// Returns None if the type is not a literal string.
    fn extract_string_literal(&self, type_id: TypeId) -> Option<String> {
        // BOOLEAN_TRUE/FALSE are intrinsic IDs that resolve to Literal(Boolean),
        // never Literal(String). Other intrinsics resolve to Intrinsic. Skip lookup.
        if type_id.is_intrinsic() {
            return None;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Literal(LiteralValue::String(s))) => Some(self.interner.resolve_atom(s)),
            _ => None,
        }
    }
}
