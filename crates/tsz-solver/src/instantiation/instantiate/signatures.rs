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

        let local_start = self.local_type_params.len();
        // This also binds each changed local parameter before walking later
        // dependent constraints; unchanged fresh params remain untouched
        // (#13044).
        let type_params = self.instantiate_type_params_if_changed(&sig.type_params);
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
            declaration_group: sig.declaration_group,
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
        type_id: TypeId,
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
            // Function shapes are canonically interned, so `type_id` already
            // names this key; skip the redundant re-intern.
            return type_id;
        }
        let (shadowed_len, saved_visiting) = self.enter_shadowing_scope(&shape.type_params);

        let local_start = self.local_type_params.len();
        // This also binds each changed local parameter before walking later
        // dependent constraints; unchanged fresh params remain untouched.
        let instantiated_type_params = self.instantiate_type_params_if_changed(&shape.type_params);
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
            type_id
        }
    }

    /// Instantiate a callable: instantiate all signatures and properties.
    pub(super) fn instantiate_callable(
        &mut self,
        shape_id: &CallableShapeId,
        type_id: TypeId,
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
            let rewrite_sig = |sig: &CallSignature| -> (CallSignature, bool) {
                let mut changed = false;
                let new_this_slot = sig.this_type.map(|s| {
                    let n = sub_top_level(s);
                    if n != s {
                        changed = true;
                    }
                    n
                });
                let new_return = sub_top_level(sig.return_type);
                if new_return != sig.return_type {
                    changed = true;
                }
                let mut new_params = Vec::with_capacity(sig.params.len());
                for p in sig.params.iter() {
                    let new_t = sub_top_level(p.type_id);
                    if new_t != p.type_id {
                        changed = true;
                        let mut np = *p;
                        np.type_id = new_t;
                        new_params.push(np);
                    } else {
                        new_params.push(*p);
                    }
                }
                let mut new_sig = sig.clone();
                new_sig.this_type = new_this_slot;
                new_sig.return_type = new_return;
                new_sig.params = new_params;
                (new_sig, changed)
            };

            let mut updated_call = Vec::with_capacity(shape.call_signatures.len());
            let mut any_changed = false;
            for sig in shape.call_signatures.iter() {
                let (new_sig, changed) = rewrite_sig(sig);
                any_changed |= changed;
                updated_call.push(new_sig);
            }
            let mut updated_construct = Vec::with_capacity(shape.construct_signatures.len());
            for sig in shape.construct_signatures.iter() {
                let (new_sig, changed) = rewrite_sig(sig);
                any_changed |= changed;
                updated_construct.push(new_sig);
            }
            if any_changed {
                return self.interner.callable(CallableShape {
                    call_signatures: updated_call,
                    construct_signatures: updated_construct,
                    properties: shape.properties.clone(),
                    string_index: shape.string_index,
                    number_index: shape.number_index,
                    symbol: shape.symbol,
                    is_abstract: shape.is_abstract,
                });
            }
            // Callable shapes are canonically interned, so `type_id` already
            // names this key; skip the redundant re-intern.
            return type_id;
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
            type_id
        }
    }
}
