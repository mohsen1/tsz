//! Property, signature, tuple, and index signature constraint collection.
//!
//! Contains methods for constraining object properties, function signatures,
//! call signatures, tuple types, and index signatures during type inference.

use crate::inference::infer::InferenceContext;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{
    CallSignature, FunctionShape, ObjectShape, ObjectShapeId, ParamInfo, PropertyInfo,
    TupleElement, TypeData, TypeId,
};
use crate::utils;
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::trace;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(super) fn constrain_properties(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source_props: &[PropertyInfo],
        target_props: &[PropertyInfo],
        priority: crate::types::InferencePriority,
        source_is_fresh: bool,
    ) {
        let mut source_idx = 0;
        let mut target_idx = 0;

        while source_idx < source_props.len() && target_idx < target_props.len() {
            let source = &source_props[source_idx];
            let target = &target_props[target_idx];

            match source.name.cmp(&target.name) {
                std::cmp::Ordering::Equal => {
                    let property_index = source_idx as u32;
                    if let Some(&var) = var_map.get(&target.type_id) {
                        ctx.add_property_candidate_with_index(
                            var,
                            source.type_id,
                            priority,
                            property_index,
                            Some(source.name),
                            source_is_fresh,
                        );
                    } else {
                        self.constrain_types(
                            ctx,
                            var_map,
                            source.type_id,
                            target.type_id,
                            priority,
                        );
                    }
                    // Constrain write type for mutable targets.
                    // Note: readonly source → writable target is allowed during
                    // inference constraint collection (TypeScript's inferFromProperties
                    // ignores readonly).  Readonly mismatches are caught later during
                    // assignability checking, not here.
                    if !target.readonly && !source.readonly {
                        if let Some(&var) = var_map.get(&target.write_type) {
                            ctx.add_property_candidate_with_index(
                                var,
                                source.write_type,
                                priority,
                                property_index,
                                Some(source.name),
                                source_is_fresh,
                            );
                        } else {
                            // Skip the reverse-direction write_type constraint when
                            // write_type == type_id for both sides (the common case).
                            // The type_id constraint above already handles it —
                            // constrain_types(target.write_type, source.write_type)
                            // goes in the contravariant direction and creates spurious
                            // candidates that widen literals incorrectly.
                            let write_type_differs = source.write_type != source.type_id
                                || target.write_type != target.type_id;
                            if write_type_differs {
                                self.constrain_types(
                                    ctx,
                                    var_map,
                                    target.write_type,
                                    source.write_type,
                                    priority,
                                );
                            }
                        }
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

    pub(super) fn constrain_function_to_call_signature(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source: &FunctionShape,
        target: &CallSignature,
        priority: crate::types::InferencePriority,
    ) {
        for (s_p, t_p) in source.params.iter().zip(target.params.iter()) {
            self.constrain_parameter_types(ctx, var_map, s_p.type_id, t_p.type_id, priority);
        }
        if let (Some(s_this), Some(t_this)) = (source.this_type, target.this_type) {
            self.constrain_parameter_types(ctx, var_map, s_this, t_this, priority);
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

    pub(super) fn constrain_call_signature_to_function(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source: &CallSignature,
        target: &FunctionShape,
        priority: crate::types::InferencePriority,
    ) {
        for (s_p, t_p) in source.params.iter().zip(target.params.iter()) {
            self.constrain_parameter_types(ctx, var_map, s_p.type_id, t_p.type_id, priority);
        }
        if let (Some(s_this), Some(t_this)) = (source.this_type, target.this_type) {
            self.constrain_parameter_types(ctx, var_map, s_this, t_this, priority);
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

    pub(super) fn constrain_call_signature_to_call_signature(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source: &CallSignature,
        target: &CallSignature,
        priority: crate::types::InferencePriority,
    ) {
        for (s_p, t_p) in source.params.iter().zip(target.params.iter()) {
            self.constrain_parameter_types(ctx, var_map, s_p.type_id, t_p.type_id, priority);
        }
        if let (Some(s_this), Some(t_this)) = (source.this_type, target.this_type) {
            self.constrain_parameter_types(ctx, var_map, s_this, t_this, priority);
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

    pub(super) fn function_type_from_signature(&self, sig: &CallSignature, is_constructor: bool) -> TypeId {
        self.interner.function(FunctionShape {
            type_params: Vec::new(),
            params: sig.params.clone(),
            this_type: sig.this_type,
            return_type: sig.return_type,
            type_predicate: sig.type_predicate,
            is_constructor,
            is_method: false,
        })
    }

    /// Erase a signature's own type parameters by substituting defaults (or constraints, or unknown).
    /// Returns a new `CallSignature` with no `type_params` and all types instantiated.
    /// This is used when the source signature is generic but the target is not --
    /// tsc instantiates the source's type params with their defaults before inferring.
    pub(super) fn erase_signature_type_params(&self, sig: &CallSignature) -> CallSignature {
        if sig.type_params.is_empty() {
            return sig.clone();
        }
        let mut sub = TypeSubstitution::new();
        for tp in &sig.type_params {
            let replacement = tp.default.or(tp.constraint).unwrap_or(TypeId::UNKNOWN);
            sub.insert(tp.name, replacement);
        }
        CallSignature {
            type_params: Vec::new(),
            params: sig
                .params
                .iter()
                .map(|p| ParamInfo {
                    name: p.name,
                    type_id: instantiate_type(self.interner, p.type_id, &sub),
                    optional: p.optional,
                    rest: p.rest,
                })
                .collect(),
            this_type: sig
                .this_type
                .map(|t| instantiate_type(self.interner, t, &sub)),
            return_type: instantiate_type(self.interner, sig.return_type, &sub),
            type_predicate: sig.type_predicate,
            is_method: sig.is_method,
        }
    }

    pub(super) fn erase_placeholders_for_inference(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
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

    pub(super) fn select_signature_for_target(
        &mut self,
        signatures: &[CallSignature],
        target_fn: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        is_constructor: bool,
    ) -> Option<usize> {
        let target_erased = self.erase_placeholders_for_inference(target_fn, var_map);
        // First pass: try non-generic signatures
        for (index, sig) in signatures.iter().enumerate() {
            if !sig.type_params.is_empty() {
                continue;
            }
            let source_fn = self.function_type_from_signature(sig, is_constructor);
            if self.checker.is_assignable_to(source_fn, target_erased) {
                return Some(index);
            }
        }
        // Second pass: try generic signatures with type params erased to defaults
        for (index, sig) in signatures.iter().enumerate() {
            if sig.type_params.is_empty() {
                continue;
            }
            let erased = self.erase_signature_type_params(sig);
            let source_fn = self.function_type_from_signature(&erased, is_constructor);
            if self.checker.is_assignable_to(source_fn, target_erased) {
                return Some(index);
            }
        }
        None
    }

    pub(super) fn constrain_matching_signatures(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
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
            if target_sig.type_params.is_empty() {
                if source_sig.type_params.is_empty() {
                    self.constrain_call_signature_to_call_signature(
                        ctx, var_map, source_sig, target_sig, priority,
                    );
                } else {
                    // Source has type params (e.g., generic class construct sig) but target doesn't.
                    // Erase source type params using defaults/constraints before constraining.
                    let erased = self.erase_signature_type_params(source_sig);
                    self.constrain_call_signature_to_call_signature(
                        ctx, var_map, &erased, target_sig, priority,
                    );
                }
            }
            return;
        }

        if target_signatures.len() == 1 {
            let target_sig = &target_signatures[0];
            if target_sig.type_params.is_empty() {
                let source_idx = if source_signatures.len() == 1 {
                    Some(0)
                } else {
                    let target_fn = self.function_type_from_signature(target_sig, is_constructor);
                    self.select_signature_for_target(
                        source_signatures,
                        target_fn,
                        var_map,
                        is_constructor,
                    )
                };
                if let Some(idx) = source_idx {
                    let source_sig = &source_signatures[idx];
                    if source_sig.type_params.is_empty() {
                        self.constrain_call_signature_to_call_signature(
                            ctx, var_map, source_sig, target_sig, priority,
                        );
                    } else {
                        let erased = self.erase_signature_type_params(source_sig);
                        self.constrain_call_signature_to_call_signature(
                            ctx, var_map, &erased, target_sig, priority,
                        );
                    }
                }
            }
            return;
        }

        if source_signatures.len() == 1 {
            let source_sig = &source_signatures[0];
            let erased_sig;
            let effective_sig = if source_sig.type_params.is_empty() {
                source_sig
            } else {
                erased_sig = self.erase_signature_type_params(source_sig);
                &erased_sig
            };
            for target_sig in target_signatures {
                if target_sig.type_params.is_empty() {
                    self.constrain_call_signature_to_call_signature(
                        ctx,
                        var_map,
                        effective_sig,
                        target_sig,
                        priority,
                    );
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
                    if source_sig.type_params.is_empty() {
                        self.constrain_call_signature_to_call_signature(
                            ctx, var_map, source_sig, target_sig, priority,
                        );
                    } else {
                        let erased = self.erase_signature_type_params(source_sig);
                        self.constrain_call_signature_to_call_signature(
                            ctx, var_map, &erased, target_sig, priority,
                        );
                    }
                }
            }
        }
    }

    pub(super) fn constrain_properties_against_index_signatures(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source_props: &[PropertyInfo],
        target: &ObjectShape,
        _priority: crate::types::InferencePriority,
    ) {
        let string_index = target.string_index.as_ref();
        let number_index = target.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() {
            return;
        }

        // Use MappedType priority so that candidates from multiple properties are
        // combined via union. This matches tsc's behavior: for `{ [x: string]: T }`,
        // calling with `{ a: number, b: string }` should infer T = number | string.
        // Without combination priority, common supertype picks only the first type.
        let idx_priority = crate::types::InferencePriority::MappedType;

        for (i, prop) in source_props.iter().enumerate() {
            // For optional properties, strip `undefined` from the type before contributing
            // to index signature inference. When inferring T from `{ a: string, b?: number }`
            // against `{ [x: string]: T }`, tsc infers T = string | number (not
            // string | number | undefined). The optionality of a property does not contribute
            // `undefined` to the inferred index signature value type.
            let prop_type = if prop.optional {
                crate::narrowing::utils::remove_undefined(self.interner, prop.type_id)
            } else {
                prop.type_id
            };
            let property_index = i as u32;

            if let Some(number_idx) = number_index
                && utils::is_numeric_property_name(self.interner, prop.name)
            {
                if let Some(&var) = var_map.get(&number_idx.value_type) {
                    ctx.add_index_signature_candidate_with_index(
                        var,
                        prop_type,
                        idx_priority,
                        property_index,
                        false,
                    );
                } else {
                    self.constrain_types(
                        ctx,
                        var_map,
                        prop_type,
                        number_idx.value_type,
                        idx_priority,
                    );
                }
            }

            if let Some(string_idx) = string_index {
                if let Some(&var) = var_map.get(&string_idx.value_type) {
                    ctx.add_index_signature_candidate_with_index(
                        var,
                        prop_type,
                        idx_priority,
                        property_index,
                        false,
                    );
                } else {
                    self.constrain_types(
                        ctx,
                        var_map,
                        prop_type,
                        string_idx.value_type,
                        idx_priority,
                    );
                }
            }
        }
    }

    pub(super) fn constrain_index_signatures_to_properties(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source: &ObjectShape,
        target_props: &[PropertyInfo],
        priority: crate::types::InferencePriority,
    ) {
        let string_index = source.string_index.as_ref();
        let number_index = source.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() {
            return;
        }

        for (i, prop) in target_props.iter().enumerate() {
            // CRITICAL: Only infer from index signatures if the property is optional.
            // Required properties missing from the source cause a structural mismatch,
            // so TypeScript does not infer from them.
            if !prop.optional {
                continue;
            }

            let prop_type = self.optional_property_type(prop);
            let property_index = i as u32;

            if let Some(number_idx) = number_index
                && utils::is_numeric_property_name(self.interner, prop.name)
            {
                if let Some(&var) = var_map.get(&prop_type) {
                    ctx.add_index_signature_candidate_with_index(
                        var,
                        number_idx.value_type,
                        priority,
                        property_index,
                        false,
                    );
                } else {
                    self.constrain_types(ctx, var_map, number_idx.value_type, prop_type, priority);
                }
            }

            if let Some(string_idx) = string_index {
                if let Some(&var) = var_map.get(&prop_type) {
                    ctx.add_index_signature_candidate_with_index(
                        var,
                        string_idx.value_type,
                        priority,
                        property_index,
                        false,
                    );
                } else {
                    self.constrain_types(ctx, var_map, string_idx.value_type, prop_type, priority);
                }
            }
        }
    }

    pub(super) fn optional_property_type(&self, prop: &PropertyInfo) -> TypeId {
        crate::utils::optional_property_type(self.interner, prop)
    }

    pub(super) fn constrain_parameter_types(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source_param: TypeId,
        target_param: TypeId,
        priority: crate::types::InferencePriority,
    ) {
        // Function parameters are contravariant: if the target parameter is a
        // type variable placeholder, add source as a contra-candidate instead
        // of a regular (covariant) candidate. This matches tsc's behavior where
        // contravariant inferences go to `contraCandidates` and are resolved
        // via intersection (not union).
        if let Some(&var) = var_map.get(&target_param) {
            ctx.add_contra_candidate(var, source_param, priority);
            // Do not feed a bare placeholder target back into source-side type parameters.
            // For higher-order generic callbacks like `callr(sn, f16)`, that reverse edge
            // creates recursive source-placeholder candidates (`A = T`, `T = [A, B]`) and
            // blows the target callback type into a self-referential tuple union.
            let source_is_type_param = matches!(
                self.interner.lookup(source_param),
                Some(TypeData::TypeParameter(_))
            );
            if !source_is_type_param {
                // Use contra mode for the reverse direction so that the
                // placeholder appearing as source gets a contra-candidate
                // instead of a hard upper bound. This matches the behavior
                // of the complex-type branch below and prevents upper bounds
                // from overriding correct covariant inference.
                let was_contra = ctx.in_contra_mode;
                ctx.in_contra_mode = true;
                self.constrain_types(ctx, var_map, target_param, source_param, priority);
                ctx.in_contra_mode = was_contra;
            }
        } else {
            // The target parameter is a complex type containing type variables
            // (e.g., `{ kind: T }`, not just `T` directly). In tsc, callback
            // parameter inference in this case goes to `contraCandidates` because
            // function parameters are contravariant. We set `in_contra_mode` for
            // BOTH directions so that:
            // - Forward (source→target): candidates are routed to contra_candidates
            // - Reverse (target→source): type parameters in source position add
            //   contra-candidates instead of hard upper bounds
            // Without contra mode on the reverse direction, decomposing a union
            // target (e.g., {kind:T} vs {kind:'a'}|{kind:'b'}) creates separate
            // upper bounds 'a' and 'b', causing false TS2345 when the covariant
            // result ('a') fails to satisfy upper bound 'b'.
            let mut placeholder_visited = FxHashSet::default();
            if self.type_contains_placeholder(target_param, var_map, &mut placeholder_visited) {
                let was_contra = ctx.in_contra_mode;
                ctx.in_contra_mode = true;
                self.constrain_types(ctx, var_map, source_param, target_param, priority);
                self.constrain_types(ctx, var_map, target_param, source_param, priority);
                ctx.in_contra_mode = was_contra;
            } else {
                self.constrain_types(ctx, var_map, target_param, source_param, priority);
            }
        }
    }

    /// Constrain each element type against the string and number index signatures
    /// of a target object shape. Used for Array→Object and Tuple→Object inference.
    pub(super) fn constrain_elements_against_index_sigs(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        element_types: &[TypeId],
        target_shape_id: ObjectShapeId,
        priority: crate::types::InferencePriority,
    ) {
        let t_shape = self.interner.object_shape(target_shape_id);
        // Arrays and Tuples only have number index signatures, not string index signatures.
        // Therefore, we only constrain their elements against the target's number index signature.
        let number_idx_type = t_shape.number_index.as_ref().map(|idx| idx.value_type);
        for &elem in element_types {
            if let Some(number_target) = number_idx_type {
                self.constrain_types(ctx, var_map, elem, number_target, priority);
            }
        }
    }

    pub(super) fn constrain_tuple_types(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
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

    /// Check if an evaluated type looks like an iterable object (has `[Symbol.iterator]`).
    /// Used during constraint collection to detect when an Application target evaluates
    /// to an Iterable-like interface, so Array/Tuple source element types can be
    /// constrained against the Application's type arguments.
    pub(super) fn is_iterable_like_evaluated_object(&self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                // Has number index → array-like (ArrayLike<T>, ReadonlyArray<T>)
                if shape.number_index.is_some() {
                    return true;
                }
                // Has Symbol.iterator property → iterable (Iterable<T>)
                for prop in &shape.properties {
                    let name = self.interner.resolve_atom(prop.name);
                    if name == "__@iterator" || name == "[Symbol.iterator]" {
                        return true;
                    }
                }
                false
            }
            Some(TypeData::Intersection(members)) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&m| self.is_iterable_like_evaluated_object(m))
            }
            _ => false,
        }
    }

    /// Check if a source type matches any of the given fixed target members.
    /// Used for union subtraction during inference: source members matching
    /// fixed (non-placeholder) target members are filtered out.
    pub(super) fn source_matches_any_fixed(&mut self, src: TypeId, fixed_targets: &[TypeId]) -> bool {
        for &fixed in fixed_targets {
            if fixed == src {
                return true;
            }
            let evaluated = self.checker.evaluate_type(fixed);
            if evaluated != fixed {
                if evaluated == src {
                    return true;
                }
                if let Some(TypeData::Union(inner_members)) = self.interner.lookup(evaluated) {
                    let inner = self.interner.type_list(inner_members);
                    if inner.contains(&src) {
                        return true;
                    }
                }
            }
        }
        false
    }
}
