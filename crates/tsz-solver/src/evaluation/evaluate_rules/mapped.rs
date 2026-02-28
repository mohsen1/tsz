//! Mapped type evaluation.
//!
//! Handles TypeScript's mapped types: `{ [K in keyof T]: T[K] }`
//! Including homomorphic mapped types that preserve modifiers.

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::objects::{PropertyCollectionResult, collect_properties};
use crate::relations::subtype::TypeResolver;
use crate::types::Visibility;
use crate::types::{
    IndexSignature, IntrinsicKind, LiteralValue, MappedModifier, MappedType, ObjectFlags,
    ObjectShape, PropertyInfo, TupleListId, TypeData, TypeId,
};
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;

pub(crate) struct MappedKeys {
    pub string_literals: Vec<Atom>,
    pub has_string: bool,
    pub has_number: bool,
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Helper for key remapping in mapped types.
    /// Returns Ok(Some(remapped)) if remapping succeeded,
    /// Ok(None) if the key should be filtered (remapped to never),
    /// Err(()) if we can't process and should return the original mapped type.
    #[tracing::instrument(level = "trace", skip(self), fields(
        param_name = ?mapped.type_param.name,
        key_type = key_type.0,
        has_name_type = mapped.name_type.is_some(),
    ))]
    fn remap_key_type_for_mapped(
        &mut self,
        mapped: &MappedType,
        key_type: TypeId,
    ) -> Result<Option<TypeId>, ()> {
        let Some(name_type) = mapped.name_type else {
            return Ok(Some(key_type));
        };

        tracing::trace!(
            key_type_lookup = ?self.interner().lookup(key_type),
            name_type_lookup = ?self.interner().lookup(name_type),
            "remap_key_type_for_mapped: before substitution"
        );

        let mut subst = TypeSubstitution::new();
        subst.insert(mapped.type_param.name, key_type);
        let remapped = instantiate_type(self.interner(), name_type, &subst);

        tracing::trace!(
            remapped_before_eval = remapped.0,
            remapped_lookup = ?self.interner().lookup(remapped),
            "remap_key_type_for_mapped: after substitution"
        );

        let remapped = self.evaluate(remapped);

        tracing::trace!(
            remapped_after_eval = remapped.0,
            remapped_eval_lookup = ?self.interner().lookup(remapped),
            is_never = remapped == TypeId::NEVER,
            "remap_key_type_for_mapped: after evaluation"
        );

        if remapped == TypeId::NEVER {
            return Ok(None);
        }
        Ok(Some(remapped))
    }

    /// Helper to compute modifiers for a mapped type property.
    fn get_mapped_modifiers(
        &mut self,
        mapped: &MappedType,
        is_homomorphic: bool,
        source_object: Option<TypeId>,
        key_name: Atom,
    ) -> (bool, bool) {
        // NOTE: This helper is now only used for index signatures.
        // Direct property modifiers are handled via the memoized map in evaluate_mapped.
        let source_mods = if let Some(source_obj) = source_object {
            match collect_properties(source_obj, self.interner(), self.resolver()) {
                PropertyCollectionResult::Properties { properties, .. } => properties
                    .iter()
                    .find(|p| p.name == key_name)
                    .map_or((false, false), |p| (p.optional, p.readonly)),
                _ => (false, false),
            }
        } else {
            (false, false)
        };

        let optional = match mapped.optional_modifier {
            Some(MappedModifier::Add) => true,
            Some(MappedModifier::Remove) => false,
            None => {
                // For homomorphic types with no explicit modifier, preserve original
                if is_homomorphic { source_mods.0 } else { false }
            }
        };

        let readonly = match mapped.readonly_modifier {
            Some(MappedModifier::Add) => true,
            Some(MappedModifier::Remove) => false,
            None => {
                // For homomorphic types with no explicit modifier, preserve original
                if is_homomorphic { source_mods.1 } else { false }
            }
        };

        (optional, readonly)
    }

    /// Evaluate a mapped type: { [K in Keys]: Template }
    ///
    /// Algorithm:
    /// 1. Extract the constraint (Keys) - this defines what keys to iterate over
    /// 2. For each key K in the constraint:
    ///    - Substitute K into the template type
    ///    - Apply readonly/optional modifiers
    /// 3. Construct a new object type with the resulting properties
    pub fn evaluate_mapped(&mut self, mapped: &MappedType) -> TypeId {
        // TODO: Array/Tuple Preservation for Homomorphic Mapped Types
        // If source_object is an Array or Tuple, we should construct a Mapped Array/Tuple
        // instead of degrading to a plain Object. This is required to preserve
        // Array.prototype methods (push, pop, map) and tuple-specific behavior.
        // Example: type Boxed<T> = { [K in keyof T]: Box<T[K]> }
        //   Boxed<[number, string]> should be [Box<number>, Box<string>] (Tuple)
        //   Boxed<number[]> should be Box<number>[] (Array)
        // Current implementation degrades both to plain Objects.

        // Check if depth was already exceeded
        if self.is_depth_exceeded() {
            return TypeId::ERROR;
        }

        // Get the constraint - this tells us what keys to iterate over
        let constraint = mapped.constraint;

        // SPECIAL CASE: Don't expand mapped types over type parameters.
        // When the constraint is `keyof T` where T is a type parameter, we should
        // keep the mapped type deferred. Even though we might be able to evaluate
        // `keyof T` to concrete keys (via T's constraint), the template instantiation
        // would fail because T[key] can't be resolved for a type parameter.
        //
        // EXCEPTION: If the type parameter is constrained to an array or tuple,
        // we should produce an array/tuple type instead of deferring. This matches
        // tsc's instantiateMappedArrayType behavior. For example:
        //   function f<T extends any[]>(a: Boxified<T>) { a.concat(a); }
        // Boxified<T> should evaluate to Box<T[number]>[] (an array), not a deferred
        // mapped type. The template's T[K] with K=number resolves through the
        // constraint (T[number] where T extends any[] → any).
        if self.is_mapped_type_over_type_parameter(mapped) {
            // Before deferring, check if the type parameter has an array/tuple constraint.
            if let Some(result) = self.try_evaluate_mapped_over_array_param(mapped) {
                return result;
            }

            tracing::trace!(
                constraint = ?self.interner().lookup(constraint),
                "evaluate_mapped: DEFERRED - mapped type over type parameter"
            );
            return self.interner().mapped(mapped.clone());
        }

        // Evaluate the constraint to get concrete keys
        let keys = self.evaluate_keyof_or_constraint(constraint);

        // If we can't determine concrete keys, keep it as a mapped type (deferred)
        let key_set = match self.extract_mapped_keys(keys) {
            Some(mut keys) => {
                // Deduplicate string literals to handle overlapping enum members.
                // For example, `enum A { CAT = "cat" }` and `enum B { CAT = "cat" }` both
                // produce key "cat". Without dedup, the mapped type would have two conflicting
                // properties named "cat" with different types.
                keys.string_literals.sort_unstable();
                keys.string_literals.dedup();
                keys
            }
            None => {
                tracing::trace!(
                    keys_lookup = ?self.interner().lookup(keys),
                    "evaluate_mapped: DEFERRED - could not extract concrete keys"
                );
                return self.interner().mapped(mapped.clone());
            }
        };

        // Limit number of keys to prevent OOM with large mapped types.
        // WASM environments have limited memory, but 100 is too restrictive for
        // real-world code (large SDKs, generated API types often have 150-250 keys).
        // 250 covers ~99% of real-world use cases while remaining safe for WASM.
        #[cfg(target_arch = "wasm32")]
        const MAX_MAPPED_KEYS: usize = 250;
        #[cfg(not(target_arch = "wasm32"))]
        const MAX_MAPPED_KEYS: usize = 500;
        if key_set.string_literals.len() > MAX_MAPPED_KEYS {
            self.mark_depth_exceeded();
            return TypeId::ERROR;
        }

        // Check if this is a homomorphic mapped type (template is T[K] indexed access).
        // Returns the source object T if homomorphic.
        // This handles both pre-evaluation form (constraint is `keyof T`) and
        // post-instantiation form (constraint eagerly evaluated to literal union).
        let homomorphic_source = self.homomorphic_mapped_source(mapped);
        let is_homomorphic = homomorphic_source.is_some();

        // For homomorphic types, source comes from the homomorphic check.
        // For non-homomorphic types, still try extracting from keyof for array/tuple preservation.
        let source_object =
            homomorphic_source.or_else(|| self.extract_source_from_keyof(mapped.constraint));

        // PERF: Memoize source properties into a hash map for O(1) lookup during the key loop.
        // This avoids repeated O(N) collect_properties calls inside the loop.
        let mut source_prop_map = FxHashMap::default();
        if let Some(source) = source_object {
            // Evaluate the source to resolve Application types (e.g., Partial<X> is
            // Application(Partial, [X]) which evaluates to { prop?: ... }). Without
            // this, collect_properties can't extract properties from unevaluated
            // Applications, causing optional/readonly modifiers to be lost.
            let resolved_source = self.evaluate(source);
            match collect_properties(resolved_source, self.interner(), self.resolver()) {
                PropertyCollectionResult::Properties { properties, .. } => {
                    for prop in properties {
                        source_prop_map
                            .insert(prop.name, (prop.optional, prop.readonly, prop.type_id));
                    }
                }
                PropertyCollectionResult::Any | PropertyCollectionResult::NonObject => {
                    // Any type properties are treated as (false, false, ANY)
                }
            }
        }

        // For homomorphic mapped types, capture the source object's property declaration
        // order. tsc preserves declaration order in mapped type results (e.g., Required<Foo>
        // lists properties in the same order as Foo). Our key extraction sorts by Atom ID
        // which can differ from declaration order. We fix this by re-sorting the output
        // properties to match the source's declaration order.
        let source_decl_order: Vec<Atom> = if is_homomorphic {
            if let Some(source) = source_object {
                let resolved = self.evaluate(source);
                let order: Vec<Atom> = match self.interner().lookup(resolved) {
                    Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                        let shape = self.interner().object_shape(shape_id);
                        shape.properties.iter().map(|p| p.name).collect()
                    }
                    _ => Vec::new(),
                };
                tracing::trace!(
                    source_id = source.0,
                    resolved_id = resolved.0,
                    decl_order = ?order.iter().map(|a| self.interner().resolve_atom_ref(*a)).collect::<Vec<_>>(),
                    "evaluate_mapped: source declaration order"
                );
                order
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // HOMOMORPHIC ARRAY/TUPLE PRESERVATION
        // If source_object is an Array or Tuple, preserve the structure instead of
        // degrading to a plain Object. This preserves Array methods (push, pop, map)
        // and tuple-specific behavior.
        //
        // Example: type Partial<T> = { [P in keyof T]?: T[P] }
        //   Partial<[number, string]> should be [number?, string?] (Tuple)
        //   Partial<number[]> should be (number | undefined)[] (Array)
        //
        // Preserve if there's NO name remapping, OR if the name type is an identity
        // mapping (as K where K is the iteration variable). Identity `as` clauses
        // don't change keys so the mapped type is still homomorphic.
        // Example: { [K in keyof T as K]: T[K] } is equivalent to { [K in keyof T]: T[K] }
        if let Some(source) = source_object {
            let is_identity_or_no_name = mapped.name_type.is_none()
                || mapped.name_type.is_some_and(|nt| {
                    matches!(
                        self.interner().lookup(nt),
                        Some(TypeData::TypeParameter(param)) if param.name == mapped.type_param.name
                    )
                });
            if is_identity_or_no_name {
                // Resolve the source to check if it's an Array or Tuple
                // Use evaluate() to resolve Lazy types (interfaces/classes)
                let resolved = self.evaluate(source);

                match self.interner().lookup(resolved) {
                    // Array type: map the element type
                    Some(TypeData::Array(element_type)) => {
                        return self.evaluate_mapped_array(mapped, element_type);
                    }

                    // Tuple type: map each element
                    Some(TypeData::Tuple(tuple_id)) => {
                        return self.evaluate_mapped_tuple(mapped, tuple_id);
                    }

                    // ReadonlyArray: map the element type and preserve readonly
                    Some(TypeData::ObjectWithIndex(shape_id)) => {
                        // Check if this is a ReadonlyArray (has readonly numeric index)
                        // Note: We DON'T check properties.is_empty() because ReadonlyArray<T>
                        // has methods like length, map, filter, etc. We only care about the index signature.
                        let shape = self.interner().object_shape(shape_id);
                        let has_readonly_index = shape
                            .number_index
                            .as_ref()
                            .is_some_and(|idx| idx.readonly && idx.key_type == TypeId::NUMBER);

                        if has_readonly_index {
                            // This is ReadonlyArray<T> - map element type
                            // Extract the element type from the number index signature
                            if let Some(index) = &shape.number_index {
                                return self.evaluate_mapped_array_with_readonly(
                                    mapped,
                                    index.value_type,
                                    true,
                                );
                            }
                        }
                    }

                    _ => {}
                }
            }
        }

        // Build the resulting object properties
        let mut properties = Vec::new();

        for key_name in key_set.string_literals {
            // Check if depth was exceeded during previous iterations
            if self.is_depth_exceeded() {
                return TypeId::ERROR;
            }

            // Create substitution: type_param.name -> literal key type
            // Use canonical constructor for O(1) equality
            let key_literal = self.interner().literal_string_atom(key_name);
            let remapped = match self.remap_key_type_for_mapped(mapped, key_literal) {
                Ok(Some(remapped)) => remapped,
                Ok(None) => continue,
                Err(()) => return self.interner().mapped(mapped.clone()),
            };
            // Extract property name(s) from remapped key.
            // Handle unions: `as \`${K}1\` | \`${K}2\`` produces multiple properties per key.
            let remapped_names: Vec<Atom> =
                if let Some(name) = crate::visitor::literal_string(self.interner(), remapped) {
                    vec![name]
                } else if let Some(TypeData::Union(list_id)) = self.interner().lookup(remapped) {
                    let members = self.interner().type_list(list_id);
                    let names: Vec<Atom> = members
                        .iter()
                        .filter_map(|&m| crate::visitor::literal_string(self.interner(), m))
                        .collect();
                    if names.is_empty() {
                        return self.interner().mapped(mapped.clone());
                    }
                    names
                } else {
                    return self.interner().mapped(mapped.clone());
                };

            let mut subst = TypeSubstitution::new();
            subst.insert(mapped.type_param.name, key_literal);

            // Substitute into the template
            let instantiated_template = instantiate_type(self.interner(), mapped.template, &subst);
            let mut property_type = self.evaluate(instantiated_template);

            // Check if evaluation hit depth limit
            if property_type == TypeId::ERROR && self.is_depth_exceeded() {
                return TypeId::ERROR;
            }

            // Get modifiers for this specific key (preserves homomorphic behavior)
            // Use memoized source property info for O(1) lookup.
            let source_info = source_prop_map.get(&key_name);
            let (source_optional, source_readonly) =
                source_info.map_or((false, false), |(opt, ro, _)| (*opt, *ro));

            let optional = match mapped.optional_modifier {
                Some(MappedModifier::Add) => true,
                Some(MappedModifier::Remove) => false,
                None => {
                    if is_homomorphic {
                        source_optional
                    } else {
                        false
                    }
                }
            };

            let readonly = match mapped.readonly_modifier {
                Some(MappedModifier::Add) => true,
                Some(MappedModifier::Remove) => false,
                None => {
                    if is_homomorphic {
                        source_readonly
                    } else {
                        false
                    }
                }
            };

            // TypeScript homomorphic mapped type behavior: the template `T[P]` evaluates
            // via IndexAccess which adds `| undefined` for optional source properties.
            // In homomorphic mapped types, since optionality is already captured by the
            // `?` modifier on the output property, we should use the DECLARED type
            // (without the extra `| undefined`) to avoid double-encoding optionality.
            //
            // This applies when:
            // 1. The mapped type is homomorphic (template is `T[P]`)
            // 2. The source property is optional
            // Regardless of whether the output property stays optional or not (e.g., `-?`
            // removes optionality, but the declared type is still correct).
            if is_homomorphic
                && source_optional
                && let Some((_, _, declared_type)) = source_info
            {
                property_type = *declared_type;
            }

            for remapped_name in remapped_names {
                properties.push(PropertyInfo {
                    name: remapped_name,
                    type_id: property_type,
                    write_type: property_type,
                    optional,
                    readonly,
                    is_method: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                });
            }
        }

        // For homomorphic mapped types, restore source declaration order.
        // The key extraction and dedup may have reordered properties.
        if !source_decl_order.is_empty() {
            let order_map: FxHashMap<Atom, usize> = source_decl_order
                .iter()
                .enumerate()
                .map(|(i, &name)| (name, i))
                .collect();
            properties.sort_by_key(|p| order_map.get(&p.name).copied().unwrap_or(usize::MAX));
        }

        let string_index = if key_set.has_string {
            match self.remap_key_type_for_mapped(mapped, TypeId::STRING) {
                Ok(Some(remapped)) => {
                    if remapped != TypeId::STRING {
                        return self.interner().mapped(mapped.clone());
                    }
                    let key_type = TypeId::STRING;
                    let mut subst = TypeSubstitution::new();
                    subst.insert(mapped.type_param.name, key_type);
                    let mut value_type =
                        self.evaluate(instantiate_type(self.interner(), mapped.template, &subst));

                    // Get modifiers for string index
                    let empty_atom = self.interner().intern_string("");
                    let (idx_optional, idx_readonly) = self.get_mapped_modifiers(
                        mapped,
                        is_homomorphic,
                        source_object,
                        empty_atom,
                    );
                    if idx_optional {
                        value_type = self.interner().union2(value_type, TypeId::UNDEFINED);
                    }
                    Some(IndexSignature {
                        key_type,
                        value_type,
                        readonly: idx_readonly,
                        param_name: None,
                    })
                }
                Ok(None) => None,
                Err(()) => return self.interner().mapped(mapped.clone()),
            }
        } else {
            None
        };

        let number_index = if key_set.has_number {
            match self.remap_key_type_for_mapped(mapped, TypeId::NUMBER) {
                Ok(Some(remapped)) => {
                    if remapped != TypeId::NUMBER {
                        return self.interner().mapped(mapped.clone());
                    }
                    let key_type = TypeId::NUMBER;
                    let mut subst = TypeSubstitution::new();
                    subst.insert(mapped.type_param.name, key_type);
                    let mut value_type =
                        self.evaluate(instantiate_type(self.interner(), mapped.template, &subst));

                    // Get modifiers for number index
                    let empty_atom = self.interner().intern_string("");
                    let (idx_optional, idx_readonly) = self.get_mapped_modifiers(
                        mapped,
                        is_homomorphic,
                        source_object,
                        empty_atom,
                    );
                    if idx_optional {
                        value_type = self.interner().union2(value_type, TypeId::UNDEFINED);
                    }
                    Some(IndexSignature {
                        key_type,
                        value_type,
                        readonly: idx_readonly,
                        param_name: None,
                    })
                }
                Ok(None) => None,
                Err(()) => return self.interner().mapped(mapped.clone()),
            }
        } else {
            None
        };

        if string_index.is_some() || number_index.is_some() {
            self.interner().object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
                properties,
                string_index,
                number_index,
                symbol: None,
            })
        } else {
            self.interner().object(properties)
        }
    }

    /// Check if a mapped type's constraint is `keyof T` where T is a type parameter.
    ///
    /// When this is true, we should not expand the mapped type because the template
    /// instantiation would fail (T[key] can't be resolved for a type parameter).
    fn is_mapped_type_over_type_parameter(&self, mapped: &MappedType) -> bool {
        // Check if the constraint is `keyof S`
        let Some(TypeData::KeyOf(source)) = self.interner().lookup(mapped.constraint) else {
            return false;
        };

        // Check if the source is a type parameter directly
        if matches!(
            self.interner().lookup(source),
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
        ) {
            return true;
        }

        // Also defer when the source is itself a mapped type that's over a type
        // parameter (transitive deferral). This handles Readonly<Partial<T>> where
        // Partial<T> is a deferred mapped type: keyof(Partial<T>) can resolve
        // through T's constraint, but we should keep the mapped type deferred to
        // preserve correct structural comparison with other deferred mapped types.
        if let Some(TypeData::Mapped(inner_mapped_id)) = self.interner().lookup(source) {
            let inner_mapped = self.interner().mapped_type(inner_mapped_id);
            return self.is_mapped_type_over_type_parameter(&inner_mapped);
        }

        false
    }

    /// Try to evaluate a mapped type over a type parameter as an array/tuple.
    ///
    /// When the mapped type's source is a type parameter constrained to an array
    /// or tuple, we produce an array/tuple type instead of deferring. This matches
    /// tsc's `instantiateMappedArrayType` behavior.
    ///
    /// For `Boxified<T>` where `T extends any[]`:
    /// - Template `Box<T[K]>` with K=number → `Box<T[number]>` → `Box<any>`
    /// - Result: `Array(Box<any>)` instead of a deferred Mapped type
    fn try_evaluate_mapped_over_array_param(&mut self, mapped: &MappedType) -> Option<TypeId> {
        // Extract the type parameter from the constraint (keyof T → T)
        let TypeData::KeyOf(source) = self.interner().lookup(mapped.constraint)? else {
            return None;
        };
        let TypeData::TypeParameter(param) = self.interner().lookup(source)? else {
            return None;
        };
        let constraint = param.constraint?;

        // Only preserve array shape for identity name mappings (no `as` clause
        // or `as K` where K is the iteration variable)
        let is_identity_or_no_name = mapped.name_type.is_none()
            || mapped.name_type.is_some_and(|nt| {
                matches!(
                    self.interner().lookup(nt),
                    Some(TypeData::TypeParameter(p)) if p.name == mapped.type_param.name
                )
            });
        if !is_identity_or_no_name {
            return None;
        }

        // Resolve the constraint to check if it's array/tuple-like
        let resolved = self.evaluate(constraint);
        match self.interner().lookup(resolved) {
            Some(TypeData::Array(element_type)) => {
                tracing::trace!(
                    element_type = element_type.0,
                    "evaluate_mapped: array-constrained type parameter → producing array"
                );
                Some(self.evaluate_mapped_array(mapped, element_type))
            }
            Some(TypeData::Tuple(tuple_id)) => {
                tracing::trace!(
                    "evaluate_mapped: tuple-constrained type parameter → producing tuple"
                );
                Some(self.evaluate_mapped_tuple(mapped, tuple_id))
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner().object_shape(shape_id);
                let has_readonly_index = shape
                    .number_index
                    .as_ref()
                    .is_some_and(|idx| idx.readonly && idx.key_type == TypeId::NUMBER);
                if has_readonly_index && let Some(index) = &shape.number_index {
                    tracing::trace!(
                        "evaluate_mapped: readonly-array-constrained type parameter → producing readonly array"
                    );
                    return Some(self.evaluate_mapped_array_with_readonly(
                        mapped,
                        index.value_type,
                        true,
                    ));
                }
                None
            }
            _ => None,
        }
    }

    /// Evaluate a keyof or constraint type for mapped type iteration.
    fn evaluate_keyof_or_constraint(&mut self, constraint: TypeId) -> TypeId {
        if let Some(TypeData::Conditional(cond_id)) = self.interner().lookup(constraint) {
            let cond = self.interner().conditional_type(cond_id);
            return self.evaluate_conditional(cond.as_ref());
        }

        // If constraint is a literal, return it
        if let Some(TypeData::Literal(LiteralValue::String(_))) = self.interner().lookup(constraint)
        {
            return constraint;
        }

        // If constraint is KeyOf, evaluate it
        if let Some(TypeData::KeyOf(operand)) = self.interner().lookup(constraint) {
            return self.evaluate_keyof(operand);
        }

        // Union: recursively evaluate each member. This handles the distributed form
        // where `(keyof T & keyof U)` after T is inferred becomes
        // `Union(Intersection("x", keyof U), Intersection("y", keyof U))` due to
        // the interner's intersection-over-union distribution. Each Union member
        // (which may be an Intersection) gets recursively simplified.
        if let Some(TypeData::Union(members)) = self.interner().lookup(constraint) {
            let member_list = self.interner().type_list(members);
            let mut evaluated_members = Vec::with_capacity(member_list.len());
            let mut any_changed = false;
            for &member in member_list.iter() {
                let evaluated = self.evaluate_keyof_or_constraint(member);
                if evaluated != member {
                    any_changed = true;
                }
                evaluated_members.push(evaluated);
            }
            if any_changed {
                return self.interner().union(evaluated_members);
            }
            return constraint;
        }

        // Intersection: evaluate each member to get its key set, then compute
        // their intersection. Handles both pre-distribution `keyof T & keyof U`
        // and post-distribution `"x" & keyof U` forms.
        if let Some(TypeData::Intersection(members)) = self.interner().lookup(constraint) {
            let member_list = self.interner().type_list(members);
            let mut key_sets = Vec::with_capacity(member_list.len());
            for &member in member_list.iter() {
                key_sets.push(self.evaluate_keyof_or_constraint(member));
            }
            if let Some(result) = self.intersect_keyof_sets(&key_sets) {
                return result;
            }
            // If intersection computation failed, fall through to general evaluation
        }

        // Evaluate the constraint to resolve type aliases (Lazy), Applications, etc.
        // For example, `type Keys = "a" | "b"; { [P in Keys]: T }` has a Lazy(DefId)
        // constraint that must be evaluated to get the concrete union `"a" | "b"`.
        let evaluated = self.evaluate(constraint);
        if evaluated != constraint {
            return self.evaluate_keyof_or_constraint(evaluated);
        }

        // Otherwise return as-is
        constraint
    }

    /// Extract mapped keys from a type (for mapped type iteration).
    fn extract_mapped_keys(&mut self, type_id: TypeId) -> Option<MappedKeys> {
        let key = self.interner().lookup(type_id)?;

        let mut keys = MappedKeys {
            string_literals: Vec::new(),
            has_string: false,
            has_number: false,
        };

        match key {
            // NEW: Handle KeyOf types directly if evaluate_keyof deferred
            // This fixes Bug #1: Key Remapping with conditionals
            TypeData::KeyOf(operand) => {
                tracing::trace!(
                    operand = operand.0,
                    operand_lookup = ?self.interner().lookup(operand),
                    "extract_mapped_keys: handling KeyOf type"
                );
                // NORTH STAR: Use collect_properties to extract keys from KeyOf operand.
                // This handles interfaces, classes, intersections, and type parameters.
                let prop_result = collect_properties(operand, self.interner(), self.resolver());
                tracing::trace!(
                    operand = operand.0,
                    prop_result = ?std::mem::discriminant(&prop_result),
                    "extract_mapped_keys: collect_properties result"
                );
                match prop_result {
                    PropertyCollectionResult::Properties {
                        properties,
                        string_index,
                        number_index,
                    } => {
                        for prop in properties {
                            keys.string_literals.push(prop.name);
                        }
                        keys.has_string = string_index.is_some();
                        keys.has_number = number_index.is_some();
                        tracing::trace!(
                            string_literals = ?keys.string_literals,
                            has_string = keys.has_string,
                            has_number = keys.has_number,
                            "extract_mapped_keys: extracted keys from KeyOf"
                        );
                        Some(keys)
                    }
                    PropertyCollectionResult::Any => {
                        keys.has_string = true;
                        keys.has_number = true;
                        tracing::trace!("extract_mapped_keys: KeyOf is Any type");
                        Some(keys)
                    }
                    PropertyCollectionResult::NonObject => {
                        // The operand might be an unevaluated Application or other
                        // deferred type (e.g., `PartialProperties<T, K>` as a type alias
                        // Application). Evaluate it first, then retry collect_properties.
                        let evaluated = self.evaluate(operand);
                        if evaluated != operand {
                            let retry_result =
                                collect_properties(evaluated, self.interner(), self.resolver());
                            match retry_result {
                                PropertyCollectionResult::Properties {
                                    properties,
                                    string_index,
                                    number_index,
                                } => {
                                    for prop in properties {
                                        keys.string_literals.push(prop.name);
                                    }
                                    keys.has_string = string_index.is_some();
                                    keys.has_number = number_index.is_some();
                                    tracing::trace!(
                                        string_literals = ?keys.string_literals,
                                        "extract_mapped_keys: extracted keys from evaluated KeyOf operand"
                                    );
                                    return Some(keys);
                                }
                                PropertyCollectionResult::Any => {
                                    keys.has_string = true;
                                    keys.has_number = true;
                                    return Some(keys);
                                }
                                PropertyCollectionResult::NonObject => {}
                            }
                        }
                        tracing::trace!("extract_mapped_keys: KeyOf operand is not an object");
                        None
                    }
                }
            }
            TypeData::Literal(LiteralValue::String(s)) => {
                keys.string_literals.push(s);
                Some(keys)
            }
            TypeData::Union(members) => {
                let members = self.interner().type_list(members);
                for &member in members.iter() {
                    if member == TypeId::STRING {
                        keys.has_string = true;
                        continue;
                    }
                    if member == TypeId::NUMBER {
                        keys.has_number = true;
                        continue;
                    }
                    if member == TypeId::SYMBOL {
                        // We don't model symbol index signatures yet; ignore symbol keys.
                        continue;
                    }
                    // Use visitor helper for data extraction (North Star Rule 3)
                    if let Some(s) = crate::visitor::literal_string(self.interner(), member) {
                        keys.string_literals.push(s);
                    } else if let Some(n) = crate::visitor::literal_number(self.interner(), member)
                    {
                        // Numeric literals become string property names (e.g., 0 → "0").
                        // This handles enum member values like `enum E { A = 0 }`.
                        let s = self.interner().intern_string(
                            &crate::relations::subtype::rules::literals::format_number_for_template(
                                n.0,
                            ),
                        );
                        keys.string_literals.push(s);
                    } else if let Some(inner_keys) = self.extract_mapped_keys(member) {
                        // Recursively extract keys from non-literal union members.
                        // Handles enum types (TypeData::Enum), lazy refs (TypeData::Lazy),
                        // and nested unions (e.g., `A | B` where A, B are enum types).
                        keys.string_literals.extend(inner_keys.string_literals);
                        keys.has_string |= inner_keys.has_string;
                        keys.has_number |= inner_keys.has_number;
                    } else {
                        // Non-literal in union - can't fully evaluate
                        return None;
                    }
                }
                if !keys.has_string && !keys.has_number && keys.string_literals.is_empty() {
                    // Only symbol keys (or nothing) - defer until we support symbol indices.
                    return None;
                }
                Some(keys)
            }
            TypeData::Intrinsic(IntrinsicKind::String) => {
                keys.has_string = true;
                Some(keys)
            }
            TypeData::Intrinsic(IntrinsicKind::Number) => {
                keys.has_number = true;
                Some(keys)
            }
            TypeData::Intrinsic(IntrinsicKind::Never) => {
                // Mapped over `never` yields an empty object.
                Some(keys)
            }
            TypeData::Enum(_def_id, members) => {
                // Enum used as mapped type constraint: extract keys from member union.
                // For `enum E { A, B }`, members is the union `0 | 1`, and the keys
                // are the enum values. Recursively extract from the members type.
                self.extract_mapped_keys(members)
            }
            TypeData::Lazy(def_id) => {
                // Lazy type reference (e.g., type alias `AB = A | B`): resolve and recurse.
                if let Some(resolved) = self.resolver().resolve_lazy(def_id, self.interner())
                    && resolved != type_id
                {
                    return self.extract_mapped_keys(resolved);
                }
                None
            }
            // Can't extract literals from other types
            _ => None,
        }
    }

    /// Check if a mapped type is homomorphic (template is T[K] indexed access).
    /// Returns `Some(source)` with the source type T if homomorphic, `None` otherwise.
    /// Homomorphic mapped types preserve modifiers from the source type.
    ///
    /// A mapped type is homomorphic if:
    /// 1. The constraint is `keyof T` for some type T
    /// 2. The template is `T[K]` where T is the same type and K is the iteration parameter
    ///
    /// Also handles the post-instantiation case where the `keyof T` constraint was
    /// eagerly evaluated to a union of string literals during `instantiate_type`.
    /// In that case, we verify that `template = obj[P]` and `keyof obj == constraint`.
    fn homomorphic_mapped_source(&mut self, mapped: &MappedType) -> Option<TypeId> {
        // Method 1: Constraint is explicitly `keyof T` (pre-evaluation form)
        if let Some(source_from_constraint) = self.extract_source_from_keyof(mapped.constraint) {
            // Check if template is an IndexAccess type T[K]
            return match self.interner().lookup(mapped.template) {
                Some(TypeData::IndexAccess(obj, idx)) => {
                    if obj != source_from_constraint {
                        return None;
                    }
                    match self.interner().lookup(idx) {
                        Some(TypeData::TypeParameter(param)) => {
                            if param.name == mapped.type_param.name {
                                Some(source_from_constraint)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
        }

        // Method 2: Post-instantiation form where `keyof T` was eagerly evaluated
        // to a union of string literals. The template still has the original structure
        // `T[P]` with the concrete object. Verify by computing `keyof obj` and
        // comparing with the constraint.
        // Key remapping (`as` clause / name_type) breaks homomorphism,
        // UNLESS the name type is an identity mapping (as K where K is the param).
        let is_identity_or_no_name = mapped.name_type.is_none()
            || mapped.name_type.is_some_and(|nt| {
                matches!(
                    self.interner().lookup(nt),
                    Some(TypeData::TypeParameter(param)) if param.name == mapped.type_param.name
                )
            });
        if is_identity_or_no_name
            && let Some(TypeData::IndexAccess(obj, idx)) = self.interner().lookup(mapped.template)
            && let Some(TypeData::TypeParameter(param)) = self.interner().lookup(idx)
            && param.name == mapped.type_param.name
        {
            // Don't match if obj is still a type parameter (not yet instantiated)
            if matches!(
                self.interner().lookup(obj),
                Some(TypeData::TypeParameter(_))
            ) {
                return None;
            }
            // Verify: the constraint is exactly the keys of obj
            let expected_keys = self.evaluate_keyof(obj);
            if expected_keys == mapped.constraint {
                return Some(obj);
            }
        }

        None
    }

    /// Extract the source type T from a `keyof T` constraint.
    /// Handles aliased constraints like `type Keys<T> = keyof T`,
    /// and intersection constraints like `keyof T & keyof U` (returns first keyof source).
    fn extract_source_from_keyof(&mut self, constraint: TypeId) -> Option<TypeId> {
        match self.interner().lookup(constraint) {
            Some(TypeData::KeyOf(source)) => Some(source),
            // Handle aliased constraints (Application)
            Some(TypeData::Application(_)) => {
                // Evaluate to resolve the alias
                let evaluated = self.evaluate(constraint);
                // Recursively check the evaluated type
                if evaluated != constraint {
                    self.extract_source_from_keyof(evaluated)
                } else {
                    None
                }
            }
            // Handle intersection constraints like `keyof T & keyof U`.
            // Return the first keyof source found (for property lookup/modifier preservation).
            Some(TypeData::Intersection(members)) => {
                let member_list = self.interner().type_list(members);
                for &member in member_list.iter() {
                    if let Some(source) = self.extract_source_from_keyof(member) {
                        return Some(source);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Evaluate a homomorphic mapped type over an Array type.
    ///
    /// For example: `type Partial<T> = { [P in keyof T]?: T[P] }`
    ///   `Partial<number[]>` should produce `(number | undefined)[]`
    ///
    /// We instantiate the template with `K = number` to get the mapped element type.
    fn evaluate_mapped_array(&mut self, mapped: &MappedType, _element_type: TypeId) -> TypeId {
        // Create substitution: type_param.name -> number
        let mut subst = TypeSubstitution::new();
        subst.insert(mapped.type_param.name, TypeId::NUMBER);

        // Substitute into the template to get the mapped element type
        let mut mapped_element =
            self.evaluate(instantiate_type(self.interner(), mapped.template, &subst));

        // CRITICAL: Handle optional modifier (Partial<T[]> case)
        // TypeScript adds undefined to the element type when ? modifier is present
        if matches!(mapped.optional_modifier, Some(MappedModifier::Add)) {
            mapped_element = self.interner().union2(mapped_element, TypeId::UNDEFINED);
        }

        // Check if readonly modifier should be applied
        let is_readonly = matches!(mapped.readonly_modifier, Some(MappedModifier::Add));

        // Create the new array type
        if is_readonly {
            // Wrap the array type in ReadonlyType to get readonly semantics
            let array_type = self.interner().array(mapped_element);
            self.interner().readonly_type(array_type)
        } else {
            self.interner().array(mapped_element)
        }
    }

    /// Evaluate a homomorphic mapped type over an Array type with explicit readonly flag.
    ///
    /// Used for `ReadonlyArray`<T> to preserve readonly semantics.
    fn evaluate_mapped_array_with_readonly(
        &mut self,
        mapped: &MappedType,
        _element_type: TypeId,
        is_readonly: bool,
    ) -> TypeId {
        // Create substitution: type_param.name -> number
        let mut subst = TypeSubstitution::new();
        subst.insert(mapped.type_param.name, TypeId::NUMBER);

        // Substitute into the template to get the mapped element type
        let mut mapped_element =
            self.evaluate(instantiate_type(self.interner(), mapped.template, &subst));

        // CRITICAL: Handle optional modifier (Partial<T[]> case)
        if matches!(mapped.optional_modifier, Some(MappedModifier::Add)) {
            mapped_element = self.interner().union2(mapped_element, TypeId::UNDEFINED);
        }

        // Apply readonly modifier if present
        let final_readonly = match mapped.readonly_modifier {
            Some(MappedModifier::Add) => true,
            Some(MappedModifier::Remove) => false,
            None => is_readonly, // Preserve original readonly status
        };

        if final_readonly {
            // Wrap the array type in ReadonlyType to get readonly semantics
            let array_type = self.interner().array(mapped_element);
            self.interner().readonly_type(array_type)
        } else {
            self.interner().array(mapped_element)
        }
    }

    /// Evaluate a homomorphic mapped type over a Tuple type.
    ///
    /// For example: `type Partial<T> = { [P in keyof T]?: T[P] }`
    ///   `Partial<[number, string]>` should produce `[number?, string?]`
    ///
    /// We instantiate the template with `K = 0, 1, 2...` for each tuple element.
    /// This preserves tuple structure including optional and rest elements.
    fn evaluate_mapped_tuple(&mut self, mapped: &MappedType, tuple_id: TupleListId) -> TypeId {
        use crate::types::TupleElement;

        let tuple_elements = self.interner().tuple_list(tuple_id);
        let mut mapped_elements = Vec::new();

        for (i, elem) in tuple_elements.iter().enumerate() {
            // CRITICAL: Handle rest elements specially
            // For rest elements (...T[]), we cannot use index substitution.
            // We must map the array type itself.
            if elem.rest {
                // Rest elements like ...number[] need to be mapped as arrays
                // Check if the rest type is an Array
                let rest_type = elem.type_id;
                let mapped_rest_type = match self.interner().lookup(rest_type) {
                    Some(TypeData::Array(inner_elem)) => {
                        // Map the inner array element
                        // Reuse the array mapping logic
                        self.evaluate_mapped_array(mapped, inner_elem)
                    }
                    Some(TypeData::Tuple(inner_tuple_id)) => {
                        // Nested tuple in rest - recurse
                        self.evaluate_mapped_tuple(mapped, inner_tuple_id)
                    }
                    _ => {
                        // Fallback: try index substitution (may not work correctly)
                        let index_type = self.interner().literal_number(i as f64);
                        let mut subst = TypeSubstitution::new();
                        subst.insert(mapped.type_param.name, index_type);
                        self.evaluate(instantiate_type(self.interner(), mapped.template, &subst))
                    }
                };

                // Handle optional modifier for rest elements
                let final_rest_type =
                    if matches!(mapped.optional_modifier, Some(MappedModifier::Add)) {
                        self.interner().union2(mapped_rest_type, TypeId::UNDEFINED)
                    } else {
                        mapped_rest_type
                    };

                mapped_elements.push(TupleElement {
                    type_id: final_rest_type,
                    name: elem.name,
                    optional: elem.optional,
                    rest: true,
                });
                continue;
            }

            // Non-rest elements: use index substitution
            // Create a literal number type for this tuple position
            let index_type = self.interner().literal_number(i as f64);

            // Create substitution: type_param.name -> literal number
            let mut subst = TypeSubstitution::new();
            subst.insert(mapped.type_param.name, index_type);

            // Substitute into the template to get the mapped element type
            let mapped_type =
                self.evaluate(instantiate_type(self.interner(), mapped.template, &subst));

            // Get the modifiers for this element
            // Note: readonly is currently unused for tuple elements, but we preserve the logic
            // in case TypeScript adds readonly tuple element support in the future
            // CRITICAL: Handle optional and readonly modifiers independently
            let optional = match mapped.optional_modifier {
                Some(MappedModifier::Add) => true,
                Some(MappedModifier::Remove) => false,
                None => elem.optional, // Preserve original optional
            };
            // Note: readonly modifier is intentionally ignored for tuple elements,
            // as TypeScript doesn't support readonly on individual tuple elements.

            mapped_elements.push(TupleElement {
                type_id: mapped_type,
                name: elem.name,
                optional,
                rest: elem.rest,
            });
        }

        self.interner().tuple(mapped_elements)
    }
}
