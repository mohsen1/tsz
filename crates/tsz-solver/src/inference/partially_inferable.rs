//! Partially-inferable source type normalization for reverse mapped inference.

use super::infer::InferenceContext;
use crate::types::{CallSignature, FunctionShape, ParamInfo, TypeData, TypeId};

impl<'a> InferenceContext<'a> {
    /// Get the "partially inferable" version of a type for property inference.
    ///
    /// Matches tsc's `getPartiallyInferableType`: for function types whose
    /// parameters have type `any` (from implicit typing in method shorthands),
    /// replace those `any` parameters with `unknown`. This prevents implicit
    /// `any` from flowing contravariantly into inference candidates, which
    /// would incorrectly produce `T = any` instead of `T = unknown` when
    /// inference has no other information.
    ///
    /// This is critical for reverse-mapped type inference where callback
    /// parameters depend on the type being inferred. Without this, patterns
    /// like `{ contains(k) { ... } }` matched against `{ [K in keyof T]: Box<T[K]> }`
    /// would infer `T[K] = any` instead of `T[K] = unknown`.
    pub(in crate::inference) fn get_partially_inferable_type(&self, type_id: TypeId) -> TypeId {
        // Intrinsics are never Function/Object/Tuple; return as-is.
        if type_id.is_intrinsic() {
            return type_id;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                // Only transform if the function has any `any`-typed parameters.
                // This indicates the parameters are implicitly typed (from method
                // shorthand or untyped callback params). Explicitly typed `any`
                // params would have the same effect but are rare enough that the
                // slightly conservative behavior is acceptable.
                let has_any_param = shape.params.iter().any(|p| p.type_id == TypeId::ANY);
                if !has_any_param {
                    return type_id;
                }
                let new_params: Vec<ParamInfo> = shape
                    .params
                    .iter()
                    .map(|p| {
                        if p.type_id == TypeId::ANY {
                            ParamInfo {
                                suppress_display_optional: false,
                                type_id: TypeId::UNKNOWN,
                                ..*p
                            }
                        } else {
                            *p
                        }
                    })
                    .collect();
                let new_shape = FunctionShape {
                    params: new_params,
                    ..(*shape).clone()
                };
                self.interner.function(new_shape)
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.interner.callable_shape(shape_id);
                let has_any_param = shape
                    .call_signatures
                    .iter()
                    .any(|sig| sig.params.iter().any(|p| p.type_id == TypeId::ANY));
                if !has_any_param {
                    return type_id;
                }
                let new_sigs: Vec<_> = shape
                    .call_signatures
                    .iter()
                    .map(|sig| {
                        let new_params: Vec<ParamInfo> = sig
                            .params
                            .iter()
                            .map(|p| {
                                if p.type_id == TypeId::ANY {
                                    ParamInfo {
                                        suppress_display_optional: false,
                                        type_id: TypeId::UNKNOWN,
                                        ..*p
                                    }
                                } else {
                                    *p
                                }
                            })
                            .collect();
                        CallSignature {
                            params: new_params,
                            ..sig.clone()
                        }
                    })
                    .collect();
                let mut new_shape = (*shape).clone();
                new_shape.call_signatures = new_sigs;
                self.interner.callable(new_shape)
            }
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                // For object types, transform any function-typed properties
                // to their partially inferable versions. This handles cases
                // like `{ contains(k) {...} }` where the method is a property
                // of an object literal.
                let shape = self.interner.object_shape(shape_id);
                let has_function_with_any = shape.properties.iter().any(|p| {
                    matches!(
                        self.interner.lookup(p.type_id),
                        Some(TypeData::Function(fid)) if {
                            let fs = self.interner.function_shape(fid);
                            fs.params.iter().any(|param| param.type_id == TypeId::ANY)
                        }
                    )
                });
                if !has_function_with_any {
                    return type_id;
                }
                let new_props: Vec<_> = shape
                    .properties
                    .iter()
                    .map(|p| {
                        let new_type = self.get_partially_inferable_type(p.type_id);
                        if new_type != p.type_id {
                            let mut new_prop = p.clone();
                            new_prop.type_id = new_type;
                            new_prop
                        } else {
                            p.clone()
                        }
                    })
                    .collect();
                let mut new_shape = (*shape).clone();
                new_shape.properties = new_props;
                // Use object_with_index for both Object and ObjectWithIndex
                // since this is a temporary type only used during inference.
                self.interner.object_with_index(new_shape)
            }
            _ => type_id,
        }
    }
}
