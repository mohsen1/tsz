//! Generic type instantiation and substitution.
//!
//! This module implements type parameter substitution for generic types.
//! When a generic function/type is instantiated, we replace type parameters
//! with concrete types throughout the type structure.
//!
//! Key features:
//! - Type substitution map (type parameter name -> TypeId)
//! - Deep recursive substitution through nested types
//! - Handling of constraints and defaults

use crate::interner::Atom;
use crate::solver::TypeDatabase;
use crate::solver::types::*;
use rustc_hash::FxHashMap;

#[cfg(test)]
use crate::solver::TypeInterner;

/// Maximum depth for recursive type instantiation.
pub(crate) const MAX_INSTANTIATION_DEPTH: u32 = 50;

/// A substitution map from type parameter names to concrete types.
#[derive(Clone, Debug, Default)]
pub struct TypeSubstitution {
    /// Maps type parameter names to their substituted types
    map: FxHashMap<Atom, TypeId>,
}

impl TypeSubstitution {
    /// Create an empty substitution.
    pub fn new() -> Self {
        TypeSubstitution {
            map: FxHashMap::default(),
        }
    }

    /// Create a substitution from type parameters and arguments.
    ///
    /// `type_params` - The declared type parameters (e.g., `<T, U>`)
    /// `type_args` - The provided type arguments (e.g., `<string, number>`)
    ///
    /// When type_args has fewer elements than type_params, default values
    /// from the type parameters are used for the remaining parameters.
    pub fn from_args(type_params: &[TypeParamInfo], type_args: &[TypeId]) -> Self {
        let mut map = FxHashMap::default();
        for (i, param) in type_params.iter().enumerate() {
            let type_id = if i < type_args.len() {
                type_args[i]
            } else {
                // Use default value if type argument not provided
                match param.default {
                    Some(default) => default,
                    None => {
                        // No default and no argument - leave this parameter unsubstituted
                        // It will remain as a TypeParameter in the result
                        continue;
                    }
                }
            };
            map.insert(param.name, type_id);
        }
        TypeSubstitution { map }
    }

    /// Add a single substitution.
    pub fn insert(&mut self, name: Atom, type_id: TypeId) {
        self.map.insert(name, type_id);
    }

    /// Look up a substitution.
    pub fn get(&self, name: Atom) -> Option<TypeId> {
        self.map.get(&name).copied()
    }

    /// Check if substitution is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Number of substitutions.
    pub fn len(&self) -> usize {
        self.map.len()
    }
}

/// Instantiator for applying type substitutions.
pub struct TypeInstantiator<'a> {
    interner: &'a dyn TypeDatabase,
    substitution: &'a TypeSubstitution,
    /// Track visited types to handle cycles
    visiting: FxHashMap<TypeId, TypeId>,
    /// Type parameter names that are shadowed in the current scope.
    shadowed: Vec<Atom>,
    substitute_infer: bool,
    depth: u32,
    max_depth: u32,
    depth_exceeded: bool,
}

impl<'a> TypeInstantiator<'a> {
    /// Create a new instantiator.
    pub fn new(interner: &'a dyn TypeDatabase, substitution: &'a TypeSubstitution) -> Self {
        TypeInstantiator {
            interner,
            substitution,
            visiting: FxHashMap::default(),
            shadowed: Vec::new(),
            substitute_infer: false,
            depth: 0,
            max_depth: MAX_INSTANTIATION_DEPTH,
            depth_exceeded: false,
        }
    }

    fn is_shadowed(&self, name: Atom) -> bool {
        self.shadowed.contains(&name)
    }

    /// Apply the substitution to a type, returning the instantiated type.
    pub fn instantiate(&mut self, type_id: TypeId) -> TypeId {
        // Fast path: intrinsic types don't need instantiation
        if type_id.is_intrinsic() {
            return type_id;
        }

        if self.depth_exceeded {
            return TypeId::ERROR;
        }

        if self.depth >= self.max_depth {
            self.depth_exceeded = true;
            return TypeId::ERROR;
        }

        self.depth += 1;
        let result = self.instantiate_inner(type_id);
        self.depth -= 1;
        result
    }

    fn instantiate_inner(&mut self, type_id: TypeId) -> TypeId {
        // Check if we're already processing this type (cycle detection)
        if let Some(&cached) = self.visiting.get(&type_id) {
            return cached;
        }

        // Look up the type structure
        let key = match self.interner.lookup(type_id) {
            Some(k) => k,
            None => return type_id,
        };

        // Mark as visiting (with original ID as placeholder for cycles)
        self.visiting.insert(type_id, type_id);

        let result = self.instantiate_key(&key);

        // Update the cache with the actual result
        self.visiting.insert(type_id, result);

        result
    }

    /// Instantiate a call signature.
    fn instantiate_call_signature(&mut self, sig: &CallSignature) -> CallSignature {
        let shadowed_len = self.shadowed.len();
        self.shadowed
            .extend(sig.type_params.iter().map(|tp| tp.name));

        let type_predicate = sig
            .type_predicate
            .as_ref()
            .map(|predicate| self.instantiate_type_predicate(predicate));
        let this_type = sig.this_type.map(|type_id| self.instantiate(type_id));
        let type_params: Vec<TypeParamInfo> = sig
            .type_params
            .iter()
            .map(|tp| TypeParamInfo {
                name: tp.name,
                constraint: tp.constraint.map(|c| self.instantiate(c)),
                default: tp.default.map(|d| self.instantiate(d)),
            })
            .collect();
        let params: Vec<ParamInfo> = sig
            .params
            .iter()
            .map(|p| ParamInfo {
                name: p.name,
                type_id: self.instantiate(p.type_id),
                optional: p.optional,
                rest: p.rest,
            })
            .collect();
        let return_type = self.instantiate(sig.return_type);

        self.shadowed.truncate(shadowed_len);

        CallSignature {
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
        }
    }

    fn instantiate_type_predicate(&mut self, predicate: &TypePredicate) -> TypePredicate {
        TypePredicate {
            asserts: predicate.asserts,
            target: predicate.target.clone(),
            type_id: predicate.type_id.map(|type_id| self.instantiate(type_id)),
        }
    }

    /// Instantiate a TypeKey.
    fn instantiate_key(&mut self, key: &TypeKey) -> TypeId {
        match key {
            // Type parameters get substituted
            TypeKey::TypeParameter(info) => {
                if self.is_shadowed(info.name) {
                    return self.interner.intern(key.clone());
                }
                if let Some(substituted) = self.substitution.get(info.name) {
                    substituted
                } else {
                    // No substitution found, return original type parameter
                    self.interner.intern(key.clone())
                }
            }

            // Intrinsics don't change
            TypeKey::Intrinsic(_) | TypeKey::Literal(_) | TypeKey::Error => {
                self.interner.intern(key.clone())
            }

            // Ref types might resolve to something that needs substitution
            TypeKey::Ref(_) | TypeKey::TypeQuery(_) | TypeKey::UniqueSymbol(_) => {
                self.interner.intern(key.clone())
            }

            // Application: instantiate base and args
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(*app_id);
                let base = self.instantiate(app.base);
                let args: Vec<TypeId> = app.args.iter().map(|&arg| self.instantiate(arg)).collect();
                self.interner.application(base, args)
            }

            // This type doesn't substitute
            TypeKey::ThisType => self.interner.intern(key.clone()),

            // Union: instantiate all members
            TypeKey::Union(members) => {
                let members = self.interner.type_list(*members);
                let instantiated: Vec<TypeId> =
                    members.iter().map(|&m| self.instantiate(m)).collect();
                self.interner.union(instantiated)
            }

            // Intersection: instantiate all members
            TypeKey::Intersection(members) => {
                let members = self.interner.type_list(*members);
                let instantiated: Vec<TypeId> =
                    members.iter().map(|&m| self.instantiate(m)).collect();
                self.interner.intersection(instantiated)
            }

            // Array: instantiate element type
            TypeKey::Array(elem) => {
                let instantiated_elem = self.instantiate(*elem);
                self.interner.array(instantiated_elem)
            }

            // Tuple: instantiate all elements
            TypeKey::Tuple(elements) => {
                let elements = self.interner.tuple_list(*elements);
                let instantiated: Vec<TupleElement> = elements
                    .iter()
                    .map(|e| TupleElement {
                        type_id: self.instantiate(e.type_id),
                        name: e.name,
                        optional: e.optional,
                        rest: e.rest,
                    })
                    .collect();
                self.interner.tuple(instantiated)
            }

            // Object: instantiate all property types
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                let instantiated: Vec<PropertyInfo> = shape
                    .properties
                    .iter()
                    .map(|p| PropertyInfo {
                        name: p.name,
                        type_id: self.instantiate(p.type_id),
                        write_type: self.instantiate(p.write_type),
                        optional: p.optional,
                        readonly: p.readonly,
                        is_method: p.is_method,
                    })
                    .collect();
                self.interner.object(instantiated)
            }

            // Object with index signatures: instantiate all types
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                let instantiated_props: Vec<PropertyInfo> = shape
                    .properties
                    .iter()
                    .map(|p| PropertyInfo {
                        name: p.name,
                        type_id: self.instantiate(p.type_id),
                        write_type: self.instantiate(p.write_type),
                        optional: p.optional,
                        readonly: p.readonly,
                        is_method: p.is_method,
                    })
                    .collect();
                let instantiated_string_idx =
                    shape.string_index.as_ref().map(|idx| IndexSignature {
                        key_type: self.instantiate(idx.key_type),
                        value_type: self.instantiate(idx.value_type),
                        readonly: idx.readonly,
                    });
                let instantiated_number_idx =
                    shape.number_index.as_ref().map(|idx| IndexSignature {
                        key_type: self.instantiate(idx.key_type),
                        value_type: self.instantiate(idx.value_type),
                        readonly: idx.readonly,
                    });
                self.interner.object_with_index(ObjectShape {
                    properties: instantiated_props,
                    string_index: instantiated_string_idx,
                    number_index: instantiated_number_idx,
                })
            }

            // Function: instantiate params and return type
            // Note: Type params in the function create a new scope - don't substitute those
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(*shape_id);
                let shadowed_len = self.shadowed.len();
                self.shadowed
                    .extend(shape.type_params.iter().map(|tp| tp.name));

                let type_predicate = shape
                    .type_predicate
                    .as_ref()
                    .map(|predicate| self.instantiate_type_predicate(predicate));
                let this_type = shape.this_type.map(|type_id| self.instantiate(type_id));
                let instantiated_type_params: Vec<TypeParamInfo> = shape
                    .type_params
                    .iter()
                    .map(|tp| TypeParamInfo {
                        name: tp.name,
                        constraint: tp.constraint.map(|c| self.instantiate(c)),
                        default: tp.default.map(|d| self.instantiate(d)),
                    })
                    .collect();
                let instantiated_params: Vec<ParamInfo> = shape
                    .params
                    .iter()
                    .map(|p| ParamInfo {
                        name: p.name,
                        type_id: self.instantiate(p.type_id),
                        optional: p.optional,
                        rest: p.rest,
                    })
                    .collect();
                let instantiated_return = self.instantiate(shape.return_type);

                self.shadowed.truncate(shadowed_len);

                self.interner.function(FunctionShape {
                    type_params: instantiated_type_params,
                    params: instantiated_params,
                    this_type,
                    return_type: instantiated_return,
                    type_predicate,
                    is_constructor: shape.is_constructor,
                    is_method: shape.is_method,
                })
            }

            // Callable: instantiate all signatures and properties
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(*shape_id);
                let instantiated_call: Vec<CallSignature> = shape
                    .call_signatures
                    .iter()
                    .map(|sig| self.instantiate_call_signature(sig))
                    .collect();
                let instantiated_construct: Vec<CallSignature> = shape
                    .construct_signatures
                    .iter()
                    .map(|sig| self.instantiate_call_signature(sig))
                    .collect();
                let instantiated_props = shape
                    .properties
                    .iter()
                    .map(|p| PropertyInfo {
                        name: p.name,
                        type_id: self.instantiate(p.type_id),
                        write_type: self.instantiate(p.write_type),
                        optional: p.optional,
                        readonly: p.readonly,
                        is_method: p.is_method,
                    })
                    .collect();

                self.interner.callable(CallableShape {
                    call_signatures: instantiated_call,
                    construct_signatures: instantiated_construct,
                    properties: instantiated_props,
                    ..Default::default()
                })
            }

            // Conditional: instantiate all parts
            TypeKey::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(*cond_id);
                if cond.is_distributive
                    && let Some(TypeKey::TypeParameter(info)) =
                        self.interner.lookup(cond.check_type)
                    && !self.is_shadowed(info.name)
                    && let Some(substituted) = self.substitution.get(info.name)
                {
                    // When substituting with `never`, the result is `never`
                    if substituted == crate::solver::types::TypeId::NEVER {
                        return substituted;
                    }
                    // For `any`, we need to let evaluation handle it properly
                    // so it can distribute to both branches
                    if let Some(TypeKey::Union(members)) = self.interner.lookup(substituted) {
                        let members = self.interner.type_list(members);
                        let cond_type = self.interner.conditional(cond.as_ref().clone());
                        let mut results = Vec::with_capacity(members.len());
                        for &member in members.iter() {
                            let mut member_subst = self.substitution.clone();
                            member_subst.insert(info.name, member);
                            let instantiated =
                                instantiate_type(self.interner, cond_type, &member_subst);
                            let evaluated =
                                crate::solver::evaluate::evaluate_type(self.interner, instantiated);
                            results.push(evaluated);
                        }
                        return self.interner.union(results);
                    }
                }
                let instantiated = ConditionalType {
                    check_type: self.instantiate(cond.check_type),
                    extends_type: self.instantiate(cond.extends_type),
                    true_type: self.instantiate(cond.true_type),
                    false_type: self.instantiate(cond.false_type),
                    is_distributive: cond.is_distributive,
                };
                self.interner.conditional(instantiated)
            }

            // Mapped: instantiate constraint and template
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(*mapped_id);
                let shadowed_len = self.shadowed.len();
                self.shadowed.push(mapped.type_param.name);

                let instantiated = MappedType {
                    type_param: TypeParamInfo {
                        name: mapped.type_param.name,
                        constraint: mapped.type_param.constraint.map(|c| self.instantiate(c)),
                        default: mapped.type_param.default.map(|d| self.instantiate(d)),
                    },
                    constraint: self.instantiate(mapped.constraint),
                    name_type: mapped.name_type.map(|t| self.instantiate(t)),
                    template: self.instantiate(mapped.template),
                    readonly_modifier: mapped.readonly_modifier,
                    optional_modifier: mapped.optional_modifier,
                };

                self.shadowed.truncate(shadowed_len);

                self.interner.mapped(instantiated)
            }

            // Index access: instantiate both parts
            TypeKey::IndexAccess(obj, idx) => {
                let inst_obj = self.instantiate(*obj);
                let inst_idx = self.instantiate(*idx);
                self.interner
                    .intern(TypeKey::IndexAccess(inst_obj, inst_idx))
            }

            // KeyOf: instantiate the operand
            TypeKey::KeyOf(operand) => {
                let inst_operand = self.instantiate(*operand);
                self.interner.intern(TypeKey::KeyOf(inst_operand))
            }

            // ReadonlyType: instantiate the operand
            TypeKey::ReadonlyType(operand) => {
                let inst_operand = self.instantiate(*operand);
                self.interner.intern(TypeKey::ReadonlyType(inst_operand))
            }

            // Template literal: instantiate embedded types
            TypeKey::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(*spans);
                let instantiated: Vec<TemplateSpan> = spans
                    .iter()
                    .map(|span| match span {
                        TemplateSpan::Text(t) => TemplateSpan::Text(*t),
                        TemplateSpan::Type(t) => TemplateSpan::Type(self.instantiate(*t)),
                    })
                    .collect();
                self.interner.template_literal(instantiated)
            }

            // StringIntrinsic: instantiate the type argument
            TypeKey::StringIntrinsic { kind, type_arg } => {
                let inst_arg = self.instantiate(*type_arg);
                self.interner.intern(TypeKey::StringIntrinsic {
                    kind: *kind,
                    type_arg: inst_arg,
                })
            }

            // Infer: keep as-is unless explicitly substituting inference variables
            TypeKey::Infer(info) => {
                if self.substitute_infer
                    && !self.is_shadowed(info.name)
                    && let Some(substituted) = self.substitution.get(info.name)
                {
                    return substituted;
                }
                self.interner.intern(TypeKey::Infer(info.clone()))
            }
        }
    }
}

/// Convenience function for instantiating a type with a substitution.
pub fn instantiate_type(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    if substitution.is_empty() {
        return type_id;
    }
    let mut instantiator = TypeInstantiator::new(interner, substitution);
    let result = instantiator.instantiate(type_id);
    if instantiator.depth_exceeded {
        TypeId::ERROR
    } else {
        result
    }
}

/// Convenience function for instantiating a type while substituting infer variables.
pub fn instantiate_type_with_infer(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    if substitution.is_empty() {
        return type_id;
    }
    let mut instantiator = TypeInstantiator::new(interner, substitution);
    instantiator.substitute_infer = true;
    let result = instantiator.instantiate(type_id);
    if instantiator.depth_exceeded {
        TypeId::ERROR
    } else {
        result
    }
}

/// Convenience function for instantiating a generic type with type arguments.
pub fn instantiate_generic(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    type_params: &[TypeParamInfo],
    type_args: &[TypeId],
) -> TypeId {
    if type_params.is_empty() || type_args.is_empty() {
        return type_id;
    }
    let substitution = TypeSubstitution::from_args(type_params, type_args);
    instantiate_type(interner, type_id, &substitution)
}

#[cfg(test)]
#[path = "instantiate_tests.rs"]
mod tests;
