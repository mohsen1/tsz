//! `Function` / `Callable` instantiation: the `TypeData::Function` and
//! `TypeData::Callable` arms of `instantiate_key`, plus the call-signature
//! instantiation helpers they share.

use crate::types::{
    CallSignature, CallableShape, CallableShapeId, FunctionShape, FunctionShapeId, TypeData, TypeId,
};

use super::TypeInstantiator;

impl<'a> TypeInstantiator<'a> {
    /// Instantiate a call signature.
    fn instantiate_call_signature_if_changed(
        &mut self,
        sig: &CallSignature,
    ) -> Option<CallSignature> {
        let (shadowed_len, saved_visiting) = self.enter_shadowing_scope(&sig.type_params);

        let type_params = self.instantiate_type_params_if_changed(&sig.type_params);
        let local_start = self.local_type_params.len();
        // Redirect occurrences of the signature's own params only when their
        // infos actually changed (constraint/default instantiated). When
        // unchanged, pushing a structural re-intern would rewrite
        // declaration-scoped fresh params to the structural canonical and
        // erase declaration identity; the shadowing scope already preserves
        // them as-is (#13044).
        if let Some(changed_params) = type_params.as_deref() {
            for type_param in changed_params {
                self.local_type_params
                    .push((type_param.name, self.interner.type_param(*type_param)));
            }
        }
        let type_predicate = sig
            .type_predicate
            .as_ref()
            .and_then(|predicate| self.instantiate_type_predicate_if_changed(predicate));
        let this_type = sig.this_type.map(|type_id| self.instantiate(type_id));
        let params = self.instantiate_params_if_changed(&sig.params);
        let return_type = self.instantiate(sig.return_type);
        self.local_type_params.truncate(local_start);

        self.exit_shadowing_scope(shadowed_len, saved_visiting);

        let this_changed = this_type != sig.this_type;
        let return_changed = return_type != sig.return_type;
        if type_params.is_none()
            && params.is_none()
            && type_predicate.is_none()
            && !this_changed
            && !return_changed
        {
            return None;
        }

        Some(CallSignature {
            type_params: type_params.unwrap_or_else(|| sig.type_params.clone()),
            params: params.unwrap_or_else(|| sig.params.clone()),
            this_type,
            return_type,
            type_predicate: type_predicate.or(sig.type_predicate),
            is_method: sig.is_method,
        })
    }

    fn instantiate_call_signatures_if_changed(
        &mut self,
        signatures: &[CallSignature],
    ) -> Option<Vec<CallSignature>> {
        let mut instantiated: Option<Vec<CallSignature>> = None;
        for (index, signature) in signatures.iter().enumerate() {
            let signature = self.instantiate_call_signature_if_changed(signature);
            if let Some(instantiated) = &mut instantiated {
                instantiated.push(signature.unwrap_or_else(|| signatures[index].clone()));
            } else if let Some(signature) = signature {
                let mut changed = Vec::with_capacity(signatures.len());
                changed.extend_from_slice(&signatures[..index]);
                changed.push(signature);
                instantiated = Some(changed);
            }
        }
        instantiated
    }

    /// Instantiate a function: instantiate params and return type.
    ///
    /// Note: Type params in the function create a new scope - don't substitute those
    pub(super) fn instantiate_function(
        &mut self,
        shape_id: &FunctionShapeId,
        key: &TypeData,
    ) -> TypeId {
        let shape = self.interner.function_shape(*shape_id);
        // Shallow-this mode: substitute `this:` parameter slot,
        // and substitute params/return_type only when they ARE the
        // top-level `ThisType` (no nesting). Don't recurse into
        // composite types like `this & T` — those carry the
        // polymorphic `this` scope that must stay raw for
        // intersection rebinding (chained `extend({a}).extend({b})`
        // pattern). Top-level `this` substitution is needed for
        // ordinary `(p: this) => this` shapes.
        if self.shallow_this_only {
            let target_this = self.this_type.unwrap_or(TypeId::ERROR);
            let sub_top_level = |id: TypeId| -> TypeId {
                if matches!(self.interner.lookup(id), Some(TypeData::ThisType)) {
                    target_this
                } else {
                    id
                }
            };
            let new_this_slot = shape.this_type.map(sub_top_level);
            let new_return_type = sub_top_level(shape.return_type);
            let mut new_params: Option<Vec<_>> = None;
            for (index, p) in shape.params.iter().enumerate() {
                let new_t = sub_top_level(p.type_id);
                if let Some(new_params) = &mut new_params {
                    let mut np = *p;
                    np.type_id = new_t;
                    new_params.push(np);
                } else if new_t != p.type_id {
                    let mut changed = Vec::with_capacity(shape.params.len());
                    changed.extend_from_slice(&shape.params[..index]);
                    let mut np = *p;
                    np.type_id = new_t;
                    changed.push(np);
                    new_params = Some(changed);
                } else {
                    // Leave the unchanged prefix borrowed until the first
                    // changed slot proves a replacement vector is needed.
                }
            }
            let this_changed = match (shape.this_type, new_this_slot) {
                (Some(a), Some(b)) => a != b,
                (None, None) => false,
                _ => true,
            };
            if new_params.is_some() || this_changed || new_return_type != shape.return_type {
                return self.interner.function(FunctionShape {
                    type_params: shape.type_params.clone(),
                    params: new_params.unwrap_or_else(|| shape.params.clone()),
                    this_type: new_this_slot,
                    return_type: new_return_type,
                    type_predicate: shape.type_predicate,
                    is_constructor: shape.is_constructor,
                    is_method: shape.is_method,
                });
            }
            return self.interner.intern(*key);
        }
        let (shadowed_len, saved_visiting) = self.enter_shadowing_scope(&shape.type_params);

        let instantiated_type_params = self.instantiate_type_params_if_changed(&shape.type_params);
        let local_start = self.local_type_params.len();
        // Redirect own-param occurrences only when the param infos
        // changed; see `instantiate_call_signature_if_changed` for
        // the declaration-identity rationale (#13044).
        if let Some(changed_params) = instantiated_type_params.as_deref() {
            for type_param in changed_params {
                self.local_type_params
                    .push((type_param.name, self.interner.type_param(*type_param)));
            }
        }
        let type_predicate = shape
            .type_predicate
            .as_ref()
            .and_then(|predicate| self.instantiate_type_predicate_if_changed(predicate));
        let this_type = shape.this_type.map(|type_id| self.instantiate(type_id));
        let instantiated_params = self.instantiate_params_if_changed(&shape.params);
        let instantiated_return = self.instantiate(shape.return_type);
        self.local_type_params.truncate(local_start);

        self.exit_shadowing_scope(shadowed_len, saved_visiting);

        let this_changed = this_type != shape.this_type;
        let return_changed = instantiated_return != shape.return_type;
        if instantiated_type_params.is_some()
            || instantiated_params.is_some()
            || type_predicate.is_some()
            || this_changed
            || return_changed
        {
            self.interner.function(FunctionShape {
                type_params: instantiated_type_params.unwrap_or_else(|| shape.type_params.clone()),
                params: instantiated_params.unwrap_or_else(|| shape.params.clone()),
                this_type,
                return_type: instantiated_return,
                type_predicate: type_predicate.or(shape.type_predicate),
                is_constructor: shape.is_constructor,
                is_method: shape.is_method,
            })
        } else {
            self.interner.intern(*key)
        }
    }

    /// Instantiate a callable: instantiate all signatures and properties.
    pub(super) fn instantiate_callable(
        &mut self,
        shape_id: &CallableShapeId,
        key: &TypeData,
    ) -> TypeId {
        let shape = self.interner.callable_shape(*shape_id);
        // Shallow-this mode: substitute the `this:` slot and
        // top-level `ThisType` references in params / return_type;
        // leave deeper composite types alone so polymorphic `this`
        // in method bodies stays raw for intersection rebinding.
        if self.shallow_this_only {
            let target_this = self.this_type.unwrap_or(TypeId::ERROR);
            let sub_top_level = |id: TypeId| -> TypeId {
                if matches!(self.interner.lookup(id), Some(TypeData::ThisType)) {
                    target_this
                } else {
                    id
                }
            };
            let rewrite_sig = |sig: &CallSignature| -> Option<CallSignature> {
                let new_this_slot = sig.this_type.map(|s| {
                    let n = sub_top_level(s);
                    n
                });
                let new_return = sub_top_level(sig.return_type);
                let mut new_params: Option<Vec<_>> = None;
                for (index, p) in sig.params.iter().enumerate() {
                    let new_t = sub_top_level(p.type_id);
                    if let Some(new_params) = &mut new_params {
                        let mut np = *p;
                        np.type_id = new_t;
                        new_params.push(np);
                    } else if new_t != p.type_id {
                        let mut changed = Vec::with_capacity(sig.params.len());
                        changed.extend_from_slice(&sig.params[..index]);
                        let mut np = *p;
                        np.type_id = new_t;
                        changed.push(np);
                        new_params = Some(changed);
                    } else {
                        // Leave the unchanged prefix borrowed until a later
                        // slot changes.
                    }
                }
                let this_changed = new_this_slot != sig.this_type;
                let return_changed = new_return != sig.return_type;
                if new_params.is_none() && !this_changed && !return_changed {
                    return None;
                }
                Some(CallSignature {
                    type_params: sig.type_params.clone(),
                    params: new_params.unwrap_or_else(|| sig.params.clone()),
                    this_type: new_this_slot,
                    return_type: new_return,
                    type_predicate: sig.type_predicate,
                    is_method: sig.is_method,
                })
            };

            let rewrite_signatures = |signatures: &[CallSignature]| -> Option<Vec<CallSignature>> {
                let mut updated: Option<Vec<CallSignature>> = None;
                for (index, signature) in signatures.iter().enumerate() {
                    let signature = rewrite_sig(signature);
                    if let Some(updated) = &mut updated {
                        updated.push(signature.unwrap_or_else(|| signatures[index].clone()));
                    } else if let Some(signature) = signature {
                        let mut changed = Vec::with_capacity(signatures.len());
                        changed.extend_from_slice(&signatures[..index]);
                        changed.push(signature);
                        updated = Some(changed);
                    }
                }
                updated
            };

            let updated_call = rewrite_signatures(&shape.call_signatures);
            let updated_construct = rewrite_signatures(&shape.construct_signatures);
            if updated_call.is_some() || updated_construct.is_some() {
                return self.interner.callable(CallableShape {
                    call_signatures: updated_call.unwrap_or_else(|| shape.call_signatures.clone()),
                    construct_signatures: updated_construct
                        .unwrap_or_else(|| shape.construct_signatures.clone()),
                    properties: shape.properties.clone(),
                    string_index: shape.string_index,
                    number_index: shape.number_index,
                    symbol: shape.symbol,
                    is_abstract: shape.is_abstract,
                });
            }
            return self.interner.intern(*key);
        }
        let instantiated_call = self.instantiate_call_signatures_if_changed(&shape.call_signatures);
        let instantiated_construct =
            self.instantiate_call_signatures_if_changed(&shape.construct_signatures);
        let instantiated_props = self.instantiate_properties_if_changed(&shape.properties);
        let instantiated_string_idx = shape
            .string_index
            .as_ref()
            .and_then(|idx| self.instantiate_index_signature_if_changed(idx));
        let instantiated_number_idx = shape
            .number_index
            .as_ref()
            .and_then(|idx| self.instantiate_index_signature_if_changed(idx));

        if instantiated_call.is_some()
            || instantiated_construct.is_some()
            || instantiated_props.is_some()
            || instantiated_string_idx.is_some()
            || instantiated_number_idx.is_some()
        {
            self.interner.callable(CallableShape {
                call_signatures: instantiated_call.unwrap_or_else(|| shape.call_signatures.clone()),
                construct_signatures: instantiated_construct
                    .unwrap_or_else(|| shape.construct_signatures.clone()),
                properties: instantiated_props.unwrap_or_else(|| shape.properties.clone()),
                string_index: instantiated_string_idx.or(shape.string_index),
                number_index: instantiated_number_idx.or(shape.number_index),
                symbol: shape.symbol,
                is_abstract: shape.is_abstract,
            })
        } else {
            self.interner.intern(*key)
        }
    }
}
