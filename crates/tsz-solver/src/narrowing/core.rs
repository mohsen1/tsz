use crate::caches::db::TypeCompilerOptions;
use crate::construction::{QueryDatabase, TypeDatabase};
use crate::def::DefId;
use crate::narrowing::cache::{NarrowExcludingKey, NarrowExcludingStableKey, NarrowingCache};
use crate::narrowing::guard::{GuardSense, TypeGuard};
use crate::narrowing::request::{NarrowingOptions, NarrowingRequest};
use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::type_queries::{UnionMembersKind, classify_for_union_members};
use crate::types::{FunctionShape, LiteralValue, ParamInfo, TypeData, TypeId};
use crate::utils::{TypeIdExt, union_or_single};
use crate::visitor::{
    application_id, index_access_parts, intersection_list_id,
    is_function_type_through_type_constraints, is_object_like_type_through_type_constraints,
    lazy_def_id, literal_value, object_shape_id, object_with_index_shape_id, template_literal_id,
    type_param_info, union_list_id,
};
use tracing::{Level, span, trace};
use tsz_common::interner::Atom;

mod helpers;

#[inline]
pub(crate) fn union_or_single_preserve(db: &dyn TypeDatabase, types: Vec<TypeId>) -> TypeId {
    match types.len() {
        0 => TypeId::NEVER,
        1 => types[0],
        _ => db.union_from_sorted_vec(types),
    }
}

/// Create a union from an already-sorted slice, excluding a single member.
///
/// This avoids allocating a Vec when removing one member from an existing union.
/// For the common case of discriminant exclusion in if-chains (where one member
/// is removed at a time), this eliminates an O(N) Vec allocation per branch.
pub(crate) fn union_excluding_one(
    db: &dyn TypeDatabase,
    members: &[TypeId],
    excluded_idx: usize,
) -> TypeId {
    debug_assert!(excluded_idx < members.len());
    let new_len = members.len() - 1;
    if new_len == 0 {
        return TypeId::NEVER;
    }
    if new_len == 1 {
        // Return the single remaining member
        return if excluded_idx == 0 {
            members[1]
        } else {
            members[0]
        };
    }
    // Build the result without the excluded member
    let mut result = Vec::with_capacity(new_len);
    result.extend_from_slice(&members[..excluded_idx]);
    result.extend_from_slice(&members[excluded_idx + 1..]);
    db.union_from_sorted_vec(result)
}

/// Result of a narrowing operation.
///
/// Represents the types in both branches of a condition.
#[derive(Clone, Debug)]
pub struct NarrowingResult {
    /// The type in the "true" branch of the condition
    pub true_type: TypeId,
    /// The type in the "false" branch of the condition
    pub false_type: TypeId,
}

/// Result of finding discriminant properties in a union.
#[derive(Clone, Debug)]
pub struct DiscriminantInfo {
    /// The name of the discriminant property
    pub property_name: Atom,
    /// Map from literal value to the union member type
    pub variants: Vec<(TypeId, TypeId)>, // (literal_type, member_type)
}

/// Narrowing context for type guards and control flow analysis.
pub struct NarrowingContext<'a> {
    pub(crate) db: &'a dyn QueryDatabase,
    /// Optional `TypeResolver` for resolving Lazy types (e.g., type aliases).
    /// When present, this enables proper narrowing of type aliases like `type Shape = Circle | Square`.
    pub(crate) resolver: Option<&'a dyn TypeResolver>,
    /// Cache for narrowing operations.
    /// If provided, uses the shared cache; otherwise uses a local ephemeral cache.
    pub(crate) cache: std::borrow::Cow<'a, NarrowingCache>,
}

impl<'a> NarrowingContext<'a> {
    pub fn new(db: &'a dyn QueryDatabase) -> Self {
        NarrowingContext {
            db,
            resolver: None,
            cache: std::borrow::Cow::Owned(NarrowingCache::new()),
        }
    }

    /// Create a new context with a shared cache.
    pub fn with_cache(db: &'a dyn QueryDatabase, cache: &'a NarrowingCache) -> Self {
        NarrowingContext {
            db,
            resolver: None,
            cache: std::borrow::Cow::Borrowed(cache),
        }
    }

    /// Set the `TypeResolver` for this context.
    ///
    /// This enables proper resolution of Lazy types (type aliases) during narrowing.
    /// The resolver should be borrowed from the Checker's `TypeEnvironment`.
    pub fn with_resolver(mut self, resolver: &'a dyn TypeResolver) -> Self {
        self.resolver = Some(resolver);
        self
    }

    fn narrowing_options(&self) -> NarrowingOptions {
        NarrowingOptions::new()
            .with_no_unchecked_indexed_access(TypeCompilerOptions::no_unchecked_indexed_access(
                self.db,
            ))
            .with_exact_optional_property_types(TypeCompilerOptions::exact_optional_property_types(
                self.db,
            ))
    }

    pub(crate) fn resolver_generation(&self) -> u64 {
        self.resolver
            .map(|resolver| resolver.resolver_generation().saturating_add(1))
            .unwrap_or(0)
    }

    /// Resolve a type to its structural representation.
    ///
    /// Unwraps:
    /// - Lazy types (evaluates them using resolver if available, otherwise falls back to db)
    /// - Application types (evaluates the generic instantiation)
    ///
    /// This ensures that type aliases, interfaces, and generics are resolved
    /// to their actual structural types before performing narrowing operations.
    pub(crate) fn resolve_type(&self, type_id: TypeId) -> TypeId {
        if let Some(&cached) = self.cache.resolve_cache.borrow().get(&type_id) {
            // A self-mapping cache entry for a Lazy type means a previous resolution
            // attempt failed (TypeEnvironment wasn't populated yet). Re-attempt resolution
            // since the environment may have been populated since then.
            if cached != type_id {
                return cached;
            }
            if matches!(
                self.db.lookup(type_id),
                Some(TypeData::Lazy(_) | TypeData::TypeQuery(_))
            ) {
                // Fall through to re-resolve — don't trust stale self-mapping for Lazy
                // or TypeQuery when a later context has a resolver available.
            } else {
                return cached;
            }
        }

        let Some(_visit_guard) = self.cache.resolve_visit_guard(type_id) else {
            return type_id;
        };
        let result = self.resolve_type_uncached(type_id);
        // Only cache if we actually resolved it — don't cache Lazy → Lazy self-mappings
        // since the TypeEnvironment may be populated later with the real mapping.
        let is_unresolved_symbolic = result == type_id
            && matches!(
                self.db.lookup(type_id),
                Some(TypeData::Lazy(_) | TypeData::TypeQuery(_))
            );
        if !is_unresolved_symbolic {
            self.cache
                .resolve_cache
                .borrow_mut()
                .insert(type_id, result);
        }
        result
    }

    fn remove_impossible_nullish_for_positive_predicate(
        &self,
        source_type: TypeId,
        predicate_type: TypeId,
    ) -> TypeId {
        if super::utils::split_nullish_type(self.db, predicate_type)
            .1
            .is_some()
        {
            return source_type;
        }

        let non_nullish_source = super::utils::remove_nullish_query(self.db, source_type);
        if non_nullish_source == TypeId::NEVER {
            source_type
        } else {
            non_nullish_source
        }
    }

    /// True when distributing `object_type[index_type]` through `index_type`'s
    /// constraint would be *lossy* — i.e. `index_type` is a bare type parameter
    /// constrained by `keyof object_type` and `object_type` has two or more
    /// named properties whose value types are not all identical, with no
    /// applicable index signature to make the access uniform.
    ///
    /// Mirrors the index-access evaluator's `keyof_constraint_distribution_is_lossy`
    /// so narrowing keeps a generic `O[K]` deferred for exactly the shapes the
    /// evaluator defers, instead of collapsing it into the value-type union
    /// `O["a"] | O["b"] | …`. The lossless case (single key, or all value types
    /// identical) is left to distribute through the constraint as before.
    fn keyof_index_distribution_is_lossy(&self, object_type: TypeId, index_type: TypeId) -> bool {
        use crate::visitor::{keyof_inner_type, object_shape_id};

        let Some(info) = type_param_info(self.db, index_type) else {
            return false;
        };
        let Some(constraint) = info.constraint else {
            return false;
        };
        // The constraint must be `keyof X` where `X` is the indexed object
        // (modulo evaluation), so the deferred key space is exactly `keyof O`.
        let Some(keyof_inner) = keyof_inner_type(self.db, constraint).or_else(|| {
            let evaluated = self.db.evaluate_type(constraint);
            (evaluated != constraint)
                .then(|| keyof_inner_type(self.db, evaluated))
                .flatten()
        }) else {
            return false;
        };
        let Some(shape_id) = object_shape_id(self.db, object_type) else {
            return false;
        };
        let shape = self.db.object_shape(shape_id);
        // An applicable index signature makes the access uniform, so the deferred
        // form would resolve through the same signature — not lossy.
        if shape.string_index.is_some() || shape.number_index.is_some() {
            return false;
        }
        // The constrained key space `keyof <inner>` must cover exactly the indexed
        // object's keys, so the deferral applies to `O[K]` (and key-preserving
        // wrappers like `Partial<O>[K]`, whose keys equal `O`'s) but not to an
        // unrelated `B[K]` where `K extends keyof A` and `B`'s keys differ.
        if keyof_inner != object_type && self.db.evaluate_type(keyof_inner) != object_type {
            let Some(inner_shape_id) = object_shape_id(self.db, self.db.evaluate_type(keyof_inner))
            else {
                return false;
            };
            let inner_shape = self.db.object_shape(inner_shape_id);
            let mut object_keys: Vec<Atom> =
                shape.properties.iter().map(|prop| prop.name).collect();
            let mut inner_keys: Vec<Atom> = inner_shape
                .properties
                .iter()
                .map(|prop| prop.name)
                .collect();
            object_keys.sort_unstable();
            inner_keys.sort_unstable();
            if object_keys != inner_keys {
                return false;
            }
        }
        let mut values = shape.properties.iter().map(|prop| prop.type_id);
        let Some(first) = values.next() else {
            return false;
        };
        values.any(|value_type| value_type != first)
    }

    fn resolve_type_uncached(&self, mut type_id: TypeId) -> TypeId {
        // Prevent infinite loops with a fuel counter
        let mut fuel = 100;

        while fuel > 0 {
            fuel -= 1;

            // Single lookup per iteration — dispatch based on TypeData variant
            let data = self.db.lookup(type_id);
            match data {
                // 1. Lazy types (DefId-based)
                Some(TypeData::Lazy(def_id)) => {
                    if let Some(resolver) = self.resolver
                        && let Some(resolved) =
                            resolver.resolve_lazy(def_id, self.db.as_type_database())
                    {
                        type_id = resolved;
                        continue;
                    }
                    type_id = self.db.evaluate_type(type_id);
                    continue;
                }

                // 2. Application types (Generics)
                Some(TypeData::Application(app_id)) => {
                    if let Some(resolver) = self.resolver {
                        let app = self.db.type_application(app_id);
                        if let Some(def_id) = lazy_def_id(self.db, app.base) {
                            let resolved_body =
                                resolver.resolve_lazy(def_id, self.db.as_type_database());
                            let type_params = resolver.get_lazy_type_params(def_id);
                            // A placeholder body of `unknown` / `error` indicates
                            // the Checker registered the DefId but never bound a
                            // real body for it (often happens for cross-file
                            // DefId aliases, e.g. a lib type alias like
                            // `NonNullable` referenced from a namespace-imported
                            // signature). Treat such bodies as unresolved so we
                            // fall through to `db.evaluate_type` instead of
                            // substituting `unknown` into the generic body.
                            let is_placeholder =
                                |t: TypeId| t == TypeId::UNKNOWN || t == TypeId::ERROR;
                            if let (Some(body), Some(params)) = (resolved_body, type_params)
                                && !is_placeholder(body)
                            {
                                // Resolve type args so Lazy aliases become their
                                // structural forms (e.g. Union) for distribution.
                                let resolved_args: Vec<TypeId> =
                                    app.args.iter().map(|&arg| self.resolve_type(arg)).collect();
                                type_id =
                                    crate::instantiation::instantiate::instantiate_generic_cached(
                                        self.db.as_type_database(),
                                        Some(self.db),
                                        body,
                                        &params,
                                        &resolved_args,
                                    );
                                continue;
                            }
                        }
                    }
                    type_id = self.db.evaluate_type(type_id);
                    continue;
                }

                // 3. TemplateLiteral types
                Some(TypeData::TemplateLiteral(spans_id)) => {
                    use crate::types::TemplateSpan;
                    let spans = self.db.template_list(spans_id);
                    let mut new_spans = Vec::with_capacity(spans.len());
                    let mut changed = false;
                    for span in spans.iter() {
                        match span {
                            TemplateSpan::Type(inner_id) => {
                                let resolved = self.resolve_type(*inner_id);
                                if resolved != *inner_id {
                                    changed = true;
                                }
                                new_spans.push(TemplateSpan::Type(resolved));
                            }
                            other => new_spans.push(other.clone()),
                        }
                    }
                    let eval_input = if changed {
                        self.db.template_literal(new_spans)
                    } else {
                        type_id
                    };
                    let evaluated = self.db.evaluate_type(eval_input);
                    if evaluated != type_id {
                        type_id = evaluated;
                        continue;
                    }
                    break;
                }

                // Mapped types with a concrete constraint evaluate to their
                // structural object form. The checker may preserve a mapped
                // alias's `Mapped` identity for display (#15392), but
                // narrowing guards (`in`-presence, property probes) must see
                // the *evaluated* key set — probing the raw mapped node reads
                // its template type for any key, wrongly reporting a property
                // present on `{ [K in never]: boolean }`. A still-generic
                // mapped type evaluates to itself and stays deferred.
                Some(TypeData::Mapped(mapped_id)) => {
                    // The constraint may hold unresolved `Lazy` refs
                    // (`keyof (Left | Right)`), which the resolver-less
                    // evaluator would leave deferred; resolve it first so the
                    // key set is concrete.
                    let mapped = self.db.mapped_type(mapped_id);
                    let resolved_constraint = self.resolve_type(mapped.constraint);
                    let eval_input = if resolved_constraint == mapped.constraint {
                        type_id
                    } else {
                        self.db.mapped(crate::types::MappedType {
                            constraint: resolved_constraint,
                            ..*mapped
                        })
                    };
                    let evaluated = self.db.evaluate_type(eval_input);
                    if evaluated != type_id && evaluated != eval_input {
                        type_id = evaluated;
                        continue;
                    }
                    if evaluated != type_id {
                        type_id = evaluated;
                    }
                    break;
                }

                // 4. KeyOf types
                Some(TypeData::KeyOf(inner)) => {
                    let resolved_inner = self.resolve_type(inner);
                    if resolved_inner != inner {
                        let new_keyof = self.db.keyof(resolved_inner);
                        type_id = self.db.evaluate_type(new_keyof);
                        continue;
                    }
                    break;
                }

                // 5. IndexAccess types
                Some(TypeData::IndexAccess(obj, idx)) => {
                    let resolved_obj = self.resolve_type(obj);
                    let resolved_idx = if let Some(info) = type_param_info(self.db, idx) {
                        // Substitute `K`'s constraint (e.g. `keyof O`) to distribute
                        // `O[K]` — but only when that distribution is *lossless*.
                        // For a bare `K extends keyof O` over an object whose
                        // properties have differing value types, distributing
                        // collapses `O[K]` into the value-type union `O["a"] |
                        // O["b"] | …`, which the index-access evaluator deliberately
                        // defers (tsc keeps a generic `O[K]` an `IndexedAccessType`).
                        // Distributing it here let a truthiness-narrowed
                        // `Partial<O>[K]` local widen its property access to the
                        // value union (correlatedUnions `if (myObj) myObj.name`).
                        // The lossless case (single key, or all value types
                        // identical — e.g. `A[K]` for `A` with one property) still
                        // substitutes, so `value !== null` narrowing of `A[K]`
                        // resolves to the concrete value union it then refines.
                        if self.keyof_index_distribution_is_lossy(resolved_obj, idx) {
                            idx
                        } else {
                            info.constraint.map(|c| self.resolve_type(c)).unwrap_or(idx)
                        }
                    } else {
                        self.resolve_type(idx)
                    };
                    if resolved_obj != obj || resolved_idx != idx {
                        let evaluated = self.db.evaluate_index_access(resolved_obj, resolved_idx);
                        if !matches!(self.db.lookup(evaluated), Some(TypeData::IndexAccess(_, _))) {
                            type_id = evaluated;
                            continue;
                        }
                    }
                    let evaluated = self.db.evaluate_type(type_id);
                    if evaluated != type_id {
                        type_id = evaluated;
                        continue;
                    }
                    break;
                }

                // 6. TypeQuery — resolve `typeof value` through the checker-owned
                // value-space type environment when narrowing discriminants.
                Some(TypeData::TypeQuery(symbol)) => {
                    if let Some(resolver) = self.resolver
                        && let Some(resolved) =
                            resolver.resolve_type_query(symbol, self.db.as_type_database())
                    {
                        type_id = resolved;
                        continue;
                    }
                    let evaluated = self.db.evaluate_type(type_id);
                    if evaluated != type_id {
                        type_id = evaluated;
                        continue;
                    }
                    break;
                }

                // 7. NoInfer — transparent wrapper
                Some(TypeData::NoInfer(inner)) => {
                    type_id = inner;
                    continue;
                }

                // 8. Intersection types with potentially Lazy members
                Some(TypeData::Intersection(members_id)) => {
                    let members = self.db.type_list(members_id);
                    let mut changed = false;
                    let mut resolved_members = Vec::with_capacity(members.len());
                    for &m in members.iter() {
                        let r = self.resolve_type(m);
                        if r != m {
                            changed = true;
                        }
                        resolved_members.push(r);
                    }
                    if changed {
                        type_id = self.db.intersection(resolved_members);
                        continue;
                    }
                    break;
                }

                // 9. Conditional types — resolve inner Lazy/Application types,
                // then re-evaluate to allow distribution/simplification.
                Some(TypeData::Conditional(cond_id)) => {
                    let cond = self.db.get_conditional(cond_id);
                    let resolved_check = self.resolve_type(cond.check_type);
                    let resolved_extends = self.resolve_type(cond.extends_type);
                    if resolved_check != cond.check_type || resolved_extends != cond.extends_type {
                        let new_cond = self.db.conditional(crate::types::ConditionalType {
                            check_type: resolved_check,
                            extends_type: resolved_extends,
                            true_type: cond.true_type,
                            false_type: cond.false_type,
                            is_distributive: cond.is_distributive,
                        });
                        let evaluated = self.db.evaluate_type(new_cond);
                        if evaluated != new_cond && evaluated != type_id {
                            type_id = evaluated;
                            continue;
                        }
                    }
                    // Try evaluating the original conditional
                    let evaluated = self.db.evaluate_type(type_id);
                    if evaluated != type_id {
                        type_id = evaluated;
                        continue;
                    }
                    break;
                }

                // Structural types (Object, Union, Primitive, etc.) — done
                _ => break,
            }
        }

        type_id
    }

    /// Narrow a type based on a typeof check.
    ///
    /// Example: `typeof x === "string"` narrows `string | number` to `string`
    pub fn narrow_by_typeof(&self, source_type: TypeId, typeof_result: &str) -> TypeId {
        let _span =
            span!(Level::TRACE, "narrow_by_typeof", source_type = source_type.0, %typeof_result)
                .entered();

        // TypeScript narrows `any` via typeof only for PRIMITIVE type checks.
        // "object" and "function" are non-primitive and do NOT narrow `any`.
        // `unknown` is always narrowed by all typeof checks.
        if source_type == TypeId::ANY {
            return match typeof_result {
                "string" => TypeId::STRING,
                "number" => TypeId::NUMBER,
                "boolean" => TypeId::BOOLEAN,
                "bigint" => TypeId::BIGINT,
                "symbol" => TypeId::SYMBOL,
                "undefined" => TypeId::UNDEFINED,
                // "object" and "function" do NOT narrow `any`
                _ => source_type,
            };
        }
        if source_type == TypeId::UNKNOWN {
            return match typeof_result {
                "string" => TypeId::STRING,
                "number" => TypeId::NUMBER,
                "boolean" => TypeId::BOOLEAN,
                "bigint" => TypeId::BIGINT,
                "symbol" => TypeId::SYMBOL,
                "undefined" => TypeId::UNDEFINED,
                "object" => self.db.union2(TypeId::OBJECT, TypeId::NULL),
                "function" => self.function_type(),
                _ => source_type,
            };
        }

        let target_type = match typeof_result {
            "string" => TypeId::STRING,
            "number" => TypeId::NUMBER,
            "boolean" => TypeId::BOOLEAN,
            "bigint" => TypeId::BIGINT,
            "symbol" => TypeId::SYMBOL,
            "undefined" => TypeId::UNDEFINED,
            "object" => return self.narrow_to_typeof_object(source_type),
            "function" => return self.narrow_to_function(source_type),
            _ => return source_type,
        };

        self.narrow_to_type(source_type, target_type)
    }

    /// Narrow a type to its `typeof x === "object"` (true-branch) facet.
    ///
    /// `narrow_to_type(source, object)` owns the union and type-parameter
    /// narrowing, but it leaves the empty object type `{}` unchanged: tsz treats
    /// `{}` as assignable to `object`, so the assignability short-circuit
    /// returns the broader `{}`. `typeof` of a primitive is never `"object"`,
    /// though, so the object facet of `{}` is the non-primitive `object` type —
    /// never `{}` itself. Canonicalizing any `{}` left in the narrowed result to
    /// `object` is therefore always correct here, and is what lets
    /// `if (value && typeof value === "object")` narrow an `unknown`/`{}` value
    /// to `object` so a subsequent `"k" in value` is a valid `in` right operand
    /// rather than a spurious TS2638.
    fn narrow_to_typeof_object(&self, source_type: TypeId) -> TypeId {
        let narrowed = self.narrow_to_type(source_type, TypeId::OBJECT);
        self.map_empty_object_to_object(narrowed)
    }

    /// Replace empty object type `{}` constituents with the non-primitive
    /// `object` intrinsic, recursing through unions. Every other shape
    /// (including non-empty object literals, intersections, and type
    /// parameters) is returned unchanged.
    fn map_empty_object_to_object(&self, source_type: TypeId) -> TypeId {
        let resolved = self.resolve_type(source_type);

        if let Some(members_id) = union_list_id(self.db, resolved) {
            let members = self.db.type_list(members_id);
            let mut mapped: Option<Vec<TypeId>> = None;
            for (index, &member) in members.iter().enumerate() {
                let replacement = self.map_empty_object_to_object(member);
                if replacement != member && mapped.is_none() {
                    let mut acc = Vec::with_capacity(members.len());
                    acc.extend_from_slice(&members[..index]);
                    mapped = Some(acc);
                }
                if let Some(acc) = mapped.as_mut() {
                    acc.push(replacement);
                }
            }
            return match mapped {
                Some(acc) => self.db.union(acc),
                None => source_type,
            };
        }

        if crate::type_queries::is_empty_object_type(self.db, resolved) {
            return TypeId::OBJECT;
        }

        source_type
    }

    /// Narrow a type to include only members assignable to target.
    pub fn narrow_to_type(&self, source_type: TypeId, target_type: TypeId) -> TypeId {
        let _span = span!(
            Level::TRACE,
            "narrow_to_type",
            source_type = source_type.0,
            target_type = target_type.0
        )
        .entered();

        // CRITICAL FIX: Resolve Lazy/Ref types to inspect their structure.
        // This fixes the "Missing type resolution" bug where type aliases and
        // generics weren't being narrowed correctly.
        let resolved_source = self.resolve_type(source_type);

        // Gracefully handle resolution failures: if evaluation fails but the input
        // wasn't ERROR, we can't narrow structurally. Return original source to
        // avoid cascading ERRORs through the type system.
        if resolved_source == TypeId::ERROR && source_type != TypeId::ERROR {
            trace!("Source type resolution failed, returning original source");
            return source_type;
        }

        // Resolve target for consistency
        let resolved_target = self.resolve_type(target_type);
        if resolved_target == TypeId::ERROR && target_type != TypeId::ERROR {
            trace!("Target type resolution failed, returning original source");
            return source_type;
        }

        // If source is the target, return it
        if resolved_source == resolved_target {
            trace!("Source type equals target type, returning unchanged");
            return source_type;
        }

        // Special case: unknown can be narrowed to any type through type guards
        // This handles cases like: if (typeof x === "string") where x: unknown
        if resolved_source == TypeId::UNKNOWN {
            trace!("Narrowing unknown to specific type via type guard");
            return target_type;
        }

        // Special case: any can be narrowed to any type through type guards
        // This handles cases like: if (x === null) where x: any
        // CRITICAL: Unlike unknown, any MUST be narrowed to match target type
        if resolved_source == TypeId::ANY {
            trace!("Narrowing any to specific type via type guard");
            return target_type;
        }

        // Decompose Enum(D, inner) so narrowing-to runs on the inner literal
        // union and the nominal enum wrapper is preserved.
        if let Some(narrowed) =
            self.narrow_enum_to_type(source_type, resolved_source, resolved_target)
        {
            return narrowed;
        }

        // If source is a union, filter members
        // Use resolved_source for structural inspection
        if let Some(members) = union_list_id(self.db, resolved_source) {
            let members = self.db.type_list(members);
            trace!(
                "Narrowing union with {} members to type {}",
                members.len(),
                target_type.0
            );
            let mut matching: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    // Resolve alias / `Lazy(DefId)` members before the keep
                    // decision. An unresolved `Lazy` is permissively treated as
                    // assignable to any target, so checking the raw member keeps
                    // every cross-file alias constituent — e.g. narrowing
                    // `AnyObject | AnyArray | AnyMap | AnySet` by `AnyMap` would
                    // retain all four members because each is an unresolved
                    // `Lazy(DefId)` that spuriously "matches" `Map<any, any>`.
                    // Filtering on the resolved structural form keeps only the
                    // genuinely-related constituent, matching tsc's
                    // `getNarrowedType`, which maps over the resolved
                    // constituents. When the member cannot be resolved further
                    // (`resolved_member == member`) fall back to the raw check so
                    // a genuinely-deferred member is still admitted.
                    let resolved_member = self.resolve_type(member);
                    let keep_member = if resolved_member != member {
                        self.is_assignable_to(resolved_member, target_type)
                    } else {
                        self.is_assignable_to(member, target_type)
                    };
                    if keep_member {
                        return Some(member);
                    }
                    // Reverse subtype check: target <: member.
                    // Handles narrowing \`string | number\` by \`"hello"\` where
                    // \`"hello" <: string\` so the member should be kept.
                    // Guard: this reverse proof only applies when either side
                    // has primitive/literal semantics. The narrowing boundary
                    // still owns the relation so budget exhaustion cannot prove
                    // membership and resolver-backed aliases stay authoritative.
                    if (self.is_js_primitive(target_type) || self.is_js_primitive(member))
                        && self.is_subtype_for_narrowing(target_type, member)
                    {
                        // Keep a wide `symbol` member over a `unique symbol`
                        // value; resolved target also catches aliases.
                        if self.keeps_wide_symbol_over_unique(member, resolved_target) {
                            return Some(member);
                        }
                        return Some(target_type);
                    }
                    // CRITICAL FIX: instanceof Array matching
                    // When narrowing by `instanceof Array`, if the member is array-like and target
                    // is a Lazy/Application type (which includes Array<T> interface references),
                    // assume it's the global Array and match the member.
                    // This handles: `x: Message | Message[]` with `instanceof Array` should keep `Message[]`.
                    // At runtime, instanceof only checks prototype chain, not generic type arguments.
                    if self.is_array_like(member) {
                        use crate::type_queries;
                        // Check if target is a type reference or generic application (Array<T>)
                        let is_target_lazy_or_app = type_queries::is_type_reference(self.db, resolved_target)
                            || type_queries::is_generic_type(self.db, resolved_target);

                        trace!("Member is array-like: member={}, target={}, is_target_lazy_or_app={}",
                            member.0, resolved_target.0, is_target_lazy_or_app);

                        if is_target_lazy_or_app {
                            trace!("Array member with lazy/app target (likely Array interface), keeping member");
                            return Some(member);
                        }
                    }
                    None
                })
                .collect();

            // tsc parity (`getNarrowedTypeWorker`): the type-parameter
            // intersection synthesis (`T & target`, via `narrow_type_param`) is
            // only a *fallback*, reached when no declared constituent is
            // structurally related to the candidate. tsc maps each constituent
            // `t` to `target` (target <: t), `t` (t <: target), or `never`, and
            // only when that whole map collapses to `never` does it re-map
            // instantiable members to `t & target`. So when at least one
            // constituent already matches structurally, a bare/unrelated
            // type-parameter member is dropped rather than retained as
            // `T & target`. Synthesizing it eagerly per member (the old
            // behavior) kept a non-callable `V & Function` next to the function
            // member, yielding spurious TS2349/TS2339.
            if matching.is_empty() {
                matching = members
                    .iter()
                    .filter_map(|&member| self.narrow_type_param(member, target_type))
                    .collect();
            }
            self.remove_redundant_intersection_members(&mut matching);

            if matching.is_empty() {
                trace!("No matching members found, returning NEVER");
                return TypeId::NEVER;
            } else if matching.len() == 1 {
                trace!("Found single matching member, returning {}", matching[0].0);
                return matching[0];
            }
            trace!(
                "Found {} matching members, creating new union",
                matching.len()
            );
            return self.db.union(matching);
        }

        // Check if this is a type parameter that needs narrowing
        // Use resolved_source to handle type parameters behind aliases
        if let Some(narrowed) = self.narrow_type_param(resolved_source, target_type) {
            trace!("Narrowed type parameter to {}", narrowed.0);
            return narrowed;
        }

        // Task 13: Handle boolean -> literal narrowing
        // When narrowing boolean to true or false, return the corresponding literal
        if resolved_source == TypeId::BOOLEAN {
            let is_target_true = if let Some(lit) = literal_value(self.db, resolved_target) {
                matches!(lit, LiteralValue::Boolean(true))
            } else {
                resolved_target == TypeId::BOOLEAN_TRUE
            };

            if is_target_true {
                trace!("Narrowing boolean to true");
                return TypeId::BOOLEAN_TRUE;
            }

            let is_target_false = if let Some(lit) = literal_value(self.db, resolved_target) {
                matches!(lit, LiteralValue::Boolean(false))
            } else {
                resolved_target == TypeId::BOOLEAN_FALSE
            };

            if is_target_false {
                trace!("Narrowing boolean to false");
                return TypeId::BOOLEAN_FALSE;
            }
        }

        if resolved_target == TypeId::OBJECT
            && crate::visitors::visitor_predicates::contains_type_parameters(
                self.db,
                resolved_source,
            )
        {
            return self.db.intersection2(source_type, TypeId::OBJECT);
        }

        // Check if source is assignable to target using resolved types for comparison
        if self.is_assignable_to(resolved_source, resolved_target) {
            trace!("Source type is assignable to target, returning source");
            source_type
        } else if self.keeps_wide_symbol_over_unique(resolved_source, resolved_target) {
            // Never collapse a wide `symbol` to a `unique symbol` (see helper).
            trace!("Keeping wide symbol over unique-symbol value");
            source_type
        } else if self.is_subtype_for_narrowing(resolved_target, resolved_source) {
            // Check if target is a subtype of source (reverse narrowing).
            // This handles cases like narrowing string to "hello" where "hello"
            // is a subtype of string. The inference engine uses this to narrow
            // upper bounds by lower bounds.
            //
            // Use the resolver-backed subtype check, not the bare
            // `is_subtype_of_with_db`: without a resolver, named class/interface
            // shapes can't be mapped to a `DefId`, so
            // `requires_explicit_declared_index_signature` degrades and an
            // interface target (e.g. a user predicate's `is NodeSource`) is
            // wrongly judged a subtype of an index-signature record source
            // (`{ [P in string]: unknown }`). That replaced the record with the
            // interface instead of intersecting, dropping the index signature
            // accumulated by an earlier guard (kysely dynamic/* TS2339s).
            trace!("Target is subtype of source, returning target");
            target_type
        } else {
            trace!("Source type is not assignable to target, returning NEVER");
            TypeId::NEVER
        }
    }

    /// Narrow a type to its nullish facet for the true branch of a loose
    /// `x == null` / `x != null` comparison, which matches both `null` and
    /// `undefined`.
    ///
    /// This is the exact dual of [`Self::narrow_excluding_type`] used in the
    /// inequality (false) branch: where exclusion drops the `null`/`undefined`
    /// members, this keeps only them. As with that branch, `any` is preserved
    /// unchanged — tsc's `narrowTypeByEquality` returns the type as-is when it
    /// is `any`, so the nullish branch must not collapse `any` to
    /// `null | undefined`. Every other source keeps only its `null`/`undefined`
    /// members, so a non-nullable source narrows to `never` (the true branch is
    /// unreachable) while e.g. `string | null` narrows to `null`.
    pub(crate) fn narrow_to_nullish(&self, source_type: TypeId) -> TypeId {
        if self.resolve_type(source_type) == TypeId::ANY {
            return source_type;
        }
        let nullish = self.db.union2(TypeId::NULL, TypeId::UNDEFINED);
        self.narrow_to_type(source_type, nullish)
    }

    /// Check if a literal type is assignable to a target for narrowing purposes.
    ///
    /// Handles union decomposition: if the target is a union, checks each member.
    /// Falls back to `narrow_to_type` to determine if the literal can narrow to the target.
    pub fn literal_assignable_to(&self, literal: TypeId, target: TypeId) -> bool {
        if literal == target || target == TypeId::ANY || target == TypeId::UNKNOWN {
            return true;
        }

        if let UnionMembersKind::Union(members) = classify_for_union_members(self.db, target) {
            return members
                .iter()
                .any(|&member| self.literal_assignable_to(literal, member));
        }

        self.narrow_to_type(literal, target) != TypeId::NEVER
    }

    /// Exclude the positive (true-branch) narrowing from a source for the
    /// false branch of a type-predicate guard, mirroring tsc's
    /// `getNarrowedTypeWorker(assumeTrue=false)`:
    /// `filterType(type, t => !isTypeSubsetOf(t, trueType))`.
    ///
    /// tsc's `filterType` is a *shallow* pass over the source union's top-level
    /// members, and `isTypeSubsetOf` is a pure identity/containment test (no
    /// structural subtype walk, no descent into a member's intersection
    /// sub-structure). The general [`Self::narrow_excluding_type`] instead
    /// recurses into every intersection/union member and runs a deep
    /// `is_assignable_to` per member; over a recursive-schema union (typebox /
    /// ts-morph `value is T` guards, where each nested schema instantiates to a
    /// distinct `TypeId` so the `(source, excluded)` memo never hits) that
    /// recursion is exponential and was the dominant non-termination frame.
    ///
    /// This boundary keeps the false-branch predicate exclusion on tsc's cheap
    /// O(N) shallow path. It returns `None` when the shallow filter cannot
    /// reduce the source (every member survives `isTypeSubsetOf`), so the caller
    /// can fall back to its structural-assignability member pass for the cases
    /// tsc covers through `directlyRelated`/intersection construction.
    pub fn narrow_excluding_positive_subset(
        &self,
        source_type: TypeId,
        positive_type: TypeId,
    ) -> Option<TypeId> {
        // `any`/`unknown` are never reduced by exclusion (tsc returns the source
        // unchanged), so there is nothing for the shallow filter to do.
        if source_type == TypeId::ANY || source_type == TypeId::UNKNOWN {
            return None;
        }

        let resolved_source = self.resolve_type(source_type);
        let Some(members) = union_list_id(self.db, resolved_source) else {
            // A non-union source is dropped to `never` iff it is a subset of the
            // positive type; otherwise it is unrelated and kept. tsc:
            // `type.flags & Never || f(type) ? type : neverType`.
            return self
                .is_type_subset_of(resolved_source, positive_type)
                .then_some(TypeId::NEVER);
        };

        let members = self.db.type_list(members);
        let remaining: Vec<TypeId> = members
            .iter()
            .copied()
            .filter(|&member| !self.is_type_subset_of(member, positive_type))
            .collect();

        if remaining.len() == members.len()
            || remaining
                .iter()
                .any(|&member| self.is_assignable_to(member, positive_type))
        {
            // Identity/containment did not catch every positive-branch member.
            // Keep the pass top-level-only, but allow structural equivalence
            // against the already-computed positive type. This covers freshly
            // materialized true-branch shapes such as #52984 deep-path
            // predicates without returning to recursive intersection descent.
            let structurally_remaining: Vec<TypeId> = members
                .iter()
                .copied()
                .filter(|&member| !self.is_assignable_to(member, positive_type))
                .collect();
            if structurally_remaining.len() == members.len() {
                return None;
            }
            return Some(match structurally_remaining.as_slice() {
                [] => TypeId::NEVER,
                [single] => *single,
                _ => self.db.union(structurally_remaining),
            });
        }
        // The pure identity/containment filter handled the positive members.
        Some(match remaining.as_slice() {
            [] => TypeId::NEVER,
            [single] => *single,
            _ => self.db.union(remaining),
        })
    }

    /// tsc's `isTypeSubsetOf`: a pure identity/containment relation used by
    /// false-branch predicate exclusion. `source` is a subset of `target` when
    /// it is identical, is `never`, or — when `target` is a union — every
    /// constituent of `source` is one of `target`'s constituents. No structural
    /// subtype walk is performed (that is the divergence this avoids).
    fn is_type_subset_of(&self, source: TypeId, target: TypeId) -> bool {
        if source == target || source == TypeId::NEVER {
            return true;
        }
        let Some(target_members) = union_list_id(self.db, target) else {
            return false;
        };
        let target_members = self.db.type_list(target_members);
        if let Some(source_members) = union_list_id(self.db, source) {
            let source_members = self.db.type_list(source_members);
            source_members.iter().all(|s| target_members.contains(s))
        } else {
            target_members.contains(&source)
        }
    }

    /// Narrow a type to exclude members assignable to target.
    ///
    /// Memoizing entry point. The recursive body (`narrow_excluding_type_uncached`)
    /// re-enters on every intersection / type-parameter / union-intersection
    /// member, so a recursive-schema union (typebox / ts-morph `value is T`
    /// false-branch guards) expands the same `(source, excluded)` subtree
    /// exponentially. The memo collapses that to linear and the visiting set
    /// breaks the `Lazy`-alias resolution cycle — returning the source unchanged
    /// on re-entry, which matches tsc's non-exhaustive exclusion over a recursive
    /// union (issue #13242 / #13250).
    pub fn narrow_excluding_type(&self, source_type: TypeId, excluded_type: TypeId) -> TypeId {
        // Intrinsics and identity pairs are answered without recursion; skip the
        // memo and budget bookkeeping for them so the common shallow path stays
        // allocation- and borrow-free.
        if source_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if source_type.is_intrinsic() && excluded_type.is_intrinsic() {
            return self.narrow_excluding_type_uncached(source_type, excluded_type);
        }

        let _frame = self.cache.enter_exclusion_frame();
        self.narrow_excluding_type_budgeted(source_type, excluded_type)
    }

    /// Override the per-request exclusion-narrowing work budget shared by every
    /// `narrow_excluding_*` family.
    ///
    /// `0` restores the default exclusion-narrowing work budget. Lets tests
    /// exercise the bail path deterministically without driving a million-step
    /// recursion.
    #[cfg(test)]
    pub(crate) fn set_narrow_excluding_budget(&self, budget: u32) {
        self.cache.set_narrow_excluding_budget(budget);
    }

    /// Memoized, budget-charged body of [`Self::narrow_excluding_type`]. Always
    /// reached with the per-request fuel primed by the outermost frame.
    fn narrow_excluding_type_budgeted(&self, source_type: TypeId, excluded_type: TypeId) -> TypeId {
        let resolver_generation = self.resolver_generation();
        let stable_key = NarrowExcludingStableKey {
            source: source_type,
            excluded: excluded_type,
        };
        let key = NarrowExcludingKey {
            source: source_type,
            excluded: excluded_type,
            resolver_generation,
        };
        if let Some(cached) = self
            .cache
            .narrow_excluding_cache
            .borrow()
            .get(&stable_key, resolver_generation)
        {
            return cached;
        }
        // Charge one unit of the per-request budget for each fresh exclusion
        // narrow. On exhaustion, bail to the unchanged source — the same
        // conservative answer the in-flight cycle guard below returns — so a
        // breadth-fanned recursion that keeps minting fresh intersections
        // terminates instead of spinning unbounded.
        if !self.cache.charge_exclusion_work() {
            return source_type;
        }
        // Re-entry on the same `(source, excluded)` pair is a recursive-alias
        // cycle: leave the source unchanged so the in-flight outer frame owns the
        // result, mirroring tsc's bounded exclusion over a recursive union.
        let Some(_visit_guard) = self.cache.narrow_excluding_visit_guard(key) else {
            return source_type;
        };
        let relation_budget_events = self.cache.relation_budget_event_count();
        let result = self.narrow_excluding_type_uncached(source_type, excluded_type);
        // Only memoize when the whole subtree stayed within budget. A result that
        // bottomed out the fuel is truncated and request-local, so caching it
        // would poison a later, fully-budgeted request with the conservative
        // answer.
        if self.cache.exclusion_within_budget()
            && self.cache.relation_budget_event_count() == relation_budget_events
        {
            self.cache.narrow_excluding_cache.borrow_mut().insert(
                stable_key,
                resolver_generation,
                result,
            );
        }
        result
    }

    fn narrow_excluding_type_uncached(&self, source_type: TypeId, excluded_type: TypeId) -> TypeId {
        // `any` cannot be narrowed by exclusion — it remains `any` in all branches.
        // Without this guard, the `is_assignable_to(any, X)` check below would always
        // succeed (any is assignable to everything), incorrectly producing `never`.
        if source_type == TypeId::ANY {
            return TypeId::ANY;
        }

        // Note: Do NOT resolve Lazy/Application types here. This function is called
        // recursively from narrow_type_param_excluding, which relies on TypeId identity
        // comparisons (narrowed_constraint == constraint). Resolving Lazy types changes
        // the TypeId, breaking those comparisons and producing incorrect intersections
        // (e.g., T & Date instead of excluding T from T | number).
        //
        // Lazy type resolution for the top-level source is handled in narrow_type()
        // before dispatching to this function.

        // Decompose `Enum(D, inner)` so exclusion runs on the inner literal
        // union and the nominal wrapper survives (issue #6823).
        if let Some(narrowed) =
            self.narrow_enum_excluding_types(source_type, std::slice::from_ref(&excluded_type))
        {
            return narrowed;
        }

        if let Some(members) = intersection_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
            let mut narrowed_members = Vec::with_capacity(members.len());
            let mut changed = false;
            for &member in members.iter() {
                let narrowed = self.narrow_excluding_type(member, excluded_type);
                if narrowed == TypeId::NEVER {
                    return TypeId::NEVER;
                }
                if narrowed != member {
                    changed = true;
                }
                narrowed_members.push(narrowed);
            }
            if !changed {
                return source_type;
            }
            return self.db.intersection(narrowed_members);
        }

        // If source is a union, filter out matching members
        if let Some(members) = union_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
            let remaining: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    if intersection_list_id(self.db, member).is_some() {
                        return self
                            .narrow_excluding_type(member, excluded_type)
                            .non_never();
                    }
                    if let Some(narrowed) = self.narrow_type_param_excluding(member, excluded_type)
                    {
                        return narrowed.non_never();
                    }
                    // A union member that is itself an alias (`Lazy`/`Application`)
                    // whose body is a *union* must be descended into, not kept
                    // whole. tsc's `filterType` excludes per top-level constituent,
                    // so a member like `Updater<P, R> = R | ((p) => R)` has its
                    // callable constituent stripped in the `!isFunction(x)` branch
                    // — instead of surviving because the *whole* alias is not
                    // assignable to the excluded type (the shallow
                    // `member_excluded_by` check below). The descent is the dual of
                    // the `intersection`/type-parameter member recursion already
                    // handled above, and is bounded by the shared exclusion budget
                    // and the `narrow_excluding_visiting` cycle guard. Only adopt it
                    // when the alias expands to a union: a non-union alias is left to
                    // the assignability check below, which already sees through it
                    // (#14739).
                    if matches!(
                        self.db.lookup(member),
                        Some(TypeData::Lazy(_) | TypeData::Application(_))
                    ) {
                        let resolved_member = self.resolve_type(member);
                        if resolved_member != member
                            && union_list_id(self.db, resolved_member).is_some()
                        {
                            let narrowed =
                                self.narrow_excluding_type(resolved_member, excluded_type);
                            // Preserve the alias member's identity/display when
                            // nothing inside it was excluded.
                            if narrowed == resolved_member {
                                return Some(member);
                            }
                            return narrowed.non_never();
                        }
                    }
                    // A `boolean` (or `true`/`false`) union member is the implicit
                    // `true | false` union; excluding one boolean literal must leave
                    // the other rather than keeping the whole member. The top-level
                    // boolean special-case below only fires when the source is exactly
                    // boolean, so recurse here so `Ann | boolean` minus `true` yields
                    // `Ann | false` (mirrors tsc's `boolean` literal decomposition).
                    if matches!(
                        member,
                        TypeId::BOOLEAN | TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE
                    ) {
                        return self
                            .narrow_excluding_type(member, excluded_type)
                            .non_never();
                    }
                    if self.member_excluded_by(member, excluded_type) {
                        None
                    } else {
                        Some(member)
                    }
                })
                .collect();

            tracing::trace!(
                remaining_count = remaining.len(),
                remaining = ?remaining.iter().map(|t| t.0).collect::<Vec<_>>(),
                "narrow_excluding_type: union filter result"
            );
            if remaining.is_empty() {
                return TypeId::NEVER;
            } else if remaining.len() == 1 {
                return remaining[0];
            }
            return self.db.union(remaining);
        }

        if let Some(narrowed) = self.narrow_type_param_excluding(source_type, excluded_type) {
            return narrowed;
        }

        // Special case: boolean type (treat as true | false union)
        // Task 13: Fix Boolean Narrowing Logic
        // When excluding true or false from boolean, return the other literal
        // When excluding both true and false from boolean, return never
        if source_type == TypeId::BOOLEAN
            || source_type == TypeId::BOOLEAN_TRUE
            || source_type == TypeId::BOOLEAN_FALSE
        {
            // Check if excluded_type is a boolean literal
            let is_excluding_true = if let Some(lit) = literal_value(self.db, excluded_type) {
                matches!(lit, LiteralValue::Boolean(true))
            } else {
                excluded_type == TypeId::BOOLEAN_TRUE
            };

            let is_excluding_false = if let Some(lit) = literal_value(self.db, excluded_type) {
                matches!(lit, LiteralValue::Boolean(false))
            } else {
                excluded_type == TypeId::BOOLEAN_FALSE
            };

            // Handle exclusion from boolean, true, or false
            if source_type == TypeId::BOOLEAN {
                if is_excluding_true {
                    // Excluding true from boolean -> return false
                    return TypeId::BOOLEAN_FALSE;
                } else if is_excluding_false {
                    // Excluding false from boolean -> return true
                    return TypeId::BOOLEAN_TRUE;
                }
                // If excluding BOOLEAN, let the final is_assignable_to check handle it below
            } else if source_type == TypeId::BOOLEAN_TRUE {
                if is_excluding_true {
                    // Excluding true from true -> return never
                    return TypeId::NEVER;
                }
                // For other cases (e.g., excluding BOOLEAN from TRUE),
                // let the final is_assignable_to check handle it below
            } else if source_type == TypeId::BOOLEAN_FALSE && is_excluding_false {
                // Excluding false from false -> return never
                return TypeId::NEVER;
            }
            // For other cases, let the final is_assignable_to check handle it below
            // CRITICAL: Do NOT return source_type here.
            // Fall through to the standard is_assignable_to check below.
            // This handles edge cases like narrow_excluding_type(TRUE, BOOLEAN) -> NEVER
        }

        // If source is assignable to excluded, return never
        if self.member_excluded_by(source_type, excluded_type) {
            TypeId::NEVER
        } else {
            source_type
        }
    }

    /// Whether `member` is removed when narrowing `source` by excluding
    /// `excluded_type` (e.g. the true branch of `x !== undefined`).
    ///
    /// Beyond ordinary assignability, `void`'s sole inhabitant is `undefined`,
    /// so excluding `undefined` removes a `void` member. This mirrors tsc's
    /// `NEUndefined`/`EQUndefined` type facts, where `void` carries
    /// `EQUndefined` but not `NEUndefined`: a `void` value can equal
    /// `undefined`, so `x !== undefined` (and `typeof x !== "undefined"`, plus
    /// the symmetric `=== undefined` false branch) discards it. Without this,
    /// `boolean | void` stays `boolean | void` after `x !== undefined`,
    /// producing a spurious TS2322 against a `boolean` target.
    fn member_excluded_by(&self, member: TypeId, excluded_type: TypeId) -> bool {
        if excluded_type == TypeId::UNDEFINED && member == TypeId::VOID {
            return true;
        }
        self.is_assignable_to(member, excluded_type)
    }

    /// Narrow a type by excluding multiple types at once (batched version).
    ///
    /// This is an optimized version of `narrow_excluding_type` for cases like
    /// switch default clauses where we need to exclude many types at once.
    /// It avoids creating intermediate union types and reduces complexity from O(N²) to O(N).
    ///
    /// # Arguments
    /// * `source_type` - The type to narrow (typically a union)
    /// * `excluded_types` - Types to exclude from the source
    ///
    /// # Returns
    /// The narrowed type with all excluded types removed
    pub fn narrow_excluding_types(&self, source_type: TypeId, excluded_types: &[TypeId]) -> TypeId {
        if excluded_types.is_empty() {
            return source_type;
        }

        // Enum decomposition for the batched path (issue #6823).
        if let Some(narrowed) = self.narrow_enum_excluding_types(source_type, excluded_types) {
            return narrowed;
        }

        // For small lists, use sequential narrowing (avoids HashSet overhead)
        if excluded_types.len() <= 4 {
            let mut result = source_type;
            for &excluded in excluded_types {
                result = self.narrow_excluding_type(result, excluded);
                if result == TypeId::NEVER {
                    return TypeId::NEVER;
                }
            }
            return result;
        }

        // For larger lists, use HashSet for O(1) lookup
        let excluded_set: rustc_hash::FxHashSet<TypeId> = excluded_types.iter().copied().collect();

        // Handle union source type
        if let Some(members) = union_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
            let remaining: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    // Fast path: direct identity check against the set
                    if excluded_set.contains(&member) {
                        return None;
                    }

                    // Handle intersection members
                    if intersection_list_id(self.db, member).is_some() {
                        return self
                            .narrow_excluding_types(member, excluded_types)
                            .non_never();
                    }

                    // Handle type parameters
                    if let Some(narrowed) =
                        self.narrow_type_param_excluding_set(member, &excluded_set)
                    {
                        return narrowed.non_never();
                    }

                    // Slow path: check assignability for complex cases
                    // This handles cases where the member isn't identical to an excluded type
                    // but might still be assignable to one (e.g., literal subtypes), and the
                    // `void`-vs-`undefined` exclusion (see `member_excluded_by`).
                    for &excluded in &excluded_set {
                        if self.member_excluded_by(member, excluded) {
                            return None;
                        }
                    }
                    Some(member)
                })
                .collect();

            if remaining.is_empty() {
                return TypeId::NEVER;
            } else if remaining.len() == 1 {
                return remaining[0];
            }
            return self.db.union(remaining);
        }

        // Handle single type (not a union)
        if excluded_set.contains(&source_type) {
            return TypeId::NEVER;
        }

        // Check assignability for single type
        for &excluded in &excluded_set {
            if self.member_excluded_by(source_type, excluded) {
                return TypeId::NEVER;
            }
        }

        source_type
    }

    pub fn narrow_type(&self, source_type: TypeId, guard: &TypeGuard, sense: GuardSense) -> TypeId {
        if !matches!(guard, TypeGuard::Predicate { .. }) {
            return self.narrow_type_uncached(source_type, guard, sense);
        }
        let request = NarrowingRequest::new(source_type, guard.clone(), sense);
        self.narrow_predicate_cached(&request)
    }

    /// Avoids re-cloning the guard when the caller already holds a `NarrowingRequest`.
    pub fn narrow_type_with_request(&self, request: &NarrowingRequest) -> TypeId {
        if !matches!(request.guard(), TypeGuard::Predicate { .. }) {
            return self.narrow_type_uncached(
                request.source_type(),
                request.guard(),
                request.sense(),
            );
        }
        self.narrow_predicate_cached(request)
    }

    fn narrow_predicate_cached(&self, request: &NarrowingRequest) -> TypeId {
        let generation = self.resolver_generation();
        let key = request.stable_cache_key(self.narrowing_options());
        if let Some(cached) = self.cache.narrow_type_cache.borrow().get(&key, generation) {
            return cached;
        }
        let relation_budget_events = self.cache.relation_budget_event_count();
        let narrowed =
            self.narrow_type_uncached(request.source_type(), request.guard(), request.sense());
        if self.cache.relation_budget_event_count() == relation_budget_events {
            self.cache
                .narrow_type_cache
                .borrow_mut()
                .insert(key, generation, narrowed);
        }
        narrowed
    }

    fn narrow_type_uncached(
        &self,
        source_type: TypeId,
        guard: &TypeGuard,
        sense: GuardSense,
    ) -> TypeId {
        let sense = matches!(sense, GuardSense::Positive);

        // For generic IndexAccess types (e.g., `Entries[EntryId]` where EntryId is a
        // type parameter), we must preserve the original deferred form after narrowing.
        // Without this, eagerly resolving to the constraint breaks assignability with
        // the original return type (e.g., false TS2322 in quickinfoTypeAtReturn...).
        let original_generic_index =
            if let Some(TypeData::IndexAccess(obj, idx)) = self.db.lookup(source_type) {
                let is_generic = crate::type_queries::contains_type_parameters_db(self.db, obj)
                    || crate::type_queries::contains_type_parameters_db(self.db, idx);
                if is_generic { Some(source_type) } else { None }
            } else {
                None
            };

        // Resolve IndexAccess types (e.g., `A[K]`) to their concrete form before
        // narrowing, so that opaque generic index access types can be decomposed
        // for guard-based narrowing (e.g., excluding null from `number | null`).
        let resolved_source = if matches!(
            self.db.lookup(source_type),
            Some(TypeData::IndexAccess(_, _))
        ) {
            self.resolve_type(source_type)
        } else {
            source_type
        };

        let narrowed = self.narrow_type_inner(resolved_source, guard, sense);

        // For generic IndexAccess, wrap the result to preserve assignability.
        if let Some(original) = original_generic_index {
            if narrowed == resolved_source || narrowed == original {
                return original;
            }
            if narrowed != TypeId::NEVER {
                return self.db.intersection2(original, narrowed);
            }
        }

        narrowed
    }

    fn narrow_type_inner(&self, source_type: TypeId, guard: &TypeGuard, sense: bool) -> TypeId {
        match guard {
            TypeGuard::Typeof(typeof_kind) => {
                let type_name = typeof_kind.as_str();
                if sense {
                    self.narrow_by_typeof(source_type, type_name)
                } else {
                    // TypeScript does NOT narrow `any` in the false branch of typeof.
                    // The true branch narrows `any` to the primitive type, but the
                    // false branch keeps `any` unchanged.
                    if source_type == TypeId::ANY {
                        return source_type;
                    }
                    // Negation: exclude typeof type — resolve Lazy types first
                    let resolved = self.resolve_for_exclusion_narrowing(source_type);
                    self.narrow_by_typeof_negation(resolved, type_name)
                }
            }

            TypeGuard::Instanceof(instance_type, _is_explicit_global) => {
                // TypeScript narrows `any` via instanceof for specific constructors
                // (e.g. Error, Date) but NOT for Function or Object. Handle this
                // in the sense-specific branches below.
                if source_type == TypeId::ANY && !sense {
                    // False branch: `any` stays `any` (can't exclude from `any`)
                    return source_type;
                }

                if sense {
                    // Positive branch: `any` narrows to instance type unless
                    // the instance type is Function or Object.
                    if source_type == TypeId::ANY {
                        // Resolve Lazy types before checking Function/Object
                        let resolved_instance = self.resolve_type(*instance_type);
                        if self.is_object_interface(resolved_instance)
                            || crate::type_queries::is_function_interface_structural(
                                self.db,
                                resolved_instance,
                            )
                        {
                            return TypeId::ANY;
                        }
                        return *instance_type;
                    }
                    // Positive: x instanceof Class
                    // Special case: `unknown` instanceof X narrows to X (or object if X unknown)
                    // This must be handled here in the solver, not in the checker.
                    if source_type == TypeId::UNKNOWN {
                        return *instance_type;
                    }

                    // When an empty object `{}` (e.g., from truthiness-narrowed `unknown`) is
                    // narrowed by `instanceof Object`, we return `TypeId::OBJECT` (the intrinsic
                    // non-primitive type) instead of the Object interface. This ensures that
                    // the result is not considered an "empty object" for TS2638 purposes.
                    //
                    // TSC emits TS2638 "may represent a primitive value" for truthiness-narrowed
                    // `unknown` used with `in`, but NOT after `instanceof Object` because the
                    // instanceof check confirms the value is a non-primitive object.
                    let resolved_instance = self.resolve_type(*instance_type);
                    if crate::type_queries::is_empty_object_type(self.db, source_type)
                        && self.is_object_interface(resolved_instance)
                    {
                        return TypeId::OBJECT;
                    }

                    // CRITICAL: The payload is already the Instance Type (extracted by Checker)
                    // Use narrow_by_instance_type for instanceof-specific semantics:
                    // type parameters with matching constraints are kept, but anonymous
                    // object types that happen to be structurally compatible are excluded.
                    // Primitive types are filtered out since they can never pass instanceof.
                    let narrowed = self.narrow_by_instance_type(source_type, *instance_type);

                    if narrowed != TypeId::NEVER || source_type == TypeId::NEVER {
                        return narrowed;
                    }

                    // Fallback 1: If standard narrowing returns NEVER but source wasn't NEVER,
                    // it might be an interface vs class check (which is allowed in TS).
                    // Only create intersection if the types don't have conflicting properties.
                    if self.are_instanceof_types_overlapping(source_type, *instance_type) {
                        let intersection = self.db.intersection2(source_type, *instance_type);
                        if intersection != TypeId::NEVER {
                            return intersection;
                        }
                    } else {
                        // Types have conflicting properties — intersection is uninhabitable.
                        return TypeId::NEVER;
                    }

                    // Fallback 2: If even intersection construction fails,
                    // narrow to object-like types. On the true branch of instanceof,
                    // we know the value must be some kind of object.
                    self.narrow_to_objectish(source_type)
                } else {
                    // Negative: !(x instanceof Class)
                    // Keep primitives (they can never pass instanceof) and exclude
                    // non-primitive types assignable to the instance type.
                    // For `instanceof Object`, this correctly excludes all non-primitives
                    // since every non-primitive is an Object instance at runtime.
                    self.narrow_by_instanceof_false(source_type, *instance_type)
                }
            }

            TypeGuard::LiteralEquality(literal_type) => {
                if sense {
                    // Equality: narrow to the literal type
                    self.narrow_to_type(source_type, *literal_type)
                } else {
                    if !crate::type_queries::is_unit_type(self.db, *literal_type) {
                        return source_type;
                    }
                    // Inequality: exclude the literal type — resolve Lazy types first
                    let resolved = self.resolve_for_exclusion_narrowing(source_type);
                    self.narrow_excluding_type(resolved, *literal_type)
                }
            }

            TypeGuard::NullishEquality => {
                if sense {
                    // Equality `x == null` (true branch): keep only the nullish
                    // facet of the source. `any` is preserved (not collapsed to
                    // `null | undefined`); see `narrow_to_nullish`.
                    self.narrow_to_nullish(source_type)
                } else {
                    // Inequality: exclude null and undefined — resolve Lazy types first
                    let resolved = self.resolve_for_exclusion_narrowing(source_type);
                    let without_null = self.narrow_excluding_type(resolved, TypeId::NULL);
                    self.narrow_excluding_type(without_null, TypeId::UNDEFINED)
                }
            }

            TypeGuard::Truthy => {
                if sense {
                    // Truthy: remove null and undefined (TypeScript doesn't narrow other falsy values)
                    self.narrow_by_truthiness(source_type)
                } else {
                    // Falsy: narrow to the falsy component(s)
                    // This handles cases like: if (!x) where x: string → "" in false branch
                    self.narrow_to_falsy(source_type)
                }
            }

            TypeGuard::Discriminant {
                property_path,
                value_type,
            } => {
                // Use narrow_by_discriminant_for_type which handles type parameters
                // by narrowing the constraint and returning T & NarrowedConstraint
                self.narrow_by_discriminant_for_type(source_type, property_path, *value_type, sense)
            }

            TypeGuard::InProperty(property_name) => {
                if sense {
                    // Positive: "prop" in x - narrow to types that have the property
                    self.narrow_by_property_presence(source_type, *property_name, true)
                } else {
                    // Negative: !("prop" in x) - narrow to types that don't have the property
                    self.narrow_by_property_presence(source_type, *property_name, false)
                }
            }

            TypeGuard::Predicate { type_id, asserts } => {
                match type_id {
                    Some(target_type) => {
                        // Type guard with specific type: is T or asserts T
                        if sense {
                            // True branch: narrow source to the predicate type.
                            // Following TSC's narrowType logic:
                            // 1. For unions: filter members using narrow_to_type
                            // 2. For non-unions:
                            //    a. source <: target → return source
                            //    b. target <: source → return target
                            //    c. otherwise → return source & target
                            //
                            // Following TSC's narrowType logic which uses
                            // isTypeSubtypeOf (not isTypeAssignableTo) to decide
                            // whether source is already specific enough.
                            //
                            // If source is a strict subtype of the target, return
                            // source (it's already more specific). If target is a
                            // strict subtype of source, return target (narrowing
                            // down). Otherwise, return the intersection.
                            //
                            // narrow_to_type uses assignability internally, which is
                            // too loose for type predicates (e.g. {} is assignable to
                            // Record<string,unknown> but not a subtype).
                            let resolved_source = self.resolve_type(source_type);
                            let resolved_target = self.resolve_type(*target_type);
                            let effective_source = self
                                .remove_impossible_nullish_for_positive_predicate(
                                    resolved_source,
                                    resolved_target,
                                );

                            if effective_source == resolved_target {
                                effective_source
                            } else if effective_source == TypeId::ANY {
                                // A user-defined type predicate does not narrow `any`
                                // away from `any` when the asserted type is exactly the
                                // global `Object` or `Function` interface. This mirrors
                                // tsc's `narrowTypeByTypePredicate`, which skips
                                // narrowing when `isTypeAny(type)` and the predicate type
                                // is `globalObjectType` or `globalFunctionType` (and the
                                // `instanceof Object`/`Function` handling above). For any
                                // other asserted type, `any` narrows to the predicate
                                // type. Without this, `any` collapses to `Object` and a
                                // following `Array.isArray` guard intersects it down to
                                // `never` (false TS2339 in ts-pattern's matcher walk).
                                if self.is_object_interface(resolved_target)
                                    || crate::type_queries::is_function_interface_structural(
                                        self.db,
                                        resolved_target,
                                    )
                                {
                                    TypeId::ANY
                                } else {
                                    *target_type
                                }
                            } else if effective_source == TypeId::UNKNOWN {
                                *target_type
                            } else if union_list_id(self.db, effective_source).is_some() {
                                // For unions: filter members, fall back to
                                // intersection if nothing matches.
                                let narrowed = self.narrow_to_type(effective_source, *target_type);
                                if narrowed == TypeId::NEVER && effective_source != TypeId::NEVER {
                                    self.db.intersection2(effective_source, *target_type)
                                } else if !crate::visitors::visitor_predicates::is_empty_object_type(
                                    self.db.as_type_database(),
                                    self.resolve_type(*target_type),
                                )
                                    && crate::visitors::visitor_predicates::is_empty_object_type(
                                        self.db.as_type_database(),
                                        self.resolve_type(narrowed),
                                    )
                                {
                                    // Same fix as the non-union branch below: when
                                    // union filtering collapsed the source to its
                                    // empty-object member (e.g. `{} | undefined`
                                    // narrowed to `{}`), upgrade to the
                                    // structurally-richer target so subsequent
                                    // property/index access sees the target's
                                    // shape. Without this, a predicate like
                                    // `obj is Partial<User>` over `Obj = {} |
                                    // undefined` leaves `obj` typed as `{}` and
                                    // trips TS2339 on `obj.name`.
                                    *target_type
                                } else {
                                    narrowed
                                }
                            } else if crate::type_param_info(self.db, effective_source).is_some()
                                && crate::visitors::visitor_predicates::contains_type_parameters(
                                    self.db,
                                    *target_type,
                                )
                            {
                                // When the source is a bare type parameter (T)
                                // AND the predicate type itself references type
                                // parameters (e.g., `Extract<T, Function>` =
                                // `T extends Function ? T : never`), the
                                // predicate type is already a refinement of T.
                                // Creating `T & Extract<T, U>` is redundant and
                                // prevents the solver from recognising the
                                // result as callable after instantiation (fixes
                                // TS2348 false positive in conditionalTypes2).
                                //
                                // When the predicate is a concrete type like
                                // `Pet` (no type params), we MUST keep the
                                // intersection `TPet & Pet` to preserve the type
                                // parameter identity (narrowingConstrainedTypeParameter).
                                *target_type
                            } else {
                                // Non-union source: following tsc's narrowType logic:
                                //   1. target <: source → return target (narrowing down)
                                //   2. source <: target → return source (already specific)
                                //   3. otherwise → intersection
                                //
                                // Check if target is a distributive conditional type
                                // whose result is always <: source. This covers
                                // Extract<T, U> = (T extends U ? T : never) where the
                                // true branch is the check type and false branch is never.
                                // The result is always a subset of the check type T, so
                                // if source IS that check type, return target directly.
                                if self.is_conditional_subtype_of_source(
                                    *target_type,
                                    effective_source,
                                ) {
                                    return *target_type;
                                }
                                // Empty-object source (`{}`, no properties / no index
                                // signatures) is the universal non-nullish supertype.
                                // Narrowing it via a type predicate to a more
                                // structured target should yield the target — not the
                                // source — so subsequent index access / property
                                // narrowing can see the target's shape. Mirrors tsc's
                                // narrowType where `target <: source` returns target.
                                // Without this, `obj: {}` after `is Record<string,
                                // unknown>` keeps the `{}` shape and falsely trips
                                // TS7053 on `obj['k']`. (See
                                // controlFlowFavorAssertedTypeThroughTypePredicate.)
                                if crate::visitors::visitor_predicates::is_empty_object_type(
                                    self.db.as_type_database(),
                                    self.resolve_type(effective_source),
                                ) && !crate::visitors::visitor_predicates::is_empty_object_type(
                                    self.db.as_type_database(),
                                    self.resolve_type(*target_type),
                                ) {
                                    return *target_type;
                                }
                                // Then use narrow_to_type. If it returns the source
                                // unchanged (assignable but possibly losing structural
                                // info) or NEVER (no overlap), fall back to an
                                // intersection to preserve the target's structure.
                                let narrowed = self.narrow_to_type(effective_source, *target_type);
                                if narrowed == effective_source && narrowed != *target_type {
                                    if self.is_subtype_for_narrowing(effective_source, *target_type)
                                    {
                                        return effective_source;
                                    }
                                    // Source was unchanged — intersect to preserve
                                    // target-side structure such as index signatures.
                                    self.db.intersection2(effective_source, *target_type)
                                } else if narrowed == TypeId::NEVER
                                    && effective_source != TypeId::NEVER
                                {
                                    self.db.intersection2(effective_source, *target_type)
                                } else {
                                    narrowed
                                }
                            }
                        } else if *asserts {
                            // CRITICAL: For assertion functions, the false branch is unreachable
                            // (the function throws if the assertion fails), so we don't narrow
                            source_type
                        } else {
                            // False branch for regular type guards: exclude the target type.
                            // Resolve Lazy/Application types first so exclusion can see
                            // through opaque wrappers (e.g. Readonly<Record<K,V>>).
                            let resolved_source = self.resolve_for_exclusion_narrowing(source_type);
                            let resolved_target =
                                self.resolve_for_exclusion_narrowing(*target_type);

                            // tsc's `getNarrowedTypeWorker(assumeTrue=false)` first
                            // computes the true-branch type, then shallow-filters the
                            // source: `filterType(type, t => !isTypeSubsetOf(t,
                            // trueType))`. `filterType`/`isTypeSubsetOf` are a pure
                            // identity/containment pass over the source union's
                            // top-level members — no descent into a member's
                            // intersection sub-structure and no structural subtype
                            // walk. Take that cheap path when it reduces the source:
                            // the general `narrow_excluding_type` recurses into every
                            // member with a deep `is_assignable_to`, which explodes on
                            // recursive-schema unions (typebox / ts-morph `value is T`,
                            // where each nested schema instantiates to a distinct
                            // `TypeId` so the `(source, excluded)` memo never hits).
                            // The positive (true-branch) type is the union members of
                            // the source that overlap the target, so filtering members
                            // that are subsets of it matches tsc. When the shallow
                            // filter cannot reduce the source (no member is a clean
                            // subset — e.g. type-parameter / single-intersection
                            // sources tsc handles via its intersection step), fall back
                            // to the existing structural exclusion.
                            let positive = self.narrow_to_type(resolved_source, resolved_target);
                            if positive != TypeId::NEVER
                                && positive != resolved_source
                                && let Some(excluded) =
                                    self.narrow_excluding_positive_subset(resolved_source, positive)
                            {
                                return excluded;
                            }
                            self.narrow_excluding_type(resolved_source, resolved_target)
                        }
                    }
                    None => {
                        // Truthiness assertion: asserts x
                        // Behaves like TypeGuard::Truthy (narrows to truthy in true branch)
                        if *asserts {
                            self.narrow_by_truthiness(source_type)
                        } else {
                            source_type
                        }
                    }
                }
            }

            TypeGuard::Array => {
                if sense {
                    // Positive: Array.isArray(x) - narrow to array-like types
                    self.narrow_to_array(source_type)
                } else {
                    // Negative: !Array.isArray(x) - exclude array-like types
                    self.narrow_excluding_array(source_type)
                }
            }

            TypeGuard::ArrayElementPredicate { element_type } => {
                trace!(
                    ?element_type,
                    ?sense,
                    "Applying ArrayElementPredicate guard"
                );
                if sense {
                    // True branch: narrow array element type
                    let result = self.narrow_array_element_type(source_type, *element_type);
                    trace!(?result, "ArrayElementPredicate narrowing result");
                    result
                } else {
                    // False branch: we don't narrow (arr.every could be false for various reasons)
                    trace!("ArrayElementPredicate false branch, no narrowing");
                    source_type
                }
            }

            TypeGuard::Constructor(instance_type) => {
                if sense {
                    self.narrow_by_constructor(source_type, *instance_type)
                } else {
                    self.narrow_by_constructor_false(source_type, *instance_type)
                }
            }
        }
    }
}
