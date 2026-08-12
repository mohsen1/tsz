//! Infer binding substitution.
//!
//! Provides `InferSubstitutor` which performs deep traversal of a type,
//! replacing all `infer X` references with their bound values.

use crate::construction::TypeDatabase;
use crate::types::{
    CallSignature, CallableShape, ConditionalType, FunctionShape, IndexSignature, MappedType,
    ObjectShape, ParamInfo, PropertyInfo, TemplateSpan, TupleElement, TypeData, TypeId,
    TypeParamInfo,
};
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;

/// Helper for substituting infer bindings into types.
///
/// This struct performs a deep traversal of a type, replacing all `infer X`
/// references with their bound values from the bindings map.
pub(crate) struct InferSubstitutor<'a> {
    interner: &'a dyn TypeDatabase,
    bindings: FxHashMap<Atom, TypeId>,
    visiting: FxHashMap<TypeId, TypeId>,
    /// Remaining distinct-node-visit budget for this traversal (breadth bound).
    ///
    /// Decremented once per node entering [`Self::substitute_inner`]; cache
    /// hits and intrinsic leaves are free. Shared across the whole traversal
    /// (not saved/restored by [`Self::with_shadowed_binding`]) so it bounds
    /// cumulative breadth, not per-scope work. See
    /// [`crate::limits::MAX_INFER_SUBSTITUTION_NODES`].
    visit_budget: u32,
}

impl<'a> InferSubstitutor<'a> {
    /// Create a new substitutor with the given interner and bindings.
    pub fn new(interner: &'a dyn TypeDatabase, bindings: &'a FxHashMap<Atom, TypeId>) -> Self {
        Self::with_visit_budget(
            interner,
            bindings,
            crate::limits::MAX_INFER_SUBSTITUTION_NODES,
        )
    }

    /// Create a substitutor with an explicit node-visit budget.
    ///
    /// `new` uses the calibrated [`crate::limits::MAX_INFER_SUBSTITUTION_NODES`];
    /// tests use a small budget to exercise the breadth-bail path without
    /// materializing a million-node type.
    fn with_visit_budget(
        interner: &'a dyn TypeDatabase,
        bindings: &'a FxHashMap<Atom, TypeId>,
        visit_budget: u32,
    ) -> Self {
        InferSubstitutor {
            interner,
            bindings: bindings.clone(),
            visiting: FxHashMap::default(),
            visit_budget,
        }
    }

    /// Substitute infer types in the given type, returning the result.
    ///
    /// Guarded by [`crate::recursion::with_solver_frame`]: the traversal
    /// recurses structurally through every nested shape, and `infer` bindings
    /// produced by deep conditional evaluation (ts-toolbelt `MetaPath` /
    /// `AutoPath` family) can nest thousands of levels with fresh `TypeId`s at
    /// every level, so the per-`TypeId` `visiting` memo alone cannot bound the
    /// OS stack. On budget exhaustion the type is left opaque (identity), the
    /// same relation-preserving bail every other guarded solver recursion uses.
    pub fn substitute(&mut self, type_id: TypeId) -> TypeId {
        if type_id.is_intrinsic() {
            return type_id;
        }
        if let Some(&cached) = self.visiting.get(&type_id) {
            return cached;
        }
        // Breadth bound: `with_solver_frame` only caps recursion *depth*, and
        // is RAII-balanced, so a shallow-but-wide or self-expanding type
        // (fresh `TypeId`s interned per conditional level — issue #13040) walks
        // an unbounded number of distinct nodes without ever tripping it. Once
        // the per-traversal node budget is spent, leave the type opaque
        // (identity) — the same relation-preserving bail the depth guard takes.
        if self.visit_budget == 0 {
            return type_id;
        }
        self.visit_budget -= 1;
        crate::recursion::with_solver_frame(|| self.substitute_inner(type_id)).unwrap_or(type_id)
    }

    /// Unguarded traversal body for [`Self::substitute`].
    fn substitute_inner(&mut self, type_id: TypeId) -> TypeId {
        let Some(key) = self.interner.lookup(type_id) else {
            return type_id;
        };

        self.visiting.insert(type_id, type_id);

        let result = match key {
            TypeData::Infer(info) => self.bindings.get(&info.name).copied().unwrap_or(type_id),
            TypeData::UnresolvedTypeName(name) => {
                self.bindings.get(&name).copied().unwrap_or(type_id)
            }
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
                    // When a rest element is substituted with a concrete Tuple (or
                    // ReadonlyType wrapping one), flatten its elements into the parent
                    // tuple — matching the same invariant enforced by instantiate.rs.
                    if element.rest {
                        let inner =
                            crate::type_queries::data::unwrap_readonly(self.interner, substituted);
                        if let Some(TypeData::Tuple(inner_list)) = self.interner.lookup(inner) {
                            new_elements
                                .extend(self.interner.tuple_list(inner_list).iter().copied());
                            changed = true;
                            continue;
                        }
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
                        is_class_prototype: prop.is_class_prototype,
                        visibility: prop.visibility,
                        parent_id: prop.parent_id,
                        declaration_order: prop.declaration_order,
                        is_string_named: prop.is_string_named,
                        is_symbol_named: prop.is_symbol_named,
                        single_quoted_name: prop.single_quoted_name,
                        non_widening: false,
                    });
                }
                if changed {
                    self.interner.object_with_flags_and_symbol(
                        properties,
                        shape.flags,
                        shape.symbol,
                    )
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
                        is_class_prototype: prop.is_class_prototype,
                        visibility: prop.visibility,
                        parent_id: prop.parent_id,
                        declaration_order: prop.declaration_order,
                        is_string_named: prop.is_string_named,
                        is_symbol_named: prop.is_symbol_named,
                        single_quoted_name: prop.single_quoted_name,
                        non_widening: false,
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
                        param_name: index.param_name,
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
                        param_name: index.param_name,
                    }
                });
                let symbol_index = shape.symbol_index.as_ref().map(|index| {
                    let key_type = self.substitute(index.key_type);
                    let value_type = self.substitute(index.value_type);
                    if key_type != index.key_type || value_type != index.value_type {
                        changed = true;
                    }
                    IndexSignature {
                        key_type,
                        value_type,
                        readonly: index.readonly,
                        param_name: index.param_name,
                    }
                });
                if changed {
                    self.interner.object_with_index(ObjectShape {
                        flags: shape.flags,
                        properties,
                        string_index,
                        number_index,
                        symbol_index,
                        symbol: shape.symbol,
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
            TypeData::Mapped(mapped_id) => {
                // Every TypeId reachable from the mapped type must be visited so
                // that infer variables captured by an outer conditional flow into
                // the constraint (the `in` clause), the key remapping (`as`), the
                // and value template. Without
                // this arm, patterns like
                //     P extends `${infer K}.${infer R}` ? { [X in K]: F<T[K], R> } : ...
                // leave `K` and `R` unbound inside the mapped type after the outer
                // match succeeds, which makes `evaluate_mapped` defer and collapse
                // the outer object level.
                let mapped = self.interner.get_mapped(mapped_id);

                // Homomorphic union distribution (tsc: `instantiateMappedType`
                // distributes over a union source via `mapType`). When the
                // mapped type is homomorphic over a substituted infer/type
                // variable — its constraint is `keyof X` where X's name is
                // bound here — and X resolves to a union, substituting X with
                // the whole union and re-interning ONE mapped type collapses
                // `keyof (A | B)` to the shared keys, losing per-member
                // structure (a union of tuples becomes an index-signature
                // object; a union of objects becomes `{}`). Distribute per
                // member instead so each homomorphic instance maps over a
                // single source and downstream evaluation preserves its shape.
                // This runs before the plain structural substitution, which only
                // sees the already-collapsed `keyof <union>` and cannot recover
                // the homomorphic origin (the directly-authored
                // `{ [K in keyof (A | B)]: ... }`, which tsc does NOT
                // distribute, never has its source name in `bindings`).
                self.try_distribute_mapped_over_union_binding(&mapped)
                    .unwrap_or_else(|| self.substitute_mapped_structural(type_id, &mapped))
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
            TypeData::StringIntrinsic { kind, type_arg } => {
                let new_type_arg = self.substitute(type_arg);
                if new_type_arg == type_arg {
                    type_id
                } else {
                    self.interner.string_intrinsic(kind, new_type_arg)
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
                        suppress_display_optional: false,
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
                        type_predicate: shape.type_predicate,
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
                                suppress_display_optional: false,
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
                            type_predicate: sig.type_predicate,
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
                                suppress_display_optional: false,
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
                            type_predicate: sig.type_predicate,
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
                            is_class_prototype: prop.is_class_prototype,
                            visibility: prop.visibility,
                            parent_id: prop.parent_id,
                            declaration_order: prop.declaration_order,
                            is_string_named: prop.is_string_named,
                            is_symbol_named: prop.is_symbol_named,
                            single_quoted_name: prop.single_quoted_name,
                            non_widening: false,
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
                        param_name: idx.param_name,
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
                        param_name: idx.param_name,
                    }
                });

                if changed {
                    self.interner.callable(CallableShape {
                        call_signatures,
                        construct_signatures,
                        properties,
                        string_index,
                        number_index,
                        symbol: shape.symbol,
                        is_abstract: shape.is_abstract,
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

    /// Plain structural substitution of a mapped type: substitute every
    /// reachable `TypeId` (constraint, iteration-variable constraint/default,
    /// `name_type`, template) and re-intern, returning `type_id` unchanged when
    /// nothing was substituted. The iteration variable is shadowed while
    /// visiting `name_type`/`template` so an inner binding of the same name does
    /// not leak the outer infer binding.
    fn substitute_mapped_structural(&mut self, type_id: TypeId, mapped: &MappedType) -> TypeId {
        let constraint = self.substitute(mapped.constraint);
        let type_param_constraint = mapped.type_param.constraint.map(|c| self.substitute(c));
        let type_param_default = mapped.type_param.default.map(|d| self.substitute(d));
        let (name_type, template) =
            self.with_shadowed_binding(mapped.type_param.name, |substitutor| {
                (
                    mapped.name_type.map(|n| substitutor.substitute(n)),
                    substitutor.substitute(mapped.template),
                )
            });
        let unchanged = constraint == mapped.constraint
            && name_type == mapped.name_type
            && template == mapped.template
            && type_param_constraint == mapped.type_param.constraint
            && type_param_default == mapped.type_param.default;
        if unchanged {
            type_id
        } else {
            self.interner.mapped(MappedType {
                type_param: TypeParamInfo {
                    constraint: type_param_constraint,
                    default: type_param_default,
                    ..mapped.type_param
                },
                constraint,
                name_type,
                template,
                readonly_modifier: mapped.readonly_modifier,
                optional_modifier: mapped.optional_modifier,
            })
        }
    }

    /// Name of the homomorphic source variable when `constraint` is `keyof X`
    /// and X is a substitutable infer / type / unresolved-name reference.
    fn homomorphic_source_name(&self, constraint: TypeId) -> Option<Atom> {
        let TypeData::KeyOf(source) = self.interner.lookup(constraint)? else {
            return None;
        };
        match self.interner.lookup(source)? {
            TypeData::Infer(info) | TypeData::TypeParameter(info) => Some(info.name),
            TypeData::UnresolvedTypeName(name) => Some(name),
            _ => None,
        }
    }

    /// Distribute a homomorphic mapped type over a union-valued binding.
    ///
    /// Returns `Some(union)` when `mapped` is homomorphic over a substituted
    /// variable (`{ [K in keyof X]: ... }` with X's name bound here) and that
    /// binding is a `Union`; the result is the union of the mapped type applied
    /// to each member (X rebound to the single member). Returns `None` when the
    /// mapped type is not homomorphic over a bound variable, or its binding is
    /// not a union, so the caller falls back to plain structural substitution.
    fn try_distribute_mapped_over_union_binding(&mut self, mapped: &MappedType) -> Option<TypeId> {
        let source_name = self.homomorphic_source_name(mapped.constraint)?;
        let bound = *self.bindings.get(&source_name)?;
        let TypeData::Union(list_id) = self.interner.lookup(bound)? else {
            return None;
        };
        let members = self.interner.type_list(list_id).to_vec();
        if members.len() < 2 {
            return None;
        }
        let mut results = Vec::with_capacity(members.len());
        for member in members {
            // Rebind the homomorphic source to this single member, then
            // re-substitute the whole mapped type. With a non-union binding the
            // distribution check above no longer fires, so this recurses into
            // the plain structural substitution and yields a homomorphic mapped
            // type over the single member (`{ [K in keyof <member>]: ... }`),
            // which downstream evaluation shapes correctly (tuple stays a tuple,
            // object stays an object).
            let previous = self.bindings.insert(source_name, member);
            // The per-member result depends on the binding environment, so the
            // shared `visiting` memo (keyed only by `TypeId`) must not leak a
            // cached substitution computed under a different member's binding.
            let saved_visiting = std::mem::take(&mut self.visiting);
            results.push(self.substitute(self.interner.mapped(*mapped)));
            self.visiting = saved_visiting;
            match previous {
                Some(prev) => {
                    self.bindings.insert(source_name, prev);
                }
                None => {
                    self.bindings.remove(&source_name);
                }
            }
        }
        Some(self.interner.union(results))
    }

    fn with_shadowed_binding<T>(&mut self, name: Atom, f: impl FnOnce(&mut Self) -> T) -> T {
        let masked = self.bindings.remove(&name);
        // `visiting` entries are only valid for the current binding environment.
        // The mapped binder shadows an outer infer binding with the same name in
        // `name_type` and `template`, so cached substitutions from the constraint
        // must not leak across this scope boundary.
        //
        // But the shadow only exists when an infer binding of that name is
        // actually masked. When `masked` is `None`, `name` was not bound, so the
        // `bindings.remove` above is a no-op and the binding environment inside
        // `f` is *identical* to the caller's: every `visiting` memo entry stays
        // valid and must be preserved. This is the common case for an infer
        // pattern such as `{ [X in K]: F<T[K], R> }`, whose mapped iteration
        // variable `X` does not collide with the captured infer variables
        // (`K`/`R`). Resetting the memo there would discard correct work and
        // force re-substitution of every shared subtree under each mapped scope —
        // the dominant infer-substitution hotspot on deeply-nested mapped/
        // conditional expansions (kysely #10663). Keeping the memo across a
        // non-shadowing scope is behavior-identical; only the redundant re-walk
        // is removed.
        let Some(masked) = masked else {
            return f(self);
        };
        let outer_visiting = std::mem::take(&mut self.visiting);
        let result = f(self);
        self.visiting = outer_visiting;
        self.bindings.insert(name, masked);
        result
    }
}

#[cfg(test)]
mod visit_budget_tests {
    use super::*;
    use crate::def::DefId;
    use crate::intern::TypeInterner;
    use crate::types::TupleElement;

    /// Build `[name_0, name_1, …, name_{n-1}]` as a tuple of distinct
    /// `UnresolvedTypeName`s plus a bindings map sending each `name_i` to a
    /// distinct `Lazy(DefId)`. Substituting it fully yields a tuple of those
    /// lazies; the per-element identity makes a partial (budget-truncated)
    /// substitution observable element by element.
    fn bound_name_tuple(
        interner: &TypeInterner,
        n: usize,
    ) -> (TypeId, FxHashMap<Atom, TypeId>, Vec<TypeId>) {
        let mut bindings = FxHashMap::default();
        let mut elements = Vec::with_capacity(n);
        let mut values = Vec::with_capacity(n);
        for i in 0..n {
            let name = interner.intern_string(&format!("Name{i}"));
            let value = interner.lazy(DefId(1000 + i as u32));
            bindings.insert(name, value);
            elements.push(TupleElement::fixed(interner.unresolved_type_name(name)));
            values.push(value);
        }
        (interner.tuple(elements), bindings, values)
    }

    /// A budget at least as large as the node count substitutes every element —
    /// byte-identical to the default (calibrated, effectively unbounded) path.
    #[test]
    fn budget_at_or_above_node_count_substitutes_fully() {
        let interner = TypeInterner::new();
        let (input, bindings, values) = bound_name_tuple(&interner, 5);
        let expected = interner.tuple(values.iter().copied().map(TupleElement::fixed).collect());

        // The tuple node plus its five elements are six distinct visits.
        let bounded =
            InferSubstitutor::with_visit_budget(&interner, &bindings, 6).substitute(input);
        let default = InferSubstitutor::new(&interner, &bindings).substitute(input);

        assert_eq!(bounded, expected, "full budget substitutes every element");
        assert_eq!(
            bounded, default,
            "an at-capacity budget matches the calibrated default path"
        );
    }

    /// Once the budget is spent the remaining elements are left opaque
    /// (identity) rather than substituted — a relation-preserving partial
    /// result, the same bail shape the depth guard takes.
    #[test]
    fn exhausted_budget_leaves_remaining_nodes_opaque() {
        let interner = TypeInterner::new();
        let (input, bindings, values) = bound_name_tuple(&interner, 5);

        // Budget 3: the tuple (1) and the first two elements (2) are visited;
        // the last three elements see an empty budget and stay unsubstituted.
        let bounded =
            InferSubstitutor::with_visit_budget(&interner, &bindings, 3).substitute(input);

        let full = interner.tuple(values.iter().copied().map(TupleElement::fixed).collect());
        assert_ne!(bounded, full, "a spent budget must not fully substitute");

        let Some(TypeData::Tuple(list)) = interner.lookup(bounded) else {
            panic!("substitution result is still a tuple");
        };
        let elements = interner.tuple_list(list);
        assert_eq!(elements[0].type_id, values[0], "element 0 substituted");
        assert_eq!(elements[1].type_id, values[1], "element 1 substituted");
        for (i, original) in [2usize, 3, 4].into_iter().enumerate() {
            // Untouched elements equal the original `UnresolvedTypeName(Name_i)`.
            let name = interner.intern_string(&format!("Name{original}"));
            assert_eq!(
                elements[original].type_id,
                interner.unresolved_type_name(name),
                "element {original} (index past budget {i}) is left opaque",
            );
        }
    }

    /// The bail point is a deterministic function of the input and the budget,
    /// so the same inputs always truncate identically (no schedule sensitivity).
    #[test]
    fn budget_truncation_is_deterministic() {
        let interner = TypeInterner::new();
        let (input, bindings, _) = bound_name_tuple(&interner, 8);
        let first = InferSubstitutor::with_visit_budget(&interner, &bindings, 4).substitute(input);
        let second = InferSubstitutor::with_visit_budget(&interner, &bindings, 4).substitute(input);
        assert_eq!(
            first, second,
            "identical input + budget truncates identically"
        );
    }

    /// A zero budget is a hard stop: the top-level type is returned unchanged.
    #[test]
    fn zero_budget_returns_input_unchanged() {
        let interner = TypeInterner::new();
        let (input, bindings, _) = bound_name_tuple(&interner, 3);
        let bounded =
            InferSubstitutor::with_visit_budget(&interner, &bindings, 0).substitute(input);
        assert_eq!(bounded, input, "a zero budget substitutes nothing");
    }
}
