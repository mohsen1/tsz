//! Argument checking, parameter analysis, and inference helpers for `CallEvaluator`.
//!
//! This module contains the argument-matching utilities used during function call
//! resolution and generic inference:
//! - Parameter/argument type checking (`check_argument_types`)
//! - Argument count bounds and rest parameter expansion
//! - Tuple rest pattern handling (`expand_tuple_rest`, `tuple_rest_element_type`)
//! - Placeholder/inference variable detection (`type_contains_placeholder`)
//! - Contextual sensitivity analysis (`is_contextually_sensitive`)

use super::{AssignabilityChecker, CallEvaluator, CallResult};
use crate::types::{ParamInfo, TemplateSpan, TupleElement, TypeData, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::trace;

pub(super) struct TupleRestExpansion {
    /// Fixed elements before the variadic portion (prefix)
    pub fixed: Vec<TupleElement>,
    /// The variadic element type (e.g., T for ...T[])
    pub variadic: Option<TypeId>,
    /// Fixed elements after the variadic portion (suffix/tail)
    pub tail: Vec<TupleElement>,
}

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Expand a `TypeParameter` to its constraint (if it has one).
    /// This is used when a `TypeParameter` from an outer scope is used as an argument.
    pub(super) fn expand_type_param(&self, ty: TypeId) -> TypeId {
        match self.interner.lookup(ty) {
            Some(TypeData::TypeParameter(tp)) => tp.constraint.unwrap_or(ty),
            _ => ty,
        }
    }

    pub(super) fn check_argument_types(
        &mut self,
        params: &[ParamInfo],
        arg_types: &[TypeId],
        allow_bivariant_callbacks: bool,
    ) -> Option<CallResult> {
        self.check_argument_types_with(params, arg_types, false, allow_bivariant_callbacks)
    }

    pub(crate) fn check_argument_types_with(
        &mut self,
        params: &[ParamInfo],
        arg_types: &[TypeId],
        strict: bool,
        allow_bivariant_callbacks: bool,
    ) -> Option<CallResult> {
        let arg_count = arg_types.len();
        for (i, arg_type) in arg_types.iter().enumerate() {
            let Some(param_type) = self.param_type_for_arg_index(params, i, arg_count) else {
                break;
            };

            if *arg_type == param_type {
                continue;
            }

            // Expand TypeParameters to their constraints for assignability checking when the
            // *parameter* expects a concrete type (e.g. `object`) but the argument is an outer
            // type parameter with a compatible constraint.
            //
            // IMPORTANT: Do **not** expand when the parameter type is itself a type parameter;
            // otherwise a call like `freeze(obj)` where `obj: T extends object` can incorrectly
            // compare `object` (expanded) against `T` and fail, even though inference would (and
            // tsc does) infer the inner `T` to the outer `T`.
            let expanded_arg_type = match self.interner.lookup(param_type) {
                Some(TypeData::TypeParameter(_) | TypeData::Infer(_)) => *arg_type,
                _ => self.expand_type_param(*arg_type),
            };

            if expanded_arg_type == param_type {
                continue;
            }

            let assignable = if allow_bivariant_callbacks || self.force_bivariant_callbacks {
                self.checker
                    .is_assignable_to_bivariant_callback(expanded_arg_type, param_type)
            } else if strict {
                let result = self
                    .checker
                    .is_assignable_to_strict(expanded_arg_type, param_type);
                if !result {
                    tracing::debug!(
                        "Strict assignability failed at index {}: {:?} <: {:?}",
                        i,
                        self.interner.lookup(expanded_arg_type),
                        self.interner.lookup(param_type)
                    );
                }
                result
            } else {
                self.checker.is_assignable_to(expanded_arg_type, param_type)
            };

            if !assignable {
                return Some(CallResult::ArgumentTypeMismatch {
                    index: i,
                    expected: param_type,
                    actual: *arg_type,
                });
            }
        }
        None
    }

    pub(crate) fn arg_count_bounds(&self, params: &[ParamInfo]) -> (usize, Option<usize>) {
        let required = params.iter().filter(|p| !p.optional && !p.rest).count();
        let rest_param = params.last().filter(|param| param.rest);
        let Some(rest_param) = rest_param else {
            return (required, Some(params.len()));
        };

        let rest_param_type = self.unwrap_readonly(rest_param.type_id);
        match self.interner.lookup(rest_param_type) {
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                let (rest_min, rest_max) = self.tuple_length_bounds(&elements);
                let min = required + rest_min;
                let max = rest_max.map(|max| required + max);
                (min, max)
            }
            _ => (required, None),
        }
    }

    pub(crate) fn param_type_for_arg_index(
        &self,
        params: &[ParamInfo],
        arg_index: usize,
        arg_count: usize,
    ) -> Option<TypeId> {
        let rest_param = params.last().filter(|param| param.rest);
        let rest_start = if rest_param.is_some() {
            params.len().saturating_sub(1)
        } else {
            params.len()
        };

        if arg_index < rest_start {
            return Some(params[arg_index].type_id);
        }

        let rest_param = rest_param?;
        let offset = arg_index - rest_start;
        let rest_arg_count = arg_count.saturating_sub(rest_start);

        let rest_param_type = self.unwrap_readonly(rest_param.type_id);
        trace!(
            rest_param_type_id = %rest_param_type.0,
            rest_param_type_key = ?self.interner.lookup(rest_param_type),
            "Extracting element type from rest parameter"
        );
        match self.interner.lookup(rest_param_type) {
            Some(TypeData::Array(elem)) => {
                trace!(
                    elem_type_id = %elem.0,
                    elem_type_key = ?self.interner.lookup(elem),
                    "Extracted array element type"
                );
                Some(elem)
            }
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                self.tuple_rest_element_type(&elements, offset, rest_arg_count)
            }
            other => {
                trace!(?other, "Rest param is not Array or Tuple, returning as-is");
                Some(rest_param_type)
            }
        }
    }

    fn tuple_length_bounds(&self, elements: &[TupleElement]) -> (usize, Option<usize>) {
        let mut min = 0usize;
        let mut max = 0usize;
        let mut variadic = false;

        for elem in elements {
            if elem.rest {
                let expansion = self.expand_tuple_rest(elem.type_id);
                for fixed in expansion.fixed {
                    max += 1;
                    if !fixed.optional {
                        min += 1;
                    }
                }
                if expansion.variadic.is_some() {
                    variadic = true;
                }
                // Count tail elements from nested tuple spreads
                for tail_elem in expansion.tail {
                    max += 1;
                    if !tail_elem.optional {
                        min += 1;
                    }
                }
                continue;
            }
            max += 1;
            if !elem.optional {
                min += 1;
            }
        }

        (min, if variadic { None } else { Some(max) })
    }

    fn tuple_rest_element_type(
        &self,
        elements: &[TupleElement],
        offset: usize,
        rest_arg_count: usize,
    ) -> Option<TypeId> {
        let rest_index = elements.iter().position(|elem| elem.rest);
        let Some(rest_index) = rest_index else {
            return elements.get(offset).map(|elem| elem.type_id);
        };

        let (prefix, rest_and_tail) = elements.split_at(rest_index);
        let rest_elem = &rest_and_tail[0];
        let outer_tail = &rest_and_tail[1..];

        let expansion = self.expand_tuple_rest(rest_elem.type_id);
        let prefix_len = prefix.len();
        let rest_fixed_len = expansion.fixed.len();
        let expansion_tail_len = expansion.tail.len();
        let outer_tail_len = outer_tail.len();
        // Total suffix = expansion.tail + outer_tail
        let total_suffix_len = expansion_tail_len + outer_tail_len;

        if let Some(variadic) = expansion.variadic {
            let suffix_start = rest_arg_count.saturating_sub(total_suffix_len);
            if offset >= suffix_start {
                let suffix_index = offset - suffix_start;
                // First check expansion.tail, then outer_tail
                if suffix_index < expansion_tail_len {
                    return Some(expansion.tail[suffix_index].type_id);
                }
                let outer_index = suffix_index - expansion_tail_len;
                return outer_tail.get(outer_index).map(|elem| elem.type_id);
            }
            if offset < prefix_len {
                return Some(prefix[offset].type_id);
            }
            let fixed_end = prefix_len + rest_fixed_len;
            if offset < fixed_end {
                return Some(expansion.fixed[offset - prefix_len].type_id);
            }
            return Some(variadic);
        }

        // No variadic: prefix + expansion.fixed + expansion.tail + outer_tail
        let mut index = offset;
        if index < prefix_len {
            return Some(prefix[index].type_id);
        }
        index -= prefix_len;
        if index < rest_fixed_len {
            return Some(expansion.fixed[index].type_id);
        }
        index -= rest_fixed_len;
        if index < expansion_tail_len {
            return Some(expansion.tail[index].type_id);
        }
        index -= expansion_tail_len;
        outer_tail.get(index).map(|elem| elem.type_id)
    }

    pub(crate) fn rest_element_type(&self, type_id: TypeId) -> TypeId {
        match self.interner.lookup(type_id) {
            Some(TypeData::Array(elem)) => elem,
            _ => type_id,
        }
    }

    /// Maximum iterations for type unwrapping loops to prevent infinite loops.
    const MAX_UNWRAP_ITERATIONS: usize = 1000;

    pub(super) fn unwrap_readonly(&self, mut type_id: TypeId) -> TypeId {
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > Self::MAX_UNWRAP_ITERATIONS {
                // Safety limit reached - return current type to prevent infinite loop
                return type_id;
            }
            match self.interner.lookup(type_id) {
                Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                    type_id = inner;
                }
                _ => return type_id,
            }
        }
    }

    pub(super) fn expand_tuple_rest(&self, type_id: TypeId) -> TupleRestExpansion {
        match self.interner.lookup(type_id) {
            Some(TypeData::Array(elem)) => TupleRestExpansion {
                fixed: Vec::new(),
                variadic: Some(elem),
                tail: Vec::new(),
            },
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                let mut fixed = Vec::new();
                for (i, elem) in elements.iter().enumerate() {
                    if elem.rest {
                        let inner = self.expand_tuple_rest(elem.type_id);
                        fixed.extend(inner.fixed);
                        // Capture tail elements: inner.tail + elements after the rest
                        let mut tail = inner.tail;
                        tail.extend(elements[i + 1..].iter().cloned());
                        return TupleRestExpansion {
                            fixed,
                            variadic: inner.variadic,
                            tail,
                        };
                    }
                    fixed.push(elem.clone());
                }
                TupleRestExpansion {
                    fixed,
                    variadic: None,
                    tail: Vec::new(),
                }
            }
            _ => TupleRestExpansion {
                fixed: Vec::new(),
                variadic: Some(type_id),
                tail: Vec::new(),
            },
        }
    }

    pub(crate) fn rest_tuple_inference_target(
        &mut self,
        params: &[ParamInfo],
        arg_types: &[TypeId],
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
    ) -> Option<(usize, TypeId, TypeId)> {
        let rest_param = params.last().filter(|param| param.rest)?;
        let rest_start = params.len().saturating_sub(1);

        let rest_param_type = self.unwrap_readonly(rest_param.type_id);
        let target = match self.interner.lookup(rest_param_type) {
            Some(TypeData::TypeParameter(_)) if var_map.contains_key(&rest_param_type) => {
                Some((rest_start, rest_param_type, 0))
            }
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                elements.iter().enumerate().find_map(|(i, elem)| {
                    if !elem.rest {
                        return None;
                    }
                    if !var_map.contains_key(&elem.type_id) {
                        return None;
                    }

                    // Count trailing elements after the variadic part, but allow optional
                    // tail elements to be omitted when they don't match.
                    let tail = &elements[i + 1..];
                    let min_index = rest_start + i;
                    let mut trailing_count = 0usize;
                    let mut arg_index = arg_types.len();
                    for tail_elem in tail.iter().rev() {
                        if arg_index <= min_index {
                            break;
                        }
                        let arg_type = arg_types[arg_index - 1];
                        let assignable = self.checker.is_assignable_to(arg_type, tail_elem.type_id);
                        if tail_elem.optional && !assignable {
                            break;
                        }
                        trailing_count += 1;
                        arg_index -= 1;
                    }
                    Some((rest_start + i, elem.type_id, trailing_count))
                })
            }
            _ => None,
        }?;

        let (start_index, target_type, trailing_count) = target;
        if start_index >= arg_types.len() {
            return None;
        }

        // Extract the arguments that should be inferred for the variadic type parameter,
        // excluding both prefix fixed elements and trailing fixed elements.
        // For example, for `...args: [number, ...T, boolean]` with call `foo(1, 'a', 'b', true)`:
        //   - rest_start = 0 (rest param index)
        //   - start_index = 1 (after the prefix `number`)
        //   - trailing_count = 1 (the trailing `boolean`)
        //   - we should infer T from ['a', 'b'], not [1, 'a', 'b', true]
        //
        // The variadic arguments start at start_index and end before trailing elements.
        let end_index = arg_types.len().saturating_sub(trailing_count);
        let tuple_elements: Vec<TupleElement> = if start_index < end_index {
            arg_types[start_index..end_index]
                .iter()
                .map(|&ty| TupleElement {
                    type_id: ty,
                    name: None,
                    optional: false,
                    rest: false,
                })
                .collect()
        } else {
            Vec::new()
        };
        Some((
            start_index,
            target_type,
            self.interner.tuple(tuple_elements),
        ))
    }

    pub(crate) fn type_contains_placeholder(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if var_map.contains_key(&ty) {
            return true;
        }
        if !visited.insert(ty) {
            return false;
        }

        let key = match self.interner.lookup(ty) {
            Some(key) => key,
            None => return false,
        };

        match key {
            TypeData::Array(elem) => self.type_contains_placeholder(elem, var_map, visited),
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|elem| self.type_contains_placeholder(elem.type_id, var_map, visited))
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&member| self.type_contains_placeholder(member, var_map, visited))
            }
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_placeholder(prop.type_id, var_map, visited))
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_placeholder(prop.type_id, var_map, visited))
                    || shape.string_index.as_ref().is_some_and(|idx| {
                        self.type_contains_placeholder(idx.key_type, var_map, visited)
                            || self.type_contains_placeholder(idx.value_type, var_map, visited)
                    })
                    || shape.number_index.as_ref().is_some_and(|idx| {
                        self.type_contains_placeholder(idx.key_type, var_map, visited)
                            || self.type_contains_placeholder(idx.value_type, var_map, visited)
                    })
            }
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.type_contains_placeholder(app.base, var_map, visited)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.type_contains_placeholder(arg, var_map, visited))
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                shape.type_params.iter().any(|tp| {
                    tp.constraint.is_some_and(|constraint| {
                        self.type_contains_placeholder(constraint, var_map, visited)
                    }) || tp.default.is_some_and(|default| {
                        self.type_contains_placeholder(default, var_map, visited)
                    })
                }) || shape
                    .params
                    .iter()
                    .any(|param| self.type_contains_placeholder(param.type_id, var_map, visited))
                    || shape.this_type.is_some_and(|this_type| {
                        self.type_contains_placeholder(this_type, var_map, visited)
                    })
                    || self.type_contains_placeholder(shape.return_type, var_map, visited)
                    || shape.type_predicate.as_ref().is_some_and(|pred| {
                        pred.type_id
                            .is_some_and(|ty| self.type_contains_placeholder(ty, var_map, visited))
                    })
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                let in_call = shape.call_signatures.iter().any(|sig| {
                    sig.type_params.iter().any(|tp| {
                        tp.constraint.is_some_and(|constraint| {
                            self.type_contains_placeholder(constraint, var_map, visited)
                        }) || tp.default.is_some_and(|default| {
                            self.type_contains_placeholder(default, var_map, visited)
                        })
                    }) || sig.params.iter().any(|param| {
                        self.type_contains_placeholder(param.type_id, var_map, visited)
                    }) || sig.this_type.is_some_and(|this_type| {
                        self.type_contains_placeholder(this_type, var_map, visited)
                    }) || self.type_contains_placeholder(sig.return_type, var_map, visited)
                        || sig.type_predicate.as_ref().is_some_and(|pred| {
                            pred.type_id.is_some_and(|ty| {
                                self.type_contains_placeholder(ty, var_map, visited)
                            })
                        })
                });
                if in_call {
                    return true;
                }
                let in_construct = shape.construct_signatures.iter().any(|sig| {
                    sig.type_params.iter().any(|tp| {
                        tp.constraint.is_some_and(|constraint| {
                            self.type_contains_placeholder(constraint, var_map, visited)
                        }) || tp.default.is_some_and(|default| {
                            self.type_contains_placeholder(default, var_map, visited)
                        })
                    }) || sig.params.iter().any(|param| {
                        self.type_contains_placeholder(param.type_id, var_map, visited)
                    }) || sig.this_type.is_some_and(|this_type| {
                        self.type_contains_placeholder(this_type, var_map, visited)
                    }) || self.type_contains_placeholder(sig.return_type, var_map, visited)
                        || sig.type_predicate.as_ref().is_some_and(|pred| {
                            pred.type_id.is_some_and(|ty| {
                                self.type_contains_placeholder(ty, var_map, visited)
                            })
                        })
                });
                if in_construct {
                    return true;
                }
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_placeholder(prop.type_id, var_map, visited))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                self.type_contains_placeholder(cond.check_type, var_map, visited)
                    || self.type_contains_placeholder(cond.extends_type, var_map, visited)
                    || self.type_contains_placeholder(cond.true_type, var_map, visited)
                    || self.type_contains_placeholder(cond.false_type, var_map, visited)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
                mapped.type_param.constraint.is_some_and(|constraint| {
                    self.type_contains_placeholder(constraint, var_map, visited)
                }) || mapped.type_param.default.is_some_and(|default| {
                    self.type_contains_placeholder(default, var_map, visited)
                }) || self.type_contains_placeholder(mapped.constraint, var_map, visited)
                    || self.type_contains_placeholder(mapped.template, var_map, visited)
            }
            TypeData::IndexAccess(obj, idx) => {
                self.type_contains_placeholder(obj, var_map, visited)
                    || self.type_contains_placeholder(idx, var_map, visited)
            }
            TypeData::KeyOf(operand)
            | TypeData::ReadonlyType(operand)
            | TypeData::NoInfer(operand) => {
                self.type_contains_placeholder(operand, var_map, visited)
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => {
                        self.type_contains_placeholder(*inner, var_map, visited)
                    }
                })
            }
            TypeData::StringIntrinsic { type_arg, .. } => {
                self.type_contains_placeholder(type_arg, var_map, visited)
            }
            TypeData::Enum(_def_id, member_type) => {
                self.type_contains_placeholder(member_type, var_map, visited)
            }
            TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::Error => false,
        }
    }

    /// Check if a type is contextually sensitive (requires contextual typing for inference).
    ///
    /// Contextually sensitive types include:
    /// - Function types (lambda expressions)
    /// - Callable types (object with call signatures)
    /// - Union/Intersection types containing contextually sensitive members
    /// - Object literals with callable properties (methods)
    ///
    /// These types need deferred inference in Round 2 after non-contextual
    /// arguments have been processed and type variables have been fixed.
    pub(crate) fn is_contextually_sensitive(&self, type_id: TypeId) -> bool {
        let key = match self.interner.lookup(type_id) {
            Some(key) => key,
            None => return false,
        };

        match key {
            // Function and callable types are contextually sensitive (lambdas or objects
            // with call signatures) ONLY if they have parameters. Parameterless functions
            // do not need contextual typing for arguments and should participate in Round 1
            // inference (e.g. `foo(() => 'hi')` should infer string immediately).
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                !shape.params.is_empty()
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                    .any(|sig| !sig.params.is_empty())
            }

            // Union/Intersection: contextually sensitive if any member is
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&member| self.is_contextually_sensitive(member))
            }

            // Object types: check if any property is callable (has methods)
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.is_contextually_sensitive(prop.type_id))
            }

            // Array types: check element type
            TypeData::Array(elem) => self.is_contextually_sensitive(elem),

            // Tuple types: check all elements
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|elem| self.is_contextually_sensitive(elem.type_id))
            }

            // Type applications: check base and arguments
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.is_contextually_sensitive(app.base)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.is_contextually_sensitive(arg))
            }

            // Readonly types: look through to inner type
            TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.is_contextually_sensitive(inner)
            }

            // Type parameters with constraints: check constraint
            TypeData::TypeParameter(info) | TypeData::Infer(info) => info
                .constraint
                .is_some_and(|constraint| self.is_contextually_sensitive(constraint)),

            // Index access: check both object and key types
            TypeData::IndexAccess(obj, key) => {
                self.is_contextually_sensitive(obj) || self.is_contextually_sensitive(key)
            }

            // Conditional types: check all branches
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                self.is_contextually_sensitive(cond.check_type)
                    || self.is_contextually_sensitive(cond.extends_type)
                    || self.is_contextually_sensitive(cond.true_type)
                    || self.is_contextually_sensitive(cond.false_type)
            }

            // Mapped types: check constraint and template
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
                self.is_contextually_sensitive(mapped.constraint)
                    || self.is_contextually_sensitive(mapped.template)
            }

            // KeyOf, StringIntrinsic: check operand
            TypeData::KeyOf(operand)
            | TypeData::StringIntrinsic {
                type_arg: operand, ..
            } => self.is_contextually_sensitive(operand),

            // Enum types: check member type
            TypeData::Enum(_def_id, member_type) => self.is_contextually_sensitive(member_type),

            // Template literals: check type spans
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => self.is_contextually_sensitive(*inner),
                })
            }

            // Non-contextually sensitive types
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::Error => false,
        }
    }
}
