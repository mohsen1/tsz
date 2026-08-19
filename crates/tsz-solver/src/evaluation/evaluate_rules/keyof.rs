//! keyof operator evaluation.
//!
//! Handles TypeScript's keyof operator: `keyof T`

use crate::construction::TypeDatabase;
use crate::instantiation::instantiate::instantiate_generic_cached;
use crate::objects::PropertyCollectionResult;
use crate::objects::apparent::literal_value_intrinsic_kind;
use crate::relations::subtype::TypeResolver;
use crate::type_queries::narrow_keyof_intersection_member_by_literal_discriminants;
use crate::types::{
    IndexSignature, IntrinsicKind, LiteralValue, MappedType, MappedTypeId, ObjectFlags,
    PropertyInfo, SymbolRef, TupleElement, TypeData, TypeId, TypeListId,
};
use rustc_hash::FxHashSet;
use smallvec::SmallVec;
use tsz_common::interner::Atom;

use super::super::evaluate::{
    ARRAY_METHODS_RETURN_ANY, ARRAY_METHODS_RETURN_BOOLEAN, ARRAY_METHODS_RETURN_NUMBER,
    ARRAY_METHODS_RETURN_STRING, ARRAY_METHODS_RETURN_VOID, TypeEvaluator,
};
use super::string_index_helpers::index_signature_key_includes_symbol;

const fn exact_property_key_sort_key(
    key: &crate::type_queries::ExactLiteralPropertyKey,
) -> (u32, bool, bool) {
    (key.name.0, key.is_symbol_named, key.is_string_named)
}

/// Structurally resolve an index-signature key type through transparent
/// `Lazy` alias wrappers (e.g. `PropertyKey` resolves to
/// `string | number | symbol`) so its key kinds can be classified. Without
/// this, an unresolved alias falls through key-kind classification and its
/// `symbol` arm is silently dropped, producing spurious `TS2536`/`TS2862` on
/// any symbol-bearing index. The bounded peel guards against pathological
/// alias cycles.
fn resolve_index_signature_key_alias(
    interner: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    key_type: TypeId,
) -> TypeId {
    let mut current = key_type;
    for _ in 0..8 {
        let Some(TypeData::Lazy(def_id)) = interner.lookup(current) else {
            return current;
        };
        match resolver.resolve_lazy(def_id, interner) {
            Some(next_key) if next_key != current => current = next_key,
            _ => return current,
        }
    }
    current
}

fn extend_keyof_with_index_signature_key_type(
    interner: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    key_types: &mut Vec<TypeId>,
    key_type: TypeId,
    suppress_string_numeric: bool,
) -> bool {
    let key_type = resolve_index_signature_key_alias(interner, resolver, key_type);
    match interner.lookup(key_type) {
        Some(TypeData::Union(members)) => {
            let mut contributed_number = false;
            for &member in interner.type_list(members).iter() {
                contributed_number |= extend_keyof_with_index_signature_key_type(
                    interner,
                    resolver,
                    key_types,
                    member,
                    suppress_string_numeric,
                );
            }
            contributed_number
        }
        // A genuine `[k: string]` index signature implies numeric keys
        // (`string | number`); a string-keyed *mapped constraint* contributes
        // only `string` (`suppress_string_numeric`).
        Some(TypeData::Intrinsic(IntrinsicKind::String)) if suppress_string_numeric => {
            key_types.push(TypeId::STRING);
            false
        }
        Some(TypeData::Intrinsic(IntrinsicKind::String)) => {
            key_types.push(TypeId::STRING);
            key_types.push(TypeId::NUMBER);
            true
        }
        Some(TypeData::Intrinsic(IntrinsicKind::Number)) => {
            key_types.push(TypeId::NUMBER);
            true
        }
        Some(TypeData::Intrinsic(IntrinsicKind::Symbol)) => {
            key_types.push(TypeId::SYMBOL);
            false
        }
        Some(TypeData::TemplateLiteral(_))
        | Some(TypeData::StringIntrinsic { .. })
        | Some(TypeData::Literal(LiteralValue::String(_))) => {
            key_types.push(key_type);
            false
        }
        _ => {
            key_types.push(TypeId::STRING);
            key_types.push(TypeId::NUMBER);
            if index_signature_key_includes_symbol(interner, resolver, key_type) {
                key_types.push(TypeId::SYMBOL);
            }
            true
        }
    }
}

/// Append the keyof key-types contributed by an object's index signatures.
///
/// `ObjectShape.string_index` is reused for both string- and symbol-keyed
/// signatures (discriminated by `key_type`), so a naive "if `string_index` is
/// Some -> push string|number" classification mis-reports the keyof for
/// objects whose only index signature is symbol-keyed. The keyof of
/// `{ [k: symbol]: V }` is `symbol`, not `string | number`.
///
/// - Symbol-keyed signature: contributes `symbol`.
/// - String-keyed signature: contributes `string | number` (string indexes
///   are implicitly numeric-key-compatible in TS).
/// - Number-keyed signature contributes `number` when no string-slot signature
///   already contributed numeric keyspace, except for enum namespace types
///   where tsc excludes the implicit `[index: number]: string` from
///   `keyof typeof E`.
fn extend_keyof_with_index_signature_keys(
    interner: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    key_types: &mut Vec<TypeId>,
    string_or_symbol_index: Option<&IndexSignature>,
    number_index: Option<&IndexSignature>,
    symbol_index: Option<&IndexSignature>,
    is_enum_namespace: bool,
    suppress_string_numeric: bool,
) {
    let string_slot_contributed_number = if let Some(idx) = string_or_symbol_index {
        extend_keyof_with_index_signature_key_type(
            interner,
            resolver,
            key_types,
            idx.key_type,
            suppress_string_numeric,
        )
    } else {
        false
    };

    // A dedicated `symbol` index signature (`[k: symbol]: V`) contributes the
    // `symbol` key space. `union` below dedupes if the string slot already
    // legacy-encoded a symbol key.
    if symbol_index.is_some() {
        key_types.push(TypeId::SYMBOL);
    }

    if number_index.is_some() && !is_enum_namespace && !string_slot_contributed_number {
        key_types.push(TypeId::NUMBER);
    }
}

/// Tracks the types of keys found during keyof evaluation.
pub(crate) struct KeyofKeySet {
    pub has_string: bool,
    pub has_number: bool,
    pub has_symbol: bool,
    pub string_literals: FxHashSet<Atom>,
    pub unique_symbols: FxHashSet<SymbolRef>,
}

impl KeyofKeySet {
    pub fn new() -> Self {
        Self {
            has_string: false,
            has_number: false,
            has_symbol: false,
            string_literals: FxHashSet::default(),
            unique_symbols: FxHashSet::default(),
        }
    }

    pub fn insert_type(&mut self, interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
        let Some(key) = interner.lookup(type_id) else {
            return false;
        };

        match key {
            TypeData::Union(members) => {
                let members = interner.type_list(members);
                members
                    .iter()
                    .all(|&member| self.insert_type(interner, member))
            }
            TypeData::Intrinsic(kind) => match kind {
                IntrinsicKind::String => {
                    self.has_string = true;
                    true
                }
                IntrinsicKind::Number => {
                    self.has_number = true;
                    true
                }
                IntrinsicKind::Symbol => {
                    self.has_symbol = true;
                    true
                }
                IntrinsicKind::Never => true,
                _ => false,
            },
            TypeData::Literal(LiteralValue::String(atom)) => {
                self.string_literals.insert(atom);
                true
            }
            TypeData::UniqueSymbol(symbol) => {
                self.unique_symbols.insert(symbol);
                true
            }
            _ => false,
        }
    }
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    fn unique_symbol_ref_from_synthetic_atom(&self, name: Atom) -> Option<SymbolRef> {
        let name_text = self.interner().resolve_atom_ref(name);
        let symbol_ref = name_text.strip_prefix("__unique_")?.parse::<u32>().ok()?;
        Some(SymbolRef(symbol_ref))
    }

    pub(super) fn unique_symbol_ref_from_symbol_named_atom(&self, name: Atom) -> Option<SymbolRef> {
        self.unique_symbol_ref_from_synthetic_atom(name)
            .or_else(|| {
                let name_text = self.interner().resolve_atom_ref(name);
                self.resolver().resolve_well_known_symbol_name(&name_text)
            })
    }

    /// Inverse of [`Self::unique_symbol_ref_from_symbol_named_atom`]: the
    /// object-shape property atom that a unique-symbol key is stored under.
    ///
    /// Well-known symbols (`Symbol.iterator`, `Symbol.asyncIterator`, …) live
    /// in shapes under their canonical `[Symbol.xxx]` text key, so a
    /// `UniqueSymbol(ref)` produced by `keyof`/indexed-access must convert back
    /// to that text — not the synthetic `__unique_N` placeholder, which only
    /// names user-authored unique symbols. Using the placeholder for a
    /// well-known ref makes member lookup and mapped-type materialization miss
    /// the real member (e.g. dropping `[Symbol.iterator]` from
    /// `{ [K in keyof T]: ... }` over an `Iterable`).
    pub(crate) fn symbol_named_atom_from_unique_symbol_ref(&self, symbol: SymbolRef) -> Atom {
        if let Some(name) = self.resolver().well_known_symbol_name_for_ref(symbol) {
            self.interner().intern_string(name)
        } else {
            self.interner()
                .intern_string(&format!("__unique_{}", symbol.0))
        }
    }

    /// The unique-symbol key type for a member stored under a well-known
    /// `[Symbol.xxx]` text key (`Symbol.iterator`, `Symbol.asyncIterator`, …).
    ///
    /// A well-known-symbol-keyed member is a *named* member, not a symbol index
    /// signature, so it carries `is_symbol_named = false` and is stored under its
    /// canonical `[Symbol.xxx]` atom rather than a synthetic `__unique_N`
    /// placeholder. `keyof` must nonetheless contribute the unique-symbol key
    /// `typeof Symbol.xxx` for it — `keyof { [Symbol.iterator]: T }` is
    /// `typeof Symbol.iterator` in `tsc`, not the string literal
    /// `"[Symbol.iterator]"`. The registered `SymbolRef` is the same canonical
    /// `SymbolConstructor` member ref that `typeof Symbol.xxx` resolves to, so
    /// the two `UniqueSymbol` type ids are identical.
    fn well_known_named_key_unique_symbol(&self, name: Atom) -> Option<TypeId> {
        let name_text = self.interner().resolve_atom_ref(name);
        if !name_text.starts_with("[Symbol.") {
            return None;
        }
        let symbol_ref = self.resolver().resolve_well_known_symbol_name(&name_text)?;
        Some(self.interner().unique_symbol(symbol_ref))
    }

    fn property_name_to_key_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.is_symbol_named {
            if let Some(symbol_ref) = self.unique_symbol_ref_from_symbol_named_atom(prop.name) {
                return self.interner().unique_symbol(symbol_ref);
            }
            return TypeId::SYMBOL;
        }
        if let Some(unique) = self.well_known_named_key_unique_symbol(prop.name) {
            return unique;
        }
        // `keyof { 1: ... }` yields the *numeric* literal `1`, not `"1"`.
        // The bare-numeric-name vs string-quoted distinction is the same
        // structural rule that drives mapped-type key substitution.
        crate::utils::literal_key_for_property_name(
            self.interner(),
            prop.name,
            prop.is_string_named,
        )
    }

    fn synthetic_property_key_to_key_type(
        &self,
        key: crate::type_queries::ExactLiteralPropertyKey,
    ) -> TypeId {
        if key.is_symbol_named {
            if let Some(symbol_ref) = self.unique_symbol_ref_from_symbol_named_atom(key.name) {
                return self.interner().unique_symbol(symbol_ref);
            }
            return TypeId::SYMBOL;
        }
        if let Some(unique) = self.well_known_named_key_unique_symbol(key.name) {
            return unique;
        }
        crate::utils::literal_key_for_property_name(self.interner(), key.name, key.is_string_named)
    }

    fn push_remapped_key_type(
        &mut self,
        key_types: &mut SmallVec<[TypeId; 8]>,
        remapped_key: TypeId,
    ) {
        if remapped_key == TypeId::STRING {
            key_types.push(TypeId::STRING);
            key_types.push(TypeId::NUMBER);
        } else {
            key_types.push(remapped_key);
        }
    }

    fn collect_keyof_for_remapped_mapped_type(
        &mut self,
        mapped_id: MappedTypeId,
        mapped: &MappedType,
    ) -> Option<TypeId> {
        let name_type = mapped.name_type?;
        let mut key_types: SmallVec<[TypeId; 8]> = SmallVec::new();

        let constraint_source = crate::keyof_inner_type(self.interner(), mapped.constraint)
            .or_else(|| {
                let evaluated = self.evaluate(mapped.constraint);
                (evaluated != mapped.constraint)
                    .then(|| crate::keyof_inner_type(self.interner(), evaluated))
                    .flatten()
            });

        if let Some(source) = constraint_source {
            let resolved_source = self.evaluate(source);
            match crate::objects::collect_properties_cached(
                resolved_source,
                self.interner(),
                self.resolver(),
                self.query_db(),
            ) {
                PropertyCollectionResult::Properties {
                    properties,
                    string_index,
                    number_index,
                    symbol_index: _,
                } => {
                    for prop in properties {
                        let source_key = self.property_name_to_key_type(&prop);
                        match self.remap_key_type_for_mapped(mapped, source_key) {
                            Ok(Some(remapped_key)) => {
                                self.push_remapped_key_type(&mut key_types, remapped_key);
                            }
                            Ok(None) => {}
                            Err(()) => return None,
                        }
                    }
                    if string_index.is_some() {
                        match self.remap_key_type_for_mapped(mapped, TypeId::STRING) {
                            Ok(Some(remapped_key)) => {
                                self.push_remapped_key_type(&mut key_types, remapped_key);
                            }
                            Ok(None) => {}
                            Err(()) => return None,
                        }
                    } else if number_index.is_some() {
                        match self.remap_key_type_for_mapped(mapped, TypeId::NUMBER) {
                            Ok(Some(remapped_key)) => key_types.push(remapped_key),
                            Ok(None) => {}
                            Err(()) => return None,
                        }
                    }
                }
                PropertyCollectionResult::Any => {
                    match self.remap_key_type_for_mapped(mapped, TypeId::STRING) {
                        Ok(Some(remapped_key)) => {
                            self.push_remapped_key_type(&mut key_types, remapped_key);
                        }
                        Ok(None) => {}
                        Err(()) => return None,
                    }
                    match self.remap_key_type_for_mapped(mapped, TypeId::NUMBER) {
                        Ok(Some(remapped_key)) => key_types.push(remapped_key),
                        Ok(None) => {}
                        Err(()) => return None,
                    }
                    match self.remap_key_type_for_mapped(mapped, TypeId::SYMBOL) {
                        Ok(Some(remapped_key)) => key_types.push(remapped_key),
                        Ok(None) => {}
                        Err(()) => return None,
                    }
                }
                PropertyCollectionResult::NonObject => {}
            }
        } else if let Some(keys) =
            crate::type_queries::collect_finite_mapped_property_keys(self.interner(), mapped_id)
        {
            let mut sorted_keys: Vec<_> = keys.into_iter().collect();
            sorted_keys.sort_by_key(exact_property_key_sort_key);
            for key in sorted_keys {
                key_types.push(self.synthetic_property_key_to_key_type(key));
            }
        }

        if crate::type_queries::contains_type_parameters_db(self.interner(), mapped.constraint) {
            key_types.push(name_type);
        }

        if key_types.is_empty() {
            None
        } else if key_types.len() == 1 {
            Some(key_types[0])
        } else {
            Some(self.interner().union(key_types.into_vec()))
        }
    }

    fn should_include_keyof_property(&self, prop: &crate::PropertyInfo) -> bool {
        prop.visibility == crate::Visibility::Public
            && !self
                .interner()
                .resolve_atom_ref(prop.name)
                .starts_with("__private_brand_")
    }

    /// Helper to recursively evaluate keyof while respecting depth limits.
    /// Creates a `KeyOf` type and evaluates it under the shared meta-recursion
    /// identity guard.
    pub(crate) fn recurse_keyof(&mut self, operand: TypeId) -> TypeId {
        let keyof = self.interner().keyof(operand);
        self.evaluate(keyof)
    }

    /// Whether a per-member `keyof` result is the *universal* key space and so is
    /// the identity element of the `keyof (A | B) = keyof A ∩ keyof B`
    /// intersection.
    ///
    /// Structural rule: tsc gives a deferred *conditional* type the apparent type
    /// of its default constraint, but `keyof` of a conditional that does not
    /// reduce — most notably a **self-referential / recursive** conditional whose
    /// inferred true branch is the conditional itself (`type R<T> = T extends U ?
    /// R<T> : Concrete`) — is left as the universal `string | number | symbol`. In
    /// a union `R<T> | Concrete` the recursive branch must therefore not restrict
    /// the shared key space (`universal ∩ keyof Concrete = keyof Concrete`).
    ///
    /// tsz models the unreduced `keyof` as a deferred `KeyOf` whose operand is (or
    /// reduces to) a `Conditional`. Treating such a member as universal lets the
    /// concrete branches own the key space instead of the whole intersection
    /// collapsing to an opaque, unresolved `KeyOf` (which downstream
    /// indexed-access validation rejects, producing a spurious `TS2536`). A
    /// non-conditional deferred `KeyOf` (e.g. `keyof T` for a bare type parameter)
    /// is *not* universal and still restricts the intersection.
    fn keyof_member_is_universal(&self, keyof_member: TypeId) -> bool {
        let Some(TypeData::KeyOf(inner)) = self.interner().lookup(keyof_member) else {
            return false;
        };
        self.keyof_operand_is_deferred_conditional(inner)
    }

    /// Whether `operand` is — or transparently reduces to (through alias
    /// `Application`/`Lazy` wrappers) — a deferred `Conditional`. Bounded peel
    /// guards against pathological wrapper nesting.
    fn keyof_operand_is_deferred_conditional(&self, operand: TypeId) -> bool {
        let mut current = operand;
        for _ in 0..8 {
            match self.interner().lookup(current) {
                Some(TypeData::Conditional(_)) => return true,
                Some(TypeData::Application(app_id)) => {
                    current = self.interner().type_application(app_id).base;
                }
                Some(TypeData::Lazy(def_id)) => {
                    match self.resolver().resolve_lazy(def_id, self.interner()) {
                        Some(resolved) if resolved != current => current = resolved,
                        _ => return false,
                    }
                }
                _ => return false,
            }
        }
        false
    }

    /// Compute `keyof (members…)` = the intersection of each member's key space,
    /// dropping members whose `keyof` is the universal key space (the identity
    /// element — see [`Self::keyof_member_is_universal`]). When every member is
    /// universal the result is the universal key space itself (a recursed `keyof`
    /// over the first member, which is already universal); otherwise the restated
    /// intersection of the remaining concrete members is returned.
    fn keyof_union_intersection(&mut self, member_list: &[TypeId]) -> TypeId {
        let mut key_types: SmallVec<[TypeId; 4]> = SmallVec::with_capacity(member_list.len());
        let mut universal_member: Option<TypeId> = None;
        for &member in member_list {
            let keyof_member = self.recurse_keyof(member);
            if self.keyof_member_is_universal(keyof_member) {
                universal_member.get_or_insert(keyof_member);
                continue;
            }
            key_types.push(keyof_member);
        }
        if key_types.is_empty() {
            // Every branch contributed the universal key space; keep it.
            return universal_member.unwrap_or(TypeId::NEVER);
        }
        if let Some(intersection) = self.intersect_keyof_sets(&key_types) {
            intersection
        } else {
            self.interner().intersection(key_types.into_vec())
        }
    }

    /// Evaluate keyof T - extract the keys of an object type
    pub fn evaluate_keyof(&mut self, operand: TypeId) -> TypeId {
        let keyof = self.interner().keyof(operand);
        self.with_meta_rereduce_recursion_identity(operand, keyof, |evaluator| {
            evaluator.evaluate_keyof_inner(operand)
        })
    }

    fn evaluate_keyof_inner(&mut self, operand: TypeId) -> TypeId {
        // PERF: Single lookup for both TemplateLiteral and Union checks.
        // CRITICAL ordering: TemplateLiteral BEFORE Union to avoid incorrect intersection,
        // and Union BEFORE general evaluation to avoid union simplification.
        match self.interner().lookup(operand) {
            Some(TypeData::TemplateLiteral(_)) => {
                return self.apparent_primitive_keyof(IntrinsicKind::String);
            }
            // Generic indexed-access operand `T[K]` where the *index* is generic
            // must keep `keyof` deferred. Eagerly resolving `T[K]` through K's
            // constraint expands to a union of T's value shapes (e.g. `{a1}|{b1}`),
            // and `keyof (A | B) = keyof A & keyof B` collapses disjoint key
            // sets to `never` — exposing user code to spurious TS2345 (#8725).
            //
            // tsc keeps `keyof T[K]` as a deferred IndexType whose apparent type
            // is `string | number | symbol`. We mirror that by returning
            // `TypeData::KeyOf(operand)` here; downstream relation/apparent-type
            // paths already handle deferred keyofs.
            Some(TypeData::IndexAccess(_, index))
                if crate::type_queries::contains_type_parameters_db(self.interner(), index) =>
            {
                return self.interner().keyof(operand);
            }
            Some(TypeData::Union(_members)) => {
                let narrowed_operand = self.prune_impossible_object_union_members_resolved(operand);
                let Some(TypeData::Union(members)) = self.interner().lookup(narrowed_operand)
                else {
                    return self.recurse_keyof(narrowed_operand);
                };
                let member_list = self.interner().type_list(members).to_vec();

                // keyof (A | B) = keyof A & keyof B — the intersection of each
                // member's key space. Universal (deferred-conditional) members are
                // the identity element and are dropped so they do not collapse the
                // shared key space to an opaque, unresolved KeyOf.
                return self.keyof_union_intersection(&member_list);
            }
            _ => {}
        }

        if let Some(keys) = self.try_keyof_mapped_application_constraint(operand) {
            return keys;
        }

        {
            // For non-union types, evaluate normally
            let evaluated_operand = self.evaluate(operand);
            if evaluated_operand != operand
                && self.same_meta_recursion_identity(operand, evaluated_operand)
            {
                self.defer_same_identity_meta_recursion();
                return self.interner().keyof(operand);
            }

            let key = match self.interner().lookup(evaluated_operand) {
                Some(k) => k,
                None => return TypeId::NEVER,
            };

            match key {
                TypeData::Union(_members) => {
                    let narrowed_operand =
                        self.prune_impossible_object_union_members_resolved(evaluated_operand);
                    let Some(TypeData::Union(members)) = self.interner().lookup(narrowed_operand)
                    else {
                        return self.recurse_keyof(narrowed_operand);
                    };
                    let member_list = self.interner().type_list(members).to_vec();
                    self.keyof_union_intersection(&member_list)
                }
                TypeData::Mapped(mapped_id) => {
                    let mapped = self.interner().get_mapped(mapped_id);
                    if let Some(name_type) = mapped.name_type {
                        let constraint_has_type_params =
                            crate::type_queries::contains_type_parameters_db(
                                self.interner(),
                                mapped.constraint,
                            );
                        if !constraint_has_type_params
                            && let Some(keys) =
                                self.collect_keyof_for_remapped_mapped_type(mapped_id, &mapped)
                        {
                            return keys;
                        }
                        if constraint_has_type_params
                            && let Some(keys) =
                                self.collect_keyof_for_remapped_mapped_type(mapped_id, &mapped)
                            && keys != name_type
                        {
                            return keys;
                        }
                        if constraint_has_type_params
                            || crate::type_queries::contains_type_parameters_except_name_db(
                                self.interner(),
                                name_type,
                                mapped.type_param.name,
                            )
                        {
                            return self.interner().keyof(evaluated_operand);
                        }
                        self.collect_keyof_for_remapped_mapped_type(mapped_id, &mapped)
                            .unwrap_or_else(|| {
                                self.evaluate(mapped.name_type.unwrap_or(TypeId::ERROR))
                            })
                    } else {
                        self.evaluate(mapped.constraint)
                    }
                }
                TypeData::ReadonlyType(inner) => self.recurse_keyof(inner),
                TypeData::TypeQuery(sym) => {
                    // Resolve typeof query before computing keyof
                    let resolved = self.resolver().resolve_symbol_ref(sym, self.interner());
                    if let Some(resolved) = resolved {
                        self.recurse_keyof(resolved)
                    } else {
                        TypeId::ERROR
                    }
                }
                TypeData::TypeParameter(param) | TypeData::Infer(param) => {
                    if let Some(constraint) = param.constraint {
                        if constraint == evaluated_operand {
                            self.interner().keyof(operand)
                        } else if matches!(
                            self.interner().lookup(constraint),
                            Some(TypeData::TypeParameter(_))
                        ) {
                            // When the constraint is itself a type parameter
                            // (e.g., B extends A where A is generic), do NOT
                            // recurse. keyof B ≠ keyof A even though B extends A,
                            // because B may have additional keys. Collapsing
                            // keyof B → keyof A breaks subtype comparisons.
                            self.interner().keyof(operand)
                        } else {
                            // Evaluate keyof of the constraint. If the result is
                            // non-informative (NEVER — e.g. `object`, `unknown`),
                            // keep keyof as deferred to preserve the type parameter
                            // connection. This is critical for for-in loops where
                            // `keyof T` must remain abstract for mapped type index
                            // access patterns to work correctly.
                            let constraint_keyof = self.recurse_keyof(constraint);
                            if constraint_keyof == TypeId::NEVER {
                                self.interner().keyof(operand)
                            } else {
                                constraint_keyof
                            }
                        }
                    } else {
                        self.interner().keyof(operand)
                    }
                }
                TypeData::Object(shape_id) => {
                    let shape = self.interner().object_shape(shape_id);
                    // Object shape properties are stored sorted by atom for hash
                    // consistency (`intern/core/constructors.rs::object_with_flags`).
                    // For keyof we need declaration order: tsc lazily allocates a
                    // literal type for each key as it iterates properties in source
                    // order, so the resulting union ends up sorted by Type.id ==
                    // declaration order. Mirroring that here ensures the printer's
                    // alloc-order-based union sort displays keys in source order
                    // (e.g. `"foo" | "bar"` for `{ foo: ...; bar: ... }`).
                    let mut props: Vec<&PropertyInfo> = shape
                        .properties
                        .iter()
                        .filter(|p| self.should_include_keyof_property(p))
                        .collect();
                    props.sort_by_key(|p| p.declaration_order);
                    let key_types: Vec<TypeId> = props
                        .into_iter()
                        .map(|p| self.property_name_to_key_type(p))
                        .collect();
                    if key_types.is_empty() {
                        return TypeId::NEVER;
                    }
                    self.interner().union(key_types)
                }
                TypeData::ObjectWithIndex(shape_id) => {
                    let shape = self.interner().object_shape(shape_id);
                    let mut props: Vec<&PropertyInfo> = shape
                        .properties
                        .iter()
                        .filter(|p| self.should_include_keyof_property(p))
                        .collect();
                    props.sort_by_key(|p| p.declaration_order);
                    let mut key_types: Vec<TypeId> = props
                        .into_iter()
                        .map(|p| self.property_name_to_key_type(p))
                        .collect();

                    extend_keyof_with_index_signature_keys(
                        self.interner(),
                        self.resolver(),
                        &mut key_types,
                        shape.string_index_signature(),
                        shape.number_index.as_ref(),
                        shape.symbol_index_signature(),
                        shape.flags.contains(ObjectFlags::ENUM_NAMESPACE),
                        shape.flags.contains(ObjectFlags::MAPPED_CONSTRAINT_KEYS),
                    );

                    if key_types.is_empty() {
                        TypeId::NEVER
                    } else {
                        self.interner().union(key_types)
                    }
                }
                TypeData::Callable(shape_id) => {
                    let shape = self.interner().callable_shape(shape_id);
                    let mut props: Vec<&PropertyInfo> = shape
                        .properties
                        .iter()
                        .filter(|p| self.should_include_keyof_property(p))
                        .collect();
                    props.sort_by_key(|p| p.declaration_order);
                    let mut key_types: Vec<TypeId> = props
                        .into_iter()
                        .map(|p| self.property_name_to_key_type(p))
                        .collect();

                    extend_keyof_with_index_signature_keys(
                        self.interner(),
                        self.resolver(),
                        &mut key_types,
                        shape.string_index.as_ref(),
                        shape.number_index.as_ref(),
                        None,
                        false,
                        false,
                    );

                    if key_types.is_empty() {
                        TypeId::NEVER
                    } else {
                        self.interner().union(key_types)
                    }
                }
                // A bare function type (`() => T`, `(x: U) => V`,
                // `<U>(u: U) => U`) has no own properties or index signatures,
                // so its key space is empty.  `Callable` with only construct
                // signatures already collapses to `never` via the arm above;
                // `Function` must do the same for `extends never` /
                // `Equal<keyof Fn, never>` parity with tsc.
                TypeData::Function(_) => TypeId::NEVER,
                TypeData::Array(_) => self.interner().union(self.array_keyof_keys()),
                TypeData::Tuple(elements) => {
                    let elements = self.interner().tuple_list(elements);
                    let mut key_types: Vec<TypeId> = Vec::new();
                    self.append_tuple_indices(&elements, 0, &mut key_types);
                    let mut array_keys = self.array_keyof_keys();
                    key_types.append(&mut array_keys);
                    if key_types.is_empty() {
                        return TypeId::NEVER;
                    }
                    self.interner().union(key_types)
                }
                TypeData::Intrinsic(kind) => match kind {
                    IntrinsicKind::Any => {
                        // keyof any = string | number | symbol
                        self.interner()
                            .union3(TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL)
                    }
                    IntrinsicKind::Unknown => {
                        // keyof unknown = never
                        TypeId::NEVER
                    }
                    IntrinsicKind::Never => {
                        // keyof never = string | number | symbol
                        self.interner()
                            .union3(TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL)
                    }
                    IntrinsicKind::Void
                    | IntrinsicKind::Null
                    | IntrinsicKind::Undefined
                    | IntrinsicKind::Object
                    | IntrinsicKind::Function => TypeId::NEVER,
                    IntrinsicKind::String
                    | IntrinsicKind::Number
                    | IntrinsicKind::Boolean
                    | IntrinsicKind::Bigint
                    | IntrinsicKind::Symbol => self.apparent_primitive_keyof(kind),
                },
                TypeData::Literal(literal) => {
                    self.apparent_primitive_keyof(literal_value_intrinsic_kind(&literal))
                }
                TypeData::TemplateLiteral(_) => {
                    self.apparent_primitive_keyof(IntrinsicKind::String)
                }
                // NOTE: Union is handled at the top of this function to avoid union simplification
                TypeData::Intersection(members) => {
                    // keyof (A & B) = keyof A | keyof B (covariance)
                    self.keyof_intersection(members, operand)
                }
                // CRITICAL: Handle Lazy (type aliases) by attempting resolution via resolver
                TypeData::Lazy(def_id) => {
                    if let Some(resolved) = self.resolver().resolve_lazy(def_id, self.interner()) {
                        // Recursively compute keyof of the resolved type.
                        self.recurse_keyof(resolved)
                    } else {
                        // The alias body is not registered on this query
                        // (e.g. a cross-file type alias whose declaring file
                        // is still being processed, or has not published its
                        // body to the shared `DefinitionStore`). The deferred
                        // `keyof` produced here is a registration-window
                        // artifact, NOT a stable function of `operand`: once
                        // the body is resolvable, `keyof` reduces to the
                        // concrete key set. Caching the deferred form poisons
                        // every later use — e.g. a generic constraint
                        // `T extends keyof Omit<ImportedIface, K>` then
                        // rejects a valid key with a spurious `TS2345`
                        // (kysely `AlterColumnNode.create`, #10663). Mark the
                        // unresolved def so the result stays out of the
                        // persistent caches, mirroring the `Application`
                        // body-resolution bails (`evaluate/application.rs`).
                        self.mark_unresolved_def_seen();
                        self.interner().keyof(operand)
                    }
                }
                // CRITICAL: Handle Application (generic types) by evaluating them first
                TypeData::Application(_app_id) => {
                    // Evaluate the application to get the instantiated type
                    let evaluated = self.evaluate(evaluated_operand);
                    // Then compute keyof of the evaluated result
                    self.recurse_keyof(evaluated)
                }
                // ThisType: resolve to the concrete class type via the resolver,
                // then compute keyof on the resolved type.
                TypeData::ThisType => {
                    if let Some(concrete_this) = self.resolver().resolve_this_type(self.interner())
                    {
                        self.recurse_keyof(concrete_this)
                    } else {
                        self.interner().keyof(operand)
                    }
                }
                // Enum types: resolve to the namespace object type for keyof
                // typeof Enum gives { Up: E.Up, Down: E.Down }, keyof gives "Up" | "Down"
                TypeData::Enum(def_id, _member_type) => {
                    if let Some(ns_type) = self.resolver().get_enum_namespace_type(def_id) {
                        tracing::trace!(
                            def_id = def_id.0,
                            ns_type = ns_type.0,
                            "keyof_enum_ns_hit"
                        );
                        self.recurse_keyof(ns_type)
                    } else {
                        tracing::trace!(def_id = def_id.0, "keyof_enum_ns_miss");
                        // The namespace object is registered when the checker
                        // computes the enum symbol's type; a miss here is a
                        // registration-window artifact, not a stable function
                        // of `operand` (same class as the `Lazy` bail above).
                        // Keep the deferred form out of the persistent caches
                        // so a later evaluation reduces to the key set.
                        self.mark_unresolved_def_seen();
                        self.interner().keyof(operand)
                    }
                }
                // Conditional types: if the check type is a type parameter and every
                // branch shares its key space (identity, or a non-remapped mapped type
                // whose constraint is `keyof <check_param>`), keyof of the conditional
                // reduces to keyof of the check parameter.  This catches expanded
                // utility-type aliases like `DeepRequired<T>` whose application has
                // already been substituted to its body before reaching `evaluate_keyof`.
                TypeData::Conditional(_) => self
                    .try_keyof_from_conditional_branches(evaluated_operand)
                    .or_else(|| {
                        self.try_keyof_from_conditional_default_constraint(evaluated_operand)
                    })
                    .unwrap_or_else(|| self.interner().keyof(operand)),
                // For other types (type parameters, etc.), keep as KeyOf (deferred)
                _ => self.interner().keyof(operand),
            }
        }
    }

    fn try_keyof_mapped_application_constraint(&mut self, operand: TypeId) -> Option<TypeId> {
        let TypeData::Application(app_id) = self.interner().lookup(operand)? else {
            return None;
        };
        let app = self.interner().type_application(app_id);
        let def_id = match self.interner().lookup(app.base)? {
            TypeData::Lazy(def_id) => Some(def_id),
            TypeData::TypeQuery(sym_ref) => self.resolver().symbol_to_def_id(sym_ref),
            TypeData::UnresolvedTypeName(atom) => {
                let name = self.interner().resolve_atom(atom);
                self.resolver().resolve_unresolved_type_name(&name)
            }
            _ => None,
        }?;
        let resolved = self.resolver().resolve_lazy(def_id, self.interner())?;

        // Inspect the body kind first so non-Mapped, non-Conditional aliases
        // (e.g. `Array<T>`, plain interface aliases) skip the cost of
        // resolving/extracting type parameters.
        let body = self.interner().lookup(resolved)?;
        if !matches!(body, TypeData::Mapped(_) | TypeData::Conditional(_)) {
            return None;
        }

        let type_params = self
            .resolver()
            .get_lazy_type_params(def_id)
            .filter(|params| params.len() == app.args.len())
            .unwrap_or_else(|| self.extract_type_params_from_type(resolved).to_vec());
        if type_params.len() != app.args.len() {
            return None;
        }

        match body {
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner().get_mapped(mapped_id);
                if mapped.name_type.is_some() {
                    return None;
                }
                let instantiated = instantiate_generic_cached(
                    self.interner(),
                    self.query_db(),
                    mapped.constraint,
                    &type_params,
                    &app.args,
                );
                Some(self.evaluate_or_keep(instantiated))
            }
            TypeData::Conditional(_) => {
                // Recursive utility types like `DeepRequired<T>` whose body is a
                // conditional (`T extends C ? A : B`) need keyof reduction when
                // every branch shares the source argument's key space; otherwise
                // `DeepRequired<T>[K]` with `K in keyof T` would misfire TS2536.
                self.try_keyof_from_conditional_application_body(resolved, &type_params, &app.args)
            }
            _ => None,
        }
    }

    /// Evaluate `ty`; if evaluation returns `ERROR`, keep the original.  Used to
    /// avoid surfacing solver errors when a usable deferred form already exists.
    fn evaluate_or_keep(&mut self, ty: TypeId) -> TypeId {
        let evaluated = self.evaluate(ty);
        if evaluated == TypeId::ERROR {
            ty
        } else {
            evaluated
        }
    }

    /// Structural rule: every branch of the conditional must be either the
    /// source type parameter itself (identity) or a non-remapped mapped type
    /// whose constraint is `keyof <source param>`. When so, `keyof F<T>` =
    /// `keyof T` (returned as the instantiated keyof of the source arg).
    fn try_keyof_from_conditional_application_body(
        &mut self,
        conditional_type_id: TypeId,
        type_params: &[crate::types::TypeParamInfo],
        args: &[TypeId],
    ) -> Option<TypeId> {
        let cond_id =
            crate::type_queries::get_conditional_type_id(self.interner(), conditional_type_id)?;
        let cond = self.interner().conditional_type(cond_id);

        let source_param_idx = type_params
            .iter()
            .position(|p| self.is_type_param_named(cond.check_type, p.name))?;
        let source_arg = args[source_param_idx];
        let source_name = type_params[source_param_idx].name;

        // Pre-instantiation screen against the alias's own param name; bail
        // before the (expensive) `instantiate_generic` when either branch
        // clearly isn't in the source's key space.
        let matches_by_name = |ty: TypeId| self.is_type_param_named(ty, source_name);
        if !self.branch_matches_keyof_source(cond.true_type, &matches_by_name)
            || !self.branch_matches_keyof_source(cond.false_type, &matches_by_name)
        {
            return None;
        }

        // Post-instantiation check uses the source ARG identity: same TypeId,
        // or — defensively, in case substitution produced a distinct TypeParameter
        // node with the same name — same param name as the arg.
        let source_arg_name =
            crate::type_param_info(self.interner(), source_arg).map(|info| info.name);
        let matches_by_arg = |ty: TypeId| {
            ty == source_arg || source_arg_name.is_some_and(|n| self.is_type_param_named(ty, n))
        };
        let inst_true = instantiate_generic_cached(
            self.interner(),
            self.query_db(),
            cond.true_type,
            type_params,
            args,
        );
        if !self.branch_matches_keyof_source(inst_true, &matches_by_arg) {
            return None;
        }
        let inst_false = instantiate_generic_cached(
            self.interner(),
            self.query_db(),
            cond.false_type,
            type_params,
            args,
        );
        if !self.branch_matches_keyof_source(inst_false, &matches_by_arg) {
            return None;
        }

        let keyof_source = self.interner().keyof(source_arg);
        Some(self.evaluate_or_keep(keyof_source))
    }

    /// Direct-conditional form of the branch-keyof reduction, for cases where
    /// `evaluate_keyof` reaches a Conditional that's already been substituted
    /// (e.g. `T extends C ? { [P in keyof T]?: ... } : T` arrived here without
    /// the surrounding Application).  Treats the conditional's check type as
    /// the "source": every branch must either *be* the check type or be a
    /// non-remapped mapped type whose constraint is `keyof <check>`.
    fn try_keyof_from_conditional_branches(&mut self, conditional: TypeId) -> Option<TypeId> {
        let cond_id = crate::type_queries::get_conditional_type_id(self.interner(), conditional)?;
        let cond = self.interner().conditional_type(cond_id);
        // Check type must already be a type parameter for the rule to apply.
        let source_info = crate::type_param_info(self.interner(), cond.check_type)?;
        let source = cond.check_type;
        let source_name = source_info.name;
        let is_source = |ty: TypeId| ty == source || self.is_type_param_named(ty, source_name);
        if !self.branch_matches_keyof_source(cond.true_type, &is_source)
            || !self.branch_matches_keyof_source(cond.false_type, &is_source)
        {
            return None;
        }
        let keyof_source = self.interner().keyof(source);
        Some(self.evaluate_or_keep(keyof_source))
    }

    /// `keyof` of a deferred conditional through its **default constraint** (tsc's
    /// `getApparentType` resolves a deferred conditional to
    /// `getDefaultConstraintOfConditionalType` — the union of its branch results —
    /// and computes `keyof` of that). This is the general fallback used when the
    /// branch-shares-check-parameter reduction
    /// ([`Self::try_keyof_from_conditional_branches`]) does not apply: it lets the
    /// keys produced inside the (possibly nested / recursive) branches participate
    /// in the key space instead of leaving an opaque deferred `KeyOf` that
    /// downstream indexed-access validation rejects (spurious `TS2536`).
    ///
    /// Returns `None` when:
    /// - the conditional has no default constraint (it is not actually deferred);
    /// - the constraint is `error`/`any` (no usable key space — preserve the
    ///   strict path);
    /// - the constraint references the conditional itself directly (a guard
    ///   against unbounded keyof recursion that
    ///   [`Self::keyof_union_intersection`] cannot break because the self-edge is
    ///   not a union member);
    /// - the resulting `keyof` is itself still an unresolved deferred `KeyOf`
    ///   (nothing was gained — keep the caller's deferred form).
    fn try_keyof_from_conditional_default_constraint(
        &mut self,
        conditional: TypeId,
    ) -> Option<TypeId> {
        let constraint =
            crate::type_queries::get_conditional_default_constraint(self.interner(), conditional)?;
        if constraint == conditional || constraint == TypeId::ERROR || constraint == TypeId::ANY {
            return None;
        }
        let keyof = self.recurse_keyof(constraint);
        // Only adopt a key space that actually resolved past a bare deferred
        // `KeyOf` (e.g. `keyof <recursive conditional>` that did not reduce). A
        // resolved literal/primitive key union — or `never` (the empty key space
        // for a conditional whose branches share no keys) — is usable.
        if matches!(self.interner().lookup(keyof), Some(TypeData::KeyOf(_))) {
            return None;
        }
        Some(keyof)
    }

    fn is_type_param_named(&self, ty: TypeId, name: tsz_common::interner::Atom) -> bool {
        crate::type_param_info(self.interner(), ty).is_some_and(|info| info.name == name)
    }

    /// Does `branch_type` have the same key space as the source, where
    /// "the source" is identified by `is_source`?  True when:
    /// - `is_source(branch_type)` (identity / name match), or
    /// - `branch_type` is a non-remapped mapped type whose constraint is
    ///   `keyof X` and `is_source(X)`.
    fn branch_matches_keyof_source<F>(&self, branch_type: TypeId, is_source: &F) -> bool
    where
        F: Fn(TypeId) -> bool,
    {
        if is_source(branch_type) {
            return true;
        }
        let Some(TypeData::Mapped(mapped_id)) = self.interner().lookup(branch_type) else {
            return false;
        };
        let mapped = self.interner().get_mapped(mapped_id);
        if mapped.name_type.is_some() {
            return false;
        }
        crate::keyof_inner_type(self.interner(), mapped.constraint).is_some_and(is_source)
    }

    /// Resolver-aware wrapper around the pure
    /// [`prune_impossible_object_union_members`](crate::type_queries::prune_impossible_object_union_members).
    ///
    /// The pure prune walks the interned type DAG without a resolver, so an
    /// object-union member whose required discriminant is an intersection of
    /// enum members reached through an alias body (`{ v: E.A } & { v: E.B }`,
    /// where `E.B` is still an unresolved `Lazy(DefId)`) never reduces to
    /// `never`: `is_unit_type`/`is_subtype_of` cannot see the enum member behind
    /// the `Lazy`, so the impossible constituent survives. `keyof` of the
    /// discriminated union then collapses to the shared key set
    /// (`keyof (A | B) = keyof A & keyof B`), silently dropping the
    /// member-specific keys — e.g. `keyof ({ v: E.A, a } | { v: E.A & E.B, b })`
    /// loses `a`. This companion runs the pure prune first (fast, memoized) and
    /// then drops any residual member whose required discriminant is an
    /// impossible enum-member intersection, resolving the `Lazy` references via
    /// the evaluator's resolver. It mirrors tsc, whose `getReducedType` prunes
    /// the impossible constituent before taking `keyof`, and materializes
    /// nothing beyond the local disjointness decision.
    fn prune_impossible_object_union_members_resolved(&mut self, operand: TypeId) -> TypeId {
        let pure =
            crate::type_queries::prune_impossible_object_union_members(self.interner(), operand);
        let Some(TypeData::Union(list_id)) = self.interner().lookup(pure) else {
            return pure;
        };
        let members = self.interner().type_list(list_id).to_vec();
        let mut retained: Vec<TypeId> = Vec::with_capacity(members.len());
        for &member in &members {
            if !self.object_member_has_impossible_enum_discriminant(member) {
                retained.push(member);
            }
        }
        match retained.len() {
            n if n == members.len() => pure,
            0 => TypeId::NEVER,
            1 => retained[0],
            _ => self.interner().union_preserve_members(retained),
        }
    }

    /// `true` when `member` is an object type carrying a required property whose
    /// type is an impossible (`never`) intersection of pairwise-disjoint enum
    /// members / literals, resolving transparent `Lazy(DefId)` wrappers first.
    fn object_member_has_impossible_enum_discriminant(&mut self, member: TypeId) -> bool {
        let Some(shape) = crate::type_queries::get_object_shape(self.interner(), member) else {
            return false;
        };
        for prop in &shape.properties {
            if prop.optional {
                continue;
            }
            if self.unit_intersection_disjoint_resolving_lazy(prop.type_id) {
                return true;
            }
        }
        false
    }

    /// `true` when `type_id` is an intersection of two or more unit types
    /// (literals / enum members) that are pairwise disjoint — i.e. a `never`
    /// intersection — after peeling transparent `Lazy(DefId)` wrappers so an
    /// alias-reached enum member is compared as its concrete member type.
    fn unit_intersection_disjoint_resolving_lazy(&mut self, type_id: TypeId) -> bool {
        let Some(TypeData::Intersection(list_id)) = self.interner().lookup(type_id) else {
            return false;
        };
        let members = self.interner().type_list(list_id).to_vec();
        let mut units: SmallVec<[TypeId; 4]> = SmallVec::new();
        for &member in &members {
            let resolved = self.resolve_lazy_alias_chain(member);
            if !crate::type_queries::is_unit_type(self.interner(), resolved) {
                continue;
            }
            let mut checker = crate::relations::subtype::SubtypeChecker::with_resolver(
                self.interner(),
                self.resolver(),
            );
            let disjoint = units.iter().any(|&other| {
                !checker.is_subtype_of(resolved, other) && !checker.is_subtype_of(other, resolved)
            });
            if disjoint {
                return true;
            }
            if !units.contains(&resolved) {
                units.push(resolved);
            }
        }
        false
    }

    /// Peel transparent `Lazy(DefId)` alias wrappers to the concrete target,
    /// bounded against pathological alias cycles. Mirrors
    /// [`resolve_index_signature_key_alias`] for non-key positions.
    fn resolve_lazy_alias_chain(&self, type_id: TypeId) -> TypeId {
        let mut current = type_id;
        for _ in 0..8 {
            let Some(TypeData::Lazy(def_id)) = self.interner().lookup(current) else {
                return current;
            };
            match self.resolver().resolve_lazy(def_id, self.interner()) {
                Some(next) if next != current => current = next,
                _ => return current,
            }
        }
        current
    }

    /// Compute keyof for an intersection type: keyof (A & B) = keyof A | keyof B
    pub(crate) fn keyof_intersection(&mut self, members: TypeListId, _operand: TypeId) -> TypeId {
        let members = self.interner().type_list(members).to_vec();
        // Use recurse_keyof to respect depth limits
        // Use loop instead of closure to allow mutable self access
        let mut key_sets: SmallVec<[TypeId; 4]> = SmallVec::with_capacity(members.len());
        for (member_idx, &member) in members.iter().enumerate() {
            let narrowed_member = narrow_keyof_intersection_member_by_literal_discriminants(
                self.interner(),
                member,
                &members,
                member_idx,
            );
            key_sets.push(self.recurse_keyof(narrowed_member));
        }
        self.interner().union(key_sets.into_vec())
    }

    /// Get the keyof keys for an array type (includes all array methods and number index).
    pub(crate) fn array_keyof_keys(&self) -> Vec<TypeId> {
        let array_base = self
            .interner()
            .get_array_display_base_type()
            .or_else(|| self.resolver().get_array_base_type())
            // Fall back to the interner-registered array base. Tests and other
            // standalone evaluator callers that wire up `set_array_base_type`
            // without registering a display base or a resolver-side base would
            // otherwise drop down to the lib-default key list and leak methods
            // that aren't on the registered Array<T> shape.
            .or_else(|| self.interner().get_array_base_type());
        if let Some(array_base) = array_base {
            let base_props = crate::type_queries::collect_homomorphic_source_property_infos(
                self.interner(),
                array_base,
            );
            if !base_props.is_empty() {
                let mut keys = Vec::with_capacity(base_props.len() + 1);
                keys.push(TypeId::NUMBER);
                for prop in base_props {
                    keys.push(self.property_name_to_key_type(&prop));
                }
                return keys;
            }
        }

        let mut keys = Vec::new();
        keys.push(TypeId::NUMBER);
        keys.push(self.interner().literal_string("length"));
        for &name in ARRAY_METHODS_RETURN_ANY {
            keys.push(self.interner().literal_string(name));
        }
        for &name in ARRAY_METHODS_RETURN_BOOLEAN {
            keys.push(self.interner().literal_string(name));
        }
        for &name in ARRAY_METHODS_RETURN_NUMBER {
            keys.push(self.interner().literal_string(name));
        }
        for &name in ARRAY_METHODS_RETURN_VOID {
            keys.push(self.interner().literal_string(name));
        }
        for &name in ARRAY_METHODS_RETURN_STRING {
            keys.push(self.interner().literal_string(name));
        }
        keys
    }

    /// Append tuple indices as string literal keys to the output vector.
    /// Returns the next index to use, or None if a rest element prevents fixed indexing.
    pub(crate) fn append_tuple_indices(
        &self,
        elements: &[TupleElement],
        base: usize,
        out: &mut Vec<TypeId>,
    ) -> Option<usize> {
        let mut index = base;

        for element in elements {
            if element.rest {
                match self.interner().lookup(element.type_id) {
                    Some(TypeData::Tuple(rest_elements)) => {
                        let rest_elements = self.interner().tuple_list(rest_elements);
                        index = self.append_tuple_indices(&rest_elements, index, out)?;
                        continue;
                    }
                    _ => return None,
                }
            }
            out.push(self.interner().literal_string(&index.to_string()));
            index += 1;
        }

        Some(index)
    }

    /// Compute the intersection of multiple keyof key sets.
    /// Returns None if the intersection cannot be computed (e.g., non-literal keys).
    pub(crate) fn intersect_keyof_sets(&self, key_sets: &[TypeId]) -> Option<TypeId> {
        let mut parsed_sets = Vec::with_capacity(key_sets.len());
        for &key_set in key_sets {
            let mut parsed = KeyofKeySet::new();
            if !parsed.insert_type(self.interner(), key_set) {
                return None;
            }
            parsed_sets.push(parsed);
        }

        let mut all_string = true;
        let mut string_possible = true;
        let mut common_literals: Option<FxHashSet<Atom>> = None;
        let mut all_number = true;
        let mut all_symbol = true;
        let mut common_unique_symbols: Option<FxHashSet<SymbolRef>> = None;

        for set in &parsed_sets {
            if set.has_string {
                // string index signatures don't restrict literal key overlap
            } else {
                all_string = false;
                if set.string_literals.is_empty() {
                    string_possible = false;
                } else {
                    common_literals = Some(match common_literals {
                        Some(mut existing) => {
                            existing.retain(|atom| set.string_literals.contains(atom));
                            existing
                        }
                        None => set.string_literals.clone(),
                    });
                }
            }

            if !set.has_number {
                all_number = false;
            }
            if !set.has_symbol {
                all_symbol = false;
            }
            if set.has_symbol {
                // A broad `symbol` key includes every unique symbol.
            } else if set.unique_symbols.is_empty() {
                common_unique_symbols = Some(FxHashSet::default());
            } else {
                common_unique_symbols = Some(match common_unique_symbols {
                    Some(mut existing) => {
                        existing.retain(|symbol| set.unique_symbols.contains(symbol));
                        existing
                    }
                    None => set.unique_symbols.clone(),
                });
            }
        }

        let mut result_keys = Vec::new();
        if string_possible {
            if all_string {
                result_keys.push(TypeId::STRING);
            } else if let Some(common) = common_literals {
                for atom in common {
                    result_keys.push(self.interner().literal_string_atom(atom));
                }
            }
        }
        if all_number {
            result_keys.push(TypeId::NUMBER);
        }
        if all_symbol {
            result_keys.push(TypeId::SYMBOL);
        } else if let Some(common) = common_unique_symbols {
            for symbol in common {
                result_keys.push(self.interner().unique_symbol(symbol));
            }
        }

        if result_keys.is_empty() {
            Some(TypeId::NEVER)
        } else if result_keys.len() == 1 {
            Some(result_keys[0])
        } else {
            Some(self.interner().union(result_keys))
        }
    }
}

#[cfg(test)]
mod exact_property_key_order_tests {
    use super::exact_property_key_sort_key;
    use crate::construction::TypeInterner;
    use crate::type_queries::ExactLiteralPropertyKey;

    #[test]
    fn numeric_and_quoted_numeric_keys_have_a_total_order() {
        let interner = TypeInterner::new();
        let name = interner.intern_string("1");
        let numeric = ExactLiteralPropertyKey {
            name,
            is_symbol_named: false,
            is_string_named: false,
        };
        let quoted = ExactLiteralPropertyKey {
            name,
            is_symbol_named: false,
            is_string_named: true,
        };
        let mut keys = [quoted, numeric];

        keys.sort_by_key(exact_property_key_sort_key);

        assert_eq!(keys, [numeric, quoted]);
    }
}
