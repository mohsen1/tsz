//! Infer binding substitution.
//!
//! Provides `InferSubstitutor` which performs deep traversal of a type,
//! replacing all `infer X` references with their bound values.

use crate::TypeDatabase;
use crate::types::{
    CallSignature, CallableShape, ConditionalType, FunctionShape, IndexSignature, ObjectShape,
    ParamInfo, PropertyInfo, TemplateSpan, TupleElement, TypeData, TypeId,
};
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;

/// Helper for substituting infer bindings into types.
///
/// This struct performs a deep traversal of a type, replacing all `infer X`
/// references with their bound values from the bindings map.
pub(crate) struct InferSubstitutor<'a> {
    interner: &'a dyn TypeDatabase,
    bindings: &'a FxHashMap<Atom, TypeId>,
    visiting: FxHashMap<TypeId, TypeId>,
}

impl<'a> InferSubstitutor<'a> {
    /// Create a new substitutor with the given interner and bindings.
    pub fn new(interner: &'a dyn TypeDatabase, bindings: &'a FxHashMap<Atom, TypeId>) -> Self {
        InferSubstitutor {
            interner,
            bindings,
            visiting: FxHashMap::default(),
        }
    }

    /// Substitute infer types in the given type, returning the result.
    pub fn substitute(&mut self, type_id: TypeId) -> TypeId {
        if let Some(&cached) = self.visiting.get(&type_id) {
            return cached;
        }

        let Some(key) = self.interner.lookup(type_id) else {
            return type_id;
        };

        self.visiting.insert(type_id, type_id);

        let result = match key {
            TypeData::Infer(info) => self.bindings.get(&info.name).copied().unwrap_or(type_id),
            TypeData::Array(elem) => {
                let substituted = self.substitute(elem);
                if substituted == elem {
                    type_id
                } else {
                    self.interner.array(substituted)
                }
            }
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                let mut changed = false;
                let mut new_elements = Vec::with_capacity(elements.len());
                for element in elements.iter() {
                    let substituted = self.substitute(element.type_id);
                    if substituted != element.type_id {
                        changed = true;
                    }
                    new_elements.push(TupleElement {
                        type_id: substituted,
                        name: element.name,
                        optional: element.optional,
                        rest: element.rest,
                    });
                }
                if changed {
                    self.interner.tuple(new_elements)
                } else {
                    type_id
                }
            }
            TypeData::Union(members) => {
                let members = self.interner.type_list(members);
                let mut changed = false;
                let mut new_members = Vec::with_capacity(members.len());
                for &member in members.iter() {
                    let substituted = self.substitute(member);
                    if substituted != member {
                        changed = true;
                    }
                    new_members.push(substituted);
                }
                if changed {
                    self.interner.union(new_members)
                } else {
                    type_id
                }
            }
            TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                let mut changed = false;
                let mut new_members = Vec::with_capacity(members.len());
                for &member in members.iter() {
                    let substituted = self.substitute(member);
                    if substituted != member {
                        changed = true;
                    }
                    new_members.push(substituted);
                }
                if changed {
                    self.interner.intersection(new_members)
                } else {
                    type_id
                }
            }
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                let mut changed = false;
                let mut properties = Vec::with_capacity(shape.properties.len());
                for prop in &shape.properties {
                    let type_id = self.substitute(prop.type_id);
                    let write_type = self.substitute(prop.write_type);
                    if type_id != prop.type_id || write_type != prop.write_type {
                        changed = true;
                    }
                    properties.push(PropertyInfo {
                        name: prop.name,
                        type_id,
                        write_type,
                        optional: prop.optional,
                        readonly: prop.readonly,
                        is_method: prop.is_method,
                        visibility: prop.visibility,
                        parent_id: prop.parent_id,
                    });
                }
                if changed {
                    self.interner.object(properties)
                } else {
                    type_id
                }
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                let mut changed = false;
                let mut properties = Vec::with_capacity(shape.properties.len());
                for prop in &shape.properties {
                    let type_id = self.substitute(prop.type_id);
                    let write_type = self.substitute(prop.write_type);
                    if type_id != prop.type_id || write_type != prop.write_type {
                        changed = true;
                    }
                    properties.push(PropertyInfo {
                        name: prop.name,
                        type_id,
                        write_type,
                        optional: prop.optional,
                        readonly: prop.readonly,
                        is_method: prop.is_method,
                        visibility: prop.visibility,
                        parent_id: prop.parent_id,
                    });
                }
                let string_index = shape.string_index.as_ref().map(|index| {
                    let key_type = self.substitute(index.key_type);
                    let value_type = self.substitute(index.value_type);
                    if key_type != index.key_type || value_type != index.value_type {
                        changed = true;
                    }
                    IndexSignature {
                        key_type,
                        value_type,
                        readonly: index.readonly,
                    }
                });
                let number_index = shape.number_index.as_ref().map(|index| {
                    let key_type = self.substitute(index.key_type);
                    let value_type = self.substitute(index.value_type);
                    if key_type != index.key_type || value_type != index.value_type {
                        changed = true;
                    }
                    IndexSignature {
                        key_type,
                        value_type,
                        readonly: index.readonly,
                    }
                });
                if changed {
                    self.interner.object_with_index(ObjectShape {
                        flags: shape.flags,
                        properties,
                        string_index,
                        number_index,
                        symbol: None,
                    })
                } else {
                    type_id
                }
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                let check_type = self.substitute(cond.check_type);
                let extends_type = self.substitute(cond.extends_type);
                let true_type = self.substitute(cond.true_type);
                let false_type = self.substitute(cond.false_type);
                if check_type == cond.check_type
                    && extends_type == cond.extends_type
                    && true_type == cond.true_type
                    && false_type == cond.false_type
                {
                    type_id
                } else {
                    self.interner.conditional(ConditionalType {
                        check_type,
                        extends_type,
                        true_type,
                        false_type,
                        is_distributive: cond.is_distributive,
                    })
                }
            }
            TypeData::IndexAccess(obj, idx) => {
                let new_obj = self.substitute(obj);
                let new_idx = self.substitute(idx);
                if new_obj == obj && new_idx == idx {
                    type_id
                } else {
                    self.interner.index_access(new_obj, new_idx)
                }
            }
            TypeData::KeyOf(inner) => {
                let new_inner = self.substitute(inner);
                if new_inner == inner {
                    type_id
                } else {
                    self.interner.keyof(new_inner)
                }
            }
            TypeData::ReadonlyType(inner) => {
                let new_inner = self.substitute(inner);
                if new_inner == inner {
                    type_id
                } else {
                    self.interner.readonly_type(new_inner)
                }
            }
            TypeData::NoInfer(inner) => {
                let new_inner = self.substitute(inner);
                if new_inner == inner {
                    type_id
                } else {
                    self.interner.no_infer(new_inner)
                }
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                let mut changed = false;
                let mut new_spans = Vec::with_capacity(spans.len());
                for span in spans.iter() {
                    let new_span = match span {
                        TemplateSpan::Text(text) => TemplateSpan::Text(*text),
                        TemplateSpan::Type(inner) => {
                            let substituted = self.substitute(*inner);
                            if substituted != *inner {
                                changed = true;
                            }
                            TemplateSpan::Type(substituted)
                        }
                    };
                    new_spans.push(new_span);
                }
                if changed {
                    self.interner.template_literal(new_spans)
                } else {
                    type_id
                }
            }
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                let base = self.substitute(app.base);
                let mut changed = base != app.base;
                let mut new_args = Vec::with_capacity(app.args.len());
                for &arg in &app.args {
                    let substituted = self.substitute(arg);
                    if substituted != arg {
                        changed = true;
                    }
                    new_args.push(substituted);
                }
                if changed {
                    self.interner.application(base, new_args)
                } else {
                    type_id
                }
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                let mut changed = false;
                let mut new_params = Vec::with_capacity(shape.params.len());
                for param in &shape.params {
                    let param_type = self.substitute(param.type_id);
                    if param_type != param.type_id {
                        changed = true;
                    }
                    new_params.push(ParamInfo {
                        name: param.name,
                        type_id: param_type,
                        optional: param.optional,
                        rest: param.rest,
                    });
                }
                let return_type = self.substitute(shape.return_type);
                if return_type != shape.return_type {
                    changed = true;
                }
                let this_type = shape.this_type.map(|t| {
                    let substituted = self.substitute(t);
                    if substituted != t {
                        changed = true;
                    }
                    substituted
                });
                if changed {
                    self.interner.function(FunctionShape {
                        params: new_params,
                        this_type,
                        return_type,
                        type_params: shape.type_params.clone(),
                        type_predicate: shape.type_predicate.clone(),
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    })
                } else {
                    type_id
                }
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                let mut changed = false;

                let call_signatures: Vec<CallSignature> = shape
                    .call_signatures
                    .iter()
                    .map(|sig| {
                        let mut new_params = Vec::with_capacity(sig.params.len());
                        for param in &sig.params {
                            let param_type = self.substitute(param.type_id);
                            if param_type != param.type_id {
                                changed = true;
                            }
                            new_params.push(ParamInfo {
                                name: param.name,
                                type_id: param_type,
                                optional: param.optional,
                                rest: param.rest,
                            });
                        }
                        let return_type = self.substitute(sig.return_type);
                        if return_type != sig.return_type {
                            changed = true;
                        }
                        let this_type = sig.this_type.map(|t| {
                            let substituted = self.substitute(t);
                            if substituted != t {
                                changed = true;
                            }
                            substituted
                        });
                        CallSignature {
                            params: new_params,
                            this_type,
                            return_type,
                            type_params: sig.type_params.clone(),
                            type_predicate: sig.type_predicate.clone(),
                            is_method: sig.is_method,
                        }
                    })
                    .collect();

                let construct_signatures: Vec<CallSignature> = shape
                    .construct_signatures
                    .iter()
                    .map(|sig| {
                        let mut new_params = Vec::with_capacity(sig.params.len());
                        for param in &sig.params {
                            let param_type = self.substitute(param.type_id);
                            if param_type != param.type_id {
                                changed = true;
                            }
                            new_params.push(ParamInfo {
                                name: param.name,
                                type_id: param_type,
                                optional: param.optional,
                                rest: param.rest,
                            });
                        }
                        let return_type = self.substitute(sig.return_type);
                        if return_type != sig.return_type {
                            changed = true;
                        }
                        let this_type = sig.this_type.map(|t| {
                            let substituted = self.substitute(t);
                            if substituted != t {
                                changed = true;
                            }
                            substituted
                        });
                        CallSignature {
                            params: new_params,
                            this_type,
                            return_type,
                            type_params: sig.type_params.clone(),
                            type_predicate: sig.type_predicate.clone(),
                            is_method: sig.is_method,
                        }
                    })
                    .collect();

                let properties: Vec<PropertyInfo> = shape
                    .properties
                    .iter()
                    .map(|prop| {
                        let prop_type = self.substitute(prop.type_id);
                        let write_type = self.substitute(prop.write_type);
                        if prop_type != prop.type_id || write_type != prop.write_type {
                            changed = true;
                        }
                        PropertyInfo {
                            name: prop.name,
                            type_id: prop_type,
                            write_type,
                            optional: prop.optional,
                            readonly: prop.readonly,
                            is_method: prop.is_method,
                            visibility: prop.visibility,
                            parent_id: prop.parent_id,
                        }
                    })
                    .collect();

                let string_index = shape.string_index.as_ref().map(|idx| {
                    let key_type = self.substitute(idx.key_type);
                    let value_type = self.substitute(idx.value_type);
                    if key_type != idx.key_type || value_type != idx.value_type {
                        changed = true;
                    }
                    IndexSignature {
                        key_type,
                        value_type,
                        readonly: idx.readonly,
                    }
                });

                let number_index = shape.number_index.as_ref().map(|idx| {
                    let key_type = self.substitute(idx.key_type);
                    let value_type = self.substitute(idx.value_type);
                    if key_type != idx.key_type || value_type != idx.value_type {
                        changed = true;
                    }
                    IndexSignature {
                        key_type,
                        value_type,
                        readonly: idx.readonly,
                    }
                });

                if changed {
                    self.interner.callable(CallableShape {
                        call_signatures,
                        construct_signatures,
                        properties,
                        string_index,
                        number_index,
                        symbol: None,
                    })
                } else {
                    type_id
                }
            }
            _ => type_id,
        };

        self.visiting.insert(type_id, result);
        result
    }
}
