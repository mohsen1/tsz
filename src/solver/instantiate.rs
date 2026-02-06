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
pub const MAX_INSTANTIATION_DEPTH: u32 = 50;

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
    ///
    /// IMPORTANT: Defaults may reference earlier type parameters, so they need
    /// to be instantiated with the substitution built so far.
    pub fn from_args(
        interner: &dyn TypeDatabase,
        type_params: &[TypeParamInfo],
        type_args: &[TypeId],
    ) -> Self {
        let mut map = FxHashMap::default();
        for (i, param) in type_params.iter().enumerate() {
            let type_id = if i < type_args.len() {
                type_args[i]
            } else {
                // Use default value if type argument not provided
                match param.default {
                    Some(default) => {
                        // Defaults may reference earlier type parameters, so instantiate them
                        if i > 0 && !map.is_empty() {
                            let subst = TypeSubstitution { map: map.clone() };
                            instantiate_type(interner, default, &subst)
                        } else {
                            default
                        }
                    }
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

    /// Get a reference to the internal substitution map.
    ///
    /// This is useful for building new substitutions based on existing ones.
    pub fn map(&self) -> &FxHashMap<Atom, TypeId> {
        &self.map
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
    /// When set, substitutes `ThisType` with this concrete type.
    pub this_type: Option<TypeId>,
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
            this_type: None,
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
                is_const: false,
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
            is_method: sig.is_method,
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

            // Lazy types might resolve to something that needs substitution
            TypeKey::Lazy(_)
            | TypeKey::Recursive(_)
            | TypeKey::BoundParameter(_)
            | TypeKey::TypeQuery(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::ModuleNamespace(_) => self.interner.intern(key.clone()),

            // Enum types: instantiate the member type (structural part)
            // The DefId (nominal identity) stays the same
            TypeKey::Enum(def_id, member_type) => {
                let instantiated_member = self.instantiate(*member_type);
                self.interner
                    .intern(TypeKey::Enum(*def_id, instantiated_member))
            }

            // Application: instantiate base and args
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(*app_id);
                let base = self.instantiate(app.base);
                let args: Vec<TypeId> = app.args.iter().map(|&arg| self.instantiate(arg)).collect();
                self.interner.application(base, args)
            }

            // This type: substitute with concrete this_type if provided
            TypeKey::ThisType => {
                if let Some(this_type) = self.this_type {
                    this_type
                } else {
                    self.interner.intern(key.clone())
                }
            }

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
                        visibility: p.visibility,
                        parent_id: p.parent_id,
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
                        visibility: p.visibility,
                        parent_id: p.parent_id,
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
                    flags: shape.flags,
                    properties: instantiated_props,
                    string_index: instantiated_string_idx,
                    number_index: instantiated_number_idx,
                    symbol: None,
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
                        is_const: false,
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
                        visibility: p.visibility,
                        parent_id: p.parent_id,
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
                        // Limit distribution to prevent OOM with large unions
                        // (e.g., string literal unions with thousands of members)
                        const MAX_DISTRIBUTION_SIZE: usize = 100;
                        if members.len() > MAX_DISTRIBUTION_SIZE {
                            self.depth_exceeded = true;
                            return TypeId::ERROR;
                        }
                        let cond_type = self.interner.conditional(cond.as_ref().clone());
                        let mut results = Vec::with_capacity(members.len());
                        for &member in members.iter() {
                            // Check depth before each distribution step
                            if self.depth_exceeded {
                                return TypeId::ERROR;
                            }
                            let mut member_subst = self.substitution.clone();
                            member_subst.insert(info.name, member);
                            let instantiated =
                                instantiate_type(self.interner, cond_type, &member_subst);
                            // Check if instantiation hit depth limit
                            if instantiated == TypeId::ERROR {
                                self.depth_exceeded = true;
                                return TypeId::ERROR;
                            }
                            let evaluated =
                                crate::solver::evaluate::evaluate_type(self.interner, instantiated);
                            // Check if evaluation hit depth limit
                            if evaluated == TypeId::ERROR {
                                self.depth_exceeded = true;
                                return TypeId::ERROR;
                            }
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
                        is_const: false,
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

            // Index access: instantiate both parts and evaluate immediately
            // Task #46: Meta-type reduction for O(1) equality
            TypeKey::IndexAccess(obj, idx) => {
                let inst_obj = self.instantiate(*obj);
                let inst_idx = self.instantiate(*idx);
                // Evaluate immediately to achieve O(1) equality
                crate::solver::evaluate::evaluate_index_access(self.interner, inst_obj, inst_idx)
            }

            // KeyOf: instantiate the operand and evaluate immediately
            // Task #46: Meta-type reduction for O(1) equality
            TypeKey::KeyOf(operand) => {
                let inst_operand = self.instantiate(*operand);
                // Evaluate immediately to expand keyof { a: 1 } -> "a"
                crate::solver::evaluate::evaluate_keyof(self.interner, inst_operand)
            }

            // ReadonlyType: instantiate the operand
            TypeKey::ReadonlyType(operand) => {
                let inst_operand = self.instantiate(*operand);
                self.interner.intern(TypeKey::ReadonlyType(inst_operand))
            }

            // Template literal: instantiate embedded types
            // After substitution, if any type span becomes a union of string literals,
            // we trigger evaluation to expand the template literal into a union of strings.
            TypeKey::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(*spans);
                let mut instantiated: Vec<TemplateSpan> = Vec::with_capacity(spans.len());
                let mut needs_evaluation = false;

                for span in spans.iter() {
                    match span {
                        TemplateSpan::Text(t) => instantiated.push(TemplateSpan::Text(*t)),
                        TemplateSpan::Type(t) => {
                            let inst_type = self.instantiate(*t);
                            // Check if this type became something that can be evaluated:
                            // - A union of string literals
                            // - A single string literal
                            // - The string intrinsic type
                            if let Some(key) = self.interner.lookup(inst_type) {
                                match key {
                                    TypeKey::Union(_)
                                    | TypeKey::Literal(LiteralValue::String(_))
                                    | TypeKey::Literal(LiteralValue::Number(_))
                                    | TypeKey::Literal(LiteralValue::Boolean(_))
                                    | TypeKey::Intrinsic(IntrinsicKind::String)
                                    | TypeKey::Intrinsic(IntrinsicKind::Number)
                                    | TypeKey::Intrinsic(IntrinsicKind::Boolean) => {
                                        needs_evaluation = true;
                                    }
                                    _ => {}
                                }
                            }
                            instantiated.push(TemplateSpan::Type(inst_type));
                        }
                    }
                }

                let template_type = self.interner.template_literal(instantiated);

                // If we detected types that can be evaluated, trigger evaluation
                // to potentially expand the template literal to a union of string literals
                if needs_evaluation {
                    crate::solver::evaluate::evaluate_type(self.interner, template_type)
                } else {
                    template_type
                }
            }

            // StringIntrinsic: instantiate the type argument
            // After substitution, if the type argument becomes a concrete type that can
            // be evaluated (like a string literal or union), trigger evaluation.
            TypeKey::StringIntrinsic { kind, type_arg } => {
                let inst_arg = self.instantiate(*type_arg);
                let string_intrinsic = self.interner.intern(TypeKey::StringIntrinsic {
                    kind: *kind,
                    type_arg: inst_arg,
                });

                // Check if we can evaluate the result
                if let Some(key) = self.interner.lookup(inst_arg) {
                    match key {
                        TypeKey::Union(_)
                        | TypeKey::Literal(LiteralValue::String(_))
                        | TypeKey::TemplateLiteral(_)
                        | TypeKey::Intrinsic(IntrinsicKind::String) => {
                            crate::solver::evaluate::evaluate_type(self.interner, string_intrinsic)
                        }
                        _ => string_intrinsic,
                    }
                } else {
                    string_intrinsic
                }
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
    let substitution = TypeSubstitution::from_args(interner, type_params, type_args);
    instantiate_type(interner, type_id, &substitution)
}

/// Substitute `ThisType` with a concrete type throughout a type.
///
/// Used for method call return types where `this` refers to the receiver's type.
/// For example, in a fluent builder pattern:
/// ```typescript
/// class Builder { setName(n: string): this { ... } }
/// const b: Builder = new Builder().setName("foo"); // this → Builder
/// ```
pub fn substitute_this_type(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    this_type: TypeId,
) -> TypeId {
    // Quick check: if the type is intrinsic, no substitution needed
    if type_id.is_intrinsic() {
        return type_id;
    }
    let empty_subst = TypeSubstitution::new();
    let mut instantiator = TypeInstantiator::new(interner, &empty_subst);
    instantiator.this_type = Some(this_type);
    let result = instantiator.instantiate(type_id);
    if instantiator.depth_exceeded {
        TypeId::ERROR
    } else {
        result
    }
}

#[cfg(test)]
#[path = "tests/instantiate_tests.rs"]
mod tests;
