//! Mapped type evaluation.
//!
//! Handles TypeScript's mapped types: `{ [K in keyof T]: T[K] }`
//! Including homomorphic mapped types that preserve modifiers.

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::objects::{PropertyCollectionResult, collect_properties};
use crate::relations::subtype::{SubtypeChecker, TypeResolver};
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

        // Delegate to centralized modifier computation in type_queries.
        crate::type_queries::compute_mapped_modifiers(
            mapped,
            is_homomorphic,
            source_mods.0,
            source_mods.1,
        )
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
            return self.interner().mapped(*mapped);
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
                // When key extraction fails but the mapped type has an `as` clause
                // and the constraint is a concrete union (of non-literal types like
                // objects), we can still evaluate by iterating over the constraint
                // members directly. Each member is substituted into both the `as`
                // clause (to derive the property name) and the template (to get the
                // property type).
                //
                // Example: { [Item in ({name:"a"} | {name:"b"}) as Item['name']]: Item }
                // → { a: {name:"a"}, b: {name:"b"} }
                if mapped.name_type.is_some()
                    && let Some(result) =
                        self.try_evaluate_mapped_with_as_over_non_literal_constraint(mapped, keys)
                {
                    return result;
                }
                tracing::trace!(
                    keys_lookup = ?self.interner().lookup(keys),
                    "evaluate_mapped: DEFERRED - could not extract concrete keys"
                );
                return self.interner().mapped(*mapped);
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
        // True identity homomorphic: template is T[K] and constraint is keyof T.
        // Used for declared-type substitution (avoid double-encoding optionality).
        let is_identity_homomorphic = homomorphic_source.is_some();

        // For homomorphic types, source comes from the homomorphic check.
        // For non-homomorphic types, still try extracting from keyof for array/tuple preservation.
        let source_object =
            homomorphic_source.or_else(|| self.extract_source_from_keyof(mapped.constraint));

        // tsc treats ANY `{ [K in keyof T]: ... }` as homomorphic for modifier
        // inheritance — the source T's optional/readonly flags propagate to the
        // output even when the template is NOT `T[K]`. For example:
        //   type M1 = { [K in keyof Partial<M0>]: M0[K] }
        // inherits optionality from Partial<M0>'s properties, even though the
        // template is `M0[K]`, not `Partial<M0>[K]`.
        let is_homomorphic = source_object.is_some();

        // PERF: Memoize source properties into a hash map for O(1) lookup during the key loop.
        // This avoids repeated O(N) collect_properties calls inside the loop.
        // Also capture resolved_source once to avoid double evaluate(source) calls.
        let mut source_prop_map = FxHashMap::default();
        let mut resolved_source_id = None;
        if let Some(source) = source_object {
            // Evaluate the source to resolve Application types (e.g., Partial<X> is
            // Application(Partial, [X]) which evaluates to { prop?: ... }). Without
            // this, collect_properties can't extract properties from unevaluated
            // Applications, causing optional/readonly modifiers to be lost.
            let resolved_source = self.evaluate(source);
            resolved_source_id = Some(resolved_source);

            // When a homomorphic mapped type has `any` as its source, the normal
            // key expansion path handles it correctly: `keyof any` = `string | number | symbol`,
            // which produces an object with string+number index signatures.
            // This matches tsc's behavior for both `Objectish<any>` and non-identity
            // homomorphic types like `{ [K in keyof T]: string }` with T=any.
            //
            // Previously this returned TypeId::ANY, which was incorrect for the
            // `Objectish<any>` case and required a checker-local workaround.

            match collect_properties(resolved_source, self.interner(), self.resolver()) {
                PropertyCollectionResult::Properties { properties, .. } => {
                    source_prop_map.reserve(properties.len());
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
        // PERF: Reuse resolved_source_id from above to avoid re-evaluating source.
        let source_decl_order: Vec<Atom> = if is_homomorphic {
            if let Some(resolved) = resolved_source_id {
                let order: Vec<Atom> = match self.interner().lookup(resolved) {
                    Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                        let shape = self.interner().object_shape(shape_id);
                        let mut props: Vec<&PropertyInfo> = shape.properties.iter().collect();
                        props.sort_by_key(|p| p.declaration_order);
                        props.iter().map(|p| p.name).collect()
                    }
                    _ => Vec::new(),
                };
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
        let mut properties = Vec::with_capacity(key_set.string_literals.len());
        // PERF: Reuse a single TypeSubstitution across all keys to avoid
        // re-allocating the inner FxHashMap on every iteration.
        let mut subst = TypeSubstitution::new();

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
                Err(()) => return self.interner().mapped(*mapped),
            };
            // Extract property name(s) from remapped key.
            // Handle unions: `as \`${K}1\` | \`${K}2\`` produces multiple properties per key.
            let remapped_names: smallvec::SmallVec<[Atom; 1]> =
                if let Some(name) = crate::visitor::literal_string(self.interner(), remapped) {
                    smallvec::smallvec![name]
                } else if let Some(TypeData::Union(list_id)) = self.interner().lookup(remapped) {
                    let members = self.interner().type_list(list_id);
                    let names: smallvec::SmallVec<[Atom; 1]> = members
                        .iter()
                        .filter_map(|&m| crate::visitor::literal_string(self.interner(), m))
                        .collect();
                    if names.is_empty() {
                        return self.interner().mapped(*mapped);
                    }
                    names
                } else {
                    return self.interner().mapped(*mapped);
                };

            // Get modifiers for this specific key (preserves homomorphic behavior)
            // Use memoized source property info for O(1) lookup.
            // Delegate to centralized modifier computation in type_queries.
            let source_info = source_prop_map.get(&key_name);
            let (source_optional, source_readonly) =
                source_info.map_or((false, false), |(opt, ro, _)| (*opt, *ro));

            let (optional, readonly) = crate::type_queries::compute_mapped_modifiers(
                mapped,
                is_homomorphic,
                source_optional,
                source_readonly,
            );

            // PERF: For identity homomorphic mapped types (template is `T[P]`),
            // skip the expensive instantiate_type + evaluate cycle when source
            // property info is available. The declared type IS the property type
            // (with optionality handled by the modifier, not by the type itself).
            // For non-optional properties in identity homomorphic types, the
            // evaluated T[K] equals the declared type, so we can also skip.
            let property_type =
                if is_identity_homomorphic && let Some(&(_, _, declared_type)) = source_info {
                    declared_type
                } else {
                    subst.clear();
                    subst.insert(mapped.type_param.name, key_literal);

                    // Substitute into the template
                    let instantiated_template =
                        instantiate_type(self.interner(), mapped.template, &subst);
                    let evaluated = self.evaluate(instantiated_template);

                    // Check if evaluation hit depth limit
                    if evaluated == TypeId::ERROR && self.is_depth_exceeded() {
                        return TypeId::ERROR;
                    }
                    evaluated
                };

            for remapped_name in remapped_names {
                properties.push(PropertyInfo {
                    name: remapped_name,
                    type_id: property_type,
                    write_type: property_type,
                    optional,
                    readonly,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                    is_string_named: false,
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
                        return self.interner().mapped(*mapped);
                    }
                    let key_type = TypeId::STRING;
                    subst.clear();
                    subst.insert(mapped.type_param.name, key_type);
                    let instantiated_template =
                        instantiate_type(self.interner(), mapped.template, &subst);
                    let mut value_type = self.evaluate(instantiated_template);

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
                Err(()) => return self.interner().mapped(*mapped),
            }
        } else {
            None
        };

        let number_index = if key_set.has_number {
            match self.remap_key_type_for_mapped(mapped, TypeId::NUMBER) {
                Ok(Some(remapped)) => {
                    if remapped != TypeId::NUMBER {
                        return self.interner().mapped(*mapped);
                    }
                    let key_type = TypeId::NUMBER;
                    subst.clear();
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
                Err(()) => return self.interner().mapped(*mapped),
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

    /// Evaluate a mapped type with an `as` clause when the constraint is a union of
    /// non-literal types (e.g., objects). Instead of extracting string literal keys,
    /// iterate over the constraint union members directly and evaluate the `as` clause
    /// for each to derive property names.
    ///
    /// Example: `{ [Item in ({name:"a"} | {name:"b"}) as Item['name']]: Item }`
    /// → `{ a: {name:"a"}, b: {name:"b"} }`
    fn try_evaluate_mapped_with_as_over_non_literal_constraint(
        &mut self,
        mapped: &MappedType,
        evaluated_constraint: TypeId,
    ) -> Option<TypeId> {
        let name_type = mapped.name_type?;

        // Extract union members from the constraint
        let members: Vec<TypeId> =
            if let Some(TypeData::Union(list_id)) = self.interner().lookup(evaluated_constraint) {
                self.interner().type_list(list_id).to_vec()
            } else {
                // Single non-literal member
                vec![evaluated_constraint]
            };

        // Verify all members are concrete (no type parameters)
        for &member in &members {
            if matches!(
                self.interner().lookup(member),
                Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
            ) {
                return None;
            }
        }

        // Limit to prevent OOM
        if members.len() > 500 {
            return None;
        }

        let mut properties = Vec::new();
        let mut subst = TypeSubstitution::new();

        for &member in &members {
            if self.is_depth_exceeded() {
                return Some(TypeId::ERROR);
            }

            // Substitute the constraint member (e.g., {name:"a"}) for the type parameter
            subst.clear();
            subst.insert(mapped.type_param.name, member);

            // Evaluate the `as` clause to get the remapped key
            let remapped_key = self.evaluate(instantiate_type(self.interner(), name_type, &subst));

            // If remapped key is `never`, skip this member (filtered out)
            if remapped_key == TypeId::NEVER {
                continue;
            }

            // Extract property name(s) from remapped key
            let remapped_names: smallvec::SmallVec<[Atom; 1]> = if let Some(name) =
                crate::visitor::literal_string(self.interner(), remapped_key)
            {
                smallvec::smallvec![name]
            } else if let Some(TypeData::Union(list_id)) = self.interner().lookup(remapped_key) {
                let key_members = self.interner().type_list(list_id);
                let names: smallvec::SmallVec<[Atom; 1]> = key_members
                    .iter()
                    .filter_map(|&m| crate::visitor::literal_string(self.interner(), m))
                    .collect();
                if names.is_empty() {
                    return None; // Can't resolve to concrete names
                }
                names
            } else {
                return None; // Can't resolve to concrete name
            };

            // Evaluate the template with the substitution
            let instantiated_template = instantiate_type(self.interner(), mapped.template, &subst);
            let property_type = self.evaluate(instantiated_template);

            if property_type == TypeId::ERROR && self.is_depth_exceeded() {
                return Some(TypeId::ERROR);
            }

            // Compute modifiers
            let (optional, readonly) = crate::type_queries::compute_mapped_modifiers(
                mapped, false, // not homomorphic (no source to inherit from)
                false, false,
            );

            for remapped_name in remapped_names {
                properties.push(PropertyInfo {
                    name: remapped_name,
                    type_id: property_type,
                    write_type: property_type,
                    optional,
                    readonly,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                    is_string_named: false,
                });
            }
        }

        Some(self.interner().object(properties))
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
            let inner_mapped = self.interner().get_mapped(inner_mapped_id);
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
            // ReadonlyType wrapping Array or Tuple: `readonly T[]` or `readonly [a, b]`
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
        // PERF: Single lookup handles all cases instead of 4 separate DashMap lookups.
        let members = match self.interner().lookup(constraint) {
            Some(TypeData::Conditional(cond_id)) => {
                let cond = self.interner().get_conditional(cond_id);
                return self.evaluate_conditional(&cond);
            }
            Some(TypeData::Literal(LiteralValue::String(_))) => {
                return constraint;
            }
            Some(TypeData::KeyOf(operand)) => {
                return self.evaluate_keyof(operand);
            }
            Some(TypeData::Union(members)) => Some(members),
            _ => None,
        };

        // Union: recursively evaluate each member. This handles the distributed form
        // where `(keyof T & keyof U)` after T is inferred becomes
        // `Union(Intersection("x", keyof U), Intersection("y", keyof U))` due to
        // the interner's intersection-over-union distribution. Each Union member
        // (which may be an Intersection) gets recursively simplified.
        if let Some(members) = members {
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
            // Numeric literals become string property names (e.g., enum value 0 → "0").
            // This handles the case where a single-member enum is used as a mapped type
            // constraint: `Record<E, any>` where `enum E { A = 0 }` produces constraint
            // Enum(_, Literal(Number(0))) → key "0".
            TypeData::Literal(LiteralValue::Number(n)) => {
                let s = self.interner().intern_string(
                    &crate::relations::subtype::rules::literals::format_number_for_template(n.0),
                );
                keys.string_literals.push(s);
                Some(keys)
            }
            // `AB[K]` in mapped constraints: resolve to the union of property
            // value types for index keys compatible with K, then recurse.
            TypeData::IndexAccess(object_type, index_type) => {
                // If index access can be simplified, recurse into the result.
                let evaluated = self.evaluate(type_id);
                if evaluated != type_id {
                    return self.extract_mapped_keys(evaluated);
                }

                let mut checker = SubtypeChecker::with_resolver(self.interner(), self.resolver());

                match collect_properties(object_type, self.interner(), self.resolver()) {
                    PropertyCollectionResult::Properties {
                        properties,
                        string_index,
                        number_index,
                    } => {
                        let mut members = Vec::new();

                        // Match literal property keys against the index constraint.
                        for prop in properties {
                            let prop_key = self.interner().literal_string(
                                self.interner().resolve_atom_ref(prop.name).as_ref(),
                            );
                            if checker.is_assignable_to(prop_key, index_type) {
                                members.push(prop.type_id);
                            }
                        }

                        // Index signatures are only used as a fallback if they are
                        // directly addressed by the index constraint.
                        if let Some(string_sig) = string_index
                            && checker.is_assignable_to(string_sig.key_type, index_type)
                        {
                            members.push(string_sig.value_type);
                        }
                        if let Some(number_sig) = number_index
                            && checker.is_assignable_to(number_sig.key_type, index_type)
                        {
                            members.push(number_sig.value_type);
                        }

                        if members.is_empty() {
                            return None;
                        }

                        let value_union = if members.len() == 1 {
                            members[0]
                        } else {
                            self.interner().union(members)
                        };

                        self.extract_mapped_keys(value_union)
                    }
                    PropertyCollectionResult::Any | PropertyCollectionResult::NonObject => None,
                }
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
            TypeData::Intersection(members) => {
                // Intersection of key sets: compute the intersection of extracted keys
                // from each member. This handles constraints like `keyof T & keyof U`
                // that remain as Intersection after evaluate_keyof_or_constraint.
                let member_list = self.interner().type_list(members);
                let mut member_keys: Vec<MappedKeys> = Vec::with_capacity(member_list.len());
                for &member in member_list.iter() {
                    match self.extract_mapped_keys(member) {
                        Some(mk) => member_keys.push(mk),
                        None => return None,
                    }
                }
                if member_keys.is_empty() {
                    return None;
                }
                // Start with the first member's keys and intersect with the rest.
                let mut result = member_keys.remove(0);
                for other in &member_keys {
                    // For string/number index: intersection means both must have it.
                    result.has_string = result.has_string && other.has_string;
                    result.has_number = result.has_number && other.has_number;
                    // For string literals: keep only those present in both sets.
                    // If one side has `has_string` (string index signature), all
                    // literals from the other side are kept (since string encompasses them).
                    if other.has_string {
                        // Other side accepts all strings, so keep result's literals.
                    } else if result.has_string {
                        // Result side accepts all strings, take other's literals.
                        result.string_literals = other.string_literals.clone();
                        result.has_string = false; // Narrowed to specific literals.
                    } else {
                        // Both have specific literals: keep only the intersection.
                        let other_set: rustc_hash::FxHashSet<_> =
                            other.string_literals.iter().copied().collect();
                        result.string_literals.retain(|lit| other_set.contains(lit));
                    }
                }
                if !result.has_string && !result.has_number && result.string_literals.is_empty() {
                    // Intersection is empty — produces empty object.
                    // Still return Some so we generate an empty object type rather than deferring.
                }
                Some(result)
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
            // Verify: the constraint is the keys of obj (exact match or subset).
            // Exact match handles `{ [P in keyof T]: T[P] }` after instantiation.
            // Subset match handles Pick/Omit where constraint is a filtered subset
            // of `keyof T` (e.g., `Exclude<keyof T, K>` evaluates to a subset of keys).
            // In both cases, the mapped type is homomorphic w.r.t. obj so modifiers
            // (readonly, optional) should be inherited from source properties.
            let expected_keys = self.evaluate_keyof(obj);
            if expected_keys == mapped.constraint {
                return Some(obj);
            }
            // Subset check: all constraint keys must exist in keyof obj.
            // Use the already-evaluated keys (from evaluate_keyof_or_constraint
            // at the top of evaluate_mapped) rather than the raw constraint.
            // The raw constraint may be an unevaluated Application type (e.g.,
            // Exclude<keyof T, K>) that extract_mapped_keys can't handle,
            // but the evaluated keys are a concrete union of string literals.
            let evaluated_constraint = self.evaluate_keyof_or_constraint(mapped.constraint);
            if let (Some(constraint_keys), Some(expected_key_set)) = (
                self.extract_mapped_keys(evaluated_constraint),
                self.extract_mapped_keys(expected_keys),
            ) {
                // Only do subset check for pure string literal keys (no string/number index)
                if !constraint_keys.has_string
                    && !constraint_keys.has_number
                    && !constraint_keys.string_literals.is_empty()
                {
                    let expected_set: rustc_hash::FxHashSet<Atom> =
                        expected_key_set.string_literals.iter().copied().collect();
                    let is_subset = constraint_keys
                        .string_literals
                        .iter()
                        .all(|k| expected_set.contains(k));
                    if is_subset {
                        return Some(obj);
                    }
                }
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
