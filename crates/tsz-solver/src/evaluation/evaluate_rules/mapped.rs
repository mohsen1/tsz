//! Mapped type evaluation.
//! Handles TypeScript's mapped types: `{ [K in keyof T]: T[K] }`
//! Including homomorphic mapped types that preserve modifiers.

mod display_order;
mod key_extraction;
mod key_types;
mod keyof_constraint;
mod keys_guard;

use crate::construction::TypeDatabase;
use crate::instantiation::instantiate::{
    TypeSubstitution, instantiate_type_cached, instantiate_type_preserving_cached,
};
use crate::intern::TEMPLATE_LITERAL_EXPANSION_LIMIT;
use crate::objects::PropertyCollectionResult;
use crate::relations::subtype::TypeResolver;
use crate::types::Visibility;
use crate::types::{
    IndexSignature, IntrinsicKind, MappedModifier, MappedType, ObjectFlags, ObjectShape,
    PropertyInfo, TypeData, TypeId,
};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;

#[cfg(test)]
mod mapped_tests;
#[cfg(test)]
mod tests;
pub(crate) use key_types::{MappedKey, MappedKeys};

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Partition `properties` from `collect_properties` into string, numeric, and symbol key buckets.
    /// Reuses the existing `unique_symbol_ref_from_symbol_named_atom` helper to avoid
    /// duplicating `__unique_N` / well-known-symbol atom conversion logic.
    fn collect_props_into_keys(&self, keys: &mut MappedKeys, properties: Vec<PropertyInfo>) {
        for prop in properties {
            if prop.is_symbol_named {
                if let Some(sym_ref) = self.unique_symbol_ref_from_symbol_named_atom(prop.name) {
                    keys.symbol_keys
                        .push(self.interner().unique_symbol(sym_ref));
                }
            } else {
                keys.keys.push(self.mapped_key_from_property(&prop));
            }
        }
    }

    /// Helper for key remapping in mapped types.
    /// Returns Ok(Some(remapped)) if remapping succeeded,
    /// Ok(None) if the key should be filtered (remapped to never),
    /// Err(()) if we can't process and should return the original mapped type.
    #[tracing::instrument(level = "trace", skip(self), fields(
        param_name = ?mapped.type_param.name,
        key_type = key_type.0,
        has_name_type = mapped.name_type.is_some(),
    ))]
    pub(crate) fn remap_key_type_for_mapped(
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

        let subst = TypeSubstitution::single(mapped.type_param.name, key_type);
        let remapped =
            instantiate_type_preserving_cached(self.interner(), self.query_db(), name_type, &subst);

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

    /// Helper to compute modifiers for a mapped type *index signature*.
    ///
    /// A homomorphic mapped type (`{ [K in keyof T]: ... }`) inherits the source
    /// `readonly` modifier. For named properties that source modifier lives on
    /// the property symbol, but for an **index signature** it lives in the source
    /// object's `string_index` / `number_index` slot — never in the named
    /// property list. Reading it from the property list (the old behavior) always
    /// returned `false`, so homomorphic maps silently dropped a source index
    /// signature's `readonly` intent (e.g. `{ [K in keyof T]: T[K] }` over
    /// `{ readonly [k: string]: V }` produced a writable index signature). We read
    /// the modifier from the correct index slot instead.
    ///
    /// Index signatures are never optional in tsc, so the source-optional input
    /// is always `false`; optionality on an index signature can only come from an
    /// explicit `+?` / `-?` directive on the mapped type itself.
    fn get_mapped_modifiers(
        &mut self,
        mapped: &MappedType,
        inherits_modifiers: bool,
        source_object: Option<TypeId>,
        key_type: TypeId,
    ) -> (bool, bool) {
        let source_readonly = source_object
            .map(|source_obj| self.source_index_signature_readonly(source_obj, key_type))
            .unwrap_or(false);

        // Delegate to centralized modifier computation in type_queries.
        crate::type_queries::compute_mapped_modifiers(
            mapped,
            inherits_modifiers,
            false,
            source_readonly,
        )
    }

    /// Read the `readonly` flag of the source object's index signature that
    /// covers `key_type`. A numeric key is also serviced by a string index
    /// signature, so when the source lacks a dedicated numeric index signature we
    /// fall back to the string one (mirroring tsc, where a string index info
    /// applies to number-like keys). The symmetric fallback keeps template-literal
    /// remapped keys, which model as string-like index signatures, aligned with a
    /// lone numeric source index signature.
    fn source_index_signature_readonly(&self, source_object: TypeId, key_type: TypeId) -> bool {
        match crate::objects::collect_properties_cached(
            source_object,
            self.interner(),
            self.resolver(),
            self.query_db(),
        ) {
            PropertyCollectionResult::Properties {
                string_index,
                number_index,
                ..
            } => {
                // A numeric key is serviced by the numeric index signature when
                // present, otherwise by the string one; string keys prefer the
                // string slot with the symmetric fallback.
                let (primary, fallback) = if key_type == TypeId::NUMBER {
                    (number_index, string_index)
                } else {
                    (string_index, number_index)
                };
                primary.or(fallback).is_some_and(|idx| idx.readonly)
            }
            _ => false,
        }
    }

    /// Strip the top-level `undefined` that an originally-optional property
    /// contributes when `-?` removes its optionality, mirroring tsc's
    /// `removeMissingOrUndefinedType` in `getTypeOfMappedSymbol`.
    ///
    /// With `exactOptionalPropertyTypes`, an explicit `| undefined` is not the
    /// missing marker and must be preserved. tsz does not model that marker
    /// separately yet, so only the non-exact path can safely remove
    /// `undefined`. The sibling mapped-array shard uses the same rule for
    /// tuple element mapping.
    pub(super) fn strip_removed_optional_undefined(&self, ty: TypeId, strip: bool) -> TypeId {
        if strip && !self.interner().exact_optional_property_types() {
            crate::narrowing::utils::remove_undefined(self.interner(), ty)
        } else {
            ty
        }
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
        if let Some(name_type) = mapped.name_type
            && (crate::type_queries::contains_type_parameters_db(self.interner(), constraint)
                || crate::type_queries::contains_type_parameters_except_name_db(
                    self.interner(),
                    name_type,
                    mapped.type_param.name,
                ))
        {
            tracing::trace!(
                constraint = ?self.interner().lookup(constraint),
                name_type = ?self.interner().lookup(name_type),
                "evaluate_mapped: DEFERRED - generic remapped mapped type"
            );
            return self.interner().mapped(*mapped);
        }

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

        if let Some(distributed) = self.try_distribute_mapped_over_composite_source(mapped) {
            return distributed;
        }

        // tsc's `instantiateMappedType` short-circuit: a generic homomorphic
        // mapped type instantiated with a non-object source (primitive, literal,
        // `never`, unique symbol, enum) reduces to that source. The check
        // distinguishes this from directly-written `{ [K in keyof string]: ... }`
        // by inspecting the iteration variable's *original* constraint.
        if let Some(reduced) = self.try_reduce_substituted_homomorphic_mapped(mapped) {
            return reduced;
        }

        // Issue #6814: `interner.union` collapses `"foo" | string | number`
        // into `string | number`, so the eager-eval path below loses literal
        // keys that an `as` clause must filter per-iteration. Rescue them when
        // the constraint is `keyof T` and T combines named properties with a
        // string/number index signature.
        let keys = self.evaluate_keyof_or_constraint(constraint);

        // If we can't determine concrete keys, keep it as a mapped type (deferred).
        // The any-constraint case (`{ [P in any]: never }`) is intentionally kept as a
        // special path only for the `never` template to avoid changing the evaluated
        // representation of `{ [P in any]: V }` for non-never V (which would alter
        // type display in error messages and cause conformance regressions).
        // Subtype checking against `{ [P in any]: V }` is handled by `try_expand_mapped`.
        let mut key_set = if constraint == TypeId::ANY
            && mapped.name_type.is_none()
            && mapped.template == TypeId::NEVER
        {
            MappedKeys {
                keys: Vec::new(),
                has_string: true,
                has_number: true,
                has_symbol: true,
                template_literals: Vec::new(),
                symbol_keys: Vec::new(),
            }
        } else {
            match self
                .try_extract_keyof_keys_for_mapped_iteration(constraint)
                .or_else(|| self.extract_mapped_keys(keys))
            {
                Some(mut keys) => {
                    // Deduplicate string literals to handle overlapping enum members
                    // (e.g. `enum A { CAT = "cat" }` and `enum B { CAT = "cat" }` both
                    // produce key "cat") while preserving the original declaration
                    // order from the constraint. tsc walks the constraint union in
                    // source order, so the resulting mapped type's property order —
                    // and therefore the type printer's output for `T[keyof T]` —
                    // must follow that same order.
                    let mut seen: FxHashSet<Atom> = FxHashSet::default();
                    keys.keys.retain(|k| seen.insert(k.name));
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
                        && let Some(result) = self
                            .try_evaluate_mapped_with_as_over_non_literal_constraint(mapped, keys)
                    {
                        return result;
                    }
                    tracing::trace!(
                        keys_lookup = ?self.interner().lookup(keys),
                        "evaluate_mapped: DEFERRED - could not extract concrete keys"
                    );
                    return self.interner().mapped(*mapped);
                }
            }
        };

        // Limit number of keys to prevent OOM with large mapped types.
        // WASM environments have limited memory, but 100 is too restrictive for
        // real-world code (large SDKs, generated API types often have 150-250 keys).
        // 250 covers ~99% of real-world use cases while remaining safe for WASM.
        if key_set.keys.len() + key_set.symbol_keys.len() > self.max_mapped_keys() {
            self.mark_depth_exceeded();
            return TypeId::ERROR;
        }

        // Check if this is a homomorphic mapped type (template is T[K] indexed access).
        // Returns the source object T if homomorphic.
        // This handles both pre-evaluation form (constraint is `keyof T`) and
        // post-instantiation form (constraint eagerly evaluated to literal union).
        let homomorphic_source = self.homomorphic_mapped_source(mapped);
        // True when constraint is `keyof T` AND template is `T[K]` (Method 1/2 matched).
        // Used for declared-type substitution and for extending modifier inheritance to
        // non-identity `as` clauses: `is_identity_homomorphic || is_homomorphic` is the
        // full modifier-inheritance condition (see below).
        let is_identity_homomorphic = homomorphic_source.is_some();

        // For homomorphic types, source comes from the homomorphic check.
        // For non-homomorphic types, still try extracting from keyof for array/tuple preservation.
        let source_object = homomorphic_source
            .or_else(|| self.extract_source_from_keyof(mapped.constraint))
            .or_else(|| self.post_instantiation_mapped_template_source(mapped));

        if source_object.is_none()
            && let Some(source) =
                self.extract_template_index_source(mapped.template, mapped.type_param.name)
            && matches!(
                self.interner().lookup(source),
                Some(TypeData::Application(_))
            )
            && self.evaluate(source) == source
        {
            return self.interner().mapped(*mapped);
        }

        // tsc treats `{ [K in keyof T]: ... }` (no as-clause or identity as K) as
        // homomorphic for modifier inheritance — the source T's optional/readonly flags
        // propagate to the output even when the template is NOT `T[K]`. For example:
        //   type M1 = { [K in keyof Partial<M0>]: M0[K] }
        // inherits optionality from Partial<M0>'s properties, even though the
        // template is `M0[K]`, not `Partial<M0>[K]`. Key remapping still breaks
        // array/tuple shape preservation below, but it does not strip source
        // property modifiers from emitted object properties.
        let is_homomorphic = source_object.is_some();

        // A filtering/remapping `as` clause can still use the original source
        // property template (`T[K]`). In that case, preserved optional source
        // properties must carry their declared type through the mapped result
        // rather than the read type (`T[K]` -> `T | undefined`). Conditional
        // extends identity checks observe that difference even when ordinary
        // assignability does not.
        let template_reads_source_property =
            source_object.is_some_and(|source| match self.interner().lookup(mapped.template) {
                Some(TypeData::IndexAccess(obj, idx)) if obj == source => {
                    matches!(
                        self.interner().lookup(idx),
                        Some(TypeData::TypeParameter(param)) if param.name == mapped.type_param.name
                    )
                }
                _ => false,
            });
        let should_use_declared_source_property_type =
            is_identity_homomorphic || template_reads_source_property;

        // PERF: Memoize source properties into a hash map for O(1) lookup during the key loop.
        // This avoids repeated O(N) collect_properties calls inside the loop.
        // Also capture resolved_source once to avoid double evaluate(source) calls.
        let mut source_prop_map = FxHashMap::default();
        let mut source_symbol_prop_names = FxHashMap::default();
        let mut source_decl_order = Vec::new();
        let mut resolved_source_id = None;
        if let Some(source) = source_object {
            // Evaluate the source to resolve Application types (e.g., Partial<X> is
            // Application(Partial, [X]) which evaluates to { prop?: ... }). Without
            // this, collect_properties can't extract properties from unevaluated
            // Applications, causing optional/readonly modifiers to be lost.
            let resolved_source = self.evaluate(source);
            resolved_source_id = Some(resolved_source);

            // Homomorphic `any` sources still expand through the normal key path.
            let mut source_props = {
                let ordered = crate::type_queries::collect_homomorphic_source_property_infos(
                    self.interner(),
                    source,
                );
                if !ordered.is_empty() {
                    ordered
                } else {
                    match crate::objects::collect_properties_cached(
                        resolved_source,
                        self.interner(),
                        self.resolver(),
                        self.query_db(),
                    ) {
                        PropertyCollectionResult::Properties { properties, .. } => properties,
                        _ => Vec::new(),
                    }
                }
            };
            self.sort_homomorphic_source_properties_for_display(
                source,
                resolved_source,
                &mut source_props,
            );
            // Map each symbol-named source property's `SymbolRef` to the atom it
            // is stored under (e.g. `Symbol.iterator` -> `"[Symbol.iterator]"`).
            // The symbol-key emission loop below looks `source_prop_map` up by
            // this atom to recover the declared method type; without the mapping
            // it falls back to the synthetic `__unique_<id>` atom, which is not a
            // `source_prop_map` key, so `T[Symbol.iterator]` resolved to
            // `undefined` and the homomorphic result silently dropped the
            // iterator method. This must run for every homomorphic mapped type,
            // not only the `as`-clause (name_type) path.
            for prop in &source_props {
                if prop.is_symbol_named
                    && let Some(sym_ref) = self.unique_symbol_ref_from_symbol_named_atom(prop.name)
                {
                    source_symbol_prop_names.entry(sym_ref).or_insert(prop.name);
                }
            }
            if mapped.name_type.is_some() {
                let mut seen_string_keys: FxHashSet<Atom> =
                    key_set.keys.iter().map(|key| key.name).collect();
                let mut seen_symbol_keys: FxHashSet<TypeId> =
                    key_set.symbol_keys.iter().copied().collect();
                for prop in &source_props {
                    if prop.is_symbol_named {
                        if let Some(sym_ref) =
                            self.unique_symbol_ref_from_symbol_named_atom(prop.name)
                        {
                            source_symbol_prop_names.insert(sym_ref, prop.name);
                            let symbol_key = self.interner().unique_symbol(sym_ref);
                            if seen_symbol_keys.insert(symbol_key) {
                                key_set.symbol_keys.push(symbol_key);
                            }
                        }
                    } else {
                        let mapped_key = self.mapped_key_from_property(prop);
                        if seen_string_keys.insert(mapped_key.name) {
                            key_set.keys.push(mapped_key);
                        }
                    }
                }
            }
            source_prop_map.reserve(source_props.len());
            source_decl_order.reserve(source_props.len());
            for prop in source_props {
                source_decl_order.push(prop.name);
                source_prop_map.insert(
                    prop.name,
                    (
                        prop.optional,
                        prop.readonly,
                        prop.type_id,
                        prop.is_string_named,
                        prop.is_symbol_named,
                        prop.single_quoted_name,
                    ),
                );
            }
        }

        // Non-homomorphic mapped types do not inherit source declaration order.
        if !is_homomorphic {
            source_decl_order.clear();
        }

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
        if let Some(source) = source_object
            && crate::type_queries::mapped::is_identity_name_mapping(self.interner(), mapped)
        {
            // Resolve the source to check if it's an Array or Tuple
            // Use evaluate() to resolve Lazy types (interfaces/classes)
            let resolved = self.evaluate(source);

            match self.interner().lookup(resolved) {
                // Array type: map the element type
                Some(TypeData::Array(element_type)) => {
                    return self.evaluate_mapped_array(mapped, element_type);
                }

                // Tuple type: map each element. Source is mutable, so the
                // result is readonly only if the modifier adds `+readonly`.
                Some(TypeData::Tuple(tuple_id)) => {
                    return self.evaluate_mapped_tuple_with_readonly_source(
                        mapped, tuple_id, source, resolved, false,
                    );
                }

                // `readonly` tuple/array source, delegated (`None` => object path).
                Some(TypeData::ReadonlyType(inner)) => {
                    if let Some(result) =
                        self.evaluate_mapped_over_readonly_source(mapped, source, inner)
                    {
                        return result;
                    }
                }

                // ReadonlyArray: map the element type and preserve readonly
                Some(TypeData::ObjectWithIndex(shape_id)) => {
                    // Only a genuine `ReadonlyArray<T>` / `readonly T[]` should map to a
                    // readonly array. A readonly numeric index signature alone is NOT
                    // enough: a plain object like `{ readonly [k: number]: V }` also has
                    // one, and tsc maps it to an object with a readonly numeric index
                    // signature — not to an array. We therefore require the array marker
                    // methods (`slice` / `concat`), the same structural signal the
                    // conditional `infer` array path uses, before taking the array
                    // shortcut. Without this guard, mapping a bare numeric-index object
                    // dropped its `readonly` modifier by reshaping it into an array.
                    let shape = self.interner().object_shape(shape_id);
                    let has_readonly_index = shape
                        .number_index
                        .as_ref()
                        .is_some_and(|idx| idx.readonly && idx.key_type == TypeId::NUMBER)
                        && self.object_shape_has_readonly_array_markers(shape_id);

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

        // Build the resulting object properties
        let mut properties = Vec::with_capacity(key_set.keys.len());
        // PERF: Reuse a single TypeSubstitution across all keys to avoid
        // re-allocating the inner FxHashMap on every iteration.
        let mut subst = TypeSubstitution::new();
        // When the source is an intersection containing type parameters (e.g., `S & State<T>`),
        // collect_properties cannot capture the deferred index access constraints from those
        // type parameters, so the identity-homomorphic shortcut must be skipped.
        // Hoisted out of the key loops because this value is constant across all iterations.
        let source_has_type_params = resolved_source_id.is_some_and(|src| {
            crate::type_queries::is_type_parameter_or_intersection_with_type_parameter(
                self.interner(),
                src,
            )
        });
        // Whether this homomorphic mapped type removes optionality (`-?`), so the
        // top-level `undefined` an originally-optional source property contributed
        // through its read type `T[K]` must be stripped from the *evaluated*
        // property type. tsc instantiates the template with the read type (which
        // includes `| undefined` for optional keys) and only afterwards removes the
        // resulting top-level `undefined` via `getTypeWithFacts(type, NEUndefined)`.
        // The per-key `source_optional` check runs inside the loops.
        let homomorphic_removes_optional = is_homomorphic
            && !source_has_type_params
            && matches!(mapped.optional_modifier, Some(MappedModifier::Remove))
            && source_object.is_some();

        // TS2590 union-complexity bail (parity with tsc's `getUnionType`, which
        // returns `errorType` once a union reaches ~100k constituents). A
        // recursive mapped distribution such as
        // `{ [K in T]: `${K}${Rec<Exclude<T, K>>}` }[T]` produces one
        // property-value union per key; indexing the result (`[T]`) unions them,
        // so the cumulative member count grows factorially and tsz would
        // materialize an oversized union (CPU-bound non-termination, #13508)
        // where tsc bails. Track the running member total and stop once it
        // crosses the limit, mirroring the existing template-literal /
        // cross-product expansion caps. The sticky `union_too_complex` flag then
        // drives TS2590 in the checker.
        let mut cumulative_value_members: usize = 0;
        // Snapshot the flag so the cascade short-circuit reacts only to
        // complexity that arose *inside* this evaluation (a nested key's
        // sub-evaluation), never to a flag a sibling type left set before this
        // mapped type was reached.
        let union_complex_before_mapped = self.interner().is_union_too_complex();

        for mapped_key in key_set.keys {
            // Check if depth was exceeded during previous iterations
            if self.is_depth_exceeded() {
                return TypeId::ERROR;
            }
            // Cascade short-circuit: a previous key's nested distribution already
            // overflowed the union-complexity budget, so the whole mapped result
            // is too complex (TS2590). Stop before instantiating/evaluating the
            // remaining keys instead of re-paying the per-key cost for each.
            if !union_complex_before_mapped && self.interner().is_union_too_complex() {
                break;
            }
            let key_name = mapped_key.name;
            let key_literal = mapped_key.key_literal;

            let remapped = match self.remap_key_type_for_mapped(mapped, key_literal) {
                Ok(Some(remapped)) => remapped,
                Ok(None) => continue,
                Err(()) => return self.interner().mapped(*mapped),
            };
            // Property key(s) from the remapped key. Each `MappedKey` carries the
            // naming key literal (string- vs number-named; see `is_string_named`).
            let remapped_keys: smallvec::SmallVec<[MappedKey; 1]> = if remapped == key_literal {
                smallvec::smallvec![mapped_key]
            } else if let Some(entry) = self.mapped_key_from_literal(remapped) {
                smallvec::smallvec![entry]
            } else if let Some(TypeData::Union(list_id)) = self.interner().lookup(remapped) {
                let members = self.interner().type_list(list_id);
                let keys: smallvec::SmallVec<[MappedKey; 1]> = members
                    .iter()
                    .filter_map(|&m| self.mapped_key_from_literal(m))
                    .collect();
                if keys.is_empty() {
                    return self.interner().mapped(*mapped);
                }
                keys
            } else {
                return self.interner().mapped(*mapped);
            };

            // Get modifiers for this specific key (preserves homomorphic behavior)
            // Use memoized source property info for O(1) lookup.
            // Delegate to centralized modifier computation in type_queries.
            let source_info = source_prop_map.get(&key_name);
            let (source_optional, source_readonly) =
                source_info.map_or((false, false), |(opt, ro, _, _, _, _)| (*opt, *ro));

            let (optional, readonly) = crate::type_queries::compute_mapped_modifiers(
                mapped,
                is_identity_homomorphic || is_homomorphic,
                source_optional,
                source_readonly,
            );

            // PERF: For identity homomorphic mapped types (template is `T[P]`),
            // skip the expensive instantiate_type + evaluate cycle when source
            // property info is available. The declared type IS the property type
            // (with optionality handled by the modifier, not by the type itself).
            // For non-optional properties in identity homomorphic types, the
            // evaluated T[K] equals the declared type, so we can also skip.
            //
            // This fast path stays consistent with the `-?` read-type-then-strip
            // path below: for an identity `T[K]` template the declared type
            // already equals `read type - undefined`, so returning `declared_type`
            // here is equivalent to instantiating with the read type and running
            // `strip_removed_optional_undefined`. Keep that equivalence if the
            // strip semantics change.
            let property_type = if should_use_declared_source_property_type
                && !source_has_type_params
                && let Some(&(_, _, declared_type, _, _, _)) = source_info
            {
                declared_type
            } else {
                subst.clear();
                subst.insert(mapped.type_param.name, key_literal);

                // Always instantiate the template with the *read* type `T[K]`
                // (which includes `| undefined` for an optional source key). A
                // distributive template such as `V extends Validator<infer T> ? T
                // : any` must see that `undefined` so it distributes to `any`,
                // exactly as tsc does; the resulting top-level `undefined` is
                // removed below when `-?` strips optionality.
                let instantiated_template = instantiate_type_preserving_cached(
                    self.interner(),
                    self.query_db(),
                    mapped.template,
                    &subst,
                );
                let evaluated = self.evaluate(instantiated_template);

                // Check if evaluation hit depth limit
                if evaluated == TypeId::ERROR && self.is_depth_exceeded() {
                    return TypeId::ERROR;
                }
                evaluated
            };

            let property_type = self.strip_removed_optional_undefined(
                property_type,
                homomorphic_removes_optional && source_optional,
            );

            // Naming flags an identity key inherits from its homomorphic source.
            let (src_string_named, src_symbol_named, src_single_quoted) =
                source_info.map_or((false, false, false), |&(_, _, _, s, y, q)| (s, y, q));
            // Accumulate the constituent count this key contributes (each remapped
            // key emits one property whose value is `property_type`).
            cumulative_value_members = cumulative_value_members.saturating_add(
                self.count_union_members(property_type)
                    .saturating_mul(remapped_keys.len()),
            );
            for mk in remapped_keys {
                let identity = mk.name == key_name;
                // Numeric-named string-literal key (`"0"`) stays string-named so `keyof` yields `"0"`, not `0`.
                let is_string_named = (src_string_named && identity)
                    || crate::utils::type_is_numeric_string_literal(
                        self.interner(),
                        mk.key_literal,
                    );
                let single_quoted_name = src_single_quoted && identity;
                let is_symbol_named = src_symbol_named && identity;
                properties.push(PropertyInfo {
                    name: mk.name,
                    type_id: property_type,
                    write_type: property_type,
                    optional,
                    readonly,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                    is_string_named,
                    is_symbol_named,
                    single_quoted_name,
                    non_widening: false,
                });
            }

            // Stop materializing further keys once the produced property-value
            // unions cross tsc's union-complexity budget; the sticky flag drives
            // TS2590. The already-built properties are kept so the (now
            // too-complex) result still has shape for downstream display.
            if cumulative_value_members >= TEMPLATE_LITERAL_EXPANSION_LIMIT {
                self.interner().mark_union_too_complex();
                break;
            }
        }

        for symbol_key_id in &key_set.symbol_keys {
            if self.is_depth_exceeded() {
                return TypeId::ERROR;
            }

            let remapped = match self.remap_key_type_for_mapped(mapped, *symbol_key_id) {
                Ok(Some(r)) => r,
                Ok(None) => continue,
                Err(()) => return self.interner().mapped(*mapped),
            };

            // Collect the remapped unique-symbol TypeIds; handle union `as` results.
            let remapped_syms: smallvec::SmallVec<[TypeId; 1]> =
                match self.interner().lookup(remapped) {
                    Some(TypeData::UniqueSymbol(_)) => smallvec::smallvec![remapped],
                    Some(TypeData::Union(list_id)) => self
                        .interner()
                        .type_list(list_id)
                        .iter()
                        .copied()
                        .filter(|&m| {
                            matches!(self.interner().lookup(m), Some(TypeData::UniqueSymbol(_)))
                        })
                        .collect(),
                    _ => continue, // remapped to non-symbol; skip
                };

            if remapped_syms.is_empty() {
                continue;
            }

            let TypeData::UniqueSymbol(source_sym_ref) = self
                .interner()
                .lookup(*symbol_key_id)
                .expect("symbol_keys only contains UniqueSymbol TypeIds")
            else {
                continue;
            };
            let source_atom = source_symbol_prop_names
                .get(&source_sym_ref)
                .copied()
                .unwrap_or_else(|| self.symbol_named_atom_from_unique_symbol_ref(source_sym_ref));
            let source_info = source_prop_map.get(&source_atom);
            let (source_optional, source_readonly) =
                source_info.map_or((false, false), |(opt, ro, _, _, _, _)| (*opt, *ro));
            let (optional, readonly) = crate::type_queries::compute_mapped_modifiers(
                mapped,
                is_identity_homomorphic || is_homomorphic,
                source_optional,
                source_readonly,
            );

            let property_type = if should_use_declared_source_property_type
                && !source_has_type_params
                && let Some(&(_, _, declared_type, _, _, _)) = source_info
            {
                declared_type
            } else {
                subst.clear();
                subst.insert(mapped.type_param.name, *symbol_key_id);
                // See the string-key loop: instantiate with the read type `T[K]`
                // (with `| undefined` for optional keys) and strip the resulting
                // top-level `undefined` afterwards when `-?` removes optionality.
                let instantiated = instantiate_type_preserving_cached(
                    self.interner(),
                    self.query_db(),
                    mapped.template,
                    &subst,
                );
                let evaluated = self.evaluate(instantiated);
                if evaluated == TypeId::ERROR && self.is_depth_exceeded() {
                    return TypeId::ERROR;
                }
                evaluated
            };

            let property_type = self.strip_removed_optional_undefined(
                property_type,
                homomorphic_removes_optional && source_optional,
            );

            for remapped_sym_id in remapped_syms {
                // Reuse source_atom when remapped symbol is the identity (no `as` remapping).
                let remapped_atom = if remapped_sym_id == *symbol_key_id {
                    source_atom
                } else {
                    let TypeData::UniqueSymbol(remapped_sym_ref) = self
                        .interner()
                        .lookup(remapped_sym_id)
                        .expect("remapped_syms only contains UniqueSymbol TypeIds")
                    else {
                        continue;
                    };
                    self.symbol_named_atom_from_unique_symbol_ref(remapped_sym_ref)
                };
                properties.push(PropertyInfo {
                    name: remapped_atom,
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
                    is_symbol_named: true,
                    single_quoted_name: false,
                    non_widening: false,
                });
            }
        }

        crate::type_queries::merge_colliding_mapped_properties(self.interner(), &mut properties);

        self.sort_mapped_properties_for_display(
            source_object,
            resolved_source_id,
            &source_decl_order,
            &mut properties,
        );

        // Track whether each materialized index signature is optional (`?`
        // modifier). Recorded as shape-level flags below, since `IndexSignature`
        // cannot carry it.
        let mut string_index_optional = false;
        let mut number_index_optional = false;
        let mut symbol_index_optional = false;

        let string_index = if key_set.has_string {
            match self.remap_key_type_for_mapped(mapped, TypeId::STRING) {
                Ok(Some(remapped)) => {
                    if remapped != TypeId::STRING {
                        return self.interner().mapped(*mapped);
                    }
                    let (sig, optional) = self.build_index_signature_for_mapped(
                        *mapped,
                        TypeId::STRING,
                        is_identity_homomorphic || is_homomorphic,
                        source_object,
                    );
                    string_index_optional = optional;
                    Some(sig)
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
                    let (sig, optional) = self.build_index_signature_for_mapped(
                        *mapped,
                        TypeId::NUMBER,
                        is_identity_homomorphic || is_homomorphic,
                        source_object,
                    );
                    number_index_optional = optional;
                    Some(sig)
                }
                Ok(None) => None,
                Err(()) => return self.interner().mapped(*mapped),
            }
        } else {
            None
        };

        let symbol_index = if key_set.has_symbol {
            match self.remap_key_type_for_mapped(mapped, TypeId::SYMBOL) {
                Ok(Some(remapped)) => {
                    if remapped != TypeId::SYMBOL {
                        return self.interner().mapped(*mapped);
                    }
                    let (sig, optional) = self.build_index_signature_for_mapped(
                        *mapped,
                        TypeId::SYMBOL,
                        is_identity_homomorphic || is_homomorphic,
                        source_object,
                    );
                    symbol_index_optional = optional;
                    Some(sig)
                }
                Ok(None) => None,
                Err(()) => return self.interner().mapped(*mapped),
            }
        } else {
            None
        };

        let string_index = if string_index.is_none() && !key_set.template_literals.is_empty() {
            let key_type =
                crate::utils::union_or_single(self.interner(), key_set.template_literals);
            let (sig, optional) = self.build_index_signature_for_mapped(
                *mapped,
                key_type,
                is_identity_homomorphic || is_homomorphic,
                source_object,
            );
            string_index_optional = optional;
            Some(sig)
        } else {
            string_index
        };

        if string_index.is_some() || number_index.is_some() || symbol_index.is_some() {
            // A non-homomorphic mapped type's index signatures derive from its
            // constraint, so `keyof` of the materialized object is the
            // constraint key space (see `ObjectFlags::MAPPED_CONSTRAINT_KEYS`).
            // Homomorphic maps keep `keyof T` and stay unflagged.
            let mut flags = if is_homomorphic || is_identity_homomorphic {
                ObjectFlags::empty()
            } else {
                ObjectFlags::MAPPED_CONSTRAINT_KEYS
            };
            if string_index_optional {
                flags |= ObjectFlags::STRING_INDEX_OPTIONAL;
            }
            if number_index_optional {
                flags |= ObjectFlags::NUMBER_INDEX_OPTIONAL;
            }
            if symbol_index_optional {
                flags |= ObjectFlags::SYMBOL_INDEX_OPTIONAL;
            }
            self.interner().object_with_index(ObjectShape {
                flags,
                properties,
                string_index,
                number_index,
                symbol_index,
                symbol: None,
            })
        } else {
            self.interner().object(properties)
        }
    }

    /// Build the result index signature for a mapped type, returning whether the
    /// `?` modifier made it *optional* (`[k: K]?: V`). The optionality cannot be
    /// stored on `IndexSignature` itself (it has no such slot); the caller records
    /// it as a shape-level `STRING_INDEX_OPTIONAL` / `NUMBER_INDEX_OPTIONAL` flag
    /// so the assignability relation can relax the index requirement for a
    /// property-less source, exactly as tsc does for an optional index signature.
    fn build_index_signature_for_mapped(
        &mut self,
        mapped: MappedType,
        key_type: TypeId,
        inherits_modifiers: bool,
        source_object: Option<TypeId>,
    ) -> (IndexSignature, bool) {
        let subst = TypeSubstitution::single(mapped.type_param.name, key_type);
        let instantiated =
            instantiate_type_cached(self.interner(), self.query_db(), mapped.template, &subst);
        let mut value_type = self.evaluate(instantiated);
        let (idx_optional, idx_readonly) =
            self.get_mapped_modifiers(&mapped, inherits_modifiers, source_object, key_type);
        if idx_optional {
            value_type = self.interner().union2(value_type, TypeId::UNDEFINED);
        }
        (
            IndexSignature {
                key_type,
                value_type,
                readonly: idx_readonly,
                param_name: None,
            },
            idx_optional,
        )
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

        // This path handles `{ [Item in (ObjA | ObjB) as Item[...]]: ... }`,
        // substituting each concrete constraint member for the iteration
        // parameter. Bail when a member is not such a concrete type: type
        // parameters/infer placeholders are still generic, and `KeyOf`/`Lazy`
        // members mean the constraint is an unresolved `keyof <ref>` (e.g. an
        // anonymous intersection argument the resolver-less evaluator cannot
        // expand) rather than a union of objects. Returning an (empty) object
        // for those would erase the mapped type's real keys; deferring instead
        // lets a resolver-aware caller expand it correctly.
        for &member in &members {
            if matches!(
                self.interner().lookup(member),
                Some(
                    TypeData::TypeParameter(_)
                        | TypeData::Infer(_)
                        | TypeData::KeyOf(_)
                        | TypeData::Lazy(_)
                )
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
            let remapped_key = self.evaluate(instantiate_type_preserving_cached(
                self.interner(),
                self.query_db(),
                name_type,
                &subst,
            ));

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
            let instantiated_template = instantiate_type_preserving_cached(
                self.interner(),
                self.query_db(),
                mapped.template,
                &subst,
            );
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
                    is_symbol_named: false,
                    single_quoted_name: false,
                    non_widening: false,
                });
            }
        }

        crate::type_queries::merge_colliding_mapped_properties(self.interner(), &mut properties);

        Some(self.interner().object(properties))
    }

    /// Returns true when the mapped type must stay deferred because its constraint
    /// is `keyof T` (or `keyof Partial<T>`) where T is still a type parameter.
    ///
    /// Intersection constraints like `keyof T & keyof C` are intentionally excluded:
    /// those defer later at the "could not extract concrete keys" fallback, which
    /// lets intermediate paths like `try_distribute_mapped_over_union_source` run
    /// correctly for patterns such as `{ [K in keyof T & string]: T[K] }`.
    fn is_mapped_type_over_type_parameter(&self, mapped: &MappedType) -> bool {
        self.constraint_has_keyof_type_param(mapped.constraint)
    }

    /// Returns true when `constraint` is `keyof T` (or `keyof Partial<T>`) where T
    /// is a type parameter or infer type.
    ///
    /// Does not recurse through intersections — `keyof T & string` must not defer
    /// early so that `try_distribute_mapped_over_union_source` still runs correctly.
    fn constraint_has_keyof_type_param(&self, constraint: TypeId) -> bool {
        let Some(TypeData::KeyOf(source)) = self.interner().lookup(constraint) else {
            return false;
        };
        match self.interner().lookup(source) {
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_)) => true,
            Some(TypeData::Substitution { base_type, .. }) => matches!(
                self.interner().lookup(base_type),
                Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
            ),
            Some(TypeData::Mapped(inner_mapped_id)) => {
                let inner_mapped = self.interner().get_mapped(inner_mapped_id);
                self.constraint_has_keyof_type_param(inner_mapped.constraint)
            }
            _ => false,
        }
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
        let constraint = match self.interner().lookup(source)? {
            TypeData::TypeParameter(param) | TypeData::Infer(param) => param.constraint?,
            TypeData::Substitution {
                base_type,
                constraint,
            } if matches!(
                self.interner().lookup(base_type),
                Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
            ) =>
            {
                constraint
            }
            _ => return None,
        };

        // Only preserve array shape for identity name mappings (no `as` clause
        // or `as K` where K is the iteration variable)
        if !crate::type_queries::mapped::is_identity_name_mapping(self.interner(), mapped) {
            return None;
        }

        // Resolve the constraint to check if it's array/tuple-like
        let resolved = self.evaluate(constraint);

        // When the constraint is a union (e.g. `T extends [number] | readonly [string]`),
        // evaluate each union member as an array/tuple mapped type and return their union.
        // This mirrors tsc's distributeObjectOver behavior for homomorphic mapped types.
        if let Some(TypeData::Union(list_id)) = self.interner().lookup(resolved) {
            let members: Vec<TypeId> = self.interner().type_list(list_id).to_vec();
            let mut results = Vec::with_capacity(members.len());
            for &member in &members {
                let resolved_member = self.evaluate(member);
                let member_result =
                    self.try_evaluate_mapped_over_array_like(mapped, resolved_member)?;
                results.push(member_result);
            }
            if !results.is_empty() {
                let union_id = self.interner().union(results);
                tracing::trace!(
                    "evaluate_mapped: union-constrained type parameter → producing union of mapped arrays/tuples"
                );
                return Some(union_id);
            }
        }

        self.try_evaluate_mapped_over_array_like(mapped, resolved)
    }

    /// Reduce `Meta<X>` to `X` when `Meta` is a generic homomorphic mapped
    /// type and `X` is non-object (primitive/literal/`never`/unique symbol/
    /// enum). Returns `None` for directly-authored `{ [K in keyof X]: V }`
    /// since its iteration variable's constraint is `keyof X`, not
    /// `keyof <TypeParameter>`.
    fn try_reduce_substituted_homomorphic_mapped(&mut self, mapped: &MappedType) -> Option<TypeId> {
        let original_constraint = mapped.type_param.constraint?;
        let TypeData::KeyOf(original_source) = self.interner().lookup(original_constraint)? else {
            return None;
        };
        if !matches!(
            self.interner().lookup(original_source),
            Some(TypeData::TypeParameter(_))
        ) {
            return None;
        }

        let current_source = self.extract_source_from_keyof(mapped.constraint)?;
        let resolved = self.evaluate(current_source);
        if Self::is_mapped_short_circuit_source(self.interner(), resolved) {
            Some(resolved)
        } else {
            None
        }
    }

    /// True when a homomorphic mapped type whose source resolves to `type_id`
    /// should reduce to `type_id` itself, mirroring the complement of
    /// `AnyOrUnknown | InstantiableNonPrimitive | Object | Intersection` in
    /// tsc's `instantiateMappedType`.
    fn is_mapped_short_circuit_source(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return !matches!(
                type_id,
                TypeId::OBJECT
                    | TypeId::UNKNOWN
                    | TypeId::ANY
                    | TypeId::ERROR
                    | TypeId::FUNCTION
                    | TypeId::PROMISE_BASE
                    | TypeId::STRICT_ANY
            );
        }
        matches!(
            types.lookup(type_id),
            Some(
                TypeData::Intrinsic(
                    IntrinsicKind::Void
                        | IntrinsicKind::Null
                        | IntrinsicKind::Undefined
                        | IntrinsicKind::Boolean
                        | IntrinsicKind::Number
                        | IntrinsicKind::String
                        | IntrinsicKind::Bigint
                        | IntrinsicKind::Symbol
                        | IntrinsicKind::Never,
                ) | TypeData::Literal(_)
                    | TypeData::UniqueSymbol(_)
                    | TypeData::Enum(_, _)
            )
        )
    }

    /// Distribute a homomorphic mapped type over a union or intersection source.
    ///
    /// When a homomorphic generic like `Partial<T>` is instantiated with a
    /// composite source (`A | B` or `A & B`), tsc distributes:
    /// - `Partial<A | B>` → `Partial<A> | Partial<B>`
    /// - `Partial<A & B>` → `Partial<A> & Partial<B>`
    ///
    /// Only fires for instantiated forms where the effective constraint differs
    /// from the declared one. Direct `{ [K in keyof (A | B)]: ... }` is excluded.
    fn try_distribute_mapped_over_composite_source(
        &mut self,
        mapped: &MappedType,
    ) -> Option<TypeId> {
        if mapped.type_param.constraint == Some(mapped.constraint) {
            return None;
        }

        let source = self.extract_source_from_keyof(mapped.constraint)?;
        let resolved_source = self.evaluate(source);
        let (members, is_union): (Vec<TypeId>, bool) = match self.interner().lookup(resolved_source)
        {
            Some(TypeData::Union(list_id)) => (self.interner().type_list(list_id).to_vec(), true),
            Some(TypeData::Intersection(list_id)) => {
                (self.interner().type_list(list_id).to_vec(), false)
            }
            _ => return None,
        };

        if members.len() < 2 {
            return None;
        }

        let results = self.distribute_mapped_over_members(mapped, source, members);
        Some(if is_union {
            self.interner().union(results)
        } else {
            self.interner().intersection(results)
        })
    }

    /// Shared loop body for union/intersection distribution.
    ///
    /// For each member, substitutes `source` → `member` in the template (and
    /// `name_type`), interns the per-member mapped type, and routes it through
    /// the cached `evaluate`. Identical instantiations share a `TypeId`, so the
    /// memo collapses repeats — curbing the over-instantiation that exhausts fuel
    /// on recursive utilities over wide intersections — and the recursion guard's
    /// cycle detection defers a self-referential member instead of diverging.
    fn distribute_mapped_over_members(
        &mut self,
        mapped: &MappedType,
        source: TypeId,
        members: Vec<TypeId>,
    ) -> Vec<TypeId> {
        let mut results = Vec::with_capacity(members.len());
        for member in members {
            let member_keyof = self.interner().keyof(member);
            let mut memo = FxHashMap::default();
            let member_template =
                self.substitute_exact_type(mapped.template, source, member, &mut memo);
            let member_name_type = mapped.name_type.map(|name_type| {
                let mut memo = FxHashMap::default();
                self.substitute_exact_type(name_type, source, member, &mut memo)
            });
            let member_id = self.interner().mapped(MappedType {
                type_param: mapped.type_param,
                constraint: member_keyof,
                name_type: member_name_type,
                template: member_template,
                readonly_modifier: mapped.readonly_modifier,
                optional_modifier: mapped.optional_modifier,
            });
            results.push(self.evaluate(member_id));
        }
        results
    }

    /// Try to evaluate a mapped type over a single array/tuple-like type.
    /// Returns None if the type is not array/tuple-like.
    fn try_evaluate_mapped_over_array_like(
        &mut self,
        mapped: &MappedType,
        resolved: TypeId,
    ) -> Option<TypeId> {
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
                // For the generic-constrained case, the template references
                // the *type parameter* (e.g. `T[K]`), not `resolved`. The
                // per-element source rewrite is therefore a no-op here, and
                // the loop falls back to the K-only substitution path —
                // preserving deferred `T[K]` element types. Passing
                // `resolved` keeps the helper signature uniform.
                Some(self.evaluate_mapped_tuple_with_readonly(mapped, tuple_id, resolved, false))
            }
            Some(TypeData::Intersection(list_id)) => {
                let members: Vec<TypeId> = self.interner().type_list(list_id).to_vec();
                let mut mapped_members = Vec::new();
                for member in members {
                    let resolved_member = self.evaluate(member);
                    if let Some(mapped_member) =
                        self.try_evaluate_mapped_over_array_like(mapped, resolved_member)
                    {
                        mapped_members.push(mapped_member);
                    }
                }
                (!mapped_members.is_empty()).then(|| self.interner().intersection(mapped_members))
            }
            // `readonly [a, b]` or `ReadonlyArray<T>` — preserve readonly wrapper
            Some(TypeData::ReadonlyType(inner)) => match self.interner().lookup(inner) {
                Some(TypeData::Tuple(tuple_id)) => {
                    tracing::trace!(
                        "evaluate_mapped: readonly-tuple-constrained type parameter → producing readonly tuple"
                    );
                    Some(self.evaluate_mapped_tuple_with_readonly(mapped, tuple_id, resolved, true))
                }
                Some(TypeData::Array(element_type)) => {
                    tracing::trace!(
                        "evaluate_mapped: readonly-array-constrained type parameter → producing readonly array"
                    );
                    Some(self.evaluate_mapped_array_with_readonly(mapped, element_type, true))
                }
                _ => None,
            },
            // ObjectWithIndex with readonly numeric index: ReadonlyArray shape from lib
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
}
